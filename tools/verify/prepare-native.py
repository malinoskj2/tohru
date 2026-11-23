#!/usr/bin/env python3
"""Prepare a private native test tree from an existing Nix derivation, offline.

This deliberately does not execute the derivation's shell hooks or build the GUI.
Only literal generated src/*.ts modules are copied from postPatch.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

from evidence import atomic, digest
from process import execute

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--derivation-json', type=Path, required=True,
                        help='Saved nix derivation show JSON for the selected installed Orca')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pnpm', type=Path, required=True)
    parser.add_argument('--zstd', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / '.tmp') or output.exists():
        parser.error('output must be fresh and under this worktree .tmp')
    data = json.loads(args.derivation_json.read_text())
    derivations = data.get('derivations', data)
    if len(derivations) != 1:
        parser.error('select exactly one derivation')
    drv_name, drv = next(iter(derivations.items()))
    inputs = drv['env']
    upstream = Path(inputs['src'])
    patches = [Path(p) for p in inputs['patches'].split()]
    archive = Path(inputs['pnpmDeps']) / 'pnpm-store.tar.zst'
    for path in [upstream, archive, *patches, args.pnpm, args.zstd]:
        if not str(path.resolve()).startswith('/nix/store/') or not path.exists():
            parser.error('input must exist in the immutable store: ' + str(path))
    if json.loads((upstream / 'package.json').read_text())['version'] != '1.4.268':
        parser.error('unsupported upstream version')
    for local in (ROOT / 'integrations/orca').glob('*.patch'):
        matches = [p for p in patches if p.name.endswith('-' + local.name)]
        if not matches or digest(matches[0]) != digest(local):
            parser.error('selected derivation does not contain the reviewed patch: ' + local.name)
    output.mkdir(parents=True, mode=0o700)
    source = output / 'source'
    shutil.copytree(upstream, source, symlinks=True)
    for path in [source, *source.rglob('*')]:
        if not path.is_symlink():
            path.chmod(path.stat().st_mode | 0o200)
    env = {'PATH': str(args.pnpm.parent) + ':' + str(args.zstd.parent) + ':' + os.environ.get('PATH', ''),
           'HOME': str(output / 'home'), 'TMPDIR': str(output / 'tmp'),
           'CI': '1', 'SCCACHE_DISABLE': '1', 'pnpm_config_pm_on_fail': 'ignore',
           'pnpm_config_trust_lockfile': 'true', 'npm_config_manage_package_manager_versions': 'false'}
    for directory in ('home', 'tmp', 'store'):
        (output / directory).mkdir()
    commands = [['git', 'apply', '--unsafe-paths', '--directory=' + str(source), str(p)] for p in patches]
    # Preserve the dependency store's writable index semantics from pnpmConfigHook.
    commands.append(['tar', '--zstd', '-xf', str(archive), '-C', str(output / 'store')])
    receipts = []
    for index, command in enumerate(commands):
        result = execute(command, ROOT, env, output / f'prepare-{index}.log', 300)
        receipts.append(result)
        if result['exit'] or result['reason']:
            atomic(output / 'preparation.json', {'outcome': 'failed', 'receipts': receipts})
            return 1
    generated = []
    for match in re.finditer(r"cat > (src/[\w/.-]+\.ts) <<'EOF'\n(.*?)\nEOF", inputs['postPatch'], re.S):
        path = source / match[1]
        if not path.resolve().is_relative_to(source):
            raise ValueError('generated module escapes private source')
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(match[2] + '\n')
        generated.append(match[1])
    for path in (output / 'store').rglob('*'):
        if not path.is_symlink():
            path.chmod(path.stat().st_mode | 0o200)
    sql = output / 'store/v11/index.db.sql'
    if sql.exists():
        import sqlite3
        with sqlite3.connect(sql.with_suffix('')) as connection:
            connection.executescript(sql.read_text())
        sql.unlink()
    for name, command in [('install', [str(args.pnpm), 'install', '--offline', '--ignore-scripts',
                                       '--frozen-lockfile', '--store-dir', str(output / 'store'),
                                       '--config.package-import-method=clone-or-copy']),
                          ('packages', [str(args.pnpm), 'run', 'build:packages'])]:
        result = execute(command, source, env, output / f'{name}.log', 600)
        receipts.append(result)
        atomic(output / 'preparation.json', {'outcome': 'failed' if result['exit'] or result['reason'] else 'prepared',
               'derivation': drv_name, 'derivation_json_sha256': digest(args.derivation_json),
               'upstream': str(upstream), 'archive_sha256': digest(archive),
               'patches': {str(p): digest(p) for p in patches},
               'generated_modules': generated, 'receipts': receipts,
               'excluded': 'Other Nix postPatch transformations and GUI/package build; no host access'})
        if result['exit'] or result['reason']:
            print(str(output / 'preparation.json'))
            return 1
    print(str(source))
    return 0


if __name__ == '__main__':
    sys.exit(main())
