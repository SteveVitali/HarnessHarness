## 1. Vision, thesis, non-goals

*Authority: ADR-0001, 0003, 0004, 0005, 0006 (amended P0, P3), 0007 (amended P4), 0008, 0010, 0011; WS-A1, WS-L7; `registers/lcd-test-battery.md`; R-2.12.5. Vocabulary per `registers/ontology.md` §6; "MetaHarness" appears only in quoted historical text (ADR-0011).*

### 1.1 Vision

The sponsor's brief asked for "the most sophisticated, generic, modular, extensible meta-harness framework on the market" and the ability to "compare everyone's" harness. Doc 3 §1.1 declined the literal reading: maximal genericity is the **lowest-common-denominator (LCD) trap** — an abstraction admitting only the intersection of its targets' behaviours hides the model-conditioned features that drive performance and is routed around until vestigial (WS-L7 F1; S-154, S-152, S-035). The centre of gravity is **comparability under a portable behavioural representation** (ADR-0003).

Two Phase 0 narrowings complete the vision. "Compare everyone's" is not the differentiator: a comparison plane spanning controlled and uncontrolled participants exists in four independent systems, one Tier-A (Harbor S-132/S-133; HAL S-134/S-135; Inspect AI S-136/S-137; Omnigent S-118; Harness-Bench S-062) — ADR-0004 refuted the Preflight `novel` flag; ADR-0001's decision stands on precedent. And no mature coding harness imports a general-purpose framework for its core loop (S-074; WS-A1's reading of seven runtimes; CF-009 resolved), so competing on framework adoption has no whitespace (ADR-0003). What survives is **component-level harness science**: a white-box participant class decomposable through a typed IR, a laboratory that varies one component under matched budget, and integrated mechanisms cited against partial prior art.

HarnessHarness is therefore a **research instrument and reference runtime for harness engineering** (ADR-0003 decision 1). "Framework" never describes HarnessHarness; it names what others build on top (ADR-0008 rule 2). Adoption pressure sits on the IR's external edges — ACP, MCP, trajectory interchange, export — never on the loop (ADR-0003, ADR-0005). The reference runtime is opinionated, minimal and event-sourced; its purpose is instrument validity (ablation, attribution), not hosting third-party core loops (ADR-0003 decision 4).

### 1.2 The HarnessHarness thesis

ADR-0006 decision 1 adopts the WS-L7 §6.1 paragraph verbatim; it is reproduced with the product name substituted per ADR-0011, the only permitted change:

> **HarnessHarness is a research instrument and reference runtime for harness engineering, not another agent framework.** Its load-bearing capability is that any harness expressed in its behavioral Harness IR — a typed, provenance-bearing representation of all seven harness planes — can be assembled from swappable component variants, compiled per model profile and protocol target without loss of model-specific behaviour, executed on a deterministic runtime, and compared against any other harness (native or hosted) as an explicit model × harness × environment × budget configuration under shared evals. The instrument is what is new: existing frameworks give you *a* harness; existing leaderboards compare *opaque* harnesses; HarnessHarness makes the harness itself the decomposable, attributable, evolvable experimental variable, and proves the representation is real by running a reference harness on it.

"Deterministic runtime" in the thesis paragraph means that the reference runtime's decision path is a deterministic function of the run ledger and is replayable as such (ADR-0028; ADR-0135 — sources are substituted only in deterministic replay); model sampling, environment and served-model drift are not eliminated but recorded and classified by the reproducibility levels R0–R3 of ADR-0038 (§10.4), which is why every effect is paired and seeded (§2.5.6) and why attribution may be reported infeasible under nondeterminism (§1.4 (b); ADR-0004 (d)). The gloss is the spec's reading of the mandated sentence, not an edit to it (ADR-0006 decision 1).

The thesis has three parts, each tied to a test (WS-L7 §6.1; ADR-0006 decision 4): the *representation* is judged by T-LCD-04 (one definition → two Model Profiles × two targets) and T-LCD-03 (a reference harness round-trips); the *instrument* by T-LCD-09 (interaction effects) and T-LCD-14 (un-budgeted comparisons refused); the *mechanisms* by their own acceptance criteria plus reflexive expiry (T-LCD-05).

**Falsifiability (ADR-0006 decision 4).** If the reference harness cannot round-trip through the Harness IR (T-LCD-03), or one definition cannot compile to two profiles × two targets with semantics preserved (T-LCD-04), the IR is an LCD wrapper and the thesis fails. Both are C0/Stage 3 executable tests; the build ladder (§9) places them **before** profiles (Stage 5) and evolution (Stage 6) (ADR-0007 decision 2).

**Conditionality.** Holds for any model tier and task class; assumes models keep family-specific tool/prompt conditioning (S-107, S-110, S-145, S-049). If conditioning converges, the profile tier is descoped by ADR without touching the comparison claim, because T-LCD-05 applies to the Profile Compiler itself (ADR-0006 (d); ADR-0007 decision 6). No `provisional` performance number is load-bearing: the 2026 preprints corroborating "harness is a policy intervention with model-conditioned effects" (S-062, S-072, S-088, S-148) supply mechanism only, backed by Tier-B production reports (S-047, S-049) and pinned-commit source (doc 3 §3.1; ADR-0006 amendment log).

### 1.3 Two participant classes, one comparison plane, and the hosting stance

ADR-0001 (pre-ratified; evidence amended by ADR-0004, decision unchanged) fixes **native-primary, hosting-as-participant**:

| element | ratified content | source |
|---|---|---|
| White-box core | Harnesses in the Harness IR (HIR) on the reference runtime; components swapped, ablated, attributed, evolved | ADR-0001; ADR-0003 |
| **IR-native participant (white-box)** — *native participant* | θ is a Harness Definition known to the Lab; the compiled harness is transparent; all three comparison granularities admissible | ADR-0001; ADR-0008; ADR-0013; §2 |
| **Hosted-external participant (black-box)** — *hosted participant* | Joined through the **Hosting ABI** (the ratified *thin observational ABI*: start/resume/cancel · stream events · supply context/tools/skills · request permission · account cost); shares environments, evals, scorecards, cost/latency/audit; never decomposable; `configuration-level` (SUPPORTED coordinates only) and `product-level` granularity | ADR-0001; ADR-0004; ADR-0013; ADR-0164 |
| One comparison plane | One experiment/eval/scorecard/analysis surface for both classes; component-level analyses only for native participants; every metric class-scoped, inapplicable cells `n/a`, never 0 | ADR-0001; ADR-0004; ADR-0045; T-LCD-15 |
| Class declared, never inferred | The participant descriptor declares class, hosting mechanism, capability declaration, observability level; "grey-box" is a level, never a third class | ADR-0013; §2 |
| Hosting first-class but secondary | Hosting ABI = minimum observable event set + capability declaration, neutral across session-ABI, model-boundary interception, container-installed; session-ABI first | ADR-0001; ADR-0005; ADR-0164…0166; §6 |
| Interoperate, don't duplicate | Typed native run ledger; interchange export (ATIF-compatible or documented mapping) and `product-level` import; HarnessHarness can act as a participant in Harbor/Inspect/Omnigent-style planes via an ACP-style session | ADR-0005; ADR-0018; ADR-0098; ADR-0141; §6 |
| Conformance is data | Capability declaration reconciled against probes into a capability vector `{declared, probed, unknown}`; DRIFT stored as experimental data; `unknown` never coerced | ADR-0004; ADR-0166; T-LCD-07 |

**Two-step delivery (ADR-0010, mandatory wording).** *Cross-participant comparison is delivered in two steps — native-vs-native comparison at Stage 3 (evaluation-first), hosted participants joining the same plane at Stage 4 via the Hosting ABI, session-ABI mechanism first (ADR-0005).* External-harness hosting (R-2.10.6) is MoSCoW **Should** at tier C2 (ADR-0010; CF-014 resolved); readiness does not require any specific external harness to be integrated (ADR-0001). The C0/Stage-1 prerequisites that make the plane class-aware from the start — participant descriptor, class/observability stamps on every ledger event, `MetricDeclaration` — are **Must** by inheritance (ADR-0010 decision 3; ADR-0013; ADR-0026; ADR-0045).

**Conditionality.** Participants exposing none of the three mechanisms are excluded, never accommodated by loosening the native path; container-installed cost attribution carries lower, stamped confidence (ADR-0001/0005; ADR-0039; ADR-0165).

### 1.4 Differentiation: novel, precedented, renamed

Doc 3 §3.4 confined novelty to (i) the comparison-first ontology + IR, (ii) the lab, (iii) the doc 2 §12 mechanisms; two independent audits narrowed each identically (WS-A1 §5–§6; WS-L7 §5; CF-010/011/015). ADR-0006 decision 2 fixes the **only three novelty statements** this spec may make; any "novel" must name the prior art it is novel relative to.

| id | narrowed statement (ADR-0006 decision 2) | novel relative to | status after Phases 1–4 |
|---|---|---|---|
| (a) | First IR spanning all seven planes — `Permission`, `Effect`, `Validator`, `Budget`, `Memory` with validity, `HarnessRule` with assumption-debt fields alongside agent/tool/flow entities — with provenance and authority on every entity and **profile-compiled lowering**, designed for comparison | Open Agent Spec (S-138/S-139: versioned IR, six adapters, evaluation package — agent/tool/flow/LLM-config only); AutoGen `ComponentModel` (S-145); DSPy (S-144); AgentSquare (S-024); GPTSwarm/ADAS (S-020/S-023); HarnessFix HTIR, a *trace* IR (S-146); PRISM (S-093); LangGraph (S-143) | **CF-011 `resolved-conditional`.** WS-A3 condition discharged on paper at Phase 1 (ADR-0015/0016; ADR-0033); WS-C3/E2 condition at Phase 2 (ADR-0124…0126; ADR-0090/0091). **Executable proof remains T-LCD-03/-04 at Stage 3**; on failure the claim reverts to "renamed" (ADR-0006). |
| (b) | A lab that varies **one component under matched budget** across native participants and admits hosted participants on the same plane **without weakening native contracts** | Harbor (S-132), HAL (S-134/S-135, Tier A), Inspect (S-136/S-137), Omnigent (S-118), Pi eval tables (S-113) — opaque participants, no component variation or attribution | Specified (ADR-0147…0163) with T-LCD-09/-14/-15 as engine preconditions; hosting with T-LCD-06/-07 pass on paper (ADR-0164…0166); attribution ADR-0199…0201. If attribution proves infeasible under nondeterminism, the "causal attribution" clause is softened; ablation remains (ADR-0004 (d)). |
| (c) | The §12 mechanisms — IR + compiler, assumption-debt manager, safe adaptive optimizer, execution-alignment layer, trust-aware memory, capability kernel, benchmark science — as **integrated contracts with provenance and expiry** (`novel as integrated`) | DSPy adapters/optimizers; AutoGen `ModelInfo` (no evidence/expiry); CaMeL/Fides (S-099/S-101); HAL Pareto analysis; Meta-Harness, PRISM, Prime `/refine` — optimizers without a governed pipeline (WS-A1 F8) | Specified (ADR-0051…0053; ADR-0112…0114; ADR-0078…0083; ADR-0194…0201). **Evidence discipline:** every C3/C4 member carries `maturity ∈ {instrument-grade, research-grade}` and matched-budget conditionality (ADR-0194 decision 6; ADR-0208 §5; CF-463); `research-grade` results are `preview`-labelled and never ground a program claim. |

Further no-precedent items (WS-A1 §6.2) are claimed only in their owning sections: expirable profiles (§3; ADR-0020/0124); declared ⇄ observed conformance as a factor (§6; ADR-0166); the governed evolution pipeline (§5; ADR-0194/0195); the capability kernel (§5; ADR-0051); the assumption-debt manager (§5; ADR-0197).

**The "renamed" list (WS-A1 §6.3; ADR-0003 decision 3).** These exist elsewhere; the spec *cites and conforms* and claims only the typed content named. Every ⊕ NEW / ↑ EXT scope item carries a `precedent | renamed | novel` annotation in its owning section (WS-A1 §4c/§6.2/§6.3; ADR-0184, ADR-0209); the readiness report checks it.

| capability | renamed from | claimed |
|---|---|---|
| Assembly / Harness Definition (R-2.1.4, R-2.10.1) | Omnigent agent YAML, OpenHands `AgentProfile`, AutoGen `ComponentModel`, AgentCore/Agent Framework config, Agent Spec | typed component *variants*; definition is data (ADR-0023) |
| Hosting / Hosting ABI (R-2.10.6) | ACP, Omnigent `Executor`, Codex app-server, Harbor installed agents, Inspect bridge | **nothing** — `renamed`; cite and conform (ADR-0004/0005; ADR-0184) |
| Run ledger (R-2.2.1) | OpenHands `EventLog`, Pi entries, LangGraph checkpoints | what is typed on events: effects, provenance, authority, class stamps (ADR-0026) |
| Experiment engine (R-2.10.3) | Harbor `JobConfig`, HAL runner, Pi, agbench | factor set, cross-class joins, refusal set (ADR-0154; ADR-0184) |
| Results store / leaderboard (R-2.10.5) | Harbor Hub, HAL, OpenHands eval CDN | admission by validated object; class strata; matched budget or no rank (ADR-0161…0163) |
| Reproducible bundle (R-2.9.3) | Harbor trial dir + ATIF; Omnigent agent image | profile + permissions + policies + provenance + `LcdReport` (ADR-0139…0141) |
| Component families: compaction, critic/verification, control strategies, sub-agent orchestration | OpenHands condensers/`CriticBase`, Inspect, Pi, Harbor verifier, ReAct/FSM (LangGraph, Agent Spec), Prime RLM, Omnigent/Claude Agent SDK/AutoGen sub-agents | model-conditioned variants inside the contract (T-LCD-08); one verdict record, no default judge (ADR-0047/0110); the control boundary β as a factor (ADR-0012); one `spawn`, one `MergePolicy` (ADR-0185…0187; ADR-0208) |

### 1.5 Canonical non-goals N1–N13

ADR-0006 decision 3 (amended P0 by CF-019, P3 by CF-342) is the **only** non-goals list in this spec; ADR-0003's five map to N1, N2, N4, N3, N13.

| id | non-goal | rationale | specified instead in |
|---|---|---|---|
| **N1** | Not a general-purpose framework for other products' core loops | S-074; WS-A1 F5; CF-009/012 (ADR-0003) | §3 (IR edges); §7 embedding (R-2.11.4; ADR-0176) |
| **N2** | Not a universal Harness ABI others must implement; the Hosting ABI is thin, observational, ours, and never treats hosted participants as decomposable | doc 2 §5.11; ADR-0001; T-LCD-06/-07 | §6 (ADR-0164…0166) |
| **N3** | Not a hosted collaboration platform / managed multi-device hosting product | Omnigent S-118, Cloudflare S-056, AWS S-058 own that plane (CF-013) | §6 |
| **N4** | Not a public leaderboard service — *the Lab's own leaderboard definition + snapshot views over its results store, and their publication as ADR-0141 exports, are in scope (R-2.10.5; ADR-0163)* — hosting exists for joint native/hosted science and interoperation with existing planes (ADR-0005) | HAL S-135 is that service (CF-342) | §6 (ADR-0161…0163); §10 |
| **N5** | Not a new tool/agent/client protocol; MCP, A2A, ACP are lowering targets and Hosting-ABI substrates | doc 2 §5.10; S-035; S-153 | §3 (ADR-0021); §5 (ADR-0096…0099) |
| **N6** | Not behavioural interchangeability of models; one semantic definition compiled per Model Profile, never "works the same" unmeasured | doc 2 §4; S-049; S-047 | §3 (ADR-0020); §5 (ADR-0124…0126) |
| **N7** | Not a claim that generic beats bespoke; the Lab measures the gap; the reference harness may lose on bespoke home turf | S-075 (`provisional`); CF-009 | §6; §10 |
| **N8** | Not autonomous self-deployment; evolution proposes, governance deploys | ADR-0002 (i)–(iv); doc 2 §11 Stage 6 | §5 (ADR-0194/0195 G1–G10; ADR-0053 D-5) |
| **N9** | Not a model-training stack; HarnessHarness never trains | ADR-0002; ADR-0202 | §5 (ADR-0202…0204, `research-grade`; R-2.9.8 conditional, ADR-0209) |
| **N10** | Not an IDE, chat product or end-user assistant surface | doc 3 §2.11 | §7 (ADR-0167…0179) |
| **N11** | Not merely an evaluation harness; the object of study is runtime harnesses | doc 2 §1; ontology §4a | §2; §10 |
| **N12** | Not a language/runtime/framework commitment before WS-L1 — **discharged** by ADR-0050, whose §8 constraint governs every contract here (ecosystems named only by ADR id and layer) | RK-09; ADR-0009/0050 | §4; §8 |
| **N13** | Not a benchmark author; benchmarks are integrated, not invented | WS-A1 §6.1; ADR-0005; CF-019 | §5 (ADR-0142…0144); §10 |

Reversing N1 would re-open the LCD trap and is the reversal ADR-0006 makes deliberate; widening into N1, N2, N5, N6 or N12 requires a superseding ADR (phase-0 memo §5.1).

### 1.6 The LCD-trap battery as a binding design constraint

The stance *stable semantic interface + model-conditioned compilation* (doc 3 §1.2) is a check, not a principle. ADR-0007 adopts T-LCD-01…15 (WS-L7 §6.3, verbatim in `registers/lcd-test-battery.md`) as **binding acceptance criteria**. Failure signatures: **feature intersection**, **untyped escape hatches**, **silent loss on lowering** (S-154); antidote: **capability declaration + verified probing + typed extension slots + per-target compilation** (S-152, S-035, S-153). Binding rules (ADR-0007 decisions 1–6): each owning section carries an acceptance-criterion line encoding its tests, and the readiness report's 15-row matrix (test → section → criterion → stage → status) **cannot pass with any row "not encoded"**; the compiler exposes `lcd_report(harness_def, profiles[], targets[]) → LcdReport` and treats `UnexpressibleSurface(entity, profile)` as an **error**, never a warning; the engine refuses un-budgeted arms; `LcdReport` is stored in the bundle (§3, §6, §8; ADR-0139); any ADR introducing an abstraction spanning models, targets or participants states which tests it satisfies, and a "does not" without mitigation is grounds for rejection — every Phase 1–4 ADR that introduces an abstraction spanning models, targets or participants carries a T-LCD statement checked at its phase synthesis (application records in `registers/lcd-test-battery.md`; the reconciliation and scope-register ADRs — ADR-0048/0145/0183/0208, ADR-0049/0050, ADR-0146/0184/0209 — introduce none and are exempt), and the Phase 5 fixer pass found ADR-0057…0059 without one and supplied them by amendment log (2026-09-11); reflexively, the Profile Compiler and Hosting ABI carry assumption-debt records so convergence triggers documented descoping.

| test | criterion (must hold) | class · stage | discharged by |
|---|---|---|---|
| **T-LCD-01** Model-specific tool shape without escape hatch | One `ToolCapability` → profile-differing surfaces; zero per-model branches in the IR; divergence only in profile-owned fields | static · C0/S1 | ADR-0015/0016; ADR-0020; ADR-0124 (closed rule kinds); ADR-0090; ADR-0022 |
| **T-LCD-02** Opacity budget | Surfaces typed; free text only as a `Text` leaf with provenance/owner/authority; opacity ratio reported; leaves ablatable | static · C0/S1 | ADR-0015 (leaf kinds; ratio per OQ-036 with ADR-0045); ADR-0019; ADR-0124; ADR-0092 |
| **T-LCD-03** Behavioural round-trip | A Stage-0 reference harness in HIR reproduces its original within pre-registered equivalence | executable · C0/S3 | ADR-0015/0019 (`equivalence_run`); ADR-0022 E1–E7; ADR-0124 (null profile); ADR-0156 (`equivalence` kind); anchor per OQ-039 (ADR-0144) |
| **T-LCD-04** Two targets × two profiles | One definition → ≥2 profiles and ≥2 targets, semantics preserved by executable validators | executable · C0/S3 | ADR-0019/0020 (two minimal profiles); ADR-0021 (MCP C0/S3); ADR-0090; ADR-0124; ADR-0173 |
| **T-LCD-05** Every conditioned rule expires | Every profile-owned rule carries hypothesis, evidence, owner, expiry, removal test; refused otherwise; reflexive | contract · C0/S1–3 | ADR-0016; ADR-0020; ADR-0124/0126; ADR-0166; ADR-0197 (`DebtHomes/1`, typed `RemovalTest`); ADR-0198 |
| **T-LCD-06** Hosted participants never weaken native contracts | Hosting changes no native contract and is removable; no IR → Hosting-ABI edge | static · C0/S1 | ADR-0018 (`OpaqueProcess`); ADR-0025 (`hosting_edges = []`); ADR-0164/0166; ADR-0182 (removability builds); ADR-0205 |
| **T-LCD-07** Declare, verify, stratify — never intersect | Declared, probed, undeclared = UNKNOWN; features never removed; comparisons stratify | hosting · C2 | ADR-0164; ADR-0166 (`hh-hosting/1`, probes P-01…P-16, DRIFT — the T-07 row); ADR-0099; ADR-0152/0181 |
| **T-LCD-08** Component contracts see the profile | Every component-class contract receives Model Profile and resource accounting | contract · C0/S1–3 | ADR-0023; ADR-0124; ADR-0181 (`bind`); ADR-0185 |
| **T-LCD-09** Interaction effects representable | Model × harness × environment × budget × seed explicit; interactions reported, never one score | contract · C0/S1–3 | ADR-0045; ADR-0154; ADR-0158 (`contrast`); ADR-0161; ADR-0186 |
| **T-LCD-10** Names are surfaces, identities semantic | Identity content-addressed over semantics; rename is a profile diff | static · C0/S1 | ADR-0015; ADR-0017 (`SurfaceEdit`); ADR-0036; ADR-0124; ADR-0163 |
| **T-LCD-11** Lowering lossless-or-declared-lossy | Metadata through typed slots; lowering loss report; lifting recovers the rest | executable · C0/S3 | ADR-0019/0021; ADR-0018; ADR-0141; ADR-0164/0166; ADR-0173; ADR-0202 |
| **T-LCD-12** Contracts are operations, not inheritance | No subclassing/import of a framework object; implementable out of process | static · C0/S1 | ADR-0023/0025; ADR-0050 §8; ADR-0176 (`hh-embed/1`); ADR-0181 (`plugin_abi/1`) |
| **T-LCD-13** Compliance observable separately from validity | *delivered / activated / followed* events with artifact ids | executable · C0/S3 | ADR-0014; ADR-0026 (CF-101 identifiers); ADR-0042; ADR-0045; ADR-0125 |
| **T-LCD-14** No un-budgeted comparison | Arms without a matched budget refused; search-time and artifact benefit separated | contract · C0/S1–3 | ADR-0041; ADR-0046; ADR-0155; ADR-0157; ADR-0163 L4; ADR-0194 (`matched_total`) |
| **T-LCD-15** Class-scoped metrics | Every metric declares classes and observability levels; hosted component metrics `n/a`, never 0 | contract · C0/S1–3 | ADR-0013; ADR-0045 (`MetricDeclaration`, `n/a{reason}`); ADR-0165; ADR-0157; ADR-0161 |

Gate placement (ADR-0045; OQ-037): T-02/-09/-14/-15 are automated gates; T-03/-13 executable tests with review items. Static and contract tests are unconditional; executable tests assume the Stage 3 suite, run on the coding/terminal domain first, with T-03 margins pre-registered per suite (§10). The tests check expressibility, declaration and reporting, not performance; no `provisional` number is load-bearing (ADR-0007 (d)).

### 1.7 Naming and vocabulary

- **Product name: HarnessHarness** (ADR-0011; resolves OQ-035/CF-013). "MetaHarness" collided with a harness generator (S-149), an optimizer implementation (S-150), the Stanford *Meta-Harness* paper/repo (S-080/S-151) and Omnigent's "the open-source meta-harness" (S-118) — where "meta-harness" already means the hosting plane disclaimed by N3. "HarnessHarness" is the proper noun only: the runtime is *the reference runtime*, the instrument *the Harness Lab*; no sub-concept uses "meta-"; docs 1–3 and Phase-0 artifacts keep the old string as history; the glossary check flags any other residual.
- **Canonical names** (ADR-0008; Ontology §6): *Harness IR (HIR)*, dialects HIR/1, HIR/2 (never "HTIR"); *Harness Definition*; *native participant* / *hosted participant*; *comparison plane*; *Harness Lab* (never "Bench"); *Hosting ABI* (never "Harness ABI"); *Model Profile* / *Profile Compiler*; *target* / *lowering* / *lifting* / *lowering loss report*; *configuration* / *arm*; *component class* / *component variant* ("plugin" = third-party packaging); *conditioned rule* / *assumption-debt record*; *class-scoped metric*; *capability declaration* / *capability vector*; *LCD trap*, *escape hatch*, *opacity ratio*, *surface vs semantic identity*. First use expands; "framework" never describes HarnessHarness; brands are aliases; the glossary check fails on unregistered synonyms.
- [[DEFERRED ADR-0210 (OQ-035 follow-up): the public qualifier line for "HarnessHarness" and the collision re-check for the new name (repositories, packages, trademarks) ordered by ADR-0011 are WS-L6 Phase 5 deliverables; WS-L6 is `todo`, so no qualifier text or re-check result exists to cite. This section states the name and rules only.]]

### 1.8 Scope coverage and cross-references

| scope item | disposition here |
|---|---|
| **R-2.12.5** Thesis & naming (Must; WS-L7) | **Specified**: §1.2–§1.7 (qualifier line = WS-L6 gap) |
| R-2.10.6 Hosting (Should, C2) | Stance and two-step delivery §1.3; contracts §6 (ADR-0164…0166) |
| R-2.10.5 Results/leaderboard (Should, C1) | N4 boundary §1.5; contracts §6 (ADR-0161…0163) |

Obligations on other sections: §2 formalizes participant classes, granularity, observability level (ADR-0012/0013); §3 specifies `lcd_report`, `UnexpressibleSurface`, the lowering loss report, the two minimal profiles (ADR-0019…0022); every subsystem section carries its T-LCD criterion line and `precedent | renamed | novel` annotation; §6 marks hosting *secondary* and states black-box comparison is precedented (ADR-0004); §9 places T-LCD-03/-04/-11/-13 at Stage 3 before Stages 5–6 with the Stage-6 maturity sub-ladder (ADR-0194); §10 pre-registers T-LCD-03 margins; the readiness report carries the LCD matrix, glossary/annotation checks and the spec-debt register (ADR-0198).
