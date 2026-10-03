# CAP.2 run — capstone composed verification (whole-build, one unit)

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only — offline/hermetic; the ticket's `operator-gated: <budget>`
  live stage is set to `none` on this run: no leg needs a runtime
  surface, credentials, or external network)
- **Ticket:** `docs/tickets/099_CAP.2__capstone-composed-verification.md`
  (manifest row 99 — capstone 2 of 3)
- **Branch:** `svitali/harnessharness-cap.2` (forked from
  `svitali/harnessharness-cap.1` @ `e3ab088`)
- **PR:** https://github.com/SteveVitali/HarnessHarness/pull/98 (stacked
  on `svitali/harnessharness-cap.1`)
- **Requirement ids:** the composed-verification / integration ids of
  `spec/CANONICAL_SPEC.md` (`R-2\.[0-9]+\.[0-9]+[a-z]?`) — exercised
  through the composed path, not per-ticket slices
- **Mode:** offline/hermetic — real `EmbedService`/`Dispatcher`/
  `LabServer`/`HostingService` boundaries, fixture + scripted executors
  only, never a fabricated green

## What landed

- `crates/hh-embed/tests/cap_2_composed.rs` — the composed battery
  (8 green / 4 xfail): turn-loop assembler pin + DF-S2.8-1 xfail;
  explicit `env.snapshot → fork` child-run composition + missing-
  snapshot typed refusal + suspend honest refusal + DF-S2.9-3 cadence
  xfail; the full `lab.experiment.*` lifecycle (register → expand →
  open → next → claim → launch → surface_append → settle → close) green
  at small scale + the exemplar-scale `Refused{LedgerError}` audit-cap
  pin + DF-S3.12b-1 xfail; the hosted spine — real `HostingService`
  (AdapterA + `FixtureParticipant`) beside `EmbedService`, `lift →
  lab.registry.register → lab.experiment.* → lab.hosting.attach →
  ResultsStore::project_and_record → lab.analysis.analyze` — with the
  convergent-attach pin, the green `summarize`, the honest
  `IncommensurableMatch{Unenforceable, ModelCalls}` matched-compare
  pin, the DF-CAP.2-1 refusal pin + xfail.
- `crates/hh-embed/Cargo.toml` — `hh-hosting` as a `[dev-dependencies]`
  edge only (the production boundary stays records-in/records-out;
  `hosting_edges = []` is unchanged).
- `crates/hh-env/tests/acceptance.rs` — the composed egress seam
  (3 green / 1 xfail): `net.mode = none` containment-floor deny;
  `mediated` + tainted proposal → `dom_net_egress` ask → durable
  `security.permission.pending` → suspended; the ADR-0031
  untainted-floor allow pin (an allowed egress mints zero
  `security.egress.*` rows); DF-S2.4-1 xfail.
- `crates/hh-mcp-lab/tests/cap_2.rs` — the surface-approval seam
  (1 green / 1 xfail): the supply-surface Π `ask` lands a durable
  `security.permission.pending{permission_id}` that is *unanswerable*
  at both seams (hosted caller: `unknown_tool`; Lab-catalogue human:
  `SchemaViolation{respond_permission/session_id}`); DF-S4.11-3 xfail
  for the run-less round trip.
- `docs/build/COMPOSED_E2E_REPORT.md` — commands + observed signals per
  scenario, the xfail inventory, the environment-blocks record (none),
  the verdict.
- `docs/tickets/099_CAP.2…` — `Live stage: none — offline-only`.
- `docs/tickets/DEFERRALS.md` — **DF-CAP.2-1** and **DF-CAP.2-2**
  appended (the two defects the composition surfaced); DF-S4.11-3
  progress note.

## Defects the composed path found (routed to CAP.3)

1. **DF-CAP.2-1** — `proj::lift` passes hosted `security.permission.*`
   payloads verbatim (`params`, `approval_wait_ms`); the audit-grade
   class partition refuses them, so `lab.hosting.attach` fails
   `SchemaViolation` when a participant's session contains an ask.
   Lift-table or class-declaration ruling owed.
2. **DF-CAP.2-2** — `analysis_ops::resolve_arms` maps hosted
   `limits_enforced != full` to `BudgetEnforcement::hosted(&[])` and
   never consults the adapter-stamped `budget_enforcement` map —
   `model_calls` reads `Unenforceable` on a hosted arm even when the
   adapter declared enforcement, so matched comparisons refuse
   `IncommensurableMatch` where the adapter's evidence could support a
   match.

Neither closes the composed battery: both pin as honest refusals
today, their residuals are `#[ignore]`d, and both are routed to CAP.3
with verification paths.

## Deferrals

- **Opened:** DF-CAP.2-1, DF-CAP.2-2.
- **Closed:** none — CAP.2 verifies; CAP.3 owns closure.
- **Progress notes:** DF-S4.11-3 (the composed pin records both refusal
  seams verbatim).

## Verify

- `cargo test -p hh-embed --test cap_2_composed` — 8 pass / 4 ignored.
- `cargo test -p hh-env --test acceptance` — 28 pass / 1 ignored.
- `cargo test -p hh-mcp-lab --test cap_2` — 1 pass / 1 ignored.
- `cargo test -p hh-embed` / `-p hh-env` / `-p hh-mcp-lab` — all suites
  green (the touched-package serial run).
- `cargo fmt --all` clean; `cargo clippy -p hh-env -p hh-mcp-lab
  -p hh-embed --tests` 0 warnings on touched code.
- `check-build-memory.sh .` — 0 violations (7 pre-existing warnings).

## chainTip → svitali/harnessharness-cap.2 · next → CAP.3
