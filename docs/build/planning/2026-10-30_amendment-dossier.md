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
  [§2](../../evidence-and-adoption-spec.md#s2) requirements, canonical spec
  §06.3.2.2–2.4 / §06.5 / §05h.2.3–2.4 / §§05a/05b/05c/05e / §08.2, research
  ADR-0155/0161/0162, [14_integration-design.md](../../design/14_integration-design.md)
  F1–F5 (retrieved 2026-10-29T20:53:30Z; anchors re-confirmed 2026-10-30).

## Seven-row amendment register

| # | §9.3 row (scope / exact source sections) | Owning requirements | Packet owner | Packet | Status |
|---|---|---|---|---|---|
| 1 | Launch/required grade/settlement: §06.3.2.2–2.4 (R-2.10.3), §06.5 overlays (R-2.10.5), §05h.2.3–2.4; ADR-0155/0161/0162 | REQ-E-201–204; experiment/embed/bench/eval/results | R5.3 | [Packet A](#packet-a) | **prepared** (this ticket) |
| 2 | Effects/prepared calls/derived admission: §§05a/05b/05c/05e and §8.2 | REQ-E-205–207; env/context/gateway/budget/control | R5.3 | [Packet B](#packet-b) | **prepared** (this ticket) |
| 3 | Native closure/profile: §05h.3 R-2.9.3/§10.4 and ADR-0363 collector linking | REQ-F-001–008, REQ-E-208; bundle/producer/ontology | R5.4 | [Packet C](#packet-c) | **prepared** (this ticket) |
| 4 | Finite-task efficacy: §10.1 reporting, §05h.2/R-2.10.4, ADR-0158 D1–D3/D8–D9 | REQ-D-107/109, REQ-E-209; eval math/lab metadata/analysis | R5.5 | slot | **owed** to R5.5 |
| 5 | Statistical parity grouping: §10.1 replicate factor, §05h.2.1 compare, ADR-0046 CF-095 | REQ-D-302/310; bench/lab/eval | R5.5 | slot | **owed** to R5.5 |
| 6 | Finite-manifest equivalence: §10.1 estimator/multiplicity, §10.2 equivalence, §05h.2.1, ADR-0158 D2/D7/D9 | REQ-D-303–307/310; eval/lab/analysis/bench | R5.5 | slot | **owed** to R5.5 |
| 7 | Detectors/analysis metadata: ADR-0077 D4, §§05c/05h/06.4/10.5.1 | REQ-D-003/005/008/106/109; eval/context/lab/analysis | R5.5 | slot | **owed** to R5.5 |

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

## Admission evidence GATE-G9 will need

- Operator/authorized-maintainer disposition of all seven rows and the
  exact proposed ADRs — encoding, correction authority, compatibility and
  numerical protocol (GATE-G9 pre-registered thresholds).
- Per packet: named owner acceptance route (maintainer), the numbered
  acceptance map above, revisit triggers, and the slot-status audit that
  all three sibling packets returned.
