#!/usr/bin/env python3
"""Real private D-Bus portal protocol harness. No desktop grant UI or live shortcuts."""
import asyncio
import contextlib
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'bin'))
from dbus_fast import Message, MessageType, Variant, NameFlag, RequestNameReply
from dbus_fast.aio import MessageBus
from tohru_shortcuts import PortalOwner, PortalUnavailable, PORTAL, PATH, GLOBAL, REQUEST, SESSION
from tohru_portability import config


class FakePortal:
    def __init__(self,bus):
        self.bus=bus
        self.version=2
        self.response=0
        self.delay=.01
        self.grants=['launcher','session','assistant']
        self.closed=[]
        self.request_closed=[]
        self.bind_count=0
        self.configure_count=0
        self.preferred={}
        self.previous=[]
        self.pending=set()
        bus.add_message_handler(self.handle)

    def handle(self,message):
        if message.message_type!=MessageType.METHOD_CALL:return
        if message.interface=='org.freedesktop.DBus.Properties' and message.member=='Get':
            return Message.new_method_return(message,'v',[Variant('u',self.version)])
        if message.interface==SESSION and message.member=='Close':
            self.closed.append(message.path)
            return Message.new_method_return(message)
        if message.interface==REQUEST and message.member=='Close':
            self.request_closed.append(message.path)
            return Message.new_method_return(message)
        if message.interface!=GLOBAL:return
        if message.member=='ConfigureShortcuts':
            self.configure_count+=1
            return Message.new_method_return(message)
        options=message.body[-1]
        token=options['handle_token'].value
        sender=message.sender[1:].replace('.','_')
        path='/org/freedesktop/portal/desktop/request/'+sender+'/'+token
        result={}
        code=0
        delay=.001
        if message.member=='CreateSession':
            session='/org/freedesktop/portal/desktop/session/'+sender+'/'+options['session_handle_token'].value
            result={'session_handle':Variant('s',session)}
        elif message.member=='ListShortcuts':
            result={'shortcuts':Variant('a(sa{sv})',self.records(self.previous))}
        elif message.member=='BindShortcuts':
            self.bind_count+=1
            self.preferred={row[0]:row[1].get('preferred_trigger').value for row in message.body[1] if 'preferred_trigger' in row[1]}
            result={'shortcuts':Variant('a(sa{sv})',self.records(self.grants))}
            code=self.response;delay=self.delay
        else:
            return Message.new_error(message,'org.freedesktop.DBus.Error.UnknownMethod','unknown')
        async def answer():
            await asyncio.sleep(delay)
            await self.bus.send(Message.new_signal(path,REQUEST,'Response','ua{sv}',[code,result]))
        task=asyncio.create_task(answer());self.pending.add(task);task.add_done_callback(self.pending.discard)
        return Message.new_method_return(message,'o',[path])

    @staticmethod
    def records(actions):
        return [[action,{'description':Variant('s',action),'trigger_description':Variant('s','Backend '+action)}] for action in actions]

    async def signal(self,member,body,signature='osta{sv}'):
        await self.bus.send(Message.new_signal(PATH,GLOBAL,member,signature,body))
        await asyncio.sleep(.02)


class PortalTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        daemon=shutil.which('dbus-daemon')
        if not daemon:self.skipTest('dbus-daemon unavailable; no private portal test')
        self.daemon=subprocess.Popen([daemon,'--session','--nofork','--print-address=1'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True)
        with selectors.DefaultSelector() as selector:
            selector.register(self.daemon.stdout,selectors.EVENT_READ)
            if not selector.select(2):raise RuntimeError('private bus startup timed out')
            address=self.daemon.stdout.readline().strip()
        self.server=await MessageBus(bus_address=address).connect()
        await self.server.request_name(PORTAL)
        self.fake=FakePortal(self.server)
        self.client=await MessageBus(bus_address=address).connect()
        self.actions=[]
        async def run(action):self.actions.append(action)
        self.owner=PortalOwner(self.client,{'shortcuts.launcher_trigger':'<Super>space'},action_runner=run,request_timeout=.08)
        for rule in ("type='signal',sender='org.freedesktop.portal.Desktop'", "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.portal.Desktop'"):
            await self.owner.call('org.freedesktop.DBus','AddMatch','s',[rule],path='/org/freedesktop/DBus',destination='org.freedesktop.DBus')
        await self.owner.open_session()

    async def asyncTearDown(self):
        if hasattr(self,'owner'):
            await self.owner.close()
            for task in list(self.fake.pending):task.cancel()
            if self.fake.pending:await asyncio.gather(*self.fake.pending,return_exceptions=True)
            self.client.disconnect();self.server.disconnect()
            await asyncio.gather(self.client.wait_for_disconnect(),self.server.wait_for_disconnect(),return_exceptions=True)
        if hasattr(self,'daemon'):
            self.daemon.terminate();self.daemon.wait(timeout=2);self.daemon.stdout.close()

    async def test_no_bind_until_explicit_setup_actual_triggers_and_configure(self):
        self.assertEqual(self.fake.bind_count,0)
        self.assertEqual(self.owner.state,'not-configured')
        await self.owner.setup()
        self.assertEqual(self.owner.state,'granted')
        self.assertEqual(self.fake.preferred['launcher'],'<Super>space')
        self.assertEqual(self.owner.snapshot()['bindings'][0]['shortcut'],'Backend launcher')
        self.assertTrue(self.owner.snapshot()['configure_available'])
        await self.owner.configure();self.assertEqual(self.fake.configure_count,1)

    async def test_denied_cancelled_partial_no_prompt_loop(self):
        self.fake.response=2;await self.owner.setup();self.assertEqual(self.owner.state,'denied')
        self.assertFalse(self.owner.bindings)
        self.fake.response=1;await self.owner.setup();self.assertEqual(self.owner.state,'cancelled')
        self.fake.response=0;self.fake.grants=['launcher'];await self.owner.setup()
        self.assertEqual(self.owner.state,'partial');self.assertEqual(set(self.owner.bindings),{'launcher'})
        await asyncio.sleep(.02);self.assertEqual(self.fake.bind_count,3)

    async def test_delayed_timeout_closes_request_and_cancelled_task_cleans(self):
        self.fake.delay=.5;await self.owner.setup()
        self.assertEqual(self.owner.state,'request-timeout');self.assertTrue(self.fake.request_closed)
        self.assertEqual(self.owner.requests,{})
        task=asyncio.create_task(self.owner.setup());await asyncio.sleep(.02);task.cancel()
        await asyncio.gather(task,return_exceptions=True)
        self.assertFalse(self.owner.pending);self.assertEqual(self.owner.requests,{})

    async def test_unknown_session_action_and_ungranted_action_never_dispatch(self):
        self.fake.grants=['launcher'];await self.owner.setup()
        for session,action in (('/wrong','launcher'),(self.owner.session,'unknown'),(self.owner.session,'assistant')):
            await self.fake.signal('Activated',[session,action,1,{}])
        self.assertEqual(self.actions,[])
        await self.fake.signal('Activated',[self.owner.session,'launcher',2,{}])
        await self.fake.signal('Activated',[self.owner.session,'launcher',3,{}])
        self.assertEqual(self.actions,['launcher'])
        await self.fake.signal('Deactivated',[self.owner.session,'launcher',4,{}])
        await self.fake.signal('Activated',[self.owner.session,'launcher',5,{}])
        self.assertEqual(self.actions,['launcher','launcher'])

    async def test_changes_revocation_disconnect_and_session_cleanup(self):
        await self.owner.setup();session=self.owner.session
        await self.fake.signal('ShortcutsChanged',[session,self.fake.records(['session'])],'oa(sa{sv})')
        self.assertEqual(set(self.owner.bindings),{'session'})
        await self.fake.signal('ShortcutsChanged',[session,[]],'oa(sa{sv})')
        self.assertEqual(self.owner.state,'revoked')
        await self.owner.setup();session=self.owner.session
        await self.server.send(Message.new_signal(session,SESSION,'Closed'))
        await asyncio.sleep(.02);self.assertEqual(self.owner.state,'revoked');self.assertEqual(self.owner.bindings,{})
        await self.owner.setup();session=self.owner.session
        await self.owner.close_session();self.assertIn(session,self.fake.closed)
        self.server.disconnect();await asyncio.sleep(.02)
        self.assertEqual(self.owner.state,'portal-unavailable');self.assertEqual(self.owner.bindings,{})

    async def test_v1_cannot_configure_and_previous_grants_are_not_active(self):
        self.fake.version=1;self.fake.previous=['launcher']
        await self.owner.close_session();await self.owner.open_session()
        self.assertEqual(self.owner.state,'previous-grants; setup required')
        self.assertFalse(self.owner.registered)
        await self.fake.signal('Activated',[self.owner.session,'launcher',1,{}]);self.assertEqual(self.actions,[])
        with self.assertRaises(ValueError):await self.owner.configure()
        await self.owner.setup();self.assertFalse(self.owner.snapshot()['configure_available'])

    async def test_portal_restart_clears_grants_and_never_auto_prompts(self):
        await self.owner.setup();self.assertTrue(self.owner.registered)
        binds=self.fake.bind_count
        await self.server.release_name(PORTAL);await asyncio.sleep(.02)
        self.assertFalse(self.owner.registered);self.assertEqual(self.owner.bindings,{})
        await self.server.request_name(PORTAL);await asyncio.sleep(.02)
        await self.owner.connect_portal()
        self.assertEqual(self.fake.bind_count,binds)
        self.assertFalse(self.owner.registered)
        await self.owner.setup();self.assertEqual(self.fake.bind_count,binds+1)

    async def test_duplicate_bus_owner_does_not_queue(self):
        first=await self.client.request_name('dev.tohru.Shortcuts',NameFlag.DO_NOT_QUEUE)
        second=await self.server.request_name('dev.tohru.Shortcuts',NameFlag.DO_NOT_QUEUE)
        self.assertEqual(first,RequestNameReply.PRIMARY_OWNER)
        self.assertNotEqual(second,RequestNameReply.PRIMARY_OWNER)

    async def test_disabled_and_compositor_modes_do_not_import_or_contact_portal(self):
        import tempfile
        for mode in ('disabled','compositor'):
            with tempfile.TemporaryDirectory() as directory:
                target=Path(directory)/'tohru';target.mkdir();(target/'config.toml').write_text('[shortcuts]\nmode="'+mode+'"\n')
                result=await asyncio.to_thread(subprocess.run,[sys.executable,str(ROOT/'bin/tohru-shortcuts')],env={**os.environ,'XDG_CONFIG_HOME':directory,'TOHRU_MANAGED_CONFIG':'','PYTHONPATH':''},capture_output=True,text=True,timeout=2)
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertIn('disabled',result.stdout)
        self.assertEqual(self.fake.bind_count,0)


if __name__=='__main__':unittest.main()
