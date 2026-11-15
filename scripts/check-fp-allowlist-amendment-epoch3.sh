#!/usr/bin/env bash
# check-fp-allowlist-amendment-epoch3.sh — offline legs for the dated
# fp-w1-q-x55 allowlist amendment (epoch 2 → epoch 3; declared data,
# never machinery). Every leg is $0 and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0
ART="docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json"
SIDE="docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.sha256"
E2="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.json"
E2SIDE="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.sha256"

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-allowlist-e3-leg.$$.log 2>&1; then
    echo "fp-allowlist-amendment-epoch3: $name OK"
  else
    echo "fp-allowlist-amendment-epoch3: $name FAIL"
    cat /tmp/fp-allowlist-e3-leg.$$.log
    FAIL=1
  fi
}

# 1 — the committed bytes hash to the recorded sidecar (the committed
#     recompute command).
leg "artifact-hash" shasum -a 256 -c "$SIDE"

# 2 — the successor record's schema members, epoch and digest recompute
#     through the idp/1 framing over the canonical list document, and
#     the file is canonical sorted-key compact JSON.
leg "freeze-record" python3 - "$REPO" <<'PY'
import hashlib, json, sys
repo = sys.argv[1]
raw = open(repo + "/docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json", "rb").read().decode()
r = json.loads(raw)
assert r["schema"] == "hh-egress-allowlist-freeze/1", r.get("schema")
assert r["protocol_id"] == "fp-w1-q-x55", r["protocol_id"]
assert r["epoch"] == 3, r["epoch"]
assert set(r) == {"schema", "protocol_id", "epoch", "list", "digest"}, set(r)
# canonical bytes — sorted-key compact + trailing newline.
assert raw == json.dumps(r, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"
# idp/1 framing: sha256("idp/1" ∥ 0x1f ∥ domain ∥ 0x1f ∥ payload).
payload = json.dumps(r["list"], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
pre = b"idp/1" + b"\x1f" + b"hh-egress-allowlist-freeze/1.list" + b"\x1f" + payload
assert "sha256:" + hashlib.sha256(pre).hexdigest() == r["digest"], r["digest"]
# every member list is sorted (canonical order) with no duplicates.
for k in ("agent", "verify", "permanent_denies"):
    v = r["list"][k]
    assert v == sorted(set(v)), k
assert r["list"]["withheld"] == []
PY

# 3 — the delta vs the epoch-2 list is exactly
#     release-assets.githubusercontent.com in VERIFY ONLY (the uv-install
#     302 target); agent/permanent_denies/withheld are byte-identical.
leg "widening-delta" python3 - "$REPO" <<'PY'
import json, sys
repo = sys.argv[1]
r3 = json.load(open(repo + "/docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json"))
r2 = json.load(open(repo + "/docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.json"))
assert r2["protocol_id"] == "fp-w1-q-x55" and r2["epoch"] == 2
l2, l3 = r2["list"], r3["list"]
ADDED = {"release-assets.githubusercontent.com"}
assert set(l3["verify"]) - set(l2["verify"]) == ADDED, set(l3["verify"]) - set(l2["verify"])
assert set(l2["verify"]) <= set(l3["verify"]), "narrowed!"
assert l3["agent"] == l2["agent"], "agent list changed"
assert l3["permanent_denies"] == l2["permanent_denies"]
assert l3["withheld"] == l2["withheld"]
# the Q-X55 posture stands: no github.com-family host in agent.
for h in l3["agent"]:
    assert not h.endswith("github.com"), h
PY

# 4 — the committed VERIFY_MIRROR_HOSTS generator names the added host
#     (generator/freeze agreement — the repair commit carries it).
leg "generator-agreement" grep -n 'release-assets.githubusercontent.com' crates/hh-env/src/egress_allowlist.rs

# 5 — the frozen predecessors are untouched: the epoch-2 artifact still
#     hashes to its committed sidecar, and the prereg's allowlist_freeze
#     citation still names the epoch-1 digest.
leg "epoch2-frozen" shasum -a 256 -c "$E2SIDE"
leg "epoch1-frozen" python3 - "$REPO" <<'PY'
import json, sys
p = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json"))
f = p["allowlist_freeze"]
assert f["protocol_id"] == "fp-w1-q-x55" and f["epoch"] == 1
assert f["digest"] == "sha256:b62762aeda7ad7cc3f97248b455d40545ff3849b83c1f5bfe75a36a5bcdba2ce", f["digest"]
PY

# 6 — the amendment record exists and names both epochs' digests, the
#     scope decision, the added host, the duplicate-Host diagnosis and
#     the declared-data classification.
leg "amendment-record" python3 - "$REPO" <<'PY'
import sys
md = open(sys.argv[1] + "/docs/build/reports/2026-11-14-fp-w1-q-x55-amendment-epoch3.md").read()
for needle in (
    "Wc7",
    "sha256:86def07e2e8402c3aa0d2332d97abaed2b5d189c9ba3c1562afc1daf10712f98",
    "sha256:787db9e1635a6ae8ebf5d50c88a1d4e33ba6fd3707f5a09dc20dee9398f6de21",
    "epoch: 3",
    "release-assets.githubusercontent.com",
    "Host",
    "302",
    "declared",
):
    assert needle in md, needle
PY

if [ "$FAIL" -ne 0 ]; then
  echo "fp-allowlist-amendment-epoch3: FAIL"
  exit 1
fi
echo "fp-allowlist-amendment-epoch3: all legs OK"
