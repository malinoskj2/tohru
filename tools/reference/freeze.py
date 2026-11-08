#!/usr/bin/env python3
"""Record source identity without copying diffs, file contents, or user config."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args])


def fingerprint(root):
    status = git(root, 'status', '--porcelain=v1', '-z', '--untracked-files=all')
    entries = status.split(b'\0')
    dirty = []
    while entries:
        entry = entries.pop(0)
        if not entry:
            continue
        code, name = entry[:2].decode(), entry[3:].decode()
        path = root / name
        row = {'status': code, 'path': name}
        if 'R' in code or 'C' in code:
            row['original_path'] = entries.pop(0).decode()
        if path.is_symlink():
            row['symlink'] = True
        elif path.is_file():
            row['working_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
            row['bytes'] = path.stat().st_size
        else:
            row['absent'] = True
        dirty.append(row)
    # Hash both index and working diff, including binaries; never publish their contents.
    return {'root': str(root), 'head': git(root, 'rev-parse', 'HEAD').decode().strip(),
            'branch': git(root, 'branch', '--show-current').decode().strip(),
            'dirty': dirty, 'status_sha256': hashlib.sha256(status).hexdigest(),
            'index_diff_sha256': hashlib.sha256(git(root, 'diff', '--cached', '--binary', '--no-ext-diff')).hexdigest(),
            'working_diff_sha256': hashlib.sha256(git(root, 'diff', '--binary', '--no-ext-diff')).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('repositories', nargs='+', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    result = {'recorded_utc': datetime.now(timezone.utc).isoformat(),
              'policy': 'identity and hashes only; no patches, contents, config or secrets copied',
              'repositories': [fingerprint(p.resolve()) for p in args.repositories]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
