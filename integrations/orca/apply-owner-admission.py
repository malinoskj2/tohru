#!/usr/bin/env python3
"""Apply owner/admission corrections to an isolated, already-patched Orca test tree."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--stage', choices=('owner', 'background', 'interest'), default='owner')
    args = parser.parse_args()
    source = args.source.resolve()
    if not source.is_relative_to(ROOT / '.tmp'):
        parser.error('--source must be an isolated test tree under this worktree .tmp')
    manifest, name = {
        'owner': ('owner-admission-provenance.json', 'desktop-voice-owner-admission.patch'),
        'background': ('background-protocol-provenance.json', 'desktop-voice-background-protocol.patch'),
        'interest': ('current-interest-provenance.json', 'desktop-voice-current-interest.patch'),
    }[args.stage]
    provenance = json.loads((HERE / manifest).read_text())
    patch = HERE / name
    if digest(patch) != provenance['patchSha256']:
        parser.error('Patch differs from its reviewed provenance')
    for name, expected in provenance['baseFiles'].items():
        if digest(source / name) != expected:
            parser.error(f'Base differs at {name}; review/rebase instead of forcing application')
    command = ['git', 'apply', '--unsafe-paths', '--directory=' + str(source)]
    subprocess.run([*command, '--check', str(patch)], cwd=ROOT, check=True)
    subprocess.run([*command, str(patch)], cwd=ROOT, check=True)
    print(f'Applied owner/admission corrections to isolated Orca {provenance["orcaVersion"]}: {source}')

if __name__ == '__main__':
    main()
