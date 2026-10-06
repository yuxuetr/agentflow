#!/usr/bin/env bash
# check_unsafe.sh — scan Rust source files for unsafe blocks.
#
# Usage (via ScriptTool):
#   script: check_unsafe.sh
#   args: {"path": "./src"}
#
# Reads JSON from stdin: {"path": "<directory or file to scan>"}
# Outputs a plain-text report of all unsafe blocks found.

set -euo pipefail

# Read JSON from stdin if available.
if [ -t 0 ]; then
    # No stdin piped; use default path.
    SCAN_PATH="."
else
    INPUT=$(cat)
    # Extract "path" field with a simple grep/sed (no jq dependency).
    SCAN_PATH=$(echo "$INPUT" | grep -o '"path"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | sed 's/"path"[[:space:]]*:[[:space:]]*"\(.*\)"/\1/' || echo ".")
fi

if [ -z "$SCAN_PATH" ]; then
    SCAN_PATH="."
fi

echo "=== Unsafe block scan: $SCAN_PATH ==="
echo ""

COUNT=0
while IFS= read -r -d '' file; do
    MATCHES=$(grep -n 'unsafe' "$file" 2>/dev/null || true)
    if [ -n "$MATCHES" ]; then
        echo "File: $file"
        echo "$MATCHES"
        echo ""
        COUNT=$((COUNT + 1))
    fi
done < <(find "$SCAN_PATH" -name "*.rs" -print0 2>/dev/null)

if [ "$COUNT" -eq 0 ]; then
    echo "✓ No unsafe blocks found."
else
    echo "Found unsafe usage in $COUNT file(s)."
fi
