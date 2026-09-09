#!/usr/bin/env python3
"""Materialize the portable orchestrator prompt without replacing local notes."""
from pathlib import Path

root = Path(__file__).resolve().parent.parent
source = root / "docs" / "orchestrator.md"
directory = root / "tmp"
if directory.is_symlink():
    raise SystemExit("Refusing a symlinked tmp directory")
directory.mkdir(exist_ok=True)
destination = directory / "orchestrator.md"
try:
    with destination.open("x", encoding="utf-8") as output:
        output.write(source.read_text(encoding="utf-8"))
except FileExistsError:
    raise SystemExit("tmp/orchestrator.md already exists; preserved without changes")
print("Created tmp/orchestrator.md; ask Claude to read it and begin.")
