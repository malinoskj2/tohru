#!/usr/bin/env python3
"""Lightweight policy and real local process/socket tests; no compositor/model/audio claims."""
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'bin'))
import tohru_portability as p
import tohru_doctor


class Portability(unittest.TestCase):
    def setUp(self):
        (ROOT/".tmp").mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=ROOT/'.tmp',prefix='portability-test-')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.env=patch.dict(os.environ,{'HOME':str(self.root),'XDG_CONFIG_HOME':str(self.root/'config'),
            'XDG_STATE_HOME':str(self.root/'state'),'XDG_RUNTIME_DIR':str(self.root),
            'WAYLAND_DISPLAY':'wayland-test','XDG_SESSION_ID':'test','TOHRU_MANAGED_CONFIG':''})
        self.env.start();self.addCleanup(self.env.stop)

    def write_config(self,text):
        target=self.root/'config/tohru/config.toml';target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text)

    def test_xdg_defaults_and_managed_lifecycle_values(self):
        values,managed=p.config();self.assertFalse(values['wallpaper.enabled']);self.assertEqual(values['idle.owner'],'external')
        self.write_config('[wallpaper]\nenabled=true\n')
        file=self.root/'managed.toml';file.write_text('[wallpaper]\nenabled=false\n[appearance]\nfont_text="Custom Sans"\n')
        with patch.dict(os.environ,{'TOHRU_MANAGED_CONFIG':str(file)}):
            values,managed=p.config()
        self.assertFalse(values['wallpaper.enabled']);self.assertIn('appearance.font_text',managed)

    def test_invalid_native_helper_types_rejected(self):
        for data in ('[wallpaper]\nenabled="true"','[idle]\nlock_seconds=-1','[idle]\nowner="random"','[wallpaper]\ndirectory="relative"'):
            self.write_config(data)
            with self.assertRaises(ValueError):p.config()

    def test_session_paths_are_stable_and_isolated(self):
        a=p.session_directory()
        with patch.dict(os.environ,{'WAYLAND_DISPLAY':'wayland-other'}):self.assertNotEqual(a,p.session_directory())
        self.assertEqual(a,p.session_directory())

    def test_probe_timeout_output_bound_and_missing(self):
        self.assertEqual(p.probe([sys.executable,'-c','import time;time.sleep(10)'],timeout=.05)[1],'timeout')
        self.assertEqual(p.probe([sys.executable,'-c','print("x"*300000)'])[1],'oversized response')
        self.assertEqual(p.probe([str(self.root/'absent')])[1],'missing')

    def test_output_discovery_without_workspaces(self):
        report="interface: 'wl_output', version: 4, name: 8\n\tname: HDMI-A-1\n\tscale: 2\ninterface: 'wl_output', version: 4, name: 9\n\tname: eDP-1\n"
        with patch.object(p,'probe',return_value=(report,'ok')):
            self.assertEqual(p.outputs()[0],[{'name':'HDMI-A-1','scale':2},{'name':'eDP-1','scale':1}])
        with patch.object(p,'probe',return_value=(None,'missing')):self.assertEqual(p.outputs(),([], 'missing'))

    def test_output_names_from_xdg_output_and_rotated_logical_size(self):
        report = """interface: 'wl_output', version: 3, name: 8
 x: 0, y: 0, scale: 2,
 output_transform: 90°,
 mode:
  width: 3840 px, height: 2160 px, refresh: 60.000 Hz,
  flags: current preferred
interface: 'zxdg_output_manager_v1', version: 3, name: 9
 xdg_output_v1
  output: 8
  name: 'HDMI-A-1'
  logical_width: 1440, logical_height: 2560
"""
        self.assertEqual(p.parse_outputs(report),[{'name':'HDMI-A-1','scale':2,'transform':'90°','width':1440,'height':2560}])

    def test_shortcut_ownership_and_conflicts(self):
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'test','PLEAMAR_GENERIC':''}),patch.object(p,'json_probe',return_value=([{'modmask':64,'key':'SPACE','dispatcher':'exec','arg':'tohru launcher toggle'}]*2,'ok')):
            data=p.shortcuts()
        self.assertEqual(data['owner'],'Hyprland-managed');self.assertFalse(data['editable'])
        self.assertEqual(data['bindings'][0]['shortcut'],'Super+SPACE');self.assertTrue(data['bindings'][1]['conflict'])
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'','SWAYSOCK':'socket'}),patch.object(p,'json_probe',return_value=({'config':'bindsym ...'},'ok')):
            data=p.shortcuts();self.assertFalse(data['editable']);self.assertEqual(data['bindings'],[])
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'','SWAYSOCK':''}):self.assertEqual(p.shortcuts()['state'],'unsupported')

    def test_doctor_missing_tools_is_structured_and_redacts(self):
        self.write_config('[assistant.discord]\ntoken_file="/secret/do-not-report"\n')
        with patch.object(tohru_doctor,'probe',return_value=(None,'missing')),patch.object(tohru_doctor,'json_probe',return_value=(None,'missing')),patch.object(tohru_doctor,'protocols',return_value=(None,'missing')),patch.object(tohru_doctor,'outputs',return_value=([],'missing')),patch.object(tohru_doctor,'shortcuts',return_value={'state':'unsupported','owner':'unknown','bindings':[]}),patch.object(tohru_doctor.shutil,'which',return_value=None),contextlib.redirect_stdout(io.StringIO()) as out:
            result=tohru_doctor.doctor(True)
        self.assertEqual(result,1);data=json.loads(out.getvalue());self.assertTrue(data['read_only'])
        self.assertNotIn('do-not-report',out.getvalue());self.assertTrue(any(c['state']=='missing' for c in data['checks']))

    def test_doctor_does_not_treat_failed_owned_wallpaper_as_healthy(self):
        self.write_config('[wallpaper]\nenabled=true\n[assistant]\nvoice=false\n')
        def responses(args):
            return ({'owner':'tohru','errors':{'eDP-1':'backend failed'}},'ok') if str(args[0]).endswith('tohru-wallpaper') else (None,'missing')
        with patch.object(tohru_doctor,'json_probe',side_effect=responses),contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertNotEqual(tohru_doctor.doctor(True),0)
        check=next(c for c in json.loads(out.getvalue())['checks'] if c['name']=='wallpaper.owner')
        self.assertEqual(check['state'],'backend-errors');self.assertTrue(check['required']);self.assertEqual(check['severity'],'error')

    def test_doctor_exit_contract_required_optional_unknown_and_invalid(self):
        values={"assistant.voice":False,"wallpaper.enabled":False,"idle.owner":"external"}
        names=tohru_doctor.required_checks(values)
        def report(overrides=None):
            overrides=overrides or {}
            checks=[{"name":name,"state":overrides.get(name,"ok")} for name in names]
            checks.append({"name":"executable.ffmpeg","state":"missing"})
            return tohru_doctor.summarize(checks,values)
        self.assertEqual(report()["exit_code"],0) # optional missing tool is a warning
        self.assertEqual(report({"executable.flock":"missing"})["exit_code"],1)
        self.assertEqual(report({"config":"invalid"})["exit_code"],2)
        self.assertEqual(report({"protocol.layer-shell":"unknown"})["exit_code"],3)
        self.assertEqual(report({"systemd-user":"missing"})["exit_code"],1)
        self.assertNotIn('executable.parec',names)
        self.assertIn('voice.vad_model',tohru_doctor.required_checks({"assistant.voice":True}))
        self.assertIn('service.lock',tohru_doctor.required_checks({'idle.owner':'tohru'}))
        self.assertIn('wallpaper.owner',tohru_doctor.required_checks({'wallpaper.enabled':True}))
        self.assertNotIn('executable.notify-send',tohru_doctor.required_checks({}))
        self.assertIn('executable.notify-send',tohru_doctor.required_checks({
            'assistant.discord.enabled':True, 'assistant.discord.post':True,
            'assistant.discord.desktop_first':True}))

    def test_doctor_malformed_and_inaccessible_config_is_exit_two(self):
        self.write_config("[broken")
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(tohru_doctor.doctor(True),2)
        self.assertEqual(json.loads(out.getvalue())["summary"]["status"],"configuration-error")
        with patch.object(tohru_doctor,'config',side_effect=PermissionError('secret-file-must-not-appear')),contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(tohru_doctor.doctor(True),2)
        self.assertNotIn('secret-file-must-not-appear',out.getvalue())


if __name__=='__main__':unittest.main()
