#!/usr/bin/env bash
# check-position.sh — R5.35 / REQ-B-001 + REQ-C-008 + REQ-C-101 + REQ-C-102:
# the §7.1 reader-spine / evidence-card / portfolio-positioning static,
# both-modes check. No build, no network.
#
#   check-position.sh [ROOT]      working-tree check (default: git toplevel)
#   check-position.sh --head [ROOT]
#                                 committed-HEAD check: clone HEAD into a
#                                 scratch dir and run the same validation
#                                 inside the clone — uncommitted edits are
#                                 never the tested HEAD (the
#                                 check-quickstart.sh precedent).
#
# Legs: entries (README <!-- spine --> block carries the three entry
#   choices once each, readiness first, plus a failure/report exit);
#   spine_page (docs/evidence.md carries the same choices in its own
#   marked block); cards (every `### Card —` section carries the exact
#   State/Scope/Evidence/Limits/Access field shape); labels (every card
#   State is in the closed Available / Not yet available / Restricted /
#   Unsupported vocabulary); gates (required card keys and states —
#   useful-task/study/operations are `Not yet available`, component/
#   interchange/evidence-access `Restricted`; fixture success never
#   substitutes for S1/S3/S4); portfolio (all 22 §1.2 family names with
#   their leading disposition class, all nine §1.3 later groups, all six
#   §1.1 scenarios and the one selected job retained — canon parsed from
#   the frozen supplement, never mirrored by hand); unmet (the
#   <!-- unmet:begin --> clause names S1/S3 unmet); sources (the
#   <!-- claims:begin --> benefit/overlap/exclusion/revisit map resolves
#   to real files); honesty (banned favorable-treatment/scientific-claim
#   phrases fail; the job keeps its inferred-demand label); links
#   (repo-relative links resolve on touched surfaces); nav (docs/README.md
#   names evidence.md in navigation and mode tables).
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
    -*) echo "check-position: usage: check-position.sh [--head] [ROOT]" >&2; exit 2 ;;
    *) ROOT_ARG="$a" ;;
  esac
done

if [ "$MODE" = head ]; then
  SRC="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  SHA="$(git -C "$SRC" rev-parse HEAD)"
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-position-head.XXXXXX")"
  trap 'rm -rf "$TMP"' EXIT
  git clone --quiet --no-hardlinks "$SRC" "$TMP/repo" \
    || { echo "check-position: FAIL — git clone of $SRC failed" >&2; exit 1; }
  git -C "$TMP/repo" checkout --quiet "$SHA" \
    || { echo "check-position: FAIL — checkout of $SHA failed in the clone" >&2; exit 1; }
  ROOT="$TMP/repo"
  LABEL="committed HEAD ${SHA:0:8} (clean clone — working-tree edits are not tested)"
else
  ROOT="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  LABEL="working tree $ROOT"
fi
export CHECK_POSITION_LABEL="$LABEL"

python3 - "$ROOT" <<'PY'
import os
import re
import sys

root = sys.argv[1]
fails = 0
results = []


def fail(cid, msg):
    global fails
    fails += 1
    results.append((cid, False))
    print(f"check-position: FAIL [{cid}] {msg}", file=sys.stderr)


def ok(cid, msg):
    results.append((cid, True))
    print(f"check-position: ok   [{cid}] {msg}")


def read_rel(path):
    with open(os.path.join(root, path), "r", encoding="utf-8") as f:
        return f.read()


def marked(doc, name):
    """Text between <!-- name:begin --> and <!-- name:end --> (or
    <!-- name --> … <!-- name:end -->)."""
    m = re.search(
        rf"<!--\s*{re.escape(name)}(?::begin)?\s*-->(.*?)<!--\s*{re.escape(name)}:end\s*-->",
        doc, re.S)
    return m.group(1) if m else ""


def link_failures(doc, doc_rel):
    """Repo-relative links in `doc` that do not resolve."""
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


ENTRIES = ["Run a useful task", "Inspect a recorded result", "Learn offline"]
STATES = ["Available", "Not yet available", "Restricted", "Unsupported"]
RECORD_REL = "docs/results/phase1-execution-surface-replay.md"
# REQ-XA-119 — the thirteen honest-bounds statements (experiments
# supplement §2.6), matched as exact substrings after whitespace
# collapse inside the record's <!-- bounds --> block.
BOUNDS = [
    "The parity tranches compared `tb run -a oracle` with a reference podman runner that is not HarnessHarness; HH cells were not a parity surface.",
    "Each tranche ran one first attempt per task per runner (R=1).",
    "`verified` is the tranche's own exact-agreement rule; the landed `parity_admission` refuses this design: on the strict records-only rung with `MissingKey{pins.rights_ref}`, and on the labelled design-probe rung with `AdapterFloor{repeats:1}`.",
    "Of the 30 agreements, 22 are both-resolved, 4 both-failed tests and 4 are pairs in which both builds failed; 26 of 30 H runs reused the O-built image.",
    "0.115703 is a descriptive concentration endpoint at N=30 with zero discordance, not a registered margin or an equivalence verdict; no margin was registered.",
    "The confirmation cohort ran through HH cells and has no original-runner comparison of its own; the records-in cross-check against tranche 3's `tb` verdicts agrees on 14 of 16 tasks with a first-attempt HH verdict and disagrees on 2; on the final fold it agrees on 26 of 30.",
    "With the oracle participant the compaction arms cannot change an outcome; the efficacy contrast is 0 by construction and is not efficacy evidence.",
    "300/300 is a four-epoch composite: 140 slots were re-attempted beyond the frozen reattempt bound, across two backends (arm64 applehv, amd64 GCE) and two network modes.",
    "Grading ran tb's `PytestParser` in scratch Python outside HH; 100 of 300 final receipts used the `vendored_offline` channel.",
    "Inference cost was $0.00 (structured in every packet); compute was a rate-card estimate of ≈$1.72 for the VM's life (16:32Z–22:15:40Z), originally recorded as ≈$1.41, recorded in prose only, billing not read; local VM time is uncosted.",
    "Benchmark edition: `terminal-bench-core==0.1.1` (80 tasks), substituted for the TB 2.0 the supplement names ([ADR-0423]",
    "The ANSI-strip and declared-workdir repairs were made in scratch drivers;",
    "The drivers that produced these packets are preserved verbatim as unmaintained provenance; the packets are recomputable records-in from the committed raw set, not re-executable from the repository.",
]
# REQ-XA-120 — record-surface bans, scanned case-insensitive and
# word-bounded over the record, evidence.md and the spine.
RECORD_BANNED = [
    "null-equivalent", "parity verified", "verified for hh",
    "proof of correctness", "proof-of-correctness",
    "equivalence verified", "efficacy result shows",
]

# ── canon from the frozen supplement (never mirrored by hand) ─────────
spec_rel = "docs/evidence-and-adoption-spec.md"
try:
    spec = read_rel(spec_rel)
except OSError as e:
    print(f"check-position: FAIL [canon] cannot read {spec_rel}: {e}", file=sys.stderr)
    sys.exit(1)

def spec_rows(sec_start, sec_end, first_cell):
    sec = spec[spec.find(sec_start):spec.find(sec_end)]
    return [l for l in sec.splitlines()
            if l.startswith("|") and "---" not in l and first_cell not in l]

fam_rows = spec_rows("### 1.2", "### 1.3", "Family |")
families = []
for r in fam_rows:
    cells = [c.strip() for c in r.split("|")]
    fam = cells[1]
    disp = cells[-2]
    m = re.search(r"\*\*(First-wave contingent|First-wave|Later)", disp)
    families.append((fam, m.group(1).lower() if m else ""))
later_rows = spec_rows("### 1.3", "### 1.4", "Later group |")
later_ids = [re.split(r"\s|—", r.split("|")[1].strip())[0] for r in later_rows]
scen_rows = spec_rows("### 1.1", "### 1.2", "Scenario |")
scen_ids = [r.split("|")[1].strip().split(" ")[0] for r in scen_rows]
if len(families) != 22 or len(later_ids) != 9 or len(scen_ids) != 6:
    fail("canon", f"spec table parse drift: {len(families)} families / "
                  f"{len(later_ids)} later / {len(scen_ids)} scenarios")
else:
    ok("canon", "frozen supplement parsed: 22 families, 9 later groups, 6 scenarios")

# ── entries: the README reader spine ──────────────────────────────────
readme_rel = "README.md"
try:
    readme = read_rel(readme_rel)
except OSError as e:
    readme = ""
    fail("entries", f"cannot read {readme_rel}: {e}")
spine = marked(readme, "spine")
if not spine:
    fail("entries", "README.md has no <!-- spine:begin -->…<!-- spine:end --> block")
else:
    for e in ENTRIES:
        n = spine.count(e)
        if n != 1:
            fail("entries", f"entry choice {e!r} appears {n} times in the spine block")
    for e in ENTRIES:
        m = re.search(rf"- \*\*{re.escape(e)}\*\*\s*—\s*\*([^*]+)\*", spine)
        if not m:
            fail("entries", f"entry {e!r} does not name its readiness state first")
    if not re.search(r"(troubleshooting|report)", spine, re.I):
        fail("entries", "the spine block has no failure/report exit")
    ok("entries", "README spine: 3 entry choices, readiness first, failure exit")

# ── spine_page: the same choices on the evidence page ─────────────────
ev_rel = "docs/evidence.md"
try:
    ev = read_rel(ev_rel)
except OSError as e:
    ev = ""
    fail("spine_page", f"cannot read {ev_rel}: {e}")
if ev:
    esp = marked(ev, "spine")
    if not esp:
        fail("spine_page", "docs/evidence.md has no marked spine block")
    else:
        for e in ENTRIES:
            if e not in esp:
                fail("spine_page", f"entry choice {e!r} absent from docs/evidence.md")
        if not re.search(r"(troubleshooting|sanitized report)", esp, re.I):
            fail("spine_page", "evidence.md spine has no failure/report exit")
        ok("spine_page", "evidence.md spine carries the three entry choices")

# ── cards: exact field shape ──────────────────────────────────────────
cards = {}
if ev:
    parts = re.split(r"^### Card — ", ev, flags=re.M)[1:]
    for p in parts:
        head, _, body = p.partition("\n")
        title = head.strip()
        m = re.search(r"\[([a-z0-9-]+)\]\s*$", title)
        key = m.group(1) if m else None
        sect = body.split("\n### ")[0].split("\n## ")[0]
        for fld in ("State", "Scope", "Evidence", "Limits", "Access"):
            if not re.search(rf"^- \*\*{fld}:\*\*\s*\S", sect, re.M):
                fail("cards", f"card {title!r} missing field **{fld}:**")
        sm = re.search(r"^- \*\*State:\*\*\s*(.+)$", sect, re.M)
        state = sm.group(1).strip() if sm else ""
        if key:
            cards[key] = (title, state)
    if len(parts) < 9:
        fail("cards", f"only {len(parts)} card sections (<9)")
    else:
        ok("cards", f"{len(parts)} cards carry the exact field shape")

# ── labels: the closed four-state vocabulary ──────────────────────────
if ev:
    bad = []
    for title, state in [(t, s) for t, s in cards.values()]:
        if not any(state.startswith(st) for st in STATES):
            bad.append((title, state))
    if bad:
        for t, s in bad:
            fail("labels", f"card {t!r} state {s!r} outside the closed vocabulary {STATES}")
    else:
        ok("labels", "every card state is in the closed four-state vocabulary")

# ── gates: required keys and their states ─────────────────────────────
REQUIRED = {
    "setup": "Available",
    "ex-01": "Available",
    "ex-02": "Available",
    "receipt": "Available",
    "useful-task": "Not yet available",
    "study": "Not yet available",
    "operations": "Not yet available",
    "component": "Restricted",
    "interchange": "Restricted",
    "evidence-access": "Restricted",
    "contribute": "Available",
    "replay-record": "Available",
}
if ev:
    for key, want in REQUIRED.items():
        if key not in cards:
            fail("gates", f"required card key [{key}] absent")
        elif not cards[key][1].startswith(want):
            fail("gates",
                 f"card [{key}] state {cards[key][1]!r} — expected {want!r} "
                 "(fixture success never substitutes for an unmet objective)")
        else:
            pass
    if all(k in cards and cards[k][1].startswith(w) for k, w in REQUIRED.items()):
        ok("gates", "task/teaching/study gates separate; S1/S3/S4 unmet preserved")

# ── portfolio: 22 families / 9 later / 6 scenarios / one job ──────────
if ev:
    fam_block = marked(ev, "portfolio:families")
    if not fam_block:
        fail("portfolio", "no <!-- portfolio:families --> block")
    else:
        for fam, disp in families:
            row = [l for l in fam_block.splitlines()
                   if l.startswith(f"| {fam} |")]
            if not row:
                fail("portfolio", f"family {fam!r} missing from the coverage table")
                continue
            cell = row[0].rstrip("|").split("|")[-1].strip().lower()
            if not cell.startswith(disp):
                fail("portfolio",
                     f"family {fam!r} disposition {cell!r} != spec {disp!r}")
        ok("portfolio", f"{len(families)} families retained with spec dispositions")
    lat_block = marked(ev, "portfolio:later")
    for g in later_ids:
        if g not in lat_block:
            fail("portfolio", f"later group {g} missing")
    sce_block = marked(ev, "portfolio:scenarios")
    for s in scen_ids:
        if not re.search(rf"\b{s}\b", sce_block):
            fail("portfolio", f"scenario {s} missing")
    job = re.sub(r"\s+", " ", marked(ev, "job"))
    if not job or "technically prepared harness evaluator" not in job:
        fail("portfolio", "the one selected job (harness evaluator) is not named")
    elif "hypothesis" not in job or "no outside demand" not in job.lower():
        fail("portfolio", "the job block drops the inferred-demand label")
    else:
        ok("portfolio", "9 later groups, 6 scenarios, one selected job retained")

# ── unmet: partial-release clause preserves S1/S3 ─────────────────────
if ev:
    un = marked(ev, "unmet")
    if not un:
        fail("unmet", "no <!-- unmet:begin --> partial-release clause")
    else:
        for tok in ("S1", "S3"):
            if tok not in un:
                fail("unmet", f"partial-release clause missing {tok!r}")
        if "are **unmet**" not in un:
            fail("unmet", "partial-release clause lost the 'are **unmet**' claim")
        if "substitut" not in un and "never" not in un:
            fail("unmet", "partial-release clause does not forbid fixture substitution")
        ok("unmet", "S1/S3 unmet objectives preserved in the partial release")

# ── sources: claim→source map resolves ────────────────────────────────
if ev:
    cm = marked(ev, "claims")
    if not cm:
        fail("sources", "no <!-- claims:begin --> claim map")
    else:
        for cls in ("Benefit", "Overlap", "Exclusion", "Revisit"):
            if not re.search(rf"^\| {cls}", cm, re.M):
                fail("sources", f"claim class {cls!r} missing from the map")
        for m in link_failures(cm, ev_rel):
            fail("sources", m)
        ok("sources", "benefit/overlap/exclusion/revisit claims map to real sources")

# ── record: the phase-1 record and its pinned surfaces ────────────────
try:
    record = read_rel(RECORD_REL)
except OSError as e:
    record = ""
    fail("record", f"cannot read {RECORD_REL}: {e}")
if record:
    head = record.split("\n", 8)[:8]
    if "internal record; not outside-verified" not in "\n".join(head):
        fail("record", "record header lacks 'internal record; not outside-verified'")
    bb = re.sub(r"\s+", " ", marked(record, "bounds"))
    for b in BOUNDS:
        if re.sub(r"\s+", " ", b) not in bb:
            fail("record", f"bounds statement absent: {b[:60]}…")
    for m in link_failures(record, RECORD_REL):
        fail("record", m)
    ok("record", "record exists; header, bounds block and links verified")
# the [replay-record] card carries the class + links the record
if ev and "replay-record" in cards:
    sect = ev.split("### Card — ")[1:]
    sect = [s for s in sect if "[replay-record]" in s.split("\n")[0]]
    if not sect:
        fail("record", "[replay-record] card section absent")
    else:
        body = sect[0].split("\n### ")[0].split("\n## ")[0]
        if "internal record; not outside-verified" not in body:
            fail("record", "[replay-record] lacks the internal-record phrase")
        evl = re.search(r"^- \*\*Evidence:\*\*(.*?)(?=\n- \*\*|\Z)", body, re.S | re.M)
        if not evl or "phase1-execution-surface-replay.md" not in evl.group(1):
            fail("record", "[replay-record] Evidence does not link the record")
# the spine links the record and names the class
if "docs/results/phase1-execution-surface-replay.md" not in spine or \
        "not outside-verified" not in spine:
    fail("record", "the spine does not link the record as not outside-verified")
# the study card links the record
if ev and "study" in cards:
    sect = [s for s in ev.split("### Card — ")[1:] if "[study]" in s.split("\n")[0]]
    if not sect or "phase1-execution-surface-replay.md" not in sect[0].split("\n### ")[0]:
        fail("record", "the study card does not link the record")

# ── record-surface bans (RECORD_BANNED over record + evidence + spine) ──
if ev:
    rhay = (record + "\n" + ev + "\n" + spine).lower()
    for b in RECORD_BANNED:
        if re.search(rf"\b{re.escape(b)}\b", rhay):
            fail("record", f"record-banned phrase {b!r} on the record surfaces")

# ── honesty: no favorable-treatment or scientific-claim language ──────
BANNED = [
    "proven", "guarantee", "guaranteed", "production-ready",
    "production ready", "state-of-the-art", "outperform",
    "significant improvement", "statistically significant",
    "users demand", "users love", "widely adopted", "market-leading",
    "best-in-class", "superior",
]
if ev:
    hay = ev + "\n" + spine + "\n" + record
    low = hay.lower()
    for b in BANNED:
        if re.search(rf"\b{re.escape(b)}\b", low):
            fail("honesty", f"banned phrase {b!r} on the positioning surface")
    ok("honesty", "no favorable-treatment / scientific-claim language")

# ── links: repo-relative links resolve on the touched surfaces ────────
if ev:
    for m in link_failures(ev, ev_rel):
        fail("links", m)
for m in link_failures(spine, readme_rel):
    fail("links", m)
ok("links", "repo-relative links resolve on touched surfaces")

# ── nav: docs/README.md carries the page in both tables ───────────────
nav_rel = "docs/README.md"
try:
    nav = read_rel(nav_rel)
except OSError as e:
    nav = ""
    fail("nav", f"cannot read {nav_rel}: {e}")
if nav:
    n = len(re.findall(r"\[evidence\.md\]|\(evidence\.md\)", nav))
    if n < 2:
        fail("nav", "docs/README.md names docs/evidence.md fewer than twice "
                    "(navigation + mode table)")
    if "evidence.md) | living |" not in nav:
        fail("nav", "docs/README.md mode table does not mark evidence.md living")
    else:
        ok("nav", "docs/README.md navigation + mode table updated")

if fails:
    print(f"check-position: {fails} failure(s)", file=sys.stderr)
    sys.exit(1)
print(f"check-position: OK — {len(results)} leg(s) green "
      f"[{os.environ.get('CHECK_POSITION_LABEL', '')}]")
print("check-position: live: skipped(no routine PR calls — ADR-0363 owns "
      "authorized live legs; unset allowance is zero)")
PY
rc=$?
if [ "$rc" -eq 0 ] && [ "$MODE" = head ]; then
  echo "check-position: tested $LABEL"
fi
exit "$rc"
