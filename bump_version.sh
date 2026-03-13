#!/bin/bash

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

cd rust &&

cargo fmt --all -- --check

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

if [ ! -z "$(git status --porcelain)" ]; then
  cd -
  echo "Commit changes before running bumping version"
  exit 1;
fi

cargo bump $LEVEL &&

while [ $? == 0 ]; do
  sleep 2;
  git diff --exit-code --name-only Cargo.lock
done;

cd - &&

git add --all

bump2version $LEVEL --allow-dirty

NEW_VERSION=$(grep current_version .bumpversion.cfg | head -n 1 | cut -d' ' -f 3)

git commit -a -m "Bump version: ${CURRENT_VERSION} → ${NEW_VERSION}"
git tag -a $NEW_VERSION -m "Bump version: ${CURRENT_VERSION} → ${NEW_VERSION}"
