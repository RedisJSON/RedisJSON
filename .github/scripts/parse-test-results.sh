#!/bin/bash
# Parse RLTest output to extract test counts
# RLTest is based on Python unittest, so it outputs similar format
# Usage: parse-test-results.sh <test_output_file> <output_json_file>

set -e

INPUT_FILE="${1}"
OUTPUT_FILE="${2:-test-results.json}"

# Initialize counters
PASSED=0
FAILED=0
SKIPPED=0
TOTAL=0

# Read input (either file or stdin)
if [ -n "$INPUT_FILE" ] && [ -f "$INPUT_FILE" ]; then
    TEST_OUTPUT=$(cat "$INPUT_FILE")
else
    TEST_OUTPUT=$(cat)
fi

# RLTest/unittest typically outputs:
# "Ran X tests in Y seconds"
# Then either:
# "OK" or "OK (skipped=Z)" or "FAILED (failures=W, errors=V)"

# Extract total number of tests
TOTAL=$(echo "$TEST_OUTPUT" | grep -iE "ran [0-9]+ test" | grep -oE "[0-9]+" | head -1 | tr -d '\n\r ' || echo "0")
TOTAL=${TOTAL:-0}

# Check for OK status (all passed)
if echo "$TEST_OUTPUT" | grep -qiE "^\s*ok\s*$|^\s*ok\s*\(|^ok"; then
    # All tests passed
    # Extract skipped count if present
    SKIPPED=$(echo "$TEST_OUTPUT" | grep -iE "ok.*skipped" | grep -oE "skipped[=:][ ]*[0-9]+" | grep -oE "[0-9]+" | head -1 | tr -d '\n\r ' || echo "0")
    SKIPPED=${SKIPPED:-0}
    if [ "$TOTAL" != "0" ] && [ -n "$TOTAL" ]; then
        PASSED=$((TOTAL - SKIPPED))
    fi
# Check for FAILED status
elif echo "$TEST_OUTPUT" | grep -qiE "^\s*failed\s*$|^\s*failed\s*\(|^failed"; then
    # Extract failure and error counts
    FAILED=$(echo "$TEST_OUTPUT" | grep -iE "failed.*failures" | grep -oE "failures?[=:][ ]*[0-9]+" | grep -oE "[0-9]+" | head -1 | tr -d '\n\r ' || echo "0")
    FAILED=${FAILED:-0}
    ERRORS=$(echo "$TEST_OUTPUT" | grep -iE "failed.*errors" | grep -oE "errors?[=:][ ]*[0-9]+" | grep -oE "[0-9]+" | head -1 | tr -d '\n\r ' || echo "0")
    ERRORS=${ERRORS:-0}
    FAILED=$((FAILED + ERRORS))
    
    # Extract skipped count if present
    SKIPPED=$(echo "$TEST_OUTPUT" | grep -iE "skipped" | grep -oE "skipped[=:][ ]*[0-9]+" | grep -oE "[0-9]+" | head -1 | tr -d '\n\r ' || echo "0")
    SKIPPED=${SKIPPED:-0}
    
    if [ "$TOTAL" != "0" ] && [ -n "$TOTAL" ]; then
        PASSED=$((TOTAL - FAILED - SKIPPED))
    fi
fi

# If we still don't have counts, try counting test execution lines
if [ "$TOTAL" = "0" ] || ([ "$PASSED" = "0" ] && [ "$FAILED" = "0" ]); then
    # Try to count test method calls or results
    # Look for patterns like "test_xxx" or ".test_xxx" or "test_xxx ... ok" or "test_xxx ... FAIL"
    
    # Count passed tests (look for "ok" or "PASS" after test name)
    PASSED_COUNT=$(echo "$TEST_OUTPUT" | grep -cE "(test_\w+.*\bok\b|test_\w+.*\bPASS\b|\[PASS\]|\[OK\])" 2>/dev/null || echo "0")
    # Extract first number only and ensure it's a valid integer
    PASSED=$(echo "$PASSED_COUNT" | tr -d '\n\r ' | grep -oE '^[0-9]+' | head -1)
    if [ -z "$PASSED" ] || ! [ "$PASSED" -eq "$PASSED" ] 2>/dev/null; then
        PASSED=0
    fi
    
    # Count failed tests (look for "FAIL" or "ERROR" after test name)
    FAILED_COUNT=$(echo "$TEST_OUTPUT" | grep -cE "(test_\w+.*\bFAIL\b|test_\w+.*\bERROR\b|\[FAIL\]|\[ERROR\])" 2>/dev/null || echo "0")
    # Extract first number only and ensure it's a valid integer
    FAILED=$(echo "$FAILED_COUNT" | tr -d '\n\r ' | grep -oE '^[0-9]+' | head -1)
    if [ -z "$FAILED" ] || ! [ "$FAILED" -eq "$FAILED" ] 2>/dev/null; then
        FAILED=0
    fi
    
    # Count skipped tests
    SKIPPED_COUNT=$(echo "$TEST_OUTPUT" | grep -cE "(test_\w+.*\bSKIP\b|\[SKIP\])" 2>/dev/null || echo "0")
    # Extract first number only and ensure it's a valid integer
    SKIPPED=$(echo "$SKIPPED_COUNT" | tr -d '\n\r ' | grep -oE '^[0-9]+' | head -1)
    if [ -z "$SKIPPED" ] || ! [ "$SKIPPED" -eq "$SKIPPED" ] 2>/dev/null; then
        SKIPPED=0
    fi
    
    # Calculate total safely
    TOTAL=$((PASSED + FAILED + SKIPPED)) 2>/dev/null || TOTAL=0
fi

# Ensure we have valid numbers (strip any whitespace/newlines)
PASSED=$(echo "$PASSED" | tr -d '\n\r ' | head -1)
FAILED=$(echo "$FAILED" | tr -d '\n\r ' | head -1)
SKIPPED=$(echo "$SKIPPED" | tr -d '\n\r ' | head -1)
TOTAL=$(echo "$TOTAL" | tr -d '\n\r ' | head -1)

# Set defaults if empty or invalid
PASSED=${PASSED:-0}
FAILED=${FAILED:-0}
SKIPPED=${SKIPPED:-0}
if [ -z "$TOTAL" ] || [ "$TOTAL" = "" ]; then
    TOTAL=$((PASSED + FAILED + SKIPPED))
fi

# Validate that they are numbers
if ! [ "$PASSED" -eq "$PASSED" ] 2>/dev/null; then PASSED=0; fi
if ! [ "$FAILED" -eq "$FAILED" ] 2>/dev/null; then FAILED=0; fi
if ! [ "$SKIPPED" -eq "$SKIPPED" ] 2>/dev/null; then SKIPPED=0; fi
if ! [ "$TOTAL" -eq "$TOTAL" ] 2>/dev/null; then TOTAL=$((PASSED + FAILED + SKIPPED)); fi

# Ensure OUTPUT_FILE path is valid (sanitize if needed)
OUTPUT_FILE_DIR=$(dirname "$OUTPUT_FILE" 2>/dev/null || echo ".")
mkdir -p "$OUTPUT_FILE_DIR" 2>/dev/null || true

# Output JSON using printf to avoid heredoc issues
printf '{"passed":%d,"failed":%d,"skipped":%d,"total":%d}\n' "$PASSED" "$FAILED" "$SKIPPED" "$TOTAL" > "$OUTPUT_FILE"

echo "Parsed test results: Passed=$PASSED, Failed=$FAILED, Skipped=$SKIPPED, Total=$TOTAL"
