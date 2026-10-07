#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_DIR="$ROOT_DIR/rust"
TARGET_ENV="${TARGET_ENV:-${CONDA_DEFAULT_ENV:-blobtk}}"
PYTHON_BIN="${PYTHON_BIN:-}"
TEST_SCRIPT="${TEST_SCRIPT:-$RUST_DIR/test/depth.py}"

# Clear the build/linker state that frequently survives between macOS rebuilds.
unset RUSTFLAGS
unset CARGO_BUILD_TARGET
unset MACOSX_DEPLOYMENT_TARGET
unset LDFLAGS
unset CPPFLAGS

TARGET_PREFIX=""
if command -v conda >/dev/null 2>&1; then
  active_env="${CONDA_DEFAULT_ENV:-}"
  if [[ -n "$active_env" && "$active_env" != "base" && "$TARGET_ENV" == "$active_env" && -n "${CONDA_PREFIX:-}" ]]; then
    TARGET_PREFIX="${CONDA_PREFIX}"
    PYTHON_BIN="${TARGET_PREFIX}/bin/python"
    CONDA_RUN=()
    echo "Using active conda environment: $TARGET_ENV ($PYTHON_BIN)"
  elif conda env list 2>/dev/null | awk '{print $1}' | grep -Fxq "$TARGET_ENV"; then
    TARGET_PREFIX="$(conda info --base 2>/dev/null)/envs/$TARGET_ENV"
    PYTHON_BIN="${TARGET_PREFIX}/bin/python"
    CONDA_RUN=(conda run -n "$TARGET_ENV")
    echo "Using conda environment: $TARGET_ENV ($PYTHON_BIN)"
  else
    echo "Conda env '$TARGET_ENV' not found; using system Python."
    CONDA_RUN=()
  fi
fi

if [[ -n "$TARGET_PREFIX" ]]; then
  export TARGET_PREFIX
fi

if [[ -n "${PYTHON_BIN}" && ! -x "${PYTHON_BIN}" ]]; then
  echo "Python interpreter not found at ${PYTHON_BIN}; falling back to the system Python."
  PYTHON_BIN=""
fi

if [[ -z "${PYTHON_BIN}" ]]; then
  PYTHON_BIN="$(command -v python || command -v python3 || true)"
fi

if [[ -z "${PYTHON_BIN}" ]]; then
  echo "No Python interpreter found. Set PYTHON_BIN or activate the target conda environment." >&2
  exit 1
fi

if [[ -z "${CARGO_BIN:-}" ]]; then
  CARGO_BIN="$(bash "$ROOT_DIR/scripts/run-local-rust.sh" --print-cargo-bin 2>/dev/null || true)"
fi

if [[ -z "${CARGO_BIN:-}" ]]; then
  echo "Could not resolve a local Rust toolchain. Set CARGO_BIN or ensure ~/.rustup/toolchains contains the repo-pinned toolchain." >&2
  exit 1
fi

run_in_env() {
  if [[ ${#CONDA_RUN[@]} -gt 0 ]]; then
    "${CONDA_RUN[@]}" "$@"
  else
    "$@"
  fi
}

echo "==> Cleaning stale artifacts"
cleanup_path() {
  local path="$1"
  [[ -z "$path" ]] && return 0
  [[ ! -e "$path" ]] && return 0

  echo "Removing: $path"
  chmod -R u+rwX -- "$path" 2>/dev/null || true
  rm -rf -- "$path" 2>/dev/null || true

  if [[ -e "$path" ]]; then
    find "$path" -mindepth 1 -exec chmod -R u+rwX -- {} + 2>/dev/null || true
    find "$path" -mindepth 1 -exec rm -rf -- {} + 2>/dev/null || true
    rm -rf -- "$path" 2>/dev/null || true
  fi
}

# Remove all Rust build outputs in the repo before rebuilding.
for stale_path in \
  "$RUST_DIR/target" \
  "$RUST_DIR/target/debug" \
  "$RUST_DIR/target/release" \
  "$RUST_DIR/target/wheels" \
  "$ROOT_DIR/target" \
  "$ROOT_DIR/target/debug" \
  "$ROOT_DIR/target/release" \
  "$ROOT_DIR/target/wheels"; do
  cleanup_path "$stale_path"
done

# Remove stale Python package artifacts only from the selected target environment's
# site-packages so we do not damage the interpreter path or unrelated conda envs.
PYTHON_SITE_CLEANUP="$($PYTHON_BIN - <<'PY'
import os
import site
import sys
import sysconfig

roots = set()
for base in list(site.getsitepackages()) + [
    sysconfig.get_paths().get('platlib'),
    sysconfig.get_paths().get('purelib'),
    sys.prefix,
    os.path.join(sys.prefix, 'lib'),
]:
    if base and os.path.isdir(base):
        roots.add(base)

prefix = os.environ.get('CONDA_PREFIX') or os.environ.get('TARGET_PREFIX')
if prefix:
    roots.add(os.path.join(prefix, 'lib'))
    for py_dir in sorted(os.listdir(os.path.join(prefix, 'lib'))) if os.path.isdir(os.path.join(prefix, 'lib')) else []:
        if py_dir.startswith('python'):
            site_path = os.path.join(prefix, 'lib', py_dir, 'site-packages')
            if os.path.isdir(site_path):
                roots.add(site_path)

for base in sorted(roots):
    if base and '/site-packages' in base:
        print(base)
PY
)"

while IFS= read -r site_base; do
  [[ -z "$site_base" ]] && continue
  [[ "$site_base" != *"/site-packages" ]] && continue
  for stale in \
    'blobtk' \
    'blobtk-*' \
    'blobtk-*.dist-info' \
    '_blobtk*.so' \
    '_blobtk*.dylib'; do
    find "$site_base" -maxdepth 1 -name "$stale" -exec chmod -R u+rwX -- {} + 2>/dev/null || true
    find "$site_base" -maxdepth 1 -name "$stale" -exec rm -rf -- {} + 2>/dev/null || true
  done
  find "$site_base" -maxdepth 1 -type d -name 'blobtk*' -exec chmod -R u+rwX -- {} + 2>/dev/null || true
  find "$site_base" -maxdepth 1 -type d -name 'blobtk*' -exec rm -rf -- {} + 2>/dev/null || true
  find "$site_base" -maxdepth 1 -type d -name 'blobtk-*.dist-info' -exec chmod -R u+rwX -- {} + 2>/dev/null || true
  find "$site_base" -maxdepth 1 -type d -name 'blobtk-*.dist-info' -exec rm -rf -- {} + 2>/dev/null || true
  find "$site_base" -maxdepth 1 -type f \( -name '_blobtk*.so' -o -name '_blobtk*.dylib' \) -delete 2>/dev/null || true
done <<< "$PYTHON_SITE_CLEANUP"

mkdir -p "$RUST_DIR/target/wheels"

export CARGO_BIN
export PATH="$(dirname "$CARGO_BIN"):${PATH:-}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

cd "$RUST_DIR"
PROJECT_PYTHONPATH="$RUST_DIR${PYTHONPATH:+:$PYTHONPATH}"

echo "==> Building release wheel"
if [[ ${#CONDA_RUN[@]} -gt 0 ]]; then
  "${CONDA_RUN[@]}" env -u RUSTFLAGS -u CARGO_BUILD_TARGET -u MACOSX_DEPLOYMENT_TARGET -u LDFLAGS -u CPPFLAGS python -m maturin build --release -i python
else
  env -u RUSTFLAGS -u CARGO_BUILD_TARGET -u MACOSX_DEPLOYMENT_TARGET -u LDFLAGS -u CPPFLAGS \
    "$PYTHON_BIN" -m maturin build --release -i "$PYTHON_BIN"
fi

echo "==> Reinstalling clean wheel"
run_in_env python -m pip uninstall -y blobtk >/dev/null 2>&1 || true
if [[ ${#CONDA_RUN[@]} -gt 0 ]]; then
  "${CONDA_RUN[@]}" python -m pip install --force-reinstall --no-deps ./target/wheels/blobtk-*.whl
else
  "$PYTHON_BIN" -m pip install --force-reinstall --no-deps ./target/wheels/blobtk-*.whl
fi

echo "==> Verifying import"
if [[ ${#CONDA_RUN[@]} -gt 0 ]]; then
  "${CONDA_RUN[@]}" env PYTHONPATH="$PROJECT_PYTHONPATH" python -c "import blobtk; print(f'blobtk import: {blobtk.__file__}')"
else
  PYTHONPATH="$PROJECT_PYTHONPATH" "$PYTHON_BIN" -c "import blobtk; print(f'blobtk import: {blobtk.__file__}')"
fi

if [[ ! -f "$TEST_SCRIPT" ]]; then
  echo "Test script not found: $TEST_SCRIPT" >&2
  exit 1
fi

echo "==> Running depth regression"
if [[ ${#CONDA_RUN[@]} -gt 0 ]]; then
  "${CONDA_RUN[@]}" env PYTHONPATH="$PROJECT_PYTHONPATH" python "$TEST_SCRIPT"
else
  PYTHONPATH="$PROJECT_PYTHONPATH" "$PYTHON_BIN" "$TEST_SCRIPT"
fi

echo "==> Clean rebuild and import check passed"
