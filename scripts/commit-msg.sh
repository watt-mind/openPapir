#!/bin/sh
set -eu
input="${1:-.git/COMMIT_EDITMSG}"
if [ "$input" = "-" ]; then
  IFS= read -r subject || true
else
  IFS= read -r subject < "$input" || true
fi
if printf '%s\n' "${subject:-}" | grep -Eq '^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z][a-z0-9-]*\))?!?: [a-z].*[^.]$'; then
  exit 0
fi
echo 'Use a Conventional Commit subject, e.g. feat(core): inspect package metadata' >&2
exit 1
