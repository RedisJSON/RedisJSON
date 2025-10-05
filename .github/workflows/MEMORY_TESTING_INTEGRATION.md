# Memory Testing GitHub Workflows Integration

## What Was Added

### New Workflow Files

#### 1. `flow-memory-regression.yml`
**Purpose**: Run memory regression tests on every PR

**Features**:
- Enforces memory budgets
- **Fails PR if memory exceeds limits**
- Runs on ubuntu-latest (configurable)
- Uploads logs on failure

**Triggered by**: `event-ci.yml` (on every PR)

#### 2. `flow-memory-nightly.yml`
**Purpose**: Generate nightly memory overhead reports

**Features**:
- Tracks memory overhead trends (e.g., "2.49x")
- Outputs machine-readable metrics
- Creates GitHub issue if overhead > 2.7x
- Uploads reports as artifacts (90 day retention)
- Adds summary to GitHub Actions UI

**Triggered by**: `event-nightly.yml` (daily at 20:20 UTC)

### Modified Workflow Files

#### `event-ci.yml` (Pull Request Workflow)
**Added**:
```yaml
memory-regression:
  needs: [prepare-values, docs-only]
  if: ${{ needs.docs-only.outputs.only-docs-changed == 'false' && !github.event.pull_request.draft }}
  uses: ./.github/workflows/flow-memory-regression.yml
  with:
    redis-ref: ${{needs.prepare-values.outputs.redis-ref}}
    os: ubuntu-latest
  secrets: inherit
```

**Effect**: Memory regression tests now run on every PR (except docs-only changes and drafts)

#### `event-nightly.yml` (Nightly Workflow)
**Added**:
```yaml
memory-nightly-report:
  needs: [prepare-values]
  uses: ./.github/workflows/flow-memory-nightly.yml
  with:
    redis-ref: ${{needs.prepare-values.outputs.redis-ref}}
    beta-version: ${{needs.prepare-values.outputs.beta-version}}
  secrets: inherit
```

**Effect**: Nightly memory report runs every night at 20:20 UTC

## How It Works

### On Every PR

```
┌─────────────────────────────────────────┐
│         Pull Request Created            │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│      event-ci.yml triggers              │
├─────────────────────────────────────────┤
│  • linux tests                          │
│  • sanitizer tests                      │
│  • coverage tests                       │
│  • linter                               │
│  • memory-regression ← NEW              │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│  flow-memory-regression.yml runs        │
├─────────────────────────────────────────┤
│  1. Build RedisJSON                     │
│  2. Run test_memory_regression.py       │
│  3. Check against budgets               │
│  4. ✅ Pass or ❌ Fail                  │
└─────────────────────────────────────────┘
                  ↓
          ✅ PR can merge
          (if all tests pass)
```

### Nightly (20:20 UTC)

```
┌─────────────────────────────────────────┐
│      Nightly Schedule Triggers          │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│    event-nightly.yml triggers           │
├─────────────────────────────────────────┤
│  • linux builds                         │
│  • macos builds                         │
│  • sanitizer tests                      │
│  • memory-nightly-report ← NEW          │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│  flow-memory-nightly.yml runs           │
├─────────────────────────────────────────┤
│  1. Build RedisJSON                     │
│  2. Run test_memory_nightly_report.py   │
│  3. Extract metrics (e.g., "2.49x")     │
│  4. Add to GitHub Summary               │
│  5. Upload report artifact              │
│  6. Check threshold (2.7x)              │
│  7. Create issue if exceeded            │
└─────────────────────────────────────────┘
                  ↓
      📊 Report available in:
      • GitHub Actions Summary
      • Artifacts (90 days)
      • Issue (if threshold exceeded)
```

## What You'll See

### On Pull Requests

**GitHub Actions UI**:
```
✅ linux / jammy
✅ linux-sanitizer / ubuntu:jammy
✅ linux-coverage
✅ linter
✅ memory-regression    ← NEW CHECK
```

**If memory regression detected**:
```
❌ memory-regression

Memory regression detected in 1 test(s)

❌ large_doc:
   Overhead ratio 3.15x exceeds budget 3.00x
   
ACTION REQUIRED:
  1. Investigate why memory usage increased
  2. If intentional, update MEMORY_BUDGETS
  3. Document the change in commit message
```

### On Nightly Runs

**GitHub Actions Summary**:
```
📊 Memory Overhead Report

Overall Overhead: 2.49x

| Document Size | Overhead |
|---------------|----------|
| Small | 2.28x |
| Large | 2.42x |

RedisJSON uses 2.49x more memory than regular Redis strings

📁 View full report
```

**Artifacts**:
- `memory-report-0c0a4a7e.txt` (full report)
- `memory_report_0c0a4a7e.json` (machine-readable)

**If threshold exceeded** (>2.7x):
- Automatic GitHub issue created
- Labels: `performance`, `memory`, `needs-investigation`

## Viewing Results

### PR Memory Regression Results

1. Go to your PR
2. Scroll to "Checks" section
3. Look for ✅ or ❌ next to "memory-regression"
4. Click "Details" to see full output

### Nightly Report Results

1. Go to Actions tab
2. Click "Event Nightly" workflow
3. Click latest run
4. See "Memory Overhead Report" in summary
5. Download artifacts for detailed analysis

### Downloading Reports

```bash
# Using GitHub CLI
gh run download <run-id> -n memory-report-<commit>

# Or via web UI
Actions → Event Nightly → Latest run → Artifacts → Download
```

## Monitoring Integration

### Extract Metrics from Nightly Report

The report outputs machine-readable metrics:

```bash
# From downloaded artifact
grep "METRIC:overall_overhead_ratio:" memory-report.txt | cut -d: -f3
# Output: 2.4943
```

### Send to Monitoring System

Example for InfluxDB:
```bash
OVERHEAD=$(grep "METRIC:overall_overhead_ratio:" memory-report.txt | cut -d: -f3)
curl -X POST "https://influx.example.com/write?db=redisjson" \
  --data-binary "memory_overhead,version=${VERSION},branch=${BRANCH} value=${OVERHEAD}"
```

Example for Datadog:
```bash
OVERHEAD=$(grep "METRIC:overall_overhead_ratio:" memory-report.txt | cut -d: -f3)
curl -X POST "https://api.datadoghq.com/api/v1/series?api_key=${DD_API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{\"series\":[{\"metric\":\"redisjson.memory.overhead\",\"points\":[[$(date +%s),${OVERHEAD}]]}]}"
```

## Customization

### Change Threshold

Edit `flow-memory-nightly.yml`:
```yaml
- name: Check for Regression
  run: |
    THRESHOLD=2.7  # Change this value
```

### Run on Different OS

Edit `event-ci.yml`:
```yaml
memory-regression:
  uses: ./.github/workflows/flow-memory-regression.yml
  with:
    os: macos-latest  # Change to macos-latest or windows-latest
```

### Change Nightly Schedule

Edit `event-nightly.yml`:
```yaml
on:
  schedule:
    - cron: '0 2 * * *'  # Change to 2 AM UTC
```

### Disable Auto-Issue Creation

Comment out in `flow-memory-nightly.yml`:
```yaml
# - name: Create Issue on Regression
#   if: steps.check.outputs.regression == 'true'
#   uses: actions/github-script@v7
#   ...
```

## Updating Memory Budgets

When memory usage changes intentionally:

1. **Update budgets** in `tests/pytest/test_memory_regression.py`:
   ```python
   MEMORY_BUDGETS = {
       # Increased from 3.0 to 3.2 due to new feature (PR #123)
       'large_doc': 3.2,  # Was 3.0
   }
   ```

2. **Commit with explanation**:
   ```
   Update memory budget for large documents
   
   - Increased from 3.0x to 3.2x (6% increase)
   - Reason: Added path caching for faster queries
   - Trade-off: 200 bytes/doc for 3x query speedup
   - Approved in: #123
   ```

3. **PR will pass** with new budgets

## Troubleshooting

### Memory Regression Test Fails

1. **Check the logs**:
   - Click "Details" on failed check
   - Look for which document type exceeded budget

2. **Run locally**:
   ```bash
   cd tests/pytest
   TEST=test_memory_regression.py bash tests.sh
   ```

3. **Investigate**:
   ```bash
   # Run detailed analysis
   TEST=test_memory_overhead.py bash tests.sh
   ```

### Nightly Report Not Running

1. Check if workflow is enabled
2. Check cron schedule
3. Verify permissions (needs `id-token: write`)

### Issue Not Created on Regression

1. Check if threshold was actually exceeded
2. Verify GitHub token has issue creation permissions
3. Check workflow logs for errors

## Summary

✅ **What's Integrated**:
- Memory regression tests on every PR
- Nightly memory overhead reports
- Automatic issue creation on regressions
- GitHub Actions UI integration
- Artifact storage (90 days)

✅ **What You Get**:
- Prevent memory regressions before merge
- Track "2.49x overhead" metric over time
- Automatic alerts when threshold exceeded
- Historical reports for trend analysis

✅ **Zero Configuration**:
- Works out of the box
- Uses existing test infrastructure
- Follows your existing patterns
- No additional services required
