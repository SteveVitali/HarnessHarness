<!-- Round-4 launch re-charter insert (decompose-spec mode=extend amendment, 2026-10-20). A HUMAN marker
     (BM-TICKET-05): operator work, NOT an implement-spec input; no run line. -->
# HUMAN-H6 — Live-leg provisioning (credentials by name, sandbox, tunnel, collector)

- **Sequence:** 149b (round-4 row 2 of 50) · **Kind:** human · **Phase:** Round 4
- **Blocks:** every live leg — R4.5a (local), R4.5b (local), R4.5d, R4.5e, R4.6, R4.7, R4.7a, R4.7b, R4.7c,
  R4.7e, L4.3 (protected `live-verify` workflow), L4.6 + L4.6a (capstone), L4.7 (quickstart-live)
- **Owner:** the operator   **Scheduled:** before the drive dispatches R4.5a (ruling 2026-10-20: provisioning
  "Now, interactively" — walked through before the drive resumes)
- **Withheld until done:** any `live-executed` claim for a leg whose required names are `provided: no`
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H6.md` (from `templates/READOUT.md`, written when the checklist is ticked)

> **Operator work — NOT an `implement-spec` input.** Values live only in
> `~/.config/harnessharness/live.env` — a symlink to a gitignored `.env` in the operator's main checkout (chmod 600 on
> the target; never committed, never copied into this worktree). No value ever enters a ledger, a contract, a log or chat;
> the record is `provided: yes/no` per NAME. An agent must not sign or assume silence is approval.

## Checklist
- [ ] `HH_LOCAL_BASE_URL` + `HH_LOCAL_MODEL` — local OpenAI-compatible server (Ollama) serving `qwen2.5:7b-instruct`
- [ ] `ANTHROPIC_API_KEY` — Anthropic Messages API key (spend counts against the $100 cap)
- [ ] `OPENAI_API_KEY` — OpenAI API key (spend counts against the $100 cap)
- [ ] `HH_GITHUB_SANDBOX_REPO` — the private sandbox repo (`SteveVitali/hh-live-sandbox`, exists)
- [ ] `HH_GITHUB_SANDBOX_TOKEN` — fine-grained token scoped to the sandbox repo alone (Issues RW, Webhooks RW, Metadata R)
- [ ] `HH_GITHUB_WEBHOOK_SECRET` — the webhook signing secret (registered by R4.7 at run time via the API)
- [ ] `HH_GITHUB_OAUTH_CLIENT_ID` — a GitHub OAuth App with **device flow enabled** (R4.7a; the operator is present to enter the device code)
- [ ] `cloudflared` on PATH (R4.7 quick-tunnel at run time); `node` ≥ 22 and `python3` on PATH (R4.2/R4.4/R4.5,
      L4.6a's temp-dir MCP/ACP SDK clients); `gitleaks` on PATH (R4.5a's secret-scan gate)
- [ ] A local OpenTelemetry-compatible collector the operator can start on loopback (e.g. `otelcol-contrib` or Jaeger all-in-one) — R4.7c's live sink
- [ ] GitHub Environment `live-verify` (secrets + required reviewer) — needed by L4.3's workflow_dispatch job; may be done at HUMAN-H7 if L4.3 lands the job dormant

## Planning-time observation (names only — not a tick)
2026-10-20T15:55Z, `sed 's/=.*//'` over `live.env`: present — `HH_LOCAL_BASE_URL`, `HH_LOCAL_MODEL`,
`HH_GITHUB_WEBHOOK_SECRET`, `HH_GITHUB_SANDBOX_REPO`; absent — `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
`HH_GITHUB_SANDBOX_TOKEN`, `HH_GITHUB_OAUTH_CLIENT_ID`. `ollama list` shows `qwen2.5:7b-instruct`; `cloudflared`,
`gitleaks`, `node` v25, `python3` present; collector not checked.

## Pre-flight before R4.5a (orchestrator — amend (b), 2026-10-20)
Once this checklist is ticked, the orchestrator runs ONE names-only pre-flight covering every NAME the round needs
before dispatching R4.5a, and records only the yes/no lines (never a value) in the readout. If
`scripts/live-verify.sh --check-names --all` exists (it lands in R4.5a, so normally it does not yet), run that; otherwise
run this literal shell check (prints names + yes/no only; `grep -q` never echoes a value):

```sh
f="$HOME/.config/harnessharness/live.env"
if test -r "$f"; then echo "live.env: readable"; else echo "live.env: MISSING (provision via HUMAN-H6)"; fi
for n in HH_LOCAL_BASE_URL HH_LOCAL_MODEL ANTHROPIC_API_KEY OPENAI_API_KEY \
         HH_GITHUB_SANDBOX_REPO HH_GITHUB_SANDBOX_TOKEN HH_GITHUB_WEBHOOK_SECRET HH_GITHUB_OAUTH_CLIENT_ID; do
  if grep -Eq "^[[:space:]]*(export[[:space:]]+)?${n}=(\"[^\"]+\"|'[^']+'|[^\"'[:space:]#])" "$f" 2>/dev/null
  then echo "$n: provided: yes"; else echo "$n: provided: no"; fi
done
for t in cloudflared node python3 gitleaks; do
  if command -v "$t" >/dev/null 2>&1; then echo "$t: present"; else echo "$t: absent"; fi
done
```

Any `provided: no` / `absent` / `MISSING` line STOPS the drive and asks the operator (the collector and the
`live-verify` Environment are confirmed by the operator's words, not by this check).

## Exit criterion
Every NAME above is `provided: yes` (or the operator's recorded words waive a leg — which then needs its own
ADR/ruling, since the round's charter forbids carried deferrals), recorded in the readout and the LEDGER.

## DEFERRALS rule for tickets that run before this completes
Round-4 override of the template rule (operator charter: no remaining deferred work): a live ticket that finds a
required NAME `provided: no` at dispatch does **not** open a `D-` row — it STOPS and the orchestrator pauses and asks
the operator to provision (or to rule). Never a fabricated pass; never a silent skip of a live leg.
