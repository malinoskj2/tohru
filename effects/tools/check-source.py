#!/usr/bin/env python3
"""Check extraction provenance and scope without a compositor or source checkout."""
from pathlib import Path
import hashlib
import json

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / "source-manifest.json").read_text())
for name, expected in manifest["files_sha256"].items():
    data = (root / name).read_bytes()
    if name == "bars/main.cpp":
        data = data.replace(b"    if (!g_pGlobalState)\n        return;\n", b"", 1)
    assert hashlib.sha256(data).hexdigest() == expected, f"unrecorded source change: {name}"

sources = ["main.cpp", "focus/main.cpp", "bars/main.cpp", "bars/barDeco.cpp",
           "bars/BarPassElement.cpp", "sheet.cpp", "recede.cpp"]
makefile = (root / "Makefile").read_text()
assert "SOURCES = " + " ".join(sources) in makefile
for name in sources:
    assert (root / name).is_file(), name
assert not (root / "glass").exists()
assert not (root / "panels.cpp").exists()
assert not (root / "effects.cpp").exists()
print("PASS: copied-source hashes, declared extraction adjustment, selected build sources and exclusions")
