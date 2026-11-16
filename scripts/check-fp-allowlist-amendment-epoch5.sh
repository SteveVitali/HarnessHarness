#!/usr/bin/env bash
# check-fp-allowlist-amendment-epoch5.sh — offline legs for the dated
# fp-w1-q-x55 allowlist amendment (epoch 4 → epoch 5; declared data,
# never machinery). The epoch-5 additions are RUNTIME-OBSERVED hosts
# (smoke1's deny evidence + the diag cell's progression), NOT
# scan-derived — the legs assert that provenance honestly. Every leg
# is $0 and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0
ART="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json"
SIDE="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.sha256"
E4="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json"
E4SIDE="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.sha256"
E3SIDE="docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.sha256"
E2SIDE="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.sha256"
SMOKE1="docs/build/reports/smoke/2026-11-16-smoke1.json"

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-allowlist-e5-leg.$$.log 2>&1; then
    echo "fp-allowlist-amendment-epoch5: $name OK"
  else
    echo "fp-allowlist-amendment-epoch5: $name FAIL"
    cat /tmp/fp-allowlist-e5-leg.$$.log
    FAIL=1
  fi
}

# 1 — the committed bytes hash to the recorded sidecar.
leg "artifact-hash" shasum -a 256 -c "$SIDE"

# 2 — the successor record's schema members, epoch and digest recompute
#     through the idp/1 framing over the canonical list document, and
#     the file is canonical sorted-key compact JSON.
leg "freeze-record" python3 - "$REPO" <<'PY'
import hashlib, json, sys
repo = sys.argv[1]
raw = open(repo + "/docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json", "rb").read().decode()
r = json.loads(raw)
assert r["schema"] == "hh-egress-allowlist-freeze/1", r.get("schema")
assert r["protocol_id"] == "fp-w1-q-x55", r["protocol_id"]
assert r["epoch"] == 5, r["epoch"]
assert set(r) == {"schema", "protocol_id", "epoch", "list", "digest"}, set(r)
assert raw == json.dumps(r, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"
payload = json.dumps(r["list"], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
pre = b"idp/1" + b"\x1f" + b"hh-egress-allowlist-freeze/1.list" + b"\x1f" + payload
assert "sha256:" + hashlib.sha256(pre).hexdigest() == r["digest"], r["digest"]
for k in ("agent", "verify", "permanent_denies"):
    v = r["list"][k]
    assert v == sorted(set(v)), k
assert r["list"]["withheld"] == []
PY

# 3 — the delta vs the epoch-4 list is exactly the three runtime-
#     observed hosts in VERIFY ONLY; agent/permanent_denies/withheld
#     are byte-identical (X-06: verify-phase additions only).
leg "widening-delta" python3 - "$REPO" <<'PY'
import json, sys
repo = sys.argv[1]
r5 = json.load(open(repo + "/docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json"))
r4 = json.load(open(repo + "/docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json"))
assert r4["protocol_id"] == "fp-w1-q-x55" and r4["epoch"] == 4
l4, l5 = r4["list"], r5["list"]
ADDED = {"download-r2.pytorch.org", "ossci-datasets.s3.amazonaws.com",
         "yann.lecun.com"}
assert set(l5["verify"]) - set(l4["verify"]) == ADDED, \
    set(l5["verify"]) - set(l4["verify"])
assert set(l4["verify"]) <= set(l5["verify"]), "narrowed!"
assert l5["agent"] == l4["agent"], "agent list changed"
assert l5["permanent_denies"] == l4["permanent_denies"]
assert l5["withheld"] == l4["withheld"]
for h in l5["agent"]:
    assert not h.endswith("github.com"), h
PY

# 4 — the D16 mechanism holds: every scanned host is granted in
#     epoch-5 (the standing drift guard), AND the honest provenance
#     leg — the mechanical scan does NOT name the three added hosts
#     (they are runtime-observed, not scan-derived; a scan that named
#     them would contradict the recorded provenance).
leg "member-scan" bash scripts/egress-conformance/scan-verify.sh "$ART"
leg "runtime-observed-not-scan-derived" bash -c '
  for h in download-r2.pytorch.org ossci-datasets.s3.amazonaws.com yann.lecun.com; do
    if grep -qx "$h" scripts/egress-conformance/derived/scanned-hosts.txt; then
      echo "SCAN-NAMED $h — provenance contradiction"; exit 1
    fi
  done'

# 5 — the frozen predecessors are untouched: epoch-4, epoch-3 and
#     epoch-2 artifacts still hash to their committed sidecars, and the
#     prereg's allowlist_freeze citation still names the epoch-1 digest.
leg "epoch4-frozen" shasum -a 256 -c "$E4SIDE"
leg "epoch3-frozen" shasum -a 256 -c "$E3SIDE"
leg "epoch2-frozen" shasum -a 256 -c "$E2SIDE"
leg "epoch1-frozen" python3 - "$REPO" <<'PY'
import json, sys
p = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json"))
f = p["allowlist_freeze"]
assert f["protocol_id"] == "fp-w1-q-x55" and f["epoch"] == 1
assert f["digest"] == "sha256:b62762aeda7ad7cc3f97248b455d40545ff3849b83c1f5bfe75a36a5bcdba2ce", f["digest"]
PY

# 6 — the runtime-observed provenance is evidenced by the committed
#     smoke1 record: the H-allow cell names download-r2.pytorch.org in
#     denied_hosts; the diag cell names the two deeper hosts.
leg "smoke1-deny-evidence" python3 - "$REPO" <<'PY'
import json, sys
r = json.load(open(sys.argv[1] + "/docs/build/reports/smoke/2026-11-16-smoke1.json"))
cells = r["cells"]
h_allow = cells["pytorch-model-cli/H-allow/r0"]
assert h_allow["denied_hosts"] == ["download-r2.pytorch.org"], h_allow["denied_hosts"]
assert h_allow["classification"] == "verifier_unavailable:bootstrap:egress_denied"
diag = cells["pytorch-model-cli/H-allow-diag/r0"]
assert diag["denied_hosts"] == ["ossci-datasets.s3.amazonaws.com", "yann.lecun.com"], \
    diag["denied_hosts"]
assert diag["classification"] == "verifier_unavailable:bootstrap:egress_denied"
PY

# 7 — the amendment record exists and names both epochs' digests, the
#     runtime-observed provenance, the honest authority note (exceeds
#     R-6's scan-derived-only), the derivation follow-up and the
#     declared-data classification.
leg "amendment-record" python3 - "$REPO" <<'PY'
import sys
md = open(sys.argv[1] + "/docs/build/reports/2026-11-16-fp-w1-q-x55-amendment-epoch5.md").read()
for needle in (
    "R-6",
    "sha256:785d709206243d4f2ba75f2a98cb0774050b8e42b1d0eea5f1269dc291b84203",
    "sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328",
    "epoch: 5",
    "download-r2.pytorch.org",
    "ossci-datasets.s3.amazonaws.com",
    "yann.lecun.com",
    "runtime-observed",
    "not scan-derived",
    "DF-R6.45l-1",
    "declared",
):
    assert needle in md, needle
PY

# 8 — the derivation follow-up is recorded: the DEFERRALS row exists.
leg "deferral-row" bash -c '
  grep -q "DF-R6.45l-1" docs/tickets/DEFERRALS.md'

if [ "$FAIL" -ne 0 ]; then
  echo "fp-allowlist-amendment-epoch5: FAIL"
  exit 1
fi
echo "fp-allowlist-amendment-epoch5: all legs OK"
