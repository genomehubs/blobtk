#!/usr/bin/env bash
# batch_import.sh — run blobtk import for every row in a TSV of assembly accessions and taxon IDs
#
# Usage:
#   bash batch_import.sh ASSEMBLIES.tsv TEMPLATE.yaml [OUTPUT_DIR] [LOG_FILE]
#
# TSV format (no header or a header line starting with #):
#   GCA_016920705.1    1518534
#
# The script replaces the anchor values for ACCESSION and TAXON in the template,
# writes a per-assembly config file, then calls:
#   blobtk import -c <config>
#
# A log TSV is written with columns: accession, taxon_id, status, duration_s, message

set -euo pipefail

ASSEMBLIES="${1:?Usage: $0 ASSEMBLIES.tsv TEMPLATE.yaml [OUTPUT_DIR] [LOG_FILE]}"
TEMPLATE="${2:?Usage: $0 ASSEMBLIES.tsv TEMPLATE.yaml [OUTPUT_DIR] [LOG_FILE]}"
OUTPUT_DIR="${3:-./import_configs}"
LOG_FILE="${4:-./batch_import.log.tsv}"

if [[ ! -f "$ASSEMBLIES" ]]; then
    echo "ERROR: assemblies file not found: $ASSEMBLIES" >&2
    exit 1
fi
if [[ ! -f "$TEMPLATE" ]]; then
    echo "ERROR: template config not found: $TEMPLATE" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# Write log header if file does not already exist
if [[ ! -f "$LOG_FILE" ]]; then
    printf 'accession\ttaxon_id\tstatus\tduration_s\tmessage\n' > "$LOG_FILE"
fi

# Locate the blobtk binary — prefer one on PATH, fall back to the Rust debug build
BLOBTK_BIN="${BLOBTK_BIN:-blobtk}"
if ! command -v "$BLOBTK_BIN" &>/dev/null; then
    RUST_DEBUG="$(dirname "$0")/../rust/target/debug/blobtk"
    if [[ -x "$RUST_DEBUG" ]]; then
        BLOBTK_BIN="$RUST_DEBUG"
        echo "INFO: using debug binary at $BLOBTK_BIN"
    else
        echo "ERROR: blobtk not found on PATH and no debug binary at $RUST_DEBUG" >&2
        exit 1
    fi
fi

total=0
passed=0
failed=0

while IFS=$'\t' read -r accession taxon_id remainder; do
    # Skip blank lines and comment lines
    [[ -z "$accession" || "$accession" == \#* ]] && continue

    total=$(( total + 1 ))
    config_file="$OUTPUT_DIR/${accession}.import.yaml"

    # Patch the anchor values for ACCESSION and TAXON in the template.
    # The pattern targets lines of the form:
    #   accession: &ACCESSION GCA_...
    #   taxon_id: &TAXON 12345
    # and replaces only the value token after the anchor, preserving everything else.
    sed \
        -e "s|^\(  accession: &ACCESSION\) .*|\1 ${accession}|" \
        -e "s|^\(  taxon_id: &TAXON\) .*|\1 ${taxon_id}|" \
        "$TEMPLATE" > "$config_file"

    echo "---"
    echo "Running: $accession (taxon $taxon_id)"

    start_ts=$(date +%s)
    if output=$("$BLOBTK_BIN" import -c "$config_file" 2>&1); then
        end_ts=$(date +%s)
        duration=$(( end_ts - start_ts ))
        status="success"
        message="ok"
        passed=$(( passed + 1 ))
        echo "  SUCCESS in ${duration}s"
    else
        end_ts=$(date +%s)
        duration=$(( end_ts - start_ts ))
        status="fail"
        # Capture last non-blank line of output as the error message
        message=$(echo "$output" | grep -v '^$' | tail -n 1 | tr '\t' ' ')
        failed=$(( failed + 1 ))
        echo "  FAILED in ${duration}s: $message"
    fi

    printf '%s\t%s\t%s\t%d\t%s\n' \
        "$accession" "$taxon_id" "$status" "$duration" "$message" >> "$LOG_FILE"

done < "$ASSEMBLIES"

echo "==="
echo "Done: $total total, $passed succeeded, $failed failed"
echo "Log written to $LOG_FILE"
