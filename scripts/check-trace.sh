#!/usr/bin/env bash
# check-trace.sh — R5.45 / REQ-A-008 + REQ-B-003 + REQ-G-002/004/006/007 +
# REQ-F-101…105 contributing: the §7.1 annotated-trace / four-family
# integration static, both-modes check. No build, no network.
#
#   check-trace.sh [ROOT]      working-tree check (default: git toplevel)
#   check-trace.sh --head [ROOT]
#                              committed-HEAD check (clean clone — uncommitted
#                              edits are never the tested HEAD).
#
# Legs: excerpts (docs/inspect-a-run.md carries the six excerpt sections,
#   each naming event classes present in the canonical class table parsed
#   from crates/hh-ledger/src/classes.rs — never mirrored); packets (both
#   cited packet_ids match the committed executed packets byte-for-byte);
#   families (docs/experiments.md carries all four families, each card's
#   `run` member verbatim); honesty (fixture labels, no causal/efficacy
#   claims, "input-required is not approval", "scratch is not sandbox",
#   adaptation-not-causal retained); links (repo-relative links resolve);
#   nav (docs/README.md names both pages in navigation + mode tables).
# The live lane is recorded, never exercised:
#   `live: skipped(…)` — ADR-0363 owns authorized live legs.
# Exit 0 = green; 1 = a leg failed; 2 = usage/malformed input.
set -euo pipefail

MODE=work
ROOT_ARG=""
for a in "$@"; do
  case "$a" in
    --head) MODE=head ;;
    --work) MODE=work ;;
    -*) echo "check-trace: usage: check-trace.sh [--head] [ROOT]" >&2; exit 2 ;;
    *) ROOT_ARG="$a" ;;
  esac
done

if [ "$MODE" = head ]; then
  SRC="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  SHA="$(git -C "$SRC" rev-parse HEAD)"
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-trace-head.XXXXXX")"
  trap 'rm -rf "$TMP"' EXIT
  git clone --quiet --no-hardlinks "$SRC" "$TMP/repo" \
    || { echo "check-trace: FAIL — git clone of $SRC failed" >&2; exit 1; }
  git -C "$TMP/repo" checkout --quiet "$SHA" \
    || { echo "check-trace: FAIL — checkout of $SHA failed in the clone" >&2; exit 1; }
  ROOT="$TMP/repo"
  LABEL="committed HEAD ${SHA:0:8} (clean clone — working-tree edits are not tested)"
else
  ROOT="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  LABEL="working tree $ROOT"
fi
export CHECK_TRACE_LABEL="$LABEL"

python3 - "$ROOT" <<'PY'
import hashlib
import json
import os
import re
import sys

root = sys.argv[1]
fails = 0

def fail(cid, msg):
    global fails
    fails += 1
    print(f"check-trace: FAIL [{cid}] {msg}", file=sys.stderr)

def ok(cid, msg):
    print(f"check-trace: ok   [{cid}] {msg}")

def read_rel(path):
    with open(os.path.join(root, path), "r", encoding="utf-8") as f:
        return f.read()

def link_failures(doc, doc_rel):
    out = []
    for target in re.findall(r"\]\(([^)\s]+)\)", doc):
        if re.match(r"[a-z]+://", target) or target.startswith("#"):
            continue
        t = target.split("#", 1)[0]
        if not t:
            continue
        p = os.path.normpath(os.path.join(os.path.dirname(doc_rel), t))
        if not os.path.exists(os.path.join(root, p)):
            out.append(f"{doc_rel}: link {target} does not resolve ({p})")
    return out

# ── canon: the registered event-class vocabulary, from its home ───────
try:
    classes_src = read_rel("crates/hh-ledger/src/classes.rs")
except OSError as e:
    print(f"check-trace: FAIL [canon] cannot read classes.rs: {e}", file=sys.stderr)
    sys.exit(1)
classes = set(re.findall(r'"([a-z][a-z_.]+)"', classes_src))

# ── excerpts: six sections, each naming real classes ──────────────────
tr_rel = "docs/inspect-a-run.md"
try:
    tr = read_rel(tr_rel)
except OSError as e:
    tr = ""
    fail("excerpts", f"cannot read {tr_rel}: {e}")
REQUIRED_CLASSES = [
    "lifecycle.run.created",
    "lifecycle.session.attached",
    "model.call.requested",
    "model.call.completed",
    "action.tool.proposed",
    "action.effect.intended",
    "security.permission.decided",
    "action.effect.authorized",
    "action.effect.prepared",
    "action.effect.committed",
    "action.effect.observed",
    "lifecycle.turn.finished",
    "lifecycle.run.finished",
    "verification.claim.recorded",
    "verification.gate.evaluated",
    "verification.completion.decided",
    "measurement.metric.emitted",
    "context.assembled",
    "lifecycle.replay.started",
    "lifecycle.replay.finished",
    "context.artefact.delivered",
]
if tr:
    for c in REQUIRED_CLASSES:
        if c not in classes:
            fail("excerpts", f"class {c!r} cited but not registered in classes.rs")
        elif c not in tr:
            fail("excerpts", f"class {c!r} not annotated in {tr_rel}")
    if "action.effect.decided" in tr:
        fail("excerpts", "nonexistent class action.effect.decided cited")
    if len(re.findall(r"^\d+\.\s+\*\*", tr, re.M)) != 6:
        fail("excerpts", "the six excerpt sections are not numbered 1–6")
    ok("excerpts", "six excerpts resolve to registered classes; no invented ordering/class")

# ── packets: cited ids match the committed packets ────────────────────
PACKETS = {
    "docs/build/reports/2026-11-04-r5.43-s1-useful.json": "hh-s1-useful/1",
    "docs/build/reports/2026-11-04-r5.44-ops-live-second.json": "hh-ops-live/1",
}
for rel, want in PACKETS.items():
    try:
        p = json.loads(read_rel(rel))
    except (OSError, json.JSONDecodeError) as e:
        fail("packets", f"{rel}: unreadable — {e}")
        continue
    if p.get("version") != want:
        fail("packets", f"{rel}: version {p.get('version')!r} != {want!r}")
    pid = p.get("packet_id", "")
    if pid not in tr:
        fail("packets", f"{tr_rel} does not cite packet_id {pid[:16]}…")
    ok("packets", f"{want} packet pinned + cited")

# ── families: all four cards named with their run commands ───────────
ex_rel = "docs/experiments.md"
try:
    ex = read_rel(ex_rel)
except OSError as e:
    ex = ""
    fail("families", f"cannot read {ex_rel}: {e}")
if ex:
    for fam in ("EX-01", "EX-02", "EX-03", "EX-04"):
        if fam not in ex:
            fail("families", f"family {fam} absent from {ex_rel}")
    cards = sorted(
        f for f in os.listdir(os.path.join(root, "scripts", "examples"))
        if re.fullmatch(r"ex-\d+\.json", f)
    )
    for cf in cards:
        try:
            card = json.loads(read_rel(f"scripts/examples/{cf}"))
        except (OSError, json.JSONDecodeError):
            continue
        run = card.get("run", "")
        if run and run not in ex:
            fail("families", f"{cf} run {run!r} not displayed in {ex_rel}")
    ok("families", "four families + canonical run commands present")

# ── honesty: labels + banned claims ───────────────────────────────────
hay = tr + "\n" + ex
for tok in ("fixture", "scratch is not sandbox", "input-required is not approval"):
    if tok.lower() not in hay.lower():
        fail("honesty", f"required label {tok!r} absent")
if not re.search(r"adaptation.{0,40}causal|adaptation — no causal", hay, re.S):
    fail("honesty", "the cap edit drops its adaptation-not-causal label")
for banned in ("causal efficacy", "proven", "guaranteed", "statistically significant"):
    if re.search(rf"\b{re.escape(banned)}\b", hay, re.I) and "not" not in hay.lower():
        fail("honesty", f"banned claim phrase {banned!r}")
if "new viewer" in hay.lower() and "no new viewer" not in hay.lower():
    fail("honesty", "viewer language drifted")
ok("honesty", "fixture labels, refusal idiom and no-causal-claim held")

# ── links ─────────────────────────────────────────────────────────────
for m in link_failures(tr, tr_rel) + link_failures(ex, ex_rel):
    fail("links", m)
ok("links", "repo-relative links resolve on touched surfaces")

# ── nav: docs/README.md names both pages ──────────────────────────────
try:
    nav = read_rel("docs/README.md")
except OSError as e:
    nav = ""
    fail("nav", f"cannot read docs/README.md: {e}")
if nav:
    for page in ("inspect-a-run.md", "experiments.md"):
        n = len(re.findall(rf"\[{re.escape(page)}\]|\({re.escape(page)}\)", nav))
        if n < 2:
            fail("nav", f"docs/README.md names {page} fewer than twice")
        if f"{page}) | living |" not in nav:
            fail("nav", f"docs/README.md mode table does not mark {page} living")
    ok("nav", "docs/README.md navigation + mode table updated")

if fails:
    print(f"check-trace: {fails} failure(s)", file=sys.stderr)
    sys.exit(1)
print(f"check-trace: OK — all legs green [{os.environ.get('CHECK_TRACE_LABEL', '')}]")
print("check-trace: live: skipped(no routine PR calls — ADR-0363 owns authorized live legs; unset allowance is zero)")
PY
rc=$?
if [ "$rc" -eq 0 ] && [ "$MODE" = head ]; then
  echo "check-trace: tested $LABEL"
fi
exit "$rc"
