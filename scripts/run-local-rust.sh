#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_DIR="$ROOT_DIR/rust"

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$HOME/.cargo/bin:${PATH:-}"

resolve_pinned_toolchain() {
  local toolchain_file="$RUST_DIR/rust-toolchain.toml"
  local channel=""
  local arch="$(uname -m)"
  local os="$(uname -s | tr '[:upper:]' '[:lower:]')"

  if [[ -f "$toolchain_file" ]]; then
    channel="$(sed -nE 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"?([^"[:space:]]+)"?[[:space:]]*$/\1/p' "$toolchain_file" | head -n 1)"
  fi

  case "$arch" in
    arm64|aarch64)
      arch="aarch64"
      ;;
    x86_64|amd64)
      arch="x86_64"
      ;;
    *)
      arch="$(uname -m)"
      ;;
  esac

  case "$os" in
    darwin)
      os="apple-darwin"
      ;;
    linux)
      os="unknown-linux-gnu"
      ;;
    msys_nt*|mingw*|windows*)
      os="pc-windows-msvc"
      ;;
  esac

  if [[ -n "$channel" ]]; then
    local candidate="$HOME/.rustup/toolchains/${channel}-${arch}-${os}/bin/cargo"
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi

    local direct_candidate="$HOME/.rustup/toolchains/${channel}/bin/cargo"
    if [[ -x "$direct_candidate" ]]; then
      printf '%s\n' "$direct_candidate"
      return 0
    fi
  fi

  return 1
}

if [[ -n "${CARGO_BIN:-}" ]]; then
  :
elif CARGO_BIN="$(resolve_pinned_toolchain)"; then
  :
elif compgen -G "$HOME/.rustup/toolchains/*/bin/cargo" >/dev/null; then
  CARGO_BIN="$(compgen -G "$HOME/.rustup/toolchains/*/bin/cargo" | sort | tail -n 1)"
elif command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  CARGO_BIN="$HOME/.cargo/bin/cargo"
else
  echo "Local cargo not found in ~/.rustup/toolchains, on PATH, or at $HOME/.cargo/bin/cargo" >&2
  echo "Set CARGO_BIN=/path/to/cargo in your environment and retry." >&2
  exit 1
fi

if [[ "${1:-}" == "--print-cargo-bin" ]]; then
  printf '%s\n' "$CARGO_BIN"
  exit 0
fi

export PATH="$(dirname "$CARGO_BIN"):${PATH}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$RUST_DIR/target}"

cd "$RUST_DIR"
exec "$CARGO_BIN" "$@"
