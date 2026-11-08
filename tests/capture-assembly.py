#!/usr/bin/env python3
"""CPU-only checks of S's actual prepared native helper/exit assembly."""
import argparse
from pathlib import Path
import subprocess
import tempfile
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
source = args.source.resolve()
manifest = tomllib.loads((source / 'Cargo.toml').read_text())
assert manifest['package']['default-run'] == 'pleamar'
helper = [entry for entry in manifest['bin'] if entry['name'] == 'tohru-capture-clipboard']
assert len(helper) == 1 and helper[0]['path'] == 'src/tohru_capture_clipboard.rs'
assert (source / helper[0]['path']).read_bytes() == (root / 'runtime/capture/clipboard.rs').read_bytes()
for name in ['native.rs', 'clipboard_owner.rs']:
    assert (source / 'src/platform/tohru_capture' / name).read_bytes() == (root / 'runtime/capture' / name).read_bytes()
assert sorted(p.name for p in (source / 'foundation/runtime/capture').iterdir()) == ['files.rs', 'geometry.rs', 'mod.rs']
foundation = tomllib.loads((source / 'foundation/Cargo.toml').read_text())
assert foundation['package']['rust-version'] == '1.85'
assert not {'image', 'wl-clipboard-rs', 'libc'} & foundation['dependencies'].keys()


def function(text, signature):
    start = text.index(signature)
    at = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[at] == '{') - (text[at] == '}')
        at += 1
    return text[start:at]


forward = function((source / 'src/platform/mod.rs').read_text(), 'pub(crate) fn shutdown_capture()')
quit_body = function((source / 'src/lib.rs').read_text(), 'fn quit() -> !')
assert quit_body.count('platform::shutdown_capture();') == 1
# Execute the actual exit body, replacing only terminal process exit to retain
# the test process. Mock foreign owners expose exact teardown forwarding order.
quit_body = quit_body.replace('std::process::exit(0)', 'panic!("fixture process exit")')
fixture = '''
use std::sync::Mutex;
static EVENTS: Mutex<Vec<&str>> = Mutex::new(Vec::new());
static BEFORE_QUIT: Mutex<Option<fn()>> = Mutex::new(Some(before));
fn before() { EVENTS.lock().unwrap().push("before"); }
mod logic_luau {pub fn stop_children(){super::EVENTS.lock().unwrap().push("logic children");}}
mod tohru_idle {pub fn shutdown(){super::EVENTS.lock().unwrap().push("idle owners");}}
mod platform {
 mod capture {pub fn shutdown(){crate::EVENTS.lock().unwrap().push("capture manager");}}
'''+forward+'\n}\n'+quit_body+'''
#[test]fn orderly_quit_stops_capture_owner_before_process_exit(){
 assert!(std::panic::catch_unwind(quit).is_err());
 assert_eq!(*EVENTS.lock().unwrap(), vec!["before","logic children","idle owners","capture manager"]);
}
'''
with tempfile.TemporaryDirectory(prefix='capture-assembly-', dir=root / '.tmp') as directory:
    directory = Path(directory)
    path = directory / 'exit.rs'
    path.write_text(fixture)
    binary = directory / 'exit'
    subprocess.run(['rustc', '--edition=2024', '--cfg', 'feature="luau"', '-Dwarnings', '--test', str(path), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
print('Actual helper placement, pure foundation boundary and orderly exit forwarding PASS')
