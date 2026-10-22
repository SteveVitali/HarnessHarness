#!/usr/bin/env bash
# test-full-run.sh — self-test for scripts/ci/full-run.sh (R4.2a
# deliverable 5 / ADR-0362 contract D-D). A `gh` shim on PATH serves
# FIXTURE JSON in the GitHub API's shape from files under $SHIM_DIR:
# `runs.json` holds the `run list --json databaseId,headSha` payload
# (an array of {databaseId, headSha}); `view-<id>.json` holds the
# `run view --json status,conclusion,jobs` payload. The shim emits the
# post---jq stdout real gh would — full-run.sh's own --jq expressions
# stay the production path (gh's built-in jq; no jq dependency here).
# The nondocs trees come from a throwaway git repo built under mktemp -d,
# so the "same non-docs tree" rule is exercised for real.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
FR="$ROOT/scripts/ci/full-run.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-full-run: $*"; }
bad() { echo "test-full-run: FAIL — $*" >&2; fails=$((fails + 1)); }

# --- throwaway clone: c1 code → c2 docs-only (same nondocs) → c3 code ---
REPO="$TMP/clone"; mkdir -p "$REPO/docs"
cd "$REPO" && git init -q .
echo code > main.rs; echo doc > docs/readme.md
git -c user.name=ci -c user.email=ci@localhost add -A
git -c user.name=ci -c user.email=ci@localhost commit -qm c1
C1=$(git rev-parse HEAD)
echo more-docs >> docs/readme.md
git -c user.name=ci -c user.email=ci@localhost commit -qam c2
C2=$(git rev-parse HEAD)
echo change >> main.rs
git -c user.name=ci -c user.email=ci@localhost commit -qam c3
C3=$(git rev-parse HEAD)
NOTCOMMIT="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

# --- gh shim ---------------------------------------------------------------
SHIM_DIR="$TMP/shim"; mkdir -p "$SHIM_DIR" "$TMP/bin"
cat > "$TMP/bin/gh" <<'EOF'
#!/usr/bin/env bash
[ -z "${GH_SHIM_FAIL:-}" ] || exit 1
# The reader always passes --jq; the shim asserts it and serves the JSON
# fixture, emitting gh's post---jq stdout.
case " $* " in
  *" --jq "*) ;;
  *) echo "gh shim: missing --jq" >&2; exit 1 ;;
esac
case " $* " in
  *" run list "*)
    python3 - "$SHIM_DIR/runs.json" <<'PYEOF'
import json, sys
try:
    rows = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
for r in rows:
    print("%s\t%s" % (r["databaseId"], r["headSha"]))
PYEOF
    exit $? ;;
  *" run view "*)
    seen=0
    for a in "$@"; do
      [ "$seen" = 1 ] || { [ "$a" = view ] && seen=1; continue; }
      case "$a" in
        *[!0-9]*) ;;
        *)
          python3 - "$SHIM_DIR/view-$a.json" <<'PYEOF'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
print(d.get("status") or "")
print(d.get("conclusion") or "")
for j in d.get("jobs") or []:
    print("%s\t%s" % (j.get("name") or "", j.get("conclusion") or ""))
PYEOF
          exit $? ;;
      esac
    done
    exit 1 ;;
  *) exit 0 ;;
esac
EOF
chmod +x "$TMP/bin/gh"
export SHIM_DIR PATH="$TMP/bin:$PATH"

# runs fixture: newest-first (id, headSha) pairs as GitHub JSON.
runs() { # $1... "<id>\t<sha>" pairs
  {
    printf '['
    first=1
    for pair in "$@"; do
      id="${pair%%	*}"; head="${pair#*	}"
      [ "$first" = 1 ] || printf ','
      first=0
      printf '{"databaseId":%s,"headSha":"%s"}' "$id" "$head"
    done
    printf ']'
  } > "$SHIM_DIR/runs.json"
}
# view fixture: $1 id, $2 status, $3 conclusion ("" → null), $4... "<name>\t<concl>" jobs
view() {
  local id="$1" status="$2" concl="$3"; shift 3
  {
    printf '{"status":"%s","conclusion":%s,"jobs":[' "$status" \
      "$( [ -n "$concl" ] && printf '"%s"' "$concl" || printf 'null' )"
    first=1
    for pair in "$@"; do
      n="${pair%%	*}"; c="${pair#*	}"
      [ "$first" = 1 ] || printf ','
      first=0
      printf '{"name":"%s","conclusion":%s}' "$n" \
        "$( [ -n "$c" ] && printf '"%s"' "$c" || printf 'null' )"
    done
    printf ']}'
  } > "$SHIM_DIR/view-$id.json"
}

expect() { # $1 want-line-regex, $2 want-exit, $3 label, $4 sha
  local out rc=0
  out=$(cd "$REPO" && bash "$FR" read "$4" 2>/dev/null) || rc=$?
  if [ "$rc" != "$2" ]; then bad "$3 — exit $rc want $2 ($out)"; return; fi
  if ! tail -n1 <<<"$out" | grep -qE "$1"; then
    bad "$3 — last line '$(tail -n1 <<<"$out")' !~ $1"
  else say "ok  $3"
  fi
}

# pass — the queried sha only needs the same non-docs tree: run 7001's
# head is C1; we ask for C2 (docs-only on top of C1). (AC1's third
# self-test — a docs-only head reads the full run that covered its tree.)
runs "7001	$C1"
view 7001 completed success "job-a	success" "job-b	success"
expect '^full-run: pass 7001 @'"${C1:0:7}"' \(2 jobs\)$' 0 \
  "pass — docs-only sha matches the run's nondocs tree" "$C2"

# newest-first: a newer run whose tree differs is skipped for an older match.
runs "7002	$C3" "7001	$C1"
expect '^full-run: pass 7001 @'"${C1:0:7}"' \(2 jobs\)$' 0 \
  "newest run with a different tree is skipped" "$C2"
runs "7001	$C1"

# fail — a failed job names itself.
view 7001 completed failure "job-a	success" "job-b	failure"
expect '^full-run: fail 7001 \(job-b: failure\)$' 3 "fail names the failed job" "$C2"

# M9 — a cancelled job must never read pass (even under a success run
# conclusion).
view 7001 completed success "job-a	success" "job-b	cancelled"
expect '^full-run: fail 7001 \(job-b: cancelled\)$' 3 "M9 — cancelled job reads fail" "$C2"
# …and a cancelled run conclusion reads fail too.
view 7001 completed cancelled "job-a	cancelled"
expect '^full-run: fail 7001 \(job-a: cancelled\)$' 3 "cancelled run reads fail" "$C2"

# pending — the run is still going.
view 7001 in_progress "" "job-a	success"
expect '^full-run: pending 7001$' 4 "in-progress run reads pending" "$C2"

# missing — no dispatch run, head not in the clone, or tree mismatch.
: > "$SHIM_DIR/runs.json"
runs
expect '^full-run: missing ' 3 "no runs at all" "$C2"
runs "7009	$NOTCOMMIT"
expect '^full-run: missing ' 3 "run head not in the local clone" "$C2"
runs "7008	$C3"
expect '^full-run: missing ' 3 "run head tree differs" "$C2"

# unknown — gh fails; the sha is not a commit.
runs "7001	$C1"
GH_SHIM_FAIL=1 expect '^full-run: unknown \(gh run list failed\)$' 5 "gh error" "$C2"
expect '^full-run: unknown \(.*is not a commit' 5 "sha not a commit" "$NOTCOMMIT"

# --repo passes through to gh (shim tolerates the extra args).
runs "7001	$C1"
view 7001 completed success "job-a	success"
rc=0; out=$(cd "$REPO" && bash "$FR" read "$C2" --repo me/repo 2>/dev/null) || rc=$?
{ [ "$rc" = 0 ] && tail -n1 <<<"$out" | grep -q '^full-run: pass 7001'; } \
  && say "ok  --repo passthrough" || bad "--repo passthrough — rc=$rc ($out)"

[ "$fails" -eq 0 ] || { echo "test-full-run: $fails failure(s)" >&2; exit 1; }
echo "test-full-run: OK"
