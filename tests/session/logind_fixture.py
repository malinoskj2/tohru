#!/usr/bin/env python3
"""Private fake logind protocol peer; never connects to a default/system bus."""
import argparse
import json
import os
from pathlib import Path
import socket
import stat
import sys

ROOT = Path(__file__).resolve().parents[2]
PATH = '/org/freedesktop/login1'
MANAGER = 'org.freedesktop.login1.Manager'
CONTROL = 'org.tohru.TestLogind'
XML = '''<node>
<interface name="org.freedesktop.login1.Manager">
 <method name="Inhibit"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="h" direction="out"/></method>
 <property name="BlockInhibited" type="s" access="read"/>
 <signal name="PrepareForSleep"><arg type="b"/></signal>
</interface>
<interface name="org.tohru.TestLogind">
 <method name="Blocked"><arg type="s" direction="in"/></method>
 <method name="Sleep"><arg type="b" direction="in"/></method>
 <method name="Own"><arg type="b" direction="in"/></method>
 <method name="Status"><arg type="s" direction="out"/></method>
 <method name="Quit"/>
</interface></node>'''


def private_address(address):
    if not address.startswith('unix:path=') or ',' in address or ';' in address:
        raise ValueError('an explicit private unix:path bus is required')
    path = Path(address[len('unix:path='):])
    if not path.is_absolute() or path.is_symlink() or path.resolve() != path:
        raise ValueError('private bus must be an absolute nonsymlink socket')
    if not path.is_relative_to(ROOT/'.tmp'):
        raise ValueError('private bus must be inside this tree .tmp')
    info = path.stat(); parent = path.parent.stat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError('private bus must be an owned unix socket')
    if parent.st_uid != os.getuid() or parent.st_mode & 0o777 != 0o700:
        raise ValueError('private bus directory must be owned mode0700')
    return address


def run(address):
    import gi
    gi.require_version('Gio','2.0')
    from gi.repository import Gio, GLib
    connection = Gio.DBusConnection.new_for_address_sync(private_address(address),
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)
    loop = GLib.MainLoop()
    peers = {}; events = []; manual = ''; sequence = 0
    def bus(method, arguments):
        return connection.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus',
            'org.freedesktop.DBus',method,arguments,None,Gio.DBusCallFlags.NONE,3000,None).unpack()
    def own(enabled):
        if enabled:
            if bus('RequestName',GLib.Variant('(su)',('org.freedesktop.login1',4)))[0] != 1:
                raise RuntimeError('private logind name already held')
        else: bus('ReleaseName',GLib.Variant('(s)',('org.freedesktop.login1',)))
    def blocked():
        return 'idle' if 'idle' in manual.split(':') or any(peer[1]=='idle' and peer[2]=='block' for peer in peers.values()) else manual
    def changed():
        connection.emit_signal(None,PATH,'org.freedesktop.DBus.Properties','PropertiesChanged',
            GLib.Variant('(sa{sv}as)',(MANAGER,{'BlockInhibited':GLib.Variant('s',blocked())},[])))
    def released(fd, condition, ident):
        peer = peers.get(ident)
        if peer is None: return False
        try:
            data = peer[0].recv(1)
            if data: raise RuntimeError('fixture inhibitor is not a data channel')
        except BlockingIOError: return True
        peer[0].close(); del peers[ident]
        events.append({'closed':ident});del events[:-128];changed()
        return False
    def method(conn,sender,path,interface,name,parameters,invocation):
        nonlocal manual,sequence
        try:
            args = parameters.unpack()
            if interface==MANAGER and name=='Inhibit':
                what,who,why,mode=args
                if len(peers)>=16:raise ValueError('private fixture inhibitor limit')
                if (what,mode) not in [('sleep','delay'),('idle','block')]:
                    raise ValueError('fixture supports only sleep delay and idle block')
                client,peer=socket.socketpair();peer.setblocking(False)
                sequence+=1;ident=sequence;peers[ident]=(peer,what,mode)
                fds=Gio.UnixFDList.new();index=fds.append(client.fileno());client.close()
                GLib.io_add_watch(peer.fileno(),GLib.IOCondition.IN|GLib.IOCondition.HUP,released,ident)
                events.append({'opened':ident,'what':what,'mode':mode});del events[:-128]
                invocation.return_value_with_unix_fd_list(GLib.Variant('(h)',(index,)),fds)
                changed();return
            if interface!=CONTROL: raise ValueError('unknown fixture method')
            if name=='Blocked':manual=args[0];changed()
            elif name=='Sleep':
                connection.emit_signal(None,PATH,MANAGER,'PrepareForSleep',GLib.Variant('(b)',(args[0],)))
                events.append({'sleep_signal':args[0]});del events[:-128]
            elif name=='Own':own(args[0])
            elif name=='Status':
                invocation.return_value(GLib.Variant('(s)',(json.dumps({'active':len(peers),'events':events}),)));return
            elif name=='Quit':GLib.idle_add(loop.quit)
            else:raise ValueError('unknown fixture control method')
            invocation.return_value(GLib.Variant('()',()))
        except Exception as error:
            invocation.return_dbus_error('org.tohru.TestLogind.Error',str(error))
    def get_property(conn,sender,path,interface,name):
        return GLib.Variant('s',blocked()) if name=='BlockInhibited' else None
    for interface in Gio.DBusNodeInfo.new_for_xml(XML).interfaces:
        connection.register_object(PATH,interface,method,get_property,None)
    if bus('RequestName',GLib.Variant('(su)',(CONTROL,4)))[0]!=1:
        raise RuntimeError('private test control name already held')
    own(True)
    print('PRIVATE_LOGIND_FIXTURE_READY',flush=True)
    try:loop.run()
    finally:
        for peer,_,_ in peers.values():peer.close()
        connection.close_sync(None)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bus',required=True)
    args=parser.parse_args()
    run(args.bus)
