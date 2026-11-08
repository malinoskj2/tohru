"""Actual workload secrecy assertion: exact native denial, never arbitrary error."""
import importlib.util
from pathlib import Path
import sys
sys.dont_write_bytecode=True
spec=importlib.util.spec_from_file_location('native_service_check',Path(__file__).with_name('native.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
module.assert_secret_denied('? secret input cannot be read')
for response in ('fixture-password','? secret input cannot be read\nfixture-password','? other failure',None,'','<secret>'):
    try:module.assert_secret_denied(response)
    except AssertionError:pass
    else:raise AssertionError('unexpected/secret disclosure accepted')
print('Exact protected secret denial accepted; disclosure/echo/arbitrary error/empty response rejected')
