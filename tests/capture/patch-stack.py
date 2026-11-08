#!/usr/bin/env python3
"""Apply the exact integration prepare.sh patch sequence, with W-owned source overrides.
Source-only, bounded copies; no fetch, compiler, native runtime, display or decoder.
Every run is retained under .tmp, including a replay of the original fit collision.
"""
import argparse
import hashlib
import pathlib
import re
import shutil
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
W_PATCHES = {
    'runtime/wallpaper/catalog.patch', 'runtime/wallpaper/image-reset.patch',
    'runtime/wallpaper/fit.patch', 'runtime/wallpaper/raster-limits.patch', 'runtime/wallpaper/presets.patch', 'runtime/capture/native.patch',
    'runtime/capture/layout-link.patch', 'runtime/capture/image-lifecycle.patch',
    'runtime/capture/output-events.patch',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=pathlib.Path, required=True)
    parser.add_argument('--integration-ref', required=True, help='immutable S integration commit')
    parser.add_argument('--preapplied-shell-lock', action='store_true',
                        help='source already has exactly the first two shell/lock patches; reverse them in the private copy')
    args = parser.parse_args()
    ref = subprocess.check_output(['git', 'rev-parse', '--verify', args.integration_ref + '^{commit}'], cwd=ROOT, text=True).strip()
    def at_ref(name):
        return subprocess.check_output(['git', 'show', ref + ':' + name], cwd=ROOT)
    prepare = at_ref('runtime/prepare.sh').decode()
    patches = re.findall(r'git apply[^\n]*"\$root/([^"]+\.patch)"', prepare)
    assert 1 <= len(patches) <= 32 and len(patches) == len(set(patches)), 'invalid/unbounded patch order'
    if 'runtime/wallpaper/fit.patch' not in patches:
        patches.insert(patches.index('runtime/wallpaper/image-reset.patch') + 1, 'runtime/wallpaper/fit.patch')
    if 'runtime/wallpaper/raster-limits.patch' not in patches:
        patches.insert(patches.index('runtime/capture/image-lifecycle.patch') + 1, 'runtime/wallpaper/raster-limits.patch')
    if 'runtime/wallpaper/presets.patch' not in patches:
        patches.insert(patches.index('runtime/wallpaper/raster-limits.patch') + 1, 'runtime/wallpaper/presets.patch')
    required = ['runtime/wallpaper/catalog.patch', 'runtime/wallpaper/image-reset.patch',
                'runtime/wallpaper/fit.patch', 'runtime/capture/native.patch',
                'runtime/capture/layout-link.patch', 'runtime/capture/image-lifecycle.patch',
                'runtime/wallpaper/raster-limits.patch', 'runtime/wallpaper/presets.patch', 'runtime/capture/output-events.patch']
    assert [patches.index(p) for p in required] == sorted(patches.index(p) for p in required)
    stamp = str(time.time_ns())
    work = ROOT / '.tmp' / ('capture-patch-stack-' + stamp)
    source = work / 'source'
    patch_dir = work / 'patches'
    patch_dir.mkdir(parents=True)
    log = ROOT / '.tmp/check-results' / ('capture-patch-stack-' + stamp + '.txt')
    log.parent.mkdir(parents=True, exist_ok=True)
    def record(message):
        with log.open('a') as stream:
            stream.write(message + '\n')
    def apply(patch, reverse=False, expected=0):
        command = ['git', 'apply', '--unsafe-paths', '--directory=' + str(source)]
        if reverse:
            command.append('--reverse')
        if expected:
            command.append('--check')
        command.append(str(patch))
        result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        record('COMMAND: ' + repr(command) + '\nEXIT: ' + str(result.returncode) + '\n' + result.stdout)
        assert (result.returncode == 0) == (expected == 0), 'unexpected patch result; see ' + str(log)
    record('INTEGRATION: ' + ref + '\nINPUT_SOURCE: ' + str(args.source.resolve()))
    files = set()
    targets = set()
    for index, name in enumerate(patches):
        data = (ROOT / name).read_bytes() if name in W_PATCHES else at_ref(name)
        assert len(data) <= 1024 * 1024, 'patch exceeds 1MiB'
        path = patch_dir / (str(index).zfill(2) + '-' + pathlib.Path(name).name)
        path.write_bytes(data)
        for line in data.decode().splitlines():
            if line.startswith('--- a/'):
                files.add(line[6:])
            if line.startswith('+++ b/'):
                targets.add(line[6:])
        record('PATCH: ' + name + ' SHA256: ' + hashlib.sha256(data).hexdigest())
    assert len(files | targets) <= 64, 'too many native target files'
    total = 0
    for name in files | targets:
        relative = pathlib.PurePosixPath(name)
        assert not relative.is_absolute() and '..' not in relative.parts
        destination = source / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if name not in files:
            continue
        original = args.source / relative
        size = original.stat().st_size
        assert size <= 8 * 1024 * 1024, 'source file exceeds 8MiB'
        total += size
        assert total <= 32 * 1024 * 1024, 'source exceeds 32MiB'
        shutil.copyfile(original, destination)
        record('SOURCE: ' + name + ' SHA256: ' + hashlib.sha256(destination.read_bytes()).hexdigest())
    if args.preapplied_shell_lock:
        assert patches[:2] == ['runtime/patches/0001-shell-settings.patch', 'runtime/patches/0002-lock-boundary.patch']
        # Exact inverse of the integration's first two patches; no SDK mutation.
        for index in (1, 0):
            apply(patch_dir / (str(index).zfill(2) + '-' + pathlib.Path(patches[index]).name), reverse=True)
    for index, name in enumerate(patches):
        if name == 'runtime/capture/image-lifecycle.patch':
            original = patch_dir / 'original-lifecycle-24cce12.patch'
            original.write_bytes(subprocess.check_output(['git', 'show', '24cce12:runtime/capture/image-lifecycle.patch'], cwd=ROOT))
            apply(original, expected=1)  # Replay is check-only; preserve diagnostic and partial source.
        apply(patch_dir / (str(index).zfill(2) + '-' + pathlib.Path(name).name))
    raster_module = ROOT / 'runtime/wallpaper/raster.rs'
    raster_target = source / 'src/tohru_wallpaper/raster.rs'
    raster_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(raster_module, raster_target)
    record('INSTALLED_W_SOURCE: runtime/wallpaper/raster.rs SHA256: ' + hashlib.sha256(raster_target.read_bytes()).hexdigest())
    text = (source / 'src/text.rs').read_text()
    start = text.index('fn capture_picture(')
    end = text.index('\n    }', start)
    capture = text[start:end]
    assert 'slot.source_size = (frame.width, frame.height)' in capture
    assert 'fit_within((frame.width, frame.height)' in capture and 'fit_frame(&frame, px)' in capture
    assert 'pub fn release_capture(' in text and 'slot.source_size = source_size' in text
    assert 'std::fs::read(path)' not in text
    assert 'image::load_from_memory' not in text and 'collect_frames()' not in text
    assert text.count('slot.source_size = decoded_size') == 4
    assert 'raster::decode(&data)' in text and 'raster::dimensions_bytes(&data)' in text
    compact = re.sub(r'\s+', '', raster_target.read_text())
    for guard in ('64*1024*1024', '32*1024*1024', '16_384', '256*1024*1024',
                  'libc::O_NONBLOCK', 'file.metadata()?', 'reader.take(limit.saturating_add(1))',
                  'reader.limits(limits())', 'Cursor::new(bytes)', 'output.len()>=4096', '*total<=MAX_ALLOC'):
        assert guard in compact, guard

    assert 'ToRender::ReleaseCapture(name)' in (source / 'src/render.rs').read_text()
    logic = (source / 'src/logic_luau.rs').read_text()
    assert 'capture_query_mutates(name) || name == "wallpaper.preset"' in logic
    assert 'check_service_permission(&c, &name, query_mutates(&name))?' in logic
    record('PASS: complete ordered patch application; original collision reproduced; frozen source dimensions and release hooks retained.')
    print('PASS:', len(patches), 'patches;', len(files), 'existing source files;', total, 'bytes')
    print('Evidence:', log)
    print('Retained source:', source)


if __name__ == '__main__':
    main()
