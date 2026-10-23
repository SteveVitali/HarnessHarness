#!/usr/bin/env bash
# live-github.sh — R4.7's `github-live` leg orchestrator (ADR-0363 D7;
# DF-S5.6-1; HUMAN-H2 "GitHub Issues + tunnel"; AC-R-2.12.6-9).
#
# Driven only through `scripts/live-verify.sh github-live` — the runner
# checks NAME provision, gates the USD 0 budget, redacts the transcript
# this prints, and emits the hh-live-evidence/1 record.
#
# The leg itself lives in `crates/hh-embed/tests/r4_7_live.rs` (the
# gated `#[ignore]` cells): `r47_github_live` runs the whole loop —
# broker-resolved credentials, the loopback listener behind a runtime
# `cloudflared` quick tunnel, one sandbox-scoped `POST /repos/…/hooks`
# (pre-state GET recorded), the test issue + comment, the signed
# X-Hub-Signature-256 `issues`/`issue_comment` deliveries admitted at
# `fleet.webhook_ingress` → `fleet.reconcile` → WorkItem, the dedupe +
# stale-window cells, then `DELETE /repos/…/hooks/{id}` and the issue
# close+`hh-live-test` rollback — all inside the repo
# `HH_GITHUB_SANDBOX_REPO` names. `r47_github_plugin` is the
# out-of-process leg: the packaged `hh-github-issues` variant spawns
# `subprocess_confined` and answers the capability probe + list/get over
# the live-captured `issues.json` snapshot.
#
# The rollback contract: the test writes `$HH_GH_DIR/state.json` the
# instant each mutation lands ({hook_id, issue_number}); this script's
# EXIT trap runs `r47_github_cleanup`, which re-fires the DELETE/close
# for anything still pending — the hook never outlives the leg, panic
# path included.
#
# Exit: 0 when every cell passed (GH_RESULT pass + GH_PLUGIN_RESULT
# pass); 2 when a leg skipped honestly or failed. NAMEs arrive exported
# by the runner; values never print and never touch argv.

set -euo pipefail
set +x

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GH_DIR="${HH_GH_DIR:-$(mktemp -d)}"
export HH_GH_DIR="$GH_DIR"
mkdir -p "$GH_DIR"

say() { printf 'live-github: %s\n' "$*"; }

for t in cargo python3 cloudflared; do
  command -v "$t" >/dev/null 2>&1 || { say "GH_SKIP required-tool-absent:$t"; exit 2; }
done

# NAMEs — report provision only, never values.
missing=""
for n in HH_GITHUB_SANDBOX_TOKEN HH_GITHUB_SANDBOX_REPO HH_GITHUB_WEBHOOK_SECRET; do
  if [ -n "${!n:-}" ]; then printf '%s provided: yes\n' "$n"
  else printf '%s provided: no\n' "$n"; missing="$missing $n"; fi
done
if [ -n "$missing" ]; then
  say "GH_SKIP unprovided:$missing"
  exit 2
fi

# The variant + helper binaries the out-of-process cell spawns.
(cd "$REPO_ROOT" && cargo build -q -p hharness-github-issues -p hharness-helper) \
  || { say "GH_SKIP build-failed"; exit 2; }

# The trap: re-run the rollback legs for anything state.json still marks
# pending — fires on success, failure, and SIGINT alike. Never fatal.
cleanup() {
  (cd "$REPO_ROOT" && HH_GH_DIR="$GH_DIR" \
    cargo test -q -p hharness-embed --test r4_7_live r47_github_cleanup \
      -- --ignored --nocapture) 2>&1 | sed 's/^/  [cleanup] /' || true
}
trap cleanup EXIT INT TERM

rc_total=0
for cell in r47_github_live r47_github_plugin; do
  rc=0
  (cd "$REPO_ROOT" && HH_GH_DIR="$GH_DIR" \
    cargo test -p hharness-embed --test r4_7_live "$cell" -- --ignored --nocapture) 2>&1 \
    || rc=$?
  [ "$rc" -ne 0 ] && rc_total=$rc
done

if [ "$rc_total" -eq 0 ]; then
  say "GH_RESULT all-cells-pass"
  exit 0
fi
say "GH_RESULT cells-rc=$rc_total"
exit 2
