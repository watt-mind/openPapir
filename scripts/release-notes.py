#!/usr/bin/env python3
"""Print the CHANGELOG.md section for one version, verbatim.

The release workflow uses this to build the notes of a draft release, and its
dry run uses it to exercise the same extraction against the `Unreleased`
section, so a section this cannot read fails a dry run instead of a tag run.

A section runs from its `## ` heading to the next `## ` heading outside a code
fence. Headings inside a fenced block (``` or ~~~) are content, not headings:
the fence rules here are the ones scripts/check-prose.py applies, so both
Markdown checks read the file the same way.

The section body is copied without interpretation. No third-party
dependencies, and no file is read except the changelog named on the command
line.
"""
import argparse
import sys

FENCES = ("```", "~~~")


def sections(lines):
    """Yield (heading text, body lines) for every heading outside a fence."""
    fence = None
    heading = None
    body = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(FENCES):
            marker = stripped[:3]
            if fence is None:
                fence = marker
            elif stripped.startswith(fence):
                fence = None
        elif fence is None and line.startswith("## "):
            if heading is not None:
                yield heading, body
            heading = line[3:].strip()
            body = []
            continue
        if heading is not None:
            body.append(line)
    if heading is not None:
        yield heading, body


def names(heading, version):
    """Return whether a heading names this version.

    `## 1.2.0`, `## [1.2.0]`, and either of those followed by a date or any
    other trailing text all name version `1.2.0`. A heading that merely starts
    with the version, such as `## 1.2.0-rc.1`, does not.
    """
    rest = heading[1:] if heading.startswith("[") else heading
    if not rest.startswith(version):
        return False
    rest = rest[len(version):]
    return rest == "" or rest[:1] in (" ", "]")


def extract(text, version):
    """Return the body of the section for this version, or None."""
    for heading, body in sections(text.splitlines()):
        if names(heading, version):
            return "\n".join(body).strip("\n")
    return None


def select(text, version, allow_empty=False):
    """Return (notes, problem) for this version.

    `problem` is `missing`, `empty`, or None. With `allow_empty` neither is a
    problem and the notes come back empty, which is what `Unreleased`
    legitimately holds between a changelog cut and the next entry.
    """
    notes = extract(text, version)
    if notes:
        return notes, None
    if allow_empty:
        return "", None
    return "", "missing" if notes is None else "empty"


CASES = (
    # A fenced heading is content: it neither opens nor closes a section.
    ("## 1.0.0\nkept\n```\n## not a heading\n```\nalso kept\n## 0.9.0\ngone\n",
     "1.0.0", "kept\n```\n## not a heading\n```\nalso kept"),
    # A tilde fence behaves the same, and a fence of the other kind inside it
    # is content too.
    ("## 1.0.0\n~~~\n```\n## inner\n~~~\nend\n", "1.0.0", "~~~\n```\n## inner\n~~~\nend"),
    # An unterminated fence swallows the rest of the file rather than letting
    # a fenced heading cut the section short.
    ("## 1.0.0\n```\n## 0.9.0\n", "1.0.0", "```\n## 0.9.0"),
    # A bracketed heading with a date, and the section stops at the next real
    # heading.
    ("## [1.0.0] - 2026-01-01\nnotes\n\n## [0.9.0] - 2025-12-01\nold\n",
     "1.0.0", "notes"),
    # The Unreleased section the dry run reads.
    ("## Unreleased\n\n### Added\n\n- a thing\n\n## [0.9.0]\nold\n",
     "Unreleased", "### Added\n\n- a thing"),
    # A prefix match is not a match, and a missing section is None.
    ("## 1.0.0-rc.1\nnotes\n", "1.0.0", None),
    ("## 1.0.0\nnotes\n", "2.0.0", None),
    # A heading before any section is not part of one.
    ("intro\n## 1.0.0\nnotes\n", "1.0.0", "notes"),
    # An empty section is empty, not missing; the caller decides.
    ("## 1.0.0\n\n## 0.9.0\nold\n", "1.0.0", ""),
)

CUT = "## Unreleased\n\n## [1.0.0] - 2026-01-01\nnotes\n"

SELECT_CASES = (
    # Right after a changelog cut the Unreleased section holds nothing, and a
    # dry run reads it with allow_empty, so an empty one is not a problem.
    (CUT, "Unreleased", True, ("", None)),
    (CUT, "Unreleased", False, ("", "empty")),
    # A section that is not there at all is equally tolerated under the flag.
    ("## [1.0.0]\nnotes\n", "Unreleased", True, ("", None)),
    ("## [1.0.0]\nnotes\n", "Unreleased", False, ("", "missing")),
    # A tagged run reads a version section without the flag and refuses both.
    (CUT, "1.0.0", False, ("notes", None)),
    ("## [1.0.0]\n\n## [0.9.0]\nold\n", "1.0.0", False, ("", "empty")),
    ("## [0.9.0]\nold\n", "1.0.0", False, ("", "missing")),
    # The flag changes nothing when the section has content.
    (CUT, "1.0.0", True, ("notes", None)),
)


def self_test():
    """Check the extraction and the selection against the cases above."""
    failures = 0
    for text, version, expected in CASES:
        actual = extract(text, version)
        if actual != expected:
            print(f"case {version}: expected {expected!r}, got {actual!r}")
            failures += 1
    for text, version, allow_empty, expected in SELECT_CASES:
        actual = select(text, version, allow_empty)
        if actual != expected:
            print(
                f"case {version} allow_empty={allow_empty}: "
                f"expected {expected!r}, got {actual!r}"
            )
            failures += 1
    print(f"release notes extraction checked, problems={failures}")
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", nargs="?", help="version, or Unreleased")
    parser.add_argument("--changelog", default="CHANGELOG.md")
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="print nothing instead of failing when the section is missing "
        "or empty, which is what Unreleased holds after a changelog cut",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="check the extraction against its own cases and exit",
    )
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.version:
        parser.error("a version is required unless --self-test is given")
    with open(args.changelog, encoding="utf-8") as handle:
        notes, problem = select(handle.read(), args.version, args.allow_empty)
    if problem == "missing":
        print(f"no {args.changelog} section for {args.version}", file=sys.stderr)
        return 1
    if problem == "empty":
        print(f"the {args.changelog} section for {args.version} is empty", file=sys.stderr)
        return 1
    if notes:
        print(notes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
