"""Test-only namespace assembly; production authentication stays native PAM."""
import hashlib
import os
from pathlib import Path
import sys


def command(arguments, bwrap, modules, evidence, run, mode):
    for name in ('pam_permit.so', 'pam_deny.so', 'pam_exec.so'):
        if not (modules / name).is_file():
            raise RuntimeError('missing controlled PAM module: ' + name)
    pam = evidence / ('pam-' + mode)
    pam.mkdir()
    if mode == 'permit':
        helper = evidence / 'pam-release-gate.py'
        helper.write_text('#!' + sys.executable + '\n'
            'from pathlib import Path\nimport time\n'
            'root=Path(' + repr(str(evidence)) + ')\n'
            '(root/"pam-started").write_text("ready")\n'
            'deadline=time.monotonic()+10\n'
            'while not (root/"pam-continue").exists():\n'
            '    if time.monotonic()>=deadline: raise SystemExit(1)\n'
            '    time.sleep(0.01)\n')
        helper.chmod(0o700)
        auth = 'auth required ' + str(modules / 'pam_exec.so') + ' quiet ' + str(helper) + '\n'
        auth += 'auth required ' + str(modules / 'pam_permit.so') + '\n'
    else:
        auth = 'auth required ' + str(modules / ('pam_deny.so' if mode == 'deny' else 'pam_permit.so')) + '\n'
    policy = auth + 'account required ' + str(modules / ('pam_deny.so' if mode == 'account-deny' else 'pam_permit.so')) + '\n'
    (pam / 'pleamar').write_text(policy)
    digest = hashlib.sha256(policy.encode()).hexdigest()
    result = [str(bwrap), '--unshare-user', '--unshare-pid', '--die-with-parent',
              '--ro-bind', '/', '/', '--dev-bind', '/dev', '/dev', '--proc', '/proc',
              '--tmpfs', '/etc']
    for entry in ('passwd', 'group', 'nsswitch.conf', 'fonts'):
        path = Path('/etc') / entry
        if path.exists(): result += ['--ro-bind', str(path), str(path)]
    result += ['--ro-bind', str(pam), '/etc/pam.d', '--bind', str(evidence), str(evidence),
               '--bind', str(run), str(run), '--', sys.executable,
               str(Path(__file__).with_name('pam_guard.py')), digest,
               os.readlink('/proc/self/ns/mnt'), os.readlink('/proc/self/ns/user'), *arguments]
    return result, digest
