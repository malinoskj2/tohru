#!/usr/bin/env python3
"""Read-only comparison of a Nix runtime's declared inputs with this checkout."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def query(*argv):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise ValueError(f"{' '.join(argv)} exited {result.returncode}: {result.stderr.strip()[:1000]}")
    return result.stdout


def foundation_files(root):
    # These selectors mirror foundationSource in nix/default.nix. Only runtime/src
    # is extension-filtered; tests and bindings include every regular file.
    files = {Path(area) for area in ("runtime/Cargo.toml", "runtime/Cargo.lock",
             "runtime/pleamar/tests/role-install.rs", "LICENSE")}

    def unreadable(error):
        raise error

    for area in ("runtime/src", "runtime/tests", "runtime/pleamar/bindings"):
        directory = root / area
        if not directory.is_dir():
            raise ValueError(f"Required derivation selector directory unavailable: {directory}")
        for parent, _, names in os.walk(directory, onerror=unreadable):
            for name in names:
                file = Path(parent) / name
                if file.is_file() and (area != "runtime/src" or file.suffix == ".rs"):
                    files.add(file.relative_to(root))
    return files


def compare_foundation(checkout, source):
    rows = []
    for member in sorted(foundation_files(checkout) | foundation_files(source)):
        local, declared = checkout / member, source / member
        if not local.is_file():
            difference = "missing from checkout"
        elif not declared.is_file():
            difference = "missing from declared inputs"
        elif local.read_bytes() != declared.read_bytes():
            difference = "content differs"
        else:
            difference = None
        rows.append({"source": str(member), "store": str(declared),
                     "match": difference is None, "difference": difference})
    return rows


def metadata_path(pattern, text, label):
    match = re.search(pattern, text)
    if not match:
        raise ValueError(f"Cannot identify {label} in derivation metadata; this helper supports nix/default.nix's runtime layout")
    return Path(match[1])


def inspect_runtime(runtime):
    if not runtime.is_file():
        raise ValueError(f"Runtime executable unavailable: {runtime}")
    drv = query("nix-store", "-q", "--deriver", str(runtime.parents[1])).strip()
    raw = json.loads(query("nix", "derivation", "show", drv))
    env = next(iter(raw.get("derivations", raw).values()))["env"]
    source = metadata_path(r"(/nix/store/[^ /]+-source)", env["postPatch"], "foundation source")
    rows = compare_foundation(ROOT, source)
    pairs = []
    patches = env["patches"].split()
    for patch in patches:
        pairs.append((ROOT / "runtime/pleamar/patches" / Path(patch).name[33:], Path(patch)))
    lock = metadata_path(r"cp (/nix/store/[^ ]+-Cargo.lock) Cargo.lock", env["postPatch"], "replacement Cargo lock")
    pairs.append((ROOT / "runtime/pleamar/Cargo.lock", lock))
    rows += [{"source": str(a.relative_to(ROOT)), "store": str(b),
              "match": a.is_file() and b.is_file() and a.read_bytes() == b.read_bytes()} for a, b in pairs]
    expected_patches = {p.name for p in (ROOT / "runtime/pleamar/patches").glob("*.patch")}
    patch_set_matches = expected_patches == {Path(p).name[33:] for p in patches}
    pin = json.loads((ROOT / "runtime/pleamar/pin.json").read_text())
    upstream_matches = "sha256-" + base64.b64encode(hashlib.sha256(Path(env["src"]).read_bytes()).digest()).decode() == pin["hash"]
    result = {"runtime": str(runtime), "deriver": drv, "files": rows,
              "patch_set_matches": patch_set_matches, "upstream_archive_matches_pin": upstream_matches,
              "limit": "Declared Nix input bytes, not an independent rebuild or compiler/toolchain audit"}
    result["result"] = "passed" if all(r["match"] for r in rows) and patch_set_matches and upstream_matches else "failed"
    result["diagnosis"] = []
    mismatches = [row for row in rows if not row["match"]]
    for row in mismatches:
        result["diagnosis"].append(f"{row['source']}: {row.get('difference', 'missing file or content differs')} (declared input {row['store']})")
    if not patch_set_matches:
        result["diagnosis"].append("Patch filenames differ between checkout and derivation")
    if not upstream_matches:
        result["diagnosis"].append("Declared upstream archive does not match runtime/pleamar/pin.json")
    if result["result"] == "failed":
        result["diagnosis"].append("Select a runtime with matching inputs or rebuild it from this checkout before claiming source provenance")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve()
    try:
        result = inspect_runtime(runtime)
    except (OSError, ValueError, KeyError, TypeError, IndexError, StopIteration, subprocess.TimeoutExpired) as error:
        result = {"runtime": str(runtime), "result": "unavailable", "diagnosis": [str(error),
                  "Supply an existing Nix-built runtime and make its derivation/input paths readable with nix and nix-store on PATH; rerun this helper without claiming a match"]}
    print(json.dumps(result, indent=2))
    return {"passed": 0, "failed": 1, "unavailable": 2}[result["result"]]


if __name__ == "__main__":
    sys.exit(main())
