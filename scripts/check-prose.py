#!/usr/bin/env python3
"""Check tracked Markdown prose style rules.

Enforces the "no em-dashes" rule from CONTRIBUTING.md: U+2014 is rejected
outside fenced code blocks (``` or ~~~). Fenced blocks hold captured output
and verbatim examples, which must stay exactly as produced.

No third-party dependencies; the file list comes from git, so untracked and
ignored files are never read.
"""
import subprocess
import sys

SKIP = ("target/", "tmp/", "node_modules/", ".claude/", ".cursor/")
EM_DASH = "—"


def tracked_markdown():
    """Return tracked Markdown paths outside the skipped directories."""
    output = subprocess.check_output(["git", "ls-files", "-z", "*.md"], text=True)
    return sorted(
        path
        for path in output.split("\0")
        if path and not path.startswith(SKIP)
    )


def check(path):
    """Return (line number, line) for every em-dash outside a code fence."""
    problems = []
    fence = None
    with open(path, encoding="utf-8") as handle:
        for lineno, line in enumerate(handle, start=1):
            stripped = line.strip()
            if stripped.startswith(("```", "~~~")):
                marker = stripped[:3]
                if fence is None:
                    fence = marker
                elif stripped.startswith(fence):
                    fence = None
                continue
            if fence is not None:
                continue
            if EM_DASH in line:
                problems.append((lineno, line.rstrip()))
    return problems


def main():
    problems = 0
    for path in tracked_markdown():
        for lineno, line in check(path):
            print(f"{path}:{lineno}: em-dash (U+2014) outside a code fence: {line}")
            problems += 1
    print(f"prose checked, problems={problems}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
