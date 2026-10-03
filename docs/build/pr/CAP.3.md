# CAP.3 — capstone closure

**Stacked on:** `svitali/harnessharness-cap.2` (PR #98) · **Do not merge — operator-controlled**

## Summary

The capstone closure ticket: every gap CAP.1's analysis routed here
and CAP.2's composed spine surfaced is **closed with a green test**,
**accepted as a deviation** (the ACCEPTED-deviations set in
`docs/build/CAPSTONE_CLOSURE.md`, proposed for operator signature at
GATE-ACCEPT), or **re-anchored to a named `DEFERRALS.md` row**.

**Five of the six routed gaps closed; the sixth (DF-S2.4-1) closed its
load-bearing member** — all six composed xfails flipped green
un-ignored, leaving exactly one `#[ignore]`d composed test
(DF-S2.9-3's snapshot-cadence leg, a staged env-driver row).

Requirement ids: `R-2.8.4` (egress mediation), `R-2.4.1` (context
assembly), `R-2.10.3` (experiment close), `R-2.10.6` (hosting lift),
`R-2.10.4` (matched compare), `R-2.11.3`/`R-2.8.7` (supply-surface
approval), `R-2.1.4` (rung hygiene); deviations per ADR-0330 D5/D6.

## What closed

| Row | Closure | Proof |
|---|---|---|
| DF-S2.4-1 (a) | Dispatch routes `net_egress` on `mediated` envs through `EgressMediator::gate`/`forward` — durable `security.egress.requested`/`decided`, terminal deny/ask, charge-before-wire; `DenyAllResolver` default (fail-closed) | `cap2_dispatch_net_egress_mediated` |
| DF-S2.8-1 (a, builder leg) | `KernelAssembler` invokes real `hh_context::assemble` over the durable prefix via `AssemblerPort(AssembleInputs)`; builder's canonical `context.assembled` + durable side-band emissions | `cap2_turn_loop_runs_hh_context_assembler` |
| DF-S3.12b-1 | `measurement.experiment.closed` → enumerated `EXPERIMENT_CLOSED_FIELDS` partition (list-shaped members at `AUDIT_FIELD_LIST_BYTES`) — an honestly-measured exemplar closes `completed` | `cap2_experiment_exemplar_close_completes` |
| DF-CAP.2-1 | Hosted `security.permission.*` lift *shapes* onto the native partitions (`params`→`request`, `approval_wait_ms`→`wait_ms`, `participant_reported` provenance, `permission_id` join) | `cap2_hosted_attach_permission_rows_land` (+ `..._refuse` still refuses) |
| DF-CAP.2-2 | `resolve_arms` admits the adapter-stamped `budget_enforcement` map — the stamp is evidence, the `limits_enforced` claim never is | `cap2_hosted_arm_stamped_enforcement_compares` |
| DF-S4.11-3 | `respond_approval` is a run-less surface-run-scoped protocol builtin — `decided` + `lease.granted` atomic, `request_id`-idempotent, retry folds durable decided rows | `cap2_supply_ask_respond_approval_round_trip` |
| DF-S3.12b-2 | ADR rung-statement sweep: all 110 records carry `**Rung (ADR-0024 obligation):**`; `_TEMPLATE.md` mandates it; index regenerated | `grep -L 'Rung (ADR-0024' docs/adr/ADR-*.md` → ∅ |

Residuals stay on their rows: DF-S2.4-1 (b) ask-endorsement ingress +
(c) fork rebind call site; DF-S2.8-1 residual set (steer arm OQ-316,
`mark_scope_ended` sequencing, `resume_set` consumers, judged/human
detectors, OOP differential corpus).

## Contract-shape rulings (ADRs)

- **ADR-0327** — close-row partition: shape the row, keep the measured
  detail in place (rejected: global cap raise, `content_refs` offload,
  under-measuring).
- **ADR-0328** — the lift *shapes*, never passes through; the
  partition is the contract the lift meets.
- **ADR-0329** — `respond_approval` belongs to the supply protocol,
  not the participant artifact (`callable ⇔ revealed` preserved).
- **ADR-0330** — the CAP.3 closure rulings (egress wiring + residual,
  assembler wiring + residual, stamped-map admissibility, rung sweep)
  + the ratified ACCEPTED-deviations set.

## Matrix movement

`R-2.4.1`, `R-2.8.4`: AT-RISK-INTEGRATION → PARTIAL. `R-2.10.4`,
`R-2.10.6`: closure evidence appended. `R-2.10.3`, `R-2.11.3`: verdicts
hold with the named rows closed.

## ACCEPTED-deviations (for GATE-ACCEPT signature)

10 items — the six MET-DIFFERENTLY verdicts + the gate-accepted set
(R2 human signature, trigger-5 satisfied-by-analysis, five
foreign-toolchain cells, the G2 recorded set). Full table with
soundness + compensating controls: `docs/build/CAPSTONE_CLOSURE.md`.

## Verify

- `cargo test --workspace` → `target/cap_3_ws.log` (counts in
  `docs/build/runs/CAP.3.md`).
- Pinned suites: `hh-embed --test cap_2_composed` 13 tests (1 ignored —
  DF-S2.9-3); `hh-env --test acceptance` 29/29; `hh-mcp-lab --test
  cap_2` 2/2.
- clippy (touched crates), fmt (touched files), check-drift,
  check-removability (serial) + hh-helper build, check-build-memory.

## Next

`GATE-ACCEPT` — the chain pauses for the operator's signature on the
ACCEPTED-deviations set.
