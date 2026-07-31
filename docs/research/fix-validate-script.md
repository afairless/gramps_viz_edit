# Fix validate-gramps-roundtrip.sh Import Failures

Date: 2026-07-30

Status: Plan (not yet implemented)

## Background

`scripts/validate-gramps-roundtrip.sh` was created as Phase 5 of the [gramps-import-fix](./gramps-import-fix.md) plan. The script performs build, generate, structural checks, and optional Gramps imports against both schema-5-1 and schema-5-2 builds.

All 12 structural checks pass. The 2 failures are in the optional Gramps import section.

## Current behavior (failing)

```
==========================================
  Gramps import check (optional)
==========================================
  gramps found at /usr/bin/gramps
  ✗ Gramps 5.1 import fails
  ✗ Gramps 5.2 import fails

==========================================
  Results
==========================================
  Passed: 12
  Failed: 2
  ✗ Some checks failed!
```

The script hides stderr behind `2>/dev/null` in its `check` helper, which masks the actual errors.

## Diagnosis

### Failure 1: Gramps 5.1 import — wrong CLI syntax (fixable)

The script runs:

```bash
gramps -i "$OUTPUT_51" -a import -f gramps -O "$GRAMPS_IMPORT_DIR/imported-51"
```

Two problems:

**Problem 1a: `-a import` is not a valid action.** Gramps 5.1.6 does not recognize `"import"` as a value for the `-a, --action` flag. Gramps prints:

> Unknown action: import. Ignoring.

The `-i` flag already triggers an import; no `-a import` is needed.

**Problem 1b: `-O` opens an existing tree, but the tree doesn't exist yet.** The `-O, --open` flag opens a named family tree that must already exist. For importing into a new tree, use `-C, --create`:

```bash
gramps -C "gramps-gen-validation-51" -i "$OUTPUT_51" -f gramps
```

When tested directly with the corrected command, the 5.1 file imports successfully:

```
Opened successfully!
Importing: file /tmp/test-51.gramps, format gramps.
...100% Cleaning up.
```

**Problem 1c: Stderr is discarded.** The `check` helper function runs `"$@" > /dev/null 2>&1`, which discards Gramps output. Even after fixing the syntax, the import output goes to stderr via Gramps' Python logging, so the `check` helper won't see the success message. However, `check` relies on exit codes (not stdout), so this is only a debuggability concern. The exit code is what matters — Gramps exits 0 on success and nonzero on failure.

### Failure 2: Gramps 5.2 import — version mismatch (unavoidable with current Gramps)

The installed Gramps is **version 5.1.6**. When asked to import a `.gramps` file with namespace `1.7.2` and `<created version="5.2.0"/>`, Gramps refuses:

> The .gramps file you are importing was made by version 5.2.0 of Gramps, while you are running an older version 5.1.6. The file will not be imported.

This is a **hard version gate** in Gramps' import code (see `gramps-import-fix.md` for the namespace comparison logic). The 5.2 file can only be imported by Gramps ≥ 5.2.0.

Even after fixing problem 1, this import will always fail on the current system.

### Bonus: Race condition (intermittent)

Back-to-back `gramps` CLI invocations can trigger:

> Gramps is already running.

This is a Gramps lock file / singleton check. A brief `sleep` between invocations prevents it.

## Plan

Each step includes its dependencies in parentheses. Apply them in order.

### Step 1: Fix the 5.1 import command and clean up temp tree (depends on: nothing)

Replace:

```bash
# Before (wrong — creates a directory path, passes it to -O, uses invalid -a import)
GRAMPS_IMPORT_DIR="$TEMP_DIR/gramps-import"
mkdir -p "$GRAMPS_IMPORT_DIR"

gramps -i "$OUTPUT_51" -a import -f gramps -O "$GRAMPS_IMPORT_DIR/imported-51"
```

With:

```bash
# After (correct — -C creates a new tree, no -a import needed, -y for non-interactive)
gramps -C "gramps-gen-validate-5.1" -i "$OUTPUT_51" -f gramps -y
```

Changes:

- **Drop `-a import`** — not a recognized action. The `-i` flag already triggers an import.
- **Use `-C` instead of `-O`** — create a new tree rather than opening a nonexistent one.
- **Use a dummy tree name** instead of a directory path — `-C` takes a name, not a path.
- **Remove `$GRAMPS_IMPORT_DIR` entirely** — the `mkdir -p` and variable declaration, and the `rm -rf "$GRAMPS_IMPORT_DIR"` between the 5.1 and 5.2 import blocks all become dead code after switching to `-C`.
- **Add `-y`** — skip interactive confirmation prompts in non-GUI mode. **Verify this works** on Gramps 5.1.6 before committing; if not, drop `-y` and note the limitation.
- **Add `-q` optionally** — suppress progress output for cleaner test logs.

**Isolation (optional):** To prevent pollution of the user's real Gramps databases, set `GRAMPSHOME` to a temp directory before the import block:

```bash
export GRAMPSHOME="$TEMP_DIR/gramps-home"
mkdir -p "$GRAMPSHOME"
```

(Whether to use this is left as an open question — see below.)

Apply the same fix pattern to the 5.2 import block, using the tree name `"gramps-gen-validate-5.2"`.

### Step 2: Gate the 5.2 import on Gramps version (depends on: nothing)

Before attempting the 5.2 import, check whether the installed Gramps supports it.

```bash
# Extract gramps version (expects output like: " gramps : 5.1.6")
GRAMPS_VERSION=$(gramps --version 2>/dev/null | grep "^ gramps " | sed 's/.*: //' | cut -d. -f1,2)

# Guard: if version extraction failed, skip all imports
if [ -z "$GRAMPS_VERSION" ]; then
    echo "  Could not determine Gramps version — skipping import checks"
elif [ "$(printf '%s\n%s\n' "5.2" "$GRAMPS_VERSION" | awk -F. '{printf "%03d%03d\n", $1, $2}' | sort -n | head -n1)" = "$(printf '%s' "5.2" | awk -F. '{printf "%03d%03d\n", $1, $2}')" ]; then
    # ... attempt 5.2 import ...
else
    echo "  gramps $GRAMPS_VERSION < 5.2 — skipping 5.2 import check (known limitation)"
fi
```

**Portability note:** The version comparison uses `awk` zero-padding + numeric `sort` instead of GNU `sort -V`, which is not available on macOS/BSD. Test on both Linux and macOS.

This converts an expected failure into a skipped informational message. The check is not considered failed because the limitation is in the test environment, not the generated file.

### Step 3: Fix the race condition and handle stale lock files (depends on: Step 1, Step 2)

Add a short wait between Gramps invocations, inserted between the 5.1 and 5.2 import blocks:

```bash
sleep 2
```

Gramps takes a moment to release its singleton lock after normal exit.

**Stale lock files:** If Gramps crashes or is killed during import, it may leave a stale lock file (typically `~/.gramps/lock` or `$GRAMPSHOME/lock`). The `sleep 2` alone won't help here. Before the first import attempt, check for and remove any stale lock:

```bash
# Remove stale Gramps lock file if present (from a previous crash)
rm -f "${GRAMPSHOME:-$HOME/.gramps}/lock"
```

Insert this just before the first `gramps -C ...` invocation inside the `if command -v gramps` block.

### Step 4: Add diagnostic error output on import failure (depends on: Step 1)

Currently `check` discards all output. For the Gramps import specifically, capture the error output and log it so the user can see what went wrong without re-running manually.

Add a new helper (named `check_cmd_log_errors` to avoid collision with the existing `check_output` function):

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
        # Show last 20 lines for a quick glance in CI output
        tail -n 20 "$logfile" | sed 's/^/    | /' >&2
        FAIL=$((FAIL + 1))
    fi
}
```

Use `check_cmd_log_errors` for the Gramps import checks. The full error output is preserved in the temp log file for inspection; CI can archive these or the user can examine them after the run.

**Naming:** The script already has `check` (run command, hide output) and `check_output` (grep a file for expected text). The new helper is `check_cmd_log_errors` to clearly distinguish it from both.

### Step 5: Guard external dependencies (depends on: nothing)

Add a dependency check at the top of the script (after the `cd` but before the main logic) so missing tools produce a clear error:

```bash
# Check required external tools
if ! command -v xmllint >/dev/null 2>&1; then
    echo "ERROR: xmllint not found — install libxml2-utils (apt) or libxml2 (brew)" >&2
    exit 1
fi
```

`cargo` is assumed present since the script lives in the project repo.

### Step 6: Verification plan (depends on: all previous steps)

After applying all changes, verify correctness:

1. **Before-fix baseline**: Run the script as-is. Confirm 12 structural checks pass, 2 Gramps imports fail. Record the failure output.
2. **After-fix with Gramps 5.1.x (if available)**: Run the fixed script. Confirm 12 structural + 1 (5.1) import pass, and the 5.2 import is skipped with the informational message. Overall exit code is 0.
3. **After-fix with Gramps 5.2.x (if available)**: Run the fixed script. Confirm both imports pass. Overall exit code is 0.
4. **After-fix without Gramps on PATH**: Run the fixed script. Confirm 12 structural checks pass, all imports skipped, exit code 0.
5. **Edge cases**: Test with `GRAMPSHOME` set and unset. Test with a stale lock file present (touch a fake lock, verify the script removes it and proceeds). Test on both Linux (GNU coreutils) and macOS (BSD coreutils) to verify the portable version comparison works.

## Target state

After these fixes, running the script will:

| Environment | 5.1 import | 5.2 import | Overall result |
|---|---|---|---|
| Gramps 5.1.x | ✅ Pass | ⏭️ Skipped (version ≤ 5.1) | Pass (all non-skipped checks pass) |
| Gramps 5.2.x | ✅ Pass | ✅ Pass | Pass (all checks pass) |
| No Gramps installed | ⏭️ Skipped | ⏭️ Skipped | Pass (all structural checks pass) |

The script's exit code is 0 (success) when:

- All structural checks pass, **and**
- All Gramps import checks that were attempted pass (skipped checks don't count as failures)

## Open questions

1. **Should we use `GRAMPSHOME` isolation?** Setting `GRAMPSHOME` to a temp directory prevents Gramps from interacting with the user's real databases. This is safer for automated testing but adds complexity. Consider the user profile: developers running this locally probably want isolation; CI environments already have no Gramps databases. **Decision:** Leave `GRAMPSHOME` isolation as an optional enhancement (documented in Step 1). It can be added later if test pollution becomes a problem.

2. **Should skipped import checks have their own counter?** Currently skipped imports don't change PASS or FAIL. A dedicated `SKIP` counter (incremented when an import is skipped, printed in the summary) would make the output more transparent. For the initial implementation, keeping it simple (no counter change for skipped) is acceptable; a `SKIP` counter can be added later if the output is confusing without it.
