#!/usr/bin/env python3
"""Retained-source resolution, provenance and closeup registration contracts."""
import hashlib
import json
import math
from pathlib import Path
import struct
import unittest

ROOT=Path(__file__).resolve().parents[2]
ART=ROOT/'tools/dragon/art/afk'
OUT=ROOT/'shell/assistant/dragon'


class AfkArt(unittest.TestCase):
    def test_actual_source_dimensions_and_preserved_bytes(self):
        packing=json.loads((ART/'packing.json').read_text())
        for name,metadata in packing['sources'].items():
            source=ART/name
            with source.open('rb') as stream:header=stream.read(24)
            self.assertEqual(header[:8],b'\x89PNG\r\n\x1a\n')
            self.assertEqual(list(struct.unpack('>II',header[16:24])),metadata['size'])
            self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),metadata['sha256'])
        for name in ('closeup','bust'):
            with (OUT/('afk-'+name+'.png')).open('rb') as stream:header=stream.read(24)
            size=struct.unpack('>II',header[16:24])
            self.assertEqual(size,(1254,1254))
            self.assertTrue(all(a<=b for a,b in zip(packing[name]['head_detail_budget'],size)))
        self.assertEqual(packing['reused_flight_source']['cell'],240)
        self.assertEqual(packing['atlas_rgba_bytes'],2560*3840*4)

    def test_body_to_bust_has_one_registered_face_and_native_pixel_budget(self):
        r=json.loads((ART/'registration.json').read_text())
        fc=[sum(eye[i] for eye in r['body_eyes'])/2 for i in range(2)]
        bc=[sum(eye[i] for eye in r['bust_eyes'])/2 for i in range(2)]
        angle=math.radians(r['body_rotation_degrees']);c,s=math.cos(angle),math.sin(angle)
        def project(point):
            x,y=point[0]-fc[0],point[1]-fc[1]
            return [bc[0]+r['body_scale']*(c*x-s*y),bc[1]+r['body_scale']*(s*x+c*y)]
        for body,bust in zip(r['body_eyes'],r['bust_eyes']):
            self.assertLess(math.dist(project(body),bust),.001)
        # The mouth agrees too; the transition does not invent a second smile.
        self.assertLess(math.dist(project(r['body_mouth']),r['bust_mouth']),6)
        self.assertLessEqual(r['handoff_physical_canvas_end']*r['body_scale'],1254)


if __name__=='__main__':unittest.main()
