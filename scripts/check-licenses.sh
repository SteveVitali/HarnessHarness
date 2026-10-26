#!/usr/bin/env bash
# check-licenses.sh — every workspace package's manifest license is exactly
# "MIT OR Apache-2.0" (L4.1; the launch ruling of 2026-10-20, R-2.12.4's
# licensing leg). Reads the workspace manifests via `cargo metadata
# --format-version 1`; fails loudly naming every offender — never a count.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v cargo   >/dev/null || { echo "check-licenses: cargo absent" >&2; exit 2; }
command -v python3 >/dev/null || { echo "check-licenses: python3 absent" >&2; exit 2; }

python3 - <<'PY'
import json, subprocess, sys

WANT = "MIT OR Apache-2.0"
try:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1"],
        stderr=subprocess.PIPE))
except subprocess.CalledProcessError as e:
    sys.exit("check-licenses: cargo metadata failed: " + e.stderr.decode()[:400])

ws = set(meta["workspace_members"])
bad = []
for p in meta["packages"]:
    if p["id"] not in ws:
        continue
    lic = p.get("license")
    if lic != WANT:
        bad.append(f'{p["name"]} {p["version"]}: license={lic!r}')
if bad:
    print("check-licenses: FAIL — workspace packages not licensed "
          f'"{WANT}":', file=sys.stderr)
    for b in sorted(bad):
        print("  " + b, file=sys.stderr)
    sys.exit(1)
print(f'check-licenses: OK — {len(ws)} workspace packages, all "{WANT}"')
PY
