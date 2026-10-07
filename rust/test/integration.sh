#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_SCRIPT="$ROOT_DIR/../scripts/run-local-rust.sh"

if [[ -x "$RUST_SCRIPT" ]]; then
  export CARGO_BIN="$($RUST_SCRIPT --print-cargo-bin)"
  export PATH="$(dirname "$CARGO_BIN"):${PATH:-}"
  export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
  export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
  echo "Using Rust toolchain: $($CARGO_BIN --version)"
fi

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
    maturin build --release &&
    yes | pip uninstall blobtk &&
    yes | pip install ./target/wheels/blobtk-*.whl"
printf "\nrunning command\n$CMD\n\n"
rm -f ./target/wheels/blobtk-*.whl &&
    maturin build --release &&
    yes | pip uninstall blobtk &&
    yes | pip install ./target/wheels/blobtk-*.whl || exit 1

CMD="./test/depth.py"
printf "\n\nrunning command\n$CMD\n\n"
$CMD || exit 1

CMD="./test/filter.py"
printf "\n\nrunning command\n$CMD\n\n"
$CMD || exit 1

printf "\nFinished running integration tests\n\n"