# CAP.1 — Capstone gap analysis (whole-build, fresh-context)

- **Verdicts recorded before any run ledger was read.** All 66 requirement
  verdicts were derived from `spec/CANONICAL_SPEC.md`, `docs/tickets/00_MANIFEST.md`,
  `docs/tickets/DEFERRALS.md`, and the composed tree at chain tip
  (`svitali/harnessharness-s6.4` @ `257dcf5`), then committed as
  `docs/build/COVERAGE_MATRIX.csv` in commit `efd71f5`. No
  `docs/build/runs/*` file was opened before that commit. Run ledgers were
  consulted afterwards only to annotate this report (test counts, landed
  slices, dated progress notes).
- **Matrix:** `docs/build/COVERAGE_MATRIX.csv` — one row per
  `R-2\.[0-9]+\.[0-9]+[a-z]?` id (66 rows, exactly the spec's id universe;
  includes the `R-2.7.2` parent, which exists only as a split parent under
  ADR-0146).

## Method and commands

1. Enumerated the requirement universe: `grep -oE 'R-2\.[0-9]+\.[0-9]+[a-z]?' spec/CANONICAL_SPEC.md | sort -u` → **66 ids** (65 `specified`/`specified-by-ADR` + 1 `deferred(ADR-0210)`, per Appendix A).
2. Read the spec in full (all §§1–11 + Appendix A/B), the manifest (chain order, requirement→ticket index, cross-cutting invariants CC1–CC10/CC13, human prerequisites HUMAN-H1/H2, gates), and every row of `DEFERRALS.md` (97 rows; status read from the trailing cell — ~34 DONE, ~4 PARTIAL, ~59 OPEN incl. 2 P-rows).
3. Surveyed the composed tree at the chain tip (50 crates) for landed evidence per requirement: primary module, boundary ops, and test nodes. Every verdict cites `path:line`/test evidence in the matrix.
4. Re-checked the manifest's cross-cutting invariants against the tree (durable-before-visible append path; participant_class/observability stamps on every event incl. `hh-ledger/src/hosted.rs`; typed `n/a{reason}` via `MetricDeclaration`/`NaReason`; honest typed refusals rather than fabricated success — `stage_pending`, `UnresolvedRef`, `SnapshotUnavailable`, `UnresolvedSummarizer`, `detached:"parked"` all observed live).
5. Seam hunt (below): traced call sites across ticket boundaries with `grep -rn` on the composed tree, not on self-reports.
6. Recorded all verdicts in the CSV and committed (`efd71f5`) before opening `docs/build/runs/*`.
7. Afterwards, read `docs/build/runs/S6.4.md` + `LEDGER.md`/`BUILD_INDEX.md` to annotate landed-slice detail and confirm the workspace test posture at tip (3010 tests / 0 failures / 1 pre-existing ignored, per the S6.4 ledger — annotation only, not a verdict input).

## Roll-up by verdict

| verdict | count | ids |
|---|---|---|
| MET | 29 | R-2.1.1, R-2.1.2, R-2.1.3, R-2.1.5, R-2.1.6, R-2.2.2, R-2.3.1–R-2.3.4, R-2.4.5, R-2.5.1, R-2.5.4, R-2.5.5, R-2.6.2–R-2.6.5, R-2.7.2a, R-2.7.3, R-2.8.1, R-2.9.5, R-2.9.7, R-2.9.8, R-2.10.1, R-2.10.2, R-2.10.4, R-2.10.6, R-2.12.2 |
| MET-DIFFERENTLY | 6 | R-2.7.2 (split, ADR-0146), R-2.11.2 (E3-ecosystem binding gate-pending; in-ecosystem proxy), R-2.12.3 (decision + E1 impl; foreign legs deferred), R-2.12.4 (spec-deferred, ADR-0210), R-2.12.5 (program-level), R-2.12.6 (HUMAN-H2 unprovisioned; fixture proxy) |
| PARTIAL | 29 | R-2.1.4, R-2.2.1, R-2.2.3–R-2.2.5, R-2.4.2–R-2.4.4, R-2.5.2, R-2.5.3, R-2.6.1, R-2.7.1, R-2.7.2b, R-2.8.2, R-2.8.3, R-2.8.5–R-2.8.7, R-2.9.1–R-2.9.4, R-2.9.6, R-2.10.3, R-2.10.5, R-2.11.1, R-2.11.3, R-2.11.4, R-2.12.1 |
| MISSING | 0 | — |
| AT-RISK-INTEGRATION | 2 | R-2.4.1, R-2.8.4 |

No requirement is wholly absent — the build's honest-refusal discipline means
unlanded surface is always a typed refusal, never a gap. But 31 of 66 rows
carry open residual legs, concentrated in the two families below.

## Roll-up by requirement family

| family | ids | verdicts |
|---|---|---|
| Ontology + IR + compilation (§2–§3) | R-2.1.1–R-2.1.4 | 3 MET, 1 PARTIAL (profile_binding grammar + org layers + corpus gate) |
| Cross-cutting (§8: provenance, budgets, versioning, plugins) | R-2.1.5, R-2.1.6, R-2.12.1, R-2.12.2 | 3 MET, 1 PARTIAL (cross-impl conformance cells open to GATE-ACCEPT) |
| Runtime & durability (§5a) | R-2.2.1–R-2.2.5 | 1 MET, 4 PARTIAL (retention/compression, producer legs, snapshot cadence, env verbs) |
| Model plane (§5b) | R-2.3.1–R-2.3.4 | 4 MET |
| Context & memory (§5c) | R-2.4.1–R-2.4.5 | 1 MET, 3 PARTIAL, 1 AT-RISK (live turn-loop composition unwired) |
| Tools & action (§5d) | R-2.5.1–R-2.5.5 | 3 MET, 2 PARTIAL (PlanMap/composite residuals, exposure-catalogue emission legs) |
| Control & orchestration (§5e) | R-2.6.1–R-2.6.5 | 4 MET, 1 PARTIAL (durable-queue steer + interpreter legs) |
| Verification (§5f) | R-2.7.1, R-2.7.2, R-2.7.2a/b, R-2.7.3 | 2 MET, 1 MET-DIFFERENTLY, 2 PARTIAL (gate call site, judge binding) |
| Security & governance (§5g) | R-2.8.1–R-2.8.7 | 1 MET, 5 PARTIAL, 1 AT-RISK (egress mediator never invoked) |
| Measurement & evolution (§5h) | R-2.9.1–R-2.9.8 | 3 MET, 5 PARTIAL (emitter coverage, eval residuals, bundle fetch trust, debt field) |
| Harness Lab (§6) | R-2.10.1–R-2.10.6 | 4 MET, 2 PARTIAL (experiment-close audit cap, foreign formats) |
| Surfaces (§7) | R-2.11.1–R-2.11.4 | 1 MET-DIFFERENTLY, 3 PARTIAL (parked-detach, env verbs, SSE/oauth/respond_approval, DefinitionInput::Ref) |
| Program & organizational (§1/§5i/§11) | R-2.12.3–R-2.12.6 | 4 MET-DIFFERENTLY |

The two heaviest-residual families are **security & governance** (6 of 7 rows
below MET) and **context & memory** (4 of 5). Both trace to the same pattern:
machinery landed and unit-tested, producer/call-site composition unwired.

## Seam hunt

Inter-ticket seams, dual-owned fields, subsumed-but-unverified requirements,
and composed paths never run green — verified against the tree, not against
per-ticket self-reports.

### S1 — Egress mediation is built but never invoked (R-2.8.4; AT-RISK-INTEGRATION)

`EgressMediator` (`crates/hh-env/src/egress.rs:313`) implements the full
decide→row→refuse/ask/allow lifecycle over `decide_egress`
(`crates/hh-containment/src/egress.rs:630`), and `crates/hh-env/tests/egress_mediation.rs`
exercises it heavily. But the only callers of `EgressMediator` are that test
file — `grep -rn 'EgressMediator' crates/` finds no production call site. On
the actual tool-dispatch path, `PreconditionDomain::Network` is unconditionally
`violated` (fail-closed, `crates/hh-env/src/dispatch.rs` `check_state_preconditions`),
so a `net_egress`-class effect is refused at the tier check without ever
reaching the mediator. The spec's §5g.4 contract — egress requests mediated
through the policy table Π with ask/endorse semantics — composes nowhere on a
real path. This is the sharpest seam in the build: the security property is
*enforced* only by total refusal, while the mediation machinery sits test-only.

- Routing: **fix in CAP.3** (wire the `net_egress` effect surface to the
  mediator, or record an ADR ruling that fail-closed refusal is the intended
  C0/C1 behavior and downgrade the mediator to a declared Stage-4 surface).
  Existing ledger: DF-S2.4-1 (call site), DF-S1.12-1/-2 (Stage-2 mediator
  halves), DF-S2.4-2 (TLS transport).

### S2 — The context plane is wired at the port, not at the machine (R-2.4.1; AT-RISK-INTEGRATION)

`hh-control`'s turn-loop driver calls `AssemblerPort::assemble`
(`crates/hh-control/src/driver.rs:106`), and `hh_context::assemble`
(`crates/hh-context/src/assemble.rs:276`) is real and unit-tested. But the only
production `AssemblerPort` implementation is `KernelAssembler`
(`crates/hh-embed/src/runtime.rs:163–176`) — a documented pass-through: "no
`hh-context` builder is wired at the embed boundary." On a live run the
assemble→retrieve→compact sequence never executes `hh-context`; the
`ContextWindowExceeded → CompactionRequired → compact` route is unwired, and
`context.compaction.*`/`context.artefact.*` events emit only under test
drivers (DF-S2.8-1). The control driver does emit `context.artefact.delivered`
/`activated` payloads (driver.rs:2277–2466) — so the seam is specifically the
builder/retrieval/compaction composition.

- Routing: **fix in CAP.3** (a real `AssemblerPort` backed by `hh-context`, or
  an ADR ruling that the pass-through is the intended embed-boundary contract
  with composition owned elsewhere). Existing ledger: DF-S2.8-1 (producer call
  sites), DF-S1.19-1/-2 (emitter/detector legs).

### S3 — `measurement.experiment.closed` cannot be written at exemplar scale (R-2.10.3; PARTIAL → live defect)

`AUDIT_FIELD_MAX_BYTES = 512` (`crates/hh-ledger/src/classes.rs:186`) refuses
`AuditFieldsTooLarge` (`crates/hh-ledger/src/store.rs:3535`). With every matched
dimension honestly measured, the `closed` row's `utilization`/`budget_match.detail`
exceeds the cap — so an honestly-measured exemplar experiment **cannot close**
(`CloseStatus::Completed` unreachable at exemplar scale; DF-S3.12b-1). This is
a contract-shape ruling problem (row members vs class cap), not missing code.

- Routing: **fix in CAP.3** — needs the class-declaration/row-shape ruling the
  deferral names (shrink members, `content_refs` offload, or re-bound the
  class). This is the only seam that outright *blocks* a specified path.

### S4 — Snapshot-producer cadence (R-2.2.4/R-2.2.5; PARTIAL)

`fs_tree` snapshots exist only where a test or op calls
`EnvDriver::fs_tree_snapshot`; nothing snapshots at checkpoints/turn boundaries
on a real run, so `fork{env: snapshot}` on an unsnapshotted run always returns
the typed `SnapshotUnavailable` refusal (DF-S2.9-3). The machinery
(`snapshot_for`, `derive_from_snapshot`, `rollback_env`, `SnapshotMissing` on
GC'd blobs) is landed and tested; the producer policy is the unwired leg.

- Routing: **defer** — already covered by DF-S2.9-3 (owner: env-driver ticket);
  CAP.3 could wire a minimal cadence if it wants a green end-to-end fork leg.

### S5 — The surface-approval round-trip cannot complete on a surface run (R-2.11.3/R-2.8.7; PARTIAL)

`respond_approval` exists and is human_principal-gated
(`crates/hh-mcp-lab/src/dispatch.rs:338`), but it resolves its run handle to a
*launched* run's session; a surface run is never minted as a `hnd-run-*`
handle, so a pending `security.permission.pending` row minted on the caller's
surface run cannot be answered (DF-S4.11-3). The ask is durable; the reply
path is a surface-verb ruling still owed.

- Routing: **defer** — DF-S4.11-3 already carries the residual + verification
  path (surface-permission reply surface). Related: SSE half (DF-S4.11-1) and
  oauth/mtls mediator (DF-S4.11-2).

### S6 — Dual-owned fields (checked, no silent divergence found)

Fields written by one subsystem and read by another were spot-checked on the
tree:

- `participant_class`/`observability_level` — stamped on every event envelope
  incl. hosted ingestion (`crates/hh-ledger/src/hosted.rs`); consumed by
  metric applicability (`MetricDeclaration`/`NaReason` in
  `crates/hh-ontology/src/compliance.rs`) and hosted analysis rows.
- `configuration_id`/`configuration_version_id` — minted in the registry,
  carried on `RunManifest`, pooled by `hh-identity::sameness`
  (`pools_by_configuration_id`, `crates/hh-identity/src/sameness.rs:110`).
- `forked_from`/`continued_from` — written at `open_run`, re-verified inside
  `verify_run`/`open_run` (`forked_signed_head_is_fork_equivocation`;
  DF-S2.5-2 DONE).
- `evidence_ref`/`debt_ref` — ledgered probation + removal-test rows minted by
  `hh-debt`, consumed by `hh-evolution` gates (S6.1b/S6.4 legs).
- `delivery_id` — written by context delivery, read by `detect_followed`
  (`crates/hh-control/src/driver.rs:2466`).
- `run_kind = fleet` — minted by the fleet reconciler (`crates/hh-fleet/src/spec.rs`),
  read by analysis partitions.

No field was found written-only or read-only across a boundary; the residual
risks are producer *absence* (S2/S4), not shape drift — consistent with CC7's
single-schema-source discipline holding.

### S7 — Subsumed-but-unverified and human-bound legs

- **Cross-implementation conformance (R-2.12.1)** is the largest carried
  residual: six V-rows (DF-S0.3-2/-3, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1,
  DF-S1.27-1) all name the same missing half — a second-ecosystem
  implementation/human cross-camp review that no build ticket can mint. All
  carried to GATE-ACCEPT by manifest design.
- **ADR-0024 rung-statement uniformity** (DF-S3.12b-2): the audit found 43 of
  74 `ADR-02**.md` records lack an explicit migration-ladder rung statement;
  the row explicitly names "the CAP.1 pass or a dedicated chore ticket" as
  owner. CAP.1 records the finding; the sweep itself is a 43-file append-only
  pass — routed below.
- **HUMAN-H1/H2** (DF-S4.10-1, DF-S5.6-1): the E3 surface-ecosystem binding and
  the real issue tracker are operator provisioning; the build landed honest
  proxies (in-ecosystem generated client; fixture adapter) — MET-DIFFERENTLY,
  never fabricated.

### S8 — Composed paths never run green end-to-end

Beyond S1/S2 (whose pieces never compose at all), the candidates CAP.2 should
exercise first: (a) a hosted participant run lifted end-to-end through
`hh-hosting/1` → ledger → `hh-analysis` comparison — the pieces exist
(`adapter_zero`, hosted ingestion, analysis kernel) but no test drives the
whole spine; (b) an experiment from `lab.experiment.*` boundary ops through
sweep engine to `measurement.experiment.closed` — blocked at the close row by
S3 until the cap is ruled; (c) a `suspend`→snapshot→`fork{env:snapshot}`→child
run — blocked by S4's absent producer cadence. These are CAP.2's domain;
recorded here so the readout targets them first.

## Top gaps and routing decisions

| # | Gap | Requirement(s) | Routing |
|---|---|---|---|
| 1 | Egress mediator never invoked on the effect path (S1) | R-2.8.4 | **Fix in CAP.3** — wire the call site or ADR the fail-closed ruling; DF-S2.4-1 |
| 2 | Context builder never driven on a live run (S2) | R-2.4.1 (± R-2.4.2/2.4.3/2.4.4 legs) | **Fix in CAP.3** — real AssemblerPort or ADR ruling; DF-S2.8-1, DF-S1.19-1 |
| 3 | Honest exemplar cannot close (audit-field cap) (S3) | R-2.10.3 | **Fix in CAP.3** — row-shape/class ruling needed; DF-S3.12b-1 |
| 4 | Snapshot cadence absent on live runs (S4) | R-2.2.4, R-2.2.5 | **Defer** — DF-S2.9-3 (existing row, owner staged) |
| 5 | Surface-approval reply path (S5) | R-2.11.3, R-2.8.7 | **Defer** — DF-S4.11-3 (+DF-S4.11-1/-2) |
| 6 | Cross-impl conformance + human signatures | R-2.12.1 | **Defer** — six existing V-rows carried to GATE-ACCEPT (environment-bound, cannot be minted in-build) |
| 7 | Human prerequisites (E3 binding; real tracker) | R-2.11.2, R-2.12.6 | **Accept as deviation** — declared proxies recorded (DF-S4.10-1, DF-S5.6-1); discharge at operator provisioning |
| 8 | ADR rung-statement uniformity (43 records) | R-2.1.4 process hygiene | **Defer** — DF-S3.12b-2 already open and explicitly names CAP.1/chore; CAP.1 records, CAP.3 or a chore ticket sweeps |
| 9 | Env verbs / parked-detach surface residuals | R-2.2.5, R-2.11.1 | **Defer** — DF-S2.10-1, DF-S1.26-2 (typed refusals landed; surfaces owed) |
| 10 | Foreign import/export vocabularies + remote-transport trust fields | R-2.9.3, R-2.10.3, R-2.10.5 | **Defer** — DF-S4.2-1/-2 (machinery landed; four declared formats + trust fields await schema rulings) |

**New deferral rows opened by CAP.1: 0.** Every gap found was already carried
by an existing OPEN/PARTIAL `DEFERRALS.md` row — the deferral ledger's coverage
of the residual set is itself verified by this matrix (each PARTIAL/AT-RISK row
cites its carrying row).

## Deviations accepted by this analysis

MET-DIFFERENTLY rows record contract-satisfied-differently verdicts, each
anchored to an existing record: ADR-0146 (R-2.7.2 split), ADR-0050 (R-2.12.3
decision + E1 impl), ADR-0210 (R-2.12.4 spec deferral), ADR-0003/0006 (R-2.12.5
naming thesis), DF-S4.10-1/DF-S5.6-1 (HUMAN-H1/H2 proxies). CAP.1's routing
table above is itself a decision record — see ADR-0326.

## Bottom line

The composed build lands real, tested machinery for all 66 canonical
requirements — nothing is MISSING, and the honest-refusal discipline held
everywhere a surface was deferred. The residual picture is concentrated and
legible: **two composed paths were built but never wired** (egress mediation,
context assembly), **one specified path is blocked by a contract-shape
ruling** (experiment-close audit cap), and the remaining 28 PARTIAL rows are
ledgered residual legs — emitter halves, surface verbs, and
environment-bound verification cells — each already carried by an open
`DEFERRALS.md` row with a named verification path. CAP.3's fix list is three
items; everything else routes through existing rows or GATE-ACCEPT.
