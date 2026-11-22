#!/usr/bin/env python3
"""Asset contracts: real pose silhouettes, registered native eyes, gutters and bounds."""
import importlib.util
import json
from pathlib import Path
import unittest
import cv2
import numpy as np
from PIL import Image
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('pack',ROOT/'tools/dragon/pack.py')
pack=importlib.util.module_from_spec(spec);spec.loader.exec_module(pack)

class DragonArt(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.meta=json.loads((pack.ART/'poses.json').read_text())
        cls.keys=pack.extract(pack.ART/'hover-source.png')
        cls.faces=np.array(Image.open(pack.OUT/'faces.webp'))

    def test_flight_rows_match_registered_keys_and_move_the_wing(self):
        body = Image.open(pack.OUT / 'body.webp').convert('RGBA')
        flight = Image.open(pack.OUT / 'flight.webp').convert('RGBA')
        self.assertEqual(flight.size, (12 * 240, 3 * 240))
        for row, (key, tip) in enumerate(((309, 152), (417, 83), (327, 64))):
            x, y = key % 24 * 240, key // 24 * 240
            original = np.array(body.crop((x, y, x+240, y+240)))
            start = np.array(flight.crop((0, row*240, 240, (row+1)*240)))
            np.testing.assert_array_equal(start[start[:,:,3]>0], original[original[:,:,3]>0])
            tips = []
            for cell in (3, 9):
                sample = np.array(flight.crop((cell*240, row*240, (cell+1)*240, (row+1)*240)))
                wing = sample[50:150, tip, 3]
                tips.append(np.where(wing>128)[0].min())
            self.assertGreater(abs(tips[0]-tips[1]), 20)

    def test_twenty_whole_bodies_with_distinct_lower_silhouettes(self):
        self.assertEqual(len(self.keys),20)
        silhouettes=[]
        for im in self.keys:
            a=np.array(im)[:,:,3]>128
            self.assertFalse(a[:8].any() or a[-8:].any() or a[:,:8].any() or a[:,-8:].any(), 'clipped art')
            silhouettes.append(a[145:])
        for i,a in enumerate(silhouettes):
            for j,b in enumerate(silhouettes[:i]):
                difference=np.logical_xor(a,b).sum()/np.logical_or(a,b).sum()
                self.assertGreater(difference,0.16,f'near-duplicate lower bodies {i}, {j}')
                reflected=np.logical_xor(a,b[:,::-1]).sum()/np.logical_or(a,b[:,::-1]).sum()
                self.assertGreater(reflected,0.16,f'mirrored lower bodies {i}, {j}')

    def test_automatic_head_registration_matches_hand_measured_views(self):
        spec=importlib.util.spec_from_file_location('registration',ROOT/'tools/dragon/registration.py')
        registration=importlib.util.module_from_spec(spec);spec.loader.exec_module(registration)
        for key,m in zip(self.keys,self.meta):
            detected=registration.eyes(np.array(key))
            self.assertLess(np.linalg.norm(detected-np.array(m['eyes']),axis=1).max(),2,m['name'])

    def test_face_registration_lands_on_own_eyes_and_mouth(self):
        for key,m in zip(self.keys,self.meta):
            rgba=np.array(key)
            for x,y in m['eyes']:
                x,y=round(x),round(y)
                eye=rgba[y-2:y+3,x-2:x+3,:3]
                self.assertLess(eye[:,:,1].min(),115,m['name']+' eye registration misses')
            x,y=m['mouth'];x,y=round(x),round(y)
            # Every face feature sits on opaque skin, including near-profile views.
            self.assertGreater(int(rgba[y,x,3]),240,m['name']+' mouth outside face')
            self.assertLess(abs(x-np.mean(np.array(m['eyes'])[:,0])),25)

    def test_native_patch_gutters_and_no_op_rest(self):
        self.assertEqual(self.faces.shape,(4080,800,4))
        for i in range(480):
            cell=self.faces[i//8*68:i//8*68+68,i%8*100:i%8*100+100]
            self.assertFalse(cell[:2,:,3].any() or cell[-2:,:,3].any() or cell[:,:2,3].any() or cell[:,-2:,3].any())
            if i%24==0:
                self.assertFalse(cell[:,:,3].any(),'closed, unblinking rest should show the untouched body face')
            else:
                self.assertTrue(cell[:,:,3].any(),'missing facial patch')

    def test_native_features_change_without_painting_the_entire_face(self):
        for i in range(20):
            cell=self.faces[i*3*68:i*3*68+68,2*100:3*100]
            self.assertGreater((cell[:,:,3]>128).sum(),20)
            self.assertLess((cell[:,:,3]>128).sum(),1200,'patch is an opaque face sticker')

    def test_every_moving_frame_has_bounded_native_mouth_registration(self):
        anchors=json.loads((pack.ART/'travel-registration.json').read_text())
        self.assertEqual(len(anchors),20)
        with Image.open(pack.OUT/'travel.webp') as im:
            self.assertEqual(im.size,(768,6120))
            a=np.array(im)
        for i,row in enumerate(anchors):
            self.assertEqual(len(row),17)
            strip=np.array(Image.open(pack.ART/'transitions'/(self.meta[i]['name']+'.webp')))
            for f,(x,y) in enumerate(row):
                alpha=strip[:,f*300:(f+1)*300,3]
                self.assertLessEqual(max(alpha[0].max(),alpha[-1].max(),alpha[:,0].max(),alpha[:,-1].max()),180,'moving frame clips its cell')
                self.assertTrue(0<=x and x+24<=150 and 0<=y and y+18<=150)
                cx,cy=round((x+12)*2),round((y+9)*2)
                self.assertGreater(int(strip[cy,f*300+cx,3]),180,'mouth on transparent pixels')
                rest=(i*17+f)*8
                self.assertFalse(a[rest//16*36:rest//16*36+36,rest%16*48:rest%16*48+48,3].any())

    def test_registered_transition_endpoints_and_texture_budget(self):
        old=Image.open(pack.ART/'legacy-body.webp')
        neutral=np.array(old.crop((1500,6000,1800,6300)))
        for key,m in zip(self.keys,self.meta):
            strip=Image.open(pack.ART/'transitions'/(m['name']+'.webp'))
            self.assertEqual(strip.size,(5100,300))
            def visible(a):
                a=a.copy(); a[a[:,:,3]==0,:3]=0; return a
            np.testing.assert_array_equal(visible(np.array(strip.crop((0,0,300,300)))),visible(neutral))
            np.testing.assert_array_equal(visible(np.array(strip.crop((4800,0,5100,300)))),visible(np.array(key)))
        with Image.open(pack.OUT/'body.webp') as body:
            self.assertLessEqual(max(body.size),16384)
            total=0
            for path in [*pack.OUT.glob('*.webp'),pack.OUT/'heart.png']:
                with Image.open(path) as image: total+=image.width*image.height*4
            self.assertLess(total,170*1024*1024)

if __name__=='__main__': unittest.main()
