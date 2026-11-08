#!/usr/bin/env python3
"""CPU-only private fixture transport/FD checks; never starts native idle/power."""
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time
import unittest
sys.dont_write_bytecode=True
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('logind_fixture',HERE/'logind_fixture.py')
fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)

class PrivateLogind(unittest.TestCase):
    def test_rejects_nonprivate_and_implicit_bus_addresses_before_connection(self):
        for address in ['system','session','unix:path=/run/dbus/system_bus_socket','tcp:host=localhost','unix:path=/tmp/a;unix:path=/tmp/b']:
            with self.assertRaises(ValueError):fixture.private_address(address)

    def test_actual_private_bus_fd_close_owner_loss_and_properties(self):
        import gi
        gi.require_version('Gio','2.0')
        from gi.repository import Gio,GLib
        processes=[];connection=None
        with tempfile.TemporaryDirectory(dir=fixture.ROOT/'.tmp') as path:
            run=Path(path);address='unix:path='+str(run/'bus')
            config=run/'bus.conf';config.write_text('<busconfig><type>session</type><listen>'+address+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            def wait(test):
                deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    result=test()
                    if result:return result
                    time.sleep(.02)
                self.fail('private fixture timeout')
            try:
                bus=subprocess.Popen(['dbus-daemon','--nofork','--config-file',str(config)],stdout=subprocess.PIPE,stderr=subprocess.PIPE);processes.append(bus)
                wait(lambda:(run/'bus').exists() and bus.poll() is None)
                peer=subprocess.Popen([sys.executable,str(HERE/'logind_fixture.py'),'--bus',address],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);processes.append(peer)
                self.assertTrue(select.select([peer.stdout],[],[],5)[0])
                ready=peer.stdout.readline()
                self.assertEqual(ready.strip(),'PRIVATE_LOGIND_FIXTURE_READY',ready)
                connection=Gio.DBusConnection.new_for_address_sync(address,Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT|Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,None,None)
                def call(name,args=None):
                    return connection.call_sync(fixture.CONTROL,fixture.PATH,fixture.CONTROL,name,args,None,Gio.DBusCallFlags.NONE,3000,None).unpack()
                def status():return json.loads(call('Status')[0])
                def blocked():
                    return connection.call_sync(fixture.CONTROL,fixture.PATH,'org.freedesktop.DBus.Properties','Get',GLib.Variant('(ss)',(fixture.MANAGER,'BlockInhibited')),None,Gio.DBusCallFlags.NONE,3000,None).unpack()[0]
                reply,fds=connection.call_with_unix_fd_list_sync(fixture.CONTROL,fixture.PATH,fixture.MANAGER,'Inhibit',GLib.Variant('(ssss)',('idle','test','synthetic','block')),None,Gio.DBusCallFlags.NONE,3000,None,None)
                fd=fds.get(reply.unpack()[0]);fds=None
                self.assertEqual(status()['active'],1);self.assertEqual(blocked(),'idle')
                os.close(fd);wait(lambda:status()['active']==0);self.assertEqual(blocked(),'')
                call('Blocked',GLib.Variant('(s)',('idle',)));self.assertEqual(blocked(),'idle')
                call('Own',GLib.Variant('(b)',(False,)))
                with self.assertRaises(GLib.Error):
                    connection.call_sync('org.freedesktop.login1',fixture.PATH,'org.freedesktop.DBus.Properties','Get',GLib.Variant('(ss)',(fixture.MANAGER,'BlockInhibited')),None,Gio.DBusCallFlags.NO_AUTO_START,3000,None)
                call('Own',GLib.Variant('(b)',(True,)))
                call('Sleep',GLib.Variant('(b)',(True,)));call('Sleep',GLib.Variant('(b)',(False,)))
                self.assertEqual(status()['events'][-1],{'sleep_signal':False})
                call('Quit');peer.wait(timeout=5);self.assertEqual(peer.returncode,0,peer.stderr.read())
            finally:
                original_failure=sys.exc_info()[0] is not None
                failures=[]
                try:
                    if connection:connection.close_sync(None)
                except Exception as error:failures.append('client close: '+str(error))
                spec=importlib.util.spec_from_file_location('native_cleanup',HERE/'native.py')
                native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
                logs=[stream for process in processes for stream in [process.stdout,process.stderr] if stream]
                failures.extend(native.cleanup(processes,logs,run,run,[]))
                if failures:
                    message='; '.join(failures)
                    if original_failure:print('cleanup diagnostics: '+message,file=sys.stderr)
                    else:raise RuntimeError(message)


if __name__=='__main__':unittest.main()
