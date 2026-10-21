#!/usr/bin/env bash
# test-live-verify.sh — hermetic self-test for scripts/live-verify.sh
# (R4.5a / ADR-0363). No real secrets, no network calls that need a
# service: fake env files, temp dirs, temp git repos. Legs that need a
# tool absent on the runner report skipped(<tool>-absent) honestly —
# never a fabricated pass.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LV="$ROOT/scripts/live-verify.sh"
CK="$ROOT/scripts/check-live-evidence.sh"
SCANBIN="$ROOT/target/debug/hh-secret-scan"

fails=0
say() { echo "test-live-verify: $*"; }
bad() { echo "test-live-verify: FAIL — $*" >&2; fails=$((fails + 1)); }
skip() { echo "test-live-verify: skipped($1)"; }
check() { # $1 desc, $2 expected exit, then command…
  local desc="$1" want="$2"; shift 2
  "$@" >/dev/null 2>&1; local got=$?
  [ "$got" = "$want" ] || bad "$desc — exit $got, want $want"
}

T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
mkdir -p "$T/live" "$T/live2"
cp "$ROOT/docs/build/reports/live/pricing.json" "$T/live/pricing.json"
# a 36-char token body — over gitleaks' entropy floor but unmistakably fake;
# built by concatenation so no credential-shaped literal is ever committed
# (the very gate under test would — correctly — refuse this file otherwise)
FAKE_SECRET="ghp_T3sT""Va1uE9xK7mQ2pR8wZ4yB6nC0jF5hL1dG3sA"
cat > "$T/env" <<EOF
HH_SELFTEST_TOKEN=selftest-token-not-a-credential
HH_PLANTED_API_KEY=$FAKE_SECRET
HH_LOCAL_BASE_URL=http://127.0.0.1:9/unreachable
HH_LOCAL_MODEL=never-served-model
EOF
printf 'UNRELATED=nothing\n' > "$T/empty.env"

run() { env HH_LIVE_ENV="$T/env" HH_LIVE_DIR="$T/live" "$@"; }

# ── 1. --check-names: names + yes/no only, never a value ────────────────
out=$(run bash "$LV" --check-names --all)
if ! printf '%s\n' "$out" | grep -q 'HH_SELFTEST_TOKEN provided: yes'; then
  bad "--check-names: HH_SELFTEST_TOKEN not reported provided: yes"
fi
if printf '%s\n' "$out" | grep -qF "$FAKE_SECRET"; then
  bad "--check-names output contains a planted VALUE"
fi
if printf '%s\n' "$out" | grep -vE '^[A-Z_]+ provided: (yes|no)$|^live-verify: '; then
  bad "--check-names printed a line outside the names-only contract"
fi
if ! printf '%s\n' "$out" | grep -q 'OPENAI_API_KEY provided: no'; then
  bad "--check-names --all: OPENAI_API_KEY (absent from fake env) not reported no"
fi
out=$(env HH_LIVE_ENV="$T/empty.env" HH_LIVE_DIR="$T/live" bash "$LV" --check-names local-reachability)
printf '%s\n' "$out" | grep -q 'HH_LOCAL_BASE_URL provided: no' \
  || bad "--check-names <leg> did not report the leg's names"

# ── 2. missing NAME → exit 4 ────────────────────────────────────────────
check "missing NAME refuses (exit 4)" 4 \
  env HH_LIVE_ENV="$T/empty.env" HH_LIVE_DIR="$T/live" bash "$LV" selftest

# ── 3. spend-cap refusals → exit 3 ──────────────────────────────────────
check "over-allocation refuses (exit 3)" 3 \
  run bash "$LV" selftest --budget 5
# over-cap: fake pricing (alloc 8, cap 10) + cumulative 5 + budget 6 → 11 > 10
cat > "$T/live2/pricing.json" <<'EOF'
{"schema":"hh-live-pricing/1","retrieved":"2026-10-21","source":"test","cap_usd":10,
 "allocations_usd":{"selftest":8,"reserve":2},"models":{}}
EOF
printf '{"ts":"t","ticket":"x","leg":"l","cost_usd":5,"cumulative_usd":5,"record":"r.json"}\n' \
  > "$T/live2/SPEND.jsonl"
check "over-cap refuses (exit 3)" 3 \
  env HH_LIVE_ENV="$T/env" HH_LIVE_DIR="$T/live2" bash "$LV" selftest --budget 6

# ── 4. selftest leg end-to-end; planted value never lands ───────────────
out=$(run bash "$LV" selftest --budget 0 --harness test-harness 2>&1)
[ $? -eq 0 ] || bad "selftest leg: $out"
rec=$(ls "$T/live/"*-selftest.json 2>/dev/null | head -1)
[ -n "$rec" ] || bad "selftest record not written"
if grep -rlqF "$FAKE_SECRET" "$T/live/"; then
  bad "planted secret VALUE landed in a written artifact"
fi
[ -f "$T/live/SPEND.jsonl" ] || bad "SPEND.jsonl not appended"
# the record says names + yes/no only
grep -q '"HH_SELFTEST_TOKEN": "yes"' "$rec" 2>/dev/null \
  || bad "record provided map missing HH_SELFTEST_TOKEN"
grep -qF "selftest-token-not-a-credential" "$rec" 2>/dev/null \
  && bad "record carries a provisioned VALUE"

# ── 5. leak detectors (needs hh-secret-scan) ────────────────────────────
if [ -x "$SCANBIN" ]; then
  printf 'the planted value: %s\n' "$FAKE_SECRET" > "$T/evil.txt"
  check "--sweep refuses a planted known value (exit 5)" 5 \
    run bash "$LV" --sweep "$T/evil.txt"
  printf 'clean = 1\n' > "$T/clean.txt"
  check "--sweep clean file (exit 0)" 0 \
    run bash "$LV" --sweep "$T/clean.txt"
else
  skip "hh-secret-scan-absent"
fi

# ── 6. gitleaks staged-diff gate ────────────────────────────────────────
if command -v gitleaks >/dev/null 2>&1; then
  TR="$T/repo"; mkdir -p "$TR"; cp "$ROOT/.gitleaks.toml" "$TR/"
  (cd "$TR" && git init -q . && git config user.email t@t && git config user.name t \
    && git commit -qm init --allow-empty)
  (cd "$TR" && printf 'clean = 1\n' > ok.txt && git add ok.txt)
  check "clean staged diff passes gate" 0 \
    env HH_GITLEAKS_CFG="$TR/.gitleaks.toml" bash -c "cd '$TR' && bash '$LV' --gate"
  (cd "$TR" && printf 'token = "%s"\n' "$FAKE_SECRET" > evil.txt && git add evil.txt)
  check "staged secret refuses (exit 6)" 6 \
    env HH_GITLEAKS_CFG="$TR/.gitleaks.toml" bash -c "cd '$TR' && bash '$LV' --gate"
else
  skip "gitleaks-absent"
fi

# ── 7. unreachable leg → honest skipped record (exit 2) ─────────────────
if [ -x "$SCANBIN" ] && command -v curl >/dev/null 2>&1; then
  out=$(run bash "$LV" local-reachability --budget 0 2>&1); rc=$?
  [ "$rc" -eq 2 ] || bad "unreachable leg exit $rc, want 2"
  lrec=$(ls "$T/live/"*-local-reachability.json 2>/dev/null | head -1)
  grep -q '"result": "skipped(unreachable' "$lrec" 2>/dev/null \
    || bad "unreachable leg record does not carry skipped(unreachable…)"
else
  skip "scan-bin-or-curl-absent"
fi

# ── 8. validator over the produced records ──────────────────────────────
if command -v python3 >/dev/null 2>&1; then
  check "check-live-evidence over produced records" 0 \
    env HH_LIVE_DIR="$T/live" bash "$CK"
else
  skip "python3-absent"
fi

[ "$fails" -eq 0 ] || { echo "test-live-verify: $fails failure(s)" >&2; exit 1; }
echo "test-live-verify: OK"
