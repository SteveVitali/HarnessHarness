#!/usr/bin/env bash
# check-fp-allowlist-amendment-epoch4.sh — offline legs for the dated
# fp-w1-q-x55 allowlist amendment (epoch 3 → epoch 4; declared data,
# never machinery). Every leg is $0 and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0
ART="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json"
SIDE="docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.sha256"
E3="docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json"
E3SIDE="docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.sha256"
E2SIDE="docs/build/reports/2026-11-13-fp-egress-allowlist-freeze-epoch-2.sha256"

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-allowlist-e4-leg.$$.log 2>&1; then
    echo "fp-allowlist-amendment-epoch4: $name OK"
  else
    echo "fp-allowlist-amendment-epoch4: $name FAIL"
    cat /tmp/fp-allowlist-e4-leg.$$.log
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
raw = open(repo + "/docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json", "rb").read().decode()
r = json.loads(raw)
assert r["schema"] == "hh-egress-allowlist-freeze/1", r.get("schema")
assert r["protocol_id"] == "fp-w1-q-x55", r["protocol_id"]
assert r["epoch"] == 4, r["epoch"]
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

# 3 — the delta vs the epoch-3 list is exactly download.pytorch.org in
#     VERIFY ONLY (the D16 scan-confirmed gap); agent/permanent_denies/
#     withheld are byte-identical.
leg "widening-delta" python3 - "$REPO" <<'PY'
import json, sys
repo = sys.argv[1]
r4 = json.load(open(repo + "/docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json"))
r3 = json.load(open(repo + "/docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json"))
assert r3["protocol_id"] == "fp-w1-q-x55" and r3["epoch"] == 3
l3, l4 = r3["list"], r4["list"]
ADDED = {"download.pytorch.org"}
assert set(l4["verify"]) - set(l3["verify"]) == ADDED, set(l4["verify"]) - set(l3["verify"])
assert set(l3["verify"]) <= set(l4["verify"]), "narrowed!"
assert l4["agent"] == l3["agent"], "agent list changed"
assert l4["permanent_denies"] == l3["permanent_denies"]
assert l4["withheld"] == l3["withheld"]
for h in l4["agent"]:
    assert not h.endswith("github.com"), h
PY

# 4 — the D16 mechanism: the mechanical member scan names
#     download.pytorch.org and every scanned host is granted in epoch-4
#     (the standing drift guard — a future scanned-but-ungranted host
#     fails here).
leg "member-scan" bash scripts/egress-conformance/scan-verify.sh "$ART"
leg "scan-names-host" bash -c '
  grep -qx "download.pytorch.org" scripts/egress-conformance/derived/scanned-hosts.txt'

# 5 — the frozen predecessors are untouched: epoch-3 and epoch-2
#     artifacts still hash to their committed sidecars, and the prereg's
#     allowlist_freeze citation still names the epoch-1 digest.
leg "epoch3-frozen" shasum -a 256 -c "$E3SIDE"
leg "epoch2-frozen" shasum -a 256 -c "$E2SIDE"
leg "epoch1-frozen" python3 - "$REPO" <<'PY'
import json, sys
p = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json"))
f = p["allowlist_freeze"]
assert f["protocol_id"] == "fp-w1-q-x55" and f["epoch"] == 1
assert f["digest"] == "sha256:b62762aeda7ad7cc3f97248b455d40545ff3849b83c1f5bfe75a36a5bcdba2ce", f["digest"]
PY

# 6 — the amendment record exists and names both epochs' digests, the
#     R-6 authority, the added host and the declared-data classification.
leg "amendment-record" python3 - "$REPO" <<'PY'
import sys
md = open(sys.argv[1] + "/docs/build/reports/2026-11-16-fp-w1-q-x55-amendment-epoch4.md").read()
for needle in (
    "R-6",
    "sha256:787db9e1635a6ae8ebf5d50c88a1d4e33ba6fd3707f5a09dc20dee9398f6de21",
    "sha256:785d709206243d4f2ba75f2a98cb0774050b8e42b1d0eea5f1269dc291b84203",
    "epoch: 4",
    "download.pytorch.org",
    "scan_hosts_in_text",
    "declared",
):
    assert needle in md, needle
PY

if [ "$FAIL" -ne 0 ]; then
  echo "fp-allowlist-amendment-epoch4: FAIL"
  exit 1
fi
echo "fp-allowlist-amendment-epoch4: all legs OK"
