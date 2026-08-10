#!/usr/bin/env bash
#
# validate-gramps-import.sh
#
# Validate a .gramps file by importing it into Gramps via the CLI.
# Gramps stays open after a successful import, so the command is run
# under a short timeout; the timeout exit code (124) is treated as
# success since the import itself already completed.
#
# Usage:
#   ./scripts/validate-gramps-import.sh <file.gramps>
#
# Stale-artifact guard note:
#   Before running the delete round-trip / import tests, always run
#   `cargo clean -p typed-graph` so the generated $OUT_DIR code matches the
#   current schema files.  Stale $OUT_DIR artifacts (e.g. an old NoteData
#   without `type_field: Option<String>`) can mask the note-type default fix
#   on the delete round-trip path.
#
# CI wiring is intentionally deferred: there is no `.github/workflows/`
# directory in this repo yet, so this script is not yet wired into a
# workflow.  When one is added (with Gramps in the runner image), call this
# script after a `cargo clean -p typed-graph` in the same job.
#
# Exit code:
#   0 — import completed with no error patterns in output
#   1 — usage error, or import failed / emitted error patterns
#
set -euo pipefail

INPUT_FILE="${1:-}"
if [ -z "$INPUT_FILE" ]; then
    echo "Usage: $0 <file.gramps>" >&2
    exit 1
fi

if ! command -v gramps >/dev/null 2>&1; then
    echo "ERROR: gramps binary not found on PATH" >&2
    exit 1
fi

TIMEOUT_SECONDS=15
OUTPUT=$(mktemp)
trap 'rm -f "$OUTPUT"' EXIT

# Run gramps import with timeout; capture stdout+stderr
if timeout "$TIMEOUT_SECONDS" gramps -y -q -i "$INPUT_FILE" >"$OUTPUT" 2>&1; then
    # Check for error patterns in the output
    if grep -qE 'ERROR:|Traceback|TypeError|Failed to import' "$OUTPUT"; then
        echo "IMPORT FAILED: errors detected in output" >&2
        cat "$OUTPUT" >&2
        exit 1
    fi
    exit 0
else
    exit_code=$?
    if [ $exit_code -eq 124 ]; then
        # timeout is expected — gramps stays open after import
        # Still check the output for errors
        if grep -qE 'ERROR:|Traceback|TypeError|Failed to import' "$OUTPUT"; then
            echo "IMPORT FAILED: errors detected in output (timed out as expected)" >&2
            cat "$OUTPUT" >&2
            exit 1
        fi
        exit 0
    fi
    echo "IMPORT FAILED: gramps exited with code $exit_code" >&2
    cat "$OUTPUT" >&2
    exit 1
fi