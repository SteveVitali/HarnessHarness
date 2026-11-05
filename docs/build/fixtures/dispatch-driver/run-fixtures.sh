#!/usr/bin/env bash
# run-fixtures.sh — fixture battery for hh-dispatch-driver/1 (R5.2).
#
# Two modes:
#   plan      (default) — mutate a git-initialized scratch copy of the driver's
#             real inputs per cases/<name>/patch.sh, run `plan`, require the
#             named refusal diagnostic in cases/<name>/expected.txt (exit 1)
#             or `EMIT` for the positive control (exit 0). The stock-checker
#             input is the committed stand-in checker-green.json; the
#             ci-boundary/1 input is named by cases/<name>/ci.txt (default
#             ci-pass-209-e2b24b8.json, `none` omits --ci-json).
#   validate  (case dir holds transcript.json) — run `validate` against the
#             envelope named by envelope.txt and require VALID or the named
#             refusal diagnostic. Transcripts marked capture_kind:stand-in are
#             fixtures, not coordinator records.
#
# All mutations happen only under mktemp sandboxes; the driver and guard are
# read-only on the real tree. git init in each sandbox gives the ancestry
# checks real refs (pinnedBaseSha is rewritten to the sandbox base commit).
set -euo pipefail
cd "$(dirname "$0")/../../../.."

DRIVER=docs/build/tools/dispatch_driver.py
FIXDIR=docs/build/fixtures/dispatch-driver
GREEN="$FIXDIR/checker-green.json"
DEFCI="ci-pass-209-e2b24b8.json"
export GIT_AUTHOR_NAME=fixture GIT_AUTHOR_EMAIL=fixture@local
export GIT_COMMITTER_NAME=fixture GIT_COMMITTER_EMAIL=fixture@local

expect() {  # expect <case> <rc> <expected> <out>
  local name="$1" rc="$2" exp="$3" out="$4"
  if [ "$exp" = "EMIT" ] || [ "$exp" = "VALID" ]; then
    if [ "$rc" -eq 0 ] && printf '%s\n' "$out" | grep -qF "\"verdict\": \"$exp\""; then
      echo "  ✓ $name — $exp"; return 0
    fi
  elif [ "$rc" -eq 1 ] && printf '%s\n' "$out" | grep -qF "\"code\": \"$exp\""; then
    echo "  ✓ $name — refused with [$exp]"; return 0
  fi
  echo "  ✗ $name — expected $exp, got rc=$rc" >&2
  printf '%s\n' "$out" | tail -15 >&2
  return 1
}

fail=0; pass=0
for case_dir in "$FIXDIR"/cases/*; do
  name=$(basename "$case_dir")
  expected=$(head -1 "$case_dir/expected.txt" | tr -d '[:space:]')
  if [ -f "$case_dir/transcript.json" ]; then
    envf="$FIXDIR/$(head -1 "$case_dir/envelope.txt" | tr -d '[:space:]')"
    out=$(python3 "$DRIVER" validate --envelope "$envf" \
          --transcript "$case_dir/transcript.json" --json 2>&1) && rc=0 || rc=$?
    if expect "$name" "$rc" "$expected" "$out"; then pass=$((pass+1)); else fail=$((fail+1)); fi
    continue
  fi
  # plan mode: scratch input tree + real refs so ancestry checks run
  root=$(mktemp -d "${TMPDIR:-/tmp}/dispatch-${name}.XXXXXX")
  mkdir -p "$root/docs/build/planning" "$root/docs/build/readouts" "$root/docs/tickets"
  cp docs/evidence-and-adoption-spec.md "$root/docs/"
  cp docs/tickets/*.md "$root/docs/tickets/"
  cp docs/build/LEDGER.md docs/build/BUILD_INDEX.md "$root/docs/build/"
  cp docs/build/planning/2026-10-30_evidence-coverage.md \
     docs/build/planning/2026-10-30_evidence-coverage.json \
     docs/build/planning/2026-10-30_evidence-legacy-baseline.json \
     "$root/docs/build/planning/"
  cp docs/build/readouts/*.md "$root/docs/build/readouts/"
  git -C "$root" init -q
  git -C "$root" commit --allow-empty -qm base
  pin=$(git -C "$root" rev-parse HEAD)
  git -C "$root" branch -f codex/harnessharness-evidence
  git -C "$root" commit --allow-empty -qm tip
  git -C "$root" branch -f codex/harnessharness-evidence-r5.1
  python3 - "$root" "$pin" <<'PY'
import re, sys
root = sys.argv[1]
p = f"{root}/docs/build/LEDGER.md"
t = open(p).read()
t2 = re.sub(r"pinnedBaseSha:\s+[0-9a-f]{40}", f"pinnedBaseSha:   {sys.argv[2]}", t)
assert t2 != t, "pinnedBaseSha cell not rewritten"
t = t2
# Era synthesis (DF-R5.1-1): the plan-mode cases replay the recorded R5.2
# dispatch boundary — they pin the R5.2 contract, GATE-G9 and the r5.1 tip
# branch by name. Rewind the copied live ledger to that boundary so the
# scenario is deterministic at any later chain position: nextTicket R5.2,
# lastCompleted R5.1, chainTip the r5.1 branch created above, and the GATE
# DECISIONS table holding only decisions recorded at that era (through
# GATE-G8 — the post-G8 round-5 gate/human rows are stripped). BUILD_INDEX
# data rows are trimmed to seq <= 169 (R5.1 last landed) so R5.2 is the
# first unlanded row, exactly as at the recorded 22/22 run.
for key, val in (("nextTicket", "R5.2"), ("lastCompleted", "R5.1"),
                 ("chainTip", "codex/harnessharness-evidence-r5.1"),
                 ("buildBranchBase", "codex/harnessharness-evidence")):
    t2, n = re.subn(rf"^{key}:\s+\S+.*$", f"{key}: {val}", t, count=1, flags=re.M)
    assert n == 1, f"{key} cell absent"
    t = t2
m = re.search(r"^(## GATE DECISIONS\s*)(.*?)(\n## )", t, re.S | re.M)
assert m, "GATE DECISIONS region absent"
post_g8 = re.compile(r"^(GATE-G(?:9|1[0-8])|HUMAN-H(?:8|9|1[01]))$")
kept = []
for line in m.group(2).splitlines():
    if line.startswith("|"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and post_g8.match(cells[1] or ""):
            continue
    kept.append(line)
t = t[:m.start(2)] + "\n".join(kept) + t[m.end(2):]
open(p, "w").write(t)
ip = f"{root}/docs/build/BUILD_INDEX.md"
kept = []
for line in open(ip):
    if line.startswith("|"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if (cells and cells[0] not in ("seq", "---")
                and not cells[0].startswith("-")):
            try:
                if int(re.sub(r"[a-z]", "", cells[0])) > 169:
                    continue
            except ValueError:
                pass
    kept.append(line)
open(ip, "w").writelines(kept)
PY
  [ -f "$case_dir/patch.sh" ] && bash "$case_dir/patch.sh" "$root"
  ci="$DEFCI"; [ -f "$case_dir/ci.txt" ] && ci=$(head -1 "$case_dir/ci.txt" | tr -d '[:space:]')
  ciargs=(); [ "$ci" != "none" ] && ciargs=(--ci-json "$FIXDIR/$ci")
  out=$(python3 "$DRIVER" plan --root "$root" --leg happy \
        --checker-json "$GREEN" ${ciargs[@]+"${ciargs[@]}"} --json 2>&1) && rc=0 || rc=$?
  if expect "$name" "$rc" "$expected" "$out"; then pass=$((pass+1)); else fail=$((fail+1)); fi
  rm -rf "$root"
done
echo "run-fixtures: $pass passed, $fail failures"
[ "$fail" -eq 0 ]
