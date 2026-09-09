# WS-L7 — Register additions sidecar

**Folded into `registers/*.md` at Phase 0 synthesis (2026-09-09); ids below are final.** Originally temporary; nothing here edits the shared registers directly.

## Sources

| temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-135 | Holistic Agent Leaderboard (HAL): The Missing Infrastructure for AI Agent Evaluation (ICLR 2026); princeton-pli/hal-harness | paper+repo | A | — | P | no (accepted) | https://arxiv.org/abs/2510.11977 ; https://github.com/princeton-pli/hal-harness | L7, A1, J4, J5, J6, I2 |
| S-137 | Inspect AI (UK AISI) — Agent Bridge documentation (in-process and sandbox model-call interception) | doc | B | — | P | no | https://inspect.aisi.org.uk/agent-bridge.html | L7, J6, I2, I4 |
| S-146 | From Failed Trajectories to Reliable LLM Agents: Diagnosing and Repairing Harness Flaws (HarnessFix / HTIR), 2026-06 | paper | D | 4 | P | yes | https://arxiv.org/abs/2606.06324 | L7, A3, I7, I5 |
| S-147 | Seong et al., The Last Harness You'll Ever Build (meta-evolution blueprint), 2026-04 | paper | D | 4 | P | yes (counter to hand-designed instruments) | https://arxiv.org/abs/2604.21003 | L7, I5 |
| S-148 | Measuring Harness-Induced Belief Divergence in Multi-Step LLM Agents (BIWM), 2026-07 | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2607.04528 | L7, G2, I2 |
| S-149 | ruvnet/metaharness — "meta-harness" CLI/Studio that scaffolds branded agent harnesses (MIT) | repo | D | — | P | vendor/naming | https://github.com/ruvnet/metaharness | L7, L6 |
| S-150 | SuperagenticAI/metaharness — unofficial Meta-Harness implementation (FSL-1.1-ALv2) | repo | D | — | P | vendor/naming | https://github.com/SuperagenticAI/metaharness | L7, L6, I5 |
| S-151 | stanford-iris-lab/meta-harness — reference code for S-080 | repo | C | 4 | P | yes | https://github.com/stanford-iris-lab/meta-harness | L7, I5 |
| S-152 | Language Server Protocol specification 3.17 (capabilities, `experimental`, `$/` methods) | spec | A | — | P | no | https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/ | L7, J6, E4, L5 |
| S-153 | Model Context Protocol specification 2026-07-28 (repo `modelcontextprotocol/modelcontextprotocol`; `_meta` prefixed keys, capabilities) | spec | B | 2 | P | no | https://github.com/modelcontextprotocol/modelcontextprotocol | L7, E4, K3, A4 |
| S-154 | "Write once, run anywhere" / AWT peer model — lowest-common-denominator history (Wikipedia; Morelli & Walde "From AWT to Swing", LibreTexts) | doc | C (historical) | — | P | no | https://en.wikipedia.org/wiki/Write_once,_run_anywhere ; https://eng.libretexts.org/Bookshelves/Computer_Science/Programming_Languages/Java_Java_Java_-_Object-Oriented_Programming_(Morelli_and_Walde)/13%3A_Graphical_User_Interfaces/13.02%3A_Java_GUIs-_From_AWT_to_Swing | L7, A3, J6 |
| S-144 | stanfordnlp/dspy (repo; Signature/Adapter/teleprompt) | repo | B | 0 | P | no | https://github.com/stanfordnlp/dspy | L7, A3, A4, C3, I5 |
| S-145 | microsoft/autogen (repo; `autogen_core.models.ModelInfo`) | repo | B | 1 | P | no | https://github.com/microsoft/autogen | L7, A1, C3 |
| S-143 | langchain-ai/langgraph (repo; `StateGraph`) | repo | B | — | P | no | https://github.com/langchain-ai/langgraph | L7, A1, F1 |

Existing S-ids actually opened by WS-L7 (promote S → P): S-013 (via repo S-144), S-024, S-035 (schema v2 + docs v2 read in source), S-045, S-047, S-049, S-056, S-058, S-059, S-062, S-072, S-074, S-079, S-080, S-088, S-093, S-107, S-110, S-113, S-118, S-130, S-131.

## Open questions

| temp-id | question | resolver | due | blocking? |
|---|---|---|---|---|
| OQ-035 | The product name "MetaHarness" collides with ruvnet/metaharness, SuperagenticAI/metaharness, the Stanford Meta-Harness paper and Omnigent's self-description. Retain unqualified, retain with a public qualifier ("MetaHarness — harness laboratory & reference runtime"), or rename? | sponsor via WS-L6 | Phase 5 | no |
| OQ-036 | Exact definition of the **opacity ratio** (T-LCD-02): token-based vs entity-based; whether tool descriptions count as typed surfaces or text leaves. | WS-A3 with WS-I2 | Phase 1 | no |
| OQ-037 | Which T-LCD tests become automated gates in the build (compiler/engine preconditions) vs. review checklists in the readiness report? | Phase 1 synthesis; WS-I2 | Phase 1 / Phase 5 | no |
| OQ-038 | Should the Hosting ABI offer an optional **model-call interposition** capability (Inspect-style proxying of a hosted participant's model calls) as a declared "grey-box" depth — giving per-call cost/latency and model-override without pretending to decompose the participant? Feeds OQ-004. | WS-J6 with WS-C1 | Phase 3 | no |
| OQ-039 | Which reference harness anchors T-LCD-03 (behavioural round-trip): a mini-SWE-agent-class Stage-0 baseline only, or additionally a richer open harness (OpenHands SDK / pi) once the IR matures? | WS-A3, WS-I4 | Phase 1 (baseline) / Phase 3 (richer) | no |

## Conflicts

| temp-id | parties | contradiction | proposed resolution |
|---|---|---|---|
| CF-014 | doc 3 §1.1 thesis + sponsor brief ("compare everyone's") vs scope.md R-2.10.6 (hosting = C2 / Could) | The thesis headlines cross-participant comparison, but the only mechanism that admits external participants is MoSCoW "Could". A reader of the spec may see the headline capability as optional. | Keep tier C2 and ADR-0001 "secondary"; synthesis to consider raising R-2.10.6 MoSCoW to *Should* (scope-change log entry) OR to state in Spec §1 that cross-participant comparison is delivered in two steps (native-vs-native first, hosted second). WS-L7 recommends the latter wording plus the MoSCoW bump; decision is synthesis's. |
| CF-015 | doc 2 §12 row 1 / doc 3 §3.4 (i) ("comparison-first ontology + IR" as novel) vs prior art DSPy Signature/Adapter (S-013, S-144), AgentSquare uniform-IO modules (S-024), GPTSwarm/ADAS graphs (S-020, S-023), HTIR (S-146), PRISM edit surfaces (S-093), LangGraph StateGraph (S-143) | "A framework-neutral declarative representation for harnesses" is partially preceded on every plane individually. | Narrow the novelty claim in Spec §1 to: "first IR spanning all seven planes with provenance/authority on every entity and profile-compiled lowering, designed for comparison" — and treat it as an engineering claim to be demonstrated by T-LCD-03/-04, not asserted. WS-A1 to confirm or extend the prior-art list. |
| CF-009 (existing) | doc 2 §7 source-code study vs framework aspirations | Addressed in WS-L7 §5 and §6.5: MetaHarness is an instrument + reference runtime (non-goal N1); S-074's prose→configuration migration and "harness hosting" role are movement toward, not away from, the proposed representation. | Mark CF-009 `resolved` on ratification of ADR-0006 (pending WS-A1 concurrence). |

## Ontology terms

| term | definition | notes |
|---|---|---|
| **LCD trap** | The failure mode of a portability abstraction that admits only the intersection of its targets' behaviours, so target-specific behaviour that drives quality becomes reachable only through escape hatches, after which the abstraction is routed around and decays. Signatures: feature intersection; untyped escape hatches; silent loss on lowering. Antidote: capability declaration + verified probing + typed extension slots + profile compilation. | proposed by WS-L7; sources S-152/-12, S-035, S-118 |
| **escape hatch** | Any IR/ABI field or path that lets a definition bypass the typed, profile-owned route to a model- or target-specific surface (e.g., a raw schema override). Presence in the IR is a T-LCD-01 failure. | proposed |
| **opacity ratio** | Fraction of a compiled model-facing surface (by tokens, pending OQ-036) that originates from untyped text leaves rather than typed IR entities. Reported per harness definition and stored in the bundle. | proposed; metric owner WS-I2 |
| **surface vs semantic identity** | A capability/procedure/rule's *identity* is content-addressed over its semantics (effects, preconditions, permissions, validators); its *surface* (model-facing name, description, schema layout, error format) is a versioned, profile-owned rendering. Renames are profile edits. | proposed; WS-A3/L4 |
| **Harness IR (HIR)** | Canonical name of the typed, behavioural, provenance-bearing representation of a harness across all seven planes (doc 2 §11 entities/edges); versioned dialects HIR/1, HIR/2… | proposed; WS-A3 arbitrates the typed vocabulary |
| **Harness Definition** | A declarative artefact in HIR that names component variants, parameters, profiles and policies for one assemblable harness; the unit WS-J1 validates and WS-J2 registers. | proposed |
| **native participant** / **hosted participant** | Canonical short-forms of the ratified ADR-0001 classes *IR-native participant (white-box)* and *hosted-external participant (black-box)*. Definitions unchanged. | proposed short-forms; does not amend ADR-0001 |
| **Harness Lab (the Lab)** | Canonical name of the meta-harness laboratory (doc 3 §2.10): assembly, component-variant registry, experiment engine, comparison engine, results ledger/leaderboard, hosting. | proposed |
| **Hosting ABI** | Canonical name of the ratified *thin observational ABI* through which hosted participants join the comparison plane. Named for its purpose (hosting), deliberately not "Harness ABI". | proposed; depth owned by WS-J6 (OQ-004) |
| **Model Profile / Profile Compiler** | A versioned, expirable object holding every model-conditioned surface decision (prompt layout, tool shapes/names, error formats, compaction/reminder policy, interaction modes) for one model family/version; the compiler that applies it during lowering. | doc 2 §11 term, canonicalised; WS-C3 |
| **target / lowering / lifting / lowering loss report** | *Target*: a protocol or runtime a Harness Definition is compiled to (native runtime, MCP, A2A, ACP, provider tool API). *Lowering*: compiling HIR to a target under a profile. *Lifting*: recovering HIR-level metadata from a target artefact. *Lowering loss report*: the compiler's declaration of metadata the target cannot carry (T-LCD-11). | proposed; WS-A4 |
| **component class / component variant** | A component class is a Core-defined contract (operations/inputs/outputs/invariants/failure modes) for one pluggable slot; a component variant is one implementation registered under that class. Every class contract receives the Model Profile and resource accounting (T-LCD-08). | proposed; WS-A5/J2/L5 |
| **conditioned rule / assumption-debt record** | A conditioned rule is any profile-owned rule that exists because of a hypothesised model deficiency; its assumption-debt record carries hypothesis, evidence refs, owner, expiry condition, removal test and status. Mandatory on every conditioned rule (T-LCD-05). | ontology has "assumption debt"; this names the unit; WS-I6 owns the manager |
| **configuration vs arm** | *Configuration*: one point in the factorial (model × harness def × profile × environment × budget × seed) — extends the ratified "model–harness–environment configuration". *Arm*: a set of configurations run under one experimental hypothesis with a matched budget. | proposed; WS-J3 |
| **class-scoped metric** | A scorecard metric whose descriptor declares the participant classes it applies to; not applicable ⇒ N/A, never 0 (T-LCD-15). | proposed; WS-J4/I2 |
| **capability vector** | A hosted participant's declared feature set (interrupt, streaming, resume, context-supply, tool-supply, permission-elicitation, cost-accounting, model-override…) verified by probes; undeclared ⇒ UNKNOWN. | proposed; precedent S-118 `harness_capabilities.py`; WS-J6 |
