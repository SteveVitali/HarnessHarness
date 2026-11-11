#!/usr/bin/env bash
# check-fp-partition.sh — recompute legs for the hh-fp-partition/1
# record set (manifest 303/R6.42 Stage 1). Every leg is offline, $0,
# and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-partition-leg.$$.log 2>&1; then
    echo "fp-partition: $name OK"
  else
    echo "fp-partition: $name FAIL"
    cat /tmp/fp-partition-leg.$$.log
    FAIL=1
  fi
}

# 1 — the record hashes reproduce byte-identically from the committed
#     canonical JSON (the committed recompute commands).
leg "partition-hash" shasum -a 256 -c docs/build/reports/2026-11-11-fp-partition.sha256
leg "dev-projection-hash" shasum -a 256 -c docs/build/reports/2026-11-11-fp-dev-projection.sha256

# 2 — the emitter recomputes the whole record set byte-identically into
#     a scratch tree (same committed inputs, no clock, no network). The
#     docs/ relpaths live in scripts/fp-partition-inputs.txt (the reader
#     rule — the crate reads them from the manifest, never literals).
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH" /tmp/fp-partition-leg.$$.log' EXIT
MANIFEST="$REPO/scripts/fp-partition-inputs.txt"
for name in packet select_commit exclusion; do
  rel="$(grep -E "^${name}=" "$MANIFEST" | cut -d= -f2-)"
  mkdir -p "$SCRATCH/$(dirname "$rel")"
  cp "$REPO/$rel" "$SCRATCH/$rel"
done
mkdir -p "$SCRATCH/scripts"
cp "$MANIFEST" "$SCRATCH/scripts/fp-partition-inputs.txt"
cargo build -p hharness-runner --bin fp_partition -q
BIN="$REPO/target/debug/fp_partition"
HH_FP_PARTITION_INPUTS="$SCRATCH/scripts/fp-partition-inputs.txt" "$BIN" emit "$SCRATCH" >/dev/null
for out in \
  docs/build/reports/2026-11-11-fp-partition.json \
  docs/build/reports/2026-11-11-fp-partition.sha256 \
  docs/build/reports/2026-11-11-fp-dev-projection.json \
  docs/build/reports/2026-11-11-fp-dev-projection.sha256; do
  leg "emit-identical:$(basename "$out")" cmp -s "$REPO/$out" "$SCRATCH/$out"
done

# 3 — the records are canonical Json::to_canonical_string output.
leg "canonical-roundtrip" python3 - "$REPO" <<'PY'
import json, sys
for rel in ("docs/build/reports/2026-11-11-fp-partition.json",
            "docs/build/reports/2026-11-11-fp-dev-projection.json"):
    raw = open(sys.argv[1] + "/" + rel, "rb").read().decode()
    p = json.loads(raw)
    s = json.dumps(p, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    assert raw == s + "\n" or raw == s, rel + " bytes are not canonical JSON"
PY

# 4 — the quote input manifest refuses outcome fields (REQ-XH-011):
#     the W2 hh-compute-decl/1 declaration's member set is closed — a
#     planted verdict/outcome member refuses typed, at top level and
#     inside quote_inputs.
leg "quote-decl-refuses-outcome" python3 - "$REPO" <<'PY'
import json, subprocess, sys
decl = json.load(open(sys.argv[1] + "/scripts/gce-backend/declarations/fp-w2.json"))
for where, mutated in (
    ("top-level", dict(decl, verdict="executed{resolved}")),
    ("quote_inputs", dict(decl, quote_inputs=dict(decl["quote_inputs"], outcome="x"))),
):
    path = sys.argv[1] + "/target/fp-partition-decl-fixture.json"
    open(path, "w").write(json.dumps(mutated))
    r = subprocess.run(
        ["python3", "-I", sys.argv[1] + "/scripts/gce-backend/validate_decl.py", path],
        capture_output=True, text=True)
    assert r.returncode == 2 and "Refused{decl." in r.stderr, \
        (where, r.returncode, r.stderr)
    assert "unknown_field" in r.stderr, (where, r.stderr)
print("validate_decl.py refused outcome members at both depths")
PY

# 5 — the committed dev projection carries no gate-half task content
#     (REQ-XH-121's sealed boundary; digests bind, they do not leak).
leg "dev-projection-boundary" python3 - "$REPO" <<'PY'
import json, sys
part = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-11-fp-partition.json"))
proj = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-11-fp-dev-projection.json"))
by = {p["name"]: p for p in part["partitions"]}
tasks = lambda p: {m.get(k) for m in p["members"] for k in ("member", "drawn") if m.get(k)}
record_bytes = json.dumps(proj["record"], sort_keys=True)
for g in ("gate_a", "gate_b"):
    for t in tasks(by[g]):
        assert t not in record_bytes, "gate-half task " + t + " in the dev projection"
    assert proj["gate_digests"][g] == by[g]["sha256"], g + " digest mismatch"
assert proj["record"]["members"] == by["dev"]["members"], "dev projection members != dev partition"
assert proj["record"]["name"] == "dev"
print("dev projection carries the dev half only; gate digests bound")
PY

# 6 — the acceptance battery.
leg "tests" cargo test -p hharness-runner --test r6_42 -q

if [ "$FAIL" -ne 0 ]; then
  echo "fp-partition: FAIL"
  exit 1
fi
echo "fp-partition: all legs OK"
