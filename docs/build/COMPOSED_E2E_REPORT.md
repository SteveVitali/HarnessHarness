# CAP.2 — composed E2E report

> The additive tickets only ever exercised their own slices. This report
> records what the **whole build as one unit** does when the composed
> seams are driven through real production boundaries — what ran green,
> what refused honestly, and every unwired seam as an `xfail`/`skip`
> whose reason begins with a `DEFERRALS.md` id. Nothing here is a
> fabricated green.

- **Ticket:** `docs/tickets/099_CAP.2__capstone-composed-verification.md`
- **Branch:** `svitali/harnessharness-cap.2` (stacked on
  `svitali/harnessharness-cap.1` @ `e3ab088`)
- **Mode:** offline/hermetic, CPU-only, deterministic — no live stage.
- **Date:** 2026-10-02

## Command surface

```
cargo test -p hh-embed --test cap_2_composed   # 8 pass · 4 ignored (xfail)
cargo test -p hh-env   --test acceptance     # 28 pass · 1 ignored (xfail)
cargo test -p hh-mcp-lab --test cap_2        # 1 pass  · 1 ignored (xfail)
cargo test -p hh-embed                        # all suites green
cargo test -p hh-env                          # all suites green
cargo test -p hh-mcp-lab                      # all suites green
cargo fmt --all / cargo clippy -p hh-env -p hh-mcp-lab -p hh-embed --tests
bash <agent-skills>/skills/build-memory/scripts/check-build-memory.sh .
```

## Composed scenarios

### 1. Turn loop — the assembler seam (`crates/hh-embed/tests/cap_2_composed.rs`)

**Driven:** a real `EmbedService::handle` submit turn over a scripted
model — `open → submit` runs the assembled context through the live
`AssemblerPort` on every proposal round.

**Observed:** each of the turn's proposal rounds emits
`context.assembled` carrying the `KernelAssembler` pass-through stamp —
the real port is invoked on the live path, and what it emits is the
verbatim input (documented pass-through).

**Honest seam:** `cap2_turn_loop_runs_hh_context_assembler` is
`#[ignore]`d — **DF-S2.8-1**: no production call site drives
`hh_context::assemble`'s builder/retrieval/compaction composition; the
live `hh_context` path is routed to CAP.3.

### 2. Environment — snapshot/fork/suspend (`cap_2_composed.rs`)

**Driven:** `env.snapshot` (explicit) → `fork{env: "snapshot"}` before
the source run finishes → child run lineage; a `fork` with no snapshot
producer → typed refusal; `env.suspend` on `local_host`.

**Observed (green):**

- `cap2_snapshot_fork_composes_child_run` — explicit snapshot → fork
  composes: the child run derives from the snapshot, lineage recorded.
- `cap2_fork_without_snapshot_producer_refuses_typed` — a fork naming
  `env: "snapshot"` with no snapshot refuses typed
  `EnvironmentUnavailable{snapshot_unavailable}` — fail-closed, never a
  guess.
- `cap2_suspend_is_environment_bound_honest_refusal` — `env.suspend` on
  `local_host` refuses `EnvironmentUnavailable` — the environment
  declares no suspend capability; the refusal is the environment's, not
  a synthetic error.

**Honest seam:** `cap2_fork_at_turn_boundary_without_explicit_snapshot`
is `#[ignore]`d — **DF-S2.9-3**: there is no snapshot producer cadence;
`fork{env: snapshot}` only composes when an explicit `env.snapshot`
preceded it. Routed to CAP.3.

### 3. Experiment boundary lifecycle (`cap_2_composed.rs`)

**Driven:** the full `lab.experiment.*` verb chain — `register → expand →
open → next → claim → launch → surface_append → settle → close` — at
small scale, and `close` at exemplar scale.

**Observed (green):**

- `cap2_experiment_boundary_lifecycle_closes_green` — the small-scale
  experiment completes and the durable `measurement.experiment.closed`
  lands.
- `cap2_experiment_exemplar_close_refuses_audit_cap` — exemplar-scale
  close refuses at the boundary as `Refused{LedgerError}` — the
  honestly-measured utilization/budget-match payload overruns
  `AUDIT_FIELD_MAX_BYTES = 512` (the typed `AuditFieldsTooLarge` is
  pinned engine-side in `s3_12b`); no completed close row exists. The
  payload is *not* under-measured to fit the cap.

**Honest seam:** `cap2_experiment_exemplar_close_completes` is
`#[ignore]`d — **DF-S3.12b-1**: the class-declaration/row-shape ruling
for exemplar-scale close is routed to CAP.3.

### 4. Hosted participant spine (`cap_2_composed.rs`)

**Driven:** a real `hh_hosting::HostingService` (AdapterA +
`FixtureParticipant`) run beside `EmbedService` — attach/open/submit/
close/stream → `proj::lift` → `lab.registry.register` → a hosted-arm
experiment through `lab.experiment.*` → `lab.hosting.attach`
(records-in/records-out — `hh-embed` never depends on `hh-hosting` in
production; the test-only dev-dep is declared as such) →
`ResultsStore::project_and_record` → `lab.analysis.analyze`.

**Observed (green):**

- `cap2_hosted_spine_attach_project_analyze` — registry pin, attach
  (convergent: `experiment.launch` may stamp the attach pair first —
  `attached: false` with the lifted rows/metrics/native-record leaves
  landed is the correct convergence), results projection, and a green
  `summarize` analysis over the projected hosted rows.
- `cap2_hosted_attach_permission_rows_refuse` — a **defect the composed
  path found**: `proj::lift` passes hosted `security.permission.*`
  payloads verbatim (`params`, `approval_wait_ms`), which violate the
  audit-grade class partition; `lab.hosting.attach` refuses
  `SchemaViolation`. → **DF-CAP.2-1** (new DEFERRALS row, routed to
  CAP.3).
- The matched `compare` half of `analyze` refuses honestly:
  `arm:hosted`'s `limits_enforced = partial` maps to
  `BudgetEnforcement::hosted(&[])` — `resolve_arms` never consults the
  adapter-stamped `budget_enforcement` map — so `model_calls` is
  `Unenforceable` and `matched_cap` refuses
  `IncommensurableMatch{Unenforceable, ModelCalls}`: the specified
  verdict for a dimension the Lab cannot enforce on an unmediated
  participant, never a fabricated parity claim. → **DF-CAP.2-2** (new
  DEFERRALS row, routed to CAP.3).

**Honest seam:** `cap2_hosted_attach_permission_rows_land` is
`#[ignore]`d — **DF-CAP.2-1**.

### 5. Egress — the mediation seam (`crates/hh-env/tests/acceptance.rs`)

**Driven:** a `net_egress` effect through the real `Dispatcher` —
admission → containment floor → `monitor.authorize` → outcome — under
the three honest policy/authority shapes, asserting the full durable
trail each time.

**Observed (green):**

- `cap2_dispatch_net_egress_mode_none_refuses` — the production-default
  `net.mode = none` policy refuses at the containment precondition
  inside `authorize` (step 0: `deny{Containment}` — Π never consulted):
  `security.permission.decided{deny}` + `action.effect.refused` land;
  the executor is never touched; **zero** `security.egress.*` rows.
- `cap2_dispatch_net_egress_asks_no_egress_rows` — under a `mediated`
  net policy with a *tainted* (`eff ≤ external`) proposal, the Π table
  runs: `dom_net_egress` asks (`host_allowlisted = unknown`), the
  durable `security.permission.pending` lands, the effect suspends —
  **zero** `security.egress.*` rows, executor untouched. The ask
  channel is the whole egress gate for untrusted proposals today.
- `cap2_dispatch_net_egress_clean_floor_allows_no_egress_rows` — a
  composed finding worth pinning: an *untainted* `≥ principal`
  `net_egress` never reaches the Π table — `floor_verdict` (ADR-0031)
  allows it outright. The reference `Ep2Model` mediates nothing, so the
  effect executes and still mints **zero** `security.egress.*` rows —
  the durable mediated-trail records are exactly DF-S2.4-1's residual.

**Honest seam:** `cap2_dispatch_net_egress_mediated` is `#[ignore]`d —
**DF-S2.4-1**: no production dispatch call site drives
`EgressMediator`; the `security.egress.requested`/`decided` pair is
routed to CAP.3.

### 6. Surface approvals — the answerable-pending seam (`crates/hh-mcp-lab/tests/cap_2.rs`)

**Driven:** a supply-surface Π `ask` under a `human_principal` hosted
binding through the real `LabServer` + `EmbedService`, then the answer
attempts the deferral names.

**Observed (green):**

- `cap2_supply_ask_pending_unanswerable` — `spicy` →
  `PermissionAskRequired` + `pending_effects[]` + durable
  `security.permission.pending{permission_id}` on the caller's surface
  run. The answer path fails closed at two distinct seams: the hosted
  caller's own `respond_approval` is `unknown_tool` (its catalogue is
  its artifact — `callable ⇔ revealed`), and a Lab-catalogue
  `human_principal` binding's run-less `respond_approval{permission_id}`
  refuses `SchemaViolation{respond_permission/session_id}` — the
  surface run is never minted as a `hnd-run-*` handle. The pending
  stays open: no serving `security.permission.decided` names it, and a
  retried call asks again.

**Honest seam:** `cap2_supply_ask_respond_approval_round_trip` is
`#[ignore]`d — **DF-S4.11-3**: the run-less surface-run-scoped answer
(resolves the pending, mints `security.permission.decided`, retried
`tools/call` applies) is routed to CAP.3.

## New defects the composed run found (routed to CAP.3)

| id | defect | evidence |
|---|---|---|
| **DF-CAP.2-1** | `proj::lift` passes hosted `security.permission.*` members (`params`, `approval_wait_ms`) the audit-grade class partition refuses — `lab.hosting.attach` → `SchemaViolation` | `cap2_hosted_attach_permission_rows_refuse` (pin) + `cap2_hosted_attach_permission_rows_land` (xfail) |
| **DF-CAP.2-2** | `analysis_ops::resolve_arms` maps hosted `limits_enforced != full` to `BudgetEnforcement::hosted(&[])` and never consults the adapter-stamped `budget_enforcement` map — matched `model_calls` comparisons refuse `IncommensurableMatch{Unenforceable}` | the matched-compare refusal pinned inside `cap2_hosted_spine_attach_project_analyze` |

## Environment blocks

None — the ticket's `operator-gated: <budget>` live stage is corrected
to `none`/offline-only on this run: every leg runs hermetic, CPU-only,
in-process. No secrets entered any ledger (`provided: no`).

## xfail inventory (reasons begin with `DF-*` ids)

| test | deferral |
|---|---|
| `cap2_turn_loop_runs_hh_context_assembler` | DF-S2.8-1 |
| `cap2_fork_at_turn_boundary_without_explicit_snapshot` | DF-S2.9-3 |
| `cap2_experiment_exemplar_close_completes` | DF-S3.12b-1 |
| `cap2_hosted_attach_permission_rows_land` | DF-CAP.2-1 |
| `cap2_dispatch_net_egress_mediated` | DF-S2.4-1 |
| `cap2_supply_ask_respond_approval_round_trip` | DF-S4.11-3 |

## Verdict

The composed path runs green **with honest seams**: 12 composed legs
green across the three batteries, 6 xfails each carrying a `DEFERRALS.md`
id, and 2 new defects surfaced by the composition itself (DF-CAP.2-1/2)
— pinned as green honest refusals now, with their residual claims
ignored pending CAP.3 wiring. Nothing was fabricated: every non-green
expectation is an `#[ignore]`d residual, every refusal is asserted at
the real boundary it came from.
