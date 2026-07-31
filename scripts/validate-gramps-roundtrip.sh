#!/usr/bin/env bash
#
# validate-gramps-roundtrip.sh
#
# Validation script that builds gramps-gen with each schema version,
# generates a .gramps file, and runs structural checks to verify the
# output is correct. Optionally imports the file into Gramps if the
# `gramps` command is on $PATH.
#
# Usage:
#   ./scripts/validate-gramps-roundtrip.sh
#
# Exit code: 0 on success, non-zero on any failure.
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

# Check required external tools
if ! command -v xmllint >/dev/null 2>&1; then
    echo "ERROR: xmllint not found — install libxml2-utils (apt) or libxml2 (brew)" >&2
    exit 1
fi

TEMP_DIR=$(mktemp -d /tmp/gramps-gen-validate-XXXXXX)
trap 'rm -rf "$TEMP_DIR"' EXIT

PASS=0
FAIL=0

green() { printf "  \033[32m✓ %s\033[0m\n" "$1"; }
red()   { printf "  \033[31m✗ %s\033[0m\n" "$1"; }

check() {
    local desc="$1"
    shift
    if "$@" > /dev/null 2>&1; then
        green "$desc"
        PASS=$((PASS + 1))
    else
        red "$desc"
        FAIL=$((FAIL + 1))
    fi
}

check_output() {
    local desc="$1"
    local expected="$2"
    local file="$3"
    if grep -q "$expected" "$file"; then
        green "$desc"
        PASS=$((PASS + 1))
    else
        red "$desc (expected: '$expected')"
        FAIL=$((FAIL + 1))
    fi
}

check_not_in_output() {
    local desc="$1"
    local pattern="$2"
    local file="$3"
    if ! grep -q "$pattern" "$file"; then
        green "$desc"
        PASS=$((PASS + 1))
    else
        red "$desc (found: '$pattern')"
        FAIL=$((FAIL + 1))
    fi
}

# ────────────────────────────────────────────────────────────────────
# Test with schema-5-1
# ────────────────────────────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  Validating with schema-5-1"
echo "=========================================="

echo ""
echo "  [build] cargo build --release --features schema-5-1"
if cargo build --release --features schema-5-1 2>/dev/null; then
    green "build succeeds with schema-5-1"
    PASS=$((PASS + 1))
else
    red "build fails with schema-5-1"
    FAIL=$((FAIL + 1))
fi

OUTPUT_51="$TEMP_DIR/test-51.gramps"
echo ""
echo "  [generate] target/release/gramps-gen generate --schema-version 5.1 --output $OUTPUT_51 --count 8 --seed 2026 --depth 4"
if target/release/gramps-gen generate \
    --schema-version 5.1 \
    --output "$OUTPUT_51" \
    --count 8 \
    --seed 2026 \
    --depth 4 \
    2>/dev/null; then
    green "generation succeeds with schema-5-1"
    PASS=$((PASS + 1))
else
    red "generation fails with schema-5-1"
    FAIL=$((FAIL + 1))
fi

echo ""
echo "  [structural checks]"
check "well-formed XML (xmllint)" xmllint --noout "$OUTPUT_51"
check_output "namespace is 1.7.1" "gramps-project.org/xml/1.7.1/" "$OUTPUT_51"
check_not_in_output "no 'Some(' artifacts" "Some(" "$OUTPUT_51"
check_output "version is 3-part semver" 'version="5.1.6"' "$OUTPUT_51"

# ────────────────────────────────────────────────────────────────────
# Test with schema-5-2 (default)
# ────────────────────────────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  Validating with default features (schema-5-2)"
echo "=========================================="

echo ""
echo "  [build] cargo build --release"
if cargo build --release 2>/dev/null; then
    green "build succeeds with default features"
    PASS=$((PASS + 1))
else
    red "build fails with default features"
    FAIL=$((FAIL + 1))
fi

OUTPUT_52="$TEMP_DIR/test-52.gramps"
echo ""
echo "  [generate] target/release/gramps-gen generate --schema-version 5.2 --output $OUTPUT_52 --count 8 --seed 2026 --depth 4"
if target/release/gramps-gen generate \
    --schema-version 5.2 \
    --output "$OUTPUT_52" \
    --count 8 \
    --seed 2026 \
    --depth 4 \
    2>/dev/null; then
    green "generation succeeds with schema-5-2"
    PASS=$((PASS + 1))
else
    red "generation fails with schema-5-2"
    FAIL=$((FAIL + 1))
fi

echo ""
echo "  [structural checks]"
check "well-formed XML (xmllint)" xmllint --noout "$OUTPUT_52"
check_output "namespace is 1.7.2" "gramps-project.org/xml/1.7.2/" "$OUTPUT_52"
check_not_in_output "no 'Some(' artifacts" "Some(" "$OUTPUT_52"
check_output "version is 3-part semver" 'version="5.2.0"' "$OUTPUT_52"

# ────────────────────────────────────────────────────────────────────
# Optional: Gramps import check
# ────────────────────────────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  Gramps import check (optional)"
echo "=========================================="

if command -v gramps &> /dev/null; then
    echo "  gramps found at $(which gramps)"

    # Try importing the 5.1 file
    echo "  [import] gramps -C gramps-gen-validate-5.1 -i $OUTPUT_51 -f gramps -y"
    if gramps -C "gramps-gen-validate-5.1" -i "$OUTPUT_51" -f gramps -y \
        2>/dev/null; then
        green "Gramps 5.1 import succeeds"
        PASS=$((PASS + 1))
    else
        red "Gramps 5.1 import fails"
        FAIL=$((FAIL + 1))
    fi

    # Try importing the 5.2 file
    echo "  [import] gramps -C gramps-gen-validate-5.2 -i $OUTPUT_52 -f gramps -y"
    if gramps -C "gramps-gen-validate-5.2" -i "$OUTPUT_52" -f gramps -y \
        2>/dev/null; then
        green "Gramps 5.2 import succeeds"
        PASS=$((PASS + 1))
    else
        red "Gramps 5.2 import fails"
        FAIL=$((FAIL + 1))
    fi
else
    echo "  gramps not found on PATH — skipping import check"
    echo "  Install Gramps to enable import validation"
fi

# ────────────────────────────────────────────────────────────────────
# Summary
# ────────────────────────────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  Results"
echo "=========================================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [ "$FAIL" -eq 0 ]; then
    green "All checks passed!"
    exit 0
else
    red "Some checks failed!"
    exit 1
fi