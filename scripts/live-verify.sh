#!/usr/bin/env bash
# live-verify.sh — the live-evidence runner (R4.5a / ADR-0363).
#
#   scripts/live-verify.sh <leg> [--budget USD] [--harness ID]
#   scripts/live-verify.sh --check-names [<leg>|--all]
#   scripts/live-verify.sh --gate            # gitleaks over the staged diff
#
# Rules (ADR-0363): secrets are sourced from `live.env` (default
# ~/.config/harnessharness/live.env — a symlink to a gitignored .env;
# HH_LIVE_ENV overrides, tests only) under `set +x`, never echoed, never
# blanket-exported to children; records say `provided: yes/no` per NAME.
# Every artifact is swept by hh-secret-scan (the hh-secrets detectors —
# CC1) before it is written; the staged diff then faces
# `gitleaks git --staged --redact -c .gitleaks.toml`. Exit codes:
#   0 pass · 2 leg skipped/failed (record written honestly) · 3 SPEND CAP —
#   ask the operator · 4 missing NAME (provision via HUMAN-H6) · 5
#   leak-scan hit (nothing written) · 6 gitleaks finding (STOP-and-ask) ·
#   7 required tool absent · 1 usage.
set -euo pipefail
set +x # belt: tracing must never be on where the env file is sourced

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
ENV_FILE="${HH_LIVE_ENV:-$HOME/.config/harnessharness/live.env}"
LIVE_DIR="${HH_LIVE_DIR:-$REPO/docs/build/reports/live}"
PRICING="$LIVE_DIR/pricing.json"
SPEND="$LIVE_DIR/SPEND.jsonl"
SCAN_BIN="${HH_SECRET_SCAN:-$REPO/target/debug/hh-secret-scan}"
GITLEAKS_CFG="${HH_GITLEAKS_CFG:-$REPO/.gitleaks.toml}"

say()  { printf '%s\n' "live-verify: $*"; }
fail() { printf '%s\n' "live-verify: FAIL — $*" >&2; }
usage() {
  sed -n '3,18p' "$0"
  exit "${1:-1}"
}

# ── leg registry (bash-3.2 safe: case tables, no assoc arrays) ──────────
# fields per leg: ticket · provider · kind · required NAMEs
legs() { printf '%s\n' "local-reachability" "local-run" "https-reachability" "selftest"; }
leg_field() { # $1 leg, $2 field
  case "$1:$2" in
    local-reachability:ticket)   printf 'R4.5a';;
    local-reachability:provider) printf 'local-ollama';;
    local-reachability:kind)     printf 'script';;
    local-reachability:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    local-run:ticket)   printf 'R4.5b';;
    local-run:provider) printf 'local-ollama';;
    local-run:kind)     printf 'cargo-test';;
    local-run:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    local-run:test)     printf 'hh-embed:r4_5b_live:r45b_live_ollama_run_ledger_verify_replay';;
    https-reachability:ticket)   printf 'R4.5c';;
    https-reachability:provider) printf 'public-https';;
    https-reachability:kind)     printf 'script';;
    https-reachability:names)    printf '';;
    selftest:ticket)   printf 'selftest';;
    selftest:provider) printf 'none';;
    selftest:kind)     printf 'script';;
    selftest:names)    printf 'HH_SELFTEST_TOKEN';;
    *) return 1;;
  esac
}
known_leg() { local l; for l in $(legs); do [ "$l" = "$1" ] && return 0; done; return 1; }
# Every NAME the framework can report on (leg requirements + the HUMAN-H6
# provision set later legs consume).
all_names() {
  printf '%s\n' HH_LOCAL_BASE_URL HH_LOCAL_MODEL \
    OPENAI_API_KEY ANTHROPIC_API_KEY \
    HH_GITHUB_OAUTH_CLIENT_ID HH_GITHUB_SANDBOX_REPO \
    HH_GITHUB_SANDBOX_TOKEN HH_GITHUB_WEBHOOK_SECRET HH_SELFTEST_TOKEN | sort -u
}

load_env() { # names only; unexported shell vars, never traced
  [ -f "$ENV_FILE" ] || return 1
  set +x
  # shellcheck disable=SC1090
  . "$ENV_FILE"
}
provided() { local n="$1"; eval "[ -n \"\${$n:-}\" ]" && printf 'yes' || printf 'no'; }

cmd_check_names() { # $1: <leg>|--all|empty → NAME provided: yes|no (never a value)
  local names="" n
  case "${1:-}" in
    --all|"") names=$(all_names);;
    *) known_leg "$1" || { fail "unknown leg: $1"; return 1; }
       names=$(leg_field "$1" names);;
  esac
  load_env || say "(env file absent: $ENV_FILE — every name reports no)"
  for n in $names; do printf '%s provided: %s\n' "$n" "$(provided "$n")"; done
}

need_names() { # $1 leg → exit 4 listing every unprovided NAME
  local n missing="" st
  for n in $(leg_field "$1" names); do
    st=$(provided "$n"); printf '%s provided: %s\n' "$n" "$st"
    [ "$st" = yes ] || missing="$missing $n"
  done
  if [ -n "$missing" ]; then
    fail "required NAME(s) unprovided:$missing — provision via HUMAN-H6"
    exit 4
  fi
}

spend_cumulative() {
  [ -f "$SPEND" ] || { printf '0'; return; }
  grep -o '"cost_usd":[0-9.]*' "$SPEND" | cut -d: -f2 | awk '{s+=$1} END{printf "%.4f", s+0}'
}

budget_gate() { # $1 ticket, $2 declared budget → exit 3 on breach
  local alloc cap cum
  alloc=$(python3 -c "import json;print(json.load(open('$PRICING'))['allocations_usd'].get('$1',0))") \
    || { fail "pricing.json unreadable"; exit 7; }
  cap=$(python3 -c "import json;print(json.load(open('$PRICING'))['cap_usd'])")
  cum=$(spend_cumulative)
  python3 - "$2" "$alloc" "$cum" "$cap" "$1" <<'PY'
import sys
budget, alloc, cum, cap = (float(sys.argv[i]) for i in (1, 2, 3, 4))
ticket = sys.argv[5]
if budget > alloc:
    print(f"live-verify: SPEND CAP — declared budget ${budget:.2f} exceeds "
          f"{ticket}'s allocation ${alloc:.2f} — ask the operator", file=sys.stderr)
    sys.exit(3)
if cum + budget > cap:
    print(f"live-verify: SPEND CAP — cumulative ${cum:.2f} + ${budget:.2f} "
          f"exceeds the ${cap:.2f} cap — ask the operator", file=sys.stderr)
    sys.exit(3)
PY
}

require() { command -v "$1" >/dev/null 2>&1 || { fail "required tool absent: $1"; exit 7; }; }

scan_binary() {
  if [ ! -x "$SCAN_BIN" ]; then
    require cargo
    (cd "$REPO" && cargo build -q -p hh-secrets --bins) \
      || { fail "cannot build hh-secret-scan"; exit 7; }
  fi
  [ -x "$SCAN_BIN" ] || { fail "hh-secret-scan unavailable"; exit 7; }
  printf '%s\n' "$SCAN_BIN"
}

# The staged-diff secret gate (ADR-0363 D8). Args = paths just staged, so a
# refusal unstages exactly them (never the operator's other staged work).
gate_staged() {
  require gitleaks
  local out; out=$(mktemp)
  if gitleaks git --staged --redact -c "$GITLEAKS_CFG" "$REPO" >"$out" 2>&1; then
    say "gitleaks staged gate: clean"; rm -f "$out"; return 0
  fi
  fail "gitleaks finding(s) in the staged diff — STOP and ask the operator (redacted output follows)"
  sed 's/^/  /' "$out" >&2; rm -f "$out"
  [ $# -gt 0 ] && git -C "$REPO" reset -q -- "$@" 2>/dev/null || true
  return 6
}

# ── legs ────────────────────────────────────────────────────────────────
# A leg sets RESULT / ASSERTIONS / MODEL_REPORTED / ENDPOINT_HOST /
# TRANSCRIPT_RAW / RESP_HDRS (paths under $TMPD_L, "" when absent).

# kind: cargo-test — the gated-test leg form (ADR-0363 D7): runs the leg's
# `#[ignore = "live — ADR-0363: …"]` test(s) via `cargo test -- --ignored`.
# Registry field `test` is `<crate>:<test-binary>[:<filter>]`.
run_leg_cargo_test() { # $1 leg — sets the leg globals from the test run
  require cargo
  local spec crate tbin filt
  spec=$(leg_field "$1" test) \
    || { RESULT="skipped(no-test-field)"; TRANSCRIPT_RAW=""; RESP_HDRS=""; return; }
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  ASSERTIONS="the ignored live test(s) pass under cargo test -- --ignored"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ cargo test -p %s --test %s %s-- --ignored\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if [ $rc -eq 0 ]; then RESULT="pass"; MODEL_REPORTED=""
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
  ENDPOINT_HOST=""; MODEL_REPORTED=""
}

run_leg_selftest() { # no network; proves the framework end-to-end
  RESULT="pass"
  ASSERTIONS="selftest leg executed; framework path exercised end-to-end"
  MODEL_REPORTED=""; ENDPOINT_HOST=""; RESP_HDRS=""
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"
  printf 'selftest artifact — no network call performed\n' > "$TRANSCRIPT_RAW"
}

run_leg_local_reachability() {
  require curl
  local base="${HH_LOCAL_BASE_URL%/}" url ids
  case "$base" in */v1) url="$base/models";; *) url="$base/v1/models";; esac
  ENDPOINT_HOST=$(python3 - "$base" <<'PY'
import sys, urllib.parse
p = urllib.parse.urlparse(sys.argv[1])
print(p.hostname or "", end="")
if p.port:
    print(":%d" % p.port, end="")
PY
)
  ASSERTIONS="HH_LOCAL_MODEL is served by the OpenAI-compatible model listing at HH_LOCAL_BASE_URL"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS="$TMPD_L/hdrs"
  if ! curl -sS --max-time 15 -D "$RESP_HDRS" -o "$TMPD_L/body" "$url" 2>"$TMPD_L/curl.err"; then
    RESULT="skipped(unreachable: $ENDPOINT_HOST)"
    { printf 'GET %s HTTP/1.1\nHost: %s\nAccept: application/json\n\n<connection failed>\n' \
        "$url" "$ENDPOINT_HOST"; cat "$TMPD_L/curl.err"; } > "$TRANSCRIPT_RAW"
    return
  fi
  { printf 'GET %s HTTP/1.1\nHost: %s\nAccept: application/json\n\n' "$url" "$ENDPOINT_HOST"
    cat "$RESP_HDRS"; printf '\n'; cat "$TMPD_L/body"; } > "$TRANSCRIPT_RAW"
  ids=$(python3 - "$TMPD_L/body" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(0)
print("\n".join(str(m.get("id", "")) for m in d.get("data", [])))
PY
)
  if printf '%s\n' "$ids" | grep -Fxq "$HH_LOCAL_MODEL"; then
    RESULT="pass"; MODEL_REPORTED="$HH_LOCAL_MODEL"
  else
    RESULT="fail:assertion — HH_LOCAL_MODEL not served ($(printf '%s\n' "$ids" | grep -c .) model id(s) listed)"
    MODEL_REPORTED=""
  fi
}

run_leg_https_reachability() { # R4.5c — the real rustls client, USD 0
  require cargo
  ENDPOINT_HOST="api.github.com:443"
  ASSERTIONS="one unauthenticated HTTPS GET over the hh-tls rustls client (webpki roots only, full verification); HTTP 200 answer"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ cargo test -p hh-tls --test r4_5c_live -- --ignored\n\n'
    (cd "$REPO" && cargo test -p hh-tls --test r4_5c_live -- --ignored) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if grep -q "SKIPPED-UNREACHABLE" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(unreachable: $ENDPOINT_HOST)"
  elif [ "$rc" -eq 0 ]; then
    RESULT="pass"
  else
    RESULT="fail:live-tests rc=$rc"
  fi
  MODEL_REPORTED=""
}

# ── record + spend write (ADR-0363 D2/D3/D4) ────────────────────────────
emit_record() { # $1 leg $2 ticket $3 out.json — python writer, all inputs via env
  python3 - "$1" "$2" "$3" <<'PY'
import json, os, subprocess, sys
leg, ticket, out = sys.argv[1:4]
env = os.environ
def sh(*a):
    return subprocess.run(list(a), capture_output=True, text=True).stdout.strip()
pricing = json.load(open(env["PRICING"]))
resp_headers, request_ids = {}, []
allow = ("request-id", "x-request-id", "cf-ray", "openai-model",
         "openai-processing-ms", "openai-version", "x-served-model",
         "x-processing-time", "x-processing-time-ms", "retry-after")
hdrfile = env.get("LV_RESP_HDRS", "")
if hdrfile and os.path.exists(hdrfile):
    for line in open(hdrfile, errors="replace"):
        if ":" not in line:
            continue
        k, v = line.split(":", 1)
        k, v = k.strip().lower(), v.strip()
        if k in allow or k.startswith(("x-ratelimit-", "ratelimit-")):
            resp_headers[k] = v
            if k in ("request-id", "x-request-id", "cf-ray"):
                request_ids.append(v)
ledger = os.path.join(env["REPO"], "docs/build/runs", ticket + ".md")
run_head = ({"path": "docs/build/runs/%s.md" % ticket,
             "sha256": sh("shasum", "-a", "256", ledger).split(" ")[0]}
            if os.path.exists(ledger) else None)
rec = {
    "schema": "hh-live-evidence/1",
    "recorded_at": env["LV_RECORDED_AT"],
    "ticket": ticket,
    "leg": leg,
    "provider": env["LV_PROVIDER"],
    "endpoint_host": env["LV_ENDPOINT_HOST"] or None,
    "model_reported": env["LV_MODEL"] or None,
    "transcript_sha256": env["LV_TSHA"] or None,
    "transcript_path": env["LV_TREL"] or None,
    "token_usage": None,
    "cost_usd": float(env["LV_COST"]),
    "cost_pricing": {"table": "docs/build/reports/live/pricing.json",
                     "source": pricing["source"],
                     "retrieved": pricing["retrieved"]},
    "run_ledger_head": run_head,
    "result": env["LV_RESULT"],
    "assertions": [a for a in env["LV_ASSERTIONS"].split("\n") if a],
    "git_head": sh("git", "-C", env["REPO"], "rev-parse", "HEAD"),
    "harness": env["LV_HARNESS"],
    "provided": {n: env.get("LV_P_" + n, "no") for n in env["LV_NAMES"].split()},
    "request_ids": request_ids,
    "response_headers": resp_headers,
    "spend_cumulative_usd": float(env["LV_CUM_AFTER"]),
}
with open(out, "w") as f:
    json.dump(rec, f, indent=2, sort_keys=True)
    f.write("\n")
PY
}

cmd_run() { # $1 leg $2 budget $3 harness
  local leg="$1" budget="$2" harness="$3"
  known_leg "$leg" || { fail "unknown leg: $leg"; exit 1; }
  local ticket provider names
  ticket=$(leg_field "$leg" ticket); provider=$(leg_field "$leg" provider)
  names=$(leg_field "$leg" names)
  require python3; require shasum
  load_env || true
  need_names "$leg"
  budget_gate "$ticket" "$budget"
  local scanbin; scanbin=$(scan_binary)
  TMPD_L=$(mktemp -d); trap 'rm -rf "$TMPD_L"' EXIT
  RESULT="skipped(not-run)"; ASSERTIONS=""; MODEL_REPORTED=""
  ENDPOINT_HOST=""; TRANSCRIPT_RAW=""; RESP_HDRS=""
  case "$(leg_field "$leg" kind)" in
    script)     "run_leg_${leg//-/_}";;
    cargo-test) run_leg_cargo_test "$leg";;
    *) RESULT="skipped(unknown-leg-kind)";;
  esac
  say "leg $leg → $RESULT"

  # redact the transcript through the canonical redactor (D3)
  local tsha="" trel="" trec=""
  if [ -n "$TRANSCRIPT_RAW" ] && [ -f "$TRANSCRIPT_RAW" ]; then
    "$scanbin" redact --mask-env "$ENV_FILE" \
      < "$TRANSCRIPT_RAW" > "$TMPD_L/transcript.red" 2>/dev/null \
      || { fail "transcript redaction failed"; exit 7; }
    tsha=$(shasum -a 256 "$TMPD_L/transcript.red" | cut -d' ' -f1)
    if [ "$(wc -c < "$TMPD_L/transcript.red" | tr -d ' ')" -le 65536 ]; then
      trel="$(date -u +%F)-$leg.transcript.txt"
      case "$LIVE_DIR" in
        "$REPO"/docs/build/reports/live*) trec="docs/build/reports/live/$trel";;
        *) trec="$trel";;
      esac
    fi
  fi

  local cum_after cost="$budget"
  cum_after=$(python3 -c "print(round($(spend_cumulative) + $cost, 6))")

  # build the record + spend line into TMPD first — artifacts are swept
  # BEFORE anything lands in the tree (ADR-0363 D3/D6).
  local n datestr record
  for n in $names; do export "LV_P_$n=$(provided "$n")"; done
  export LV_RECORDED_AT="$(date -u +%FT%TZ)" LV_RESULT="$RESULT" \
    LV_ASSERTIONS="$ASSERTIONS" LV_ENDPOINT_HOST="$ENDPOINT_HOST" \
    LV_MODEL="$MODEL_REPORTED" LV_TSHA="$tsha" \
    LV_TREL="$trec" \
    LV_COST="$cost" LV_CUM_AFTER="$cum_after" LV_HARNESS="$harness" \
    LV_NAMES="$names" LV_PROVIDER="$provider" LV_RESP_HDRS="$RESP_HDRS" \
    PRICING="$PRICING" REPO="$REPO"
  emit_record "$leg" "$ticket" "$TMPD_L/record.json" \
    || { fail "record writer failed"; exit 7; }
  printf '{"ts":"%s","ticket":"%s","leg":"%s","cost_usd":%s,"cumulative_usd":%s,"record":"%s"}\n' \
    "$LV_RECORDED_AT" "$ticket" "$leg" "$cost" "$cum_after" \
    "$(date -u +%F)-$leg.json" > "$TMPD_L/spend.line"

  local -a sweep=("$TMPD_L/record.json" "$TMPD_L/spend.line")
  [ -n "$tsha" ] && sweep+=("$TMPD_L/transcript.red")
  if ! "$scanbin" scan --mask-env "$ENV_FILE" "${sweep[@]}"; then
    fail "leak-scan hit in evidence artifacts — nothing written"
    exit 5
  fi

  mkdir -p "$LIVE_DIR"
  datestr=$(date -u +%F)
  record="$LIVE_DIR/$datestr-$leg.json"
  cp "$TMPD_L/record.json" "$record"
  [ -n "$trel" ] && cp "$TMPD_L/transcript.red" "$LIVE_DIR/$trel"
  cat "$TMPD_L/spend.line" >> "$SPEND"
  say "record: $record"

  # staged-diff gate (D8) — only when the evidence lands inside the repo.
  local -a staged=("$record" "$SPEND")
  [ -n "$trel" ] && staged+=("$LIVE_DIR/$trel")
  case "$LIVE_DIR" in
    "$REPO"/docs/build/reports/live*)
      git -C "$REPO" add -- "${staged[@]}"
      gate_staged "${staged[@]}" || exit 6
      ;;
    *) say "evidence outside the repo (override dir) — staged gate n/a";;
  esac

  case "$RESULT" in
    pass) exit 0;;
    *) exit 2;;
  esac
}

main() {
  case "${1:-}" in
    --check-names) shift; cmd_check_names "${1:-}";;
    --gate) shift; gate_staged; exit $?;;
    --sweep) # the write-time artifact sweep, directly exercisable
      shift; load_env || true
      local sw rc
      sw=$(scan_binary)
      "$sw" scan --mask-env "$ENV_FILE" "$@" && { say "sweep clean"; exit 0; }
      rc=$?
      if [ "$rc" -eq 1 ]; then fail "leak-scan hit — refuse"; exit 5; fi
      fail "sweep could not run (scanner error $rc)"; exit 7;;
    ""|-h|--help) usage "${1:+0}";;
    -*)
      fail "unknown option: $1"; usage 1;;
    *)
      local leg="$1"; shift
      local budget="0" harness="${HH_LIVE_HARNESS:-unknown}"
      while [ $# -gt 0 ]; do
        case "$1" in
          --budget)  budget="${2:?--budget needs USD}"; shift 2;;
          --harness) harness="${2:?--harness needs an id}"; shift 2;;
          *) fail "unknown argument: $1"; usage 1;;
        esac
      done
      [[ "$budget" =~ ^[0-9]+(\.[0-9]+)?$ ]] \
        || { fail "--budget must be a non-negative USD number"; exit 1; }
      cmd_run "$leg" "$budget" "$harness";;
  esac
}
main "$@"
