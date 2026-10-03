# CAP.2 — capstone composed verification

**Stacked on:** `svitali/harnessharness-cap.1` (PR #97) · **Do not merge — operator-controlled**

## Summary

The additive tickets only ever exercised their own slices. This run
drives the **whole build as one unit** through real production
boundaries — `EmbedService`, the seven-stage `Dispatcher`,
`LabServer`/`handle_message`, and a real `HostingService`
(AdapterA + `FixtureParticipant`) — and records the composed state
honestly: **12 composed legs green, 6 xfails each carrying a
`DEFERRALS.md` id, 2 new defects surfaced by the composition itself**
(DF-CAP.2-1/2, routed to CAP.3). No fabricated greens: every unwired
seam is an `#[ignore]`d residual whose reason begins with a `DF-*` id.

## What landed

- `crates/hh-embed/tests/cap_2_composed.rs` — the composed battery
  (8 green / 4 xfail): turn-loop `KernelAssembler` pass-through pin +
  DF-S2.8-1 xfail; explicit `env.snapshot → fork` child composition +
  missing-snapshot `EnvironmentUnavailable` pin + `local_host` suspend
  honest refusal + DF-S2.9-3 cadence xfail; the full
  `lab.experiment.*` lifecycle green at small scale + the
  exemplar-scale `Refused{LedgerError}` audit-cap pin + DF-S3.12b-1
  xfail; the hosted spine (`lift → registry → experiment → attach →
  results → analyze`) with the DF-CAP.2-1/2 pins.
- `crates/hh-env/tests/acceptance.rs` — the composed egress seam
  (3 green / 1 xfail): `net.mode = none` floor deny; `mediated` +
  tainted → `dom_net_egress` ask → durable pending → suspended; the
  ADR-0031 clean-floor allow pin (allowed egress, zero
  `security.egress.*` rows); DF-S2.4-1 xfail.
- `crates/hh-mcp-lab/tests/cap_2.rs` — the surface-approval seam
  (1 green / 1 xfail): durable `security.permission.pending`
  unanswerable at both seams (hosted `unknown_tool`; run-less
  `SchemaViolation{respond_permission/session_id}`); DF-S4.11-3 xfail.
- `crates/hh-embed/Cargo.toml` — `hh-hosting` test-only dev-dep
  (production boundary unchanged: records-in/records-out).
- `docs/build/COMPOSED_E2E_REPORT.md` — the evidence record.
- `docs/tickets/099_CAP.2…` — `Live stage: none — offline-only`.
- `docs/tickets/DEFERRALS.md` — DF-CAP.2-1 + DF-CAP.2-2 appended.

## New defects the composition surfaced (→ CAP.3)

- **DF-CAP.2-1** — lifted hosted `security.permission.*` rows carry
  hosted members the audit-grade class partition refuses;
  `lab.hosting.attach` → `SchemaViolation` when a participant's session
  contains an ask.
- **DF-CAP.2-2** — `resolve_arms` ignores the adapter-stamped
  `budget_enforcement` map; hosted `limits_enforced = partial` reads
  `model_calls` `Unenforceable` → matched compare refuses
  `IncommensurableMatch` even where the adapter declared enforcement.

## Requirement ids

The composed-verification / integration ids driven here (the seams
each leg exercises):

R-2.4.1 (assembler — pass-through green, `hh_context` DF-S2.8-1) ·
R-2.8.4 (egress mediation — fail-closed green, mediator DF-S2.4-1) ·
R-2.9.3 (snapshot cadence — explicit path green, cadence DF-S2.9-3) ·
R-2.9.x suspend/fork honest refusals · R-2.10.3 (experiment close —
small-scale green, exemplar DF-S3.12b-1) · R-2.11.3 (surface Π ask —
durable green, answer path DF-S4.11-3) · R-2.10.x hosted
participant spine (attach/project/summarize green; DF-CAP.2-1/-2)

## Verify

- `cargo test -p hh-embed --test cap_2_composed` — **8 pass / 4 ignored**
- `cargo test -p hh-env --test acceptance` — **28 pass / 1 ignored**
- `cargo test -p hh-mcp-lab --test cap_2` — **1 pass / 1 ignored**
- `cargo test -p hh-embed` / `-p hh-env` / `-p hh-mcp-lab` — all suites
  green (touched-package serial run)
- `cargo fmt --all` clean; `cargo clippy --tests` on the touched crates
  — 0 warnings
- `check-build-memory.sh .` — 0 violations (7 pre-existing warnings)
