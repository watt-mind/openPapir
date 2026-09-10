#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo machete
cargo deny --all-features check
python3 scripts/check-doc-links.py
python3 scripts/check-file-length.py
python3 scripts/check-prose.py
python3 scripts/release-notes.py --self-test
python3 scripts/release-notes.py Unreleased > /dev/null
npx --yes markdownlint-cli2@0.18.1 "**/*.md" "#target" "#samples" "#refs" "#tmp" "#node_modules"
actionlint
