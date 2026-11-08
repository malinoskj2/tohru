#!/usr/bin/env python3
"""Exec a test locker only after proving its private PAM mount and namespace."""
import hashlib
import os
from pathlib import Path
import sys


def verify(digest, parent_mount, parent_user):
    if os.readlink('/proc/self/ns/mnt') == parent_mount:
        raise RuntimeError('refusing PAM: mount namespace was not isolated')
    if os.readlink('/proc/self/ns/user') == parent_user:
        raise RuntimeError('refusing PAM: user namespace was not isolated')
    policy = Path('/etc/pam.d/pleamar')
    if hashlib.sha256(policy.read_bytes()).hexdigest() != digest:
        raise RuntimeError('refusing PAM: private fixture policy differs')
    if Path('/etc/pam.d/login').exists():
        raise RuntimeError('refusing PAM: unexpected fallback policy')


def main():
    digest, parent_mount, parent_user, executable, *arguments = sys.argv[1:]
    verify(digest, parent_mount, parent_user)
    print('PRIVATE_PAM_FIXTURE_VERIFIED ' + digest, flush=True)
    os.execv(executable, [executable, *arguments])


if __name__ == '__main__':
    main()
