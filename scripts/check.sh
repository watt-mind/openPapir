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
npx --yes markdownlint-cli2@0.18.1 "**/*.md" "#target" "#samples" "#refs" "#tmp" "#node_modules"
actionlint
