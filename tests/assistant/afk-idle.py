#!/usr/bin/env python3
"""Real local IPC/lifecycle tests with a stub idle client, not desktop-idle evidence."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'bin'))
import tohru_afk_idle as idle
import tohru_portability as portability


class Idle(unittest.TestCase):
    def setUp(self):
        (ROOT/'.tmp').mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=ROOT/'.tmp',prefix='afk-idle-')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.runtime=tempfile.TemporaryDirectory(prefix='afk-')
        self.addCleanup(self.runtime.cleanup)
        self.env=patch.dict(os.environ,{'XDG_RUNTIME_DIR':self.runtime.name,'WAYLAND_DISPLAY':'private-test',
            'XDG_SESSION_ID':'private-test','XDG_CONFIG_HOME':str(self.root/'config'),'TOHRU_MANAGED_CONFIG':''})
        self.env.start();self.addCleanup(self.env.stop)
        self.config=self.root/'config/tohru/config.toml'
        self.config.parent.mkdir(parents=True)

    def test_config_types_bounds_and_managed_values(self):
        defaults,_=portability.config()
        self.assertFalse(defaults['assistant.afk_enabled'])
        self.assertEqual(defaults['assistant.afk_idle_seconds'],300)
        self.assertEqual(defaults['assistant.afk_probability_percent'],50)
        self.assertEqual(defaults['assistant.afk_seed'],'')
        for key,value in (('afk_enabled','1'),('afk_idle_seconds','0'),('afk_idle_seconds','86401'),
                          ('afk_probability_percent','50.5'),('afk_probability_percent','true'),
                          ('afk_probability_percent','101'),('afk_seed','12'),('afk_seed','"-1"'),
                          ('afk_seed','"2147483647"'),('afk_scene','"unknown"'),
                          ('afk_scene','["screen_window"]'),('afk_scenes','"screen_window,typo"')):
            self.config.write_text('[assistant]\n'+key+'='+value+'\n')
            with self.assertRaises(ValueError,msg=key+'='+value):portability.config()
        self.config.write_text('[assistant]\nafk_enabled=true\nafk_seed="2147483646"\nafk_scenes="screen_window,wing_grooming"\n')
        managed=self.root/'managed.toml';managed.write_text('[assistant]\nafk_enabled=false\n')
        with patch.dict(os.environ,{'TOHRU_MANAGED_CONFIG':str(managed)}):
            values,keys=portability.config()
        self.assertFalse(values['assistant.afk_enabled']);self.assertIn('assistant.afk_enabled',keys)

    def test_passive_client_never_loads_user_idle_config_or_owns_lock_suspend(self):
        args=idle.sway_arguments(300,123)
        self.assertEqual(args[:5],['swayidle','-C','/dev/null','timeout','300'])
        self.assertFalse(set(args)&{'-w','before-sleep','lock','unlock','after-resume'})
        self.assertIn('resume',args)
        for command in (args[5],args[7]):
            parsed=shlex.split(command)
            self.assertEqual(parsed[-2:],['--generation','123'])

    def test_owned_idle_captures_generation_once_and_mismatch_is_inactive(self):
        values={'assistant.afk_enabled':True,'assistant.afk_idle_seconds':300}
        self.assertEqual(idle.owned_callbacks(values),[])
        _,registration=idle.endpoint()
        registration.write_text(json.dumps({'schema':1,'generation':123,'seconds':300}))
        args=idle.owned_callbacks(values)
        self.assertEqual(args[:2],['timeout','300'])
        registration.write_text(json.dumps({'schema':1,'generation':124,'seconds':300}))
        self.assertIn('123',args[2]);self.assertNotIn('124',args[2])
        values['assistant.afk_idle_seconds']=301
        self.assertEqual(idle.owned_callbacks(values),[])

    def test_unknown_protocol_does_not_start_an_idle_client(self):
        self.config.write_text('[assistant]\nafk_enabled=true\n')
        with patch.object(idle,'protocols',return_value=(None,'unknown')),patch.object(idle.subprocess,'Popen') as spawn:
            with self.assertRaisesRegex(ValueError,'inactive'):idle.serve()
            spawn.assert_not_called()

    def wait_for(self,condition):
        deadline=time.monotonic()+3
        while time.monotonic()<deadline:
            value=condition()
            if value:return value
            time.sleep(.01)
        self.fail('bounded local IPC setup timed out')

    def test_real_socket_generation_lifecycle_and_callback_delivery(self):
        self.config.write_text('[assistant]\nafk_enabled=true\n')
        client=self.root/'swayidle'
        client.write_text('#!'+sys.executable+'\nimport os,time,json\nfrom pathlib import Path\n'
                          +'Path('+repr(str(self.root/'client.json'))+').write_text(json.dumps({"pid":os.getpid(),"args":__import__("sys").argv[1:]}))\n'
                          +'time.sleep(30)\n')
        client.chmod(0o755)
        events=self.root/'events.jsonl'
        runner=self.root/'server.py'
        runner.write_text('import sys,json\nfrom pathlib import Path\n'
            +'sys.path.insert(0,'+repr(str(ROOT/'bin'))+')\nimport tohru_afk_idle as m\n'
            +'m.protocols=lambda: (["ext_idle_notifier_v1"],"ok")\n'
            +'m.session_monitor=lambda: None\n'
            +'def emit(name,generation):\n'
            +' with Path('+repr(str(events))+').open("a") as f: f.write(json.dumps([name,generation])+"\\n")\n'
            +' return True\nm.emit=emit\nraise SystemExit(m.serve())\n')
        log=(self.root/'server.log').open('w')
        process=subprocess.Popen([sys.executable,str(runner)],stdout=log,stderr=log,
            env={**os.environ,'PATH':str(self.root)+':'+os.environ['PATH']})
        def cleanup():
            if process.poll() is None:process.terminate();process.wait(timeout=4)
            log.close()
        self.addCleanup(cleanup)
        value=self.wait_for(lambda:idle.read_registration() if idle.endpoint()[1].exists() else None)
        child_info=self.wait_for(lambda:json.loads((self.root/'client.json').read_text()) if (self.root/'client.json').exists() else None)
        self.assertIn('/dev/null',child_info['args'])
        generation=value['generation']
        self.wait_for(lambda:events.exists())
        idle.notify('idle',generation+1)
        idle.notify('idle',generation)
        idle.notify('resume',generation)
        observed=self.wait_for(lambda:[json.loads(line) for line in events.read_text().splitlines()] if 'resume' in events.read_text() else None)
        self.assertEqual([name for name,token in observed if name not in ('source','unsafe')],['idle','resume'])
        process.terminate();self.assertEqual(process.wait(timeout=4),0)
        self.assertFalse(idle.endpoint()[0].exists());self.assertFalse(idle.endpoint()[1].exists())
        self.assertEqual(json.loads(events.read_text().splitlines()[-1]),['lost',generation])
        with self.assertRaises(ProcessLookupError):os.kill(child_info['pid'],0)


if __name__=='__main__':unittest.main()
