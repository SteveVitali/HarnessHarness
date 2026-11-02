# Round-5 §9.3 amendment dossier — common amendment register (UNADMITTED)

- **Status:** PROPOSED — dossier preparation only. Nothing here is admitted,
  allocated an ADR number, or implemented. **GATE-G9 decides**
  ([ticket 174](../../tickets/174_GATE-G9__admit.md)); source edits
  implementing any semantics below may not land before that decision.
- **Authority:** GATE-G8 PASSED 2026-10-30 scoped authorization —
  admission-dossier preparation only (LEDGER GATE DECISIONS; operator
  verbatim "passed. please proceed"). REQ-H-004 governs the amendment
  protocol; REQ-E-210 governs schema/tier/dependency discipline.
- **Collection rule:** one register, three owner packets — **R5.3** prepares
  rows 1–2, **R5.4** returns the row-3 evidence/profile packet here, **R5.5**
  returns the rows 4–7 method packets here. No agent acceptance/signature
  appears anywhere in this register. ADR numbers are allocated only when a
  later admitted ticket executes; until then proposals are cited as
  `PADR-R5-<row>` (proposal id, not an ADR number).
- **Naming rule:** every interface, event class, field, view and record name
  below is an **unadmitted candidate** (marked `∗`). Candidates are not
  already-existing RPC/wire commitments (I-02); an admitted name lands once
  in the owning codec/class table/home (CC1/CC7/CC10) at implementation.
- **Sources:** supplement [§9.3](../../evidence-and-adoption-spec.md#s9) rows,
  [§2](../../evidence-and-adoption-spec.md#s2), [§3](../../evidence-and-adoption-spec.md#s3)
  and [§4](../../evidence-and-adoption-spec.md#s4) requirements, canonical spec
  §06.3.2.2–2.4 / §06.5 / §05h.2.3–2.4 / §§05a/05b/05c/05e / §08.2 /
  §10.1 / §10.2 / §05h.2.1 / §06.4 / §10.5.1, research
  ADR-0155/0161/0162, research ADR-0158/0046/0077,
  [14_integration-design.md](../../design/14_integration-design.md)
  F1–F5 (retrieved 2026-10-29T20:53:30Z; anchors re-confirmed 2026-10-30) and
  [23_parity-method-review.md](../../design/23_parity-method-review.md)
  F1–F8 (retrieved 2026-10-30; code anchors re-confirmed at 8eeb3740).

## Seven-row amendment register

| # | §9.3 row (scope / exact source sections) | Owning requirements | Packet owner | Packet | Status |
|---|---|---|---|---|---|
| 1 | Launch/required grade/settlement: §06.3.2.2–2.4 (R-2.10.3), §06.5 overlays (R-2.10.5), §05h.2.3–2.4; ADR-0155/0161/0162 | REQ-E-201–204; experiment/embed/bench/eval/results | R5.3 | [Packet A](#packet-a) | **prepared** (this ticket) |
| 2 | Effects/prepared calls/derived admission: §§05a/05b/05c/05e and §8.2 | REQ-E-205–207; env/context/gateway/budget/control | R5.3 | [Packet B](#packet-b) | **prepared** (this ticket) |
| 3 | Native closure/profile: §05h.3 R-2.9.3/§10.4 and ADR-0363 collector linking | REQ-F-001–008, REQ-E-208; bundle/producer/ontology | R5.4 | [Packet C](#packet-c) | **prepared** (this ticket) |
| 4 | Finite-task efficacy: §10.1 reporting, §05h.2/R-2.10.4, ADR-0158 D1–D3/D8–D9 | REQ-D-107/109, REQ-E-209; eval math/lab metadata/analysis | R5.5 | [Packet D](#packet-d) | **prepared** (this ticket) |
| 5 | Statistical parity grouping: §10.1 replicate factor, §05h.2.1 compare, ADR-0046 CF-095 | REQ-D-302/310; bench/lab/eval | R5.5 | [Packet E](#packet-e) | **prepared** (this ticket) |
| 6 | Finite-manifest equivalence: §10.1 estimator/multiplicity, §10.2 equivalence, §05h.2.1, ADR-0158 D2/D7/D9 | REQ-D-303–307/310; eval/lab/analysis/bench | R5.5 | [Packet F](#packet-f) | **prepared** (this ticket) |
| 7 | Detectors/analysis metadata: ADR-0077 D4, §§05c/05h/06.4/10.5.1 | REQ-D-003/005/008/106/109; eval/context/lab/analysis | R5.5 | [Packet G](#packet-g) | **prepared** (this ticket) |

Sibling owners fill only their slot; a slot cites the §9.3 row verbatim until
its packet lands. Rows 4–7 packets must additionally carry the R5.5 AC-1
items: shared population/metadata contract, additive integer-canonical
numerical representation, frozen error/tolerance/doubling rules, exact
simulation parameters/seeds, routing review and approval scope, preserved S11
confidence labels.

<a id="packet-a"></a>
## Packet A — PADR-R5-1∗: opted-in required-grade mode; execution-cost settlement once; immutable graded disposition/revision

Landing owners **experiment, embed, bench, eval, results** (REQ-E-201–204,
+ REQ-E-008/102/103 adopted hooks).

### A.1 Exact before → proposed after

**Before (re-confirmed at 33658aa3).**

- §06.3.2.2 / ADR-0155 D2/D5: `settle(run_plan, run_id) → RunOutcome` fires on
  `lifecycle.run.finished` — outcome class per ADR-0106, `grade` per ADR-0144
  charged `instrument`, utilisation posted, accept/supersede, claim released,
  slice remainder returned: **terminal-derived final settle alone**; grading
  is not a settlement input.
- §06.3.2.3 / ADR-0155 D5: six outcome classes `{scored, budget_exhausted,
  refused, oracle_failure, infrastructure_failure, cancelled}`;
  `oracle_failure` ⇒ row kept, regrade only (overlay run, §06.5/ADR-0161 D2 —
  never appends to the subject ledger); superseded runs never count as
  replicates.
- §06.3.2.4 / ADR-0155 D4: matched-budget enforcement E-1 register → E-2
  launch → E-3 settle → E-4 close.
- §06.5 / ADR-0161: `ResultsRow/1` projection, row versions by supersession,
  `ScoringContext.overlay_runs[]`, watermark-repeatable reads; §06.5 /
  ADR-0162: closed experiment-ledger event family, immutable analysis
  records.
- §05h.2.3: outcome-class derivation + veto invariants; §05h.2.4: per-run
  scorer boundary `(end-state, ledger cursor, artifact refs) → [MetricValue]`.
- Inspected anchors (F2): `hh-control/src/driver.rs` ~5009 stamps M1
  `wall_ms` from the turn-1 open — the created→finished origin correction is
  the §9.3 *existing-contract fix* (R-2.9.1), routed beside this amendment,
  not inside it; `hh-experiment/src/engine.rs` ~1508 `settle` charges once at
  terminal facts; `hh-results/src/projection.rs` ~202 latches overlay
  `oracle_failure`.

**Proposed after (candidates — unadmitted).**

1. `launch_bound∗` recoverable launch record persisted before arming:
   launch intent, lease, funding slice, actual subject root budget, grade
   binding; recovery links existing manifests or idempotently
   aborts/releases — no cross-run atomicity, orphan-root spend or second
   subject. `adopt_subject(binding, environment, ports)∗` reuses the
   existing runtime, one subject and the current fenced writer;
   stale/duplicate/mismatched adoption refuses.
2. Required-grade mode **opted in and durably frozen before launch**: mode,
   task/submission/oracle pins, selection/regrade policy, instrument
   funding. Absence of the freeze preserves legacy mode byte-for-byte;
   unknown required features refuse (versioned negotiation, never silent
   downgrade).
3. Stop participant actions → drain committed work to observed/unknown →
   revoke access → finalize usage → seal immutable end-state/submission
   (failed sealing yields no grade). M1 stays created→finished including
   adoption delay, approvals, restart downtime and drain/seal; instrument
   and end-to-end spans stay separate.
4. `execution_settled∗` — deduplicated execution-cost settlement +
   returned funding recorded **exactly once**; `grade_pending∗` is an
   **admission state, not a seventh outcome class**. While pending,
   scheduler, results/query, export, compare, analysis, participant
   relaunch and non-partial close block scored-success/completion
   interpretations; partial close lists every unresolved slot. Resource
   release conveys no correctness.
5. Bench persists the instrument invocation before running the unchanged
   pinned oracle on the sealed submission (executable/environment/access,
   exit, raw stdout/stderr/reward, timeout/refusal, grade mapping,
   instrument finish/cost); invoked/verdict refs bind subject, submission,
   oracle, attempt, policy, cost. First-valid grade within the frozen
   valid-grade set; wrong-but-valid is valid zero; crash/timeout/empty/
   non-finite/refusal is `oracle_failure` permitting capped instrument-only
   regrade — never a participant rerun, recharge or new n.
6. `settle_graded∗` / `plan_disposition_at(scoring_ref)∗` — shared pure
   selection over owner-validated targets/causes/watermarks (no eval→ledger
   edge); a valid grade restores the stop-derived outcome class (possibly
   `budget_exhausted`); raw grade, stop and veto stay separate.
7. Authorized invalidation/correction appends reason, selection revision
   and supersedes links — immutable revisions, never edits; invalidated
   evidence remains unavailable/`oracle_failure` until replaced; reports
   pin eligible grade set, selection refs, prefixes and watermarks so old
   reports reproduce old selections.

### A.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| `launch_bound∗` | experiment + budget (hh-experiment, hh-budget) | intent→allocate/open→bind→arm; funding slice + grade binding refs |
| `adopt_subject∗` | embed arming/drive (hh-embed) | binding + execution profile on the subject run's fenced writer |
| durable effect result | control/env Dispatcher (hh-control, hh-env) | Packet B's seam — packet A consumes, never re-defines |
| instrument oracle | bench/verification (hh-bench, hh-verification) | invoked/verdict refs on the instrument run/Account |
| selection/disposition | eval + experiment/results (hh-eval, hh-experiment, hh-results) | pure selection, `plan_disposition_at∗`, selection revisions |

### A.3 Durable payload / home / class candidates (all `∗`)

- `launch_bound∗`: `{intent_ref, lease_ref, funding_slice_ref,
  subject_root_budget_id, grade_binding{mode, pins_ref,
  selection_policy_ref, instrument_budget_ref}}`; home = the
  `run_kind = experiment` run's `measurement.experiment.*` family; class-table
  home `hh-ledger/classes.rs`, plane home `hh-ontology/planes.rs`.
- Adoption facts: extend existing lifecycle/binding payloads on the subject
  run — prefer invoked/verdict/bound extensions over new classes (F7).
- `execution_settled∗` on the experiment run: `{plan_id, run_id,
  settled_cost_ref, returned_funding_ref, dedup_key}`; `grade_pending∗` is
  plan/results admission state — not an event class, not a seventh
  `outcome_class` member.
- Instrument invoked/verdict refs: instrument-run events under existing
  verification/measurement planes; payload `{subject_run, submission_ref,
  oracle_ref, attempt_no, policy_ref, cost_ref}`.
- Selection revision (results owner): `{revision_id, reason, supersedes,
  authority, watermarks[], eligible_set_ref}` — immutable, one home.
- Any admitted kind/event/payload lands exactly once in class table +
  ontology home with durability/producer/audit/content-ref policy (CC10,
  I-02); no name here is committed.

### A.4 Authority proof for grade correction

- Correction is **authorized-only**: the revision record requires a named
  `authority`; analyst exclusions already require `origin = human`
  (ADR-0162 D2); no agent or scheduler infers grading authority — a
  correction lacking the authorized field is a typed refusal, never a
  warning.
- Evidence: supersedes chain + preserved inputs; revoked evidence stays
  `unavailable`/`oracle_failure` until replaced; authorized correction
  watermarks preserved; a material oracle/task/environment/method
  correction requires a new confirmatory cohort; regrading old submissions
  is a labelled diagnostic; clerical/recomputation corrections produce
  superseding reports with preserved originals.

### A.5 Recovery matrix (AC-2-1 crash windows)

| Window | Required behavior |
|---|---|
| pre-seal/finish | resume the same native subject; no second run |
| finish → grade | restore pending state without participant work |
| invocation → verdict | retain ambiguity; capped instrument-only retry |
| verdict → settlement | reuse the recorded verdict; never re-run oracle |
| transfer → disposition | settle once; cannot recharge |
| launch recovery | link manifests or idempotent abort/release; orphan funding refused |

### A.6 Tier / negotiation

- The mode freeze is data on existing owners; unknown required features
  refuse at launch (typed refusal) — that is the negotiation surface.
- Lab-side `register/next/close` and kernel-side `open_run`/allocation keep
  the ADR-0050 §8 boundary; any admitted boundary field travels
  `hh-embed-schema` → `hh-codegen` → all generated outputs with
  compatibility/tier-off checks — none is added by this dossier.
- No new tier or feature dependency; `tier-c*` removability unchanged.

### A.7 Compatibility and preserved legacy

- Legacy settle path, all six outcome classes, every old record/watermark
  preserved; mode absence ≡ legacy. M1 created→finished preserved — the
  rejected preterminal-oracle alternative extends M1 and is refused for
  that reason. No historical WAL/cost/denominator rewrite; old reports
  reproduce old selections; re-attempt/regrade denominators unchanged.

### A.8 Dependency edges

- Consumed by: P2 task/effect/oracle (sealed submission + instrument refs)
  and P7 four caps (subject root vs funding slice) — P1 interfaces precede
  both; P3 receipt (row 3) consumes grade/selection/usage roots; P5/P8 sit
  downstream per the §9.1/F7 graph.
- Consumes: Packet B's Dispatcher durable-result seam (one shared seam, no
  second definition); existing Account/reservation machinery unchanged.
- Forbidden edges held: no embed→bench, experiment→results, eval→ledger,
  lower-layer→runner, shadow result/receipt store or copied runtime.

### A.9 Acceptance mapping (AC-2-1 → numbered checks)

1. Stale/duplicate/mismatched adoption + orphan funding → refusal fixtures.
2. One actual Dispatcher effect and output joined to the launched subject;
   one valid writer/commit/terminal; no phantom second run.
3. Six crash windows (A.5) restore same subject/pending/costs.
4. failure→valid→failure overlays; invalid targets; authorized
   invalidation; duplicate settlement; veto; budget-exhausted valid grade.
5. Pending blocks independently in scheduler/query/export/compare/
   analysis/non-partial close; partial close lists unresolved slots.
6. Old-watermark reproducibility; long verifier, delayed adoption, approval
   wait and restart leave correct M1, no recharge, no new n.

### A.10 Alternatives rejected

- Preterminal oracle/submission grading — extends M1; rejected.
- `grade_pending` as a seventh outcome class — rejected (admission state).
- Mutable settlement/correction records — rejected (immutable revisions).

<a id="packet-b"></a>
## Packet B — PADR-R5-2∗: narrow adopted-subject/Dispatcher durable result; retained-view→wire; whole-vector derived-cap checks; actual-use/exposure enforcement

Landing owners **env, context, gateway, budget, control** (REQ-E-205–207,
+ REQ-E-012/110 adopted hooks).

### B.1 Exact before → proposed after

**Before (re-confirmed at 33658aa3).**

- §05a R-2.2.2: twelve-phase effect lifecycle `intended→authorized→
  prepared→committed→observed/unknown→terminal`; durable-before-dispatch
  `committed`; per-class delivery; `effect_ledger` projection; §05d kernel
  dispatcher owns resolve/prepare/execute/observe.
- §05b/§05c (F3 anchors): `hh-embed/src/runtime.rs` ~1803 **discards the
  retained view** after compaction; `hh-embed/src/gateway_model.rs` ~819/
  ~1343 rebuilds the transcript and dispatches unreserved — selected/
  omitted identities and legal joins are not persisted; wire bytes cannot
  be proven equal to the retained selection.
- §05e: driver chain `render → propose → interpret → authorize → execute →
  verify → persist → decide`; hh-env `dispatch.rs` ~200/~1750 Dispatcher
  owns suspension/commit/Kp4 for the adopted-subject drive.
- §08.2: `reserve` precedes every model call; Σ reservations + consumed ≤
  hard along the ancestor chain; `charge` idempotent on
  `(source_event, dimension)`; `tokens.*.total` are derived, never stored
  primary; `MatchSpec`/`budget_enforcement` unchanged.

**Proposed after (candidates — unadmitted).**

1. `DurableEffectRef∗` — the control/env Dispatcher validates an
   already-intended coordinate and the scoped writer, owns
   authorization→settlement **once**, and returns a durable effect
   reference; driver lifecycle rows for the same effect are suppressed,
   not duplicated (REQ-E-205, F4).
2. Durable retained selection: persist retained/omitted item and content
   identities plus legal tool/result, batch and required-group joins;
   rebuild the context view from this selection. The **existing gateway
   renderer** serializes exactly that view — it may never resurrect
   omitted transcript (REQ-E-206). Actual `evict_oldest` and
   `clear_tool_results` bindings/parameters freeze at admission;
   unknown/unbound variants refuse.
3. Whole-vector derived-cap check: Account `reserve` checks **projected
   derived totals** on every ancestor — consumed usage, outstanding and
   new primary holds, and slice holds — before sealed provider plan,
   durable request, deadline-bounded dispatch, durable actual/unknown
   usage, charge/release. Admitted primary token categories only are
   charged; `tokens.input/output.total` stay derived (REQ-E-207).
4. Actual-use/exposure enforcement: physical incurred/billed use and
   unknown exposure persist separately from logical scope totals; logical
   reversal does not replenish a physical ceiling; unknown bounds/exposure
   refuse enforced matching and unsafe dispatch; real elapsed deadlines
   persist across restart/wait; socket timeout is not remote cancellation;
   expiry blocks new dispatch, drains committed work to observed/unknown,
   charges incurred use and records overruns.
5. Fixed order: retained view → existing serialization + primary bounds →
   whole-vector Account reservation → sealed provider plan → durable
   request → deadline-bounded dispatch → durable actual/unknown usage →
   charge/release → guard. Capture actual dispatched bytes, inspectable
   content refs, call/transport-attempt joins, intended/served policy,
   next-dispatch, pressure/fallback/ineffective/impossible outcomes.

### B.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| `DurableEffectRef∗` | control/env Dispatcher (hh-control, hh-env) | one authorization→settlement; duplicate driver rows suppressed |
| retained selection | context (hh-context) | retained/omitted ids + joins; rebuild equality |
| retained-view→wire | gateway (hh-gateway, hh-embed render) | selected view → serialized bytes; sealed plan/request refs |
| derived-cap checks | budget/control (hh-budget, hh-control envelope) | whole-vector reserve evaluation; usage/charge/release facts |
| operational proof | same Dispatcher seam | REQ-E-205's §5/AC-5-2 cells (owed to P5 — see B.8) |

### B.3 Durable payload / home / class candidates (all `∗`)

- `DurableEffectRef∗`: `{effect_id, coordinate_ref, writer_lease_ref,
  settlement_ref}` on the subject run — extension of the existing
  `action.effect.*` family/dispatcher records, not a second lifecycle.
- Retained selection: `{policy_binding_ref, params, retained_ids[],
  omitted_ids[], joins{tool_result_pairs, batch_groups,
  required_groups}}`; home = context plane (`context.*` classes); rebuild
  equality is the invariant.
- Prepared/sealed call facts: `{selected_view_ref, sealed_plan_ref,
  request_ref, dispatched_bytes_ref, attempt_joins[],
  policy{intended, served}}`; home = existing `model.call.*`/
  `measurement.*` families — a hash without accessible content cannot
  demonstrate treatment.
- Derived-cap check: **no new primary dimension** — reads existing
  `DimensionId` counters/gauges + `MeteringFormula` derivations;
  usage/dedup facts stay under `control.budget.*` +
  `measurement.cost.attributed`; propagated leaf/ancestor copies count
  once under existing scoped Account deduplication.
- Wire fields, if admitted, originate only in `hh-embed-schema` →
  `hh-codegen` → all generated outputs (CC7); none is committed.

### B.4 Authority

- Dispatcher validates the already-intended coordinate and the scoped
  writer lease — the same fencing/writer rules as §05a; no new authority
  class; `authorize` stays `code`-owned (05e I5); no inferred grading or
  dispatch authority.

### B.5 Recovery

- Reservations die with the holder's lease; retries of `unknown` effects
  reuse the same reservation; restart/wait never resets a real elapsed
  deadline; recovery settlement is idempotent — duplicate/recovery
  settlement counts once; committed work drains to observed/unknown on
  expiry; no replenished physical capacity after logical reversal.

### B.6 Tier / negotiation

- Unknown or unbound treatment variants refuse; the fallback ladder ends
  at C0 `evict_oldest`; profile/binding negotiation is versioned data.
- No new tier; `tier-c1` helper/suspend surfaces stay removable; fixtures
  prove plumbing only — fixture versus live enforcement reported
  separately.

### B.7 Compatibility and preserved legacy

- The existing renderer is reused (no second serializer); the effect
  lifecycle is extended, never duplicated (side-ledger and fixed-counter
  alternatives rejected); Account dedup and dimension list unchanged;
  logical totals preserved beside new actual-use records;
  `MatchSpec`/`budget_enforcement`/hosted paths untouched; equal caps ≠
  equal consumption (`imbalanced` reporting unchanged).

### B.8 Dependency edges and owed predicates

- Packet A consumes `DurableEffectRef∗` (one seam); P2 task/effect proof
  and P7 four-cap enforcement build on P1 interfaces; {P1,P2,P7} → {P3
  receipt (row 3), P6 treatment}; {P2,P3,P7} → P5 operations;
  {P3,P6,P7,method/parity admission} → P8.
- REQ-E-205 contributes the Dispatcher seam contract here; its §5/AC-5-2
  operational cells (Kp4 sink, deny/pre-prepare, permission resume, real
  exhaustion — eight cells/24 trials) are owed to the P5 operations
  tickets (R5.32–R5.34) — recorded, not discharged, by this dossier.
- No runner-side ledger, alternate codec/store, new backend or new public
  boundary is proposed; a backend decision returns for its own scope
  review.

### B.9 Acceptance mapping (AC-2-2 / AC-2-3 → numbered checks)

1. Capturing transport proves both policies change expected payload bytes
   and preserve required/legal groups (AC-2-2).
2. Unknown policies, no trigger, ineffective reduction, fallback, no next
   dispatch, retries, crash/restart covered; exact
   policy→selection→assembly→sealed-request→actual-wire joins; no
   omitted-item resurrection (AC-2-2).
3. Every cap — `tokens.input.total`, `tokens.output.total`, `model_calls`,
   `time.wall_ms` — plus combined primary components and simultaneous
   child/ancestor/slice holds; actual ≠ reserved; duplicate/recovery
   settlement; failed/ambiguous usage; logical reversal after real spend;
   cache/role drift; restart/approval delay; expiry with committed work
   (AC-2-3).
4. Negative: no replenished physical capacity from reversal; no double
   counting; zero dispatch when exposure is unknown/unfunded; fixture
   versus live enforcement reported separately (AC-2-3).

### B.10 Alternatives rejected

- Side-ledger for effect settlement — rejected (one authoritative ledger).
- Fixed counter in place of whole-vector projected checks — rejected
  (cannot express ancestor/slice composition).
- Rebuilt transcript without durable selection — rejected (unprovable
  treatment; resurrection hazard).

<a id="packet-c"></a>
## Packet C — PADR-R5-3∗: additive `evidence-package/1`∗ typed member/role profile inside `hh-bundle/1`; owner-typed recipient closure; access/loss/procedure verdicts

Landing owners **bundle, producer (ledger/experiment/bench/verification/
budget/results/analysis/eval/identity/provenance), ontology** (REQ-F-001–008,
REQ-E-208 + merged aliases REQ-E-010/108/109).

### C.1 Exact before → proposed after

**Before (re-confirmed at b07d0c5).**

- §05h.3 / ADR-0139–0141 and `crates/hh-bundle/src/manifest.rs`: three-layer
  `hh-bundle/1` — `MemberRef{role, ref, media_type, size, status ∈
  {present, fetch, redacted, gc}, foreign?}` (:104), `FetchEntry{address,
  size?, locations[] credential-free, expires?}` (:257), `SubjectSection
  {run_ids, heads{run_id → {seq, event_id, hash}}, lineage, watermarks,
  status ∈ {open, finished}, experiment}` (:571), per-run chunked
  `LedgerExport`; `validate_bundle` S1–S9 (`check_completeness` = {S1, S2,
  S4, S7}); basis-derived `max_supported_level`; `reproduce` R0–R3;
  export lowering + `measurement.export.delivered` + loss reports; status
  by supersession.
- Inspected anchors: `export.rs` ~95–120 builds `ledger_page`/`ledger_tree`
  members — the member index enumerates page/tree addresses, while typed
  payload references inside event payloads (causes, sealed
  submission/grade/usage/selection refs) are not enumerated (the F5
  closure gap); `assemble.rs` ~167–179 derives `subject.status`
  `open|finished` from `lifecycle.run.finished`; `bundle_ops.rs`
  1365–1367/1567–1584 R0 folds `lifecycle.run.*` envelopes to
  `subject.status` with `lifecycle_status` drift rows — the §9.3
  "existing-contract fix" (R0 open/finished mapping, R-2.9.3) is already
  lifecycle-correct at this head and rides **beside** this amendment, not
  inside it; `scripts/live-verify.sh` ~1516–1533 `run_ledger_head` =
  sha256 of `docs/build/runs/<ticket>.md` — a build-note snapshot, never a
  native WAL head (§6.5 label).
- ADR-0363: `hh-live-evidence/1` records + `SPEND.jsonl` + `live-verify.sh`
  gating — preserved byte-for-byte; `live-executed` / `fixture-verified` /
  `foreign-run-verified` stay distinct classes.
- `crates/hh-bundle/Cargo.toml` deps = wire/identity/provenance/ontology/
  ledger/secrets/lab/telemetry; `hharness-results` depends **on** bundle —
  no bundle→bench/results edge exists or is added.

**Proposed after (candidates — unadmitted).**

1. `evidence-package/1`∗ — an **additive, versioned** typed member/role
   profile inside `hh-bundle/1`, owned by the bundle codec/validator; a
   **derived index** over the §6.1 member groups (required even when
   partial), recomputable from the named authorities at the recorded
   watermark — never a second authoritative store or receipt authority.
2. Immutable coordinates: attempt/grade-attempt/export identities from
   native bindings — plan id, attempt/launch binding, subject run,
   ordinal, predecessor/reason, settlement, sealed subject/submission
   digest, oracle/version, frozen selection and supersession/revision
   refs; `native_heads∗` `{run_id, seq, event_id, hash}` per included
   subject/experiment/instrument/overlay/lineage prefix plus projection
   watermarks; later appends do not change a snapshot; date/leg
   filenames and provider ids are never attempt identity.
3. `closure_walk∗` — owner-typed closure interface: traverse claim/
   procedure roots through native heads/manifests/pages/causes/lineage,
   ordinary refs, IR refs and **owner-typed payload refs** (response
   bytes, sealed submission/grade/usage/selection/rows/analysis); the
   owning codecs enumerate references — no hash regex, no second class
   table; visited dedup `{parent, role, field, media, size}` plus explicit
   traversal limits; an unknown relevant schema or an exhausted limit
   fails closure.
4. `closure_report∗` — access/loss/procedure verdicts: `{bundle_id,
   profile_version, procedure, recipient_class, checked_at, required,
   resolved, withheld, unresolved, verdict, failures[]}`; every required
   edge resolves to verified member bytes or explicit `fetch`/`redacted`/
   `gc`; `present` requires recipient bytes, never a producer assertion;
   `fetch` requires exact identity, allowed locator/trust/expiry/rights
   and verified arrival under existing durable rules — it never executes;
   a sidecar cannot raise a level.
5. Unknown-version behavior: an unknown required profile version/major
   refuses new claims while legacy inspection is preserved; optional
   additions cannot strengthen old evidence; a profile major bump is a
   superseding member family, never an in-place rewrite.
6. Producer export binding: every producer export reference names its
   lower owner — ledger (prefixes/causes/payloads/blobs), experiment
   (plans/launches/settlement), bench/verification (instrument/grade
   attempts), budget/telemetry (Account usage/cost view), results
   (rows/selections), analysis/eval (records/families),
   identity/provenance (access/claims) — bound by type/domain through
   owner enumeration; no crate edge is added to hh-bundle.

### C.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| `evidence-package/1`∗ codec + validator | hh-bundle | profile member inside `hh-bundle/1`; derived index only |
| heads/prefixes/causes/payloads/blobs | hh-ledger | fenced run ledger, chunked export, watermark reads |
| plans/launches/attempt bindings/settlement | hh-experiment | durable experiment events → population index |
| instrument/grade-attempt refs | hh-bench, hh-verification | invoked/verdict refs on the instrument run |
| usage/cost | hh-budget, hh-telemetry | reservations/actual/unknown; physical outlay separate (`cost_view` fold) |
| rows/selection/analysis | hh-results, hh-analysis, hh-eval | row versions, overlay watermarks, AnalysisSpec/report |
| identity/provenance/access/redaction | hh-identity, hh-provenance, hh-secrets | provenance mandatory; secret-bearing bundle invalid |
| class/plane homes if admitted | hh-ledger/classes.rs, hh-ontology/planes.rs | one home per kind (CC10) |

### C.3 Durable payload / home / class candidates (all `∗`)

- `evidence-package/1`∗ profile record — every field group is labelled
  **R** (required even when partial), **C** (required for a dependent
  claim/procedure) or **O** (optional), per §6.1/F2, so requiredness is
  reviewable field by field:
  - R: `profile_version`, `source_versions{code_commit,
    dirty_patch_member | dirty_unknown, runtime, toolchain, features}`,
    `claims[]{id, text, scope, class, population, native_refs,
    report_refs}`, `protocol_ref`, `phase`, `slot_set{plans, hash,
    count, close_reason, watermark}`, `attempts[]`, `native_heads[]`,
    `watermarks[]`, `pins{task, models, treatment, usage}`,
    `access{supersedes, losses, policy, rights, recipient_class,
    retention, owner}` (empty lists allowed).
  - C: `oracle_observation_refs[]` (task claims: pinned oracle, raw
    stdout/stderr/reward/exit, mapping, access/veto evidence),
    `grade_attempts[]` (task claims), `treatment_joins[]` (treatment
    claims: trigger→selection→actual request/response/call joins),
    `rows_analysis_refs[]` (quantitative claims),
    `operational_evidence_refs[]` (operational claims).
  - O: `legacy_refs[]` (e.g., a `hh-live-evidence/1` member: exact
    bytes/path/revision/digest, labelled build-note, never a native
    head), `presentation_refs[]`.
  - Carried as a typed member inside the manifest's member set under
    `idp/1`; derived — the record asserts nothing the named authorities
    did not durably produce. Historical prefix behavior is explicit:
    `native_heads[]` carries `lineage_prefixes[{run_id, up_to_seq,
    head_hash}]` verbatim for every included prefix, and prior
    `hh-bundle/N` manifests keep decoding unchanged (C.7).
- `closure_report∗` — versioned sidecar-class report (same shape as
  `BundleValidationReport`), pure over bundle bytes plus `fetch` when
  permitted; it cannot raise `max_supported_level`.
- `producer_ref∗{domain, owner, ref, role}` — each producer export
  reference bound to its lower owner's codec domain; enumeration lives in
  the owner, never in a scan of payload bytes.
- Member/edge status vocabulary reuses `MemberStatus{present, fetch,
  redacted, gc}` and `FetchEntry`; any admitted kind/event name lands
  once in the class table + ontology home with durability/producer/audit/
  content-ref policy (CC10, I-02) — none is committed here.

### C.4 Authority

- No stand-alone receipt authority: producers remain the authorities; the
  profile records references and verified bytes, never decisions; a
  process pass cannot erase a format failure or skipped oracle.
- Per-item access records require a recorded `rights`/`recipient`/
  `retention`/`owner`; unknown rights = withheld; no agent infers access
  or grading authority — mirroring Packet A's authority-field discipline.
- No public RPC is proposed; `closure_walk∗`/fetch are in-process owner
  interfaces, not wire commitments (I-02).

### C.5 Recovery

- Regrade/correction/redaction/export retry produces a new snapshot by
  supersession, never rewrites; create-if-absent identities refuse
  conflicting bytes; latest pointers are derived.
- `fetch`/`redacted`/`gc` statuses carry their records; expiry/GC/deletion
  emits permitted superseding tombstones and disables dependent
  procedures even after earlier passes; a required-input loss marks the
  dependent procedure unavailable — arithmetic may survive only while row
  inputs remain.
- Unknown relevant schema, exhausted traversal limit, or
  missing/wrong/expired/unauthorized fetch → typed failure/refusal; no
  producer home/cache/path/credential fallback, no secret-bearing URLs.

### C.6 Tier / negotiation

- An unknown required profile version refuses (versioned negotiation —
  the §9.3 row's "unknown required versions refuse"); optional members
  decode absent (additive, mirroring `SubjectSection.experiment`'s
  additive decode).
- No new tier or feature dependency; hh-bundle stays in its current
  removability scope; fixtures prove plumbing only — fixture versus live
  enforcement reported separately.

### C.7 Compatibility and preserved legacy

- Every `hh-live-evidence/1` byte and ADR-0363 meaning preserved;
  `run_ledger_head` is labelled a build-note snapshot in new views —
  never relabelled native WAL; no reconstruction of missing attempts, no
  evidence-class upgrade, no historical prefix rewrite.
- R0 `open|finished` lifecycle semantics preserved (already correct at
  this head — C.1); prior `hh-bundle/N` manifests keep decoding; unknown
  required versions refuse while legacy stays inspectable.
- Old bytes/fields/levels preserved; the exported last event still agrees
  with the recorded head; later appends never mutate a snapshot; legacy
  receipts remain exact bytes.

### C.8 Dependency edges and owed predicates

- Consumes: Packet A's sealed subject/submission, `execution_settled∗`,
  `grade_pending∗` and selection-revision roots; Packet B's
  dispatched-bytes/usage/exposure facts — {P1, P2, P7} → P3 per the §9.1
  graph. P3 feeds P4 recipe, P5 operations and P8 study per the same
  graph.
- REQ-F-001–008 + REQ-E-208 full-target predicates stay owed to their
  coverage-register primary owners — R5.21 profile (REQ-F-001/002), R5.22
  closure (REQ-F-005), R5.23 population receipt (REQ-F-003/004/008,
  REQ-E-208), R5.24 R0 (REQ-F-007), R5.27 rights (REQ-F-006) — this
  packet is the admission-dossier slice, not the implementation; closing
  it does not satisfy those targets.
- Forbidden edges held: no hh-bundle→bench/results crate edge (dep
  manifest re-confirmed), no hash-regex reference discovery, no second
  codec/store/class table, no duplicated fold, no implied public closure.

### C.9 Acceptance mapping (AC-6-1/AC-6-2 → numbered checks)

1. Regenerate the profile from authorities; verify after producer
   stores/caches/credentials are inaccessible (clean-recipient closure).
2. Same-day attempts/regrades/export collision; removed
   slot/orphan/duplicate grade; pending/skip/format failure → all
   attempts retained, one first-subject denominator, explicit
   partial/deviation — never a favorable shrink or fabricated zero.
3. Stale watermark/build-note-head substitution; missing ordinary and
   payload-only blobs → typed coordinate/head/projection mismatch,
   closure failure.
4. Wrong/expired/unauthorized fetch; traversal limit; unknown profile
   version → typed fetch failure/refusal; recipient-specific loss.
5. Hierarchical/reverted/unknown costs; secret-bearing export;
   redaction/GC → one owning total with physical outlay separate;
   forbidden export invalid; dependent procedures unavailable.
6. Exact all-planned membership and typed losses; never infer unseen
   executions or grade truth (AC-6-1).
7. Six R0 cases (successful→finished, failed→finished, open→open;
   changed bytes, missing member, re-sealed wrong lifecycle) plus each
   advertised procedure independently after producer cleanup, and one
   consenting outside reviewer completing verify/recompute (AC-6-2) —
   owed to R5.24/R5.52, recorded not discharged.

### C.10 Alternatives rejected

- Stand-alone receipt store/authority — rejected; the §9.3 row names it
  rejected; the profile is a derived index.
- Hash-regex/byte-scan reference discovery — rejected; owner-typed codec
  enumeration only.
- Reverse dependency hh-bundle→bench/results — rejected; dependency
  direction preserved, producers pass records in.
- Legacy byte rewrite / build-note relabelled as native head — rejected
  (§6.5).
- Implied public closure — rejected; closure verdicts are recipient-class
  and time-scoped; public rows may support arithmetic while withheld
  members still block independent truth/fidelity checks.

<a id="shared-method"></a>
## Shared method contract (rows 4–7) — PADR-R5-4∗/5∗/6∗/7∗ common spine

The four method/detector packets share one spine so no packet re-defines it.
Every name is `∗` unadmitted; nothing here is implemented or admitted.

### S.1 Shared population and metadata contract

- **Population.** Method inputs are the existing `CellTable`/analysis-input
  population — cells keyed `(task_id, split_label, stratum, arm,
  replicate_index | pair_id, attempt_no, subject_run)` produced by the
  §06.3/§06.5 producer contract; first subject per slot is primary; the
  canonical, raw and joint-task-pass-without-observed-veto populations stay
  three named sets with class counts (§3.1). Short cells keep
  `planned`/`not-run` rows — no n inflation; retries/regrades add no
  independent n.
- **Metadata.** Every proposed method record carries declared
  `{estimator, version, target, assumptions[], prior?, keys,
  stratum_membership, seed, draws_requested, draws_used, floors,
  interval{kind ∈ {credible, confidence, concentration_bound}, level},
  refusal?}` — REQ-D-008/109/REQ-E-209 routing metadata on the report,
  never implied by a method name or registry entry.
- **Missingness.** Unknown/veto/not-run/short cells cannot improve a
  denominator; `[s/M, (s+u)/M]` bounds and equal-task completion envelopes
  are preserved (§3.2); missing required inputs refuse through the actual
  report path — no `n/a` laundering into a pass.

### S.2 Additive integer-canonical numerical representation

- All proposed quantities — interval endpoints, draws, counts, η/α levels,
  margins, tolerances — carry `i64` ppm fixed point (the existing
  `PPM`/`Interval` convention, `hh-eval/src/stats.rs`); canonical JSON
  carries integers only (`hh-wire/src/json.rs` `Json::Int` — no float ever
  reaches a canonical byte).
- Field additions are **additive**: new members decode absent on old
  records; no existing field changes meaning or unit.
- Comparison rule: endpoints are computed inward then **rounded outward**
  (lower bound floored, upper bound ceiled), so a stored ppm endpoint never
  overstates coverage; verdict equality stays strict per the owning method
  (Packet F's `L_Δ > −ε ∧ U_Δ < ε`).
- Posterior/resampling draws are `i64` ppm variates from
  canonical-input-seeded xorshift64* (the existing `XorShift64`); seed
  material derives from the declared spec/canonical inputs — recorded on
  the report, never shopped.
- Unspecified precision or encoding blocks the dependent method: a packet
  whose encoding or tolerance is not frozen here carries no implementable
  claim (AC-2).

### S.3 Frozen numerical error / tolerance / doubling rules

- Each method's tolerance `tol_ppm` is declared **before any numerical
  execution**; convergence is checked by draw-doubling — recompute at 2×
  draws through the frozen cap (efficacy posterior: 8,000 base → 64,000
  cap; whole-task bootstrap: ≥2,000 confirmatory; sign-flip: 8,000 Monte
  Carlo) — and the max endpoint delta is recorded as `numerical_error` on
  the report: a fact, never a silent retry or redraw.
- Bisection iteration bounds (Beta inverse-CDF, KL inversion) and their
  residual tolerances are frozen as proposed values here; actual draws,
  iterations and deltas land on the persisted report.
- Unknown precision ⇒ stop/requote or an explicitly approved limited scope
  (§4.3); post-observation tolerance changes are barred (AC-2).

### S.4 Frozen simulation generator parameters and seeds

- Proposed registered battery (frozen before execution; seed material
  allocated at admission): T ∈ {10, 20, 30} tasks × R ∈ {5, 10} replicates
  × effect ∈ {0, +.05, −.05, +.10, −.10} × task mixture
  {easy, hard, heterogeneous effects} × mechanism {zero activation,
  informative missingness/vetoes, shared shocks} — ≥2,000 datasets per
  scenario cell.
- Generator seed rule (proposed): `seed_sim = H(canonical(scenario_id ∥
  battery_version ∥ replicate_index))` — one derivable stream per dataset;
  the parameter table, seed list and every generator failure are retained
  on the battery record; no post-hoc seed selection.
- Reported per scenario: availability, coverage/prior sensitivity,
  relevant frequentist type-I error, widths/verdict probabilities and
  declared Monte Carlo error.

### S.5 Routing review and approval scope

- Nothing here is executed. The §9.2 **method/contract** reviewer/operator
  gate disposes the ADR semantics; the **numerical/routing** reviewer gate
  (known-answer/refusal/heterogeneity/missingness/simulation plus actual
  dispatch evidence) must pass before any method-dependent collection; the
  **endpoint/independence/margins** inputs remain sponsor-signed
  prerequisites.
- GATE-G9 scope is design + implementation admission only — numerical
  validation is evidence still to be earned and study funding stays $0;
  the D2 offline envelope (≤8 h, 4 cores/8 GiB, 32 core-hours, 2 GiB) is
  cited per stage, never pooled or authorized by this dossier.
- Independent derivation/routing/numerics review precedes implementation:
  existing owners (eval math, lab metadata, analysis persistence, bench
  pairs) implement only after admission; metadata-only method support
  fails (§9.2).

### S.6 S11 confidence labels preserved

- `credible{prior}` (Bayesian), `confidence` (frequentist),
  `concentration_bound` (KL/Hoeffding) and descriptive bounds stay four
  distinct label kinds; a credible interval is never reported as TOST or
  confidence, and a mixed Bayesian-efficacy + frequentist-calibration
  claim is not a joint 95% certificate (§4.5).
- S11 (spec §0.3): inferential **not-equivalent** requires the separately
  computed 95% reporting interval wholly outside the equivalence band
  under selected-claim multiplicity; disjointness of the 90% TOST
  interval is descriptive and the negative inference stays
  `inconclusive`.

<a id="packet-d"></a>
## Packet D — PADR-R5-4∗: labelled finite-manifest task efficacy — independent task/arm Beta(1,1) posterior contrast, frozen draws/doubling, whole-task bootstrap + sharp-null sign-flip companions

Landing owners **eval math, lab metadata, analysis** (REQ-D-107/109,
REQ-E-209).

### D.1 Exact before → proposed after

**Before (re-confirmed at 8eeb3740).**

- §10.1 distributional reporting (ADR-0045 D4; ADR-0157; ADR-0158):
  `EstimatorSelection{declared, floors{T_clt placeholder 100, T_bca 30},
  fallback_chain, substituted?}` on every interval; the task is the
  cluster, pairing key and resampling unit; `clustered_clt(task)` only at
  `n_tasks ≥ T_clt`, else Wilson / Beta-Binomial `Beta(1,1)` labelled
  `credible{prior}`; unclustered `clt` inadmissible for task-level metrics
  with replicates and `unit ∈ {tokens, money, ms}`; per-task `(c,n)`;
  pass^k/pass@k; tails; paired effects at matched budget with
  `test{permutation_signflip | wilcoxon_signed_rank | paired_bayes}`;
  `contrast` interactions; `outcome_bounds`; `multiplicity{holm|bh}`;
  `label{confirmatory|exploratory|preview}`; `n/a{reason}` closed set.
- §05h.2 / §06.4 (R-2.10.4): `analyze(AnalysisSpec, QuerySpec @
  watermark_set) → AnalysisReport` pure; `AnalysisRecord` persisted under
  supersession; `record_analysis` refusals (`UnmatchedBudget`,
  `NotPreRegistered`).
- ADR-0158: D1 task unit + `estimator_undefined`/`MissingClusterKey`; D2
  declared selection with labelled substitution (`bayesian_beta` small-n
  fallback beside Wilson); D3 paired effects; D8 declared resampling
  parameters (placeholders 2000 confirmatory / 200 preview); D9
  `interval_method` enum growth by dialect bump.
- Inspected anchors: `hh-eval/src/stats.rs` — `wilson`:159, `clt`:177,
  `clustered_clt`:194, `XorShift64`:211, `bootstrap`:243,
  `BOOTSTRAP_RESAMPLES = 4_000`:239 (hardcoded — the §4.3 "hardcoded
  4,000 draws" gap), `bootstrap_paired`:267, `bayesian_beta`:339 (integer
  hyperparameters, bisection on `regularized_beta`:371),
  `permutation_signflip_p`:454 (`(hits+1)/(draws+1)` in ppm);
  `hh-eval/src/compare.rs` ~638–648 `Pairing::ByTaskAndReplicate`
  requiring `seed_honoured` (CF-095), paired bootstrap ~781–791,
  `EstimatorSelection` ~883; `hh-analysis/src/kernel.rs`
  `resampling_draws`:97 (declared `AnalysisSpec` field); `hh-wire` Json is
  integer-only (`json.rs` :15–23).
- No finite-manifest **contrast posterior** exists: `bayesian_beta` is a
  per-cell interval; nothing samples `Δ^(b) = Σ_t w_t[p_Bt^(b) −
  p_At^(b)]`, w_t = 1/T, over independent per-cell Beta(1,1) posteriors;
  nothing reports a numerical-error/doubling record or the registered
  8,000-draw battery.

**Proposed after (candidates — unadmitted).**

1. `finite_manifest_contrast∗` — for the frozen finite manifest within each
   declared contamination stratum: `p_at | data ~ Beta(c_at+1,
   n_at−c_at+1)`, independent across task/arm cells; posterior draws
   `Δ^(b) = Σ_t w_t[p_Bt^(b) − p_At^(b)]`, `w_t = 1/T`; analytic
   posterior mean; a labelled 95% equal-tail **`credible{Beta(1,1),
   finite-manifest}`** interval; the observed equal-task contrast always
   reported beside. Task pairing is stratification, never
   correlated-replicate modelling; no pooled Beta, hierarchical or
   task-population claim.
2. Integer inverse-CDF draws: `i64` ppm variates via bisection on
   `regularized_beta` (integer hyperparameters — the existing primitive);
   canonical-input-seeded xorshift64*; **8,000 posterior draws** base
   with the frozen doubling check through **64,000** (S.3); actual draws
   and `numerical_error` recorded on the report; no seed shopping.
3. Refusals preserved: one task, missing keys, short/unequal required
   cells, unsupported dependence or a degenerate estimator refuse
   inference through the actual report path (D1/D2 vocabulary —
   `estimator_undefined`, `MissingClusterKey`, `InsufficientReplicates`).
4. Distinct companions, never merged into an omnibus verdict: whole-task
   paired bootstrap for rates/cost (BCa at ≥30 tasks, percentile below,
   declared seed/draws/degeneracy, ≥2,000 confirmatory draws —
   `bootstrap_paired` reused) as **task-mixture sensitivity**; task
   sign-flips under the registered lane-randomization/exchangeability
   argument (exact enumeration at ≤20 tasks, else 8,000 Monte Carlo draws
   at `(extreme+1)/(draws+1)` — `permutation_signflip_p` reused) testing
   the sharp no-effect null, not weak-mean or practical-margin nulls.
5. `Beta(2,2)` sensitivity predeclared **exploratory** only — it never
   replaces the registered `Beta(1,1)` prior.
6. Frozen simulation battery per S.4 with parameters/seeds/failures
   retained; pilot uncertainty propagated; the existing paired-binary
   power formula is a cross-check, never override authority.
7. Multiplicity: D2 Holm α=.05 across all three primaries and
   confirmatory strata including unavailable members; exploratory BH
   stays separate; interval coverage and multiplicity-adjusted tests
   remain distinct.

### D.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| `finite_manifest_contrast∗` estimator | hh-eval (math) | posterior draws/interval + doubling record on the report |
| estimator/floor metadata + `interval_method` member | hh-lab (registry declarations) | declared method/prior/floors before collection |
| `AnalysisSpec` seed/draws/family + persisted report | hh-analysis | `AnalysisRecord` carrying seed, draws, `numerical_error` |
| cell population (consumed, unchanged) | hh-results | `CellTable` first-subject canonical/raw/joint cells |

### D.3 Durable payload / home / class candidates (all `∗`)

- `finite_manifest_contrast∗` — additive `AnalysisReport` member:
  `{stratum, cells[{task_id, arm, c, n}], weights_ref, prior{beta, 1, 1},
  draws{requested, used, doubling[{draws, max_delta_ppm}], tol_ppm},
  seed, posterior_mean_ppm, interval{lo_ppm, hi_ppm, kind: credible},
  observed_contrast_ppm}` — report-payload data, not a new event class.
- An admitted `interval_method` member is a dialect bump landing once in
  the owning codec (hh-embed-schema → hh-codegen → all outputs if it
  crosses the boundary — CC7); none is committed here.
- Companion records reuse `bootstrap_paired`/`permutation_signflip_p`
  result facts — `ResamplePlan`/`EstimatorSelection` extended additively.

### D.4 Authority

- The estimator computes and labels; it never decides task truth beyond
  the declared finite-manifest target; unsupported inputs are typed
  refusals, never warnings; no new authority class.

### D.5 Recovery

- Reports are pure persisted `AnalysisRecord`s: same spec + inputs ⇒
  same report address; recomputation under the recorded seed reproduces
  the interval; supersession only.

### D.6 Tier / negotiation

- Unknown method/prior/family version → typed refusal at the report
  boundary; no new tier or feature dependency.

### D.7 Compatibility and preserved legacy

- The declared-selection ladder, floors, Wilson/per-cell-Beta fallbacks,
  `test`/`sign_profile`/`tail_effects`, outcome bounds and n/a vocabulary
  are unchanged; the observed contrast rides beside every posterior;
  no global floor reduction or historical re-estimation required
  (stored `(c,n)` permits it, never demands it).

### D.8 Dependency edges and owed predicates

- REQ-D-107 → R5.16, REQ-D-109/REQ-E-209 → R5.40 are the primary owners;
  this packet is their admission-dossier slice — the landings,
  simulation battery and dispatch evidence are owed to them and the
  §9.2 numerical/routing gate; nothing discharged here.
- Consumed by: R5.37 (task-mixture), R5.38 (KL/equivalence — Packet F
  inputs), R5.39 (parity admission), R5.49 (study report);
  {P3,P6,P7,method/parity admission} → P8 collection/report.
- Forbidden edges held: no new fitter/dependency, no second estimator
  home, no pooled-Beta substitution for the declared target.

### D.9 Acceptance mapping (AC-4-2 → numbered checks)

1. Analytic/known answers: closed-form per-cell posterior means/tails vs
   computed draws; canonical small-cell tables.
2. Numerical tolerance + doubling: 8,000→64,000 recompute; endpoint
   delta ≤ frozen `tol_ppm` or `numerical_error` recorded.
3. Heterogeneous/unequal n; prior placement (`Beta(2,2)` sensitivity
   labelled exploratory); independent-versus-correlated counterexamples
   refuse or are flagged as assumptions.
4. Floors/refusals: one task, missing keys, short/unequal cells,
   unsupported dependence, degenerate estimator.
5. Actual seed/draw/family routing; deterministic recomputation; the
   eval→analysis→persisted-report dispatch; a metadata change without a
   computation change fails.
6. Registered simulation battery per S.4 retained with Monte Carlo
   uncertainty; availability/coverage/type-I/widths/verdict
   probabilities reported.

### D.10 Alternatives rejected

- Pooled Beta over all cells — rejected (finite-manifest target).
- Hierarchical/task-population or correlated-replicate model — rejected
  (not the target; needs a fitter).
- Credible-as-TOST certificate — rejected (S.6).
- Retain hardcoded 4,000 draws — rejected (frozen 8,000 + doubling).
- Seed shopping — rejected (canonical-input-seeded stream).
- Combined omnibus efficacy verdict — rejected (distinct targets).

<a id="packet-e"></a>
## Packet E — PADR-R5-5∗: narrow preassigned H/O first-attempt parity grouping under an independent-across-pairs finite design

Landing owners **bench, lab, eval** (REQ-D-302/310).

### E.1 Exact before → proposed after

**Before (re-confirmed at 8eeb3740).**

- §10.1 replicate factor: "pairing by task, by replicate only where all
  participants declare `seed_honoured` (CF-095)"; §05h.2.1 `compare` row
  carries the same rule plus `UnmatchedBudget`,
  `InsufficientReplicates(task)`, `IncommensurableMatch`,
  `MissingMatchSpec` refusals; ADR-0046 D2–D5 supply the budget/
  task-set/split/image/fault/model preconditions and the pairing rule.
- `hh-eval/src/compare.rs` ~638–648: `Pairing::ByTaskAndReplicate`
  requires `seed_honoured` on every run (`by_task_and_replicate requires
  seed_honoured: {run_id}` refusal :130); no runner-side (H/O) pairing
  exists — the `hh-bench` parity fixture repeats recorded `model_io`
  with empty model snapshots and `equivalence_margin: None` (design F1);
  product smoke carries `parity=none`.
- Design F4: identities create no correlation/CRN — independent p=.5
  outcomes typically yield q+ = q− = .25 — so honoured-seed pairing
  cannot express the proposed design and is not extended silently.

**Proposed after (candidates — unadmitted).**

1. `parity_pair_grouping∗` — H/O **first attempts** preassigned within
   `(task, repeat, pair_id)`; for one participant/policy/stratum with T
   fixed tasks and equal repeats R, N = T·R pairs;
   `W+ = 1{Y_H=1, Y_O=0}`, `W− = 1{Y_H=0, Y_O=1}` with counts c+/c− feed
   Packet F's directional bound.
2. `independence_declaration∗` — a recorded **design assumption**:
   across-pair independence is required; within-pair dependence is
   allowed; the justification is signed before data and is never proved
   by reset logs, calendar balance or IDs. Known common shocks,
   cache/carryover or reused subjects → typed refusal or a separately
   admitted design.
3. No coupling claim: the grouping exception grants no CRN, honoured-seed
   coupling, R2 or causal pairing; requested/honoured seed status is
   still recorded per run (CF-095 preserved); §4.1's efficacy assignment
   is unaltered.
4. Floors preserved: adapter floor ≥3/task; the S3 floor ≥5/task/arm;
   task pairing unchanged. Unequal/missing `(task, repeat, pair_id)`
   identity, retried/superseded attempts or non-first attempts → typed
   refusal or recorded exclusion — never re-pairing after observation.

### E.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| authentic H/O attempt records + first-attempt binding | hh-bench | adapter-pair identities on run records |
| `Design`/`PreRegistration` pairing + independence declaration | hh-lab | frozen design metadata before first bound run |
| grouping consumed in compare/equivalence | hh-eval | c+/c−/N inputs to `equivalence_verdict∗` |

### E.3 Durable payload / home / class candidates (all `∗`)

- `pair_id` — additive member on the plan's attempt coordinates (a
  design/attempt metadata field, not a lifecycle event); `Pairing`
  member `parity_pairs{pair_id}` would be a dialect bump landing once in
  the owning codec if admitted (CC7).
- `independence_declaration∗{statement, justification_ref,
  known_shock_checks[]}` on the design record — immutable once the first
  run binds (mirrors `PreRegistration` immutability).

### E.4 Authority

- Pairing metadata is recorded under the frozen pre-registration; it
  confers no scheduler or oracle authority; refusal is typed, never a
  warning.

### E.5 Recovery

- Identities are assigned before launch; duplicated/missing pair ids are
  detected at bind; superseded attempts keep their original rows and
  reasons — no silent re-pairing.

### E.6 Tier / negotiation

- An unknown pairing member → typed refusal on versioned data; no new
  tier.

### E.7 Compatibility and preserved legacy

- The honoured-seed pairing path (CF-095) is unchanged; product smoke
  `parity=none` is retained until admission; `compare` budget/image/
  oracle/task-set preconditions are untouched; task pairing unchanged.

### E.8 Dependency edges and owed predicates

- REQ-D-302 → R5.39, REQ-D-310 → R5.40 are the primary owners; authentic
  pair capture and live-parity evidence stay owed to R5.39/R5.47 and the
  parity admission packet (AC-4-4).
- Consumed by: Packet F (c+/c−/N); R5.47 (parity-live), R5.49 (study
  report). Consumes: the S.1 population contract only — no Packet A/B/C
  seam is redefined.

### E.9 Acceptance mapping (AC-4-3 grouping cells → numbered checks)

1. Unequal/missing `(task, repeat, pair_id)` identity refuses.
2. Reused-subject / shared-shock / cache-carryover counterexamples
   refuse or route to a separately admitted design.
3. Empty pins and non-first attempts refuse; honoured-seed pairing path
   regression-tested unchanged.
4. Actual grouping routed through the compare/equivalence report path —
   metadata-only support fails.

### E.10 Alternatives rejected

- Reuse honoured-seed pairing unchanged — rejected (cannot express H/O
  pairs; F4).
- CRN or fitted-correlation model — rejected (no such claim admitted).
- Retry/best-of pairing — rejected (first attempts only).
- Product smoke as parity evidence — rejected (design F1).

<a id="packet-f"></a>
## Packet F — PADR-R5-6∗: directional-KL finite-manifest equivalence, completion envelope, bounded relative-mean test, one IUT conjunction exception

Landing owners **eval, lab, analysis, bench** (REQ-D-303–307/310).

### F.1 Exact before → proposed after

**Before (re-confirmed at 8eeb3740).**

- §10.2 equivalence (ADR-0046 D5; ADR-0158 D7): `equivalence_run` is a
  `compare` with a pre-registered per-dimension margin and two one-sided
  tests — `equivalent` iff the paired-Δ interval lies within ±margin,
  `not_equivalent` iff it excludes the band, else `inconclusive`;
  margins come only from `PreRegistration`.
- §10.1 multiplicity: Holm across pre-registered primaries, BH
  elsewhere; `multiplicity{holm|bh}` on `ComparisonReport`; ADR-0158 D2
  estimator selection + D9 enum growth.
- Inspected anchors (design F8): `hh-eval/src/compare.rs` ~622–634 /
  ~781–791 / ~883–892 always constructs the paired bootstrap with empty
  floors; equivalence parses relative margins but compares the raw Δ
  against ±margin_ppm; `hh-analysis/src/kernel.rs` ~557–569 /
  ~1583–1591 applies multiplicity over the same computation — no KL
  endpoint, no completion envelope, no relative-mean branch, no IUT
  conjunction exists.
- REQ-D-108's historical R5 width calculation remains an inspected
  diagnostic only — superseded by REQ-D-304 (with REQ-D-309/310
  dependencies) per the coverage register.

**Proposed after (candidates — unadmitted).**

1. `kl_directional_bound∗` — `K_η(x,N) = [L,U]` by continuous Bernoulli
   KL inversion `N·kl(x,L) = N·kl(x,U) = log(1/η)`, `L ≤ x ≤ U`;
   `L=0` at `x=0`, `U=1` at `x=1` (continuous endpoint conventions;
   `x=0 ⇒ U = 1−η^(1/N)`, `x=1 ⇒ L = η^(1/N)`); integer-ppm endpoints by
   bisection, outward-rounded (S.2/S.3). One-sided noncoverage ≤ η per
   endpoint — concentration, never CP/binomial inversion.
2. `equivalence_verdict∗` — TOST α=.05: `η = α/2 = .025` per direction;
   `point_Δ = (c+−c−)/N`, `L_Δ = L+−U−`, `U_Δ = U+−L−`; **equivalent ⇔
   strictly** `L_Δ > −ε ∧ U_Δ < ε`; touching/overlap ⇒ `inconclusive`;
   each null branch's size ≤ α; joint coverage ≥ 1−2α = .90; a separate
   95% reporting interval uses α=.025, η=.0125.
3. S11 verdict rule (S.6): inferential `not_equivalent` only when the
   95% reporting interval lies wholly outside the band under applicable
   selected-claim multiplicity; otherwise the outside-band wording is
   descriptive and the negative inference stays `inconclusive`.
4. `completion_envelope∗` — with `u` uncertain pairs the planned N is
   preserved: each direction becomes `[K_η(c/N).L,
   K_η((c+u)/N,N).U]` — a conservative envelope, never missing-at-random;
   canonical/raw/joint populations and class counts kept; informative
   exclusions cannot gain coverage by shrinking N; required conditional
   claims need another admitted method.
5. `bounded_difference∗` — for a required absolute bounded contrast
   `D = X_H − X_O ∈ [a,b]`: apply K to `mean((D−a)/(b−a))` and transform
   back; `a=b` is exact; zero sample variance never proves zero range.
6. `relative_mean_test∗` — relative margin ρ targets the **ratio of
   finite-manifest means**: lower bound for `E_H−(1−ρ)E_O > 0` and upper
   bound for `E_H−(1+ρ)E_O < 0`, using known physical ranges, affine
   normalization and K at α per direction; zero/near-zero reference ⇒
   frozen absolute convention or refusal. S11 negative verdict on a
   relative endpoint: `U(C−)<0` or `L(C+)>0` at one-sided α/2=.025 per
   negative branch; union controls .05 — no ratio interval or
   observed-reference division implied.
7. `iut_conjunction∗` — one preregistered **all-endpoints/all-strata
   equivalent** claim: the IUT needs no additional endpoint Bonferroni
   (at least one true null must be rejected); selected individual or
   multiple claims and any-endpoint efficacy keep their family
   correction; unknown members and compliance vetoes can never be
   removed or averaged away; the D2 efficacy Holm is unchanged.
8. Declared `α/η/target/range/unit/margin` per endpoint before data;
   unbounded/unknown billing, unbounded intensity, missing values
   without valid completion bounds, or changed eligible-only
   populations → `estimator_undefined`/unavailable inference — never a
   fabricated bound.

### F.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| `kl_directional_bound∗` + `equivalence_verdict∗` math | hh-eval | ppm endpoints/verdict on the report |
| dimension→metric/margin map metadata | hh-lab | declared targets/margins before data |
| report persistence + multiplicity/family records | hh-analysis | `AnalysisRecord` incl. unavailable members |
| authentic pair input (consumed) | hh-bench | Packet E's c+/c−/N |

### F.3 Durable payload / home / class candidates (all `∗`)

- `kl_bound∗{eta_ppm, n, x_ppm, lo_ppm, hi_ppm}`;
  `equivalence_verdict∗{point_delta_ppm, lo_ppm, hi_ppm,
  margin{kind: absolute|relative{rho_ppm}}, verdict ∈ {equivalent,
  not_equivalent, inconclusive}, reporting{lo_ppm, hi_ppm, eta_ppm}}`;
  `completion_envelope∗{c, u, n}`; `iut_conjunction∗{claim_ref,
  members[{endpoint, verdict}], unknown_members[]}` — additive report
  payload members; no new event class.

### F.4 Authority

- Margins come only from `PreRegistration`/the sponsor — never from the
  caller or post-outcome data; the verdict is a report fact; refusals
  are typed.

### F.5 Recovery

- Pure recompute over durable inputs; supersession only; deterministic
  under recorded seeds.

### F.6 Tier / negotiation

- Unknown margin/method/member versions → typed refusal; no new tier.

### F.7 Compatibility and preserved legacy

- The existing TOST path and estimator defaults are unchanged outside
  the admitted scope; the REQ-D-108 diagnostic stays inspectable;
  floors, outcome classes, veto axes and history are retained; no
  post-outcome margin or endpoint.

### F.8 Dependency edges and owed predicates

- Consumes Packet E's pair records; REQ-D-303 → R5.39,
  REQ-D-304–307 → R5.38, REQ-D-310 → R5.40 are the primary owners;
  R5.47 parity-live and R5.49 study report predicates remain owed —
  recorded, not discharged.
- {P3,P6,P7,method/parity admission} → P8; the dimension map, margins
  and both-runner quote are sponsor inputs outside this dossier.

### F.9 Acceptance mapping (AC-4-3/AC-4-4 → numbered checks)

1. Endpoints/continuous limits/monotonicity; strict and outward-rounded
   boundaries.
2. Small-N/heterogeneous tails; sign reversal; .90 TOST vs .95
   reporting interval distinction.
3. Missingness/veto completion envelopes at preserved N.
4. Relative scale invariance; zero/near-zero reference convention or
   refusal; unknown/unbounded cases unavailable.
5. Unequal/missing identity; empty pins; reused-subject/shared-shock
   counterexamples; selected-versus-conjunction family corrections.
6. Unsupported assumptions refuse through the actual report path;
   derivation + existing-owner routing reviewed before collection
   (design F8's acceptance line).

### F.10 Alternatives rejected

- D2's per-task CP/Bonferroni construction — superseded by this narrow
  proposal (concentration bound; design F4).
- Credible-as-TOST — rejected (S.6).
- Raw-unit vs ppm mixed comparisons — rejected (single canonical unit).
- Post-outcome CP fallback — rejected.
- Mean-of-task-ratios / observed-reference division / clipping /
  invented bounds — rejected (F5's explicit bars).
- A mixed Bayesian-efficacy + frequentist-calibration joint certificate
  — rejected (§4.5).

<a id="packet-g"></a>
## Packet G — PADR-R5-7∗: exact detector join windows, unbounded-intensity denominators and complete analysis metadata families

Landing owners **eval, context, lab, analysis** (REQ-D-003/005/008/106/109).

### G.1 Exact before → proposed after

**Before (re-confirmed at 8eeb3740).**

- ADR-0077 D4 metric declarations — `grounding.reacquisition_rate`,
  `reliability.repeated_action_rate`, `grounding.recall_probe@compaction`,
  `efficiency.compaction_overhead_share`, `efficiency.compaction_latency_ms`
  — `computed_from` ledger events under `trace_predicate` oracles;
  `n/a{no_detector}` without an extractor.
- §05c: `context.compaction.completed{…CompactionRecord{compaction_id,
  applied_ops, forgotten{ids | range}, tokens_before, tokens_after,
  …}}`; `context.artefact.delivered`; idempotency keys (ADR-0031);
  provider-side compaction's `forgotten = {range: all_prior}` fallback.
- §05h `MetricDeclaration`; §06.4 analysis engine; §10.5.1 the
  `lab/compaction-family-v1` exemplar table.
- Inspected anchors: `hh-eval/src/compare.rs` ~365–367 the
  `replicate_reducer` is applied per task; §4.2's exact window
  `(completion_seq(c), subject_execution_stop_seq]` — including later
  compactions and overlapping `(c,event)` joins — plus `(c,event,item)`/
  `(c,effect)` join retention are **not yet** the declared detector
  contract: the §9.3 row's "overlapping completion→execution-stop
  joins, repeated deliveries/idempotency keys, denominators/coverage/
  unbounded intensity" is the proposed change.

**Proposed after (candidates — unadmitted).**

1. `detector_join_contract∗` — per completed compaction c the subject
   window is `(completion_seq(c), subject_execution_stop_seq]`,
   including later compactions; retain `(c,event,item)` joins —
   reacquisition = matching post-c deliveries / fs or memory reads ÷
   forgotten items, repeated deliveries count so intensity may exceed
   one (never probability/ppm-clamped) — and `(c,effect)` joins with
   idempotency keys — repeated action = post-c effects matching pre-c
   keys ÷ all post-c effects; argument/capability similarity stays a
   distinctly named approximation.
2. `detector_declaration∗` — versioned registry names, native
   units/bounds/direction, detector/extractor IDs, windows and reducers
   declared **before collection**; absent extractor `no_detector`;
   zero denominator `estimator_undefined`; missing ID/key `no_detector`;
   a required unavailable detector blocks its claims and stays in the
   frozen multiplicity family.
3. `coverage_reducer∗` — sum numerator/denominator within subject,
   divide, average available subject ratios within task, then
   equal-task contrast; coverage and missingness disclosed; never pool
   events as trials or weight tasks by opportunities.
4. `analysis_family∗` — the complete frozen primary/stratum family
   recorded per report **including unavailable members**; actual
   estimator, version, target, assumptions, prior, keys, seed, draws,
   floors, refusal and interval `{kind, level}` recorded per analysis
   (REQ-D-008/109, REQ-E-209); uncertainty labels per S.6.
5. Activation semantics — starts/eligible pressure opportunities,
   completions/starts, triggered first subjects/all first subjects;
   ineffective/fallback counts with fallback an assigned-arm deviation
   (never a relabelled arm); untriggered subjects remain in task
   success — activated-only analysis is exploratory post-treatment
   selection; association with omission is never causal-forgetting
   proof.

### G.2 Owner map

| Seam | Owner (crate) | Durable fact |
|---|---|---|
| detector predicates + catalogue | hh-eval | `(c,event,item)`/`(c,effect)` join facts |
| compaction window facts (consumed, unchanged) | hh-context | `CompactionRecord`/delivery events |
| versioned registry declarations | hh-lab | detector/extractor ids, units, reducers |
| families + analysis metadata | hh-analysis | `analysis_family∗` on `AnalysisRecord` |

### G.3 Durable payload / home / class candidates (all `∗`)

- `detector_join∗{compaction_id, window{from_seq, to_seq},
  joins[{event_ref, item_ref}], numerator, denominator, coverage}` —
  join facts on existing measurement families (no second store);
  `analysis_family∗{family_id, members[{claim, status ∈ {reported,
  unavailable, refused}}], correction{holm|bh}}` on `AnalysisRecord`.

### G.4 Authority

- Detectors compute from ledger evidence only — no grading authority is
  inferred; a missing/unknown required declaration refuses or yields
  `no_detector`, never a silent zero.

### G.5 Recovery

- Joins recompute deterministically from durable evidence;
  later-compaction overlaps re-join identically; no mutable detector
  state.

### G.6 Tier / negotiation

- Unknown detector/extractor version → `no_detector`/typed refusal; no
  new tier.

### G.7 Compatibility and preserved legacy

- Old approximation names are retained; six outcome classes, veto axes
  and the `n/a{reason}` vocabulary unchanged; no count-to-rate
  relabelling; grading-time events never enter the subject window.

### G.8 Dependency edges and owed predicates

- REQ-D-003 → R5.15, REQ-D-005/106 → R5.36, REQ-D-008 → R5.37,
  REQ-D-109 → R5.40 are the primary owners; R5.20 (wire) and R5.49
  (study report) predicates remain owed — recorded, not discharged.
- Feeds Packets D/F inputs (population + metadata contract, S.1/S.4);
  no Packet A/B/C seam is redefined.

### G.9 Acceptance mapping (AC-3-1 / AC-4-1 / AC-4-2 → numbered checks)

1. Golden traces: overlapping compactions, repeated deliveries,
   matching/nonmatching idempotency keys, missing keys/extractors,
   zero opportunities, no event, fallback/ineffective/impossible
   outcomes, a next-dispatch join.
2. Subject/task reducers and every reported denominator recomputed from
   native evidence; old approximation names retained.
3. Unknown/veto/short-cell denominators unchanged — the AC-3-1 D1
   counterexample bound `[−0.4, 0.5]` stands.
4. Missing-key/missing-detector tests; complete frozen family incl.
   unavailable members; actual draws/seeds routed and recorded.

### G.10 Alternatives rejected

- Probability/ppm clamp on intensity — rejected (unbounded).
- Count-to-rate relabelling — rejected.
- Pooling events as trials / task weighting by opportunities —
  rejected.
- Dropping unavailable members from the family — rejected.
- Grading-time events inside the subject window — rejected.

## Non-admission ledger (AC-2 negative space — all unadmitted)

- **Wire/domain/event names:** every `∗` name is a candidate; none is
  registered, allocated or emitted by this dossier.
- **Public adoption RPC:** none proposed — `adopt_subject∗` is an
  in-process owner interface; proposed names are design seams, not public
  RPC promises; Rust adoption promises no RPC.
- **Alternate codec/store:** none — payloads reuse canonical JSON/identity,
  ledger classes and the existing class table.
- **Duplicated lifecycle/totals:** expressly rejected (A.10/B.10).
- **Inferred grading authority:** authority is a required recorded field
  (A.4); inference is unadmitted.
- **New backend/dependency/public boundary:** none; HelperExecutor/Podman
  feasibility stays a separately reviewed P2 prerequisite.
- **Stand-alone receipt authority:** none proposed — `evidence-package/1`∗
  is a derived index inside `hh-bundle/1`; producers stay authoritative.
- **Reverse dependency to bench/results:** none — producer export
  references bind to lower owners by typed enumeration; hh-bundle's
  manifest edge set is unchanged.
- **Hash-regex reference discovery:** none — owning codecs enumerate
  payload refs; no byte scanning, no second class table.
- **Legacy byte rewrite / implied public closure:** none — historical
  receipts/members stay exact bytes (build-note label is a view
  annotation); closure verdicts are recipient-class/time-scoped.
- **Semantics in source:** no crate, schema or generated file is touched;
  implementing edits wait for GATE-G9.

- **New fitter/prior/method beyond the four rows:** none proposed —
  `factorial_glmm`, hierarchical/task-population and correlated-replicate
  models stay unadmitted; `Beta(2,2)` is exploratory sensitivity only.
- **CP fallback:** none — `kl_directional_bound∗` is concentration, not
  CP/binomial inversion; no post-outcome CP substitution.
- **Credible-as-TOST / mixed uncertainty transfer:** barred — a
  Bayesian-efficacy + frequentist-calibration claim is no joint 95%
  certificate (§4.5); `credible{prior}` is never relabelled.
- **Guessed endpoint/margin/seed/tolerance:** none — unspecified
  precision/encoding blocks dependent implementation (S.2, AC-2);
  margins/δ/κ stay sponsor-signed inputs; the provisional δ=.05 / h=.05 /
  half-width .10 are not ratified.
- **Post-observation tolerance or N:** barred — tolerances, N and the
  simulation battery freeze before any numerical execution; no peeking
  or top-ups.
- **Coupling inflation:** none — `parity_pair_grouping∗` grants no
  CRN/honoured-seed/R2/causal-pairing claim (E.1).
- **Intensity clamp / event pooling:** none — repeated deliveries count
  and intensity may exceed one (G.1); no count-to-rate relabelling, no
  dropped unavailable family members.
- **Numerical execution:** none — parameters/seeds/envelope are cited,
  not run; the D2 tranche (≤8 h, 4 cores/8 GiB, 32 core-hours, 2 GiB) is
  not authorized by this dossier; $0 stands.

## Admission evidence GATE-G9 will need

- Operator/authorized-maintainer disposition of all seven rows and the
  exact proposed ADRs — encoding, correction authority, compatibility and
  numerical protocol (GATE-G9 pre-registered thresholds).
- Per packet: named owner acceptance route (maintainer), the numbered
  acceptance map above, revisit triggers, and the slot-status audit that
  all three sibling packets returned.
