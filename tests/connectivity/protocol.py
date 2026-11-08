"""Validate the actual synthetic fixture's native GLib schemas WITHOUT any bus.

Only pure constant definitions/function code and register XML are evaluated;
connection/bootstrap/owner/main-loop statements are never executed.
"""
import ast
from pathlib import Path
from gi.repository import Gio, GLib

source=Path(__file__).with_name('services.py')
module=ast.parse(source.read_text())
context={'GLib':GLib,'Gio':Gio}
constants={'NM','DEVICE','ACTIVE','SETTINGS','BLUE','BTDEV','WIFI','BDEV','state','networks','aps','saved'}
functions={'objects','get_property','method','save'}
for node in module.body:
    pure_constant=isinstance(node,ast.Assign) and len(node.targets)==1 and isinstance(node.targets[0],ast.Name) and node.targets[0].id in constants
    pure_function=isinstance(node,ast.FunctionDef) and node.name in functions
    if pure_constant or pure_function:
        exec(compile(ast.Module(body=[node],type_ignores=[]),str(source),'exec'),context)
interfaces=[]
for node in ast.walk(module):
    if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id=='register':
        xml=eval(compile(ast.Expression(node.args[1]),str(source),'eval'),context)
        interfaces.extend(Gio.DBusNodeInfo.new_for_xml('<node>'+xml+'</node>').interfaces)
assert any(interface.name=='org.bluez.AgentManager1' for interface in interfaces)
assert any(interface.name=='org.freedesktop.NetworkManager' for interface in interfaces)
variant=GLib.Variant('(a{oa{sa{sv}}})',(context['objects'](),))
assert variant.unpack()[0][context['BTDEV']][context['BDEV']]['Address']=='AA:BB:CC:DD:EE:FF'
assert context['get_property'](None,None,context['NM'],'org.freedesktop.NetworkManager','State').unpack()==20
assert context['get_property'](None,None,context['aps']['Fixture Saved'],'org.freedesktop.NetworkManager.AccessPoint','Strength').unpack()==49
class Invocation:
    value=None
    def return_value(self,value):self.value=value
invocation=Invocation()
context['method'](None,None,context['saved']['Fixture Saved'],'org.freedesktop.NetworkManager.Settings.Connection','GetSettings',GLib.Variant('()',()),invocation)
assert bytes(invocation.value.unpack()[0]['802-11-wireless']['ssid'])==b'Fixture Saved'
print(f'Actual fixture GLib XML ({len(interfaces)} interfaces), BlueZ object tree, NM properties/settings schemas pass without endpoints')

# Execute the actual fixture writer while an owned reader observes the published
# record; no partial/truncated JSON may replace a previously valid snapshot.
import json
import tempfile
import threading
root=Path(__file__).resolve().parents[2]
(root/'.tmp').mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='b-service-record-',dir=root/'.tmp') as directory:
    context['record']=Path(directory)/'record.json'
    context['json']=json
    context['save']('Ready')
    stop=threading.Event();failures=[]
    def reader():
        while not stop.is_set():
            try:json.loads(context['record'].read_text())
            except Exception as error:failures.append(error);return
    thread=threading.Thread(target=reader);thread.start()
    try:
        for index in range(50):context['save']('Synthetic',index=index)
    finally:stop.set();thread.join(timeout=5)
    assert not thread.is_alive() and not failures,failures
    assert json.loads(context['record'].read_text())['calls'][-1]['index']==49
print('Actual fixture record writer publishes complete atomic JSON to concurrent reader')
