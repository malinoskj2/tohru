"""Actual shipped CLI journey; generate files only, never install/start services."""
from evidence import digest
from process import execute, private_environment


def snapshot(directory):
    return {p.name: {'sha256': digest(p), 'text': p.read_text()}
            for p in sorted(directory.glob('*.service'))}


def generated_units_valid(files, prefix):
    expected = {'tohru-bar.service': 'tohru-run',
                'tohru-launcher.service': 'tohru-launcher-run',
                'tohru-lock.service': 'tohru-lock'}
    if set(files) != set(expected):
        return False
    for name, program in expected.items():
        text = files[name]['text']
        if not all(line in text.splitlines() for line in (
                '[Unit]', '[Service]', 'PartOf=graphical-session.target',
                'After=graphical-session.target', 'ConditionEnvironment=WAYLAND_DISPLAY',
                f'ExecStart="{prefix / program}"')):
            return False
    return ('KillMode=process' in files['tohru-launcher.service']['text'].splitlines()
            and 'Type=notify' in files['tohru-lock.service']['text'].splitlines()
            and 'NotifyAccess=all' in files['tohru-lock.service']['text'].splitlines()
            and 'WantedBy=' not in files['tohru-lock.service']['text']
            and 'WantedBy=graphical-session.target' in files['tohru-bar.service']['text'].splitlines())


def drive(root, run, timeout):
    executable = root / 'bin/tohru'
    units = run / 'units'
    command = [str(executable), 'session-units', '--output', str(units),
               '--components', 'bar,launcher', '--prefix', str(root / 'bin')]
    actions, observations, assertions = [], [], []
    before = snapshot(units)
    commands = [('generate', command), ('refuse-overwrite', command),
                ('reject-invalid-target', [str(executable), 'session-units', '--output',
                                          str(run / 'rejected-units'), '--target', 'bad target'])]
    original = None
    for index, (name, argv) in enumerate(commands):
        action = execute(argv, root, private_environment(run), run / f'action-{index}.log', timeout)
        action.update(sequence=index + 1, entrypoint='bin/tohru session-units',
                      action_id=name, deadline_seconds=timeout)
        actions.append(action)
        files = snapshot(units)
        observations.append({'action_id': name, 'files': files,
                             'before_files': before if index == 0 else original,
                             'rejected_output_exists': (run / 'rejected-units').exists()})
        healthy = action['reason'] is None
        if name == 'generate':
            passed = healthy and action['exit'] == 0 and not before and generated_units_valid(files, root / 'bin')
            original = files
        elif name == 'refuse-overwrite':
            passed = healthy and action['exit'] != 0 and files == original
        else:
            passed = healthy and action['exit'] != 0 and not (run / 'rejected-units').exists() and files == original
        assertions.append({'action_id': name, 'passed': passed,
                           'expected': {'generate': 'three correctly bound session files',
                                        'refuse-overwrite': 'nonzero exit and identical persisted files',
                                        'reject-invalid-target': 'nonzero exit, no rejected directory, original files unchanged'}[name]})
        if not passed:
            break
    return ('passed' if len(assertions) == 3 and all(a['passed'] for a in assertions) else 'failed',
            actions, observations, assertions)
