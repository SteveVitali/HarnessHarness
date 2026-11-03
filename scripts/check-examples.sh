#!/usr/bin/env bash
# check-examples.sh — R5.30 / REQ-F-106/107: the §7.3 "static, both modes"
# lane for the committed `example-card/1` records. No build, no network.
#
#   check-examples.sh [ROOT]     working-tree check (default: git toplevel)
#   check-examples.sh --head [ROOT]
#                                committed-HEAD check: clone HEAD into a
#                                scratch dir and run the same validation
#                                inside the clone — uncommitted edits are
#                                never the tested HEAD (the
#                                check-quickstart.sh precedent).
#
# Validates, at the chosen tree:
#   1. every scripts/examples/ex-<NN>.json decodes as `example-card/1`
#      — the closed member set and admitted `case` set are extracted
#      from the canonical runner source
#      (crates/hharness/src/examples.rs), never mirrored here; any
#      other committed JSON there is fixture data a card must pin;
#   2. every pins.<name>.{path,sha256} recomputes over the pinned file's
#      bytes — missing/stale fails; traversal outside the tree refuses;
#   3. every card checks[] id resolves to a check-id literal the runner
#      actually emits (pass/fail/fail_json/expected_refusal first arg);
#   4. displayed↔canonical equivalence: the
#      <!-- examples:begin -->…<!-- examples:end --> fenced `sh` block in
#      docs/getting-started.md must list each card's `run` member exactly
#      once, in card order, and scripts/examples/README.md must name each
#      card and its `run` verbatim;
#   5. repo-relative links inside the displayed section resolve.
# The live lane is recorded, never exercised:
#   `live: skipped(…)` — ADR-0363 owns authorized live legs; the unset
#   allowance is zero (REQ-F-107: skips never become passes).
# Exit 0 = green; 1 = a leg failed; 2 = usage/malformed input.
set -euo pipefail

MODE=work
ROOT_ARG=""
for a in "$@"; do
  case "$a" in
    --head) MODE=head ;;
    --work) MODE=work ;;
    -*) echo "check-examples: usage: check-examples.sh [--head] [ROOT]" >&2; exit 2 ;;
    *) ROOT_ARG="$a" ;;
  esac
done

if [ "$MODE" = head ]; then
  SRC="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  SHA="$(git -C "$SRC" rev-parse HEAD)"
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-examples-head.XXXXXX")"
  trap 'rm -rf "$TMP"' EXIT
  git clone --quiet --no-hardlinks "$SRC" "$TMP/repo" \
    || { echo "check-examples: FAIL — git clone of $SRC failed" >&2; exit 1; }
  git -C "$TMP/repo" checkout --quiet "$SHA" \
    || { echo "check-examples: FAIL — checkout of $SHA failed in the clone" >&2; exit 1; }
  ROOT="$TMP/repo"
  LABEL="committed HEAD ${SHA:0:8} (clean clone — working-tree edits are not tested)"
else
  ROOT="${ROOT_ARG:-$(git rev-parse --show-toplevel)}"
  LABEL="working tree $ROOT"
fi
export CHECK_EXAMPLES_LABEL="$LABEL"

python3 - "$ROOT" <<'PY'
import hashlib
import json
import os
import re
import sys

root = sys.argv[1]
fails = 0


def fail(msg):
    global fails
    fails += 1
    print(f"check-examples: FAIL — {msg}", file=sys.stderr)


def ok(msg):
    print(f"check-examples: {msg}")


def read_rel(path):
    with open(os.path.join(root, path), "r", encoding="utf-8") as f:
        return f.read()


# ── the canonical card contract, extracted from the owning runner ──────
runner_path = "crates/hharness/src/examples.rs"
try:
    runner = read_rel(runner_path)
except OSError as e:
    fail(f"cannot read the canonical runner {runner_path}: {e}")
    sys.exit(1)

m = re.search(r"const CARD_MEMBERS[^;]*;", runner)
members = re.findall(r'"([a-z_]+)"', m.group(0)) if m else []
m = re.search(r"matches!\(\s*case\.as_str\(\),\s*([^)]+)\)", runner)
cases = re.findall(r'"([a-z0-9-]+)"', m.group(1)) if m else []
produced = set(
    re.findall(
        r'(?:pass|fail|fail_json|expected_refusal)\(\s*"([A-Za-z0-9:_-]+)"', runner
    )
)
if not members or not cases or not produced:
    fail(f"runner contract extraction empty (members={len(members)} cases={cases} ids={len(produced)})")

# the runner permits `title` but never req()s it — optional member
required = [k for k in members if k != "title"]

# ── the committed cards ────────────────────────────────────────────────
# Card files are `ex-<NN>.json`; any other committed JSON under
# scripts/examples/ is fixture data and MUST be pinned by some card —
# an unpinned stray is a stale-snapshot class defect.
cards_dir = os.path.join(root, "scripts", "examples")
try:
    all_json = sorted(f for f in os.listdir(cards_dir) if f.endswith(".json"))
except OSError as e:
    all_json = []
    fail(f"scripts/examples/ unreadable: {e}")
card_files = [f for f in all_json if re.fullmatch(r"ex-\d+\.json", f)]
data_files = [f for f in all_json if f not in card_files]
if not card_files:
    fail("no committed example cards under scripts/examples/")

cards = []
npins = 0
pinned_paths = set()
for name in card_files:
    rel = f"scripts/examples/{name}"
    try:
        card = json.loads(read_rel(rel))
    except (OSError, json.JSONDecodeError) as e:
        fail(f"{rel}: not valid JSON — {e}")
        continue
    if not isinstance(card, dict):
        fail(f"{rel}: card must be a JSON object")
        continue
    extra = sorted(set(card) - set(members))
    missing = [k for k in required if k not in card]
    if extra:
        fail(f"{rel}: member(s) outside the closed set: {extra}")
    if missing:
        fail(f"{rel}: required member(s) absent: {missing}")
    if card.get("schema") != "example-card/1":
        fail(f"{rel}: schema must be example-card/1, got {card.get('schema')!r}")
    case = card.get("case")
    if not isinstance(case, str):
        fail(f"{rel}: case must be a string")
    elif case not in cases:
        fail(f"{rel}: case {case!r} is not admitted by the runner {cases}")
    elif case != name[: -len(".json")]:
        fail(f"{rel}: case {case!r} does not match the card filename")
    for k in ("card_version", "fixture_label", "run"):
        if not isinstance(card.get(k), str) or not card[k]:
            fail(f"{rel}: {k} must be a non-empty string")
    for k in ("checks", "negative_controls", "limitations"):
        v = card.get(k)
        if not isinstance(v, list) or not v or not all(isinstance(i, str) and i for i in v):
            fail(f"{rel}: {k} must be a non-empty string list")
    pre = card.get("prerequisites")
    if not isinstance(pre, dict):
        fail(f"{rel}: prerequisites must be an object")
    else:
        if not isinstance(pre.get("network"), bool):
            fail(f"{rel}: prerequisites.network must be a bool")
        if not isinstance(pre.get("spend_usd"), (int, float)):
            fail(f"{rel}: prerequisites.spend_usd must be a number")
        for k in ("rust", "credentials"):
            if not isinstance(pre.get(k), str):
                fail(f"{rel}: prerequisites.{k} must be a string")
    costs = card.get("costs")
    if not isinstance(costs, dict) or not all(
        isinstance(costs.get(k), str) and costs[k] for k in ("build", "runtime")
    ):
        fail(f"{rel}: costs must carry non-empty build/runtime strings")
    # pins — recompute each declared sha256 over the pinned bytes
    pins = card.get("pins")
    if not isinstance(pins, dict) or not pins:
        fail(f"{rel}: pins must be a non-empty object")
        pins = {}
    for pname, pin in sorted(pins.items()):
        if not isinstance(pin, dict):
            fail(f"{rel}: pins.{pname} must be an object")
            continue
        ppath, psha = pin.get("path"), pin.get("sha256")
        if not isinstance(ppath, str) or not ppath:
            fail(f"{rel}: pins.{pname}.path must be a non-empty string")
            continue
        if ppath.startswith("/") or ".." in ppath.split("/"):
            fail(f"{rel}: pins.{pname}.path escapes the tree: {ppath}")
            continue
        if not isinstance(psha, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", psha):
            fail(f"{rel}: pins.{pname}.sha256 must be sha256:<64-hex>")
            continue
        npins += 1
        pinned_paths.add(ppath)
        try:
            with open(os.path.join(root, ppath), "rb") as f:
                got = "sha256:" + hashlib.sha256(f.read()).hexdigest()
        except OSError as e:
            fail(f"{rel}: pins.{pname} — {ppath}: {e}")
            continue
        if got != psha:
            fail(f"{rel}: stale pin {pname}: card pins {psha[:20]}…, {ppath} recomputes {got[:20]}…")
    # checker mapping — every declared check id is one the runner emits
    for cid in card.get("checks") or []:
        if cid not in produced:
            fail(f"{rel}: check {cid!r} is not a check id the runner produces")
    cards.append((name, card))

# every committed non-card JSON is pinned data — never an unpinned stray
for name in data_files:
    if f"scripts/examples/{name}" not in pinned_paths:
        fail(f"scripts/examples/{name}: committed fixture data no card pins (stale snapshot)")

# ── displayed ↔ canonical equivalence ─────────────────────────────────
doc_rel = "docs/getting-started.md"
displayed = []
try:
    inmark = insh = False
    for ln in read_rel(doc_rel).splitlines():
        if "<!-- examples:begin -->" in ln:
            inmark = True
        if "<!-- examples:end -->" in ln:
            inmark = False
        if inmark and re.match(r"^```sh\s*$", ln):
            insh = True
            continue
        if insh and re.match(r"^```\s*$", ln):
            insh = False
            continue
        if insh:
            displayed.append(ln)
except OSError as e:
    fail(f"{doc_rel} unreadable: {e}")

expected = [c["run"] for _, c in cards if isinstance(c.get("run"), str)]
if not displayed:
    fail(f"{doc_rel}: no <!-- examples:begin -->…<!-- examples:end --> fenced sh block")
elif displayed != expected:
    fail(
        f"{doc_rel}: displayed commands {displayed} != canonical card run members {expected}"
    )

readme_rel = "scripts/examples/README.md"
try:
    readme = read_rel(readme_rel)
    for name, card in cards:
        if name not in readme:
            fail(f"{readme_rel}: card {name} not named in the index")
        run = card.get("run")
        if isinstance(run, str) and run not in readme:
            fail(f"{readme_rel}: {name} run {run!r} not displayed verbatim")
except OSError as e:
    fail(f"{readme_rel} unreadable: {e}")

# repo-relative links inside the displayed section resolve
try:
    doc = read_rel(doc_rel)
    mark = doc.find("<!-- examples:begin -->")
    if mark != -1:
        sect_start = doc.rfind("## ", 0, mark)
        sect_end = doc.find("\n## ", doc.find("<!-- examples:end -->", mark))
        sect = doc[
            sect_start if sect_start != -1 else 0 : sect_end
            if sect_end != -1
            else len(doc)
        ]
    else:
        sect = ""
    for target in re.findall(r"\]\(([^)\s]+)\)", sect):
        if re.match(r"[a-z]+://", target):
            continue
        t = target.split("#", 1)[0]
        p = os.path.normpath(os.path.join(os.path.dirname(doc_rel), t))
        if not os.path.exists(os.path.join(root, p)):
            fail(f"{doc_rel}: examples-section link {target} does not resolve ({p})")
except OSError:
    pass  # already reported above

if fails:
    print(f"check-examples: {fails} failure(s)", file=sys.stderr)
    sys.exit(1)
ok(
    f"OK — {len(cards)} card(s), {npins} pin(s) recomputed, "
    f"{len(produced)} runner check ids, displayed block equivalent [{os.environ.get('CHECK_EXAMPLES_LABEL', '')}]"
)
ok("live: skipped(no routine PR calls — ADR-0363 owns authorized live legs; unset allowance is zero)")
PY
rc=$?
if [ "$rc" -eq 0 ] && [ "$MODE" = head ]; then
  echo "check-examples: tested $LABEL"
fi
exit "$rc"
