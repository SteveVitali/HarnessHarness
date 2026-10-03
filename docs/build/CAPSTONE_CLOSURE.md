# CAP.3 — capstone closure

> The capstone-closure record: every gap CAP.1's analysis routed here
> and CAP.2's composed spine found is either **closed with a green
> test**, **accepted as a deviation** (signed at GATE-ACCEPT), or
> **re-anchored to a named `DEFERRALS.md` row**. Nothing is silently
> dropped; every claimed green names its test.

- **Ticket:** `docs/tickets/100_CAP.3__capstone-closure.md`
- **Branch:** `svitali/harnessharness-capstone` (stacked on
  `svitali/harnessharness-cap.2` @ `d8c8011`)
- **Mode:** offline/hermetic, CPU-only, deterministic — no live stage.
- **Date:** 2026-10-02
- **ADRs:** ADR-0327, ADR-0328, ADR-0329, ADR-0330 (all `Accepted`).

## What closed — the six routed gaps + the rung sweep

| Row | Disposition | Evidence |
|---|---|---|
| DF-S2.4-1 (member a) | **CLOSED (member a); (b)/(c) stay OPEN on the row** | `Dispatcher` routes `net_egress` effects on `mediated` environments through `EgressMediator::gate` (durable `security.egress.requested`, token attribution, pure `decide_egress`, terminal deny/ask) and `::forward` (recheck → sentinels → durable `decided` → wire → charge) at the wire point post-`committed`. Default broker is `DenyAllResolver` — fail-closed until a host installs a live broker. Test: `hh-env/tests/acceptance.rs::cap2_dispatch_net_egress_mediated` (green). Ruling: ADR-0330 D1. |
| DF-S2.8-1 (member a builder leg) | **CLOSED (builder leg); residual set stays OPEN** | `AssemblerPort::assemble(AssembleInputs{model_call_id, prefix, window_cap_tokens})`; `KernelAssembler` runs the real `hh_context::assemble` over the run's durable prefix — `context.assembled` is the builder's canonical plan record, side-band emissions land durable. Test: `hh-embed/tests/cap_2_composed.rs::cap2_turn_loop_runs_hh_context_assembler` (green). Ruling: ADR-0330 D2. |
| DF-S3.12b-1 | **DONE** | `measurement.experiment.closed` moved from `OPEN_AUDIT` to the enumerated `EXPERIMENT_CLOSED_FIELDS` partition — record/list-shaped members at `AUDIT_FIELD_LIST_BYTES`. An honestly-measured exemplar closes `status: completed`. Test: `cap2_experiment_exemplar_close_completes` (green). Ruling: ADR-0327. |
| DF-S3.12b-2 | **DONE** | All 110 `docs/adr/ADR-*.md` records carry the `**Rung (ADR-0024 obligation):**` line (30 already-compliant + 80 swept); `_TEMPLATE.md` now requires the line; index regenerated via `adr-index.sh`. Ruling: ADR-0330 D4. |
| DF-CAP.2-1 | **DONE** | The lift *shapes* hosted `security.permission.*` members onto the native partitions — `params`→`request`, `approval_wait_ms`→`wait_ms`, `provenance` declared + mint-stamped `participant_reported`, `permission_id` joins pending↔decided. Tests: `cap2_hosted_attach_permission_rows_land` (green), `cap2_hosted_attach_permission_rows_refuse` (still refuses `SchemaViolation`). Ruling: ADR-0328. |
| DF-CAP.2-2 | **DONE** | `resolve_arms` admits the adapter-stamped `budget_enforcement` map (`extra["budget_enforcement"]`); `limits_enforced` is never read as evidence. A hosted matched compare lands a real `ComparisonReport`. Test: `cap2_hosted_arm_stamped_enforcement_compares` (green). Ruling: ADR-0330 D3. |
| DF-S4.11-3 | **DONE** | `respond_approval` is a protocol builtin — run-less, surface-run-scoped, `human_principal`-gated; mints `security.permission.decided` (+`lease.granted` atomic, `request_id`-idempotent, `responder_provenance`); retried `tools/call` folds durable decided rows (allow applies, deny refuses `DeniedByPolicy`). Test: `cap2_supply_ask_respond_approval_round_trip` (green). Ruling: ADR-0329. |

The `#[ignore]`d composed xfails dropped from 6 to 1 — only
`cap2_fork_at_turn_boundary_without_explicit_snapshot` (DF-S2.9-3)
remains, a staged env-driver row that was never CAP.3's.

## Matrix movement

- `R-2.4.1`: AT-RISK-INTEGRATION → **PARTIAL** (the live composition is
  wired and driven green; the DF-S2.8-1 residual set stays open).
- `R-2.8.4`: AT-RISK-INTEGRATION → **PARTIAL** (the dispatch path is
  wired and driven green; DF-S2.4-1 (b)/(c), DF-S2.4-2, DF-S1.12-1/-2
  stay open).
- `R-2.10.3`: PARTIAL (unchanged verdict — DF-S3.12b-1 closed;
  DF-S4.2-1/-2 open).
- `R-2.10.4`: MET — DF-CAP.2-2 closed (ADR-0330 D3).
- `R-2.10.6`: MET — DF-CAP.2-1 closed (ADR-0328).
- `R-2.11.3`: PARTIAL — DF-S4.11-3 closed; DF-S4.11-1/-2 open.

## ACCEPTED-deviations — proposed for operator signature at GATE-ACCEPT

Each row: **id · what deviates · why sound · compensating control.**
The set is the six MET-DIFFERENTLY verdicts (CAP.1 D3) plus the
gate-accepted deviations; nothing new was accepted at CAP.3 — its own
residuals are carried as OPEN deferral rows, not acceptances. **Set
size: 10.**

1. **R-2.7.2 split (ADR-0146)** — the parent requirement is discharged
   through children R-2.7.2a/b rather than as one row. *Sound:* the
   split is a spec-sanctioned decomposition; both children carry their
   own verdicts and tests. *Control:* children verdicts recorded and
   driven; the parent row references the split explicitly.
2. **R-2.12.3 ecosystem decision (ADR-0050; ADR-0009; ADR-0226)** —
   the contract is discharged by the recorded decision + the E1
   implementation; the E2/E3 foreign legs are deferred, not executed.
   *Sound:* S0.3b proved E1=E2=E3 byte-identity on the golden corpus —
   the foreign legs add no evidence the identity proof doesn't already
   give. *Control:* DF-S0.3-2 carried OPEN to GATE-ACCEPT; the E2
   trigger-5 disposition is recorded (item 8).
3. **R-2.12.4 spec-level deferral (ADR-0210)** — the item is deferred
   by the spec itself. *Sound:* the build honours a spec-anchored
   deferral; no code was ever expected. *Control:* the deferral is
   named in the spec; nothing claims the row.
4. **R-2.12.5 program-level naming thesis (ADR-0003/0006)** — the
   discharge is a program artifact (uniform naming), not a code
   deliverable. *Sound:* the requirement names a program-level
   property. *Control:* naming is applied uniformly and is
   grep-verifiable across the build.
5. **R-2.11.2 E3-ecosystem binding (HUMAN-H1; DF-S4.10-1)** — the
   generated client runs in-ecosystem as the declared honest proxy;
   the real E3 surface-ecosystem binding is unprovisioned. *Sound:* a
   human prerequisite cannot be minted in-build; the proxy exercises
   the same wire surface. *Control:* `provided: no` is recorded;
   discharge at operator provisioning (DF-S4.10-1 stays OPEN).
6. **R-2.12.6 real issue tracker (HUMAN-H2; DF-S5.6-1)** — the fixture
   adapter is the declared honest proxy; the real tracker is
   unprovisioned. *Sound:* as item 5. *Control:* the fixture exercises
   the real contract surface; DF-S5.6-1 stays OPEN.
7. **R2 cross-camp human signature (DF-S0.3-3 residual; accepted at
   GATE-G1)** — the human-signature cell cannot be produced by the
   build. *Sound:* it is a human deliverable by definition. *Control:*
   the machine cells are measured and the byte-identity gate is proven;
   the residual rides to GATE-ACCEPT.
8. **Armed revalidation trigger 5 satisfied-by-analysis (ADR-0226)** —
   E2 steps 5–7 (ADR-0009) were dispositioned by analysis rather than
   re-run, E2's C5/C7 cells falling outside the ±1 band on a
   non-kernel candidate. *Sound:* the analysis is documented and the
   winner (E5a) is unmoved by it. *Control:* the disposition is an ADR;
   the band analysis is reproducible from the recorded data.
9. **Foreign-toolchain verification cells (DF-S0.3-2, DF-S1.2-2,
   DF-S1.5-3, DF-S1.8-1, DF-S1.27-1)** — cross-implementation
   conformance cells sit outside the hermetic scope. *Sound:* the
   environment-bound cells cannot be minted offline; within-E1
   byte-equality against the pinned golden corpus plus S0.3b's proven
   E1=E2=E3 identity bound the same property. *Control:* all five rows
   stay OPEN to GATE-ACCEPT with named verification paths.
10. **GATE-G2 recorded set** — LT-03's live corpus arm, DF-S1.17-1's
    exposure form arm, DF-S1.21-2's OOP conformance battery, DF-S1.15-1,
    DF-S1.24-1 residual, DF-S1.14-4, DF-S1.22-1 (rule-4 reading),
    DF-S5.4-1 — accepted at G2/G3 with recorded reasons. *Sound:* each
    was accepted at a gate with its reason on the record. *Control:*
    the gate readouts name each row; nothing is silently dropped.

## What remains deferred (not accepted)

Every other residual stays an **OPEN** `DEFERRALS.md` row — deferred,
not accepted — each with a named owner and a verification path,
including: DF-S2.4-1 members (b)/(c); DF-S2.8-1's residual set;
DF-S2.9-3 (snapshot cadence — the one remaining composed xfail);
DF-S2.4-2 (TLS transport); DF-S1.12-1/-2; DF-S4.11-1/-2 (SSE half,
oauth/mtls mediator); DF-S4.2-1/-2 (remote transports, foreign
formats); DF-S1.26-2 (parked-detach member); DF-S2.10-1 (env verbs);
DF-S2.11-1 (durable steer cue); DF-S1.25-1 (DefinitionInput::Ref); and
the cross-implementation V-rows. The deferral ledger, not this
document, is their authoritative state.

## Verification

- `cargo test --workspace` → `target/cap_3_ws.log` (counts recorded in
  `docs/build/runs/CAP.3.md`).
- Touched-package `cargo clippy --all-targets --no-deps`, touched-file
  `cargo fmt --check`, `check-drift.sh`, serial `check-removability.sh`
  + `cargo build -p hh-helper`, `check-build-memory.sh` — recorded in
  the run ledger.
- Each closed pin re-runs green **un-ignored**; each companion refusal
  pin stays green (`cap2_hosted_attach_permission_rows_refuse`,
  `cap2_supply_ask_pending_unanswerable`'s refusal seam, the
  `human_principal` gate).
