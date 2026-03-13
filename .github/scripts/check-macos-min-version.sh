#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <mach-o-file> [max-allowed-version]"
  exit 2
fi

file_path="$1"
max_allowed="${2:-10.13}"

if [[ ! -f "$file_path" ]]; then
  echo "File not found: $file_path"
  exit 1
fi

meta=$(otool -l "$file_path" | grep -A3 -E 'LC_BUILD_VERSION|LC_VERSION_MIN_MACOSX' || true)
echo "$meta"

minos=$(echo "$meta" | awk '/minos/{print $2; exit} /version/{print $2; exit}')
if [[ -z "$minos" ]]; then
  echo "Unable to detect minimum macOS version for $file_path"
  exit 1
fi

python3 - "$minos" "$max_allowed" <<'PY'
import sys

def parse(version: str) -> tuple[int, ...]:
    return tuple(int(x) for x in version.split('.'))

detected = parse(sys.argv[1])
allowed = parse(sys.argv[2])
if detected > allowed:
    raise SystemExit(
        f"Minimum macOS version too high: {sys.argv[1]} (max allowed {sys.argv[2]})"
    )
print(f"Deployment target OK: {sys.argv[1]} <= {sys.argv[2]}")
PY
