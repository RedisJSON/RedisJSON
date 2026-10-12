#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
export GUNGRAUN_HOME="${GUNGRAUN_HOME:-$PWD/target/gungraun-instructions}"
mkdir -p "$GUNGRAUN_HOME/bin"
GUNGRAUN_HOME="$(cd "$GUNGRAUN_HOME" && pwd)"

cargo bench -p json_path --bench path_performance --locked --no-run \
    --message-format=json > "$GUNGRAUN_HOME/build.jsonl"

# Use the same executable path across builds: even argv path lengths can change
# allocator state before collection starts. Run comparisons sequentially.
python3 - "$GUNGRAUN_HOME" <<'PY'
import json
import shutil
import sys
from pathlib import Path

output = Path(sys.argv[1])
artifacts = [json.loads(line) for line in (output / "build.jsonl").read_text().splitlines()]
executables = [
    artifact["executable"] for artifact in artifacts
    if artifact.get("reason") == "compiler-artifact"
    and artifact["target"]["name"] == "path_performance"
    and artifact.get("executable")
]
if len(executables) != 1:
    raise SystemExit(f"Expected one benchmark executable, found {len(executables)}")
shutil.copy2(executables[0], output / "bin" / "path_performance")
PY

exec "$GUNGRAUN_HOME/bin/path_performance" --bench "$@"
