#!/usr/bin/env python3
"""Source-only fit patch application/guards plus bounded independent math oracles.
No Rust/native compiler, decoder, display, or GPU. Shader execution is a later gate.
"""
import argparse
import math
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


def source_uv(point, destination, source, scale, tile):
    """Retained j2bar shader equations, in physical pixels, as an oracle."""
    physical = tuple(point[i] * scale for i in range(2))
    extent = tuple(destination[i] * scale for i in range(2))
    uv = tuple((physical[i] + (0 if tile else (source[i] - extent[i]) / 2)) / source[i]
               for i in range(2))
    if tile:
        return tuple(v - math.floor(v) for v in uv)
    return uv if all(0 <= v <= 1 for v in uv) else None


def native_uv(point, destination, source, scale, tile):
    """Proposed native logical-coordinate mapping; not actual WGSL execution."""
    logical_source = tuple(size / scale for size in source)
    uv = tuple((point[i] - (0 if tile else destination[i] / 2)) / logical_source[i]
               + (0 if tile else 0.5) for i in range(2))
    if tile:
        return tuple(v - math.floor(v) for v in uv)
    return uv if all(0 <= v <= 1 for v in uv) else None


def atlas_sample(uv, bounds):
    sample = []
    for i in range(2):
        low, high = bounds[i], bounds[i + 2]
        half = min(0.5, (high - low) / 2)
        sample.append(min(high - half, max(low + half, low + (high - low) * uv[i])))
    return tuple(sample)


class FitMath(unittest.TestCase):
    def test_centred_padding_and_crop(self):
        self.assertIsNone(native_uv((0, 0), (100, 100), (20, 10), 1, False))
        self.assertEqual(native_uv((40, 45), (100, 100), (20, 10), 1, False), (0, 0))
        self.assertEqual(native_uv((60, 55), (100, 100), (20, 10), 1, False), (1, 1))
        self.assertEqual(native_uv((0, 0), (20, 10), (100, 100), 1, False), (0.4, 0.45))

    def test_tile_origin_period_and_partial_repeat(self):
        for scale in (1, 1.25, 1.5, 2, 3):
            self.assertEqual(native_uv((0, 0), (101, 99), (20, 10), scale, True), (0, 0))
            self.assertEqual(native_uv((20 / scale, 10 / scale), (101, 99), (20, 10), scale, True), (0, 0))
            self.assertEqual(native_uv((5 / scale, 5 / scale), (101, 99), (20, 10), scale, True), (0.25, 0.5))

    def test_bounded_grid_matches_retained_physical_equations(self):
        # Rotation swaps upright dimensions before rendering, so include both orders.
        for destination in ((100, 200), (200, 100), (1, 1)):
            for source in ((1, 1), (32, 16), (16, 32), (8192, 16384)):
                for scale in (1, 1.25, 1.5, 2, 3):
                    for tile in (False, True):
                        for fx, fy in ((0, 0), (0.125, 0.25), (0.5, 0.5), (0.875, 0.75), (1, 1)):
                            point = (destination[0] * fx, destination[1] * fy)
                            wanted = source_uv(point, destination, source, scale, tile)
                            actual = native_uv(point, destination, source, scale, tile)
                            self.assertEqual(wanted is None, actual is None)
                            if actual is not None:
                                for a, b in zip(actual, wanted):
                                    self.assertAlmostEqual(a, b, places=10)

    def test_reduced_storage_does_not_change_repeat_period(self):
        # Original 16000x8000 stored at 8000x4000: metadata retains physical size.
        uv = native_uv((8000, 4000), (20000, 10000), (16000, 8000), 1, True)
        self.assertEqual(uv, (0.5, 0.5))
        self.assertEqual(atlas_sample(uv, (0, 0, 8000, 4000)), (4000, 2000))

    def test_atlas_and_sheet_cells_cannot_bleed(self):
        for bounds in ((0, 0, 10, 20), (55, 99, 65, 119), (55.5, 99.5, 55.75, 99.75)):
            for uv in ((0, 0), (1, 1), (0.5, 0.5), (0.999999, 0.000001)):
                at = atlas_sample(uv, bounds)
                self.assertTrue(bounds[0] <= at[0] <= bounds[2])
                self.assertTrue(bounds[1] <= at[1] <= bounds[3])
        # Same original per-cell size on atlas and standalone page, independent of offset.
        uv = native_uv((16, 8), (100, 100), (64 / 2, 32 / 2), 1, True)
        self.assertEqual(uv, (0.5, 0.5))
        self.assertEqual(atlas_sample(uv, (100, 200, 116, 208)), (108, 204))


def check_source(sdk):
    work = ROOT / '.tmp/wallpaper-fit-review'
    if work.exists():
        shutil.rmtree(work)
    files = ['src/text.rs', 'src/gpu.rs', 'src/shape.wgsl', 'src/scene.rs',
             'src/language/vocabulary.rs']
    for name in files:
        dest = work / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(sdk / name, dest)
    for patch in ('image-reset.patch', 'fit.patch'):
        subprocess.run(['git', 'apply', '--unsafe-paths', '--directory=' + str(work),
                        str(ROOT / 'runtime/wallpaper' / patch)], check=True)
    text = (work / 'src/text.rs').read_text()
    gpu = (work / 'src/gpu.rs').read_text()
    shader = (work / 'src/shape.wgsl').read_text()
    vocab = (work / 'src/language/vocabulary.rs').read_text()
    assert 'IMAGE_FITS: &[&str] = &["fill", "contain", "cover", "centre", "tile"]' in vocab
    assert 'source.unwrap_or(px)' in text and text.count('slot.source_size = source_size') == 5
    assert '((3840, 2160), true, (3840, 2160))' in text
    assert 'self.elements[at * PER_ELEMENT + 16] = source.0;' in gpu
    assert 'self.elements[at * PER_ELEMENT + 18] = *fit as f32;' in gpu
    assert 'source = (source.0 / columns, source.1 / rows)' in gpu
    assert 'matches!(*fit, 1 | 2)' in gpu
    assert 'el.line.xy / max(u.header.w, 0.001)' in shader
    assert 'el.light.w < 0.5 && (el.line.z == 3.0 || el.line.z == 4.0)' in shader
    assert 'fract((q - el.dest.xy) / source_size)' in shader
    assert 'at = clamp(at, el.uv.xy + half_texel, el.uv.zw - half_texel)' in shader
    # Legacy mode math and atlas/picture dispatch are byte-for-byte unchanged.
    old = (sdk / 'src/gpu.rs').read_text()
    start, end = 'let k = (pw / ph)', '// Pixel art stays pixels'
    assert gpu[gpu.index(start):gpu.index(end)] == old[old.index(start):old.index(end)]
    old = (sdk / 'src/shape.wgsl').read_text()
    start, end = '// Above 0 in the second slot', '// Tinted:'
    assert shader[shader.index(start):shader.index(end)] == old[old.index(start):old.index(end)]
    view = (ROOT / 'shell/wallpaper/view.plm').read_text()
    for slot in range(32):
        for fit, index in (('cover', 0), ('contain', 1), ('centre', 2), ('tile', 3)):
            assert f'image wallpaper_image_{slot} ' in view
            assert f'fit: {fit}; show: wallpaper_fit == {index}' in view
        assert view.count(f'image wallpaper_image_{slot} {{') == 4
    assert 'to_workshop.send(Job::Live' in text  # Existing identity/cache boundary retained.
    print('Both patches apply; native source/legacy-mode/cache/scale/atlas guards pass.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=pathlib.Path, required=True)
    args = parser.parse_args()
    check_source(args.source)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(FitMath)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
