#!/usr/bin/env bash
#
# Fetch the authoritative third-party TOML conformance corpus.
#
#   upstream: https://github.com/BurntSushi/toml-test
#   pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
#
# The corpus is NEVER committed to this repository (project rule: no
# vendored third-party test corpora). It is cloned into a gitignored
# directory (see .gitignore: `ts/test/toml-test/`) and pinned to an exact
# commit SHA. A git commit SHA is a content hash over the whole tree, and
# git verifies every object it writes against it, so the pin IS the
# integrity check: there is no unverified tarball anywhere in this path.
# Conformance numbers are therefore reproducible, and upstream cannot
# move the corpus under us.
#
# Idempotent: safe to re-run. If the checkout already exists at the pinned
# SHA with both halves present it does nothing.
#
# Both test suites run this automatically when the corpus is missing, and
# FAIL LOUDLY if it still is not there afterwards. Neither suite is
# allowed to skip: a conformance suite that quietly does not run reports a
# green tick that is a lie.

set -euo pipefail

REPO_URL="https://github.com/BurntSushi/toml-test.git"
PIN="9eef1b959e0449d41a31d4e4e0a839faee534b36"

# Only the fixture corpus and its licence are checked out. Deliberately
# SPARSE: upstream toml-test is itself a Go module, and a `go.mod` under
# this tree gets swept into the generated repo-root `go.work` by
# admin/scripts/link.sh (and by the CI `go work use` loop), wiring a
# third-party module into every sibling's Go workspace. Checking out only
# `tests/` keeps the workspace clean.
SPARSE_PATHS=(tests /LICENSE /README.md)

# Repo root = parent of this script's directory.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/ts/test/toml-test"

if [ -d "$DEST/.git" ]; then
  current="$(git -C "$DEST" rev-parse HEAD 2>/dev/null || echo none)"
  if [ "$current" = "$PIN" ] && [ -d "$DEST/tests/valid" ] && [ -d "$DEST/tests/invalid" ]; then
    echo "toml-test: already at pinned commit $PIN"
    exit 0
  fi
  echo "toml-test: at $current, moving to pinned $PIN"
  git -C "$DEST" fetch --quiet origin "$PIN" || git -C "$DEST" fetch --quiet origin
  git -C "$DEST" sparse-checkout set --no-cone "${SPARSE_PATHS[@]}"
  git -C "$DEST" checkout --quiet --force "$PIN"
else
  if [ -e "$DEST" ]; then
    echo "toml-test: $DEST exists but is not a git checkout; removing"
    rm -rf "$DEST"
  fi
  echo "toml-test: cloning $REPO_URL into $DEST"
  mkdir -p "$(dirname "$DEST")"
  git clone --quiet --filter=blob:none --sparse "$REPO_URL" "$DEST"
  git -C "$DEST" sparse-checkout set --no-cone "${SPARSE_PATHS[@]}"
  git -C "$DEST" checkout --quiet --force "$PIN"
fi

# The upstream repo is a Go module; a stray go.mod here would be swept
# into the generated repo-root go.work. Belt-and-braces.
rm -f "$DEST/go.mod" "$DEST/go.sum"

# The commit we ended up on must be the pin, and both asserted halves must
# be present. Anything else is a broken fetch, not a usable corpus.
got="$(git -C "$DEST" rev-parse HEAD)"
if [ "$got" != "$PIN" ]; then
  echo "toml-test: FATAL - checkout is at $got, expected pinned $PIN" >&2
  exit 1
fi
for half in valid invalid; do
  if [ ! -d "$DEST/tests/$half" ]; then
    echo "toml-test: FATAL - $DEST/tests/$half missing after fetch" >&2
    exit 1
  fi
done

echo "toml-test: ready at $got"
