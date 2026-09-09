# WS-A1 — register additions sidecar

**Folded into `registers/*.md` at Phase 0 synthesis (2026-09-09); ids below are final.** Originally temporary; do not edit shared registers directly.

## Sources

| temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-132 | harbor-framework/harbor — "Harbor: a framework for evaluating and optimizing agents and models in container environments" (v0.22.0, 2026-08-22; audited commit 7d5285b 2026-09-09) | repo | B | 2 | P | no | https://github.com/harbor-framework/harbor | A1, J6, J3, J5, I3, I4 |
| S-133 | Harbor Adapters and Harbor-Index: Infrastructure and a Curated Meta-Dataset for Large-Scale Agentic Evaluation (2026) | paper | C | 4 | P | yes (mech) | https://arxiv.org/abs/2609.04298 | A1, J6, I4 |
| S-134 | princeton-pli/hal-harness (audited 16bb03e 2026-07-01) | repo | B | 2 | P | no | https://github.com/princeton-pli/hal-harness | A1, J3, J5, L2 |
| S-135 | Holistic Agent Leaderboard: The Missing Infrastructure for AI Agent Evaluation (ICLR 2026) | paper | A | 4 | P | no (accepted; single-team) | https://arxiv.org/abs/2510.11977 | A1, I2, J4, L2 |
| S-136 | UKGovernmentBEIS/inspect_ai (audited 75f4891 2026-09-09) — agent bridge / sandbox agent bridge | repo | B | 2 | P | no | https://github.com/UKGovernmentBEIS/inspect_ai | A1, J6, I4, H7 |
| S-137 | Inspect AI docs — Agent Bridge | doc | B | 2 | P | no | https://inspect.aisi.org.uk/agent-bridge.html | J6 |
| S-138 | Open Agent Specification (Agent Spec): A Unified Representation for AI Agents (Oracle, arXiv 2510.04173 v4, 2025-11) | paper | C | 0 | P | no | https://arxiv.org/abs/2510.04173 | A1, A3, A4, A5, L7 |
| S-139 | oracle/agent-spec — pyagentspec/tsagentspec + adapters (LangGraph, AutoGen, CrewAI, OpenAI Agents, WayFlow, MS Agent Framework) (audited 6f0b6ae 2026-08-31) | repo | B | 0 | P | no | https://github.com/oracle/agent-spec | A3, A4, A5, J6 |
| S-140 | ClawGym II: Exploring Black-Box RL on Agent Harness (2026-08) | paper | D | 4 | P | yes | https://arxiv.org/abs/2608.16798 | J6, I8, L2 |
| S-141 | Winder.AI — A Comparison of AI Agent Harnesses in 2026 (2026-08-20; vendor blog) | post | D | — | P | vendor | https://winder.ai/ai-agent-harness-comparison/ | A1, L7 |
| S-142 | AgentManifest: a declarative spec where the harness is the first-class decision (personal blog RFC v0.3, 2026-04) | post | D | — | P | yes (proposal only) | https://dev.to/mouserider/agentmanifest-a-declarative-spec-where-the-harness-is-the-first-class-decision-lnc | A5, L7 |
| S-143 | langchain-ai/langgraph (audited e539ac1 2026-09-09) | repo | B | 2 | P | no | https://github.com/langchain-ai/langgraph | A1, B1, B3, F1 |
| S-144 | stanfordnlp/dspy (audited ca54a85 2026-09-09) | repo | B | 0 | P | no | https://github.com/stanfordnlp/dspy | A1, A3, A4, I5 |
| S-145 | microsoft/autogen (audited 027ecf0 2026-04-06) | repo | B | 1 | P | no | https://github.com/microsoft/autogen | A1, A5, F3, F5 |

**Existing S-ids actually opened by WS-A1 (promote S → P):** S-012, S-013, S-030, S-031, S-035, S-045, S-056, S-058, S-059, S-062, S-072, S-074, S-079, S-080, S-083, S-093, S-107 (app-server-protocol source only), S-109, S-110, S-113, S-116, S-117, S-118. **Not opened:** S-055 (HTTP 403; substituted by S-107 source).

**Corrections to seeded rows:** S-118 Omnigent — the README now self-describes as "the open-source meta-harness" hosting Claude Code, Codex, Cursor, OpenCode, Hermes, Pi and custom agents; its bench is a *conformance* suite, not a performance benchmark (relevant to J6/J4). S-062 Harness-Bench — its evaluation setting is a Harbor-style shared environment with native execution preserved (relevant to I4).

## Open questions

| temp-id | question | resolver | due | blocking? |
|---|---|---|---|---|
| OQ-030 | Interchange format: adopt ATIF (Harbor RFC 0001) as the black-box trajectory export/import target, or define a documented superset with a lossless mapping for black-box rows? What compaction/subagent markers must it carry? | WS-J6 (with WS-B1, WS-I3) | Phase 3 | yes (J5 import rows) |
| OQ-031 | Should the Harness IR be exportable to Open Agent Spec (and importable from it) as an interop target, so Agent Spec's six runtime adapters become compile targets? What is lost (permissions, effects, validators, provenance)? | WS-A3 / WS-A4 | Phase 1 | no |
| OQ-032 | Is "an IR-native harness can run as a participant inside Harbor/Inspect/Omnigent-style planes via an ACP-style session interface" a C2 acceptance criterion (external validity of the reference runtime)? | WS-J6, WS-K4 | Phase 3 | no |
| OQ-033 | Cost attribution rules per hosting mechanism (measured model I/O vs participant-reported usage vs native-log reconstruction): how is confidence recorded and how do Pareto analyses treat mixed-confidence rows? | WS-L2, WS-J4 | Phase 1–3 | no |
| OQ-034 | Does a DRIFT conformance verdict exclude a participant version from comparisons or merely annotate it? Who re-probes and when (assumption-debt style expiry for declarations)? | WS-J6, WS-I6 | Phase 3 | no |

## Conflicts

| temp-id | parties | contradiction | proposed resolution |
|---|---|---|---|
| CF-010 | ADR-0001 evidence (b) vs Harbor (S-132/S-133), HAL (S-134/S-135), Inspect (S-136/S-137), Omnigent bench (S-118), Harness-Bench (S-062) | ADR-0001 flags the two-participant-class comparison plane `novel`; four independent systems already run controlled and uncontrolled participants on one environment/verifier/cost plane | Amend ADR-0001 (b) per ADR-0004: precedent cited; novelty narrowed to the component-decomposable white-box axis + metric class declarations + declared/observed conformance. Decision unchanged. |
| CF-011 | doc 3 §3.4 novelty claim (i) "comparison-first ontology + IR as a portable behavioral representation" vs Open Agent Spec (S-138/S-139), AutoGen `ComponentModel`, DSPy module state | A versioned declarative agent IR compiled to multiple runtimes with a standardized evaluation harness already exists | Narrow claim (i) to: IR entity set spanning all seven planes (Permission, Effect, Validator, Budget, Memory-with-validity, HarnessRule) with provenance on every entity and model-conditioned compilation. WS-A3 must deliver that set or the claim reverts to "renamed". WS-L7 to phrase accordingly. |
| CF-012 | CF-009 (S-074; audit of 7 runtimes) vs sponsor brief "most generic framework" | Mature harnesses hand-roll their loops and converge on external contracts; a general framework has no adoption path | Accepted tension resolved by ADR-0003: research instrument + reference runtime with explicit non-goals N1–N5; adoption pressure on IR edges (ACP/MCP/interchange/export), not on the loop. Mark CF-009 `resolved (ADR-0003)` pending synthesis. |
| CF-013 | Project name "MetaHarness" vs Omnigent README ("the open-source meta-harness for all your AI agents"; Databricks managed Omnigent, S-060) and doc 2 §5.11 usage of "meta-harness" for hosting layers | The term "meta-harness" is already used in the market for a *hosting/control layer over many harnesses* — a plane this project explicitly does not compete on (N4) | WS-L7 to decide naming/qualifier (e.g., emphasize "instrument/laboratory" framing) and to avoid claiming the hosting-product sense; cite Omnigent as the reference for that sense. |

## Ontology terms

| term | definition | notes |
|---|---|---|
| hosting mechanism | The technical means by which a hosted-external participant is attached to the comparison plane: **session-ABI** (start/resume/prompt/cancel + typed event stream + permission requests; ACP, Omnigent `Executor`, Codex app-server), **model-boundary interception** (a proxy or client patch that captures every model call; Inspect bridge, ClawGym II), or **container-installed** (vendor CLI installed in the task environment; trajectory reconstructed from native logs; Harbor). | Each mechanism yields a different observability level. Owner WS-J6; proposed by WS-A1. |
| observability level | The set of evidence a participant run can supply to the ledger: `events` (typed lifecycle/tool/permission events), `model_io` (complete model requests/responses), `end_state` (environment state + verifier), `ledger` (full IR-native typed ledger). IR-native participants supply all; hosted-external participants supply the subset their mechanism permits, declared per version. | Every metric declares the levels it requires; missing → `n/a`, never 0. |
| comparison granularity | The unit at which an experimental factor varies: `component` (an IR component variant/parameter — IR-native only), `configuration` (what the ABI lets us vary for any participant: model, context/tool/skill supply, permission policy, budget), `product` (a participant version as a whole). | Refines ADR-0001's "configuration-level, not component-level" note. |
| capability declaration | A typed, versioned record of what a participant claims to support (streaming, interrupt, steer, resume mode, fork, compaction, subagents, permission surface, instruction delivery, model family, effort vocabulary, trajectory export, native config). | Precedents: Omnigent `HarnessCapabilities`, Harbor `AgentCapabilities`. |
| observed conformance / drift | The result of probing a participant version against its capability declaration: SUPPORTED / UNSUPPORTED / PARTIAL / NOT_APPLICABLE / UNKNOWN / SKIPPED / **DRIFT** (declared ≠ observed). DRIFT is a fact about a participant version and is stored as experimental data. | Precedent: Omnigent `tests/harness_bench/verdict.py`. |
| trajectory interchange | A lossy, participant-class-neutral serialization of a run (steps with source/model/message/tool calls/observation/metrics; subagent references; final metrics) used to import/export runs across evaluation planes. Distinct from the native run ledger. | Precedent: Harbor ATIF (RFC 0001). |
| reference runtime | The opinionated, minimal, event-sourced runtime that executes IR-native harnesses so that white-box participants exist; its purpose is instrument validity (ablation/attribution), not adoption as a general framework. | Positioning term (ADR-0003). |
| research instrument | The assembly + experiment + comparison + analysis apparatus that treats model × harness (component/configuration/product) × environment × budget × seed as explicit factors and spans both participant classes. | Positioning term (doc 3 §1.1). |
