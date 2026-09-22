# Phase 4 synthesis memo — autonomy and evolution (F3, F4, F5, I5, I6, I7, I8, L8)

**Date:** 2026-09-10 · **Pass:** Phase 4 synthesis (doc 3 §7.1, §11.5) · **Gate:** **passed** · **Binding on Phase 5:** §6 of this memo, ADR-0208 (reconciliation rulings), ADR-0209 (scope changes), Ontology **v4** (`registers/ontology.md` §5f/§5g/§6), the maturity vocabulary `{instrument-grade, research-grade}`, and the amended Phase 1–3 ADRs listed in §1.3.

Product name per ADR-0011: **HarnessHarness** ("MetaHarness" appears in this program only inside quoted historical text and file paths; the Phase 4 naming audit found no residual product-name use — CF-467). Language-use per ADR-0050 §8: no Phase 4 artifact commits to a language, runtime, package ecosystem or framework (CF-465).

Resume note: the interrupted synthesis run had folded `sources.md` (S-564…S-627) and `open-questions.md` (OQ-413…OQ-466) and had committed, in the OQ resolution notes, to the ADR/CF numbering used here. This run verified both folds against every sidecar (no row missing, no duplicate, next ids correct) and completed the remaining steps against that numbering.

---

## 1. Decisions ratified

### 1.1 Workstream ADRs (23 proposed → 23 ratified, 14 amended, 0 rejected)

Every ADR carries the doc 3 §3.3 five evidence fields, a T-LCD statement and ADR-0024 rungs (23/23). "amended" = ratified with a logged "Amendment log (Phase 4 synthesis)" section applying a synthesis ruling; the ruling's CF id is in the log.

| WS | ADRs | tier / stage | subject | amended |
|---|---|---|---|---|
| WS-F3 | ADR-0185…0187 | C1/Stage 4 kernel slice; C3/Stage 4–5 | `spawn` = `delegate + allocate + derive + lease` (+ ownership grants); `SubagentSpec`; SP-1…SP-8; topology T0–T7; `delegation_reason`; `lab/delegation-v1`; `SubagentResult`/`ReturnContract`; parent-applied merge; C-1…C-10 | 0185 (spawn steps 4b/4c; payload union; `ChildOutcome`), 0186 (messaging mechanism; scheduler binds, never emits), 0187 (`MergePolicy` = ADR-0192; `control.merge.*`; KP numbering) |
| WS-F4 | ADR-0188…0190 | C3/Stage 4–5; C4 via I5 | bind-only `compute_policy` (B-1…B-6; `static` null); `compute_estimator` {rules, bandit, surface_prior, predictor}; `lab/value-of-compute-v1`; `voi_weighted` | — |
| WS-F5 | ADR-0191…0193 | C3/Stage 4 (two slices) | `OwnershipRecord` (one writer per object); labelled peer messages; closed `MergePolicy` (no LWW); `MergeReport`; vetoes; `ChildOutcome`; FD-1…FD-4; reservation order; `ConsistencyLevel`; `CoordinationPolicy` | 0191 (`detach_to_child`; one mechanism), 0192 (one sum; first-slice default), 0193 (FD names; KP rows; `coordination_delta`) |
| WS-I5 | ADR-0194…0196 | C4/Stage 6a…6d | stages S0–S10; candidate state machine; `EvolutionCampaignSpec`; seven-point acceptance report; G1–G10; `evolution_proposer` class + taxonomy; `BudgetSplittingTrap`; hosted-coordinate search | 0194 (S2 `attribution_ref`; S5 as designed ablation; S6 `CompatibilityRecord`; `split` canaries exploratory), 0195 (typed `RemovalTest`; `SelfModificationRefused`) |
| WS-I6 | ADR-0197/0198 | C0/Stage 1 schemas; C0/Stage 3 fixtures; C1/Stage 5 triggers; C4/Stage 6 service | `DebtHomes/1` (17 homes); additive `AssumptionDebtRecord/1`; `DeficiencyClass/1`; derived `evidence_grade`; `DebtIndex`; one trigger model; `RemovalTest` sum with `validate_removal_test` at `seal`; `RetirementRecord`; `DebtReport`/`DebtPolicy`; spec-debt register | 0197 (events generic + instances; consolidation hook; snapshot scope) |
| WS-I7 | ADR-0199…0201 | C2/Stage 5 (M1, report); C4/Stage 6 (M2–M5) | values only from executed interventions; estimands keyed to validity modes; OQ-323 constants; M0–M5 + V1–V12; `AttributionReport/1`; label gate; `attribution_quality`; KA-I7-1…12 | — |
| WS-I8 | ADR-0202…0204 | C0/Stage 1 schemas; C1/Stage 5 provider-drift guard; C4/Stage 6 `research-grade` | HarnessHarness never trains; `training_export/1`; `SnapshotClaim`; `CompatibilityRecord`; harness regression suite; consolidation proven by retirement; `CoEvolutionCycleRecord`; safety co-evolution scope | 0202 (maturity spelling; record home), 0203 (no `snapshot_scope` field; `snapshot_pair`), 0204 (no reason-enum growth; record homes) |
| WS-L8 | ADR-0205…0207 | C4/Stage 4–6; D-1…D-6 deferred | `WorkItem` spec/status; tri-state adapters; `run_kind = fleet`; reconciler RC-1…8; state map at Π; owner-as-principal; accountability record; escalation chains; `FleetView`; irreversibility ceiling; `lab/org-policy-v1` | 0205 (`fleet` via ADR-0183 rule; audit subset; F3 seam), 0207 (`ext.*` dimension; D-1 entry test; attendance table) |

### 1.2 Synthesis-authored ADRs

- **ADR-0208** — cross-dossier reconciliation: §A one coordination substrate (one `spawn`, one `MergePolicy`, one peer-message mechanism, FD/KP/`on_owner_end` names, kernel/budget composition); §B the control seam (bind step, scheduler never emits decisions, `TaskValue`, `effort`); §C the evolution loop with its record homes, label gate, governance-invariant verification and maturity vocabulary; §D the organizational layer's boundaries and the attendance/default table; §E Ontology v4 rulings; §F audits.
- **ADR-0209** — scope changes: eight items → `specified-by-ADR`; stage and maturity notes on seven; R-2.9.8 `research-grade, conditional`; novelty annotations confirmed; no tier or MoSCoW change.

### 1.3 Phase 1–3 ADRs amended at Phase 4 (each carries an "Amendment log (Phase 4 synthesis)")

ADR-0007 (debt-record fields), 0016 (`hh.evolution/hypothesis_ref` ext key), 0017 (`coordination_delta`), 0020 (retired-by-ADR meaning), 0026 (consolidated ledger additions), 0035 (check 8; provenance table re-keyed), 0039 (`ext.effects.external_irreversible`), 0045 (metric registrations; evolvability re-keying), 0053 (`SelfModificationRefused`), 0066 (audit-grade work-item classes; fleet anchor; `EscalationCause`), 0087 (`resource_keys[]`), 0103 (bind step; no new `Cue`), 0107 (sizing term), 0109 (`TaskValue`), 0117 (expiry normalised), 0120 (`SnapshotClaim` fields; `policy_version_exposed`), 0124 (`effort` capability), 0126 (retire = human-sealed supersession), 0131 (`Trigger.peer_message`), 0132 (KP-16…21, KP-F1…F5), 0135 (coupled seed derivation), 0141 (`training_export/1`), 0152 (`snapshot_pair`), 0156 (`voi_weighted`; candidate-bound retirement; `retirement_batch`), 0157 (`AttributionReport/1`; `pairing_key = fork_point`), 0160 (`refit`), 0161 (`arm_role`, `intervention_ref`; annotations), 0168 (attendance table pointer), 0170 (`DebtReport` views; `FleetView`; rotations narrowed), 0178 (`zero_uses`), 0182 (X6), 0183 (`run_kind = fleet`; `snapshot_pair`). Every amendment is additive or a wording refinement; no ratified decision is reversed; every closed sum that grew did so by dialect bump.

---

## 2. Contract convergence achieved

### 2.1 Evidence discipline on the highest-uncertainty cluster (brief focus 1)

- **Matched-budget conditionality + maturity on every C3/C4 ADR (CF-463).** Field (d) of ADR-0185…0207 states the `MatchSpec`/`ComparisonReport` under which any benefit claim holds; ADR-0186/0190/0193/0194/0207 ship the Lab recipes (`lab/delegation-v1`, `lab/value-of-compute-v1`, `lab/coordination-topology-v1`, the campaign experiments, `lab/org-policy-v1`) that are the removal tests of every default; maturity is placed per ADR (C1 slices and Stage 6a `instrument-grade`; automated families, hosted-coordinate campaigns and the co-evolution layer `research-grade`, `preview`-labelled, excluded from C0–C3 acceptance and leaderboards without independent reproduction).
- **No C0–C2 decision rests on provisional Phase-4 evidence.** The 32 amendments to Phase 1–3 ADRs are dialect bumps, field additions and wording; each cites a ratified rule or a Tier-A/B source (LangGraph channels, CRDT theory, Jepsen, Shapley/Castro/Grabisch, Adaptive-Consistency, Garivier–Moulines, Sculley, Piranha, Chromium/OpenHands/litellm deprecation regimes, Symphony/Kubernetes controllers) never a 2026 number. The C0/C1 slices ratified this phase — ADR-0197's `DebtHomes/1`/`AssumptionDebtRecord/1` fields and seal-time `validate_removal_test`, ADR-0203's `CompatibilityRecord`/`SnapshotClaim` schemas, ADR-0185's C1 spawn slice, ADR-0199's use of the ratified ADR-0135 hook — rest on ratified ADRs and pinned-commit source reads (`sources.md` Phase 4 notes).
- **ADR-0002's governance invariants are acceptance criteria consistent with the H1 monitor (CF-462).** (i) proposal ≠ deployment → G1, `SealRefused{not_human}`, AC-I5-1, AT-H1-05 (iv); (ii) the proposer never deploys → G1/G7, AC-I5-1/-5 (judges never gate); (iii) the kernel outside the editable surface, narrowing only → G2/G5, `classify` at S1 and again at S7, `SelfModificationRefused` (now in ADR-0053 D-5 and ADR-0182 X6), AC-I5-2/-8; (iv) reversibility and expiry → G3 (`revert` only to a human-sealed ancestor, escalated, never silent) and G4 (reflexive debt record with a typed removal test), AC-I5-9/-10. Each is one-to-one with D-5's refusals; no new authority path exists anywhere in the cluster.

### 2.2 F3/F4/F5 compose with the security kernel and the budget model (brief focus 2)

```
delegate decision (strategy or HarnessRule; owner ∈ {code, model}; delegation_reason mandatory)      ADR-0103/0186
   └─ compute_policy.bind(d, ctx): binds budget_slice / spec_ref / unbound reason; tightens; never widens   ADR-0188 B-1…B-6
      └─ envelope.check → spawn(parent, lease, decision, SubagentSpec)                                     ADR-0185
            (1) fan_out/depth/reserve   (2) authorize spawn_process{agent_process}   (3) delegate ⊆ delegable(parent), attenuation_delta recorded
            (4) allocate slice|pool ≤ remaining  (4b) grant_ownership ⊆ grantor's  (4c) share: acquire(resource(key)) all-or-nothing
            (5) derive: containment = effective(parent ⊓ child)   (6) child run, own lease, parent_anchor   (7) spawned{union payload}   (8) await | background
child run: fresh context ⊆ supplies; peer messages = Trigger.peer_message → Cue.woken at ≤ delegate, never endorse     ADR-0186/0191
child terminal → control.subagent.result{outcome: ChildOutcome, SubagentResult} → Cue.delegation_completed              ADR-0187/0193
merge(parent, lease, children, MergePolicy) → MergeReport{lost_write_count = 0, conflicts[], absent[]} → verify on merged state   ADR-0192
budget: slice remainder released at terminal; pool children only tighten; exhaustion root-first; charges propagate to every ancestor   ADR-0040
```

Delegation narrows-or-preserves at exactly one seam (`delegate` inside `spawn`); ownership is attenuated like authority and is never authority (O-2/O-6); the scheduler cannot widen (`amend` absent from its verb set; an evolution-authored loosening classifies `budget_delta = loosening` and is refused); the reference monitor gains check 8 (ownership at `prepare`); conservation across fan-out is AC-F3-02/AC-F5-08 (= AC-L2-3/-5 through `spawn`). The topology, the merge policy, the isolation mode and the compute policy are explicit component-level factors under matched budget (T-LCD-09/-14); the packaged defaults are T0 `single`, `fork_snapshot`, `single_writer` + `parent_decides`, `static`.

### 2.3 The evidence-bearing evolution loop I5 ↔ I6 ↔ I7 ↔ I8 over J2/J3/J4/J5 (brief focus 3)

```
observe   S0  EvidenceCorpus = project(runs, evolution_evidence_view) — no held-out surface (L1); read through the D-5 fs_read handle      ADR-0194/0196, ADR-0143
edit      S1  CandidateProposal{diff: HirDiff, base} — classify: no widening/loosening/MUST-code/excluded target; validate_assembly(apply(base, diff))   ADR-0195 G2/G5, ADR-0017, ADR-0147 M-2
          S2  FailureHypothesis + PredictedEffect; attribution_ref → AttributionHypothesis(observational) | AttributionReport/1; TargetMismatch      ADR-0195, ADR-0201 D1
attribute S3  CounterexampleSet on search/dev, rows `estimated` (PredictionFalsified)                                                       ADR-0194
evaluate  S4  adaptive_search arm set; MatchSpec{matched_total} normative; search_time_benefit vs oracle_best_of_n; SearchBudgetRecord complete   ADR-0041 M3, ADR-0046, ADR-0156/0159, ADR-0190 voi_weighted
matched   S5  full_set held-out artifact_benefit + retention + vetoes  ≡  M1 LOI designed ablation over the diff → attribution_label ≥ designed_ablation   ADR-0200 M1, ADR-0201 D2
          S6  transfer on ≥ 1 held-out family + beneficiary compliance chain → TransferProfile; CompatibilityRecord{verified} per (snapshot, definition)   ADR-0159, ADR-0203, ADR-0152 snapshot_pair
          S7  security invariance: classify again; AT-H1-05; LT-01…12; vetoes ≤ base                                                       ADR-0053 D-5, ADR-0059
seal      S8  human `seal` (definition class) sees EvolutionAcceptanceReport (items 1–7) + AssemblyDiff; canary shadow | split(exploratory); abort ⇒ revert to a human-sealed ancestor   ADR-0195 G1/G3, ADR-0135, ADR-0149
rollout   S9  publish/supersede{edit}; AssumptionDebtRecord complete (ConditionedRuleIncomplete otherwise); selection{n_candidates_registered, …} on the leaderboard   ADR-0037, ADR-0163 L4
expire/   S10 removal_test: RemovalTest{kind: retirement_experiment} validated at seal; triggers ∪ {transfer_sign_flip, superseding_candidate}; RemovalVerdict;
retire        retired = human-sealed supersession + RetirementRecord; expired_used rows excluded from headline reports                        ADR-0197, ADR-0156 (candidate-bound; retirement_batch), ADR-0126
consolidate   candidate lesson (C1–C5, never N1–N5) → export_training(training_export/1) → external trainer → import_snapshot(SnapshotClaim) → run_regression_suite
              → verified | drifted{rules[]} | broken → per-lesson retirement experiment on snapshot_out under retention + held-out family → consolidated | partial | not
              → RetirementRecord{rationale: ConsolidationReport ref, snapshot_ref}; the import fires model_version_change on every scoped rule; research-grade   ADR-0202…0204
substrate  every evaluation an ExperimentSpec run as run_kind = experiment (J3); every score a ResultsRow/1 version with arm_role/intervention_ref (J5);
           every claim a ComparisonReport with label/budget_match/benefit_kind/EstimatorSelection (J4); every artefact a registry record, never model-facing (J2)
```

Every step is a ratified operation; the two things no published loop has — a per-candidate held-out gate and a separate deploying authority — are G1 and S5. `attribution_quality` (evolvability) is defined over the same matched Δ. `DebtHomes/1` makes "no rule without a hypothesis" one schema rule across seventeen homes, with the seal-time `validate_removal_test` as the executable form of T-LCD-05; the spec-debt register (ADR-0198) applies the same record to the program's own decisions.

### 2.4 The organizational layer and its boundaries (brief focus 4)

L8 is records, projections and policies over ratified contracts (CF-464): triggers, occurrences, inbox runs and `continue_goal` are ADR-0131's; leases and restore ADR-0130's; Π-12, `UnattendedPolicy`, `ApproverGrant`, `defer` and `ActionPattern` ADR-0069…0071's; the escalation record ADR-0066's. It adds the `WorkItem` (desired `spec` at `≤ external` / observed `status`), `run_kind = fleet`, the reconciler (RC-1…8; reconcile-before-dispatch; blocked ≠ stalled; no memory-only state), the state map enforced at Π as narrowing leaves on lifted write capabilities, owner-as-principal with the acknowledged handshake, escalation *routing* as budgeted `message_human` effects with legitimate-responder resolution, `FleetView`, and an optional unit-priced irreversibility ceiling (`ext.*`). The F3 seam is `causes[]` + `goal_ref` (inter- vs intra-activation objects). The four `ask` defaults are one table (ADR-0208 §D.5).

### 2.5 Vocabulary (Ontology v4)

119 terms folded (§5f); 15 rulings (§5g): one `MergePolicy`; one peer-message mechanism; `SubagentResult{outcome: ChildOutcome}`; `on_parent_end`/`on_owner_end` with `detach_to_child`; FD-1…FD-4; one maturity vocabulary; snapshot scope as `scope.model_selectors`; weight vs memory consolidation; `noise_coupling` vs `coupling_assumption`; `attribution_quality` vs `attribution_completeness`; `Goal.origin` for sourced work; `run_kind` v4 sum; KP numbering; `CompatibilityRecord` home; evidence grade vs debt status. Nine canonical-name rows added to §6.

---

## 3. Evidence discipline check (doc 3 §3, RK-03, RK-11)

- **Five fields:** 23/23 proposed ADRs carry (a)–(e) and a T-LCD statement (mechanical check at synthesis); every C3/C4 ADR's (d) names its `MatchSpec` conditionality; every C4 ADR names its maturity placement.
- **Provisional audit:** the 23 2026 single-team rows added this phase (S-564, S-565, S-574, S-582, S-583, S-585–S-588, S-593–S-596, S-599–S-601, S-609, S-610, S-612, S-613, S-618, S-619, S-626, S-627) and the 2025 unreplicated preprints (S-576, S-599) are cited for mechanism existence, hypotheses or counter-evidence only; the 13 promotions S→P (S-026, S-027, S-028, S-054, S-063, S-064, S-069, S-075, S-081, S-086, S-087, S-090, S-094) supply mechanisms and failure signatures already implied by ratified rules. Every C0/C1 element ratified this phase rests on ratified ADRs, Tier-A theory (Shapley 1953, Castro 2009, Grabisch–Roubens 1999, Pearl 2009, Glasserman–Yao 1992, Lamport 1978, Herlihy–Wing 1990, Shapiro 2011, Garivier–Moulines 2008, Aggarwal 2023, Sculley 2015, Ramanathan 2020, Parasuraman 2000, Bainbridge 1983), Tier-B production regimes, or source read at pinned commits. No exception found (CF-463).
- **Sources:** 64 new rows (S-564…S-627); no cross-sidecar duplicate; path-level citations folded into 16 existing rows; 13 promotions S→P; S-048 (HTTP 403) stays `S` under the CF-024 pattern.
- **Language-leak audit (CF-465) and naming audit (CF-467):** clean.

---

## 4. Register state after folding (id maps)

Temp ids in every dossier, sidecar and ADR were rewritten to final ids.

| WS | ADRs | sources | open questions | conflicts |
|---|---|---|---|---|
| WS-F3 | ADR-0185…0187 | S-564…S-565 | OQ-413…OQ-418 | CF-394…CF-398 |
| WS-F4 | ADR-0188…0190 | S-566…S-577 | OQ-419…OQ-426 | CF-399…CF-405 |
| WS-F5 | ADR-0191…0193 | S-578…S-584 | OQ-427…OQ-434 | CF-406…CF-414 |
| WS-I5 | ADR-0194…0196 | S-585…S-588 | OQ-435…OQ-440 | CF-415…CF-422 |
| WS-I6 | ADR-0197…0198 | S-589…S-592 | OQ-441…OQ-447 | CF-423…CF-430 |
| WS-I7 | ADR-0199…0201 | S-593…S-609 (sidecar 17 skipped; 18 → S-609) | OQ-448…OQ-453 | CF-431…CF-435 |
| WS-I8 | ADR-0202…0204 | S-610…S-621 | OQ-454…OQ-460 | CF-436…CF-440 |
| WS-L8 | ADR-0205…0207 | S-622…S-627 | OQ-461…OQ-466 | CF-441…CF-448 |
| synthesis | ADR-0208, ADR-0209 | — | — | CF-449…CF-467 |

Next ids: **S-628**, **OQ-467**, **CF-468**, **ADR-0210**. Ontology **v4**.

Pre-existing rows updated: 38 open questions resolved or advanced (list in `registers/open-questions.md` "Synthesis notes (Phase 4)"); CF-017's residual exposure unchanged (WS-K2 Stage ≥ 5).

---

## 5. Open items carried forward (owner · due · blocking?)

All Phase 4 conflicts are dispositioned (55 workstream-flagged: 48 resolved, 4 resolved as rejected precedent, 3 recorded; 19 synthesis-raised: 16 resolved, 3 verified/clean).

**Deferral ADR candidates for Phase 5 (brief focus 5 — listed, not ratified):**

| candidate | question | owner · entry/decision test |
|---|---|---|
| OQ-422 | Certaindex-style probe completions as `Validator{kind: judge}` vs `router_predictor` call | WS-G3/C2 · decides calibration and independence obligations for `predictor` variants |
| OQ-423 | Allocation of evaluation budget *across* designs/candidate components (cross-design half of OQ-025) | WS-I5/F4 · a portfolio allocator over `adaptive_search` designs with one budget node |
| OQ-425 | HIR/2 `route`/`effort` `DecisionPoint` (joins OQ-292) vs the bind step on `propose` | WS-A3/F1 · HIR/2 backlog with `derived-from.hypothesis_ref` (CF-416) |
| OQ-429 | `ResourceKey` namespace registration (`custom(ns, bytes)`) — extends OQ-110/OQ-313 | WS-F5/E1/J2 · a `NamespaceRecord` rule for resource keys |
| OQ-431 | Ownership escrow when a grantor never restores: expiry and inheritance | WS-L8/B3/F5 · a policy with the owner as fallback (ADR-0207 O-5) |
| OQ-437 | `live_split` design kind (unpaired live canary) vs `exploratory` only | WS-J3/J4 · an unpaired estimator with `budget_match` over realised consumption |
| OQ-439 | Evolving the evolution service (human-origin campaign; reviewer independence; G5 applied to the reviewer) | WS-H1/I5/L5 · validators disjoint from every campaign suite |
| OQ-443 | Joint removal / redundancy: wiring coalition credit (ADR-0200 M3) into `RemovalVerdict` | WS-I7/I6/J4 · pairwise designs admissible under the OQ-364 ceiling |
| OQ-449 | Provider-side noise coupling (`seed_honoured` + batch-invariant sampling as probed capabilities; `coupling_agreement` floor) | WS-C1/C3/I7 · a gateway declaration + probe |
| OQ-451 | Calibrated judged outcomes in `confirmatory` attribution reports | WS-G3/I2/I7 · calibration protocol per domain |
| OQ-456 | Attached-mode reward subscription over Group R vs an `experimental` verb | WS-K4 · ADR-0178 experimental opt-in with a removal test |
| OQ-459 | Judged rewards export: calibrated non-reasoning-reading judges vs deterministic-only first cut | WS-G3/I2 · OQ-127 interaction |
| OQ-461 | Fleet sharding (per source, per definition, per organization; item migration) | WS-L8/B1/H6 · default one fleet per definition + source set |
| OQ-462 | `PrincipalDirectory` contract (L8 deferral D-1 entry test) | WS-L8/L3/H7 · two out-of-process precedents |
| OQ-466 | Responder simulation for `lab/org-policy-v1` | WS-J3/G3/L8 · calibration set + independence axes (ADR-0116) |
| L8 D-2…D-6 | multi-tenant fleets; value-at-risk pricing; dispatch prioritization by expected value (WS-F4); reconciler-side source mirroring; cross-item shared-state coordination (WS-F5) | ADR-0207 D7 entry tests |
| OQ-440 | Promotion test `research-grade → instrument-grade` for proposer families (replication across suites/families) | WS-I5/L7/J2 · fixed in the readiness report |
| OQ-445 | Spec-debt register mechanics (`evidence_max_age` for ADRs; who runs revalidation after Phase 5) | Phase 5 synthesis / WS-L6 · ADR-0198 D1 |

**Blocking Stage 4–6 (MUST-data placeholders in force):** OQ-417 (message caps as budget dimensions — WS-L2/A3), OQ-433 (HLC envelope placement — WS-B1), OQ-434 (hosted `ChildOutcome` lifting — WS-J6), OQ-418 (child attendance — WS-H7/H1), OQ-413 (`ReturnContract.summary.max_tokens` — WS-F3/C3), OQ-424 (`bind` hot-path ceiling — WS-L5/F1; joins OQ-075/OQ-407), OQ-420 (bandit window/discount/`min_n`), OQ-430 (semantic-conflict validator scope), OQ-432 (parent `supersede` basis — WS-D4/H7), OQ-435/OQ-436 (S3 screen and S5 retention defaults), OQ-438 (rebase at 6c), OQ-441 (debt policy defaults), OQ-446 (owner identity `TeamRef`), OQ-448 (attribution constants by A13 power), OQ-450 (Shapley pricing: refuse vs degrade), OQ-453 (hosted `observation_substitution`), OQ-457/OQ-458 (cycle and snapshot-scope defaults). The Phase 3 list stands (OQ-363, OQ-246, OQ-116, OQ-242 pin half, OQ-235/OQ-240, OQ-165 HIR half, OQ-402/OQ-131, OQ-407/OQ-075).

**Accepted tensions and notes:** every Phase 4 dossier exceeds the length target because the stage, invariant, catalogue and acceptance tables *are* the deliverable (recorded, not fixed); the co-evolution layer's *value* is unknown for frontier snapshots (only the provider-drift half of the guard applies) — recorded in ADR-0204 (d) and R-2.9.8; S-048 unreachable (403).

**Risks:** RK-07 exercised and held (ADR-0208); RK-03/RK-09/RK-11 clean; RK-06 mitigation specified (ADR-0198); RK-05 advanced (novelty claims (iii) specified end to end); no new risk.

---

## 6. Settled for Phase 5 — binding on spec assembly (section authors, architect, review lenses, fixer, readiness)

Phase 5 cites these by ADR id. Each item names what Phase 5 may rely on and what it must not re-decide.

### 6.1 The complete ratified ADR set by tier (209 ADRs; Spec §7.2 skeleton input)

- **Program / pre-ratified (Preflight, Phase 0):** ADR-0001 (positioning), 0002 (evolution in scope with governance (i)–(iv) and the seven-point standard), 0003, 0004, 0005, 0006 (thesis; N1–N13), 0007 (LCD battery), 0008 (canonical names), 0009 + 0050 (language decision; §8 language-use constraint), 0010, 0011 (product name).
- **C0 Foundations (Phase 1):** ADR-0012…0014 (ontology), 0015…0018 (HIR/1), 0019…0022 (compilation), 0023…0025 (definition-is-data; assembly validation), 0026…0029 (ledger), 0030…0032 (effects), 0033…0035 (provenance/authority; monitor set incl. check 8), 0036…0038 (identity/versioning/bundle levels), 0039…0041 (resources; budgets; matched budget), 0042…0044 (telemetry), 0045…0047 (scorecard; protocol; oracles), 0048, 0049 (synthesis).
- **C0/C1/C2 Planes (Phase 2):** ADR-0051…0053 (kernel; D-5 with `SelfModificationRefused`), 0054…0056 (IFC), 0057…0059 (credentials), 0060…0062 (containment/egress), 0063…0065 (extension trust), 0066…0068 (audit incl. fleet anchor), 0069…0071 (approvals), 0072…0074 (context), 0075…0077 (compaction), 0078…0080 (memory), 0081…0083 (memory lifecycle), 0084…0086 (procedures/skills), 0087…0089 (capabilities), 0090…0092 (tool compiler), 0093…0095 (exposure), 0096…0099 (protocol edges), 0100…0102 (execution), 0103…0105 (control strategy; the bind step), 0106…0108 (envelope), 0109…0111 (verification; `TaskValue`), 0112…0114 (claims), 0115…0117 (critics), 0118…0120 (gateway; `SnapshotClaim`), 0121…0123 (router; ensembles), 0124…0126 (profiles; `effort`; expiry), 0127…0129 (caches), 0130…0132 (durability; KP battery), 0133…0135 (branches; counterfactual hook), 0136…0138 (environments), 0139…0141 (bundles; `training_export/1`), 0142…0144 (benchmarks), 0145, 0146 (synthesis).
- **C1/C2 Lab, surfaces, extensibility (Phase 3):** ADR-0147…0150 (assembly service), 0151…0153 (registry; `snapshot_pair`), 0154…0156 (experiments; `voi_weighted`; candidate-bound retirement), 0157…0160 (analysis; `AttributionReport/1`), 0161…0163 (results; leaderboard), 0164…0166 (Hosting ABI), 0167…0169 (CLI), 0170…0172 (web), 0173…0175 (MCP server), 0176…0179 (`hh-embed/1`), 0180…0182 (plugins; X6), 0183, 0184 (synthesis).
- **C1/C3 Control and orchestration (Phase 4):** ADR-0185…0187 (F3), 0188…0190 (F4), 0191…0193 (F5).
- **C0/C1/C2/C4 Measurement and evolution (Phase 4):** ADR-0194…0196 (I5), 0197…0198 (I6), 0199…0201 (I7), 0202…0204 (I8, `research-grade`).
- **C4 Organizational layer (Phase 4):** ADR-0205…0207 (L8); **synthesis:** ADR-0208, ADR-0209.

### 6.2 Contracts Phase 5 assembles verbatim and must not re-decide

The one `spawn` and its invariants SP-1…SP-8 (ADR-0185); the closed `MergePolicy`, `MergeReport` and the two coordination vetoes (ADR-0192); the peer-message mechanism (`Trigger.peer_message` → `Cue.woken`; ADR-0131/0191); the bind step and B-1…B-6 (ADR-0103/0188); `static` as the packaged scheduler default and the acceptance rule for changing it (ADR-0190 D6); the ten stages and G1–G10 (ADR-0194/0195); the label gate (`observational` evaluates, `designed_ablation` accepts, `causal_*` licenses locality; ADR-0201); `DebtHomes/1`, the typed `RemovalTest` and seal-time `validate_removal_test` (ADR-0197); the estimand hierarchy and OQ-323 placeholders (ADR-0199); the training-stack boundary (HarnessHarness never trains; ADR-0202) and the consolidation proof (ADR-0204); the `WorkItem` split, RC-1…8 and owner-as-principal (ADR-0205…0207); the record homes of CF-460; the maturity vocabulary and the `research-grade` placements (CF-453; ADR-0209); the attendance/default table (ADR-0208 §D.5); every ruling of ADR-0208 §A–§E.

### 6.3 Binding instructions for spec assembly

1. **Section §7.2-5 (per-subsystem specs):** F3/F4/F5 are one control-and-orchestration subsystem with four parties that never merge (strategy proposes, scheduler binds, envelope checks, driver executes) and one coordination substrate; I5/I6/I7/I8 are one evolution subsystem stated as the loop of §2.3 — write it once, cite the stage table, the invariants, the label gate, the typed removal test and the record homes; L8 is stated as records/projections/policies over B3/H7/H6 with the boundary list of CF-464.
2. **Section §7.2-6 (Harness Lab):** register the Phase 4 recipes (`lab/delegation-v1`, `lab/value-of-compute-v1`, `lab/coordination-topology-v1`, the campaign experiments and `retirement_batch`, the KA-I7 fixture suite, `lab/org-policy-v1`) beside the Phase 2–3 exemplars; every default of the cluster names its recipe as removal test.
3. **Section §7.2-9 (build ladder):** Stage 4 = the C1 spawn slice + `single_writer`/`parent_decides` + `static`/`rules` + `run_kind = fleet` over the fixture adapter; Stage 5 = `bandit`/`voi_weighted`, `share` mode, M1 ablation + `AttributionReport/1`, the provider-drift compatibility guard, live debt triggers, adapters/`FleetView`; Stage 6 = 6a human-proposed pipeline (instrument-grade) → 6b one automated family → 6c code search/attribution arms/hosted campaigns → 6d co-evolution interface (research-grade); preconditions: Stage 3, AT-H1-05, ADR-0135 hook, `exp/` namespaces, `retirement` kind, ADR-0126 removal test, `plugin_abi/1` `subprocess_confined`.
4. **Section §7.2-10 (evaluation plan):** every C4 claim is a `ComparisonReport` at `matched_total`; the seven-point standard is the acceptance report; `attribution_quality`, `assumption_debt_health` (the `debt.*` family), `coordination.*`, `scheduling.*` and the fleet metrics enter the scorecard's dimensions as registered in ADR-0045's Phase 4 log.
5. **Section §7.2-11 (deferred items):** list the §5 deferral candidates and L8's D-1…D-6 with their entry tests; ratify deferral ADRs there; carry a spec-debt row for each (ADR-0198).
6. **The spec-debt register:** generate `registers/spec-debt.md` at Phase 5 synthesis with one row per ratified ADR (ADR-0198 D1); apply the readiness-gate rule (no C0 ADR `hypothesized`; every C4 ADR names the model generation it assumes).
7. **Vocabulary:** Ontology v4 is the tie-breaker; use §5g/§6 canonical names (`spawn`, `SubagentResult`, `MergePolicy`, `ChildOutcome`, `compute_policy`, candidate/campaign, `DebtHomes/1`, `AttributionReport/1`, `SnapshotClaim`, `CompatibilityRecord`, `WorkItem`, fleet run, maturity flag); never `Cue.peer_message`, `ChildResult`, `snapshot_scope[]`, `detach_to_owner`, `retired{reason: consolidated}`, `research_grade`/`experimental` as maturity values, or bare "consolidation".
8. **Rules of the road:** cite ADRs by number; ADR-0050 §8 unchanged; precedent paths are evidence, never dependencies; a Phase 5 author who needs a Phase 1–4 amendment proposes it as a CF row for the fixer/readiness pass, never edits a ratified ADR.

---

## 7. Gate check (doc 3 §6; LEDGER Phase 4 row)

| criterion | result |
|---|---|
| All 8 workstreams `done` with ADRs dispositioned | **yes** — 23 ratified (14 amended), 0 rejected; LEDGER rows updated |
| Every C3/C4 ADR carries matched-budget conditionality + a maturity flag | **yes** — CF-463; field (d) and the maturity placement of every ADR-0185…0207 |
| No C0 decision rests on provisional evidence (full ADR set re-audited) | **yes** — §3; the 32 Phase 1–3 amendments are dialect bumps/wording on ratified or Tier-A/B evidence |
| ADR-0002's governance invariants are acceptance criteria consistent with H1's monitor | **yes** — CF-462; G1–G4 ↔ D-5/AT-H1-05 ↔ AC-I5-1/-2/-9/-10 |
| F3/F4/F5 compose with the security kernel and the budget model | **yes** — §2.2; ADR-0208 §A.5/§B |
| The evolution loop composes with the lab substrate and the security kernel | **yes** — §2.3; ADR-0208 §C |
| L8 does not duplicate B3/H7; boundaries recorded | **yes** — CF-464; ADR-0208 §D |
| Deferral candidates listed for Phase 5 | **yes** — §5 |
| Conflicts dispositioned | **yes** — CF-394…CF-467 |
| No technology commitments; naming clean | **yes** — CF-465, CF-467 |

**Gate: passed.** The orchestrator commits at the phase boundary (RK-10).
