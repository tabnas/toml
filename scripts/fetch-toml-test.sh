#!/usr/bin/env bash
#
# Fetch the authoritative third-party TOML conformance corpus.
#
#   upstream: https://github.com/BurntSushi/toml-test
#   pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
#
# The corpus is NEVER committed to this repository (project rule: no
# vendored third-party test corpora). It is cloned into a gitignored
# directory (see .gitignore: `ts/test/toml-test/`) and pinned to an
# exact commit SHA so the conformance numbers are reproducible.
#
# Idempotent: safe to re-run. If the checkout already exists and is at
# the pinned SHA it does nothing; otherwise it fetches and hard-resets
# to the pin.
#
# Both test suites call this automatically when the corpus is missing,
# and FAIL LOUDLY if it still is not there afterwards. Neither suite is
# allowed to skip: a conformance test that quietly does not run is worse
# than no test at all.

set -euo pipefail

REPO_URL="https://github.com/BurntSushi/toml-test.git"
PIN="9eef1b959e0449d41a31d4e4e0a839faee534b36"

# Only the fixture corpus and its licence are checked out. Deliberately
# SPARSE: upstream toml-test is itself a Go module, and a `go.mod` under
# this tree gets swept into the generated repo-root `go.work` by
# admin/scripts/link.sh, wiring a third-party module into every sibling's
# Go workspace (and breaking `go build` outright whenever the corpus is
# absent). Checking out only `tests/` keeps the workspace clean and lets
# the Go conformance test re-fetch the corpus on demand.
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
  git -C "$DEST" reset --quiet --hard "$PIN"
  git -C "$DEST" clean -qfd
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

# Sanity: the two halves we assert against must both be present.
for half in valid invalid; do
  if [ ! -d "$DEST/tests/$half" ]; then
    echo "toml-test: FATAL - $DEST/tests/$half missing after fetch" >&2
    exit 1
  fi
done

echo "toml-test: ready at $(git -C "$DEST" rev-parse HEAD)"
