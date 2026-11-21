#!/usr/bin/env python3
"""An MPRIS player for trying the media popup: a song that plays on a clock, with seeking and volume.

    python3 tests/media/mpris-player.py --name spotify --identity Spotify --title "Midnight City" \
        --artist M83 --art cover.png --length 243 --position 134 --playing --volume 0.72

It owns org.mpris.MediaPlayer2.<name> on the session bus. Without --volume it has no Volume property,
as browsers do. Next and Previous start the song again.
"""
import argparse
import time
from pathlib import Path
from gi.repository import Gio, GLib

PATH = '/org/mpris/MediaPlayer2'
PLAYER = 'org.mpris.MediaPlayer2.Player'
NODE = '''<node>
<interface name="org.mpris.MediaPlayer2">
<property name="Identity" type="s" access="read"/>
<method name="Raise"/><method name="Quit"/>
</interface>
<interface name="org.mpris.MediaPlayer2.Player">
<method name="PlayPause"/><method name="Play"/><method name="Pause"/><method name="Stop"/>
<method name="Next"/><method name="Previous"/>
<method name="Seek"><arg name="offset" type="x" direction="in"/></method>
<method name="SetPosition"><arg name="track" type="o" direction="in"/><arg name="position" type="x" direction="in"/></method>
<signal name="Seeked"><arg name="position" type="x"/></signal>
<property name="PlaybackStatus" type="s" access="read"/>
<property name="Metadata" type="a{sv}" access="read"/>
<property name="Position" type="x" access="read"/>
<property name="Rate" type="d" access="read"/>
<property name="CanSeek" type="b" access="read"/>
<property name="CanControl" type="b" access="read"/>
%s
</interface>
</node>'''

parser = argparse.ArgumentParser()
parser.add_argument('--name', required=True)
parser.add_argument('--identity', required=True)
parser.add_argument('--title', default='')
parser.add_argument('--artist', default='')
parser.add_argument('--art', default='')
parser.add_argument('--length', type=float, default=0)
parser.add_argument('--position', type=float, default=0)
parser.add_argument('--playing', action='store_true')
parser.add_argument('--volume', type=float)
parser.add_argument('--no-seek', action='store_true')
args = parser.parse_args()

state = {'playing': args.playing, 'at': args.position, 'since': time.monotonic(), 'volume': args.volume}
track = '/org/tohru/track/' + args.name.replace('.', '_')


def position():
    p = state['at'] + (time.monotonic() - state['since'] if state['playing'] else 0)
    return min(p, args.length) if args.length > 0 else p


def place(seconds):
    state['at'] = max(0.0, min(seconds, args.length) if args.length > 0 else seconds)
    state['since'] = time.monotonic()


def metadata():
    m = {'mpris:trackid': GLib.Variant('o', track), 'xesam:title': GLib.Variant('s', args.title), 'xesam:artist': GLib.Variant('as', [args.artist] if args.artist else [])}
    if args.length > 0:
        m['mpris:length'] = GLib.Variant('x', int(args.length * 1e6))
    if args.art:
        m['mpris:artUrl'] = GLib.Variant('s', Path(args.art).resolve().as_uri())
    return m


def properties():
    p = {
        'Identity': GLib.Variant('s', args.identity),
        'PlaybackStatus': GLib.Variant('s', 'Playing' if state['playing'] else 'Paused'),
        'Metadata': GLib.Variant('a{sv}', metadata()),
        'Position': GLib.Variant('x', int(position() * 1e6)),
        'Rate': GLib.Variant('d', 1.0),
        'CanSeek': GLib.Variant('b', not args.no_seek),
        'CanControl': GLib.Variant('b', True),
    }
    if state['volume'] is not None:
        p['Volume'] = GLib.Variant('d', state['volume'])
    return p


connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
volume_xml = '<property name="Volume" type="d" access="readwrite"/>' if args.volume is not None else ''
info = Gio.DBusNodeInfo.new_for_xml(NODE % volume_xml)


def changed(*names):
    p = properties()
    connection.emit_signal(None, PATH, 'org.freedesktop.DBus.Properties', 'PropertiesChanged', GLib.Variant('(sa{sv}as)', (PLAYER, {n: p[n] for n in names}, [])))


def seeked():
    connection.emit_signal(None, PATH, PLAYER, 'Seeked', GLib.Variant('(x)', (int(position() * 1e6),)))


def get_property(_c, _s, _p, _i, name):
    return properties()[name]


def set_property(_c, _s, _p, _i, name, value):
    if name != 'Volume' or state['volume'] is None:
        return False
    state['volume'] = max(0.0, value.unpack())
    changed('Volume')
    return True


def method(_c, _s, _p, _i, name, params, invocation):
    if name in ('PlayPause', 'Play', 'Pause', 'Stop'):
        place(position())
        state['playing'] = {'PlayPause': not state['playing'], 'Play': True}.get(name, False)
        changed('PlaybackStatus', 'Position')
    elif name in ('Next', 'Previous'):
        place(0)
        changed('Metadata', 'Position')
        seeked()
    elif name == 'Seek' and not args.no_seek:
        place(position() + params.unpack()[0] / 1e6)
        seeked()
    elif name == 'SetPosition' and not args.no_seek:
        tid, us = params.unpack()
        if tid == track:
            place(us / 1e6)
            seeked()
    invocation.return_value(None)


def wrap():
    # A song that ends starts again, so the player keeps playing for as long as it is needed.
    if state['playing'] and args.length > 0 and position() >= args.length:
        place(0)
        seeked()
    return True


GLib.timeout_add(500, wrap)
for interface in info.interfaces:
    connection.register_object(PATH, interface, method, get_property, set_property)
Gio.bus_own_name_on_connection(connection, 'org.mpris.MediaPlayer2.' + args.name, Gio.BusNameOwnerFlags.NONE, None, None)
GLib.MainLoop().run()
