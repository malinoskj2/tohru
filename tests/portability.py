#!/usr/bin/env python3
"""Lightweight policy and real local process/socket tests; no compositor/model/audio claims."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'bin'))
import tohru_portability as p
import tohru_wallpaper as wall
import tohru_doctor


def script_module(name, path):
    loader=importlib.machinery.SourceFileLoader(name,str(path))
    spec=importlib.util.spec_from_loader(name,loader)
    module=importlib.util.module_from_spec(spec);loader.exec_module(module)
    return module


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

    def test_unit_generation_is_explicit_and_child_lifecycle_preserved(self):
        specs=json.loads((p.DATA/'session/services.json').read_text())
        lock=p.unit_text('lock',specs['lock'],self.root,'graphical-session.target')
        self.assertIn('Type=notify',lock);self.assertIn('NotifyAccess=all',lock);self.assertNotIn('WantedBy=',lock)
        launcher=p.unit_text('launcher',specs['launcher'],'/opt/Tohru % 1/bin','graphical-session.target','/config/managed.toml')
        self.assertIn('KillMode=process',launcher);self.assertIn('%%',launcher)
        args=type('Args',(),{'components':'bar,launcher','target':'graphical-session.target','output':str(self.root/'units'),'prefix':str(ROOT/'bin'),'managed':None})()
        with contextlib.redirect_stdout(io.StringIO()):p.generate_units(args)
        self.assertEqual(len(list((self.root/'units').glob('*.service'))),3)
        with self.assertRaises(FileExistsError):p.generate_units(args)

    def test_relocated_release_layout_finds_helpers_and_shared_spec(self):
        bundle=self.root/'bundle with spaces';bundle.mkdir()
        shutil.copytree(ROOT/'bin',bundle/'bin',ignore=shutil.ignore_patterns('__pycache__'))
        shutil.copytree(ROOT/'share',bundle/'share')
        result=subprocess.run([str(bundle/'bin/tohru'),'session-units','--output',str(bundle/'units'),'--components','bar,shortcuts'],
            env={**os.environ,'TOHRU_BIN_DIR':''},capture_output=True,text=True,timeout=3)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertTrue((bundle/'units/tohru-shortcuts.service').is_file())
        self.assertIn(str(bundle/'bin/tohru-shortcuts'),(bundle/'units/tohru-shortcuts.service').read_text())

    def test_echo_config_does_not_change_default_routes_or_overwrite(self):
        content=(p.DATA/'pipewire/60-tohru-echo-cancel.conf').read_text()
        self.assertIn('libpipewire-module-echo-cancel',content);self.assertIn('priority.session = 0',content)
        args=type('Args',(),{'output':str(self.root/'aec.conf')})()
        with contextlib.redirect_stdout(io.StringIO()):p.echo_cancel(args)
        with self.assertRaises(FileExistsError):p.echo_cancel(args)

    def test_shortcut_ownership_and_conflicts(self):
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'test','PLEAMAR_GENERIC':''}),patch.object(p,'json_probe',return_value=([{'modmask':64,'key':'SPACE','dispatcher':'exec','arg':'tohru launcher toggle'}]*2,'ok')):
            data=p.shortcuts()
        self.assertEqual(data['owner'],'Hyprland-managed');self.assertFalse(data['editable'])
        self.assertEqual(data['bindings'][0]['shortcut'],'Super+SPACE');self.assertTrue(data['bindings'][1]['conflict'])
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'','SWAYSOCK':'socket'}),patch.object(p,'json_probe',return_value=({'config':'bindsym ...'},'ok')):
            data=p.shortcuts();self.assertFalse(data['editable']);self.assertEqual(data['bindings'],[])
        with patch.dict(os.environ,{'HYPRLAND_INSTANCE_SIGNATURE':'','SWAYSOCK':''}):self.assertEqual(p.shortcuts()['state'],'unsupported')

    def test_voice_environment_only_reexecs_explicit_interpreter(self):
        env=self.root/'voice-env.toml';env.write_text('[environment]\nOMP_NUM_THREADS="2"\n')
        self.write_config(f'[assistant]\nvoice_python="/opt/voice/bin/python"\nvoice_environment="{env}"\n')
        executable,environment=p.voice_environment()
        self.assertEqual(executable,'/opt/voice/bin/python');self.assertEqual(environment['OMP_NUM_THREADS'],'2')
        self.assertEqual(environment['HF_HUB_OFFLINE'],'1')

    def test_doctor_missing_tools_is_structured_and_redacts(self):
        self.write_config('[assistant.discord]\ntoken_file="/secret/do-not-report"\n')
        with patch.object(tohru_doctor,'probe',return_value=(None,'missing')),patch.object(tohru_doctor,'json_probe',return_value=(None,'missing')),patch.object(tohru_doctor,'protocols',return_value=(None,'missing')),patch.object(tohru_doctor,'outputs',return_value=([],'missing')),patch.object(tohru_doctor,'shortcuts',return_value={'state':'unsupported','owner':'unknown','bindings':[]}),patch.object(tohru_doctor.shutil,'which',return_value=None),contextlib.redirect_stdout(io.StringIO()) as out:
            result=tohru_doctor.doctor(True)
        self.assertEqual(result,1);data=json.loads(out.getvalue());self.assertTrue(data['read_only'])
        self.assertNotIn('do-not-report',out.getvalue());self.assertTrue(any(c['state']=='missing' for c in data['checks']))

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

    def test_doctor_malformed_and_inaccessible_config_is_exit_two(self):
        self.write_config("[broken")
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(tohru_doctor.doctor(True),2)
        self.assertEqual(json.loads(out.getvalue())["summary"]["status"],"configuration-error")
        with patch.object(tohru_doctor,'config',side_effect=PermissionError('secret-file-must-not-appear')),contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(tohru_doctor.doctor(True),2)
        self.assertNotIn('secret-file-must-not-appear',out.getvalue())

    def test_lock_status_requires_a_fresh_nonce_and_live_native_reply(self):
        import threading
        import hashlib
        action=script_module('action_status_test',ROOT/'bin/tohru-session-action')
        with tempfile.TemporaryDirectory(prefix='tlq-') as runtime,patch.dict(os.environ,{'XDG_RUNTIME_DIR':runtime}):
            identity=os.environ['WAYLAND_DISPLAY']+'\0'+os.environ['XDG_SESSION_ID']
            path=Path(runtime)/('tohru-lock-'+hashlib.sha256(identity.encode()).hexdigest()[:16]+'.status')
            self.assertFalse(action.lock_held()) # stale/missing endpoint cannot pass
            for held,valid_nonce in ((True,True),(False,True),(True,False)):
                path.unlink(missing_ok=True)
                with socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM) as server:
                    server.bind(str(path));server.settimeout(1)
                    def respond():
                        nonce,peer=server.recvfrom(64)
                        server.sendto(json.dumps({'schema':1,'nonce':nonce.decode() if valid_nonce else 'stale','held':held,'cycle':4}).encode(),peer)
                    worker=threading.Thread(target=respond);worker.start()
                    try:self.assertEqual(action.lock_held(),held and valid_nonce)
                    finally:worker.join(timeout=2)
                self.assertFalse(action.lock_held()) # ready-then-exit never passes

    def test_wallpaper_crash_retry_is_bounded(self):
        values,_=p.config();runtime=self.root/'wallpaper-runtime';runtime.mkdir()
        owner=wall.Owner(values,self.root,self.root/'assign.json',runtime)
        owner.assignments={'eDP-1':'unused'}
        with patch.object(wall,'outputs',return_value=([{'name':'eDP-1'}],'ok')),patch.object(wall,'valid_file',return_value='unused'),patch.object(owner,'start',side_effect=ValueError('crashed')) as start,patch.object(wall.time,'monotonic',return_value=10):
            owner.reconcile();owner.reconcile();self.assertEqual(start.call_count,1)
            self.assertEqual(owner.retry_at['eDP-1'],15)
            with patch.object(wall.time,'monotonic',return_value=15):owner.reconcile()
            self.assertEqual(start.call_count,2)

    def test_idle_config_disables_and_validates_timers(self):
        idle=script_module('idle_test',ROOT/'bin/tohru-idle')
        values,_=p.config();args=idle.command(values)
        self.assertEqual(args[:2],['swayidle','-w']);self.assertIn('before-sleep',args)
        values['idle.suspend_seconds']=100
        with self.assertRaises(ValueError):idle.command(values)
        values['idle.suspend_seconds']=600;self.assertIn('600',idle.command(values))

    def test_sleep_requires_confirmed_active_locker(self):
        action=script_module('action_test',ROOT/'bin/tohru-session-action')
        with patch.object(action,'probe',return_value=(None,'timeout')) as call:
            with self.assertRaises(ValueError):action.main(['sleep'])
            self.assertEqual(call.call_count,1)
        with patch.object(action,'probe',side_effect=[('', 'ok'),('inactive','ok')]) as call:
            with self.assertRaises(ValueError):action.main(['sleep'])
            self.assertEqual(call.call_count,2)
        with patch.object(action,'lock_held',return_value=False), patch.object(action,'probe',side_effect=[('', 'ok'),('active','ok')]) as call:
            with self.assertRaises(ValueError):action.main(['sleep'])
            self.assertEqual(call.call_count,2)
        with patch.object(action,'lock_held',return_value=True), patch.object(action,'probe',side_effect=[('', 'ok'),('active','ok'),('', 'ok')]) as call:
            self.assertEqual(action.main(['sleep']),0);self.assertEqual(call.call_args.args[0],['systemctl','suspend'])

    def test_wallpaper_disabled_never_starts_backend(self):
        values,_=p.config();directory,state,runtime=wall.paths(values)
        with patch.object(wall.subprocess,'Popen') as spawn:
            with contextlib.redirect_stderr(io.StringIO()):wall.serve(values,directory,state,runtime)
            spawn.assert_not_called()

    def test_wallpaper_assignment_validation_and_atomic_persistence(self):
        directory=self.root/'Wallpapers';directory.mkdir();file=directory/'one.png';file.write_bytes(b'test')
        self.assertEqual(wall.valid_file(directory,str(file)),str(file))
        other=self.root/'outside.png';other.write_bytes(b'test')
        with self.assertRaises(ValueError):wall.valid_file(directory,str(other))
        state=self.root/'state.json';wall.save(state,{'eDP-1':str(file)});self.assertEqual(wall.assignment_data(state),{'eDP-1':str(file)})
        with self.assertRaises(ValueError):wall.seconds('nan')

    def test_wallpaper_supervisor_control_socket_shutdown_and_disabled_restart(self):
        # End-to-end supervisor with protocol-output and player fixtures, no compositor.
        import hashlib
        with tempfile.TemporaryDirectory(prefix='tws-') as runtime:
            directory=self.root/'pictures';directory.mkdir();image=directory/'one.png';image.write_bytes(b'fixture')
            tools=self.root/'tools';tools.mkdir()
            discovery=tools/'wayland-info';discovery.write_text('#!'+sys.executable+'\nprint("interface: \'wl_output\', version: 4, name: 1\\n\\tname: eDP-1")\n');discovery.chmod(0o755)
            backend=tools/'mpv-fixture'
            backend.write_text('#!'+sys.executable+"\nimport socket,sys,re,json\npath=re.search(r'--input-ipc-server=([^ ]+)',sys.argv[2])[1]\ns=socket.socket(socket.AF_UNIX);s.bind(path);s.listen(2)\nwhile True:\n c,_=s.accept()\n with c:\n  request=json.loads(c.recv(65536));c.sendall(json.dumps({'request_id':1,'error':'success','data':None}).encode()+b'\\n')\n")
            backend.chmod(0o755)
            self.write_config(f'[wallpaper]\nenabled=true\ndirectory="{directory}"\nexecutable="{backend}"\n')
            env={**os.environ,'XDG_RUNTIME_DIR':runtime,'PATH':str(tools)+os.pathsep+os.environ.get('PATH','')}
            with patch.dict(os.environ,env):
                values,_=p.config();_,state,endpoint=wall.paths(values)
            log=(ROOT/'.tmp/wallpaper-supervisor-fixture.log').open('w')
            daemon=subprocess.Popen([sys.executable,str(ROOT/'bin/tohru_wallpaper.py'),'serve'],env=env,stdout=log,stderr=log)
            try:
                deadline=time.monotonic()+2
                while not (endpoint/'control.sock').exists() and daemon.poll() is None and time.monotonic()<deadline:time.sleep(.01)
                self.assertTrue((endpoint/'control.sock').exists())
                wall.request(endpoint,['apply','eDP-1',str(image)])
                self.assertEqual(wall.assignment_data(state),{'eDP-1':str(image)})
                self.assertGreater(len(list(endpoint.glob('*.sock'))),1)
            finally:
                daemon.terminate();daemon.wait(timeout=5);log.close()
            self.assertEqual(list(endpoint.glob('*.sock')),[])
            self.write_config('[wallpaper]\nenabled=false\n')
            result=subprocess.run([sys.executable,str(ROOT/'bin/tohru_wallpaper.py'),'serve'],env=env,capture_output=True,text=True,timeout=2)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertFalse((endpoint/'control.sock').exists())
            self.assertEqual(wall.assignment_data(state),{'eDP-1':str(image)})

    def test_owned_wallpaper_process_socket_cleanup_and_hotplug(self):
        # A real child/socket fixture validates ownership, not compositor rendering.
        backend=self.root/'mpvpaper-fixture'
        backend.write_text('''#!'''+sys.executable+'''\nimport os,socket,sys,time,re,json
path=re.search(r'--input-ipc-server=([^ ]+)',sys.argv[2])[1]
s=socket.socket(socket.AF_UNIX);s.bind(path);s.listen(2)
while True:
 c,_=s.accept()
 with c:
  data=json.loads(c.recv(65536));c.sendall(json.dumps({'request_id':1,'error':'success','data':sys.argv[-1]}).encode()+b'\\n')
''');backend.chmod(0o755)
        values,_=p.config();values['wallpaper.executable']=str(backend)
        runtime_temp=tempfile.TemporaryDirectory(dir=ROOT/'.tmp',prefix='w')
        self.addCleanup(runtime_temp.cleanup)
        runtime=Path(runtime_temp.name);directory=self.root/'images';directory.mkdir();image=directory/'one.png';image.write_bytes(b'fixture')
        owner=wall.Owner(values,directory,self.root/'assignments.json',runtime)
        external=runtime/'external.sock'
        unrelated=subprocess.Popen([str(backend),'-o',f'--input-ipc-server={external}','external-output',str(image)])
        deadline=time.monotonic()+1
        while not external.exists() and time.monotonic()<deadline:time.sleep(.01)
        self.assertTrue(external.exists())
        self.addCleanup(lambda: (unrelated.terminate(),unrelated.wait()))
        self.addCleanup(lambda:[owner.stop(name) for name in list(owner.children)])
        owner.monitors=[{'name':'eDP-1'}]
        owner.command(['apply','eDP-1',str(image)])
        original=owner.children['eDP-1'];original.kill();original.wait()
        with patch.object(wall,'outputs',return_value=([{'name':'eDP-1'}],'ok')):owner.reconcile()
        self.assertNotEqual(original.pid,owner.children['eDP-1'].pid)
        child=owner.children['eDP-1'];endpoint=owner.socket('eDP-1')
        self.assertTrue(endpoint.exists());self.assertIsNone(child.poll())
        self.assertEqual(wall.assignment_data(owner.state),{'eDP-1':str(image)})
        with patch.object(wall,'outputs',return_value=([],'ok')):owner.reconcile()
        self.assertIsNotNone(child.poll());self.assertFalse(endpoint.exists());self.assertIsNone(unrelated.poll());self.assertTrue(external.exists())
        with patch.object(wall,'outputs',return_value=([{'name':'eDP-1'}],'ok')):owner.reconcile()
        self.assertIn('eDP-1',owner.children);self.assertNotEqual(child.pid,owner.children['eDP-1'].pid)


if __name__=='__main__':unittest.main()
