# Phase 3 synthesis memo — the laboratory, the surfaces and the extension contract (J1–J6, K1–K4, L5)

**Date:** 2026-09-10 · **Pass:** Phase 3 synthesis (doc 3 §7.1, §11.5) · **Gate:** **passed** · **Binding on Phase 4:** §6 of this memo, ADR-0183 (reconciliation rulings), ADR-0184 (scope changes), Ontology **v3** (`registers/ontology.md` §5d/§5e/§6), and the amended Phase 1–2 ADRs listed in §1.3.

Product name per ADR-0011: **HarnessHarness** ("MetaHarness" appears in this program only inside quoted historical text and file paths; the Phase 3 naming audit found no residual product-name use — CF-393; one stale LEDGER title was corrected). Language-use per ADR-0050 §8: no Phase 3 artifact commits to a language, runtime, package ecosystem or framework (CF-392); the pre-registered K2/K4 exposure (CF-017) is discharged.

---

## 1. Decisions ratified

### 1.1 Workstream ADRs (36 proposed → 36 ratified, 14 amended, 0 rejected)

Every ADR carries the doc 3 §3.3 five evidence fields and a T-LCD statement (36/36). "amended" = ratified with a logged "Amendment log (Phase 3 synthesis)" section applying a synthesis ruling; the ruling's CF id is in the log.

| WS | ADRs | subject | amended |
|---|---|---|---|
| WS-J1 | ADR-0147…0150 | assembly service (`AssemblySource` desugaring; `assemble/plan/apply/validate_batch/explain`; dry-run parity; M-1/M-2); closed diagnostic codes; `AssemblyDiff` + snapshot drift (`freeze`); hosted definitions with a degraded validation profile | 0147 (Group L verbs), 0150 (`participant` kind; `hosting_adapter` binding event) |
| WS-J2 | ADR-0151…0153 | `registry/1` record model + envelope + three status axes + not-model-facing; conformance suites/reports/vector; third-party variants, namespaces (OQ-110), quarantined foreign lifts | 0151 (`participant`/`adapter` kinds; `placement`), 0152 (one `conformance_report` kind), 0153 (`placement` wording) |
| WS-J3 | ADR-0154…0156 | `ExperimentSpec` + `expand` + refusal set (OQ-076/078); experiment-as-run scheduler, settlement, KP-E1…E5; experiment kinds + two exemplars + search-only adaptive validation | 0155 (`run_kind`; merged event family; lease key) |
| WS-J4 | ADR-0157…0160 | pure `analyze` A1–A16 with labels and outcome bounds; estimator set (OQ-016/129); frontier/transfer/benefit (OQ-094); compatibility surface (OQ-019/050) | 0157 (results-query seam, CF-388) |
| WS-J5 | ADR-0161…0163 | `ResultsRow/1` + versions + annotation index + cell table + catalogue; experiment ledger + `AnalysisRecord`; leaderboard L1–L9 + retention | 0162 (`run_kind`; merged event family) |
| WS-J6 | ADR-0164…0166 | Hosting ABI depth (OQ-004/248); mediation + `budget_enforcement` + `requires_*` (OQ-038); `hh-hosting/1` negotiation, probes, DRIFT (OQ-034), adapters + adapter zero (OQ-032) | 0164 (lifting-table note; supply-surface ref; `export`), 0166 (registry kinds) |
| WS-K1 | ADR-0167…0169 | generated-client CLI with noun·verb taxonomy; safety defaults (OQ-265/076); I/O + exit classes | 0167 (K-2 binding, CF-384) |
| WS-K2 | ADR-0170…0172 | projection client V1–V12 (OQ-130); time-travel viewer; security posture P1–P13 | — |
| WS-K3 | ADR-0173…0175 | one serving path + exposure definition (OQ-247); `CallerBinding` caller model + surface-session run; run exposure (OQ-241) | 0174 (`run_kind = surface` in the one sum) |
| WS-K4 | ADR-0176…0179 | `hh-embed/1` contract; injection discipline; stability policy; three bindings (OQ-130) | 0176 (Group L; branch ops; audit proofs — CF-383) |
| WS-L5 | ADR-0180…0182 | `PluginManifest/1` + `ContractVersionPolicy`; `plugin_abi/1` variant host + `GuardVerdict` + V1–V6; extension DAG X1–X6 | 0181 (scope/M19/placement/`host_unconfined`/helper `session` confirmations) |

### 1.2 Synthesis-authored ADRs

- **ADR-0183** — cross-dossier reconciliation: Group L of the embedding contract (every surface a client); the CLI's binding; one `run_kind` sum; one experiment event family; `participant`/`adapter` registry kinds and one `conformance_report`; the analysis↔results seam; the hosting-tier placement check; Ontology v3 rulings.
- **ADR-0184** — scope changes: eleven items → `specified-by-ADR`; stage notes on nine; novelty annotations confirmed; no tier or MoSCoW change.

### 1.3 Phase 1–2 ADRs amended at Phase 3 (each carries an "Amendment log (Phase 3 synthesis)")

ADR-0006 (N4 clause), 0023 (`placement`), 0025 (stage 6a/6b), 0026 (`run_kind`, the experiment family, hosted/registry/surface/session/component classes, `branch_tree`/`ir_refs`, bidirectional dialect compatibility, `hh.experimental` ext), 0038/0139 (bundle members), 0041 (`MatchSpec` enforceability), 0042 (`component_call`), 0043 (M19), 0045 (`requires_capabilities`/`requires_mediation`, `interval_method`, `pareto_position`, veto tiering), 0046 (`ComparisonReport` fields), 0050 (OQ-130 confirmation; D3 wording; bindings), 0063 (`host_unconfined` narrowed), 0068 (`Pinned`), 0071 (pseudo-class retired), 0087 (envelope; `host_process`), 0100 (helper `session`), 0103 (`steer_mode`), 0105 (two arms), 0130 (`resource(run_plan_id)`), 0133 (row fields), 0143 (L5 wording). Every amendment is additive or a wording refinement; no ratified decision is reversed; every closed sum that grew did so by dialect bump.

---

## 2. Contract convergence achieved

### 2.1 OQ-004 — the Hosting ABI's depth (brief focus 1)

ADR-0164 fixes the depth as three lists rather than a union or an intersection: **baseline** verbs every mechanism satisfies (`describe, open, submit, cancel, resume, close, stream_events, probe`; upcalls `request_permission, report_usage`), **capability-declared** verbs (`set_coordinate` per coordinate, `steer`, exact `account`, `export`, `elicit`; the `resume` mode), and an explicit **excluded** list whose members each guard T-LCD-06/-07 (`inspect_artifacts` is an environment-handle operation; policy, authority, credentials and budgets never enter the loop; `fork/compact/spawn` are observed only; no `inject_tool` after `open`; no `read_ledger`). The `HostedEvent` envelope carries a **mediation stamp** so that a participant *reporting* zero tool calls never scores perfect autonomy; ADR-0165 turns what the Lab can enforce into per-dimension `budget_enforcement` that the matched-budget engine now consumes (ADR-0041 amended), and ADR-0045 gains `requires_capabilities`/`requires_mediation` so ADR-0071's pseudo-class is retired. ADR-0166 supplies the handshake (`hh-hosting/1`), `ext`-first extension, adapters as debt-carrying records, the probe catalogue P-01…P-16 and the DRIFT rule (P0 quarantines; elsewhere annotates — OQ-034). The verbs map one-to-one onto ADR-0098 D6's ACP v2 stable core and lift onto ADR-0026 classes through a MUST-data table (tool-plane spellings follow OQ-235 — CF-389). **LCD battery:** T-LCD-06/-07/-11 recorded pass on paper for ADR-0164…0166 (`registers/lcd-test-battery.md` Phase 3 record); the T-07 hosting row now has its schema and probe suite. **Hosting stays strictly secondary:** the C0/Stage-3 items are schemas and a fixture over native runs, removable with the tier (CF-390).

### 2.2 The lab chain (brief focus 2)

```
author  → AssemblySource ──desugar──▶ compose → resolve(snapshot) → validate_assembly → seal   (ADR-0147/0148; kernel ops of ADR-0025)
        → apply/publish ──▶ registry record {sealed_definition, ValidationReport, snapshot, layers[]} (ADR-0151; M-1: no run without one)
design  → ExperimentSpec(levels from slot_choices; MatchSpec; PreRegistration) ──register──▶ refusal set incl. DependsOnDriftedCapability (ADR-0154/0152)
        → expand → CellPlan/RunPlan(run_plan_id) → open_experiment (run_kind = experiment; measurement.experiment.declared before any run_bound)
run     → claim(resource(run_plan_id)) → launch(eval_budget slice; fresh env; experiment layer) → subject run stamps measurement.experiment.bound{cell_id, replicate_index, attempt_no}
        → settle by outcome class (never delete; run_excluded{reason}; run_replanned) → close (drift bracket; bundle(kind = experiment))   (ADR-0155/0162)
score   → ResultsRow/1 = project(run, results_row, scoring) — versions under (configuration_version_id, run_id); cell table; annotation index → bundle_refs (ADR-0161)
analyse → analyze(AnalysisSpec, QuerySpec @ watermark_set) → AnalysisReport ──record_analysis──▶ AnalysisRecord{pre_registered | exploratory}  (ADR-0157/0158/0162; CF-388)
publish → RankReport sets → LeaderboardSnapshot (L1 no score without a validated bundle; L2 class strata; L4 matched budget or no rank) → publish as export (ADR-0163)
```

Every experiment result links to a bundle (I3) through the derived annotation index (CF-298 honoured); every score is class-labelled with a typed `n/a{reason ∈ {class, observability, capability, mediation, estimator_undefined, not_run, no_detector}}`. The four matched-budget enforcement points (`register`, `launch`, `settle`, `close`) plus report-time refusal make T-LCD-14 an engine property end to end. Both canonical exemplars (ADR-0077, ADR-0105) are registered `ExperimentSpec` documents at Stage-3 size (ADR-0156).

### 2.3 L5 ↔ J2 ↔ H5 (brief focus 3)

A plugin is an `ExtensionRecord{kind: plugin}` whose manifest is an inventory of `registry/1` contributions, each with its own identity and trust legs (ADR-0180 over ADR-0063); compatibility is declared over Core contract versions and checked before any executable is launched; `requests` are claims lowered to one `Permission` at `seal`. Third-party variants are `component_variant` extensions that bind only through the variant host (ADR-0181) at `placement ≥ subprocess_confined` (ADR-0153; CF-141); `placement` replaces the three "where does code run" vocabularies as the primary registration field with `locality` derived (CF-378); `host_unconfined` is narrowed to tool executables (CF-380; ADR-0063 amended). Hooks return `GuardVerdict`s composed by meet — never `allow`, never a mutated input (OQ-165 L5 half); ADR-0177's host `HookResult` is the same sum seen from an embedding host. The extension DAG is stated as invariants X1–X6 with a readiness-gate spec-DAG check (`tier_violations = []`, acyclicity) generalising T-LCD-06 and per-tier removability builds on the ladder (ADR-0182).

### 2.4 Every surface is a client of `hh-embed/1` (brief focus 4)

ADR-0176 completes the WS-L1 §6.5 contract into the single public kernel surface (Groups H/S/W/R/M/U, invariants I1–I9, the frame model, a closed error sum). Four dossiers independently found the same gap — the Lab and surface operations had no crossing (CF-322, CF-356, CF-359, CF-369) — and the synthesis resolved it once (CF-383): **Group L** carries every design-time and Lab operation record ratified this phase (assembly, registry, experiment, analysis, results, leaderboard, hosting `describe/probe/attach`, `serve`), Group W gains the branch operations, Group R the audit proofs; all generated from one schema source. The CLI binds in-process per ADR-0050 D1 (CF-384; ADR-0167 amended), the web surface over the local-network binding with `attach` for viewers, the MCP server's Lab tools wrap Groups S/M/L, the ACP artefact lowers Group R/U; AC-K4-2 fails a private verb. OQ-130 is answered: Q-L1-14 is not narrowed, ADR-0050 trigger 6 does not fire, the surface ecosystem stays late-bound for WS-K2 at Stage ≥ 5.

### 2.5 Technology neutrality (brief focus 5)

No surface or lab ADR commits to a language, runtime, UI framework, transport library or package (CF-392). Contracts are stated by transport properties (newline-delimited JSON-RPC, localhost HTTP/WebSocket, in-process typed channels with canonical envelopes) and reference ADR-0050 by id and layer only; every language mention is a precedent path or a quoted spec enum.

### 2.6 One ledger, one discriminator, one family (ADR-0183 §C)

Three dossiers proposed three manifest discriminators (CF-385) and two proposed two experiment event families (CF-386). The ruling: `run_kind ∈ {agent, experiment, surface, inbox}` (also closing OQ-314 for B3's inbox), `charged_to` orthogonal; one closed family `measurement.experiment.{declared, run_planned, …, bundle_assembled}` + `measurement.analysis.*` + `measurement.leaderboard.*` produced by the sweep engine and validated by the results store. Eleven separate ledger-addition requests are consolidated in one ADR-0026 Phase 3 log (CF-391) with Rule P intact.

---

## 3. Evidence discipline check (doc 3 §3, RK-03, RK-11)

- **Five fields:** 36/36 proposed ADRs carry (a)–(e) and a T-LCD statement (mechanical check at synthesis).
- **Provisional audit of C0 decisions:** the C0 slices of J1 (assembly validation codes), J2 (`registry/1`, conformance shapes), J3 (`ExperimentSpec`, experiment-as-run, settlement), J4 (estimator kernel — Bowyer et al. is ICML 2025 accepted, S-508; classics S-516…S-522), J5 (rows, catalogue, lab-internal view), J6 (`HostedEvent`/`proj_ABI` schemas), K1 (driver slice, attendance, exit classes), K3 (fixture server bindings), K4 (contract, frame model, error sum, injection table, stability policy), L5 (manifest, variant host, DAG) were read against the folded `sources.md` rows: the 2026 single-team rows added this phase (S-512, S-513, S-514, S-544–S-547) and S-523 (*The Leaderboard Illusion*), S-073 (Logos), S-079/S-140 (hosting), S-082 (Task-CoEvolve) are cited for mechanism existence, counter-evidence or a failure signature already implied by a ratified rule; every C0 decision rests on ratified ADRs, Tier-A theory/standards (AIP-180, RFC-class MCP/ACP text, Efron/Holm/BH/TOST/Manski, Hyperband, fractional-factorial theory) or source read at pinned commits (codex 0735c51, software-agent-sdk 3fc7b22, goose fae91d0, opencode 9f8db11, pi-mono 400d690, gemini-cli ed2ac40, omnigent 6a069da, harbor 7d5285b, inspect_ai 75f4891, hal-harness 16bb03e, helm 63754d0, terminal-bench d28711d, METR eval-analysis-public 52cb829, arena-hard-auto 196f6b8, lm-evaluation-harness ad8737ae, cloudflare/agents b9142be, agent-client-protocol 9b00c27, modelcontextprotocol aa8ce04, ext-tasks 9263312). No exception found.
- **Sources:** 65 new rows (S-499…S-563); 4 temp rows folded onto existing rows (S-302, S-370, S-371, S-165); path-level citations folded into 20 existing rows; 4 promotions S → P (S-073 for L5, S-119, S-370, and S-114/S-115 `P (Phase 2)` → `P`). S-055 remains unreachable (CF-024 stands).
- **Language-leak audit (CF-392) and naming audit (CF-393):** clean.

---

## 4. Register state after folding (id maps)

Temp ids in every dossier, sidecar and ADR were rewritten to final ids.

| WS | ADRs | sources | open questions | conflicts |
|---|---|---|---|---|
| WS-J1 | ADR-0147…0150 | S-499…S-501 | OQ-346…OQ-351 | CF-322…CF-323 |
| WS-J2 | ADR-0151…0153 | S-502…S-503 | OQ-352…OQ-358 | CF-324…CF-330 |
| WS-J3 | ADR-0154…0156 | S-504…S-507 | OQ-359…OQ-364 | CF-331…CF-336 |
| WS-J4 | ADR-0157…0160 | S-508…S-522 (09b → S-517, 10b → S-519) | OQ-365…OQ-371 | CF-337…CF-341 |
| WS-J5 | ADR-0161…0163 | S-523…S-531 | OQ-372…OQ-377 | CF-342…CF-347 |
| WS-J6 | ADR-0164…0166 | S-532…S-534 (temp 03 → S-107, 04 → S-132) | OQ-378…OQ-382 | CF-348…CF-352 |
| WS-K1 | ADR-0167…0169 | S-535…S-537 | OQ-383…OQ-387 | CF-353…CF-357 |
| WS-K2 | ADR-0170…0172 | S-538…S-550 (temp 12/14/15/16/17/18 → S-119/S-107/S-112/S-110/S-111/S-136) | OQ-388…OQ-392 | CF-358…CF-362 |
| WS-K3 | ADR-0173…0175 | S-551…S-556 (temp 07/08/09 → S-302/S-370/S-371; 10–15 → S-107/S-107/S-109/S-112/S-136/S-119) | OQ-393…OQ-400 | CF-363…CF-369 |
| WS-K4 | ADR-0176…0179 | S-557…S-559 (temp 03–06 → S-107/S-113/S-110/S-109) | OQ-401…OQ-406 | CF-370…CF-375 |
| WS-L5 | ADR-0180…0182 | S-560…S-563 (temp 05 → S-165) | OQ-407…OQ-412 | CF-376…CF-382 |
| synthesis | ADR-0183, ADR-0184 | — | — | CF-383…CF-393 |

Next ids: **S-564**, **OQ-413**, **CF-394**, **ADR-0185**. Ontology **v3**: 143 terms folded in §5d (status `ratified (v3, …)`), 8 vocabulary rulings in §5e, 10 canonical names in §6.

Pre-existing rows updated: CF-017 (resolved — K2/K4 exposure discharged), CF-023 (resolved — layering confirmed); 37 existing open questions resolved or advanced (list in `registers/open-questions.md` "Synthesis notes (Phase 3)").

---

## 5. Open items carried forward (owner · due · blocking?)

All Phase 3 conflicts are dispositioned (61 workstream-flagged: 50 resolved, 11 resolved as rejected precedent; 11 synthesis-raised: 10 resolved, 1 accepted with rationale — CF-390).

**Blocking Stage 1–3 (schema / kernel / lab):**
- OQ-235, OQ-240 (tool-plane event names; surface record fields) — WS-B1/E3/E1 · Stage 1; the hosted lifting table (ADR-0164) is re-keyed as data when they close.
- OQ-165 HIR half (hooks as guards on β vs `HarnessRule` actions) — WS-F1/A3 · Stage 1; the output sum is fixed (`GuardVerdict`).
- OQ-402 / OQ-131 (connection bounds from the S2 spike) — Stage 0.
- OQ-363 (may an `iso_cost` arm share subject runs with a `matched_cap` arm) — WS-L2 · **before the Stage-3 execution of `lab/control-strategy-family-v1`** (AC-J3-9).
- OQ-360 / OQ-366 / OQ-367 / OQ-375 (re-attempt defaults; draws; `T_clt`/`T_bca`; admission replicate floors) — Stage 3 measurement inputs (MUST-data placeholders in force).
- OQ-407 / OQ-075 (hot-path ceiling for out-of-process variants) — WS-L2/I1 · Stage 3 data.
- OQ-387 (`workspace_trust` narrowing rows) — WS-H1/H5 · Stage 2.

**Blocking Stage 4 (surfaces and hosting):** OQ-246 (approval persistence across an edge — K1/K4/J6 offer `allow_once`/`deny_once` only until it closes); OQ-116 (consent for hosted L2 content); OQ-242 (ACP v2 pin); OQ-378…OQ-381 (hosted `StopReason` lifting, placement/bridging, re-probe cadence, liveness deadlines); OQ-389 / OQ-392 / OQ-401 (token delivery; surface `SinkPolicy` defaults; non-local hosts); OQ-400 (supply-surface lifecycle); OQ-388 (index latency bound).

**Phase 4/5 (program-level):** OQ-347 (`base` layer source kind — WS-I7), OQ-350 (lineage drift policy — I5), OQ-382 (evolution over hosted coordinates — I5), OQ-352 / OQ-355 / OQ-356 / OQ-411 / OQ-412 (cross-registry revocation; suite adequacy; pipeline conformance; sunset defaults; partial trust), OQ-383 / OQ-384 / OQ-394 (naming and exit-code bands — L6), OQ-390 (annotations as records — L8), OQ-397 / OQ-398 / OQ-399 (tasks carrier alias; provider clients; rate gauge), OQ-408 / OQ-409 / OQ-410 (one-shot hooks; `read_view` coverage; component-model placement).

**Accepted tensions and notes:** CF-390 (hosting-tier C0/Stage-3 schemas — accepted with the removability test); dossier length (every Phase 3 dossier exceeds the 2.5–6k target because the verb, code, event and view tables *are* the deliverable — recorded, not fixed); the terminal-bench dashboard credential is cited as a negative precedent without the value (S-490 note).

**Risks:** RK-07 exercised and held; RK-09/RK-11 clean; RK-03 held; RK-05 advanced (the instrument is specified end to end); no new risk.

---

## 6. Settled for Phase 4 — binding on WS-F3, F4, F5, WS-I5, I6, I7, I8, WS-L8

Phase 4 dossiers cite these by ADR id. Each item names what Phase 4 may rely on and what it must not re-decide.

### 6.1 The measurement and lab contracts every Phase 4 workstream uses

- **Experiments are `ExperimentSpec` documents** (ADR-0154): six ratified factor kinds, arms with `eval_budget`/`search_budget`/`MatchSpec`, `PreRegistration` required unless `exploratory`, `expand` to a content-addressed plan, refusal at `register` on the closed set. WS-F4's scheduler and WS-I5's pipeline *author* specs; they do not run arms outside one. `adaptive_search` is the design kind for search (ADR-0156 decision 4): `disagreement_weighted`/`successive_halving` only over `search`/`dev` splits after a `SplitAssignmentRecord`, rows `estimated`; the frozen artifact's benefit is always a `full_set` held-out experiment.
- **The experiment is a run** (`run_kind = experiment`, ADR-0155/0162/0183): scheduler state is a projection of its ledger; claims are `resource(run_plan_id)` leases; settlement is by outcome class; re-attempts never delete; `run_excluded{reason}` is appended, never inferred. WS-F4 (value-of-compute) allocates *within* the experiment and instrument budget nodes (ADR-0040 `scope: Experiment`) and may re-order `OrderPlan` only under a recorded permutation seed with the drift bracket intact.
- **Results are rows** (ADR-0161): `ResultsRow/1` keyed `(configuration_version_id, run_id)`; counterfactual and regrade scorings are **overlay runs** producing row *versions*, never edits — WS-I7's counterfactual arms and WS-I5's candidate scorings enter this way; `comparable = false` rows never enter a comparison.
- **Analysis is `analyze`** (ADR-0157/0158/0159): pure over `cells @ watermark_set`; every claim is a `ComparisonReport` with `label`, `budget_match`, `benefit_kind`, `EstimatorSelection`, `outcome_bounds`; interactions are `contrast` (difference of paired differences) over probed cells — never interpolation; C4 gates (ADR-0046 §7) read intervals produced under the declared selection rule (no under-covering CLT at small n); `search_time_benefit` is computed against `oracle_best_of_n`/`selected_best_of_n` from stored replicates; transfer is a `TransferProfile` with sign stability over ≥ 2 families and ≥ 1 held-out level. WS-I6 owns `FittedSurfaceReport` debt records and their expiry (ADR-0160); profile learning consumes `conditionality_region` entries as *evidence refs only* (no rule is ever emitted by the engine).
- **Leaderboards** (ADR-0163): L1 no score without a validated reproducible object; L4 matched budget or no rank; M3 with a `selection{n_candidates_registered, rule, search_budget_ref}` record for any arm with search spend — the leaderboard form of WS-I5's search-vs-artifact separation; disclosure counts derive from the experiment ledger (every registered arm counts, restricted or not).
- **Registry** (ADR-0151…0153): the evolution service publishes only into `exp/<experiment_id>/` at `origin = model`, as non-widening `HirDiff`s applied through the assembly service (`apply(base, diff)`, ADR-0147 M-2); candidates that validate with errors are rejected with their codes (ADR-0148); registry reads are never model-facing (R-NM); third-party variants are out of process with declarations as claims until probed; every arm depending on a DRIFT-ed capability is refused.
- **Assembly** (ADR-0147/0149): every harness change WS-I5/I7 proposes is a `HirDiff` between sealed definitions, read back as an `AssemblyDiff` for human review; `origin = evolution` may never adopt L3 snapshot drift; `freeze` is the only default for opened arms.
- **Hosted participants** (ADR-0164…0166): configuration-level coordinates only, `SUPPORTED` before they are factors; comparisons stratify by `capability_vector`, `mediation` and cost `confidence`; `matched_cap` requires `enforced` dimensions (ADR-0041 amended); hosted rows never claim a security or autonomy property the Lab did not mediate. WS-I5 may search over hosted coordinates only under OQ-382's answer (Phase 4).

### 6.2 The security-kernel constraints the evolution service must respect (ADR-0051…0053, 0033…0035, 0063…0065 as amended; ADR-0177, 0181, 0182)

- **The evolution service is a threat actor and an attenuated child** (ADR-0053 D-5): it holds handles derived from the sealed definition, never mints authority; its edits are `HirDiff`s with `authority_delta ∈ {none, narrowing}` and `budget_delta ≠ loosening`; a widening requires `origin = human` with attestation (ADR-0017/0037; ADR-0147 S-6; ADR-0168 I-1).
- **Hooks and guards never `allow`** (ADR-0181 `GuardVerdict`; ADR-0177 `HookResult`; CF-121): an evolution-authored guard may narrow, annotate or propose a replacement that re-enters `resolve → authorize`; it cannot rewrite an approved proposal, substitute a result, or persist a grant.
- **Authority never flows up the DAG** (ADR-0182 X6): extension-contributed `HarnessRule`s carry `authority_cap` narrowing only; the evolution service's plugins live in `exp/` under the ADR-0053 D-5 handle set; `validate_assembly` refuses `AuthorityViolation`; the readiness gate refuses `tier_violations`.
- **No model-facing registry, no in-process third-party code** (ADR-0151 R-NM; ADR-0153; CF-141): a candidate may not bind a registry operation as a tool; any executable a candidate introduces runs out of process under a `ContainmentPolicy` with deny-by-default `requests` (ADR-0180).
- **Every crossing is one of three bindings** (Ontology v3 §5e): the evolution service drives runs only through `hh-embed/1` (Groups S/W/R/M/L), never through a private verb; its budgets are `slice`s under monotone containment; its spend is `charged_to = instrument` for search and `subject` for evaluated arms; approvals it needs route to a principal (`delegate` never endorses — ADR-0035, ADR-0174 mirror).
- **Provenance labels are fixed on entry** (ADR-0177 injection table): candidate text is `Text{authority ≤ external}`; candidate-authored context enters only as labelled leaves under `role_map(authority)`; nothing a candidate produces reaches `definition` authority except by a human `seal`.

### 6.3 Contracts Phase 4 must not re-decide

The embedding contract and its bindings (ADR-0176/0179 — a Phase 4 dossier needing an operation proposes it as `experimental` via ADR-0178, never as a new boundary); the Hosting ABI's verb lists (ADR-0164); `run_kind` and the experiment event family (ADR-0183 §C); the `registry/1` kinds and the three status axes (ADR-0151); the estimator selection rule and the `contrast` default (ADR-0158); the placement vocabulary and `host_unconfined` narrowing (CF-378/380); the results row key and the version rule (ADR-0161; ADR-0038); the leaderboard rules L1–L9 (ADR-0163).

### 6.4 Rules of the road for Phase 4 dossiers

1. Cite ADRs by number; Ontology v3 is the vocabulary tie-breaker; use the §5e canonical names (`hh-embed/1`, `plugin_abi/1`, `hh-hosting/1`, `run_kind`, `placement`, `GuardVerdict`, `mediation`, results store / experiment ledger / leaderboard definition).
2. Register additions go to a sidecar with temp ids (`S-WS-XX-NN`, `OQ-WS-XX-NN`, `CF-WS-XX-NN`); synthesis renumbers from S-564 / OQ-413 / CF-394; ADRs from ADR-0185.
3. Every ADR carries the five §3.3 fields, a T-LCD statement and ADR-0024 rungs; **every C4 decision carries matched-budget conditionality** (ADR-0002/0041/0046; T-LCD-14) and no C0 element rests on `provisional` evidence.
4. ADR-0050 §8 language-use constraint applies unchanged; CF-017 is closed — a Phase 4 dossier that needs runtime concreteness answers in contract terms.
5. Precedent file paths are evidence, never dependencies; every claim of production behaviour names a pinned commit.
6. A Phase 4 dossier that needs a Phase 1–3 amendment proposes it as a CF row for synthesis; it does not edit ratified ADRs.

---

## 7. Gate check (doc 3 §6; LEDGER Phase 3 row)

| criterion | result |
|---|---|
| All 11 workstreams `done` with ADRs dispositioned | **yes** — 36 ratified (14 amended), 0 rejected; LEDGER rows updated |
| OQ-004 resolved by a ratified ADR with the LCD-battery result | **yes** — ADR-0164/0165/0166; T-LCD-06/-07/-11 recorded pass on paper (`registers/lcd-test-battery.md`); hosting strictly secondary (CF-390); verbs map onto ADR-0098 D6 and ADR-0026 (CF-389) |
| Lab chain J1↔J2↔J3↔J5↔J4 composes end to end; every result linked to a bundle; every score class-labelled | **yes** — §2.2 / ADR-0183 §D |
| L5 consistent with J2 and H5; extension-DAG rule stated as spec invariants | **yes** — §2.3; ADR-0182 X1–X6; CF-378/380 |
| All surfaces (K1/K2/K3) build on K4's embedding contract; no surface bypasses it | **yes** — §2.4; CF-383/384; ADR-0176 as amended |
| No technology commitments | **yes** — CF-392 |
| Conflicts dispositioned | **yes** — CF-322…CF-393 |
| No C0 ADR rests on provisional evidence | **yes** — §3 |
| Naming audit | **clean** — CF-393 |

**Gate: passed.** The orchestrator commits at the phase boundary (RK-10).
