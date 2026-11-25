"""Immutable collections: files, marker, then atomic pointer. No mutable latest proof."""
import hashlib
import json
import os
from pathlib import Path
import time
import uuid

REQUIRED = ('manifest.json', 'actions.jsonl', 'observations.jsonl', 'assertions.json',
            'before.json', 'after.json', 'resources.json', 'timings.json', 'result.json')


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def atomic(path, value):
    temporary = path.with_name('.' + path.name + '-' + str(uuid.uuid4()))
    with temporary.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def read(path):
    if path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('metadata exceeds 4 MiB')
    return json.loads(path.read_text())


def jsonl(path, rows):
    with path.open('x') as stream:
        for row in rows:
            stream.write(json.dumps(row) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


def records(path):
    if path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('event stream exceeds metadata budget')
    return [json.loads(line) for line in path.read_text().splitlines()]


def publish(run):
    state = read(run / 'state.json')
    if state['phase'] != 'cleaned':
        raise ValueError('cleanup must complete before publishing')
    collection = run / 'collections' / str(uuid.uuid4())
    collection.mkdir(parents=True)
    for name in REQUIRED:
        (collection / name).write_bytes((run / name).read_bytes())
    manifest = read(collection / 'manifest.json')
    manifest['collection_id'] = collection.name
    atomic(collection / 'manifest.json', manifest)
    for path in run.glob('*.log'):
        (collection / path.name).write_bytes(path.read_bytes())
    for path in collection.iterdir():
        with path.open('rb') as stream:
            os.fsync(stream.fileno())
    marker = {'schema_version': 1, 'run_id': state['run_id'],
              'scenario': state['scenario'], 'collection_id': collection.name,
              'action_start_ns': state['action_start_ns'],
              'action_end_ns': state['action_end_ns'], 'completed_ns': time.time_ns(),
              'cursor': len(records(run / 'actions.jsonl')), 'collector_exit': 0,
              'complete': True, 'hashes': {p.name: digest(p) for p in collection.iterdir()}}
    atomic(collection / 'collection-complete.json', marker)
    validate(run, collection)
    atomic(run / 'latest-completed.json', {'collection_id': collection.name})
    return collection


def validate(run, collection=None):
    state = read(run / 'state.json')
    collection = collection or run / 'collections' / read(run / 'latest-completed.json')['collection_id']
    if collection.is_symlink():
        raise ValueError('symlink collection')
    collection = collection.resolve()
    if collection.parent != (run / 'collections').resolve():
        raise ValueError('collection outside owned run')
    marker = read(collection / 'collection-complete.json')
    manifest = read(collection / 'manifest.json')
    if not (marker['schema_version'] == 1 and marker['complete'] is True and marker['collector_exit'] == 0
            and marker['collection_id'] == collection.name
            and manifest['collection_id'] == collection.name
            and marker['run_id'] == state['run_id'] == manifest['run_id']
            and marker['scenario'] == state['scenario'] == manifest['scenario']
            and marker['action_start_ns'] == state['action_start_ns']
            and marker['action_end_ns'] == state['action_end_ns']
            and marker['completed_ns'] >= marker['action_end_ns'] >= marker['action_start_ns']
            and marker['cursor'] == len(records(collection / 'actions.jsonl'))):
        raise ValueError('stale or mismatched completion marker')
    if not set(REQUIRED).issubset(marker['hashes']):
        raise ValueError('missing required artifacts')
    for name, expected in marker['hashes'].items():
        path = collection / name
        if Path(name).name != name or path.is_symlink() or digest(path) != expected:
            raise ValueError('artifact hash mismatch: ' + name)
    return read(collection / 'result.json')
