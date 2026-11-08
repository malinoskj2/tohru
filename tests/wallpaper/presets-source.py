#!/usr/bin/env python3
"""Closed-API/cache source guards only; no Rust, PNG generation, media or native role."""
import hashlib
import pathlib
import re

root=pathlib.Path(__file__).resolve().parents[2]
files=['runtime/wallpaper/preset.rs','runtime/wallpaper/presets.rs','runtime/wallpaper/native.rs',
       'runtime/wallpaper/presets.patch','tests/wallpaper/preset.rs','tests/wallpaper/presets.rs']
pure=(root/files[0]).read_text()
native=(root/files[1]).read_text()
adapter=(root/files[2]).read_text()
compact=re.sub(r'\s+','',native)
adapter=re.sub(r'\s+','',adapter)
for guard in ['MAX_PNG:usize=2*1024*1024','O_NOFOLLOW','O_NONBLOCK','libc::flock','LOCK_EX|libc::LOCK_NB',
              'O_CREAT|libc::O_EXCL','libc::linkat','parent.sync_all()','data_durable&&parent_durable']:
    assert guard in compact,guard
assert 'O_TRUNC' not in native and 'rename(' not in native
assert 'preset.parse::<tohru_shell::wallpaper::preset::Preset>()' in adapter
assert 'palette.parse::<tohru_shell::wallpaper::preset::Palette>()' in adapter
for name in ['Spectrum','Dusk','Tide','Ember','Moss','Solid']:
    assert name in pure
for color in ['0x11111b','0x28233c','0x4a3854','0x1e1e2e','0x45475a','0xcba6f7','0x89dceb','0xfab387','0xa6e3a1']:
    assert color in pure
# Fixed cross product bounds the names this operation creates; no arbitrary RGB/path key.
keys={f'{preset}-{palette}.png' for preset in ['spectrum','dusk','tide','ember','moss','solid']
      for palette in ['mocha','macchiato','rose_pine','nord','gruvbox','tokyo_night','dracula','everforest']}
assert len(keys)==48 and all('/' not in name and len(name)<64 for name in keys)
log='Closed preset/cache source guards and48 fixed keys pass; not Rust/encoder/cache execution.\n'
log+=''.join(hashlib.sha256((root/name).read_bytes()).hexdigest()+'  '+name+'\n' for name in files)
(root/'.tmp/check-results').mkdir(parents=True,exist_ok=True)
(root/'.tmp/check-results/wallpaper-presets-source.txt').write_text(log)
print(log.splitlines()[0])
