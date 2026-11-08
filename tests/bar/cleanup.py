#!/usr/bin/env python3
"""CPU-only fault injection against the actual native runner cleanup helper."""
import sys
sys.dont_write_bytecode = True
from pathlib import Path
import subprocess
from unittest.mock import patch
from native import finish_owned

calls = []
class Process:
    def __init__(self, label, timeout=False, broken=False):
        self.label, self.timeout, self.broken = label, timeout, broken
    def poll(self): return None
    def terminate(self):
        calls.append((self.label, 'terminate'))
        if self.broken: raise OSError('injected stop failure')
    def wait(self, timeout):
        calls.append((self.label, 'wait'))
        if self.timeout:
            self.timeout = False
            raise subprocess.TimeoutExpired('private fixture', timeout)
    def kill(self): calls.append((self.label, 'kill'))
class Log:
    def __init__(self, label): self.label = label
    def close(self): calls.append((self.label, 'close'))

with patch.object(Path, 'write_text', side_effect=OSError('injected evidence write failure')):
    try:
        finish_owned([Process('app'), Process('pointer', timeout=True), Process('compositor')], [Log('app-log'), Log('compositor-log')], Path('/no-write'), [])
    except ExceptionGroup as error:
        assert any('evidence write' in str(item) for item in error.exceptions)
    else:
        raise AssertionError('write failure was hidden')
for label in ('app', 'pointer', 'compositor'):
    assert (label, 'terminate') in calls and (label, 'wait') in calls
assert ('pointer', 'kill') in calls
assert ('app-log', 'close') in calls and ('compositor-log', 'close') in calls
calls.clear()
with patch.object(Path, 'write_text', return_value=0):
    try:
        finish_owned([Process('app'), Process('broken', broken=True), Process('compositor')], [Log('log')], Path('/no-write'), [])
    except ExceptionGroup:
        pass
    else:
        raise AssertionError('cleanup failure was hidden')
assert ('app', 'terminate') in calls and ('compositor', 'terminate') in calls and ('log', 'close') in calls
print('Actual cleanup helper attempts every owner before evidence failure; timeout escalates to kill')
