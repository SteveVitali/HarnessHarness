#!/usr/bin/env bash
# runtime-verify.sh — the R6.51 runtime-evidence leg beside
# scan-verify.sh (DF-R6.45l-1): a committed
# `hh-verify-host-derivation/1` record's candidate set is checked
# against a freeze record's verify set — a runtime-observed host absent
# from verify FAILS the leg with a typed reason; the freeze is never
# silently extended and an observation is never silently dropped.
# The leg also checks the record's honesty members: `na` is either
# null (evidence seen) or exactly `n/a{no_runtime_evidence}`, and the
# excluded legs (`non_verify_denies` / `unattributed_denies`) carry
# their dispositions.
#
#   runtime-verify.sh <derivation.json> [freeze.json]
# (freeze default: newest committed epoch)
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO="$PWD"

DERIV="${1:?usage: runtime-verify.sh <derivation.json> [freeze.json]}"
FREEZE="${2:-}"
if [ -z "$FREEZE" ]; then
  FREEZE="$(ls -1 "$REPO"/docs/build/reports/*-fp-egress-allowlist-freeze-epoch-*.json | sort | tail -1)"
fi
[ -f "$DERIV" ] || { echo "runtime-verify: no derivation file $DERIV" >&2; exit 2; }
[ -f "$FREEZE" ] || { echo "runtime-verify: no freeze file" >&2; exit 2; }

python3 - "$DERIV" "$FREEZE" <<'PY'
import json, sys
deriv = json.load(open(sys.argv[1]))
freeze = json.load(open(sys.argv[2]))
assert deriv["schema"] == "hh-verify-host-derivation/1", deriv.get("schema")
assert freeze["schema"] == "hh-egress-allowlist-freeze/1", freeze.get("schema")
verify = set(freeze["list"]["verify"])
hosts = list(deriv["hosts"])
cand = {c["host"] for c in deriv["candidates"]}
assert cand == set(hosts), "candidates/hosts disagree"
na = deriv["na"]
assert na is None or na == "n/a{no_runtime_evidence}", na
missing = sorted(set(hosts) - verify)
print(f"runtime-verify: derivation {sys.argv[1]} (window {deriv['window']})")
print(f"runtime-verify: freeze {sys.argv[2]} "
      f"(protocol_id={freeze['protocol_id']} epoch={freeze['epoch']})")
print(f"runtime-verify: derived {len(hosts)} runtime-observed hosts: {hosts}")
for d in deriv.get("non_verify_denies", []):
    assert "excluded" in d["disposition"], d["disposition"]
for d in deriv.get("unattributed_denies", []):
    assert "excluded" in d["disposition"], d["disposition"]
excluded = sorted(d["host"] for d in deriv.get("non_verify_denies", []))
print(f"runtime-verify: excluded non-verify denies (never derived): {excluded}")
if na is not None and hosts:
    print("runtime-verify: FAIL — na set with a non-empty candidate set")
    sys.exit(1)
if missing:
    print(f"runtime-verify: FAIL runtime_evidence_absent_from_verify{missing} — "
          "an observed host is absent from the freeze verify set")
    sys.exit(1)
print("runtime-verify: OK — every derived runtime-observed host is granted in verify")
PY
