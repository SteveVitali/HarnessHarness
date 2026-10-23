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
# Every artifact is swept by hh-secret-scan (the hharness-secrets detectors —
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
legs() { printf '%s\n' "local-reachability" "local-run" "https-reachability" "anthropic-call" "openai-call" "routing-probe" "lt03-live" "bench-manifest-live" "github-live" "oauth-device" "context-adapters" "judge-calibrate" "judge-run" "otel-live" "r4-15d-fetch" "capstone-run-local" "capstone-run-anthropic" "capstone-run-openai" "capstone-sandboxed-exec" "capstone-judge" "capstone-ledger-replay" "capstone-web" "capstone-mcp" "capstone-fleet-webhook" "capstone-bundle-repro" "l4-6b-replay" "quickstart-live" "selftest"; }
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
    anthropic-call:ticket)   printf 'R4.5d';;
    anthropic-call:provider) printf 'anthropic';;
    anthropic-call:kind)     printf 'script';;
    anthropic-call:names)    printf 'ANTHROPIC_API_KEY';;
    anthropic-call:test)     printf 'hh-embed:r4_5d_live:r45d_live_anthropic_call';;
    anthropic-call:pricekey) printf 'anthropic/claude-haiku-4-5-20251001';;
    anthropic-call:host)     printf 'api.anthropic.com:443';;
    openai-call:ticket)   printf 'R4.5d';;
    openai-call:provider) printf 'openai';;
    openai-call:kind)     printf 'script';;
    openai-call:names)    printf 'OPENAI_API_KEY';;
    openai-call:test)     printf 'hh-embed:r4_5d_live:r45d_live_openai_call';;
    openai-call:pricekey) printf 'openai/gpt-4.1-mini-2025-04-14';;
    openai-call:host)     printf 'api.openai.com:443';;
    routing-probe:ticket)   printf 'R4.5e';;
    routing-probe:provider) printf 'multi-provider';;
    routing-probe:kind)     printf 'script';;
    routing-probe:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL ANTHROPIC_API_KEY OPENAI_API_KEY';;
    routing-probe:test)     printf 'hh-embed:r4_5e_live:r45e_live_routing_probe';;
    routing-probe:pricekey.anthropic) printf 'anthropic/claude-haiku-4-5-20251001';;
    routing-probe:pricekey.openai)    printf 'openai/gpt-4.1-mini-2025-04-14';;
    lt03-live:ticket)   printf 'R4.6';;
    lt03-live:provider) printf 'multi-provider';;
    lt03-live:kind)     printf 'script';;
    lt03-live:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL ANTHROPIC_API_KEY';;
    lt03-live:test)     printf 'hh-embed:r4_6_live:r46_lt03_live_corpus_arm';;
    lt03-live:pricekey.anthropic) printf 'anthropic/claude-haiku-4-5-20251001';;
    bench-manifest-live:ticket)   printf 'R4.6';;
    bench-manifest-live:provider) printf 'public-upstream';;
    bench-manifest-live:kind)     printf 'script';;
    bench-manifest-live:names)    printf '';;
    bench-manifest-live:test)     printf 'hh-bench:r4_6:r46_live_manifest_import';;
    bench-manifest-live:host)     printf 'raw.githubusercontent.com:443';;
    github-live:ticket)   printf 'R4.7';;
    github-live:provider) printf 'github-sandbox';;
    github-live:kind)     printf 'script';;
    github-live:names)    printf 'HH_GITHUB_SANDBOX_TOKEN HH_GITHUB_SANDBOX_REPO HH_GITHUB_WEBHOOK_SECRET';;
    github-live:test)     printf 'hh-embed:r4_7_live:r47_github_live';;
    oauth-device:ticket)   printf 'R4.7a';;
    oauth-device:provider) printf 'github-oauth';;
    oauth-device:kind)     printf 'script';;
    oauth-device:names)    printf 'HH_GITHUB_OAUTH_CLIENT_ID';;
    context-adapters:ticket)   printf 'R4.7b';;
    context-adapters:provider) printf 'multi-provider';;
    context-adapters:kind)     printf 'script';;
    context-adapters:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL OPENAI_API_KEY ANTHROPIC_API_KEY';;
    context-adapters:test)     printf 'hh-embed:r4_7b_live';;
    context-adapters:pricekey.openai)    printf 'openai/text-embedding-3-small';;
    context-adapters:pricekey.anthropic) printf 'anthropic/claude-haiku-5-5';;
    judge-calibrate:ticket)   printf 'R4.7e';;
    judge-calibrate:provider) printf 'multi-provider';;
    judge-calibrate:kind)     printf 'script';;
    judge-calibrate:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL OPENAI_API_KEY';;
    judge-calibrate:test)     printf 'hharness-embed:r4_7e_live:r47e_live_judge_calibrate';;
    judge-calibrate:pricekey) printf 'openai/gpt-4.1-mini-2025-04-14';;
    judge-run:ticket)   printf 'R4.7e';;
    judge-run:provider) printf 'multi-provider';;
    judge-run:kind)     printf 'script';;
    judge-run:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL OPENAI_API_KEY';;
    judge-run:test)     printf 'hharness-embed:r4_7e_live:r47e_live_judge_run';;
    judge-run:pricekey) printf 'openai/gpt-4.1-mini-2025-04-14';;
    otel-live:ticket)   printf 'R4.7c';;
    otel-live:provider) printf 'local-otel-collector';;
    otel-live:kind)     printf 'script';;
    otel-live:names)    printf '';;
    r4-15d-fetch:ticket)   printf 'R4.15d';;
    r4-15d-fetch:provider) printf 'public-https';;
    r4-15d-fetch:kind)     printf 'script';;
    r4-15d-fetch:names)    printf '';;
    r4-15d-fetch:test)     printf 'hh-embed:r4_15d_live:r415d_live_member_fetch';;
    r4-15d-fetch:host)     printf 'raw.githubusercontent.com:443';;
    capstone-run-local:ticket)   printf 'L4.6';;
    capstone-run-local:provider) printf 'local-ollama';;
    capstone-run-local:kind)     printf 'script';;
    capstone-run-local:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    capstone-run-local:test)     printf 'hharness-embed:l4_6_live:l46_live_reference_task';;
    capstone-run-local:host)     printf '127.0.0.1:11434';;
    capstone-run-anthropic:ticket)   printf 'L4.6';;
    capstone-run-anthropic:provider) printf 'anthropic';;
    capstone-run-anthropic:kind)     printf 'script';;
    capstone-run-anthropic:names)    printf 'ANTHROPIC_API_KEY';;
    capstone-run-anthropic:test)     printf 'hharness-embed:l4_6_live:l46_live_reference_task';;
    capstone-run-anthropic:pricekey) printf 'anthropic/claude-haiku-4-5-20251001';;
    capstone-run-anthropic:host)     printf 'api.anthropic.com:443';;
    capstone-run-openai:ticket)   printf 'L4.6';;
    capstone-run-openai:provider) printf 'openai';;
    capstone-run-openai:kind)     printf 'script';;
    capstone-run-openai:names)    printf 'OPENAI_API_KEY';;
    capstone-run-openai:test)     printf 'hharness-embed:l4_6_live:l46_live_reference_task';;
    capstone-run-openai:pricekey) printf 'openai/gpt-4.1-mini-2025-04-14';;
    capstone-run-openai:host)     printf 'api.openai.com:443';;
    capstone-sandboxed-exec:ticket)   printf 'L4.6';;
    capstone-sandboxed-exec:provider) printf 'local-sandbox';;
    capstone-sandboxed-exec:kind)     printf 'script';;
    capstone-sandboxed-exec:names)    printf '';;
    capstone-judge:ticket)   printf 'L4.6';;
    capstone-judge:provider) printf 'multi-provider';;
    capstone-judge:kind)     printf 'script';;
    capstone-judge:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL OPENAI_API_KEY';;
    capstone-judge:test)     printf 'hharness-embed:r4_7e_live:r47e_live_judge_run';;
    capstone-judge:pricekey) printf 'openai/gpt-4.1-mini-2025-04-14';;
    capstone-ledger-replay:ticket)   printf 'L4.6';;
    capstone-ledger-replay:provider) printf 'local-ollama';;
    capstone-ledger-replay:kind)     printf 'script';;
    capstone-ledger-replay:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    capstone-ledger-replay:test)     printf 'hharness-embed:l4_6_live:l46_live_ledger_replay';;
    capstone-ledger-replay:host)     printf '127.0.0.1:11434';;
    capstone-web:ticket)   printf 'L4.6';;
    capstone-web:provider) printf 'local-loopback';;
    capstone-web:kind)     printf 'script';;
    capstone-web:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    capstone-web:test)     printf 'hharness-embed:l4_6_live:l46_live_web';;
    capstone-web:host)     printf '127.0.0.1';;
    capstone-mcp:ticket)   printf 'L4.6';;
    capstone-mcp:provider) printf 'mcp-fixture';;
    capstone-mcp:kind)     printf 'script';;
    capstone-mcp:names)    printf '';;
    capstone-mcp:test)     printf 'hharness-mcp:l4_6_live:l46_live_mcp_client';;
    capstone-mcp:host)     printf '127.0.0.1';;
    capstone-fleet-webhook:ticket)   printf 'L4.6';;
    capstone-fleet-webhook:provider) printf 'github-sandbox';;
    capstone-fleet-webhook:kind)     printf 'script';;
    capstone-fleet-webhook:names)    printf 'HH_GITHUB_SANDBOX_TOKEN HH_GITHUB_SANDBOX_REPO HH_GITHUB_WEBHOOK_SECRET';;
    capstone-fleet-webhook:host)     printf 'api.github.com:443';;
    capstone-bundle-repro:ticket)   printf 'L4.6';;
    capstone-bundle-repro:provider) printf 'local-ollama';;
    capstone-bundle-repro:kind)     printf 'script';;
    capstone-bundle-repro:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    capstone-bundle-repro:test)     printf 'hharness-embed:l4_6_live:l46_live_bundle_repro';;
    capstone-bundle-repro:host)     printf '127.0.0.1:11434';;
    l4-6b-replay:ticket)   printf 'L4.6b';;
    l4-6b-replay:provider) printf 'none';;
    l4-6b-replay:kind)     printf 'script';;
    l4-6b-replay:names)    printf '';;
    l4-6b-replay:test)     printf 'hharness-embed:l4_6b:l46b_replay_effect_run';;
    l4-6b-replay:host)     printf '127.0.0.1';;
    quickstart-live:ticket)   printf 'L4.7';;
    quickstart-live:provider) printf 'local-ollama';;
    quickstart-live:kind)     printf 'script';;
    quickstart-live:names)    printf 'HH_LOCAL_BASE_URL HH_LOCAL_MODEL';;
    quickstart-live:host)     printf '127.0.0.1:11434';;
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
  printf '%s\n' HH_LOCAL_BASE_URL HH_LOCAL_MODEL HH_LOCAL_EMBED_MODEL \
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
  # 6 decimals — SPEND rows carry six; a %.4f fold truncates a fine-grained
  # history and a zero-cost leg then records a *decreasing* cumulative.
  grep -o '"cost_usd":[0-9.]*' "$SPEND" | cut -d: -f2 | awk '{s+=$1} END{printf "%.6f", s+0}'
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
    (cd "$REPO" && cargo build -q -p hharness-secrets --bins) \
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
  ASSERTIONS="one unauthenticated HTTPS GET over the hharness-tls rustls client (webpki roots only, full verification); HTTP 200 answer"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ cargo test -p hharness-tls --test r4_5c_live -- --ignored\n\n'
    (cd "$REPO" && cargo test -p hharness-tls --test r4_5c_live -- --ignored) 2>&1
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

# R4.15d — `r4-15d-fetch`: ONE real `kernel.fetch` over the public
# Internet (OQ-332; the GATE-G7-adopted §trust-fields set). The pinned
# Harbor README member rides the adopted seam end to end — `first_party`
# admission against the declared `admitted_origins`, the Π-mediated
# `security.egress.{requested,decided}` trail on the kernel fetch run,
# `hharness_tls` webpki-roots verification, content-address re-verify on
# arrival, `bundle.member.fetched` durable before the deposit, and the
# blob-pool read-back byte-identical to the committed pin. USD 0,
# read-only, no secrets, no extra roots (the public chain is the claim).
run_leg_r4_15d_fetch() {
  require cargo
  ENDPOINT_HOST=$(leg_field "r4-15d-fetch" host)
  ASSERTIONS="one kernel.fetch{trust:FetchTrustPolicy(first_party, admitted_origins=[raw.githubusercontent.com], require_ledgered_arrival)} materializing the pinned harbor-framework/harbor README.md member (commit dd88157): admit → security.egress.requested/decided durable on the kernel fetch run → hharness_tls webpki-roots GET → content-address re-verify → bundle.member.fetched{verified} durable-before-deposit → blob-pool read-back byte-identical; USD 0 read-only, no secrets"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local spec crate tbin filt rc=0
  spec=$(leg_field "r4-15d-fetch" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  {
    printf '$ cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
  } > "$TRANSCRIPT_RAW" || rc=$?
  if grep -q "SKIPPED-UNREACHABLE" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(unreachable: $ENDPOINT_HOST)"
  elif [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local row
    row=$(grep '^LIVE_FETCH: ' "$TRANSCRIPT_RAW" | tail -1 || true)
    [ -n "$row" ] && ASSERTIONS="$ASSERTIONS; $row"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else
    RESULT="fail:live-tests rc=$rc"
  fi
  MODEL_REPORTED=""
}

# R4.5d — the hosted-provider legs: one minimal real call each through
# `GatewayModel` (the `r4_5d_live` ignored cells). The test prints
# `LIVE_USAGE:`/`LIVE_MODEL:` markers the leg parses into the evidence
# record's `token_usage` and prices against the pinned `pricing.json`
# row (`LEG_COST_MEASURED` — measured usage × pinned rates; the declared
# `--budget` stays the worst-case gate).
run_leg_hosted_call() { # $1 leg — sets the leg globals from the test run
  require cargo; require python3
  local spec crate tbin filt
  spec=$(leg_field "$1" test) \
    || { RESULT="skipped(no-test-field)"; TRANSCRIPT_RAW=""; RESP_HDRS=""; return; }
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  # The credential NAMEs export to the cargo child — values never print.
  local n
  for n in $(leg_field "$1" names); do export "$n"; done
  ENDPOINT_HOST=$(leg_field "$1" host)
  ASSERTIONS="one minimal hosted call through GatewayModel: model.call.requested carries the launch_env:* coordinate binding_id; model.call.completed lands with usage{available,record{view,raw}}; the credential value appears in no payload/blob; ledger verifies; measured usage rides token_usage + the pinned-rate cost_usd"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local usage model pricekey
  usage=$(grep '^LIVE_USAGE: ' "$TRANSCRIPT_RAW" | head -1 | sed 's/^LIVE_USAGE: //' || true)
  model=$(grep '^LIVE_MODEL: ' "$TRANSCRIPT_RAW" | head -1 | sed 's/^LIVE_MODEL: //' || true)
  if [ $rc -eq 0 ]; then
    RESULT="pass"; MODEL_REPORTED="$model"
    if [ -n "$usage" ]; then
      LEG_TOKEN_USAGE="$usage"
      pricekey=$(leg_field "$1" pricekey)
      LEG_COST_MEASURED=$(python3 - "$PRICING" "$pricekey" "$usage" <<'PY'
import json, sys
pricing = json.load(open(sys.argv[1]))
row = pricing["models"].get(sys.argv[2])
if row is None:
    sys.exit("live-verify: NoPrice — no pinned row for " + sys.argv[2])
u = json.loads(sys.argv[3])
def f(k):
    v = u.get(k)
    return float(v) if v is not None else 0.0
i, o, cr, cw = f("input_total"), f("output_total"), f("cache_read"), f("cache_write")
uncached = max(i - cr - cw, 0.0)
c = (uncached * row["prompt_per_1k_usd"]
     + cr * row.get("cache_read_per_1k_usd", row["prompt_per_1k_usd"])
     + cw * row.get("cache_write_per_1k_usd", row["prompt_per_1k_usd"])
     + o * row["completion_per_1k_usd"]) / 1000.0
print("%.6f" % c)
PY
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    fi
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}

run_leg_anthropic_call() { run_leg_hosted_call "anthropic-call"; }
run_leg_openai_call() { run_leg_hosted_call "openai-call"; }

# R4.5e — the `routing-probe` leg (ADR-0363 D7; DF-S1.18-1 / ADR-0284 (b)):
# `probe_profile` executed against all three admitted families at open —
# the authenticated model listing folds the `served_model.<id>` pin and
# the capability walk rides the one measured tiny probe call per family —
# then one routed run the `health_aware` arm selects from the
# probe-derived health. The test's `LIVE_PROBE_USAGE: <dialect> <json>`
# markers carry the measured probe usage per family; the hosted
# families' spend prices against the pinned `pricing.json` rows (the
# local family's probe rides the zero-spend leg).
run_leg_routing_probe() { routing_probe_exec "routing-probe"; }

routing_probe_exec() { # $1 leg — sets the leg globals from the test run
  require cargo; require python3
  local spec crate tbin filt
  spec=$(leg_field "$1" test) \
    || { RESULT="skipped(no-test-field)"; TRANSCRIPT_RAW=""; RESP_HDRS=""; return; }
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  # The credential NAMEs export to the cargo child — values never print.
  local n
  for n in $(leg_field "$1" names); do export "$n"; done
  ENDPOINT_HOST=""
  ASSERTIONS="probe_exec executed for all three families at open: model.profile.probed{records[],observed.usage} durable before the first model.call.requested; the health_aware arm selects from the probe-derived health (model.route.decided, LIVE_SELECTED); the routed local call completes with usage; no provisioned value in any payload/blob; ledger verifies; the hosted families' measured probe usage prices against the pinned rows"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local usage model
  usage=$(grep '^LIVE_USAGE: ' "$TRANSCRIPT_RAW" | head -1 | sed 's/^LIVE_USAGE: //' || true)
  model=$(grep '^LIVE_MODEL: ' "$TRANSCRIPT_RAW" | head -1 | sed 's/^LIVE_MODEL: //' || true)
  if [ $rc -eq 0 ]; then
    RESULT="pass"; MODEL_REPORTED="$model"
    [ -n "$usage" ] && LEG_TOKEN_USAGE="$usage"
    # Price the hosted families' measured probe-call usage — the local
    # family's probe is the declared zero-spend leg.
    local lines ka ko
    ka=$(leg_field "$1" pricekey.anthropic)
    ko=$(leg_field "$1" pricekey.openai)
    lines=$(grep '^LIVE_PROBE_USAGE: ' "$TRANSCRIPT_RAW" | sed 's/^LIVE_PROBE_USAGE: //' || true)
    if [ -n "$lines" ]; then
      LEG_COST_MEASURED=$(python3 - "$PRICING" "$ka" "$ko" "$lines" <<'PY'
import json, sys
pricing = json.load(open(sys.argv[1]))
key = {"anthropic-messages@1": sys.argv[2], "openai@1": sys.argv[3]}
total, seen = 0.0, set()
for line in sys.argv[4].splitlines():
    line = line.strip()
    if not line:
        continue
    dialect, _, uj = line.partition(" ")
    u = json.loads(uj)
    seen.add(dialect)
    pk = key.get(dialect)
    if pk is None:
        continue  # the local family's zero-spend leg
    row = pricing["models"].get(pk)
    if row is None:
        sys.exit("live-verify: NoPrice — no pinned row for " + pk)
    total += (u.get("input_tokens", 0) * row["prompt_per_1k_usd"]
              + u.get("output_tokens", 0) * row["completion_per_1k_usd"]) / 1000.0
if not {"anthropic-messages@1", "openai@1"} <= seen:
    sys.exit("live-verify: missing LIVE_PROBE_USAGE for a hosted family")
print("%.6f" % total)
PY
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    fi
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}

# R4.6 — `lt03-live` (DF-S1.13-3's LT-03 live-corpus arm): the pinned
# AgentDojo-shaped injection battery driven through real model calls on
# BOTH model families in one leg — the provisioned local model plus the
# one hosted provider (Anthropic haiku). The test's `LT03_TASK` markers
# carry the per-task outcome (`carried` | `refused` | `leak_detected` —
# every outcome is a pass shape: the wire holds the placeholder, the
# request was refused, or the veto's detector input landed); the
# `LT03_ASSERT` markers carry the durable halves (out-of-scope refusal,
# canary → leak_detected, sink value → known_value veto). Only the
# hosted family's `LT03_USAGE` prices (the local lane is zero-spend).
run_leg_lt03_live() {
  require cargo; require python3
  local spec crate tbin filt rc_total=0
  spec=$(leg_field "lt03-live" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  local n; for n in $(leg_field "lt03-live" names); do export "$n"; done
  ENDPOINT_HOST=""; MODEL_REPORTED=""
  ASSERTIONS="AgentDojo-shaped injection battery (pinned upstream corpus, tests/fixtures/r4_6/lt03-battery.json) over real model calls on both families: per task the outbound request carries the mh_secret: placeholder or is refused; a canary bind lands durable leak_detected; a real value at a sink is success-with-veto (known_value); no provisioned value in any payload/blob"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  : > "$TRANSCRIPT_RAW"
  local fam rc
  for fam in local anthropic; do
    rc=0
    if {
      printf '$ HH_LT03_FAMILY=%s cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' \
        "$fam" "$crate" "$tbin" "$filt"
      (cd "$REPO" && HH_LT03_FAMILY="$fam" \
        cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
    } >> "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
    [ "$rc" -ne 0 ] && rc_total=$rc
  done
  local ntask nref nleak usage
  ntask=$(grep -c '^LT03_TASK ' "$TRANSCRIPT_RAW" || true)
  nref=$(grep -c '^LT03_TASK .* refused$' "$TRANSCRIPT_RAW" || true)
  nleak=$(grep -c '^LT03_TASK .* leak_detected$' "$TRANSCRIPT_RAW" || true)
  usage=$(grep '^LT03_USAGE anthropic ' "$TRANSCRIPT_RAW" | head -1 | sed 's/^LT03_USAGE anthropic //' || true)
  if [ "$rc_total" -eq 0 ]; then
    RESULT="pass"
    ASSERTIONS="$ASSERTIONS; LT03_TASK rows=$ntask refused=$nref leak_detected=$nleak across both families"
    if [ -n "$usage" ]; then
      LEG_TOKEN_USAGE="$usage"
      local ka; ka=$(leg_field "lt03-live" pricekey.anthropic)
      LEG_COST_MEASURED=$(python3 - "$PRICING" "$ka" "$usage" <<'PY'
import json, sys
pricing = json.load(open(sys.argv[1]))
row = pricing["models"].get(sys.argv[2])
if row is None:
    sys.exit("live-verify: NoPrice — no pinned row for " + sys.argv[2])
u = json.loads(sys.argv[3])
def f(k):
    v = u.get(k)
    return float(v) if v is not None else 0.0
i, o, cr, cw = f("input_total"), f("output_total"), f("cache_read"), f("cache_write")
uncached = max(i - cr - cw, 0.0)
c = (uncached * row["prompt_per_1k_usd"]
     + cr * row.get("cache_read_per_1k_usd", row["prompt_per_1k_usd"])
     + cw * row.get("cache_write_per_1k_usd", row["prompt_per_1k_usd"])
     + o * row["completion_per_1k_usd"]) / 1000.0
print("%.6f" % c)
PY
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    fi
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc_total"; fi
}

# R4.6 — `bench-manifest-live` (DF-S1.24-2): the real upstream foreign
# manifest — the Harbor Index `registry.json` at the committed pin —
# fetched over read-only public HTTPS, sha256-verified against the
# fixture's recorded provenance, then imported through the canonical
# importer with L1/L2 admission + `task_id` round-trip stability (the
# test prints `BENCH_IMPORT …`; USD 0 — public data).
run_leg_bench_manifest_live() {
  require curl; require cargo; require python3; require shasum
  ENDPOINT_HOST=$(leg_field "bench-manifest-live" host); MODEL_REPORTED=""
  ASSERTIONS="real upstream manifest (Harbor Index registry.json at the committed pin) sha256-verified then imported: L1 record.validate per task, L2 manifest.validate + split_assignment.validate, task_id stable through import round-trips (BENCH_IMPORT marker); USD 0 public read-only fetch"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS="$TMPD_L/hdrs"
  local fixture="$REPO/crates/hh-bench/fixtures/live/r4_6/harbor-registry-subset.json"
  local url pin
  url=$(python3 -c "import json;print(json.load(open('$fixture'))['upstream']['url'])") \
    || { RESULT="fail:no-pin"; return; }
  pin=$(python3 -c "import json;print(json.load(open('$fixture'))['upstream']['source_sha256'])")
  if ! curl -sS --max-time 120 -fL -D "$RESP_HDRS" -o "$TMPD_L/registry.json" "$url" \
      2>"$TMPD_L/curl.err"; then
    RESULT="skipped(unreachable: $ENDPOINT_HOST)"
    { printf 'GET %s\n\n<connection failed>\n' "$url"; cat "$TMPD_L/curl.err"; } \
      > "$TRANSCRIPT_RAW"
    return
  fi
  local got
  got=$(shasum -a 256 "$TMPD_L/registry.json" | cut -d' ' -f1)
  {
    printf 'GET %s\nsha256-pin %s\nsha256-got %s\n\n' "$url" "$pin" "$got"
    cat "$RESP_HDRS"; printf '\n'
  } > "$TRANSCRIPT_RAW"
  if [ "$got" != "$pin" ]; then
    RESULT="fail:pin-drift — fetched manifest sha256 != committed pin"
    return
  fi
  local spec crate tbin filt rc=0
  spec=$(leg_field "bench-manifest-live" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  {
    printf '$ HH_FOREIGN_MANIFEST=<verified-fetch> cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' \
      "$crate" "$tbin" "$filt"
    (cd "$REPO" && HH_FOREIGN_MANIFEST="$TMPD_L/registry.json" \
      cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
  } >> "$TRANSCRIPT_RAW" || rc=$?
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local row
    row=$(grep '^BENCH_IMPORT ' "$TRANSCRIPT_RAW" | tail -1 || true)
    [ -n "$row" ] && ASSERTIONS="$ASSERTIONS; $row"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}

# R4.7 — `github-live` (DF-S5.6-1; HUMAN-H2; AC-R-2.12.6-9): the real
# GitHub Issues adapter + signed webhook lane, exercised against the
# private sandbox repo `HH_GITHUB_SANDBOX_REPO` names — one runtime
# `cloudflared` quick tunnel, one webhook (pre-state GET recorded,
# `DELETE` in the trap), one test issue + comment rolled back by
# close+`hh-live-test`, the signed X-Hub-Signature-256 deliveries admitted
# at `fleet.webhook_ingress` → `fleet.reconcile` → WorkItem, plus the
# capability-probe/source-read and out-of-process plugin cells.
# `scripts/live-github.sh` is the orchestrator; the cells are the gated
# `r4_7_live` tests; both credentials resolve through the real broker
# inside the test (values never print — GH_* markers carry the
# assertion view). USD 0 — GitHub + Cloudflare quick tunnels are free.
run_leg_github_live() {
  require cargo; require python3; require cloudflared
  local n; for n in $(leg_field "github-live" names); do export "$n"; done
  ENDPOINT_HOST="api.github.com:443"; MODEL_REPORTED=""
  ASSERTIONS="sandbox-scoped: capability probe + list/get over the live repo; one webhook (pre-state GET, unconditional DELETE in trap); signed X-Hub-Signature-256 issues/issue_comment deliveries admitted at fleet.webhook_ingress and folded to a WorkItem at fleet.reconcile; replay dedupe + stale-window no-op; out-of-process subprocess_confined capability probe over the live snapshot; rollback = hook deleted + issue closed+hh-live-test; GH_* markers carry the assertion view; credentials broker-resolved — provided: yes only"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ HH_GH_DIR=<tmp> %s\n\n' "scripts/live-github.sh"
    (cd "$REPO" && HH_GH_DIR="$TMPD_L/gh" bash scripts/live-github.sh) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local markers
    markers=$(grep '^GH_' "$TRANSCRIPT_RAW" | tr '\n' ';' || true)
    [ -n "$markers" ] && ASSERTIONS="$ASSERTIONS; $markers"
  elif grep -q "GH_SKIP" "$TRANSCRIPT_RAW"; then
    RESULT="skipped($(grep 'GH_SKIP' "$TRANSCRIPT_RAW" | head -1 | sed 's/.*GH_SKIP *//'))"
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  else
    RESULT="fail:live-tests rc=$rc"
  fi
}

# R4.7a — `oauth-device` (DF-S4.11-2; the 2026-10-20 "GitHub OAuth App"
# ruling; operator present to enter the code): the resumable GitHub
# device flow over the admitted hharness-tls/hharness-wire stack — `initiate` writes
# the operator-facing code file + resumable state under the gitignored
# logs/ subtree (the user_code NEVER enters a transcript/record/ledger),
# `complete` polls → access token → `GET /user` subject →
# `OAUTH_SUBJECT_SHA256`. `HH_OAUTH_PHASE` selects
# initiate|complete|auto (default auto — initiate then poll up to
# `HH_OAUTH_POLL_CAP_S`, default 840s inside GitHub's ~900s window).
# USD 0 — the device flow needs no client secret.
run_leg_oauth_device() {
  require cargo; require python3
  local n; for n in $(leg_field "oauth-device" names); do export "$n"; done
  export HH_OAUTH_STATE="${HH_OAUTH_STATE:-$REPO/docs/build/logs/oauth-device.state.json}"
  export HH_OAUTH_CODE_FILE="${HH_OAUTH_CODE_FILE:-$REPO/docs/build/logs/oauth-device.code}"
  ENDPOINT_HOST="github.com:443"; MODEL_REPORTED=""
  ASSERTIONS="GitHub device flow over hharness-tls+hh-wire: initiate → operator-facing code file (gitignored, never a transcript/ledger); complete → poll (pending/slow_down/denied/expired handled) → access token held exchange-scope only → GET /user subject; record carries provided: yes + subject_login_sha256 + outcome; USD 0"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local rc=0
  if {
    printf '$ HH_OAUTH_PHASE=%s %s\n\n' "${HH_OAUTH_PHASE:-auto}" "scripts/live-oauth-device.sh"
    (cd "$REPO" && bash scripts/live-oauth-device.sh "${HH_OAUTH_PHASE:-auto}") 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local verdict
  verdict=$(grep '^OAUTH_RESULT ' "$TRANSCRIPT_RAW" | tail -1 | sed 's/^OAUTH_RESULT //' || true)
  case "$verdict" in
    pass) RESULT="pass";;
    initiated) RESULT="skipped(environment-pending — awaiting operator device-code entry)";;
    skipped\(*)
      RESULT="skipped($(printf '%s' "$verdict" | sed 's/^skipped(//;s/)$//') — awaiting operator device-code entry)";;
    skipped*) RESULT="$verdict";;
    "") RESULT="fail:live-tests rc=$rc";;
    *) RESULT="fail:$verdict";;
  esac
  local subj
  subj=$(grep '^OAUTH_SUBJECT_SHA256 ' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.* //' || true)
  # `if`, never `&&` as the function's last statement — an empty $subj
  # would exit-1 under `set -e` and kill cmd_run before emit_record.
  if [ -n "$subj" ]; then ASSERTIONS="$ASSERTIONS; subject_login_sha256=$subj"; fi
}


# R4.7b — `context-adapters` (BL-31's adapter member; ADR-0253/ADR-0313
# fired-answered): the lane-backed adapters over real providers — the
# local lane's embedding batch (llama.cpp/Ollama `openai-compatible@1`;
# a capability-absent endpoint answers the provider's own typed refusal,
# recorded — never a fabricated vector) + local summarization through
# `LaneSummarizer`/`GatewayModel`, then the hosted legs: `openai@1`
# `POST /v1/embeddings` over the benchset corpus slice (the recorded
# vectors are the committed `hh-embedding-cache/1` fixture the hermetic
# battery replays — `HH_EMBED_CACHE_OUT`) and `anthropic-messages@1`'s
# provider-native compaction (`anthropic-beta: compact-2026-09-04`).
# `LIVE_EMBED_USAGE`/`LIVE_COMPACT_USAGE` markers price against the
# pinned rows; the local cells are the declared zero-spend legs.
run_leg_context_adapters() {
  require cargo; require python3
  local spec crate tbin filt
  spec=$(leg_field "context-adapters" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  local n; for n in $(leg_field "context-adapters" names); do export "$n"; done
  # The optional embedding-model name reaches the test only when set —
  # absent it is not a required name (the local lane may already name a
  # model that serves embeddings).
  [ -n "${HH_LOCAL_EMBED_MODEL:-}" ] && export HH_LOCAL_EMBED_MODEL
  ENDPOINT_HOST=""; MODEL_REPORTED=""
  ASSERTIONS="the lane-backed adapters over real providers: local embedding batch (or the endpoint's own typed refusal, recorded) + local summarization through LaneSummarizer/GatewayModel; hosted openai@1 embeddings over the benchset corpus slice — the recorded vectors land the committed hh-embedding-cache/1 fixture the hermetic battery replays; anthropic-messages@1 provider-native compaction (compact-2026-09-04) — stop_reason:compaction, no structured forgotten ranges (all_prior verbatim); usage rides the charge records; provided names only"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local cache="$REPO/crates/hh-embed/tests/fixtures/r4_7b/embedding_cache.json"
  mkdir -p "$(dirname "$cache")"
  local rc=0
  if {
    printf '$ HH_EMBED_CACHE_OUT=<fixture> HH_LOCAL_EMBED_MODEL=<optional> cargo test -p %s --test %s -- --ignored --nocapture --test-threads=1\n\n' "$crate" "$tbin"
    (cd "$REPO" && HH_EMBED_CACHE_OUT="$cache" \
      cargo test -p "$crate" --test "$tbin" -- --ignored --nocapture --test-threads=1) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local usage model
  # Markers are unanchored — libtest may prefix a line with
  # `test <name> ...` while the ignored cell prints.
  model=$(grep -o 'LIVE_MODEL: .*' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.*LIVE_MODEL: //' || true)
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"; MODEL_REPORTED="$model"
    local eu cu ka ko
    eu=$(grep -o 'LIVE_EMBED_USAGE: .*' "$TRANSCRIPT_RAW" | head -1 | sed 's/.*LIVE_EMBED_USAGE: //' || true)
    cu=$(grep -o 'LIVE_COMPACT_USAGE: .*' "$TRANSCRIPT_RAW" | head -1 | sed 's/.*LIVE_COMPACT_USAGE: //' || true)
    ka=$(leg_field "context-adapters" pricekey.anthropic)
    ko=$(leg_field "context-adapters" pricekey.openai)
    LEG_COST_MEASURED=$(python3 - "$PRICING" "$ko" "$ka" "$eu" "$cu" <<'PRICE'
import json, sys
pricing = json.load(open(sys.argv[1]))
def price(key, uj):
    if not uj:
        return 0.0
    row = pricing["models"].get(key)
    if row is None:
        sys.exit("live-verify: NoPrice — no pinned row for " + key)
    u = json.loads(uj)
    return (u.get("input_total", 0) * row["prompt_per_1k_usd"]
            + u.get("output_total", 0) * row["completion_per_1k_usd"]) / 1000.0
print("%.6f" % (price(sys.argv[2], sys.argv[4]) + price(sys.argv[3], sys.argv[5])))
PRICE
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    [ -n "$eu" ] && LEG_TOKEN_USAGE="$eu"
    local marks
    marks=$(grep -oE 'LIVE_(EMBED|SUMM|COMPACT)[ :].*' "$TRANSCRIPT_RAW" | sed 's/^test [^ ]* \.\.\. //' | tr '\n' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
    grep -q 'cache-recorded' "$TRANSCRIPT_RAW"       || ASSERTIONS="$ASSERTIONS; WARN no cache-recorded marker — the hermetic cache fixture was not rewritten this leg"
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}

# R4.7e — `judge-calibrate` + `judge-run` (DF-S1.21-3's live half;
# DF-S1.21-2's last member; R-2.7.2b): the different-family judge pair —
# the local `openai-compatible@1` lane + the hosted `openai@1` lane
# (`gpt-4.1-mini`, `env:OPENAI_API_KEY` a coordinate) — calibrated over
# the committed labelled set (`tests/fixtures/r4_7e/labelled_cases.json`,
# the stage3_v1-derived cases + distractors), then bound at link
# (independence + RedundantJudge + the calibration fold consumed for
# real) and driven on a real `Driver` run: judged `verification.*` rows +
# `idp:` model_io blobs durable through the fenced drain, per-critic
# `claim.reconciled{detector: judged}` at completion, `RecordedJudge`
# replay byte-identical. `JUDGE_USAGE` rows carry every call's
# normalized usage; `LIVE_JUDGE_USAGE` prices the hosted aggregate
# against the pinned row. The calibrate leg also writes the committed
# `live_calibrations.json` evidence (`HH_JUDGE_CAL_OUT`).
run_leg_judge() { # $1 leg — the shared judge-leg runner
  require cargo; require python3
  local leg="$1"
  local spec crate tbin filt
  spec=$(leg_field "$leg" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  local n; for n in $(leg_field "$leg" names); do export "$n"; done
  ENDPOINT_HOST="api.openai.com:443"; MODEL_REPORTED=""
  ASSERTIONS="the different-family judge pair over real providers: local openai-compatible@1 + hosted openai@1 calibrated on the committed stage3_v1-derived labelled set (measured agreement + self-consistency → CalibrationRecord per judge — HH_JUDGE_CAL_OUT evidence on the calibrate leg); link consumes calibration + independence + RedundantJudge on the real bindings; the real Driver run lands judged verification.* rows + idp: model_io through the fenced drain + per-critic claim.reconciled{detector:judged}; RecordedJudge replay answers the recorded verdict byte-identically; usage rides JUDGE_USAGE / LIVE_JUDGE_USAGE; provided names only"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  local calout="$REPO/crates/hh-embed/tests/fixtures/r4_7e/live_calibrations.json"
  local rc=0
  if {
    printf '$ HH_JUDGE_CAL_OUT=<fixture> cargo test -p %s --test %s %s-- --ignored --nocapture --test-threads=1

'       "$crate" "$tbin" "$filt"
    (cd "$REPO" && HH_JUDGE_CAL_OUT="$calout"       cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture --test-threads=1) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local usage model
  model=$(grep -o 'LIVE_MODEL: .*' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.*LIVE_MODEL: //' || true)
  if grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    # The leg's own honest skip marker — checked before rc: a test that
    # early-returns exits 0, so the marker (not the code) carries the
    # verdict (provider-degraded is `skipped`, never a pass).
    RESULT="skipped(credential-unprovided)"
  elif [ "$rc" -eq 0 ]; then
    RESULT="pass"; MODEL_REPORTED="$model"
    usage=$(grep -o 'LIVE_JUDGE_USAGE: .*' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.*LIVE_JUDGE_USAGE: //' || true)
    local pricekey
    pricekey=$(leg_field "$leg" pricekey)
    LEG_COST_MEASURED=$(python3 - "$PRICING" "$pricekey" "$usage" <<'PRICE'
import json, sys
pricing = json.load(open(sys.argv[1]))
if not sys.argv[3]:
    print("0.000000"); sys.exit(0)
row = pricing["models"].get(sys.argv[2])
if row is None:
    sys.exit("live-verify: NoPrice — no pinned row for " + sys.argv[2])
u = json.loads(sys.argv[3])
cost = (u.get("input_total", 0) * row["prompt_per_1k_usd"]
        + u.get("output_total", 0) * row["completion_per_1k_usd"]) / 1000.0
print("%.6f" % cost)
PRICE
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    [ -n "$usage" ] && LEG_TOKEN_USAGE="$usage"
    local marks
    marks=$(grep -oE '(JUDGE_(CALIBRATE|LINK|RUN)|LIVE_MODEL)[: ].*' "$TRANSCRIPT_RAW" | sed 's/^test [^ ]* \.\.\. //' | tr '
' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
    if [ "$leg" = "judge-calibrate" ]; then
      grep -q 'evidence-recorded' "$TRANSCRIPT_RAW"         || ASSERTIONS="$ASSERTIONS; WARN no evidence-recorded marker — the live_calibrations.json fixture was not written this leg"
    fi
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}
run_leg_judge_calibrate() { run_leg_judge "judge-calibrate"; }
run_leg_judge_run() { run_leg_judge "judge-run"; }

# R4.7c — the OTLP live-sink leg (ADR-0346 revisit trigger (a); BL-31;
# R-2.9.1; USD 0 loopback): the operator-provisioned collector (HUMAN-H6:
# `jaegertracing/jaeger:2.22.0`, OTLP/HTTP :4318, query API :16686 —
# container stopped; the leg starts it) runs on loopback; the gated test
# exports a short local run's convention spans through the real
# `EgressMediator`; the leg then queries the collector's trace API for
# the run's `LIVE_TRACE_ID:` marker — read-back is the delivery proof.
# Absent docker/image/collector readiness records `skipped(<reason>)`,
# never a pass.
run_leg_otel_live() {
  require curl; require cargo
  local image="jaegertracing/jaeger:2.22.0" cname="hh-r47c-otel"
  ENDPOINT_HOST="127.0.0.1:4318"; MODEL_REPORTED=""
  ASSERTIONS="the operator-provisioned collector starts on loopback; the run's OTLP/HTTP ExportTraceServiceRequest rides the EgressMediator to :4318/v1/traces (measurement.export.delivered lands on the 2xx ack); GET :16686/api/v3/traces/{traceId} answers the exported trace — collector read-back is the delivery proof"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  {
    printf '$ docker image inspect %s\n' "$image"
    if command -v docker >/dev/null 2>&1 && docker image inspect "$image" >/dev/null 2>&1; then
      printf 'image: present\n'
    else
      printf 'image: ABSENT (or docker absent)\n'
    fi
  } > "$TRANSCRIPT_RAW"
  if ! command -v docker >/dev/null 2>&1; then
    RESULT="skipped(docker absent — the HUMAN-H6 collector runtime)"
    return
  fi
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    RESULT="skipped(collector image absent: $image — provision via HUMAN-H6)"
    return
  fi
  docker rm -f "$cname" >/dev/null 2>&1 || true
  if ! docker run -d --name "$cname" \
      -p 127.0.0.1:4318:4318 -p 127.0.0.1:16686:16686 \
      "$image" >"$TMPD_L/cid" 2>"$TMPD_L/run.err"; then
    RESULT="skipped(collector start failed)"
    cat "$TMPD_L/run.err" >> "$TRANSCRIPT_RAW"
    return
  fi
  printf '$ docker run -d --name %s -p 127.0.0.1:4318:4318 -p 127.0.0.1:16686:16686 %s\ncontainer: %s\n' \
    "$cname" "$image" "$(cat "$TMPD_L/cid")" >> "$TRANSCRIPT_RAW"
  local ready="" i
  for i in $(seq 1 45); do
    curl -sf -o /dev/null "http://127.0.0.1:16686/api/v3/services" && { ready=1; break; }
    sleep 1
  done
  printf 'readiness: GET :16686/api/v3/services → %s\n\n' \
    "${ready:+200}" >> "$TRANSCRIPT_RAW"
  if [ -z "$ready" ]; then
    RESULT="skipped(collector never answered :16686/api/v3/services)"
    docker rm -f "$cname" >>"$TRANSCRIPT_RAW" 2>&1 || true
    return
  fi
  local rc=0
  {
    printf '$ cargo test -p hharness-embed --test r4_7c_live -- --ignored --nocapture\n\n'
    (cd "$REPO" && cargo test -p hharness-embed --test r4_7c_live -- --ignored --nocapture) 2>&1
  } >> "$TRANSCRIPT_RAW" || rc=$?
  local tid
  tid=$(grep -o 'LIVE_TRACE_ID: [0-9a-f]*' "$TRANSCRIPT_RAW" | head -1 | sed 's/.*LIVE_TRACE_ID: //')
  if [ $rc -ne 0 ]; then
    RESULT="fail:live-export rc=$rc"
    docker rm -f "$cname" >>"$TRANSCRIPT_RAW" 2>&1 || true
    return
  fi
  if [ -z "$tid" ]; then
    RESULT="skipped(no LIVE_TRACE_ID marker — the export leg produced no trace)"
    docker rm -f "$cname" >>"$TRANSCRIPT_RAW" 2>&1 || true
    return
  fi
  # Collector read-back — the delivery proof. The collector indexes
  # asynchronously; bound the wait, then require the same trace ID.
  local tries=0 body=""
  while [ $tries -lt 30 ]; do
    body=$(curl -sf "http://127.0.0.1:16686/api/v3/traces/$tid" 2>/dev/null) && break
    sleep 1; tries=$((tries+1))
  done
  {
    printf '\n$ curl -sf http://127.0.0.1:16686/api/v3/traces/%s  (after %ss)\n' "$tid" "$tries"
    printf '%s\n' "$body" | head -c 8192; printf '\n'
    printf '\n$ docker rm -f %s\n' "$cname"
    docker rm -f "$cname" 2>&1 || true
  } >> "$TRANSCRIPT_RAW"
  if printf '%s' "$body" | grep -q "\"traceId\":\"$tid\""; then
    RESULT="pass"
  else
    RESULT="fail:assertion — trace $tid absent from the collector's trace API (read-back failed)"
  fi
}

# ── L4.6 — the live E2E capstone legs ───────────────────────────────────
# The reference task over all three bindings per provider, the sandboxed
# helper exec, the calibrated judge pair, `hh` verify+replay, `hh-web`,
# the MCP surface via a real client, the fleet webhook → reconciler on
# the sandbox via tunnel, and bundle export + reproduce on a clean store.
# `scripts/live-capstone.sh` orchestrates these; each is independently
# re-runnable.

# The binaries the capstone cells spawn (`bin()` asserts them).
capstone_build() {
  (cd "$REPO" && cargo build -q -p hharness-cli -p hharness-kernel \
    -p hharness-web -p hharness-mcp) \
    || { fail "capstone binary build failed"; return 1; }
}

# `capstone-run-{local,anthropic,openai}` — the reference task over
# binding (a) in-process, binding (b) the real `hh` CLI, and binding (c)
# `serve --http` + NetClient (plus a cross-binding attach-verify of the
# CLI run). Markers: CAPSTONE_RUN lines (assertion view), LIVE_USAGE /
# LIVE_MODEL for the hosted lanes.
run_leg_capstone_run() { # $1 leg — provider from the name tail
  require cargo; require python3
  local leg="$1" provider="${1#capstone-run-}"
  local spec crate tbin filt
  spec=$(leg_field "$leg" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  local n; for n in $(leg_field "$leg" names); do export "$n"; done
  export HH_CAPSTONE_PROVIDER="$provider"
  ENDPOINT_HOST=$(leg_field "$leg" host); MODEL_REPORTED=""
  ASSERTIONS="the reference task live over all three bindings — (a) in-process EmbedService, (b) the real hh CLI (run start/status/events over spawned hh-kernel serve stdio), (c) hh-kernel serve --http + the generated NetClient, plus a (c) read-only attach onto the hh run's store — per arm model.call.requested/completed{usage:available} → verify ok → replay{deterministic} no divergence and no provider re-call (model.* rows unchanged); hosted arms carry launch_env:* coordinates and pass the payload/blob leak sweep"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  capstone_build || { RESULT="fail:binary-build"; return; }
  local rc=0
  if {
    printf '$ HH_CAPSTONE_PROVIDER=%s cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' \
      "$provider" "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture --test-threads=1) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  local usage model
  usage=$(grep -o 'LIVE_USAGE: .*' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.*LIVE_USAGE: //' || true)
  model=$(grep -o 'LIVE_MODEL: .*' "$TRANSCRIPT_RAW" | tail -1 | sed 's/.*LIVE_MODEL: //' || true)
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"; MODEL_REPORTED="$model"
    local marks
    marks=$(grep -oE 'CAPSTONE_(RUN|REPLAY)[: ].*' "$TRANSCRIPT_RAW" | tr '\n' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
    if [ -n "$usage" ]; then
      LEG_TOKEN_USAGE="$usage"
      local pricekey; pricekey=$(leg_field "$leg" pricekey)
      LEG_COST_MEASURED=$(python3 - "$PRICING" "$pricekey" "$usage" <<'PY'
import json, sys
pricing = json.load(open(sys.argv[1]))
row = pricing["models"].get(sys.argv[2])
if row is None:
    sys.exit("live-verify: NoPrice — no pinned row for " + sys.argv[2])
u = json.loads(sys.argv[3])
def f(k):
    v = u.get(k)
    return float(v) if v is not None else 0.0
i, o, cr, cw = f("input_total"), f("output_total"), f("cache_read"), f("cache_write")
uncached = max(i - cr - cw, 0.0)
c = (uncached * row["prompt_per_1k_usd"]
     + cr * row.get("cache_read_per_1k_usd", row["prompt_per_1k_usd"])
     + cw * row.get("cache_write_per_1k_usd", row["prompt_per_1k_usd"])
     + o * row["completion_per_1k_usd"]) / 1000.0
print("%.6f" % c)
PY
) || { RESULT="fail:no-pinned-price"; LEG_COST_MEASURED=""; }
    fi
  elif grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}
run_leg_capstone_run_local() { run_leg_capstone_run "capstone-run-local"; }
run_leg_capstone_run_anthropic() { run_leg_capstone_run "capstone-run-anthropic"; }
run_leg_capstone_run_openai() { run_leg_capstone_run "capstone-run-openai"; }

# `capstone-sandboxed-exec` — the helper-backend legs: real `hh-helper`
# (seatbelt) spawn, exec dispatch streams + settles observed, helper-side
# deadline. USD 0, no names.
run_leg_capstone_sandboxed_exec() {
  require cargo
  ENDPOINT_HOST="127.0.0.1"; MODEL_REPORTED=""; RESP_HDRS=""
  ASSERTIONS="sandboxed tool exec on the real hh-helper backend (seatbelt): local_sandboxed attach spawns the helper; exec dispatch streams journaled frames and settles observed; the deadline is helper-enforced — AC-R-2.2.5-3 + AC-R-2.5.5-6 live halves; USD 0"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"
  (cd "$REPO" && cargo build -q -p hharness-helper) \
    || { RESULT="fail:binary-build"; return; }
  local rc=0
  if {
    for cell in ac_s2_live_exec_streams_and_settles_observed \
                ac_s2_deadline_is_helper_enforced; do
      printf '$ cargo test -p hharness-env --test helper_live %s -- --nocapture\n\n' "$cell"
      (cd "$REPO" && cargo test -p hharness-env --test helper_live \
        "$cell" -- --nocapture) 2>&1 || exit $?
    done
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if grep -q 'SKIP: seatbelt backend unavailable' "$TRANSCRIPT_RAW"; then
    RESULT="skipped(seatbelt backend unavailable)"
  elif [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local marks
    marks=$(grep -c 'test result: ok' "$TRANSCRIPT_RAW" || true)
    ASSERTIONS="$ASSERTIONS; helper_live cells green=$marks"
  else RESULT="fail:helper-live rc=$rc"; fi
}

# `capstone-judge` — judge scoring with the calibrated R4.7e pair:
# reuses the `judge-run` machinery and gated cell verbatim (the
# different-family pair, the real Driver run, judged reconciliation,
# RecordedJudge replay byte-identical).
run_leg_capstone_judge() { run_leg_judge "capstone-judge"; }

# The local-run cargo-cell legs (web / ledger-replay / bundle-repro):
# build the binaries, run the named gated cell, fold its CAPSTONE_*
# markers into the assertions.
run_leg_capstone_cell() { # $1 leg
  require cargo
  local spec crate tbin filt
  spec=$(leg_field "$1" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  local n; for n in $(leg_field "$1" names); do export "$n"; done
  ENDPOINT_HOST=$(leg_field "$1" host); MODEL_REPORTED=""; RESP_HDRS=""
  case "$1" in
    capstone-ledger-replay)
      ASSERTIONS="hh CLI run over binding (b) then attach over binding (c): verify verdict ok + replay{deterministic} byte-identical offline — zero provider re-calls (model.* row count unchanged across the replay)";;
    capstone-web)
      ASSERTIONS="hh-web serving the live run over binding (c): GET / answers the shell 200; POST /api {op:run_index} (bearer + x-hh-script) answers 200 carrying the live run's id — the HTTP read asserted; USD 0 loopback";;
    capstone-bundle-repro)
      ASSERTIONS="bundle export + reproduce: hh bundle create --sink → validate → reproduce on a clean store at the bundle's max supported level — verdict reproduced; the bundle is the only evidence the clean store sees";;
    *) ASSERTIONS="capstone cell";;
  esac
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"
  capstone_build || { RESULT="fail:binary-build"; return; }
  local rc=0
  if {
    printf '$ cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture --test-threads=1) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if grep -q "unprovided — skipped" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(credential-unprovided)"
  elif [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local marks
    marks=$(grep -oE 'CAPSTONE_[A-Z_]*[: ].*' "$TRANSCRIPT_RAW" | tr '\n' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}
run_leg_capstone_ledger_replay() { run_leg_capstone_cell "capstone-ledger-replay"; }
run_leg_capstone_web() { run_leg_capstone_cell "capstone-web"; }
run_leg_capstone_bundle_repro() { run_leg_capstone_cell "capstone-bundle-repro"; }

# `l4-6b-replay` — L4.6b's USD-0 re-verification of the diverged
# capstone hh-cli-attach arm (ADR-0381, leg (a)): a dispatching record —
# `action.tool.proposed` + the §5a.2 write-ahead chains — produced over
# binding (c) itself (`hh-kernel serve --http` + the scripted EmbedModel
# lane; no provider dial, no credential names), then replayed
# `driver_mode:deterministic` from a fresh read-only attach session on
# the same store. The verdict must be `deterministic` with no
# `replay_diverged` reason — pre-fix this arm answered `invalid`
# (3/3 on the anthropic lane). Marker: L4_6B_REPLAY.
run_leg_l4_6b_replay() {
  require cargo
  local spec crate tbin filt
  spec=$(leg_field "l4-6b-replay" test)
  crate=${spec%%:*}; spec=${spec#*:}; tbin=${spec%%:*}; filt=${spec#*:}
  [ "$filt" = "$tbin" ] && filt=""
  ENDPOINT_HOST=$(leg_field "l4-6b-replay" host); MODEL_REPORTED=""; RESP_HDRS=""
  ASSERTIONS="the diverged capstone arm re-run at USD 0: an effect-dispatching record (hh.submit completion — action.tool.proposed + the write-ahead ceremony pair) produced over binding (c) by hh-kernel serve --http + the scripted lane, then replayed{deterministic} from a read-only attach session — verdict deterministic, no replay_diverged, zero model.*/effect rows minted by the replay"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"
  (cd "$REPO" && cargo build -q -p hharness-kernel) \
    || { RESULT="fail:binary-build"; return; }
  local rc=0
  if {
    printf '$ cargo test -p %s --test %s %s-- --ignored --nocapture\n\n' "$crate" "$tbin" "$filt"
    (cd "$REPO" && cargo test -p "$crate" --test "$tbin" $filt -- --ignored --nocapture) 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local marks
    marks=$(grep -oE 'L4_6B_REPLAY[: ].*' "$TRANSCRIPT_RAW" | tr '\n' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:live-tests rc=$rc"; fi
}

# `capstone-fleet-webhook` — R4.7's full sandbox loop (the trap-guarded
# webhook + test issue create/delete on SteveVitali/hh-live-sandbox via
# the cloudflared quick tunnel, signed ingress → reconcile → WorkItem,
# the OOP probe) re-run as the capstone's fleet leg under ticket L4.6.
run_leg_capstone_fleet_webhook() { run_leg_github_live; }

# `capstone-mcp` — the hh-mcp surface (hh-mcp-serve) driven by a real
# MCP client: the official @modelcontextprotocol/sdk client when it
# installs without new repo deps (npx, runtime-only, never committed),
# else the in-repo McpClient cell — the record names which client ran.
run_leg_capstone_mcp() {
  require cargo; require python3
  ENDPOINT_HOST="127.0.0.1"; MODEL_REPORTED=""; RESP_HDRS=""
  ASSERTIONS="hh-mcp surface (hh-mcp-serve over a fixture hh-bundle/1 target:mcp member) driven by a real MCP client — negotiate handshake + tools/list carries the served tool + tools/call answers a typed outcome; USD 0"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"
  local client_kind="in-repo"
  local rc=1
  # Official-SDK arm — installable without new repo deps? Bound the
  # install hard; any failure degrades to the stated in-repo client.
  if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
    local sdk="$TMPD_L/sdk" bundle="$TMPD_L/mcp-bundle" npmwrap=""
    command -v timeout >/dev/null 2>&1 && npmwrap="timeout 120"
    if {
      printf '$ npm install --prefix <tmp> @modelcontextprotocol/sdk  (bounded)\n'
      (cd "$TMPD_L" && $npmwrap npm install --prefix "$sdk" --no-audit --no-fund --loglevel=error "@modelcontextprotocol/sdk") 2>&1
    } >>"$TRANSCRIPT_RAW" 2>&1; then
      capstone_build >>"$TRANSCRIPT_RAW" 2>&1 || true
      if {
        printf '$ HH_MCP_BUNDLE_OUT=<tmp> cargo test -p hharness-mcp --test l4_6_live -- --ignored (materialize bundle)\n'
        (cd "$REPO" && HH_MCP_BUNDLE_OUT="$bundle" \
          cargo test -q -p hharness-mcp --test l4_6_live -- --ignored --nocapture) 2>&1
      } >>"$TRANSCRIPT_RAW" 2>&1 && [ -d "$bundle" ]; then
        local kbin="$REPO/target/debug/hh-mcp-serve"
        # The driver runs *inside* the install dir so ESM `import`
        # resolves the package (NODE_PATH does not apply to ESM).
        cp "$REPO/scripts/live-capstone-mcp.mjs" "$sdk/" 2>/dev/null
        if (cd "$sdk" && node live-capstone-mcp.mjs "$kbin" --bundle "$bundle") \
            >>"$TRANSCRIPT_RAW" 2>&1; then
          client_kind="official-sdk"; rc=0
        fi
      fi
    fi
  fi
  if [ "$rc" -ne 0 ]; then
    client_kind="in-repo"
    if {
      printf '$ cargo test -p hharness-mcp --test l4_6_live -- --ignored --nocapture\n\n'
      (cd "$REPO" && cargo test -p hharness-mcp --test l4_6_live -- --ignored --nocapture) 2>&1
    } >>"$TRANSCRIPT_RAW" 2>&1; then rc=0; else rc=$?; fi
  fi
  ASSERTIONS="$ASSERTIONS; client=$client_kind"
  if [ "$rc" -eq 0 ]; then
    RESULT="pass"
    local marks
    marks=$(grep -oE 'MCP_[A-Z]*[: ].*' "$TRANSCRIPT_RAW" | tr '\n' ';' || true)
    [ -n "$marks" ] && ASSERTIONS="$ASSERTIONS; $marks"
  elif grep -q "0 filtered out" "$TRANSCRIPT_RAW" && grep -q "0 passed" "$TRANSCRIPT_RAW"; then
    RESULT="skipped(no-ignored-tests-matched)"
  else RESULT="fail:mcp-leg rc=$rc"; fi
}

# `quickstart-live` — L4.7: the README's `<!-- quickstart-live:… -->`
# block executed verbatim (the same extraction discipline
# check-quickstart.sh applies to the hermetic block): `hh run start`
# over the committed conformance fixture + the `/assembly/slots/
# model_port` override binding `hh/gateway_http` to the provisioned
# local lane — `env:HH_LOCAL_BASE_URL`/`env:HH_LOCAL_MODEL` coordinates
# resolve inside the kernel (NAMEs only; the override arg is the only
# place they appear). Asserts `exit_class:ok` + a real
# `model.call.completed` + `lifecycle.run.finished` — the run's terminal
# `stop_reason` is recorded verbatim in the assertions (a local model's
# schema-violating answer honestly ends `format_failure`; the wire leg —
# a real provider call with usage — is what the block proves).
# USD 0 — the local lane.
run_leg_quickstart_live() {
  require cargo; require python3
  local n; for n in $(leg_field "quickstart-live" names); do export "$n"; done
  ENDPOINT_HOST=$(python3 - "$HH_LOCAL_BASE_URL" <<'PY'
import sys, urllib.parse
p = urllib.parse.urlparse(sys.argv[1])
print(p.hostname or "", end="")
if p.port:
    print(":%d" % p.port, end="")
PY
)
  ASSERTIONS="the README quickstart-live block executed verbatim: hh run start on clients/ts/fixture-definition.json + the model_port override binds gateway_http openai-compatible@1 to env:HH_LOCAL_BASE_URL/env:HH_LOCAL_MODEL (NAMEs only); a real model.call.requested/completed lands (usage recorded) and the run reaches lifecycle.run.finished with exit_class:ok — the terminal outcome is the model's honest tool-call fidelity; USD 0 local lane"
  TRANSCRIPT_RAW="$TMPD_L/transcript.raw"; RESP_HDRS=""
  awk '
    /<!-- quickstart-live:begin -->/ { inmark = 1 }
    /<!-- quickstart-live:end -->/   { inmark = 0 }
    inmark && /^```sh[[:space:]]*$/ { insh = 1; next }
    insh   && /^```[[:space:]]*$/   { insh = 0 }
    insh                          { print }
  ' "$REPO/README.md" > "$TMPD_L/block.sh"
  if [ ! -s "$TMPD_L/block.sh" ]; then
    RESULT="skipped(no-quickstart-live-block)"
    return
  fi
  (cd "$REPO" && cargo build -q -p hharness-cli -p hharness-kernel) \
    || { RESULT="fail:binary-build"; return; }
  local rc=0
  if {
    printf '$ <verbatim <!-- quickstart-live --> block> (HH_STORE_ROOT/HH_WORKSPACE_ROOT → scratch)\n\n'
    (cd "$REPO" && HH_STORE_ROOT="$TMPD_L/store" HH_WORKSPACE_ROOT="$TMPD_L/ws" \
      bash -eu "$TMPD_L/block.sh") 2>&1
  } > "$TRANSCRIPT_RAW"; then rc=0; else rc=$?; fi
  if [ "$rc" -eq 0 ] \
     && grep -qF '"exit_class":"ok"' "$TRANSCRIPT_RAW" \
     && grep -qF '"class":"model.call.completed"' "$TRANSCRIPT_RAW" \
     && grep -qF '"class":"lifecycle.run.finished"' "$TRANSCRIPT_RAW"; then
    RESULT="pass"
    MODEL_REPORTED="$HH_LOCAL_MODEL"
    local run stop
    run=$(grep -o '"run_id":"[^"]*"' "$TRANSCRIPT_RAW" | head -1 | cut -d'"' -f4)
    stop=$(grep -o '"stop_reason":{[^}]*}' "$TRANSCRIPT_RAW" | tail -1)
    [ -n "$run" ] && ASSERTIONS="$ASSERTIONS; run=$run"
    [ -n "$stop" ] && ASSERTIONS="$ASSERTIONS; terminal:$stop"
  elif [ "$rc" -eq 0 ]; then
    RESULT="fail:assertion — block exited 0 but the documented outputs were absent (no real model call / no run-finished event)"
  else
    RESULT="fail:quickstart-live rc=$rc"
  fi
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
    "token_usage": (json.loads(env["LV_TOKEN_USAGE"])
                  if env.get("LV_TOKEN_USAGE") else None),
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
  LEG_COST_MEASURED=""; LEG_TOKEN_USAGE=""
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

  # A leg may report the *measured* cost (usage × the pinned pricing
  # row); absent that the declared --budget stays the recorded cost —
  # the budget_gate has already bounded it against the allocation.
  local cum_after cost="${LEG_COST_MEASURED:-$budget}"
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
    LV_TOKEN_USAGE="$LEG_TOKEN_USAGE" \
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
