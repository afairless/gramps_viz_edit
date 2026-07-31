# Implementation Plan: Fix validate-gramps-roundtrip.sh Import Failures

Source: `docs/research/fix-validate-script.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add external dependency check for xmllint in validation script` | External dependency guard | `scripts/validate-gramps-roundtrip.sh` | — |
| 2 | `fix: correct Gramps 5.1 import command syntax and remove temp tree cleanup` | Fix 5.1 import command | `scripts/validate-gramps-roundtrip.sh` | — |
| 3 | `fix: gate 5.2 import on Gramps version to avoid expected failure` | Gramps version gate | `scripts/validate-gramps-roundtrip.sh` | — |
| 4 | `feat: add diagnostic error output on import failure` | Diagnostic error logging | `scripts/validate-gramps-roundtrip.sh` | — |
| 5 | `fix: add sleep between Gramps invocations and handle stale lock files` | Race condition + stale lock fix | `scripts/validate-gramps-roundtrip.sh` | — |
| 6 | `test: verify fix-validate-script changes with manual verification` | Verification | — | — |

## Step Details

### 1 — External dependency guard

**What**: Add a `command -v xmllint` check at the top of the script (after the `cd` to project root, before any build or test logic). Exit with a clear error message if `xmllint` is not found.

**Rationale**: The script already uses `xmllint` for structural checks but fails silently (or with a cryptic error) if it's missing. A guard at the top gives a clear, actionable error message.

**Location**: After the `cd "$PROJECT_DIR"` line, before the `TEMP_DIR` and `trap` setup.

**Tests**: — (manual verification: run without xmllint, confirm clear error message and exit code 1)

### 2 — Fix 5.1 import command

**What**: Two changes in the Gramps import block:

**Problem 1a: `-a import` is invalid.** Replace the Gramps 5.1 import command:

```bash
# Before (wrong — -a import is not a recognized action, -O opens nonexistent tree)
gramps -i "$OUTPUT_51" -a import -f gramps -O "$GRAMPS_IMPORT_DIR/imported-51"
```

```bash
# After (correct — -C creates a new tree, no -a import needed, -y for non-interactive)
gramps -C "gramps-gen-validate-5.1" -i "$OUTPUT_51" -f gramps -y
```

**Problem 1b: Dead code cleanup.** Remove `$GRAMPS_IMPORT_DIR` entirely:

- Remove the `GRAMPS_IMPORT_DIR="$TEMP_DIR/gramps-import"` variable declaration
- Remove the `mkdir -p "$GRAMPS_IMPORT_DIR"` before the 5.1 import
- Remove the `rm -rf "$GRAMPS_IMPORT_DIR"` and second `mkdir -p "$GRAMPS_IMPORT_DIR"` between the 5.1 and 5.2 import blocks

Apply the same fix pattern to the 5.2 import block, using the tree name `"gramps-gen-validate-5.2"`.

**Open question (GRAMPSHOME isolation)**: As documented in the plan, setting `GRAMPSHOME` to a temp directory is an optional enhancement. The initial implementation will NOT add `GRAMPSHOME` isolation; it can be added later if test pollution becomes a problem.

**Tests**: — (manual: run the fixed script with Gramps 5.1.x, confirm 5.1 import passes, 5.2 import still fails with expected error)

### 3 — Gramps version gate

**What**: Add a version check before attempting the 5.2 import. Extract the installed Gramps version, compare it to 5.2, and skip the 5.2 import if the installed version is older.

```bash
# Extract gramps version (expects output like: " gramps : 5.1.6")
GRAMPS_VERSION=$(gramps --version 2>/dev/null | grep "^ gramps " | sed 's/.*: //' | cut -d. -f1,2)

if [ -z "$GRAMPS_VERSION" ]; then
    echo "  Could not determine Gramps version — skipping import checks"
elif [ "$(printf '%s\n%s\n' "5.2" "$GRAMPS_VERSION" | awk -F. '{printf "%03d%03d\n", $1, $2}' | sort -n | head -n1)" = "$(printf '%s' "5.2" | awk -F. '{printf "%03d%03d\n", $1, $2}')" ]; then
    # ... attempt 5.2 import ...
else
    echo "  gramps $GRAMPS_VERSION < 5.2 — skipping 5.2 import check (known limitation)"
fi
```

**Portability**: Use `awk` zero-padding + numeric `sort` instead of GNU `sort -V` (not available on macOS/BSD).

**Location**: The version check wraps the 5.2 import block. The existing `if command -v gramps` block is restructured to contain both the 5.1 import (always attempted if gramps is available) and the 5.2 import (gated on version ≥ 5.2).

**Tests**: — (manual: run with Gramps 5.1.x, confirm 5.2 import is skipped with informational message; run with Gramps 5.2.x, confirm both imports are attempted)

### 4 — Diagnostic error output

**What**: Add a new helper function `check_cmd_log_errors` that preserves error output on failure:

```bash
check_cmd_log_errors() {
    local desc="$1"
    shift
    local logfile="$TEMP_DIR/$(echo "$desc" | tr ' ' '-' | tr -cd '[:alnum:]-').log"
    if "$@" > "$logfile" 2>&1; then
        green "$desc"
        PASS=$((PASS + 1))
    else
        red "$desc"
        echo "    Full log: $logfile" >&2
        tail -n 20 "$logfile" | sed 's/^/    | /' >&2
        FAIL=$((FAIL + 1))
    fi
}
```

Replace the `gramps -C ...` invocation blocks with `check_cmd_log_errors` calls instead of the bare `if gramps ...` pattern. This preserves the full error output in a temporary log file and shows the last 20 lines inline for quick CI debugging.

**Tests**: — (manual: force an import failure, confirm the error log is captured and displayed)

### 5 — Race condition + stale lock fix

**What**: Two reliability improvements:

1. **Stale lock cleanup**: Before the first `gramps -C` invocation (inside the `if command -v gramps` block), remove any stale lock file:

   ```bash
   rm -f "${GRAMPSHOME:-$HOME/.gramps}/lock"
   ```

2. **Sleep between invocations**: Add `sleep 2` between the 5.1 and 5.2 import blocks to prevent Gramps' singleton lock from causing an intermittent "Gramps is already running" error.

**Location**: The lock cleanup goes just before the first `gramps -C ...` invocation. The `sleep 2` goes between the 5.1 and 5.2 import blocks (after the 5.1 `check_cmd_log_errors` call, before the 5.2 version gate).

**Tests**: — (manual: run back-to-back, confirm no race condition; simulate stale lock, confirm cleanup and successful import)

### 6 — Verification

**What**: Manual verification following the verification plan in the source document:

1. **Before-fix baseline**: Run the script as-is. Confirm 12 structural checks pass, 2 Gramps imports fail.
2. **After-fix with Gramps 5.1.x**: Run the fixed script. Confirm 12 structural + 1 (5.1) import pass, and the 5.2 import is skipped. Overall exit code 0.
3. **After-fix with Gramps 5.2.x** (if available): Run the fixed script. Confirm both imports pass. Exit code 0.
4. **After-fix without Gramps on PATH**: Run the fixed script. Confirm 12 structural checks pass, all imports skipped, exit code 0.
5. **Edge cases**: Test with `GRAMPSHOME` set and unset. Test with a stale lock file present. Test on both Linux (GNU coreutils) and macOS (BSD coreutils) if possible.

**Tests**: — (manual verification only)
