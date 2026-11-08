#!/usr/bin/env python3
"""Synthetic unprivileged NM/BlueZ fixture, ONLY the runner's private system bus.

Not shipped/used by production. No radios, network interfaces or host endpoints.
"""
import json
import os
from pathlib import Path
import sys
from gi.repository import Gio, GLib

runtime=Path(sys.argv[1]).absolute()
ready=Path(sys.argv[2]).absolute()
record=ready.parent/'service-record.json'
address='unix:path='+str(runtime/'system-bus')
assert runtime.parent==Path(__file__).resolve().parents[2]/'.tmp'
assert not runtime.is_symlink() and (runtime/'system-bus').resolve()==runtime/'system-bus'
assert os.environ['DBUS_SYSTEM_BUS_ADDRESS']==address
assert os.environ['PLEAMAR_SOCKETS']==str(runtime/'ipc')
assert not os.environ.get('WAYLAND_SOCKET') and not os.environ.get('CUA_INJECT_SOCKET')
connection=Gio.DBusConnection.new_for_address_sync(address,Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT|Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,None,None)
NM='/org/freedesktop/NetworkManager'
DEVICE=NM+'/Devices/1'
ACTIVE=NM+'/ActiveConnection/1'
SETTINGS=NM+'/Settings'
BLUE='/org/bluez/hci0'
BTDEV=BLUE+'/dev_AA_BB_CC_DD_EE_FF'
WIFI='org.freedesktop.NetworkManager.Device.Wireless'
BDEV='org.bluez.Device1'
state={'wifi':True,'active':'','known':['Fixture Saved'],'powered':True,'discovering':False,'paired':False,'connected':False,'trusted':False,'kind':'pin','calls':[],'agent':None,'pending':None,'attempt':0}
networks=[('Fixture Open',80,False),('Fixture Saved',49,True),('Fixture Secure',100,True)]
aps={name:NM+'/AccessPoint/'+str(index+1) for index,(name,_,_) in enumerate(networks)}
saved={name:SETTINGS+'/'+str(index+1) for index,(name,_,_) in enumerate(networks)}


def save(event,**details):
    state['calls'].append({'event':event,**details})
    assert len(state['calls'])<=256,'finite fixture call limit exceeded'
    temporary=record.with_suffix('.new')
    temporary.write_text(json.dumps({'calls':state['calls'],'wifi':state['wifi'],'active':state['active'],'paired':state['paired'],'connected':state['connected'],'discovering':state['discovering']},indent=2)+'\n')
    temporary.replace(record)


def changed(path,interface,properties):
    connection.emit_signal(None,path,'org.freedesktop.DBus.Properties','PropertiesChanged',GLib.Variant('(sa{sv}as)',(interface,properties,[])))


def network_changed():
    changed(NM,'org.freedesktop.NetworkManager',{'State':GLib.Variant('u',70 if state['active'] else 20),'PrimaryConnection':GLib.Variant('o',ACTIVE if state['active'] else '/')})
    changed(DEVICE,WIFI,{'ActiveAccessPoint':GLib.Variant('o',aps.get(state['active'],'/'))})


def bt_changed():
    changed(BTDEV,BDEV,{'Paired':GLib.Variant('b',state['paired']),'Connected':GLib.Variant('b',state['connected'])})
    changed(BLUE,'org.bluez.Adapter1',{'Discovering':GLib.Variant('b',state['discovering'])})


def objects():
    return {BLUE:{'org.bluez.Adapter1':{'Powered':GLib.Variant('b',state['powered']),'Discovering':GLib.Variant('b',state['discovering'])}},BTDEV:{BDEV:{'Address':GLib.Variant('s','AA:BB:CC:DD:EE:FF'),'Alias':GLib.Variant('s','Fixture Headphones'),'Name':GLib.Variant('s','Fixture Headphones'),'Paired':GLib.Variant('b',state['paired']),'Connected':GLib.Variant('b',state['connected']),'Trusted':GLib.Variant('b',state['trusted']),'Icon':GLib.Variant('s','audio-headphones')},'org.bluez.Battery1':{'Percentage':GLib.Variant('y',80)}}}


def finish_pair(success):
    pending=state['pending'];state['pending']=None
    if pending is None:return
    if success:
        state['paired']=True;bt_changed();pending.return_value(None)
    else:pending.return_dbus_error('org.bluez.Error.AuthenticationCanceled','Synthetic pairing canceled')


def start_pair(invocation):
    assert state['agent'] is not None
    state['pending']=invocation;state['attempt']+=1;attempt=state['attempt']
    destination,path=state['agent'];kind=state['kind']
    method,signature,arguments={
        'pin':('RequestPinCode','(o)',(BTDEV,)),
        'passkey':('RequestPasskey','(o)',(BTDEV,)),
        'confirm':('RequestConfirmation','(ou)',(BTDEV,123456)),
        'authorize':('RequestAuthorization','(o)',(BTDEV,)),
        'display':('DisplayPinCode','(os)',(BTDEV,'000042')),
    }[kind]
    save('Pair',kind=kind)
    def answered(bus,result,_):
        if state['attempt']!=attempt or state['pending'] is not invocation:return
        try:
            answer=bus.call_finish(result)
            values=answer.unpack() if answer is not None else ()
            save('AgentReply',kind=kind,fixture_code_matches=(values==('000042',) if kind=='pin' else values==(42,) if kind=='passkey' else True))
            if kind!='display':finish_pair(True)
        except GLib.Error:
            save('AgentRefused',kind=kind);finish_pair(False)
    connection.call(destination,path,'org.bluez.Agent1',method,GLib.Variant(signature,arguments),None,Gio.DBusCallFlags.NONE,15000,None,answered,None)


def get_property(_,sender,path,interface,name):
    if interface=='org.freedesktop.NetworkManager':
        return {'State':GLib.Variant('u',70 if state['active'] else 20),'WirelessEnabled':GLib.Variant('b',state['wifi']),'PrimaryConnection':GLib.Variant('o',ACTIVE if state['active'] else '/')}[name]
    if interface=='org.freedesktop.NetworkManager.Device':return GLib.Variant('u',2)
    if interface==WIFI:return GLib.Variant('o',aps.get(state['active'],'/'))
    if interface=='org.freedesktop.NetworkManager.Connection.Active':return GLib.Variant('s','802-11-wireless' if name=='Type' else state['active'])
    if interface=='org.freedesktop.NetworkManager.AccessPoint':
        network=next(item for item in networks if aps[item[0]]==path)
        return {'Ssid':GLib.Variant('ay',list(network[0].encode())),'Strength':GLib.Variant('y',network[1]),'Flags':GLib.Variant('u',int(network[2])),'WpaFlags':GLib.Variant('u',int(network[2])),'RsnFlags':GLib.Variant('u',0)}[name]
    return objects()[path][interface][name]


def set_property(_,sender,path,interface,name,value):
    if interface=='org.freedesktop.NetworkManager' and name=='WirelessEnabled':
        state['wifi']=value.unpack()
        if not state['wifi']:state['active']=''
        save('Wifi',enabled=state['wifi']);network_changed();changed(NM,interface,{'WirelessEnabled':value});return True
    if interface=='org.bluez.Adapter1' and name=='Powered':state['powered']=value.unpack();save('Power',enabled=state['powered']);changed(BLUE,interface,{'Powered':value});return True
    if interface==BDEV and name=='Trusted':state['trusted']=value.unpack();return True
    return False


def method(_,sender,path,interface,name,parameters,invocation):
    args=parameters.unpack()
    if interface=='org.freedesktop.DBus.ObjectManager':invocation.return_value(GLib.Variant('(a{oa{sa{sv}}})',(objects(),)));return
    if interface=='org.bluez.AgentManager1':
        if name=='RegisterAgent':state['agent']=(sender,args[0]);save('RegisterAgent')
        invocation.return_value(None);return
    if interface=='org.bluez.Adapter1':
        if name=='RemoveDevice':state['paired']=False;state['connected']=False;save('ForgetDevice');bt_changed()
        else:state['discovering']=name=='StartDiscovery';save(name);bt_changed()
        invocation.return_value(None);return
    if interface==BDEV:
        if name=='Pair':start_pair(invocation);return
        if name=='CancelPairing':save('CancelPairing');finish_pair(False)
        else:state['connected']=name=='Connect';save(name);bt_changed()
        invocation.return_value(None);return
    if interface=='org.tohru.TestConnectivity':
        if name=='PairingKind':
            assert args[0] in ('pin','passkey','confirm','authorize','display')
            state['kind']=args[0];state['paired']=False;state['connected']=False;bt_changed()
        elif name=='CompleteDisplay':finish_pair(True)
        invocation.return_value(None);return
    if interface=='org.freedesktop.NetworkManager' and name=='GetDevices':invocation.return_value(GLib.Variant('(ao)',([DEVICE],)));return
    if interface==WIFI:
        if name=='GetAllAccessPoints':invocation.return_value(GLib.Variant('(ao)',(list(aps.values()),)));return
        save('Scan');network_changed();invocation.return_value(None);return
    if interface=='org.freedesktop.NetworkManager.Settings':invocation.return_value(GLib.Variant('(ao)',([saved[name] for name in state['known']],)));return
    if interface=='org.freedesktop.NetworkManager.Settings.Connection':
        network=next(name for name,value in saved.items() if value==path)
        if name=='Delete':state['known'].remove(network);save('ForgetNetwork',ssid=network);network_changed();invocation.return_value(None);return
        settings={'connection':{'id':GLib.Variant('s',network)},'802-11-wireless':{'ssid':GLib.Variant('ay',list(network.encode()))}}
        invocation.return_value(GLib.Variant('(a{sa{sv}})',(settings,)));return
    if interface=='org.freedesktop.NetworkManager':
        if name=='DeactivateConnection':state['active']='';save('DisconnectNetwork');network_changed();invocation.return_value(None);return
        if name=='ActivateConnection':ssid=next(name for name,value in saved.items() if value==args[0]);password=None
        else:
            settings=args[0];ssid=bytes(settings['802-11-wireless']['ssid']).decode();password=settings.get('802-11-wireless-security',{}).get('psk')
        assert ssid in aps
        if ssid=='Fixture Secure' and password!='fixture-password':invocation.return_dbus_error('org.freedesktop.NetworkManager.Error.InvalidPassword','Synthetic credentials refused');return
        state['active']=ssid
        if ssid not in state['known']:state['known'].append(ssid)
        save('ConnectNetwork',ssid=ssid,password_matches_fixture=password=='fixture-password' if password is not None else None);network_changed()
        invocation.return_value(GLib.Variant('(o)',(ACTIVE,)) if name=='ActivateConnection' else GLib.Variant('(oo)',(saved[ssid],ACTIVE)));return
    invocation.return_dbus_error('org.freedesktop.DBus.Error.UnknownMethod','Fixture method unsupported')


def register(path,xml):
    for interface in Gio.DBusNodeInfo.new_for_xml('<node>'+xml+'</node>').interfaces:
        connection.register_object(path,interface,method,get_property,set_property)


register('/', '<interface name="org.freedesktop.DBus.ObjectManager"><method name="GetManagedObjects"><arg type="a{oa{sa{sv}}}" direction="out"/></method></interface>')
register('/org/bluez','<interface name="org.bluez.AgentManager1"><method name="RegisterAgent"><arg type="o"/><arg type="s"/></method><method name="UnregisterAgent"><arg type="o"/></method><method name="RequestDefaultAgent"><arg type="o"/></method></interface>')
register(BLUE,'<interface name="org.bluez.Adapter1"><property name="Powered" type="b" access="readwrite"/><property name="Discovering" type="b" access="read"/><method name="StartDiscovery"/><method name="StopDiscovery"/><method name="RemoveDevice"><arg type="o"/></method></interface>')
register(BTDEV,'<interface name="org.bluez.Device1"><property name="Address" type="s" access="read"/><property name="Alias" type="s" access="read"/><property name="Name" type="s" access="read"/><property name="Paired" type="b" access="read"/><property name="Connected" type="b" access="read"/><property name="Trusted" type="b" access="readwrite"/><property name="Icon" type="s" access="read"/><method name="Pair"/><method name="Connect"/><method name="Disconnect"/><method name="CancelPairing"/></interface>')
register('/org/tohru/TestConnectivity','<interface name="org.tohru.TestConnectivity"><method name="PairingKind"><arg type="s"/></method><method name="CompleteDisplay"/></interface>')
register(NM,'<interface name="org.freedesktop.NetworkManager"><property name="State" type="u" access="read"/><property name="WirelessEnabled" type="b" access="readwrite"/><property name="PrimaryConnection" type="o" access="read"/><method name="GetDevices"><arg type="ao" direction="out"/></method><method name="ActivateConnection"><arg type="o"/><arg type="o"/><arg type="o"/><arg type="o" direction="out"/></method><method name="AddAndActivateConnection"><arg type="a{sa{sv}}"/><arg type="o"/><arg type="o"/><arg type="o" direction="out"/><arg type="o" direction="out"/></method><method name="DeactivateConnection"><arg type="o"/></method></interface>')
register(DEVICE,'<interface name="org.freedesktop.NetworkManager.Device"><property name="DeviceType" type="u" access="read"/></interface><interface name="'+WIFI+'"><property name="ActiveAccessPoint" type="o" access="read"/><method name="GetAllAccessPoints"><arg type="ao" direction="out"/></method><method name="RequestScan"><arg type="a{sv}"/></method></interface>')
register(ACTIVE,'<interface name="org.freedesktop.NetworkManager.Connection.Active"><property name="Type" type="s" access="read"/><property name="Id" type="s" access="read"/></interface>')
register(SETTINGS,'<interface name="org.freedesktop.NetworkManager.Settings"><method name="ListConnections"><arg type="ao" direction="out"/></method></interface>')
for path in saved.values():register(path,'<interface name="org.freedesktop.NetworkManager.Settings.Connection"><method name="GetSettings"><arg type="a{sa{sv}}" direction="out"/></method><method name="Delete"/></interface>')
for path in aps.values():register(path,'<interface name="org.freedesktop.NetworkManager.AccessPoint"><property name="Ssid" type="ay" access="read"/><property name="Strength" type="y" access="read"/><property name="Flags" type="u" access="read"/><property name="WpaFlags" type="u" access="read"/><property name="RsnFlags" type="u" access="read"/></interface>')
owned=set()
def acquired(_,name):
    owned.add(name)
    if len(owned)==2:save('Ready');ready.write_text('ready\n')
for name in ('org.bluez','org.freedesktop.NetworkManager'):Gio.bus_own_name_on_connection(connection,name,Gio.BusNameOwnerFlags.NONE,acquired,None)
GLib.MainLoop().run()
