#!/usr/bin/env python3
"""Read-only comparison of a Nix runtime's declared inputs with this checkout."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]


def query(*argv):
    return subprocess.check_output(argv, text=True, timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve()
    drv = query("nix-store", "-q", "--deriver", str(runtime.parents[1])).strip()
    raw = json.loads(query("nix", "derivation", "show", drv))
    env = next(iter(raw.get("derivations", raw).values()))["env"]
    source = Path(re.search(r"/nix/store/[^ /]+-source", env["postPatch"])[0])
    pairs = []
    for area in ("runtime/Cargo.toml", "runtime/Cargo.lock", "runtime/src", "runtime/tests",
                 "runtime/pleamar/bindings", "runtime/pleamar/tests/role-install.rs", "LICENSE"):
        local = ROOT / area
        files = [local] if local.is_file() else local.rglob("*")
        for file in files:
            if file.is_file() and (area != "runtime/src" or file.suffix == ".rs"):
                pairs.append((file, source / file.relative_to(ROOT)))
    patches = env["patches"].split()
    for patch in patches:
        pairs.append((ROOT / "runtime/pleamar/patches" / Path(patch).name[33:], Path(patch)))
    lock = re.search(r"cp (/nix/store/[^ ]+-Cargo.lock) Cargo.lock", env["postPatch"])[1]
    pairs.append((ROOT / "runtime/pleamar/Cargo.lock", Path(lock)))
    rows = [{"source": str(a.relative_to(ROOT)), "store": str(b),
             "match": a.is_file() and b.is_file() and a.read_bytes() == b.read_bytes()} for a, b in pairs]
    expected_patches = {p.name for p in (ROOT / "runtime/pleamar/patches").glob("*.patch")}
    patch_set_matches = expected_patches == {Path(p).name[33:] for p in patches}
    pin = json.loads((ROOT / "runtime/pleamar/pin.json").read_text())
    upstream_matches = "sha256-" + base64.b64encode(hashlib.sha256(Path(env["src"]).read_bytes()).digest()).decode() == pin["hash"]
    result = {"runtime": str(runtime), "deriver": drv, "files": rows,
              "patch_set_matches": patch_set_matches, "upstream_archive_matches_pin": upstream_matches,
              "limit": "Declared Nix input bytes, not an independent rebuild or compiler/toolchain audit"}
    result["result"] = "passed" if all(r["match"] for r in rows) and patch_set_matches and upstream_matches else "failed"
    print(json.dumps(result, indent=2))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
