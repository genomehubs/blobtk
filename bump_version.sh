#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_SCRIPT="$ROOT_DIR/scripts/run-local-rust.sh"

if [[ -x "$RUST_SCRIPT" ]]; then
  export CARGO_BIN="$($RUST_SCRIPT --print-cargo-bin)"
  export PATH="$(dirname "$CARGO_BIN"):${PATH:-}"
  export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
  export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
  echo "Using Rust toolchain: $($CARGO_BIN --version)"
else
  echo "Missing local Rust wrapper script: $RUST_SCRIPT" >&2
  exit 1
fi

LEVEL=$1

if [ -z "$LEVEL" ]; then
  echo "Usage: ./bump_version.sh major|minor|patch"
  exit 1;
fi

CURRENT_VERSION=$(grep current_version .bumpversion.cfg | head -n 1 | cut -d' ' -f 3)
CARGO_VERSION=$(grep '^version = "' rust/Cargo.toml | head -n 1 | cut -d '"' -f 2)

if [ "$CURRENT_VERSION" != "$CARGO_VERSION" ]; then
  echo "Version mismatch before bump:"
  echo "  .bumpversion.cfg: $CURRENT_VERSION"
  echo "  rust/Cargo.toml: $CARGO_VERSION"
  echo "Sync versions before running bump_version.sh"
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is dirty. Commit or stash changes before running bump_version.sh."
  git status --short
  exit 1
fi

cd rust &&
export PYTHONPATH="$ROOT_DIR/rust${PYTHONPATH:+:$PYTHONPATH}"

"$CARGO_BIN" fmt --all -- --check

if [ $? != "0" ]; then
  cd -
  echo "failed cargo fmt check"
  exit 1
fi

./test/integration.sh

if [ $? != "0" ]; then
  cd -
  echo "failed integration tests"
  exit 1
fi

echo "Passed all tests"
echo

if [ "$LEVEL" == "test" ]; then
  cd -
  exit
fi

"$CARGO_BIN" bump "$LEVEL"

cd - &&

git add --all

bump2version $LEVEL --allow-dirty

NEW_VERSION=$(grep current_version .bumpversion.cfg | head -n 1 | cut -d' ' -f 3)

git commit -a -m "Bump version: ${CURRENT_VERSION} → ${NEW_VERSION}"
git tag -a $NEW_VERSION -m "Bump version: ${CURRENT_VERSION} → ${NEW_VERSION}"
