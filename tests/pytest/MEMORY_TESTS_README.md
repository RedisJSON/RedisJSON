# Memory Overhead Tests for RedisJSON

## Overview

These tests track and prevent memory regressions in RedisJSON by comparing memory usage against regular Redis strings.

**Current Result: RedisJSON uses ~2.5x more memory than regular Redis strings**

## Test Files

### 1. `test_memory_regression.py` ⚠️ **Run on Every PR**
**Purpose**: Prevent memory regressions in CI/CD

- Enforces memory budgets per document type
- **Fails the build** if budgets are exceeded
- Runs fast (~20 seconds)

**Usage**:
```bash
# Run all regression tests
TEST=test_memory_regression.py bash tests.sh

# Run specific test
TEST=test_memory_regression.py::test_memory_regression_all_sizes bash tests.sh
```

### 2. `test_memory_overhead.py` 📊 **Run On-Demand**
**Purpose**: Detailed memory analysis

- Comprehensive overhead comparison
- Multiple document sizes and types
- Educational output for developers

**Usage**:
```bash
TEST=test_memory_overhead.py bash tests.sh
```

### 3. `test_memory_nightly_report.py` 📈 **Run Nightly**
**Purpose**: Track memory trends over time

- Generates headline metrics (e.g., "2.49x overhead")
- Machine-readable output for monitoring
- Never fails - just reports

**Usage**:
```bash
TEST=test_memory_nightly_report.py::test_nightly_memory_report bash tests.sh
```

## Integration with Existing Test Structure

### Unit Tests (Rust)
```bash
cargo test
```
*No memory tests here - these are in flow tests*

### Flow Tests (Python/RLTest)
```bash
# Run all flow tests (includes memory regression)
make pytest

# Run only memory tests
TEST=test_memory_regression.py bash tests/pytest/tests.sh
```

## CI/CD Integration

### Add to `.github/workflows/`

Update your existing workflow to include memory regression tests:

```yaml
# In your existing test workflow
- name: Run tests
  run: |
    echo ::group::Unit tests
      cargo test
    echo ::endgroup::
    echo ::group::Flow tests
      make pytest
    echo ::endgroup::
    echo ::group::Memory regression tests
      cd tests/pytest
      TEST=test_memory_regression.py bash tests.sh
    echo ::endgroup::
```

### Add Nightly Job

Create `.github/workflows/nightly-memory-report.yml`:

```yaml
name: Nightly Memory Report

on:
  schedule:
    - cron: '0 2 * * *'  # 2 AM UTC
  workflow_dispatch:

jobs:
  memory-report:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup
        run: |
          python3 -m venv venv
          source venv/bin/activate
          ./.install/common_installations.sh
      
      - name: Build
        run: |
          . "$HOME/.cargo/env"
          cargo build --release
      
      - name: Run Memory Report
        env:
          REDISJSON_VERSION: ${{ github.ref_name }}
          GIT_COMMIT: ${{ github.sha }}
          GIT_BRANCH: ${{ github.ref_name }}
          BUILD_NUMBER: ${{ github.run_number }}
        run: |
          source venv/bin/activate
          cd tests/pytest
          TEST=test_memory_nightly_report.py::test_nightly_memory_report \
            bash tests.sh ../../target/release/rejson.so | tee ../../memory_report.txt
      
      - name: Extract Metrics
        id: metrics
        run: |
          OVERHEAD=$(grep "METRIC:overall_overhead_ratio:" memory_report.txt | cut -d: -f3)
          echo "overhead=$OVERHEAD" >> $GITHUB_OUTPUT
          echo "### 📊 Memory Overhead: ${OVERHEAD}x" >> $GITHUB_STEP_SUMMARY
      
      - name: Upload Report
        uses: actions/upload-artifact@v3
        with:
          name: memory-report-${{ github.sha }}
          path: memory_report.txt
          retention-days: 90
```

## Memory Budgets

Current budgets in `test_memory_regression.py`:

```python
MEMORY_BUDGETS = {
    'tiny_doc': 6.0,      # Max 6x overhead for tiny docs
    'small_doc': 4.0,     # Max 4x overhead
    'medium_doc': 3.5,    # Max 3.5x overhead
    'large_doc': 3.0,     # Max 3x overhead
    'array_1000': 2.0,    # Max 2x for large arrays
}
```

### When to Update Budgets

✅ **Update when**:
- You've optimized memory (lower the budget)
- New feature justifies increase (document why)

❌ **Don't update when**:
- Tests fail unexpectedly
- You don't know why it increased

### How to Update

1. **Investigate**:
   ```bash
   TEST=test_memory_overhead.py bash tests/pytest/tests.sh
   ```

2. **Update budget** in `test_memory_regression.py`:
   ```python
   MEMORY_BUDGETS = {
       # Increased from 3.0 to 3.2 for path caching (PR #123)
       # Trade-off: 6% memory for 3x faster queries
       'large_doc': 3.2,  # Was 3.0
   }
   ```

3. **Document in commit**:
   ```
   Update memory budget for large documents
   
   - Increased from 3.0x to 3.2x (6% increase)
   - Reason: Added path caching for faster queries
   - Trade-off: 200 bytes/doc for 3x query speedup
   ```

## Quick Commands

```bash
# Run regression tests (for CI/CD)
cd tests/pytest
TEST=test_memory_regression.py bash tests.sh

# Run detailed analysis
TEST=test_memory_overhead.py bash tests.sh

# Run nightly report
TEST=test_memory_nightly_report.py::test_nightly_memory_report bash tests.sh

# Run with specific module
TEST=test_memory_regression.py bash tests.sh ../../target/release/rejson.so
```

## Makefile Integration

Add to your `Makefile`:

```makefile
# Add to existing pytest target or create new target
test-memory:
	@echo "Running memory regression tests..."
	cd tests/pytest && TEST=test_memory_regression.py bash tests.sh

test-memory-report:
	@echo "Generating memory overhead report..."
	cd tests/pytest && TEST=test_memory_nightly_report.py::test_nightly_memory_report bash tests.sh
```

Then run:
```bash
make test-memory
make test-memory-report
```

## Expected Output

### Regression Tests (Pass)
```
================================================================================
MEMORY REGRESSION TEST RESULTS
================================================================================

Test                 Bytes      JSON Mem     Ratio      Budget     Status    
----------------------------------------------------------------------------------------------------
tiny_doc             25         128          5.12       6.00x      ✓ PASS    
small_doc            120        328          2.28       4.00x      ✓ PASS    
large_doc            2,555      7,480        2.42       3.00x      ✓ PASS    

✅ All memory regression tests passed!
```

### Regression Tests (Fail)
```
❌ large_doc:
   Overhead ratio 3.15x exceeds budget 3.00x
   
ACTION REQUIRED:
  1. Investigate why memory usage increased
  2. If intentional, update MEMORY_BUDGETS
  3. Document the change in commit message
```

### Nightly Report
```
📊 OVERALL OVERHEAD: 2.49x
   (RedisJSON uses 2.49x more memory than regular Redis strings)

📈 BY DOCUMENT SIZE:
   • Small documents:  2.28x overhead
   • Medium documents: 2.76x overhead
   • Large documents:  2.42x overhead

MACHINE-READABLE METRICS:
METRIC:overall_overhead_ratio:2.4943
METRIC:small_doc_overhead:2.2778
METRIC:large_doc_overhead:2.4223
```

## Monitoring

Extract metrics from nightly report:

```bash
# Get overall overhead
grep "METRIC:overall_overhead_ratio:" report.txt | cut -d: -f3
# Output: 2.4943

# Send to monitoring system
OVERHEAD=$(grep "METRIC:overall_overhead_ratio:" report.txt | cut -d: -f3)
curl -X POST "https://influx.example.com/write?db=redisjson" \
  --data-binary "memory_overhead value=${OVERHEAD}"
```

## Summary

| Test | When | Purpose | Fails Build |
|------|------|---------|-------------|
| `test_memory_regression.py` | Every PR | Prevent regressions | ✅ Yes |
| `test_memory_overhead.py` | On-demand | Detailed analysis | ❌ No |
| `test_memory_nightly_report.py` | Nightly | Track trends | ❌ No |

**Key Takeaway**: RedisJSON uses ~2.5x more memory, but enables powerful partial updates and path-based queries that would otherwise require full document GET/parse/SET cycles.
