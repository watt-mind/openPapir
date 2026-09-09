#!/usr/bin/env python3
"""Enforce source size limits on tracked Rust files without reading private data."""
from pathlib import Path
import subprocess
import sys

paths = subprocess.check_output(["git", "ls-files", "-z", "crates"], text=True).split("\0")
failures = []
for name in filter(None, paths):
    path = Path(name)
    if path.suffix != ".rs":
        continue
    limit = 1500 if "tests" in path.parts else 800
    count = len(path.read_text().splitlines())
    if count > limit:
        failures.append(f"{name}: {count} lines exceeds {limit}")
print("\n".join(failures) if failures else "Source file lengths checked")
sys.exit(bool(failures))
