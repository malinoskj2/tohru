"""Bound storage while the owned process group is alive; never retry mutations."""
import os
import selectors
import signal
import subprocess
import time


def private_environment(run):
    environment = {'PATH': os.environ.get('PATH', ''), 'HOME': str(run / 'home'),
                   'XDG_CONFIG_HOME': str(run / 'config'), 'XDG_DATA_HOME': str(run / 'data'),
                   'XDG_STATE_HOME': str(run / 'state'), 'XDG_CACHE_HOME': str(run / 'cache'),
                   'XDG_RUNTIME_DIR': str(run / 'runtime'), 'TMPDIR': str(run / 'tmp'),
                   'CI': '1', 'ELECTRON_RUN_AS_NODE': '1', 'SCCACHE_DISABLE': '1'}
    # The checkout's native commands are links to bin/tohru-native, which finds the build here.
    for key in ('TOHRU_DESKTOP_TARGET', 'CARGO_TARGET_DIR'):
        if os.environ.get(key):
            environment[key] = os.environ[key]
    return environment


def execute(argv, cwd, env, log, timeout=180, limit=2 * 1024 * 1024):
    wall_start_ns = time.time_ns()
    started = time.monotonic()
    size, reason = 0, None
    child = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, start_new_session=True)
    try:
        with selectors.DefaultSelector() as selector, log.open('xb') as output:
            selector.register(child.stdout, selectors.EVENT_READ)
            while selector.get_map():
                remaining = timeout - (time.monotonic() - started)
                if remaining <= 0:
                    reason = 'timeout'
                    break
                for key, _ in selector.select(min(remaining, 0.1)):
                    chunk = os.read(key.fileobj.fileno(), min(65536, limit - size + 1))
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    output.write(chunk[:limit - size])
                    size += len(chunk)
                    if size > limit:
                        reason = 'output_limit'
                        break
                if reason:
                    break
            if reason is None:
                try:
                    child.wait(timeout=max(.001, timeout - (time.monotonic() - started)))
                except subprocess.TimeoutExpired:
                    reason = 'timeout'
    finally:
        # Even a successful parent must not leave its owned group running.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.stdout.close()
        child.wait(timeout=5)
    return {'argv': argv, 'exit': child.returncode, 'reason': reason,
            'wall_start_ns': wall_start_ns, 'wall_end_ns': time.time_ns(),
            'duration_seconds': time.monotonic() - started, 'stored_bytes': min(size, limit),
            'pid': child.pid, 'reaped': True, 'log': str(log)}
