# ADR-0215 package briefs — BL-38 (R4.14, 2026-10-22): 15 program-level OQs + the 24 READINESS_REPORT §10 minors

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
change). Every ADR-0215 interim rule below is quoted verbatim from its Decision table;
every landed-evidence citation was re-checked at base `4ddc130`. Conform rows: `R4.15b`
(the 164e skeleton names "BL-37/BL-38 … the §10 minors' adopted dispositions" — it is this
package's records row; the BL-37 briefs' `R4.15a` cells are naming drift the GATE-G7
fleshing reconciles) and `R4.15c` for code legs (none needed — no OQ leg is unimplemented
code). Every ADR-0215 entry test stays preserved verbatim as the re-entry trigger.

## Batch-adoptable table

### Part A — the 15 BL-38 OQs (ADR-0215 program-level deferrals)

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-292 | adopt-interim-as-final | `route`/`effort` decisions happen at the bind step on `propose` (ADR-0188/0121); `DecisionPoint::ALL` has no `route` member (`hh-hir/src/schema.rs:954-963`); the model-facing escalation half rides the landed `ContextItem`/`act` + `delivered`/`activated`/`followed` chain (T-LCD-13); CF-107's dialect bump does not land | R4.15b |
| OQ-422 | adopt-interim-as-final | a Certaindex-style forced completion classifies as `Validator{kind: judge}` under ADR-0116/0117 (`OracleClass::Judge` → `Detector::Judged`, `ModelRole::Judge`, `Purpose::Judge` landed); a `router_predictor` *model-call class* does not exist — `predictor` is a control-strategy variant registered-but-refused at bind (`hh-control/src/compute.rs`) | R4.15b |
| OQ-423 | adopt-interim-as-final | within-campaign allocation is the landed `SlotAllocation{uniform, fractional_design, concentrate, voi}` (`hh-evolution/src/records.rs:25-116`); no cross-design/portfolio allocator exists — each design carries its own budget node | R4.15b |
| OQ-425 | adopt-interim-as-final | the OQ-292-joint interim verbatim — no `route`/`effort` `DecisionPoint` in HIR (`schema.rs:952-962`); effort/role selection is the landed bind-step `effort_level` option over profile `effort` capabilities (`hh-control/src/compute.rs`); the entry test's artifacts land but show no measured distinct-point benefit | R4.15b |
| OQ-429 | adopt-interim-as-final | `ResourceKey.custom(ns, bytes)` namespaces stay unregistered; `collision_policy = "refuse"` is pinned "the only value at this stage" (`hh-registry/src/records.rs:473-503`); `ResourceKey` rides `OwnedObject`/`ScopeKind` as the `resource(key)` lease scope | R4.15b |
| OQ-431 | adopt-interim-as-final | escrow held on a grantor's failure — the landed `return_to_grantor`-eligible fold keeps `grant_ref` so the grantor chain is auditable (`hh-monitor/src/ownership.rs:75-95`); the item owner is the fallback responder (ADR-0207 O-5); no expiry/inheritance machinery exists — records never expire | R4.15b |
| OQ-437 | adopt-interim-as-final | the CF-421 ruling is landed: `shadow` commits unconditionally; `split` commits only under a declared `RolloutPolicy{mode:split}` with deterministic `assignment_seed` + share floor, split-arm rows `comparable:false`/`exploratory:true` (`hh-evolution/src/campaign.rs:2643-2712`); no `live_split` kind | R4.15b |
| OQ-439 | adopt-interim-as-final | `SelfModificationRefused` landed and enforced (`hh-evolution/src/errors.rs:82`; `propose_refuses_self_modification` in `tests/s6_1a.rs:536-572`, `s6_2.rs:933-960`) — the service's own definition stays outside every target set | R4.15b |
| OQ-440 | resolve-with-evidence | the readiness report fixed the test (`spec/READINESS_REPORT.md` §7 — quoted verbatim below): replication across ≥2 task families/≥1 held-out level/≥2 model families + `outcome_bounds.verdict = robust`; landed evidence: `ahe` alone is `instrument-grade`, research families forced `research-grade` (`records.rs:583-592`; `proposer_conformance.rs:255-261`) — **no landed campaign satisfies the test → zero promotions** | R4.15b |
| OQ-443 | adopt-interim-as-final | `RemovalVerdict{debt_ref, kind, verdict, reason, report_ref, settled_at}` is per rule — no coalition field (`hh-ontology/src/debt.rs:824-870`); M3 Shapley credit exists in `hh-analysis` but never feeds a verdict; `retire`/`propose` read the settled-verdicts list | R4.15b |
| OQ-449 | resolve-with-evidence | landed answer: `provider_sampling` may join `coupled_sources[]` only declared+measured (`CoupledSource{source, coupling_agreement}` — `attribution.rs:364-378`; unmeasured ⇒ `CouplingUnavailable` :809-820); the floor is `coupling_agreement = true`; `seed_honoured` is declared (`plan.rs:335`) and the serving-stack fingerprint probe lands (`snapshot.rs`); M5 and judged outcomes stay `exploratory` (:1356) | R4.15b |
| OQ-451 | adopt-interim-as-final | the interim verbatim landed: `outcome_oracle_class = judge` requires `calibration_ref` and forces `exploratory` (`attribution.rs:436,1356`); `JudgedCellLabel` carries `judged` labels and exploratory cells are report-only, never headline/gate inputs (`hh-lab/src/analysis.rs:1383-1447`) | R4.15b |
| OQ-456 | adopt-interim-as-final | no reward subscription exists in `hh-embed/1` — the only admitted route stays an `experimental` item with a debt record + `zero_uses` removal test (ADR-0178 D4/ADR-0202 D4); Group R reads land unchanged | R4.15b |
| OQ-459 | resolve-with-evidence | landed answer is the stricter first cut: `ADMISSIBLE_REWARD_SOURCES = {oracle_metric, oracle_verdict, metric_value}` — oracle classes only; `{critic_verdict, reward_model_output, participant_reported}` refuse `InadmissibleRewardSource`, never dropped (`hh-lab/src/coevolution.rs:274-292, 750-762`); judged rewards never export until an ADR widens the set | R4.15b |
| OQ-466 | adopt-interim-as-final | the interim verbatim landed: `lab/org-policy-v1` declares `attendance` as `role: responder_stratum`; a simulated responder is an instrument model named by `simulated_responder_calibration_ref` (instrument-charged; `exemplars.rs:4113-4200`); `human_wait_ms_per_item` is a declared primary — `n/a` only where a design says nothing | R4.15b |

### Part B — the 24 READINESS_REPORT §10 minors

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| C-11 | adopt-interim-as-final | §5a.4's `provenance.origin ∈ {runtime, lab, human, evolution}` stands as the BranchRecord's own creator-kind sum — a distinct field, never a §8.1 `Origin` (the landed `Origin` sum — human/model/tool/kernel/participant/import/evolution/cache — is the provenance record's field; `hh-provenance/src/origin.rs`) | R4.15b |
| C-16 | resolve-with-evidence | landed: `replay.driver_mode ∈ {reconstruct, deterministic}` (`ReplayDriverMode::parse`, `replay_ops.rs:38-69`) vs `fork.replay_mode ∈ {inherited, exact, structural, observational, none}` vs attribution `replay_mode_requested ∈ {re_executed, deterministic}` — three distinct fields on three surfaces; no contract conflict | R4.15b |
| C-17 | resolve-with-evidence | the landed `ClassRecord.cardinality` is the §3.3 closed sum (`Cardinality` enum, ADR-0023 — `hh-registry/src/kinds.rs:485`); §5h.4's `one-per-suite`/`locality` reads as a descriptive scoping note over it, never a second schema | R4.15b |
| C-20 | adopt-interim-as-final | the two `SchedulingPolicy` records stay distinct: §6.3's landed record is the experiment engine's (`hh-experiment` `SchedulingPolicy`/pools); §5i.1's is the fleet scheduling surface — the fleet carries no `SchedulingPolicy` member; dispatch order is the reconciler's ratified fold (consistent with R4.10's D-4 reading) | R4.15b |
| C-21 | resolve-with-evidence | the landed class table homes `control.work_item.annotated` under the `control` namespace with its sibling work-item lifecycle classes (`hh-ledger/src/classes.rs:1306-1315`) — the registration is the home-plane answer; fleet-plane domain ownership is the record's subject, not the event class's home | R4.15b |
| C-26 | needs-code-leg(R4.15b) | no `hook_point` is implemented anywhere — the §7.4 record's `hook_point: DecisionPoint` member is wrong-typed; the leg retypes it to a named hook-site kind (never a `DecisionPoint` member) at the records conform | R4.15b |
| C-27 | resolve-with-evidence | `defer` IS a landed approval response — Π-12's `UnattendedPolicy` is `deny | defer | auto_review` and the hook/ask projection is `{allow, deny, ask, defer}` (`hh-monitor/src/approval.rs:676, 810, 948-951` — defer suspends via `lifecycle.run.suspended{awaiting_approval}`); `lab/org-policy-v1` declares `unattended_policy` levels `deny`/`defer`/`auto_review`; ADR-0070's B3 suspend/wakeup wording stands | R4.15b |
| DAG-10 | adopt-interim-as-final | the in-force convention ratified: a parenthesised "(Stage-5 rules)" tag on a §9 row annotates *rule content* landing at that stage while the item proper lands at its §4.4 node — the §9.0 grammar note records the content-vs-node convention | R4.15b |
| DAG-11 | descope-by-ADR | inlining `synthesis/l1-spike-spec.md` §5's score bands into §10.6 is permanently out — the by-reference citation stands (a synthesis artifact never inlines into the frozen spec) | R4.15b |
| DAG-12 | needs-code-leg(R4.15b) | the owed stage-note ADR (OQ-410) lands at the conform: `R-2.8.4²` and `R-2.12.2²` each staged per their owning sections' §4.4 placement — the ADR names both nodes verbatim | R4.15b |
| TR-06 | resolve-with-evidence | maturity is per-family landed data: `ProposerDeclaration.maturity`; `ahe` alone is `instrument-grade` (`proposer.rs:623`), 6c research families forced `research-grade` (`records.rs:583-592`, `proposer_conformance.rs:255-261`) — the §9.7 6b cell conforms to the per-item flags | R4.15b |
| TR-07 | needs-code-leg(R4.15b) | §1.1's "event-sourced"/"instrument validity" attributions re-cited to the ADR-0003/0005 sections that actually carry them — by quote (the TR2-04 convention) | R4.15b |
| TR2-04 | needs-code-leg(R4.15b) | the citation convention: cite-by-quote, never "decision n" over unnumbered bullets — the four "ADR-0003/0010 decision n" references re-cited by quoted text at the conform | R4.15b |
| LCD-04 | adopt-interim-as-final | the endorsed `amend()` widening keeps its name — `amend` is the term of art; "escape hatch" stands as descriptive prose, never a contract term | R4.15b |
| LCD-05 | adopt-interim-as-final | the ratified ADR wording stands — "meta-harnessing" (ADR-0196) and "meta-harness document shape" (ADR-0150) are verbatim ADR terms, never renamed (the build's product surface is `harness`; the ADR terms are history) | R4.15b |
| LCD-06 | descope-by-ADR | inlining the report §6 test matrix into §1.6 is permanently out — §1.6's ADR-mapping stands; the matrix lives in the readiness report by reference | R4.15b |
| LCD-08 | descope-by-ADR | a native-`seal` rule for unregistered `ext` keys never lands — `ext` members stay declared foreign data, opaque to native seal (the hosting edge's lift/loss-list discipline is the contract) | R4.15b |
| LCD-R2-05 | resolve-with-evidence | landed: sampling members pass through the gateway as ordinary `model.call` data under provider spellings (`anthropic.rs:206`, `plan.rs` `Sampling`) — the contract statement is what the code does | R4.15b |
| r2 C-12 | needs-code-leg(R4.15b) | each of the ten wildcard `R-2.n.x` plane-shorthand dependency paragraphs gains its exact R-id list (expanded from the owning sections at the conform) | R4.15b |
| r2 C-16 | needs-code-leg(R4.15b) | §5b.3's `probe_profile` signature conformed to the landed `ConformanceRecord{participant_version_identity, adapter_version_id, dimension, declared, observed, verdict, observed_in, evidence_ref, seq}` (`hh-registry/src/records.rs:278`) — the landed shape is the authority | R4.15b |
| E2-05 | needs-code-leg(R4.15b) | the mis-citations re-lettered: "ADR-0208 §5" (maturity/conditionality) → §C (the evidence-bearing evolution loop); "ADR-0208 §4" (the R-2.1.2 IR row) → §B (the control seam) — verified against ADR-0208's §A–§F at the conform | R4.15b |
| E2-09 | descope-by-ADR | no separate glossing-convention document lands — the E1–E3/C6/S1–S4 short-ids disambiguate by context + §1's glossary; the convention is descoped, the overloads tolerated | R4.15b |
| E2-11 | needs-code-leg(R4.15b) | the AC-A2-2 symbol list corrected to the spellings the sections actually declare (the conform names the corrected list verbatim, verified against §3/§5 symbols) | R4.15b |
| E2-17 | descope-by-ADR | the §4.4 Core-column style mix is descoped — the tables are parsed as data; a style-uniformity pass never lands | R4.15b |

## Entry-test readings — verbatim ADR-0215 entry tests vs landed state

Every entry test was read verbatim from ADR-0215's Decision table and checked against
landed state at `4ddc130`. The defer-until-data column the ticket names: **the data does
not exist for any of the twelve interim-adopted rows** (the three `resolve-with-evidence`
rows are moot — landed evidence decides them directly). Verdicts:

- **OQ-292 / OQ-425** — entry test: "the HIR/2 backlog item with `derived-from.hypothesis_ref`
  (CF-416) and a `lab/value-of-compute-v1` arm showing a measured benefit of a distinct
  decision point". Artifacts land: `derived-from{hypothesis, trajectories, candidate_id?}`
  is a sealed HIR member (`hh-hir/src/records.rs:1215`, `schema.rs:2895`); the
  `lab/value-of-compute-v1` design exists (`exemplars.rs:2565`; the `compute_policy`
  factor spans `{static, uniform, rules, bandit}`). **Data: does not exist** — no arm
  shows a *measured* benefit of a distinct `route`/`effort` decision point (the design
  is registered, the campaign never ran). Interim stands → adopt-as-final.
- **OQ-422** — entry test: "WS-G3/C2 ratify the governance class with calibration and
  independence obligations for `predictor` variants (C4 via WS-I5)". **Data: does not
  exist** — `predictor` is registered-but-refused `PolicyInvalid{variant_not_admitted}`
  at bind (`compute.rs:1410-1442`); no Certaindex-style completion call exists for the
  rule to classify. The classification rule is vacuously in force → adopt-as-final.
- **OQ-423** — entry test: "a portfolio allocator over `adaptive_search` designs with one
  budget node, evaluated at `matched_total`". **Data: does not exist** — no cross-design
  allocator is landed (`grep` over `hh-evolution`/`hh-lab` for a portfolio/cross-design
  allocator is empty); within-campaign `SlotAllocation` is the only landed allocator.
- **OQ-429** — entry test: "a `NamespaceRecord` rule for resource-key namespaces ratified
  by WS-F5/E1/J2". **Data: does not exist** — no `NamespaceRecord` anywhere; namespaces
  stay unregistered under the pinned `collision_policy = refuse`.
- **OQ-431** — entry test: "WS-L8/B3/F5 ratify escrow expiry and inheritance; referenced
  by D-6 (ADR-0211)". **Data: does not exist** — the code comment keeps the escrow
  question explicitly deferred (`hh-monitor/src/ownership.rs:81-95`); the landed
  `return_to_grantor`-eligible fold is the only mechanism. (D-6 descoped at R4.10 — its
  reference to this row is noted, never a re-open.)
- **OQ-437** — entry test: "WS-J3/J4 propose `live_split` with an unpaired estimator over
  realised consumption and `budget_match`, admitted only if a KA fixture shows coverage
  at nominal level". **Data: does not exist** — no `live_split` kind, no unpaired
  estimator, no KA coverage fixture.
- **OQ-439** — entry test: "WS-H1/I5/L5 ratify the human-origin campaign with validators
  disjoint from every campaign suite and reviewer independence per ADR-0195 G7". **Data:
  does not exist** — `SelfModificationRefused` blocks the admission path itself; no
  self-referential campaign exists to ratify.
- **OQ-443** — entry test: "WS-I7/I6/J4 ratify pairwise designs admissible under the
  OQ-364 ceiling and the verdict's joint-removal form". **Data: does not exist** —
  `RemovalVerdict` is per-`debt_ref` with no joint form; coalition credit (M3) never
  feeds it.
- **OQ-449** — entry test: "WS-C1/C3/I7 ratify a gateway declaration and probe for
  `seed_honoured` and batch-invariant sampling with a `coupling_agreement` floor".
  **Data: partially landed, question resolved** — `seed_honoured` is a declared
  `ModelParams` member (`plan.rs:335`); the serving-stack fingerprint probe lands
  (`FINGERPRINT_PROBE_ID`, `snapshot.rs`); the floor is the landed `coupling_agreement =
  true` requirement (`attribution.rs:809-820`); M5/judged stay `exploratory`
  "pending OQ-449" (:1356). The *ratification* leg is GATE-G7's act itself — the
  mechanism answer is landed → `resolve-with-evidence`.
- **OQ-451** — entry test: "ratify a per-domain calibration protocol under which a judged
  outcome may be the metric of a `confirmatory` report". **Data: does not exist** — no
  per-domain calibration protocol; `judge` outcomes are forced `exploratory`.
- **OQ-456** — entry test: "WS-K4 shows the subscription is expressible over existing
  Group R reads or promotes the experimental item to `stable`". **Data: does not
  exist** — no reward-subscription op exists in either form.
- **OQ-459** — entry test: "WS-G3/I2 ratify deterministic-only vs calibrated-judged
  export from the first co-evolution cycle's regression-suite data". **Data: moot** —
  the landed export admits oracle classes only and refuses judged sources outright
  (`InadmissibleRewardSource`); the landed rule is stricter than either entry-test arm
  → `resolve-with-evidence`.
- **OQ-466** — entry test: "WS-J3/G3/L8 ratify the calibration set and independence axes
  (ADR-0116) for the responder simulator". **Data: does not exist** — the landed design
  names a `CalibrationRecord` ref per simulated arm but no calibration-set artefact is
  ratified; the interim is the in-force answer.
- **OQ-440** — entry test: "the readiness report fixes the promotion test (one campaign
  under M3 vs replication across ≥ 2 suites and ≥ 2 model families) and names who records
  the flag change; until it does, no family is promoted". **Run — the report fixed the
  test** (`READINESS_REPORT.md` §7, verbatim below); the promotion *evidence* does not
  exist → `resolve-with-evidence`.

## OQ-440 promotion-test verdict

The readiness report's fixed test (§7, verbatim): *"a `research-grade` proposer family is
promoted to `instrument-grade` only when (i) an `EvolutionAcceptanceReport` under S4
`matched_total` passes §10.2 gates 1–6 on the Stage-3 suite **and** (ii) the ADR-0159 D4
transfer test holds — sign-stable `artifact_benefit` across ≥ 2 task families and ≥ 1
held-out level, replicated on ≥ 2 model families … with `outcome_bounds.verdict =
robust`; (iii) the flag change is recorded as data on the registry
`VariantRecord.maturity` by the registry owner (WS-J2; ADR-0209 D3, §10.8 item 2) and
ratified by an amending ADR naming the campaigns (ADR-0198 D5) … A single campaign is
insufficient."*

**Verdict: no landed campaign evidence satisfies the test — zero promotions.** The
benchset is a recorded `model_io` corpus of one model generation; limb (ii)'s
≥2-model-family replication cannot be evidenced offline. The landed enforcement is
exactly the interim: `ahe` is the sole `instrument-grade` family (`proposer.rs:623`;
records.rs:617 "the 6b instrument-grade rule stays"), and every 6c research-family
campaign is forced to carry `research-grade` in `maturity_flags` (records.rs:583-592)
with the declaration's `maturity` checked at conformance
(`proposer_conformance.rs:240-261`). The flag change is `ProposerDeclaration.maturity` /
`VariantRecord.maturity` data + an amending ADR — the ratification path the report
named. Proposed `resolve-with-evidence`: the question (one campaign vs replication; who
records) is answered by the ratified report text + landed mechanics; the
`research-grade` wall stands until a future campaign satisfies limbs (i)+(ii) and an
amending ADR lands limb (iii).

## OQ cells (deliverable 1) — per-OQ basis

- **OQ-292** — interim (quoted, the shared OQ-425/292 row): "`route`/`effort` decisions
  happen at the bind step on `propose` (ADR-0188; ADR-0121); no `DecisionPoint` exists
  for them in HIR/1". Landed: `DecisionPoint::ALL` = {plan, act, retrieve, compact,
  verify, delegate, authorize, retry, stop, escalate} (`schema.rs:952-962`) — no `route`;
  `effort_level` is a `ComputeOptionKind` bound at `bind` over the profile's `effort`
  capability (`compute.rs:59-89, 411-413`); `Purpose`/`ContextItem.act` +
  `delivered`/`activated`/`followed` chain events carry the model-facing escalation
  request (T-LCD-13, landed). **Final answer (proposed):** no `route` DecisionPoint —
  model selection places at bind; a "request a stronger model" artefact is a
  `ContextItem` under `act`; CF-107's dialect bump does not land.
- **OQ-425** — same joint interim; HIR/2 backlog item named individually. Landed:
  bind-step effort/role selection as above; `lab/value-of-compute-v1` is registered but
  shows no measured distinct-point benefit. **Final answer (proposed):** the bind step
  on `propose` is the ratified placement — no `route`/`effort` `DecisionPoint`.
- **OQ-422** — interim (quoted): "Certaindex-style probe completions are
  `Validator{kind: judge}` calls under ADR-0116/0117 independence and calibration; a
  `router_predictor` class does not exist". Landed: `OracleClass::Judge` →
  `Detector::Judged` with `!is_c0_headline` (`eval.rs:95-152, 2884-2890, 3489`); judge
  calls spend on the instrument (`eval.rs:2062`); `Purpose::Judge`/`Purpose::Probe` in
  the gateway vocabulary (`vocab.rs:942-976`); hosted judges ride `judge_adapters.rs`.
  `predictor` is a refused control-strategy variant — a different seam, never a
  model-call class. **Final answer (proposed):** the classification rule verbatim.
- **OQ-423** — interim (quoted): "within-campaign allocation is `SlotAllocationPolicy`
  (ADR-0196 D4); cross-design evaluation budget has no allocator — each design carries
  its own budget node". Landed: `SlotAllocation` four-member sum per campaign
  (`records.rs:25-116`); no cross-design allocator. **Final answer (proposed):** the
  interim verbatim — cross-design stays unallocated; per-design budget nodes.
- **OQ-429** — interim (quoted): "`ResourceKey.custom(ns, bytes)` namespaces are
  unregistered; collisions refused at `register` by the registry's `collision_policy =
  refuse` (ADR-0191 D1; ADR-0153 D2)". Landed: `collision_policy` pinned `refuse` — "the
  only value at this stage" (`records.rs:473-503`); `resource_key` scopes `share`-mode
  leases (`hh-hir/src/tools.rs:143`); `OwnedObject::ResourceKey` is the ownership-table
  spelling. **Final answer (proposed):** the interim verbatim.
- **OQ-431** — interim (quoted): "escrow held on a grantor's failure; the item owner is
  the fallback responder (ADR-0193 D3; ADR-0207 O-5)". Landed: revocation makes records
  `return_to_grantor`-eligible and the record keeps `grant_ref` — the grantor chain is
  auditable (`hh-monitor/src/ownership.rs:75-95`; `hh-subagent/src/ownership.rs:127-135`
  `control.ownership.returned`). **Final answer (proposed):** the interim verbatim —
  held-for-grantor with item-owner fallback; no expiry/inheritance.
- **OQ-437** — interim (quoted): "`split` canaries are `exploratory` at 6a (ADR-0156 P4;
  CF-421); no `live_split` design kind". Landed: `campaign.rs:2643-2712` — `shadow`
  commits; `split` requires declared `RolloutPolicy{mode:split, assignment_seed,
  share_ppm}` (`SplitRolloutNotCommittable` otherwise) and marks rows
  `comparable:false`/`exploratory:true`. **Final answer (proposed):** the interim
  verbatim — `shadow` is the affordable default; `split` is declared-policy exploratory.
- **OQ-439** — interim (quoted): "the evolution service's own definition, proposer
  variants and campaign spec are outside every candidate's target set
  (`SelfModificationRefused`; ADR-0195 G5); a human-origin campaign over it is not
  admitted". Landed: `Refusal::SelfModificationRefused` + refusal tests (s6_1a:536-572,
  s6_2:933-960). **Final answer (proposed):** the interim verbatim — the self-editing
  campaign is never admitted.
- **OQ-440** — see §OQ-440 promotion-test verdict above (`resolve-with-evidence`).
- **OQ-443** — interim (quoted): "`RemovalVerdict` is per rule; coalition credit
  (ADR-0200 M3) does not feed it". Landed: `RemovalVerdict` carries `debt_ref` + `kind`
  + `verdict` — per-rule, no joint/coalition member (`debt.rs:824-870`);
  `manager.rs:551-640` `retire`/`propose` read settled verdicts per debt. **Final answer
  (proposed):** the interim verbatim — leave-one-out stays per-rule.
- **OQ-449** — resolved by landed evidence (table row + §entry-test readings): the
  `coupling_agreement` floor is `true`-measured; `provider_sampling` is admissible only
  as a declared+measured coupled source; M5 and judged outcomes stay `exploratory`. The
  interim's literal "never joins" is superseded by the landed V10 admission rule — the
  final answer is the landed rule, never a rollback of the coupled-source mechanism.
- **OQ-451** — interim (quoted): "calibrated judged outcomes are labelled `judged` and
  never headline; `confirmatory` attribution uses deterministic outcome metrics only
  (ADR-0200 V5; ADR-0047)". Landed: `judge` outcome requires `calibration_ref` and caps
  the label at `exploratory` (`attribution.rs:436,1356`); `judged` cell labels are
  report-only (analysis.rs:1559-1647). **Final answer (proposed):** the interim verbatim
  — `confirmatory` never takes a judged metric.
- **OQ-456** — interim (quoted): "attached-mode reward subscriptions enter `hh-embed/1`
  only as an `experimental` item with a debt record and a `zero_uses` removal test
  (ADR-0178 D4; ADR-0202 D4)". Landed: no reward-subscription op (Group R reads +
  `experimental` tier exist; nothing subscribed). **Final answer (proposed):** the
  interim verbatim — it needs the experimental item; no Group-R spelling is admitted.
- **OQ-459** — resolved by landed evidence: `ADMISSIBLE_REWARD_SOURCES` admits oracle
  classes only; judged spellings refuse. **Final answer (proposed):** deterministic
  oracle classes only — the stricter first cut is the landed contract.
- **OQ-466** — interim (quoted): "`lab/org-policy-v1` declares responders as a factor;
  a simulated responder is an instrument model (ADR-0207 D6); human-wait metrics under
  simulation render `n/a` unless the design says otherwise". Landed: the exemplar
  declares `attendance` `role: responder_stratum`, instrument-charges simulated
  responders via `simulated_responder_calibration_ref`, and declares
  `human_wait_ms_per_item` a primary — the design "says otherwise". **Final answer
  (proposed):** the interim verbatim.

## §10 minors (deliverable 2) — per-minor basis (dated engineering cells)

Each row carries the §10 entry's stated blocker and this ticket's proposed resolution
(2026-10-22). None of the 24 is a code gap — every `needs-code-leg` names a
records-conform change at `R4.15b` (the 164e skeleton's explicit §10-minors scope).

- **C-11** — "creator-kind sum vs §8.1 `Origin`". Landed `Origin` (human/model/tool/
  kernel/participant/import/evolution/cache) is the provenance record's field; §5a.4's
  four-member sum is the `BranchRecord`'s own `provenance.origin` — *proposed:*
  ratify the distinct-fields reading; the two never merge.
- **C-16** — "values differ, not only the name". Landed: `replay.driver_mode` ∈
  `{reconstruct, deterministic}` (required, `ReplayDriverMode::parse`);
  `fork.replay_mode` ∈ `{inherited, exact, structural, observational, none}`;
  attribution `replay_mode_requested` ∈ `{re_executed, deterministic}`. *Proposed:*
  resolved — three distinct fields, each landed on its own surface.
- **C-17** — "`cardinality = one-per-suite, locality` vs the §3.3 sum". Landed:
  `Cardinality` enum = the §3.3 sum (ADR-0023). *Proposed:* resolved — the enum
  governs; `one-per-suite`/`locality` is the §5h.4 cell's descriptive scoping.
- **C-20** — "unify the two `SchedulingPolicy` records". Landed: one `SchedulingPolicy`
  (the experiment engine's pools — `hh-experiment`); the fleet carries none (dispatch
  is the reconciler's ratified order). *Proposed:* they stay distinct — §5i.1's record
  names the fleet surface, §6.3's the engine's.
- **C-21** — "`control.work_item.annotated` homed to plane 5". Landed: the class table
  registers the whole `control.work_item.*` family under `control`
  (`classes.rs:1306-1315`). *Proposed:* resolved — the `control` home stands.
- **C-26** — "`hook_point: DecisionPoint` is wrong-typed". Nothing implements hook
  points. *Proposed leg:* retype the §7.4 member to a hook-site kind spelling (a named
  hook point, never a `DecisionPoint`) at the records conform.
- **C-27** — "is `defer` an approval response?". Landed: yes — Π-12's
  `UnattendedPolicy` is `deny | defer | auto_review` (`approval.rs:676, 810,
  948-951`); `unattended_policy` levels name it. *Proposed:* resolved —
  ADR-0070's wording is the contract's.
- **DAG-10** — "(Stage-5 rules)" tags outside the §9.0 grammar. *Proposed:* ratify the
  content-vs-node convention — the tag annotates rule-content landing; items proper
  land at §4.4 stages.
- **DAG-11** — "inline the l1-spike-spec score bands". *Proposed:* descope — the
  by-reference citation stands.
- **DAG-12** — "unstaged `R-2.8.4²`/`R-2.12.2²` — stage-note ADR owed". *Proposed leg:*
  the conform lands the stage-note ADR naming each item's §4.4 stage (OQ-410's cell).
- **TR-06** — "§9.7 6b maturity cell covers the whole bundle". Landed: per-family
  `maturity`; `ahe` instrument-grade, research families research-grade. *Proposed:*
  resolved — the cell conforms to per-item flags.
- **TR-07** — "§1.1 attributions to ADR-0003/0005". *Proposed leg:* re-cite by quote to
  the carrying sections (cite-by-quote convention, TR2-04).
- **TR2-04** — "'decision n' over unnumbered bullets". *Proposed leg:* adopt
  cite-by-quote; the conform re-cites the four references.
- **LCD-04** — "'escape hatch' naming the endorsed `amend()` widening". *Proposed:*
  the in-force naming stands — `amend` widening is the term; "escape hatch" is gloss.
- **LCD-05** — "rename the meta-harness terms". *Proposed:* the ratified ADR wording
  stands verbatim.
- **LCD-06** — "inline the §6 matrix into §1.6". *Proposed:* descope — the ADR-mapping
  stands; the matrix stays by reference.
- **LCD-08** — "native-seal rule for unregistered `ext` keys". *Proposed:* descope —
  unregistered `ext` stays declared-foreign, never natively sealed.
- **LCD-R2-05** — "'sampling passthrough' as an ordinary `model.call`". Landed: the
  gateway lowers sampling members under provider spellings (`anthropic.rs:206`).
  *Proposed:* resolved — the contract statement is the landed behaviour.
- **r2 C-12** — "wildcard `R-2.n.x` shorthands in ten paragraphs". *Proposed leg:* the
  conform expands each to its exact R-id list from the owning sections.
- **r2 C-16** — "`ConformanceRecord`/`probe_profile` signatures differ". Landed:
  `records.rs:278`'s signature. *Proposed leg:* conform §5b.3's `probe_profile` cell to
  the landed record shape.
- **E2-05** — "ADR-0208 §4/§5 vs §A–§F". *Proposed leg:* re-letter — §5 → §C (evolution
  loop, the maturity rulings), §4 → §B (control seam) — verified at the conform.
- **E2-09** — "short-id overloads need a glossing convention". *Proposed:* descope —
  context + §1's glossary carry it; no convention document lands.
- **E2-11** — "AC-A2-2 symbol list is wrong". *Proposed leg:* the conform corrects the
  list to the declared symbols (verified against §3/§5 at write time).
- **E2-17** — "§4.4 Core-column style mix". *Proposed:* descope — tables are data;
  style-uniformity is not a contract property.

## Proposals — the descope ADR text + authorising words

One ADR text covers the five `descope-by-ADR` minors (DAG-11, LCD-06, LCD-08, E2-09,
E2-17); proposed shape for the R4.15b conform to write once adopted:

- **ADR text (proposed):** a `docs/adr/` build ADR (next free number at write time) —
  *"The five READINESS_REPORT §10 minors whose resolution is 'no change ever' are
  descoped: §10.6 never inlines `synthesis/l1-spike-spec.md` §5's score bands (the
  by-reference citation stands); §1.6 never inlines the report §6 test matrix (the
  ADR-mapping stands); no native-`seal` rule lands for unregistered `ext` keys
  (declared-foreign stays opaque); no E1–E3/C6/S1–S4 glossing-convention document lands
  (context + the §1 glossary carry disambiguation); the §4.4 Core-column style mix is
  permanent (tables are data). Re-entry: a new ADR per item."*
- **Authorising operator words (proposed):** a GATE-G7 answer in the adopt/amend/decline
  row — e.g. `"descope the five no-change §10 minors — DAG-11, LCD-06, LCD-08, E2-09,
  E2-17"` — or the batch act `"adopt all"` on this file's table.
- **Conform row:** `R4.15b` (164e — records conform for ADR-0215 + the §10 minors); the
  seven `needs-code-leg(R4.15b)` items land their named text changes in the same conform
  pass. Nothing here is `needs-code-leg(R4.15c)` — no code is required by any adopted row.

## What this brief does not decide

No register status change, no ADR text and no spec amendment land here — every cell above
is a proposal the operator adopts, amends or declines at GATE-G7 (the recording is
R4.15b's). The interim rules stay in force unchanged until then; every ADR-0215 entry
test is preserved verbatim as the re-entry trigger. BL-38's flip is R4.16's accounting
after GATE-G7 adoption and the conforms land. Note for the fleshing pass: the BL-37
briefs' `R4.15a` conform labels are naming drift — the 164e skeleton scopes BL-37/BL-38
to `R4.15b`; reconcile at GATE-G7, never here.
