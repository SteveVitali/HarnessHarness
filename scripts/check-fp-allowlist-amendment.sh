#!/usr/bin/env bash
# check-fp-allowlist-amendment.sh — offline legs for the dated
# fp-w1-q-x55 allowlist amendment (epoch 1 → epoch 2; declared data,
# never machinery). Every leg is $0 and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0
ART="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.json"
SIDE="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.sha256"

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-allowlist-leg.$$.log 2>&1; then
    echo "fp-allowlist-amendment: $name OK"
  else
    echo "fp-allowlist-amendment: $name FAIL"
    cat /tmp/fp-allowlist-leg.$$.log
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
raw = open(repo + "/docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.json", "rb").read().decode()
r = json.loads(raw)
assert r["schema"] == "hh-egress-allowlist-freeze/1", r.get("schema")
assert r["protocol_id"] == "fp-w1-q-x55", r["protocol_id"]
assert r["epoch"] == 2, r["epoch"]
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

# 3 — the delta vs the epoch-1 list is exactly the three Ubuntu noble
#     apt hosts in BOTH agent and verify; permanent_denies is unchanged.
leg "widening-delta" python3 - "$REPO" <<'PY'
import json, sys
repo = sys.argv[1]
r2 = json.load(open(repo + "/docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.json"))
# the epoch-1 list verbatim (as committed in the fp-wc4 prove-held
# re-proof packet's freeze member — the frozen datum, never rewritten).
p1 = json.load(open(repo + "/docs/build/reports/compute/harvest/fp-wc4/run-root/prove-held/reproof-packet.json"))["freeze"]
assert p1["protocol_id"] == "fp-w1-q-x55" and p1["epoch"] == 1
l1, l2 = p1["list"], r2["list"]
UBUNTU = {"archive.ubuntu.com", "security.ubuntu.com", "ports.ubuntu.com"}
assert set(l2["agent"]) - set(l1["agent"]) == UBUNTU, set(l2["agent"]) - set(l1["agent"])
assert set(l2["verify"]) - set(l1["verify"]) == UBUNTU, set(l2["verify"]) - set(l1["verify"])
assert set(l1["agent"]) <= set(l2["agent"]) and set(l1["verify"]) <= set(l2["verify"]), "narrowed!"
assert l2["permanent_denies"] == l1["permanent_denies"]
assert l2["withheld"] == l1["withheld"]
# the previously-denied hosts are now covered in both phases.
for h in ("archive.ubuntu.com", "security.ubuntu.com"):
    assert h in l2["agent"] and h in l2["verify"], h
PY

# 4 — the frozen epoch-1 datum is untouched: the prereg's
#     allowlist_freeze citation still names the epoch-1 digest.
leg "epoch1-frozen" python3 - "$REPO" <<'PY'
import json, sys
p = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json"))
f = p["allowlist_freeze"]
assert f["protocol_id"] == "fp-w1-q-x55" and f["epoch"] == 1
assert f["digest"] == "sha256:b62762aeda7ad7cc3f97248b455d40545ff3849b83c1f5bfe75a36a5bcdba2ce", f["digest"]
PY

# 5 — the amendment record exists and names both epochs' digests, the
#     scope decision, and the declared-data classification.
leg "amendment-record" python3 - "$REPO" <<'PY'
import sys
md = open(sys.argv[1] + "/docs/build/reports/2026-11-13-fp-w1-q-x55-amendment.md").read()
for needle in (
    "Widen allowlist + Wc5",
    "sha256:b62762aeda7ad7cc3f97248b455d40545ff3849b83c1f5bfe75a36a5bcdba2ce",
    "sha256:86def07e2e8402c3aa0d2332d97abaed2b5d189c9ba3c1562afc1daf10712f98",
    "epoch: 2",
    "archive.ubuntu.com",
    "security.ubuntu.com",
    "ports.ubuntu.com",
    "declared",
):
    assert needle in md, needle
PY

if [ "$FAIL" -ne 0 ]; then
  echo "fp-allowlist-amendment: FAIL"
  exit 1
fi
echo "fp-allowlist-amendment: all legs OK"
