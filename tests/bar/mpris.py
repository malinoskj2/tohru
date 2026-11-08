#!/usr/bin/env python3
"""Minimal MPRIS protocol fixture on the integration runner's private bus."""
import os
from pathlib import Path
import sys
from gi.repository import Gio, GLib

if os.environ.get('TOHRU_B_PRIVATE_BUS') != '1':
    raise RuntimeError('MPRIS fixture requires the private integration bus')

connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
info = Gio.DBusNodeInfo.new_for_xml('''<node>
<interface name="org.mpris.MediaPlayer2"><property name="Identity" type="s" access="read"/></interface>
<interface name="org.mpris.MediaPlayer2.Player">
<property name="PlaybackStatus" type="s" access="read"/>
<property name="Metadata" type="a{sv}" access="read"/>
<property name="Position" type="x" access="read"/>
<property name="Rate" type="d" access="read"/>
<property name="CanSeek" type="b" access="read"/>
</interface>
<interface name="org.tohru.TestPlayer"><method name="SetPlaying"><arg name="playing" type="b" direction="in"/></method></interface>
</node>''')
playing = True


def get_property(_, sender, path, interface, prop):
    return {
        'Identity': GLib.Variant('s', 'B private player'),
        'PlaybackStatus': GLib.Variant('s', 'Playing' if playing else 'Paused'),
        'Metadata': GLib.Variant('a{sv}', {'xesam:title': GLib.Variant('s', 'Native track'), 'xesam:artist': GLib.Variant('as', ['Native artist'])}),
        'Position': GLib.Variant('x', 0), 'Rate': GLib.Variant('d', 1), 'CanSeek': GLib.Variant('b', False),
    }[prop]


def method(_, sender, path, interface, name, params, invocation):
    global playing
    playing = params.unpack()[0]
    connection.emit_signal(None, '/org/mpris/MediaPlayer2', 'org.freedesktop.DBus.Properties', 'PropertiesChanged', GLib.Variant('(sa{sv}as)', ('org.mpris.MediaPlayer2.Player', {'PlaybackStatus': GLib.Variant('s', 'Playing' if playing else 'Paused')}, [])))
    invocation.return_value(None)


for interface in info.interfaces:
    connection.register_object('/org/mpris/MediaPlayer2', interface, method, get_property, None)
Gio.bus_own_name_on_connection(connection, 'org.mpris.MediaPlayer2.tohru_test', Gio.BusNameOwnerFlags.NONE, lambda *_: Path(sys.argv[1]).write_text('ready'), None)
GLib.MainLoop().run()
