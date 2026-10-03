# CAP.3 run — capstone closure

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only — offline/hermetic; the ticket's live stage is `offline-only`:
  no leg needs a runtime surface, credentials, or external network)
- **Ticket:** `docs/tickets/100_CAP.3__capstone-closure.md`
  (manifest row 100 — capstone 3 of 3)
- **Branch:** `svitali/harnessharness-capstone` (forked from the
  chain tip `svitali/harnessharness-cap.2` @ `d8c8011`)
- **PR:** https://github.com/SteveVitali/HarnessHarness/pull/99 (stacked
  on `svitali/harnessharness-cap.2`)
- **Requirement ids:** `R-2.8.4` · `R-2.4.1` · `R-2.10.3` · `R-2.10.4` ·
  `R-2.10.6` · `R-2.11.3`/`R-2.8.7` · `R-2.1.4` (rung hygiene) · the six
  MET-DIFFERENTLY rows (`R-2.7.2`, `R-2.11.2`, `R-2.12.3`, `R-2.12.4`,
  `R-2.12.5`, `R-2.12.6`)
- **Mode:** offline/hermetic — real `EmbedService`/`Dispatcher`/
  `KernelAssembler`/`HostingService` boundaries; the closures land on
  the production paths CAP.2's composed battery pinned

## What landed

A prior worker landed the closure code under WIP commit `33775f1` and
ended before committing or verifying; this run audited the residue,
verified each routed gap flips its xfail green, wrote the closure
rulings + docs, and ran the full verification.

### The six routed gaps (closure code, `33775f1` + finishing pass)

- **DF-S2.4-1 (member a — landed; b/c stay open).** `hh-env`'s
  `Dispatcher` routes `net_egress` effects on `mediated` environments
  through `EgressMediator::gate` (durable `security.egress.requested`,
  token attribution, pure `decide_egress`, terminal deny/ask at the
  same lifecycle point as the monitor's own ask) and `::forward`
  (recheck → sentinels → durable `security.egress.decided` → wire →
  charge) at the wire point post-`committed` — the effect's
  write-ahead still precedes the wire and the consume-once
  re-resolution still guards the SSRF pivot. The broker seam defaults
  to `DenyAllResolver` (fail-closed; SV-8). Pin:
  `hh-env/tests/acceptance.rs::cap2_dispatch_net_egress_mediated`
  green un-ignored. Still open on the row: member (b) the
  ask-endorsement ingress (nothing feeds `endorse_asked` on resume),
  member (c) the `fork` rebind call site (broker custody seam).
- **DF-S2.8-1 (member a's builder leg — landed; residual set open).**
  `AssemblerPort::assemble` gains `AssembleInputs{model_call_id,
  prefix, window_cap_tokens}`; `KernelAssembler` runs the real
  `hh_context::assemble` over the run's durable prefix —
  `context.assembled` is the builder's canonical plan record
  (`plan_id`/`layout_ref`/`policy_ref`/`derived_from`/
  `occupancy_estimate`/`assembly_ms`/`compaction_state`), side-band
  emissions append durable under the same call scope, and an assembly
  error is recorded on the row rather than hidden. Pin:
  `cap2_turn_loop_runs_hh_context_assembler` green un-ignored. Still
  open: `deliver_wakeup` steer arm (OQ-316), `mark_scope_ended`
  sequencing, `resume_set` consumers, judged/human detectors, the OOP
  differential corpus.
- **DF-S3.12b-1 — DONE.** `measurement.experiment.closed` moved from
  `OPEN_AUDIT` to the enumerated `EXPERIMENT_CLOSED_FIELDS` partition
  (scalar members at the member bound; `under_utilised`/
  `budget_match`/`na_cells`/`utilization` at
  `AUDIT_FIELD_LIST_BYTES`). An honestly-measured exemplar closes
  `status: completed` — `cap2_experiment_exemplar_close_completes`
  green; the `assert_close_audit_cap` removal test asserts the landed
  row carries the refused members.
- **DF-CAP.2-1 — DONE.** `hh_hosting::proj::lift` shapes hosted
  `security.permission.*` members onto the native partitions —
  `params` → `request` (in full, record bound), `approval_wait_ms` →
  `wait_ms`, `provenance` declared on pending/decided and mint-stamped
  `participant_reported`, `permission_id` joins the pair.
  `cap2_hosted_attach_permission_rows_land` green un-ignored;
  `cap2_hosted_attach_permission_rows_refuse` still refuses
  `SchemaViolation` on an undeclared member.
- **DF-CAP.2-2 — DONE.** `analysis_ops::resolve_arms` admits the
  adapter-stamped `budget_enforcement` map on the bound subject run's
  manifest — a stamped `model_calls: enforced` arm satisfies
  `matched_cap`'s enforceability bar; `limits_enforced` is never read
  as evidence. `cap2_hosted_arm_stamped_enforcement_compares` green.
- **DF-S4.11-3 — DONE.** `respond_approval` is a protocol builtin on
  every supply surface — run-less, surface-run-scoped,
  `human_principal`-gated; `EmbedService::surface_respond_permission`
  re-derives from the durable fold (pending, already-decided,
  asked-risk, grants, active turn), mints `security.permission.decided`
  + `lease.granted` atomically, `request_id`-idempotent,
  `responder_provenance` stamped; retried `tools/call` folds durable
  decided rows (allow applies / deny refuses `DeniedByPolicy`).
  `cap2_supply_ask_respond_approval_round_trip` green un-ignored.
- **DF-S3.12b-2 — DONE.** The ADR-0024 rung-statement sweep ran over
  the whole directory: all 110 `docs/adr/ADR-*.md` records carry the
  `**Rung (ADR-0024 obligation):**` line (30 already-compliant + 80
  swept); `_TEMPLATE.md` now requires the line; `docs/adr/README.md`
  regenerated via `adr-index.sh docs/adr`.

### The finishing pass (this run)

- `crates/hh-cli/tests/in_process.rs` — the CAP.3 builder-side
  `context.assembled` record carries `plan_id` (a derived content
  address whose preimage incorporates the shifted sequence/view data)
  and `assembly_ms{value, measured_at}` (a measured wall — identical
  inputs still time differently). Both added to the CLI parity test's
  mechanical-fields allowlist; the sole non-mechanical diff was
  `.payload.plan_id`.
- Stale xfail comments in `cap_2_composed.rs`, `cap_2.rs`,
  `acceptance.rs` rewritten to describe the CAP.3 closures.
- `#[allow(clippy::too_many_arguments)]` on
  `surface_respond_permission` (the record's arity is the §5g.7 answer
  shape — the codebase's own convention).
- `rustfmt` scoped to the touched files only (the WIP's `supply.rs`
  hunks were unformatted); `hh-embed-client-generated` untouched.

### Docs

- `docs/adr/ADR-0327` (close-row partition), `ADR-0328` (lift shaping),
  `ADR-0329` (`respond_approval` builtin), `ADR-0330` (closure rulings
  + the accepted-deviation set) — all `Accepted`, each with
  `## Revisit trigger`.
- `docs/build/CAPSTONE_CLOSURE.md` — the closure record + the
  ACCEPTED-deviations list (10 items) for GATE-ACCEPT signature.
- `docs/build/COVERAGE_MATRIX.csv` — `R-2.4.1`/`R-2.8.4`
  AT-RISK-INTEGRATION → PARTIAL; closure evidence + ADRs on
  `R-2.10.3`/`R-2.10.4`/`R-2.10.6`/`R-2.11.3`. Columns unchanged.
- `docs/tickets/DEFERRALS.md` — DF-S3.12b-1, DF-S3.12b-2, DF-S4.11-3,
  DF-CAP.2-1, DF-CAP.2-2 → **DONE** with dated evidence; DF-S2.4-1,
  DF-S2.8-1 → progress cells (remaining members stay OPEN).

## Verify

- `cargo test --workspace` → **`target/cap_3_ws.log`: 280 result
  blocks / 3028 passed / 0 failed / 2 ignored** —
  `cap2_fork_at_turn_boundary_without_explicit_snapshot` (DF-S2.9-3's
  cadence leg) + the pre-existing `ac_r_2_2_1_16_latency_1e5_fixture`.
  The six composed xfails from CAP.2 are all green un-ignored.
- Pinned suites: `hh-embed --test cap_2_composed` 13 tests (12 pass /
  1 ignored); `hh-env --test acceptance` 29/29; `hh-mcp-lab --test
  cap_2` 2/2.
- `cargo clippy -p hh-ledger -p hh-hosting -p hh-mcp-lab -p hh-bench
  -p hh-embed -p hh-control -p hh-env -p hh-cli --all-targets
  --no-deps` — clean on touched code (one pre-existing
  `too_many_arguments` in `hh-bench::benchset::load_task`, untouched
  since S4.15; the WIP's new `surface_respond_permission` carries the
  codebase's `#[allow]` convention).
- `rustfmt --edition 2021 --check` over the touched file set — clean
  after formatting the WIP's `supply.rs` hunks. No `cargo fmt --all`;
  `hh-embed-client-generated` never formatted.
- `bash scripts/check-drift.sh` — regenerated; schema/compat-matrix/
  generated-client **in sync** (no schema change; `respond_approval`
  is a protocol builtin, not an op).
- `bash scripts/check-removability.sh` — **serial** green across all
  stanzas (tier-0/1, S2.3 C1 surface, extension edge-freedom, S4.6,
  S4.9, S6.1a, S6.1b, S6.4, S4.11, S4.12). `cargo build -p hh-helper`
  green after.
- `bash ../agent-skills/skills/build-memory/scripts/
  check-build-memory.sh .` — **0 violations** / 7 pre-existing
  warnings (unchanged set).

### CC re-check (the capstone sweeps all ten)

- **CC1** — no second scheme: `provenance`/`responder_provenance`/
  `request_id` are declared members on the existing
  `security.permission.*` partitions; `EXPERIMENT_CLOSED_FIELDS` is the
  same class's partition, not a new class; `respond_approval` is a
  dispatch builtin, not a second catalogue.
- **CC2** — lifted rows stamp `participant_reported`; the adapter's
  `budget_enforcement` *stamp* is evidence, the `limits_enforced`
  *claim* is never read as proof; `responder_provenance` records who
  answered.
- **CC3** — charge-before-wire preserved at the wire point;
  `decided` + `lease.granted` atomic; `request_id` idempotency —
  nothing double-mints, nothing dropped silently.
- **CC4** — the open finding (S0.3 report naming "cargo" once in
  prose) adjudicated: a *report*, not a contract/data-model/criterion/
  stage — outside CC4's scope per the finding's own analysis, and the
  winning candidate's own tool. Left as-is; recorded here for the
  GATE-ACCEPT readout.
- **CC5** — no new `depends_on` edges; `surface_respond_permission` is
  a service method, not a contract dep.
- **CC6** — removability serial green all stanzas; `hh-mcp-lab` stays
  a leaf consumer; no new crate edges.
- **CC7** — check-drift regenerated + in sync; `respond_approval` is
  protocol-dispatch, not an op — the generated client is unchanged and
  unformatted.
- **CC8** — new members additive on declared partitions (absent-not-
  null); the partition enumeration tightens a kernel declaration, no
  `contract_version` bump.
- **CC9** — unstamped hosted arms still refuse `Unenforceable`;
  nothing is made comparable retroactively.
- **CC10** — partition shapes owned by §5g.7 in
  `hh-ledger::classes`; the lift shapes *into* the owned spellings,
  never redefines them; the surface verb lives in the §7.3 supply
  protocol's own dispatch.

## Deferrals

- **Closed:** DF-S3.12b-1, DF-S3.12b-2, DF-S4.11-3, DF-CAP.2-1,
  DF-CAP.2-2 (all `DONE` with dated evidence).
- **Progress:** DF-S2.4-1 (member a landed; b/c open), DF-S2.8-1
  (builder leg landed; residual set open).
- **Opened:** none — every residual is already carried by an existing
  OPEN row.

## ADRs

ADR-0327 · ADR-0328 · ADR-0329 · ADR-0330 — all `Accepted` at
2026-10-02.

## Next

`GATE-ACCEPT` — the chain pauses for the operator's signature on the
ACCEPTED-deviations set (`docs/build/CAPSTONE_CLOSURE.md`, 10 items).
projectStatus stays `IN_PROGRESS`.
