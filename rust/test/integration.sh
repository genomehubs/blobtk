#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_SCRIPT="$ROOT_DIR/../scripts/run-local-rust.sh"
PYTHON_BIN="${PYTHON_BIN:-$(command -v python || command -v python3 || true)}"

if [[ -z "$PYTHON_BIN" ]]; then
  echo "No Python interpreter found on PATH; set PYTHON_BIN explicitly." >&2
  exit 1
fi

if [[ -x "$RUST_SCRIPT" ]]; then
  export CARGO_BIN="$($RUST_SCRIPT --print-cargo-bin)"
  export PATH="$(dirname "$CARGO_BIN"):${PATH:-}"
  export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
  export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
  echo "Using Rust toolchain: $($CARGO_BIN --version)"
fi

export PYTHONPATH="$ROOT_DIR${PYTHONPATH:+:$PYTHONPATH}"
export PYO3_PYTHON="$PYTHON_BIN"
echo "Using Python interpreter: $PYTHON_BIN ($($PYTHON_BIN -c 'import sys; print(sys.version)'))"

echo "Runnning integration tests"

# CMD="cargo build --release"
# printf "\nrunning command\n$CMD\n\n"
# $CMD || exit 1

# CMD="./target/release/blobtk depth -b test/test.bam -O test/test.bed"
CMD="$CARGO_BIN run -- depth -b test/test.bam -O test/test.bed"
printf "\nrunning command\n$CMD\n\n"
$CMD || exit 1

#CMD="./target/release/blobtk depth -b test/test.bam -s 1000 -O test/test.1000.bed"
CMD="$CARGO_BIN run -- depth -b test/test.bam -s 1000 -O test/test.1000.bed"
printf "\n\nrunning command\n$CMD\n\n"
$CMD || exit 1

#CMD="./target/release/blobtk filter -i test/test.list -b test/test.bam -f test/reads_1.fq.gz -r test/reads_2.fq.gz -F"
CMD="$CARGO_BIN run -- filter -i test/test.list -b test/test.bam -f test/reads_1.fq.gz -r test/reads_2.fq.gz -F"
printf "\n\nrunning command\n$CMD\n\n"
$CMD || exit 1

CMD="rm -f ./target/wheels/blobtk-*.whl && 
    maturin build --release -i \"$PYTHON_BIN\" &&
    $PYTHON_BIN -m pip uninstall -y blobtk || true &&
    $PYTHON_BIN -m pip install --force-reinstall ./target/wheels/blobtk-*.whl"
printf "\nrunning command\n$CMD\n\n"
rm -f ./target/wheels/blobtk-*.whl &&
    maturin build --release -i "$PYTHON_BIN" &&
    "$PYTHON_BIN" -m pip uninstall -y blobtk >/dev/null 2>&1 || true &&
    "$PYTHON_BIN" -m pip install --force-reinstall ./target/wheels/blobtk-*.whl || exit 1

CMD="PYTHONPATH=\"$ROOT_DIR${PYTHONPATH:+:$PYTHONPATH}\" $PYTHON_BIN ./test/depth.py"
printf "\n\nrunning command\n$CMD\n\n"
PYTHONPATH="$ROOT_DIR${PYTHONPATH:+:$PYTHONPATH}" "$PYTHON_BIN" ./test/depth.py || exit 1

CMD="PYTHONPATH=\"$ROOT_DIR${PYTHONPATH:+:$PYTHONPATH}\" $PYTHON_BIN ./test/filter.py"
printf "\n\nrunning command\n$CMD\n\n"
PYTHONPATH="$ROOT_DIR${PYTHONPATH:+:$PYTHONPATH}" "$PYTHON_BIN" ./test/filter.py || exit 1

printf "\nFinished running integration tests\n\n"