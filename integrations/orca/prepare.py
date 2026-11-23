#!/usr/bin/env python3
"""Prepare a fresh isolated Orca source tree; never modify the supplied sources."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", required=True, type=Path)
    parser.add_argument("--routing-patch", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / ".tmp/orca-progress/review-source")
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / ".tmp") or output.exists():
        parser.error("--output must be a fresh directory under this worktree's .tmp")
    provenance = json.loads((HERE / "provenance.json").read_text())
    if digest(args.upstream / "package.json") != provenance["upstreamPackageSha256"]:
        parser.error("Upstream package manifest differs from the pinned Orca 1.4.268 source")
    if digest(args.routing_patch) != provenance["routingPatchSha256"]:
        parser.error("Routing patch differs from the reviewed dependency; review/rebase instead of forcing application")
    shutil.copytree(args.upstream, output, symlinks=True,
                    ignore=shutil.ignore_patterns(".git", "node_modules", ".tmp"))
    for path in [output, *output.rglob("*")]:
        if not path.is_symlink():
            path.chmod(path.stat().st_mode | stat.S_IWUSR)
    def apply(patch):
        command = ["git", "apply", "--unsafe-paths", "--directory=" + str(output)]
        subprocess.run([*command, "--check", str(patch.resolve())], cwd=ROOT, check=True)
        subprocess.run([*command, str(patch.resolve())], cwd=ROOT, check=True)
    apply(args.routing_patch)
    for name, expected in provenance["baseFiles"].items():
        if digest(output / name) != expected:
            parser.error(f"Routing source differs at {name}; keep the output for review")
    apply(HERE / "desktop-voice-tasks.patch")
    print(f"Prepared isolated Orca {provenance['orcaVersion']} source: {output}")
    print("No installed application, service, or upstream checkout was changed.")


if __name__ == "__main__":
    main()
