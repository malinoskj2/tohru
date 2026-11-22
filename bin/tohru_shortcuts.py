"""Opt-in application-scoped GlobalShortcuts portal owner.

Requests never run on the compositor-managed path. Preferred triggers are hints;
only backend-confirmed registrations are published or allowed to dispatch.
"""
import asyncio
import contextlib
import json
import os
import secrets
import signal
import sys
from pathlib import Path

from dbus_fast import BusType, Message, MessageType, NameFlag, RequestNameReply, Variant
from dbus_fast.aio import MessageBus
from tohru_portability import BIN, config, probe, session_directory

PORTAL = 'org.freedesktop.portal.Desktop'
PATH = '/org/freedesktop/portal/desktop'
GLOBAL = 'org.freedesktop.portal.GlobalShortcuts'
REQUEST = 'org.freedesktop.portal.Request'
SESSION = 'org.freedesktop.portal.Session'
ACTIONS = {
    'launcher': ('Application launcher', ['launcher','toggle']),
    'session': ('Session menu', ['session','toggle']),
    'assistant': ('Assistant', ['assistant','toggle']),
}


class PortalUnavailable(RuntimeError):
    pass


class PortalOwner:
    def __init__(self, bus, values, *, action_runner=None, request_timeout=90):
        self.bus, self.values = bus, values
        self.action_runner = action_runner or self.run_action
        self.request_timeout = request_timeout
        self.portal_owner = ''
        self.generation = 0
        self.version = 0
        self.session = ''
        self.state = 'connecting'
        self.bindings = {}
        self.pressed = set()
        self.requests = {}
        self.tasks = set()
        self.pending = False
        self.registered = False
        self.bound_once = False
        self.closed = False
        self.bus.add_message_handler(self.handle_message)

    async def call(self, interface, member, signature='', body=None, *, path=PATH, destination=None):
        try:
            reply = await asyncio.wait_for(self.bus.call(Message(destination=destination or self.portal_owner or PORTAL,
                path=path, interface=interface, member=member, signature=signature, body=body or [])), 3)
        except asyncio.CancelledError:
            raise
        except Exception as error:
            raise PortalUnavailable('portal call unavailable') from error
        if reply.message_type == MessageType.ERROR:
            raise PortalUnavailable(reply.error_name or 'portal method failed')
        return reply

    def task(self, coroutine):
        task = asyncio.create_task(coroutine)
        self.tasks.add(task)
        task.add_done_callback(self.tasks.discard)
        return task

    def invalidate(self, state):
        self.generation += 1
        self.session = ''
        self.bindings.clear()
        self.registered = False
        self.pressed.clear()
        self.state = state
        for future in self.requests.values():
            if not future.done():
                future.set_exception(PortalUnavailable('portal session ended'))

    def update_bindings(self, records, *, changed=False):
        granted = {}
        for record in records[:32] if isinstance(records,list) else []:
            if not isinstance(record,(list,tuple)) or len(record)!=2:
                continue
            action, properties = record
            if not isinstance(action,str) or action not in ACTIONS or not isinstance(properties,dict):
                continue
            trigger = properties.get('trigger_description')
            trigger = trigger.value if isinstance(trigger,Variant) else None
            if not isinstance(trigger,str) or not trigger or len(trigger)>512:
                continue
            granted[action] = {'action':action, 'shortcut':trigger, 'description':ACTIONS[action][0],
                               'arguments':'', 'submap':'', 'conflict':False, 'flags':'portal grant'}
        previous = bool(self.bindings)
        self.bindings = granted
        self.pressed.intersection_update(granted)
        if granted and not self.registered:
            self.state = "previous-grants; setup required"
            for binding in granted.values():binding["flags"]="previous grant; not registered in current session"
            return
        self.state = ('granted' if len(granted)==len(ACTIONS) else 'partial') if granted else 'revoked' if changed and previous else 'not-configured'

    def handle_message(self, message):
        if message.message_type != MessageType.SIGNAL:
            return
        if (message.sender=='org.freedesktop.DBus' and message.interface=='org.freedesktop.DBus'
                and message.member=='NameOwnerChanged' and len(message.body)==3 and message.body[0]==PORTAL):
            self.portal_owner = message.body[2]
            self.version = 0
            self.invalidate('connecting' if self.portal_owner else 'portal-unavailable')
            return
        if not self.portal_owner or message.sender != self.portal_owner:
            return
        if message.interface==REQUEST and message.member=='Response':
            future = self.requests.get(message.path)
            if future is not None and not future.done() and len(message.body)==2:
                future.set_result(message.body)
            return
        if message.interface==SESSION and message.member=='Closed' and message.path==self.session:
            self.invalidate('revoked')
            return
        if message.interface!=GLOBAL or message.path!=PATH or not self.session or len(message.body)<2 or message.body[0]!=self.session:
            return
        action = message.body[1]
        if message.member=='ShortcutsChanged':
            self.update_bindings(action,changed=True)
        elif self.registered and isinstance(action,str) and action in self.bindings and action in ACTIONS:
            if message.member=='Activated' and action not in self.pressed and len(self.tasks)<4:
                self.pressed.add(action)
                self.task(self.dispatch(action))
            elif message.member=='Deactivated':
                self.pressed.discard(action)

    async def dispatch(self, action):
        try:
            await self.action_runner(action)
        except (OSError,ValueError,asyncio.TimeoutError):
            self.state = 'action-failed'

    async def run_action(self, action):
        # Fixed allowlist only: no executable/config command strings from D-Bus.
        result, state = await asyncio.to_thread(probe,[str(BIN/'tohru'),*ACTIONS[action][1]],3)
        if result is None:
            raise ValueError('action unavailable')

    async def request(self, member, make_body, signature):
        token = 'tohru'+secrets.token_hex(8)
        sender = self.bus.unique_name[1:].replace('.','_')
        path = '/org/freedesktop/portal/desktop/request/'+sender+'/'+token
        future = asyncio.get_running_loop().create_future()
        self.requests[path] = future
        generation = self.generation
        try:
            reply = await self.call(GLOBAL,member,signature,make_body(token))
            if reply.body != [path]:
                raise PortalUnavailable('unexpected portal request path')
            code,results = await asyncio.wait_for(future,self.request_timeout)
            if generation != self.generation:
                raise PortalUnavailable('obsolete portal request')
            if code != 0:
                self.state = 'cancelled' if code==1 else 'denied'
                return None
            if not isinstance(results,dict):
                raise PortalUnavailable('malformed portal response')
            return results
        except (asyncio.TimeoutError,asyncio.CancelledError,PortalUnavailable):
            with contextlib.suppress(Exception):
                await self.call(REQUEST,'Close',path=path)
            raise
        finally:
            self.requests.pop(path,None)
            if not future.done():future.cancel()

    async def open_session(self):
        generation = self.generation
        reply = await self.call('org.freedesktop.DBus.Properties','Get','ss',[GLOBAL,'version'],destination=PORTAL)
        version = reply.body[0].value if reply.body and isinstance(reply.body[0],Variant) else 0
        if generation!=self.generation or not isinstance(version,int) or version<1:
            raise PortalUnavailable('unsupported portal version')
        self.version = version
        self.portal_owner = reply.sender
        session_token = 'tohru'+secrets.token_hex(8)
        result = await self.request('CreateSession',lambda token:[{'handle_token':Variant('s',token),
            'session_handle_token':Variant('s',session_token)}],'a{sv}')
        if result is None:return False
        session = result.get('session_handle')
        session = session.value if isinstance(session,Variant) else None
        expected = '/org/freedesktop/portal/desktop/session/'+self.bus.unique_name[1:].replace('.','_')+'/'+session_token
        if session!=expected:
            raise PortalUnavailable('unexpected portal session path')
        self.session = session
        self.bound_once = False
        result = await self.request('ListShortcuts',lambda token:[self.session,{'handle_token':Variant('s',token)}],'oa{sv}')
        if result is not None:
            records = result.get('shortcuts')
            self.update_bindings(records.value if isinstance(records,Variant) else [])
        return True

    async def close_session(self):
        session,self.session = self.session,''
        self.registered = False
        self.bindings.clear()
        self.pressed.clear()
        if session:
            with contextlib.suppress(Exception):
                await self.call(SESSION,'Close',path=session)

    async def setup(self):
        if self.pending:return
        self.pending = True
        self.state = 'request-pending'
        try:
            if self.bound_once or not self.session:
                await self.close_session()
                if not await self.open_session():return
            self.state = 'request-pending'
            choices = []
            for action,(description,_) in ACTIONS.items():
                properties = {'description':Variant('s',description)}
                preferred = self.values.get('shortcuts.'+action+'_trigger','')
                if preferred:properties['preferred_trigger']=Variant('s',preferred)
                choices.append([action,properties])
            self.bound_once = True
            result = await self.request('BindShortcuts',lambda token:[self.session,choices,'',{'handle_token':Variant('s',token)}],'oa(sa{sv})sa{sv}')
            if result is not None:
                self.registered = True
                registered = result.get('shortcuts')
                self.update_bindings(registered.value if isinstance(registered,Variant) else [])
                if not self.bindings:self.state='denied'
            else:
                self.registered=False; self.bindings.clear()
        except asyncio.TimeoutError:
            self.state = 'request-timeout'
        except (OSError,ValueError,PortalUnavailable):
            self.state = 'portal-unavailable'
        finally:
            self.pending = False

    async def configure(self):
        if self.version<2 or not self.session or not self.bindings or not self.registered:
            raise ValueError('configuration unavailable; portal v2 and existing grants required')
        await self.call(GLOBAL,'ConfigureShortcuts','osa{sv}',[self.session,'',{}])
        self.state = 'configuration-requested'

    def snapshot(self):
        return {'owner':'Tohru actions via desktop portal','state':self.state,'editable':self.version>=1,
                'portal_version':self.version,'setup_available':self.version>=1 and not self.pending,
                'configure_available':self.version>=2 and self.registered and bool(self.bindings) and not self.pending,
                'active_registration':self.registered and bool(self.bindings),
                'bindings':list(self.bindings.values()),'registration':'portal-session',
                'notice':'Preferred triggers are requests; displayed triggers are backend-confirmed. Compositor bindings remain separately owned.'}

    async def connect_portal(self):
        self.pending = True
        try:
            await self.open_session()
        except asyncio.TimeoutError:
            self.state = 'request-timeout'
        except (OSError,ValueError,PortalUnavailable):
            self.state = 'portal-unavailable'
        finally:
            self.pending = False

    async def close(self):
        self.closed = True
        for task in list(self.tasks):task.cancel()
        if self.tasks:await asyncio.gather(*self.tasks,return_exceptions=True)
        for path,future in list(self.requests.items()):
            if not future.done():future.cancel()
            with contextlib.suppress(Exception):await self.call(REQUEST,'Close',path=path)
        await self.close_session()
        self.bus.remove_message_handler(self.handle_message)


async def serve(values):
    directory=session_directory();directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    import fcntl
    with (directory/'shortcuts.lock').open('w') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        bus=await asyncio.wait_for(MessageBus(bus_type=BusType.SESSION).connect(),3)
        owner=None
        endpoint=directory/'shortcuts.sock'
        server=None
        monitor=None
        stop=asyncio.Event()
        try:
            result=await asyncio.wait_for(bus.request_name('dev.tohru.Shortcuts',NameFlag.DO_NOT_QUEUE),3)
            if result!=RequestNameReply.PRIMARY_OWNER:
                raise ValueError('another Tohru portal owner is running')
            owner=PortalOwner(bus,values)
            for rule in ("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.portal.Desktop'",
                         "type='signal',sender='org.freedesktop.portal.Desktop'"):
                await owner.call('org.freedesktop.DBus','AddMatch','s',[rule],path='/org/freedesktop/DBus',destination='org.freedesktop.DBus')
            semaphore=asyncio.Semaphore(8)
            async def control(reader,writer):
                if semaphore.locked():
                    writer.close();return
                async with semaphore:
                    try:
                        operation=(await asyncio.wait_for(reader.readline(),2)).decode().strip()
                        if operation=='setup':
                            if not owner.pending and owner.version>=1:
                                owner.task(owner.setup())
                        elif operation=='configure':await owner.configure()
                        elif operation!='status':raise ValueError('unknown shortcut operation')
                        result=owner.snapshot()
                    except (ValueError,OSError,PortalUnavailable,asyncio.TimeoutError):
                        result={'error':'shortcut operation unavailable; inspect portal state'}
                    writer.write(json.dumps(result).encode()+b'\n')
                    with contextlib.suppress(OSError):await writer.drain()
                    writer.close()
                    with contextlib.suppress(OSError):await writer.wait_closed()
            endpoint.unlink(missing_ok=True)
            server=await asyncio.start_unix_server(control,path=endpoint,limit=8192)
            os.chmod(endpoint,0o600)
            async def maintain():
                while not stop.is_set():
                    if not owner.pending and owner.state in ('connecting','portal-unavailable'):
                        await owner.connect_portal()
                    await asyncio.sleep(3)
            monitor=asyncio.create_task(maintain())
            loop=asyncio.get_running_loop()
            for signum in (signal.SIGTERM,signal.SIGINT):loop.add_signal_handler(signum,stop.set)
            disconnected=asyncio.create_task(bus.wait_for_disconnect())
            stopped=asyncio.create_task(stop.wait())
            done,_=await asyncio.wait([disconnected,stopped],return_when=asyncio.FIRST_COMPLETED)
            for task in (disconnected,stopped):
                if task not in done:task.cancel()
            await asyncio.gather(disconnected,stopped,return_exceptions=True)
            if disconnected in done:
                owner.invalidate('disconnected')
                raise PortalUnavailable('session bus disconnected')
        finally:
            if monitor:
                monitor.cancel()
                await asyncio.gather(monitor,return_exceptions=True)
            if server:
                server.close();await server.wait_closed()
                endpoint.unlink(missing_ok=True)
            if owner:await owner.close()
            bus.disconnect()
