# ADR-0211 deferral-set briefs — BL-34 package (R4.10, 2026-10-22)

<!-- Brief-file convention OWNED BY R4.8 (docs/build/planning/round4-bl33-ws-l6-briefs.md;
     ticket 157): this file reuses its table columns, adopt-all-except line and the four
     disposition tokens verbatim. This ticket never rules — `recording-act` rows only cite
     an operator ruling already in LEDGER GATE DECISIONS or a landed ADR; everything else
     is a proposal for GATE-G7. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it — cite
tests/ADRs) · `adopt-interim-as-final` (the interim rule in force becomes the ratified
answer — quoted) · `descope-by-ADR` (permanently out of the build — ADR text + authorising
operator words named) · `needs-code-leg(<row>)` (post-GATE-G7 conform row + the exact
change). Every entry test below was read verbatim from ADR-0211's Decision table and
checked against landed state (base 9982e8d); all six fail — and the R4.10 contract names a
deferral whose entry test fails a natural `descope-by-ADR`: permanently out of launch
scope, the re-entry test preserved as the future trigger. That is a final disposition, not
a carry.

## Batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| D-1 | descope-by-ADR | entry test unmet — no `PrincipalDirectory` class contract proposed by any ADR; the landed mechanism is owner-issued `ApproverGrant`s + static `EscalationTarget`s | R4.15a |
| D-2 | descope-by-ADR | entry test unmet — no tenant `readers` partition and no sharding/migration ruling; the landed shape is one fleet per definition + source set under single-principal sessions | R4.15a |
| D-3 | descope-by-ADR | entry test unmet — the `ext.effects.external_irreversible` counter + owner-pool ceiling landed, but no pricing rule with debt record + `ComparisonReport{benefit_kind = artifact_benefit}` exists | R4.15a |
| D-4 | descope-by-ADR | entry test unmet — no `compute_policy` variant binds `SchedulingPolicy.order`; the reconciler's ratified (deterministic) order is the landed dispatch order; no `matched_total` evaluation, no adopting `ComparisonReport` | R4.15a |
| D-5 | descope-by-ADR | entry test unmet — the `work_source_adapter` ABI is read-only (`capabilities`/`occurrences`/`suspended`/`activate_run`/`list`/`get`); no mirroring procedure or principal-issued writer exists | R4.15a |
| D-6 | descope-by-ADR | entry test unmet — no `OwnershipRecord` outlives an activation (ownership stays per coordination object within one activation, ADR-0191); `lab/coordination-topology-v1` landed intra-run only; OQ-431 escrow unresolved | R4.15a |
| OQ-461 | adopt-interim-as-final | the interim rule in force — "no (C4/Stage 4 default: one fleet per definition + source set)" — is the landed FleetSpec shape; proposed as the ratified answer (no work-item migration) | R4.15a |
| OQ-462 | descope-by-ADR | the question exists solely as D-1's entry test; it is moot under the D-1 descope (the same ADR text covers it) | R4.15a |
| OQ-463 | resolve-with-evidence | mechanical consumer count = 1 (the hh-fleet ceiling; hh-ontology's row is the registration, not a consumer) — the ADR-0211 "two consumers" promotion trigger is unmet, `ext.*` stands per ADR-0208 §D + CF-445 | R4.15a |

## Entry-test readings (deliverable 1) — verbatim test vs landed state

**D-1 — Human directory and rotations.** Entry test (ADR-0211 Decision, verbatim): "a
`PrincipalDirectory` component-class contract proposed by ADR with two out-of-process
precedents; the adapter never mints `principal` authority; rotation resolution time
(`raise` vs `after_ms`) fixed in the same ADR (OQ-462; joins OQ-446 on `PrincipalRef` vs
`TeamRef`)".
- Landed evidence: `grep -rn "PrincipalDirectory"` over `crates docs research spec` hits
  only research/spec text (ADR-0207 D7, ADR-0211, the WS-L8 dossier, spec §11.3/§05i) — no
  component-class contract exists, no ADR proposes one. The interim rule is landed:
  `crates/hh-fleet/src/lib.rs` doc header "no live directory (the `ApproverGrant`/static-
  `EscalationTarget` substitutes are documented under D-1/D-2)"; `engine.rs:101-104`
  `TRANSFER_BASES = ["approval", "policy_rule"]` with `escalation_ref` rows
  (`engine.rs:1916-2080`); the adapter never mints authority — the `work_source_adapter`
  ABI is read-only (`hh-fleet-adapter/src/abi.rs:18-24`).
- **FAIL.** No `PrincipalDirectory` contract has been proposed by any ADR; the precedent
  and rotation-time sub-clauses never arise.

**D-2 — Multi-tenant fleets.** Entry test (verbatim): "a tenant boundary expressible as a
`readers` partition (ADR-0033) with no new authority class and no change to
`CallerBinding`; fleet sharding and item migration ruled in the same ADR (OQ-461)".
- Landed evidence: `readers` exists only as the fleet-run annotation-row field
  (`hh-fleet/src/payloads.rs:535`, `engine.rs:2359 annotate` — ledger envelope metadata,
  not a tenant partition). One `FleetSpec` scopes one activation's declared agent set —
  `agents` resolution refuses `owner_unknown`/`cross_fleet` (`spec.rs:100-105`); sessions
  stay single-principal (ADR-0174 D6). No tenant boundary, no sharding or item-migration
  ruling anywhere.
- **FAIL.** The boundary and the same-ADR sharding/migration ruling do not exist.

**D-3 — Value-at-risk pricing of irreversibility.** Entry test (verbatim): "a pricing
rule with an assumption-debt record and a matched-budget `ComparisonReport{benefit_kind =
artifact_benefit}` against the unit counter under `lab/org-policy-v1`; a second consumer
named at the same time promotes the dimension by ADR-0039 dialect bump".
- Landed evidence: the counter itself is landed — `ext.effects.external_irreversible`
  registered in `hh-ontology/src/dimensions.rs:76,115`; the `WorkItem` dispatch class
  declares it (`hh-fleet/src/work_item.rs:35-41`) and the engine enforces the optional
  ceiling at the owner pool (`engine.rs:1301-1361`; `errors.rs:172` typed bound). The
  `lab/org-policy-v1` recipe exists (`hh-lab/src/exemplars.rs:4210 org_policy_v1`;
  `lab.org_policy.recipe` op, `hh-embed/src/coevolution_ops.rs:784`) — but no pricing
  rule, no assumption-debt record for a pricing hypothesis, and no `ComparisonReport`
  against the unit counter has been produced. Mechanical consumer count:
  `grep -rln "external_irreversible" crates/*/src` → `hh-fleet` (declaration + ceiling
  enforcement) and `hh-ontology` (the registration site — a declaration, not a consumer):
  **one consumer**, so the promotion half is unmet as well.
- **FAIL.** The pricing comparison — the entry test's substance — was never run.

**D-4 — Dispatch prioritization by expected value.** Entry test (verbatim): "a
`compute_policy` variant that binds `SchedulingPolicy.order` at the ratified
`delegate`/`dispatch` seam without becoming a second owner of choice (ADR-0188 D1;
ADR-0208 §B), evaluated by a `lab/value-of-compute-v1`-shaped recipe at `matched_total`
with `static` order as the null arm and `TaskValue` (ADR-0188 D8) as the value input;
adoption only by a `ComparisonReport` at the pre-registered margin".
- Landed evidence: `compute_policy` variants `static`/`uniform`/`rules` are landed
  (`hh-control/src/compute.rs:1425-1427`; the registry class doc parks
  `bandit`/`surface_prior`/`predictor` at Stage 5/6) and `TaskValue` exists
  (`compute.rs:426-548` `TaskValueFact`). But `SchedulingPolicy.order` is not landed in
  the fleet — `FleetSpec` carries no scheduling member (`spec.rs:84-140`); dispatch order
  is the reconciler's ratified deterministic fold (`engine.rs:1061` `items.sort_by` —
  "dispatch order is the reconciler's ratified order", ADR-0211's own interim). No
  `lab/value-of-compute-v1` arm evaluates a fleet-order variant; no `ComparisonReport`
  adopts one.
- **FAIL.** Neither the binding variant nor its matched-budget evaluation exists.

**D-5 — Reconciler-side source mirroring.** Entry test (verbatim): "AC-L8-06 passing with
the mirroring procedure's effects recorded as ordinary `action.effect.*` rows under a
principal-issued `Permission` (ADR-0053 D-5 forbids a principal-less writer)".
- Landed evidence: the accountability record the AC reads is landed
  (`hh-fleet/src/engine.rs:2949-3014` `accountability_record`; exercised in
  `hh-fleet/tests/fleet.rs:459-465`). But there is nothing for the test to pass *over*:
  the `work_source_adapter` op set is read-only — `OPS =
  {capabilities, occurrences, suspended, activate_run, list, get}`
  (`hh-fleet-adapter/src/abi.rs:18-24`; `TrackerLogic`, `tracker.rs:23-33`, has no write
  method); the `mirror` member of `binding.role` (`payloads.rs:278`) is a read-binding
  label, not a write-back procedure. No mirroring procedure, no principal-issued
  `Permission` writer, no `action.effect.*` rows to record.
- **FAIL.** The procedure the test conditions on does not exist.

**D-6 — Cross-item shared-state coordination.** Entry test (verbatim): "an
`OwnershipRecord` whose owner may outlive an activation, conferred and fenced through the
existing `resource(key)` lease scope (ADR-0130; ADR-0193 D3) with escrow expiry ruled by
the OQ-431 ADR, and a `lab/coordination-topology-v1` arm at `matched_total` showing
`lost_write_count = 0` and no `silent_overwrite` across activations (ADR-0192 vetoes); no
new authority class and no change to `spawn`".
- Landed evidence: `lab/coordination-topology-v1` is landed with the veto metrics —
  `hh-orchestrator/src/coordination.rs:104-119` (`lost_write_count`,
  `conflicts_open_at_completion`), `tests/s4_8.rs`, `hh-lab` exemplars — over *intra-run*
  orchestrator topologies. Cross-activation ownership does not exist: ownership is per
  coordination object within one activation (ADR-0191 D1; `hh-fleet/src/ownership.rs` has
  no outliving-activation owner; grep for `outlive`/`across activation`/`inter-activation`
  over `hh-fleet`/`hh-subagent` is empty), and the OQ-431 escrow ruling the test requires
  stays `deferred(ADR-0215)`.
- **FAIL.** The outliving-activation `OwnershipRecord` — the test's subject — does not
  exist.

## OQ cells (deliverable 2)

- **OQ-461** (fleet sharding/migration). Cell reads `open — deferral candidate · Phase 5:
  deferred(ADR-0211)` with interim "no (C4/Stage 4 default: one fleet per definition +
  source set)". The landed build is exactly the interim: one `FleetSpec` per
  definition + source binding, item `owner`s resolving inside one declared agent set
  (`cross_fleet` refused at `set_owner`), no migration path. **Proposed
  `adopt-interim-as-final`** — quote the interim rule verbatim as the ratified answer:
  *"no — one fleet per definition + source set; a work item does not migrate between
  fleets."* Consistent with the D-2 descope: multi-tenant machinery never lands, so the
  sharding/migration question resolves to the landed default; a future D-2 re-entry ADR
  would amend it.
- **OQ-462** (`PrincipalDirectory` contract). Cell reads `open — deferral D-1 (ADR-0207
  D7) · Phase 5: deferred(ADR-0211)`. The question exists solely as D-1's entry test; its
  substance (assignee → `PrincipalRef` mapping, rotation resolution time) presupposes a
  directory the D-1 descope removes from launch. **Proposed `descope-by-ADR`** — mooted by
  the same ADR text that descopes D-1; the question is preserved verbatim inside D-1's
  re-entry test.
- **OQ-463** (`ext.effects.external_irreversible` kernel dimension vs `ext.*`). Cell
  already reads `resolved (ADR-0208 §D; CF-445): registered ext.effects.external_irreversible
  until D-3 closes; promotion to the kernel list at Phase 5 if two consumers exist ·
  Phase 5: deferred(ADR-0211)`. The promotion condition is mechanically checkable:
  `grep -rln "external_irreversible" crates/*/src` yields `hh-fleet` (the one consumer —
  item declaration + owner-pool ceiling) and `hh-ontology` (the dimension's registration
  site — a declaration, not a consumer). **Count = 1 < 2 → the trigger cannot fire.**
  **Proposed `resolve-with-evidence`** — the landed evidence plus the ratified rule answer
  the question: `ext.effects.external_irreversible` stays a registered `ext.*` dimension
  (ADR-0208 §D; CF-445; ADR-0211); promotion would need a second consumer named by ADR.

## Proposals — the descope ADR text + authorising words (deliverable 3)

One ADR text covers D-1…D-6 and moots OQ-462; proposed shape for the R4.15a conform to
write once adopted:

- **ADR text (proposed):** a `docs/adr/` build ADR (next free number at write time) —
  *"The ADR-0211 deferral set (D-1…D-6) descoped at launch — every entry test evaluated
  and unmet at the R4.10 readings; each ratified interim rule stands as the launch
  mechanism (`ApproverGrant`s/static `EscalationTarget`s; one fleet per
  definition + source set; the `ext.effects.external_irreversible` counter + optional
  ceiling; the reconciler's ratified dispatch order; mediated read-only tracker ingress;
  per-activation ownership); every entry test is preserved verbatim as the re-entry
  trigger — re-entry is a new ADR passing its ADR-0211 test. OQ-462 is mooted by this
  act; OQ-461's answer is ratified as the interim default; OQ-463 is recorded resolved
  (`ext.*` — one consumer)."* Plus: an appended dated note on
  `research/decisions/ADR-0211.md` (executed contract — amend by append only) naming the
  descope ADR, the spec-debt/open-questions register flips, and any §11.3/§11.4 note per
  the manifest's amendment protocol.
- **Authorising operator words (proposed):** a GATE-G7 answer in the adopt/amend/decline
  row — e.g. `"descope the ADR-0211 set — D-1…D-6 permanently out of launch"` — or the
  batch act `"adopt all"` on this file's table.
- **Conform row:** `R4.15a` (164b — "Adopted-ruling records conform —
  ADR-0210/0211/0212/0213/0216 packages") for all nine rows: the descope ADR + the dated
  ADR-0211 note + register status flips + dated evidence cells are records work; nothing
  here is a `needs-code-leg` (no adopted row requires code).

## What this brief does not decide

No register status changes, no ADR text, and no spec amendment land here — every cell
above is a proposal the operator adopts, amends or declines at GATE-G7 (the recording is
R4.15a's). The interim rules stay in force unchanged until then; the D-1…D-6 re-entry
tests are never deleted. BL-34's flip is R4.16's accounting after GATE-G7 adoption and
the conforms land.
