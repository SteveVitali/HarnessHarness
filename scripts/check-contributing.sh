#!/usr/bin/env bash
# check-contributing.sh — R5.31 / REQ-A-205 + REQ-B-004: the §8.1 static,
# both-modes check for the ordinary contribution and failure-report
# routes. No build, no network.
#
#   check-contributing.sh [ROOT]     working-tree check (default: git toplevel)
#   check-contributing.sh --head [ROOT]
#                                    committed-HEAD check: clone HEAD into a
#                                    scratch dir and run the same validation
#                                    inside the clone — uncommitted edits are
#                                    never the tested HEAD (the
#                                    check-quickstart.sh precedent).
#
# Validates, at the chosen tree:
#   lanes          every §8.1 lane heading is present exactly once in
#                  CONTRIBUTING.md (no-run / receipt / attach-evidence /
#                  propose / verifyfixed / gated-routes / starter map);
#   no_run         the no-run lane names the required fields: command,
#                  expected/actual, exit code or signal, revision + dirty
#                  state, OS/architecture, toolchain, features/backend/
#                  prerequisites, sanitized stderr, doctor-conditional;
#   receipt        the receipt lane names run/attempt/configuration/input
#                  + event/grade/procedure ids, expected-vs-observed fields,
#                  the real `hh run export`/`hh bundle validate` locator,
#                  and the access/loss-reason alternative;
#   privacy        the secrets-inspection list, cleared-data rule,
#                  uncleared-secrets block and private security route;
#   verifyfixed    pin revision/input/recipe/rights, preserve the original
#                  failure, scratch copies, regression-fails-without,
#                  candidate comparison, changed-input disclosure,
#                  unable-to-reproduce ≠ fixed, distinct verifier classes;
#   no_ritual      ordinary contributors need no internal Q register /
#                  ledger / manifest ritual; the manifest workflow stays
#                  scoped inside its conditional subsection and the PR
#                  template keeps build-ticket notes optional;
#   gated          the optional packaged component and scientific
#                  replication lanes are explicitly gated;
#   form_fields    .github/ISSUE_TEMPLATE/bug_report.yml mirrors the
#                  documented fields (exit code/signal, dirty state,
#                  toolchain/features/backend/prerequisites, receipt-or-
#                  loss prompt, required sanitization attestation) and
#                  config.yml keeps the private security route;
#   commands_real  every `hh …` command cited in CONTRIBUTING.md resolves
#                  to a dispatch arm in crates/hh-cli/src/cli.rs — a
#                  fictitious `hh bug-bundle`/`hh recompute`/`hh replicate`
#                  fails;
#   blocked        the four route-blocking rules are stated: unsupported
#                  command (commands_real), suppressed loss/access reason,
#                  uncleared attached secrets, and manifest machinery
#                  confined to the manifest-ticket subsection;
#   codeowners     CODEOWNERS keeps `* @SteveVitali` review authority and
#                  no longer tells ordinary contributors that everything
#                  lands through the manifest ticket chain (§8.1's
#                  governance-preserving reconcile);
#   links          repo-relative links in CONTRIBUTING.md resolve.
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
    -*) echo "check-contributing: usage: check-contributing.sh [--head] [ROOT]" >&2; exit 2 ;;
    *) ROOT_ARG="$a" ;;
  esac
done

if [ "$MODE" = head ]; then
  SRC="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  SHA="$(git -C "$SRC" rev-parse HEAD)"
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-contributing-head.XXXXXX")"
  trap 'rm -rf "$TMP"' EXIT
  git clone --quiet --no-hardlinks "$SRC" "$TMP/repo" \
    || { echo "check-contributing: FAIL — git clone of $SRC failed" >&2; exit 1; }
  git -C "$TMP/repo" checkout --quiet "$SHA" \
    || { echo "check-contributing: FAIL — checkout of $SHA failed in the clone" >&2; exit 1; }
  ROOT="$TMP/repo"
  LABEL="committed HEAD ${SHA:0:8} (clean clone — working-tree edits are not tested)"
else
  ROOT="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  LABEL="working tree $ROOT"
fi
export CHECK_CONTRIB_LABEL="$LABEL"

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
    results.append((cid, False, msg))
    print(f"check-contributing: FAIL [{cid}] {msg}", file=sys.stderr)


def ok(cid, msg):
    results.append((cid, True, msg))
    print(f"check-contributing: ok   [{cid}] {msg}")


def read_rel(path):
    with open(os.path.join(root, path), "r", encoding="utf-8") as f:
        return f.read()


def section(doc, heading):
    """Text of the '### <heading>' section through the next ### or ##,
    whitespace-normalized so a phrase wrapped across lines still matches."""
    m = re.search(rf"^### {re.escape(heading)}\s*$", doc, re.M)
    if not m:
        return None
    tail = doc[m.end():]
    stop = re.search(r"^#{2,3} ", tail, re.M)
    span = tail[: stop.start()] if stop else tail
    return re.sub(r"\s+", " ", span)


def require(cid, cond, msg):
    if cond:
        ok(cid, msg)
    else:
        fail(cid, msg)


CONTRIB = "CONTRIBUTING.md"
try:
    doc = read_rel(CONTRIB)
except OSError as e:
    print(f"check-contributing: FAIL — cannot read {CONTRIB}: {e}", file=sys.stderr)
    sys.exit(1)

# ── lanes ────────────────────────────────────────────────────────────
LANES = [
    "## Report a problem or propose a change",
    "### Report a failure without a run",
    "### Report a wrong run or receipt",
    "### Before you attach evidence",
    "### Propose a change",
    "### Show a fix works: verifyfixed",
    "### Two routes that are not open yet",
    "### Where a contribution lands",
]
missing = [
    h for h in LANES
    if len(re.findall(rf"^{re.escape(h)}\s*$", doc, re.M)) != 1
]
require("lanes", not missing, f"lane headings present once each (missing/dup: {missing})")

no_run = section(doc, "Report a failure without a run") or ""
receipt = section(doc, "Report a wrong run or receipt") or ""
attach = section(doc, "Before you attach evidence") or ""
propose = section(doc, "Propose a change") or ""
vfix = section(doc, "Show a fix works: verifyfixed") or ""
gated = section(doc, "Two routes that are not open yet") or ""
lands = section(doc, "Where a contribution lands") or ""

# ── no_run ───────────────────────────────────────────────────────────
NO_RUN_TERMS = [
    "command", "expected", "actual", "exit code or signal",
    "git rev-parse HEAD", "dirty", "operating system", "architecture",
    "cargo --version", "feature flags", "backend", "prerequisites",
    "sanitized stderr", "hh doctor",
]
absent = [t for t in NO_RUN_TERMS if t not in no_run]
require("no_run", bool(no_run) and not absent,
        f"no-run lane names the required fields (absent: {absent})")

# ── receipt ──────────────────────────────────────────────────────────
RECEIPT_TERMS = [
    "run", "attempt", "configuration", "input", "event", "grade",
    "procedure", "expected", "observed", "hh run export",
    "hh bundle validate", "locator", "loss reason", "receipt",
]
absent = [t for t in RECEIPT_TERMS if t not in receipt]
require("receipt", bool(receipt) and not absent,
        f"receipt lane names run/report fields + receipt-or-loss (absent: {absent})")

# ── privacy ──────────────────────────────────────────────────────────
PRIV_TERMS = [
    "credentials", "signed URLs", "private", "model I/O", "solutions",
    "oracles", "redistribution rights", "cleared data", "synthetic",
    "blocked until", "SECURITY.md",
]
absent = [t for t in PRIV_TERMS if t not in attach]
require("privacy", bool(attach) and not absent,
        f"privacy inspection + secrets block + private route (absent: {absent})")

# ── verifyfixed ──────────────────────────────────────────────────────
VFIX_TERMS = [
    "revision", "input", "recipe", "rights", "preserve", "scratch",
    "regression", "fails without the fix", "candidate", "unable to reproduce",
    "reporter", "maintainer", "outside verification",
]
absent = [t for t in VFIX_TERMS if t not in vfix]
require("verifyfixed", bool(vfix) and not absent,
        f"verifyfixed disposition complete (absent: {absent})")

# ── no_ritual ────────────────────────────────────────────────────────
intro = re.sub(r"\s+", " ", doc.split("### ", 1)[0])
ritual_ok = (
    "internal question register" in intro
    and "build ledger" in intro
    and "manifest ticket ritual" in intro
    and "### If you are implementing a manifest ticket" in doc
)
try:
    prt = read_rel(".github/pull_request_template.md")
    ritual_ok = ritual_ok and "(if applicable)" in prt
except OSError:
    ritual_ok = False
require("no_ritual", ritual_ok,
        "ordinary contributors need no Q/ledger/manifest ritual; "
        "manifest workflow conditional; PR-template ticket notes optional")

# ── gated ────────────────────────────────────────────────────────────
gated_ok = (
    "hharness-compact-evict-oldest" in gated
    and re.search(r"gat(ed|ing)", gated)
    and "Scientific replication" in gated
    and "separately admitted" in gated
    and "not an ordinary" in gated
)
require("gated", bool(gated) and gated_ok,
        "optional packaged component + scientific replication explicitly gated")

# ── form_fields ──────────────────────────────────────────────────────
form_ok, detail = True, ""
try:
    form = read_rel(".github/ISSUE_TEMPLATE/bug_report.yml")
    for need in ("id: exit", "id: receipt", "id: cleared", "id: commit",
                 "id: context", "dirty", "cargo --version", "backend",
                 "prerequisites", "hh run export", "access/loss reason",
                 "required: true", "setup / build / startup"):
        if need not in form:
            form_ok, detail = False, need
            break
except OSError:
    form_ok, detail = False, "unreadable"
if form_ok:
    try:
        cfg = read_rel(".github/ISSUE_TEMPLATE/config.yml")
        if "security/advisories" not in cfg and "SECURITY" not in cfg:
            form_ok, detail = False, "config.yml lacks the private security route"
    except OSError:
        form_ok, detail = False, "config.yml unreadable"
require("form_fields", form_ok,
        f"bug_report.yml mirrors the documented fields (offender: {detail})")

# ── commands_real — `hh …` citations resolve against the cli dispatch ──
try:
    cli = read_rel("crates/hh-cli/src/cli.rs")
except OSError as e:
    fail("commands_real", f"cannot read cli.rs: {e}")
    cli = ""
pairs = set(re.findall(r'\(\s*"([a-z][a-z0-9-]*)"\s*,\s*"([a-z][a-z0-9-]*)"\s*\)', cli))
singles = set(
    m.group(1)
    for m in re.finditer(r'\(\s*"([a-z][a-z0-9-]*)"\s*,\s*_\s*\)', cli)
) | {a for a, _ in pairs}
bad_cmds = []
for m in re.finditer(r"(?<![A-Za-z0-9_-])hh\s+([a-z][a-z0-9-]*)(?:\s+([a-z][a-z0-9-]*))?", doc):
    c1, c2 = m.group(1), m.group(2)
    if c2 is None:
        if c1 not in singles:
            bad_cmds.append(f"hh {c1}")
    elif (c1, c2) not in pairs:
        bad_cmds.append(f"hh {c1} {c2}")
require("commands_real", not bad_cmds,
        f"every cited hh command resolves in cli.rs dispatch (bogus: {bad_cmds})")

# ── blocked — the four route-blocking rules are stated ───────────────
m = re.search(
    r"^### If you are implementing a manifest ticket\s*$", doc, re.M
)
if m:
    tail = doc[m.end():]
    stop = re.search(r"^#{2,3} ", tail, re.M)
    raw_span = tail[: stop.start()] if stop else tail
    outside = doc[: m.start()] + tail[len(raw_span):]
else:
    outside = doc
leak = [t for t in ("deferrals-open.sh", "bm-row.sh", "00_MANIFEST.md",
                  "docs/build/LEDGER.md") if t in outside]
blocked_ok = (
    "suppressing the reason" in receipt and "blocks the report" in receipt
    and "blocked until" in attach
    and not leak
)
require("blocked", blocked_ok,
        f"loss-suppression + uncleared-secrets + manifest-bureaucracy blocks "
        f"(manifest tokens outside the ticket section: {leak})")

# ── codeowners — governance kept, manifest-chain broad wording reconciled ──
try:
    co = read_rel("CODEOWNERS")
    co_ok = (
        re.search(r"^\* @SteveVitali\s*$", co, re.M) is not None
        and "Everything lands through the manifest ticket chain" not in co
        and "manifest ticket chain governs assigned" in co
    )
except OSError:
    co_ok = False
require("codeowners", co_ok,
        "CODEOWNERS: owner review retained; manifest chain scoped to "
        "assigned build tickets (ordinary PRs carry no ritual)")

# ── links — repo-relative links resolve ──────────────────────────────
bad_links = []
for target in re.findall(r"\]\(([^)\s]+)\)", doc):
    if re.match(r"[a-z]+://", target) or target.startswith("#"):
        continue
    t = target.split("#", 1)[0]
    if not t:
        continue
    p = os.path.normpath(os.path.join(os.path.dirname(CONTRIB), t))
    if not os.path.exists(os.path.join(root, p)):
        bad_links.append(target)
require("links", not bad_links, f"repo-relative links resolve (bad: {bad_links})")

# ── summary ──────────────────────────────────────────────────────────
if fails:
    print(f"check-contributing: {fails} failure(s)", file=sys.stderr)
    sys.exit(1)
print(
    f"check-contributing: OK — {len(results)} check ids green "
    f"[{os.environ.get('CHECK_CONTRIB_LABEL', '')}]"
)
print("check-contributing: live: skipped(no routine PR calls — ADR-0363 owns authorized live legs)")
PY
rc=$?
if [ "$rc" -eq 0 ] && [ "$MODE" = head ]; then
  echo "check-contributing: tested $LABEL"
fi
exit "$rc"
