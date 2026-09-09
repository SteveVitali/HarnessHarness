# Source Registry

**Program:** MetaHarness research/synthesis (doc 3 §4, §11.2 step 3).
**Seeded:** 2026-09-09 from doc 2 (`2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md`) — its full Sources section, all unique footnote citations, and the Tier 0–4 syllabus. Doc 1 contributed no sources absent from doc 2.
**Maintained by:** every workstream appends rows (`S-###`) as it adds sources; synthesis passes update `provisional?` when corroboration lands.

## Column semantics

| Column | Meaning |
|---|---|
| `id` | Stable identifier `S-###`; cite as `[S-042]` in dossiers/ADRs. |
| `kind` | `paper` / `repo` / `spec` / `post` / `doc` (vendor docs) / `bench` (benchmark paper+suite). |
| `tier` | Doc 2 evidence hierarchy: **A** peer-reviewed/accepted or mature benchmark; **B** primary production engineering report or mature OSS implementation; **C** reproducible preprint with code / unusually clear method; **D** very fresh unreplicated preprint or vendor-specific claim. Syllabus tier (0–4) recorded in `syllabus`. |
| `primary/secondary` | `P` = read directly in this program; `S` = cited via doc 2 only until a workstream reads it. All rows start `S` except where noted. |
| `provisional?` | `yes` for every 2026 single-team/preprint result (§3.1). **A `yes` row may never be load-bearing for a C0 decision** without corroboration or independent mechanism re-derivation. `mech` = mechanism evidence acceptable, performance claims provisional. |
| `feeds` | Workstreams most likely to consume it. |

---

## A. Foundational & pre-harness research (Tier 1 syllabus)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-001 | Vaswani et al. (2017), Attention Is All You Need | paper | A | 1 | S | no | https://arxiv.org/abs/1706.03762 | context only |
| S-002 | Brown et al. (2020), Language Models are Few-Shot Learners (GPT-3) | paper | A | 1 | S | no | https://arxiv.org/abs/2005.14165 | A2 |
| S-003 | Lewis et al. (2020), Retrieval-Augmented Generation | paper | A | 1 | S | no | https://arxiv.org/abs/2005.11401 | D3 |
| S-004 | Nakano et al. (2021), WebGPT | paper | A | 1 | S | no | https://arxiv.org/abs/2112.09332 | F1 |
| S-005 | Karpas et al. (2022), MRKL Systems | paper | C | 1 | S | no | https://arxiv.org/abs/2205.00445 | F1, E1 |
| S-006 | Yao et al. (2022/23), ReAct | paper | A | 1 | S | no | https://arxiv.org/abs/2210.03629 | F1 |
| S-007 | Gao et al. (2023), PAL: Program-aided Language Models | paper | A | 1 | S | no | https://arxiv.org/abs/2211.10435 | F1, G1 |
| S-008 | OpenAI (2022), Introducing ChatGPT | post | B | — | S | no | https://openai.com/index/chatgpt/ | context only |
| S-009 | Schick et al. (2023), Toolformer | paper | A | 1 | S | no | https://arxiv.org/abs/2302.04761 | E1 |
| S-010 | Shinn et al. (2023), Reflexion | paper | A | 1 | S | no | https://arxiv.org/abs/2303.11366 | D3, G3 |
| S-011 | Wang et al. (2023), Voyager | paper | A | 1 | S | no | https://arxiv.org/abs/2305.16291 | D5 |
| S-012 | Wu et al. (2024), AutoGen (COLM 2024) | paper | A | 1 | S | no | https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/ | A1, F3 |
| S-013 | Khattab et al. (2023/24), DSPy | paper | A | 0 | S | no | https://arxiv.org/abs/2310.03714 | A1, A3, A4 |
| S-014 | Yang et al. (NeurIPS 2024), SWE-agent (ACI) | paper | A | 1 | S | no | https://arxiv.org/abs/2405.15793 | E1, E2, I4 |
| S-015 | Zhang et al. (ICLR 2025), AFlow | paper | A | 0 | S | no | https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html | A3, I5 |
| S-016 | Packer et al. (2023), MemGPT | paper | C | 4 | S | no | https://arxiv.org/abs/2310.08560 | D3 |
| S-017 | Park et al. (UIST 2023), Generative Agents | paper | A | 4 | S | no | https://arxiv.org/abs/2304.03442 | D3 |

## B. Tier 0 — systems prehistory & the optimization lineage

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-018 | Yang et al. (ICLR 2024), Large Language Models as Optimizers (OPRO) | paper | A | 0 | S | no | https://arxiv.org/abs/2309.03409 | I5 |
| S-019 | Fernando et al. (ICML 2024), Promptbreeder | paper | A | 0 | S | no | https://arxiv.org/abs/2309.16797 | I5 |
| S-020 | Zhuge et al. (ICML 2024), GPTSwarm: Language Agents as Optimizable Graphs | paper | A | 0 | S | no | https://proceedings.mlr.press/v235/zhuge24a.html | A3, I5 |
| S-021 | Yuksekgonul et al. (2024), TextGrad | paper | C | 0 | S | no | https://arxiv.org/abs/2406.07496 | I5 |
| S-022 | Yuan et al. (2024), EvoAgent | paper | C | 0 | S | no | https://arxiv.org/abs/2406.14228 | I5, F3 |
| S-023 | Hu, Lu & Clune (ICLR 2025), Automated Design of Agentic Systems (ADAS) | paper | A | 0 | S | no | https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html | A3, I5 |
| S-024 | Shang et al. (2024), AgentSquare | paper | C | 0 | S | no | https://arxiv.org/abs/2410.06153 | A3, J2, I5 |
| S-025 | FoundationAgents, AFlow code | repo | B | 0 | S | no | https://github.com/FoundationAgents/AFlow | A3, I5 |
| S-026 | Zhang et al. (2025; ICLR 2026), Darwin Gödel Machine | paper | A | 0 | S | no | https://arxiv.org/abs/2505.22954 | I5 |
| S-027 | jennyzzt/dgm — Darwin Gödel Machine reference implementation | repo | B | 0 | S | no | https://github.com/jennyzzt/dgm | I5 |
| S-028 | Agrawal et al. (2025; ICLR 2026 Oral), GEPA | paper | A | 0 | S | no | https://arxiv.org/abs/2507.19457 | I5 |
| S-029 | Wang et al. (2025), Maestro | paper | C | 0 | S | no | https://arxiv.org/abs/2509.04642 | I5 |
| S-030 | Mei et al. (COLM 2025), AIOS: LLM Agent Operating System | paper | A | 0 | S | no | https://arxiv.org/abs/2403.16971 | A1, A2, B5, L2 |
| S-031 | agiresearch/AIOS | repo | B | 0 | S | no | https://github.com/agiresearch/AIOS | A1, B5, L2 |

## C. Tier 2 — production harness engineering (posts, docs, protocols)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-032 | Anthropic (2024), Building effective agents | post | B | 1 | S | no | https://www.anthropic.com/engineering/building-effective-agents | F1 |
| S-033 | Anthropic (2024), Introducing the Model Context Protocol | post | B | 2 | S | no | https://www.anthropic.com/news/model-context-protocol | E4, K3 |
| S-034 | Google (2025), Announcing the Agent2Agent Protocol (A2A) | post | B | 2 | S | no | https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/ | E4 |
| S-035 | Agent Client Protocol (ACP) | spec | B | 2 | S | no | https://github.com/agentclientprotocol/agent-client-protocol | E4, J6, K4 |
| S-036 | Anthropic (2025), How we built our multi-agent research system | post | B | 2 | S | no | https://www.anthropic.com/engineering/multi-agent-research-system | F3, F5 |
| S-037 | Aizawa et al. / Anthropic (2025), Writing effective tools for agents | post | B | 2 | S | no | https://www.anthropic.com/engineering/writing-tools-for-agents | E1, E2 |
| S-038 | Anthropic (2025), Effective context engineering for AI agents | post | B | 2 | S | no | https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | D1, D2 |
| S-039 | Anthropic (2025), Equipping agents for the real world with Agent Skills | post | B | 2 | S | no | https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills | D5, H5 |
| S-040 | Anthropic (2025), Introducing advanced tool use (deferred loading, tool search) | post | B | 2 | S | no | https://www.anthropic.com/engineering/advanced-tool-use | E3 |
| S-041 | Anthropic (2025), Effective harnesses for long-running agents | post | B | 2 | S | no | https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents | B3, D3 |
| S-042 | Bolin / OpenAI (2026-01-23), Unrolling the Codex agent loop | post | B | 2 | S | no | https://openai.com/index/unrolling-the-codex-agent-loop/ | F1, F2 |
| S-043 | Hashimoto (2026-02-05), My AI Adoption Journey | post | B | 2 | S | no | https://mitchellh.com/writing/my-ai-adoption-journey | A1, I6 |
| S-044 | OpenAI (2026-02-11), Harness engineering: leveraging Codex in an agent-first world | post | B | 2 | S | no | https://openai.com/index/harness-engineering/ | G1, A1 |
| S-045 | Trivedy / LangChain (2026-03-10), The Anatomy of an Agent Harness | post | B | 2 | S | no | https://www.langchain.com/blog/the-anatomy-of-an-agent-harness | A1, A2 |
| S-046 | Rajasekaran / Anthropic (2026-03-24), Harness design for long-running application development | post | B | 2 | S | no | https://www.anthropic.com/engineering/harness-design-long-running-apps | G1, G3 |
| S-047 | Anthropic (2026-04-08), Scaling Managed Agents: Decoupling the brain from the hands | post | B | 2 | S | no | https://www.anthropic.com/engineering/managed-agents | B1, B5, I6 |
| S-048 | Kotliarskyi, Zhu & Brock / OpenAI (2026-04-27), Symphony (post) | post | B | 2 | S | no | https://openai.com/index/open-source-codex-orchestration-symphony/ | B3, F3, L-org |
| S-049 | Heule & Katz / Cursor (2026-04-30), Continually improving our agent harness | post | B | 2 | S | no | https://cursor.com/blog/continually-improving-agent-harness | C3, E2, I6, I2 |
| S-050 | Katz / Cursor (2026-01-06), Dynamic context discovery | post | B | 2 | S | no | https://cursor.com/blog/dynamic-context-discovery | D1, D2, E3 |
| S-051 | Cursor (2026-02-12), Expanding our long-running agents research preview | post | B | 2 | S | no | https://cursor.com/blog/long-running-agents | B3 |
| S-052 | Ma / Cursor (2026-06-02), What we've learned building cloud agents | post | B | 2 | S | no | https://cursor.com/blog/cloud-agent-lessons | B3, B5, F3 |
| S-053 | McGuinness et al. / Anthropic (2026-05-25), How we contain Claude across products | post | B | 2 | S | no | https://www.anthropic.com/engineering/how-we-contain-claude | H3, H4, H7 |
| S-054 | Cursor (2026-08-19), Cloud Agents and Cursor Harness Improvements (changelog) | post | B | 2 | S | no | https://cursor.com/changelog/08-19-26 | B3, F3 |
| S-055 | OpenAI (2026), Unlocking the Codex harness: App Server | post | B | 2 | S | no | https://openai.com/index/unlocking-the-codex-harness/ | J6, K4 |
| S-056 | Cloudflare (2026-06-03), Harnesses (Agents SDK docs) | doc | B | 2 | S | no | https://developers.cloudflare.com/agents/harnesses/ | A2, B3, J6 |
| S-057 | Gauvin / Cloudflare (2026-06-17), Bringing more agent harnesses and frameworks to Cloudflare (Flue) | post | B | 2 | S | no | https://blog.cloudflare.com/agents-platform-flue-sdk/ | J6 |
| S-058 | AWS (2026-06-17), AgentCore harness generally available | post | D | 2 | S | vendor | https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/ | A1, J6 |
| S-059 | Microsoft (2026-07-22), Agent Framework Harness released | post | D | 2 | S | vendor | https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/ | A1, J6 |
| S-060 | Databricks (2026-09-02), Omnigent on Databricks | doc | D | 2 | S | vendor | https://docs.databricks.com/aws/en/omnigent/ | J6 |

## D. Tier 3 — 2026 research frontier

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-061 | Lin et al. (2026-04/05), Agentic Harness Engineering (AHE) | paper | C | 3 | S | yes | https://arxiv.org/abs/2604.25850 | I5, I7 |
| S-062 | Yao et al. (2026-05-27), Harness-Bench | bench | C | 3 | S | yes (mech) | https://arxiv.org/abs/2605.27922 | I2, I4, G2, J4 |
| S-063 | Pan et al. (2026), Retrospective Harness Optimization (RHO) — Findings of EMNLP 2026 | paper | A | 3 | S | yes (accepted; single-team) | https://arxiv.org/abs/2606.05922 | I5 |
| S-064 | Lumer et al. (2026-06-11), Recursive Agent Harnesses | paper | D | 3 | S | yes | https://arxiv.org/abs/2606.13643 | F3 |
| S-065 | Qi et al. (2026-06), LLM-as-Code: Agentic Programming for Agent Harness | paper | D | 3 | S | yes | https://arxiv.org/abs/2606.15874 | F1 |
| S-066 | Huang et al. (2026-07-14), MemoHarness | paper | D | 3 | S | yes | https://arxiv.org/abs/2607.14159 | I5, D3 |
| S-067 | Huang et al. (2026-08-10), Evo-Bench | bench | D | 3 | S | yes | https://arxiv.org/abs/2608.09096 | I5 |
| S-068 | Luthra et al. (2026-08-11), Evaluating Agentic Learning Harness Capabilities Without Labels | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.13608 | I2 |
| S-069 | Karten et al. (2026-08-24), Prime Agent: A Self-Improving RLM Harness | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.23552 | F3, B3 |
| S-070 | Dhage (2026-08-25), Harness Engineering for Predictable Agentic Systems | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.26197 | F1, F2 |
| S-071 | Zhang et al. (2026-08-26), JIT-Agent | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.25593 | I5 |
| S-072 | Lewis (2026-08-26), Same Model, Different Harness | paper | D | 3 | S | yes (mech) | https://arxiv.org/abs/2608.26218 | I2, J4 |
| S-073 | Jia et al. (2026-08-28), Logos: An Agent Harness on a Cross-Process Bus | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.28553 | B1, B3, L5 |
| S-074 | Barbaste et al. (2026), Harness Engineering: Source-Code Study of Eleven Systems | paper | C | 3 | S | yes (mech) | https://arxiv.org/abs/2609.00006 | A1, A5, E4, L5 |
| S-075 | Wu et al. (2026-09-01), HarnessDev | paper | D | 3 | S | yes | https://arxiv.org/abs/2609.01437 | I5 (counter) |
| S-076 | Jin et al. (2026-09-01), HEART / Agent-Native Reusable Tool Primitives | paper | D | 3 | S | yes | https://arxiv.org/abs/2609.01736 | E2, E3 |

## E. Late-2026 frontier addendum & Tier 4 counterevidence

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-077 | Wu & Canedo (2026-08-31), Invalidation Contracts for Cross-Episode Agent Memory | paper | D | 4 | S | yes (mech) | https://arxiv.org/abs/2609.00243 | D4, A2 |
| S-078 | Yu et al. (2026-05-11), Shepherd: formalized execution trace substrate | paper | D | 4 | S | yes | https://arxiv.org/abs/2605.10913 | B4, I7 |
| S-079 | Yan et al. (2026-09-01), Harness-of-Harness | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.01481 | F3, J6 |
| S-080 | Lee et al. (2026-03-30), Meta-Harness: End-to-End Optimization of Model Harnesses | paper | D | 4 | S | yes | https://arxiv.org/abs/2603.28052 | I5 |
| S-081 | Shao et al. (2026-08-03), Harness-R1 | paper | D | 4 | S | yes | https://arxiv.org/abs/2608.02276 | I5 |
| S-082 | Miyai et al. (2026-08-20), Task-CoEvolve | paper | D | 4 | S | yes | https://arxiv.org/abs/2608.20169 | F4, I5 |
| S-083 | Wang et al. (2026-07-14, rev. 08-27), Rethinking the Evaluation of Harness Evolution | paper | C | 4 | S | yes (counter-evidence; matched-budget) | https://arxiv.org/abs/2607.12227 | I5, I2 |
| S-084 | Chen et al. (2026-07-17), Co-Harness | paper | D | 4 | S | yes | https://arxiv.org/abs/2607.22688 | I8 |
| S-085 | Kim et al. (2026-08-31), WHALE | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.00196 | I8 |
| S-086 | Mao et al. (2026-09-02), SafeEvolve | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.02786 | I5, I8, H1 |
| S-087 | Luo et al. (2026-07-15), HarnessBank: gated semantic quality-diversity | paper | D | 4 | S | yes | https://arxiv.org/abs/2607.13683 | I5 |
| S-088 | (2026), Harness Updating Is Not Harness Benefit | paper | D | 4 | S | yes (mech: validity vs compliance) | https://arxiv.org/abs/2605.30621 | A2, I2 |
| S-089 | (2026-08-27), Verify Smarter, Evolve Further / HarnessLens | paper | D | 4 | S | yes | https://arxiv.org/abs/2608.27311 | I5, G3 |
| S-090 | (2026), HarnessEvolve | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.00829 | I5 |
| S-091 | Ke et al. (2026-09-03), EVOHARNESSBENCH | bench | D | 4 | S | yes (counter) | https://arxiv.org/abs/2609.04280 | I5, I2 |
| S-092 | (2026), Where Does Harness-Optimization Value Live? | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.02889 | I5, I7 |
| S-093 | Zhao et al. (EMNLP 2026), PRISM: Beyond Prompts — Measuring & Optimizing Tool-Agent Harnesses | paper | A | 4 | S | yes (accepted; single-team) | https://arxiv.org/abs/2609.05736 | I2, I5 |
| S-094 | (2026-09), Safe Harness Self-Evolution | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.08175 | I5, H1 |
| S-095 | (2026-09-08), Procedural Graphs: Self-Evolving Execution Structures | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.09153 | D5 |
| S-096 | (2026-09-08), Co-Evolving Harnesses and Models: On-Policy Correction | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.09134 | I8, I6 |

## F. Security, memory & runtime semantics (Tier 4 security/memory path)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-097 | Debenedetti et al. (NeurIPS 2024), AgentDojo | bench | A | 4 | S | no | https://arxiv.org/abs/2406.13352 | H1, I4 |
| S-098 | Deng et al. (ICLR 2024 Spotlight), ToolEmu | paper | A | 4 | S | no | https://arxiv.org/abs/2309.15817 | H1, G3 |
| S-099 | Debenedetti et al. (2025), Defeating Prompt Injections by Design (CaMeL) | paper | C | 4 | S | no | https://arxiv.org/abs/2503.18813 | H1, L3 |
| S-100 | google-research/camel-prompt-injection | repo | B | 4 | S | no | https://github.com/google-research/camel-prompt-injection | H1 |
| S-101 | Costa et al. (2025), Securing AI Agents with Information-Flow Control (Fides) | paper | C | 4 | S | no | https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/ | H2, L3 |
| S-102 | microsoft/fides | repo | B | 4 | S | no | https://github.com/microsoft/fides | H2 |
| S-103 | Li et al. (2026-09-01), Context Privilege Escalation Attacks against AI Agent Harness | paper | D | 4 | S | yes (mech) | https://arxiv.org/abs/2609.01222 | H5, L3, D1 |
| S-104 | He et al. (2026-08-27), When Context Gets Root | paper | D | 4 | S | yes (mech) | https://arxiv.org/abs/2608.27299 | H5, L3 |
| S-105 | Bouras, Dai & Mechtaev (2026-09-08), Authority Is Not a String (CapScope) | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.08371 | H1 |
| S-106 | Shen, Toyoda & Leung (2026-09-08), Revoked but Still Authoritative | paper | D | 4 | S | yes (mech) | https://arxiv.org/abs/2609.08258 | D4 |

## G. Open-source systems (repository atlas)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-107 | openai/codex | repo | B | 2 | S | no | https://github.com/openai/codex | B1, E5, F2, K1 |
| S-108 | openai/symphony | repo | B | 2 | S | no | https://github.com/openai/symphony | B3, L-org |
| S-109 | anthropics/claude-agent-sdk-python | repo | B | 2 | S | no | https://github.com/anthropics/claude-agent-sdk-python | K4 |
| S-110 | OpenHands/software-agent-sdk | repo | B | 2 | S | no | https://github.com/OpenHands/software-agent-sdk | A1, B1, B5, K4 |
| S-111 | anomalyco/opencode | repo | B | 2 | S | no | https://github.com/anomalyco/opencode | C1, K1, L5 |
| S-112 | aaif-goose/goose | repo | B | 2 | S | no | https://github.com/aaif-goose/goose | C1, E4, K1 |
| S-113 | badlogic/pi-mono | repo | B | 2 | S | no | https://github.com/badlogic/pi-mono | C1, K4, L5 |
| S-114 | SWE-agent/SWE-agent | repo | B | 2 | S | no | https://github.com/SWE-agent/SWE-agent | I4, K1 |
| S-115 | SWE-agent/mini-swe-agent | repo | B | 2 | S | no | https://github.com/SWE-agent/mini-swe-agent | F1, K1 |
| S-116 | PrimeIntellect-ai/prime-agent | repo | D | 3 | S | yes | https://github.com/PrimeIntellect-ai/prime-agent | F3, B3 |
| S-117 | HKUDS/OpenHarness | repo | D | 3 | S | yes | https://github.com/HKUDS/OpenHarness | A1 |
| S-118 | omnigent-ai/omnigent (meta-harness) | repo | B | 3 | S | no | https://github.com/omnigent-ai/omnigent | J6, A1 |
| S-119 | google-gemini/gemini-cli | repo | B | 2 | S | no | https://github.com/google-gemini/gemini-cli | C1, E4, K1 |
| S-120 | mistralai/mistral-vibe | repo | B | 2 | S | no | https://github.com/mistralai/mistral-vibe | K1 |
| S-121 | Aider-AI/aider | repo | B | 2 | S | no | https://github.com/Aider-AI/aider | D1, G1 |
| S-122 | browser-use/browser-use | repo | B | 2 | S | no | https://github.com/browser-use/browser-use | E1, B5 |

## H. Benchmark families

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-123 | Jimenez et al., SWE-bench | bench | A | — | S | no | https://arxiv.org/abs/2310.06770 | I4 |
| S-124 | Terminal-Bench 2.0 (2026) | bench | C | — | S | no | https://arxiv.org/abs/2601.11868 | I4 |
| S-125 | Zhou et al., WebArena | bench | A | — | S | no | https://arxiv.org/abs/2307.13854 | I4 |
| S-126 | Xie et al. (NeurIPS 2024), OSWorld | bench | A | — | S | no | https://arxiv.org/abs/2404.07972 | I4 |
| S-127 | Yao et al. (2024), τ-bench | bench | A | — | S | no | https://arxiv.org/abs/2406.12045 | I4 |
| S-128 | METR (2025), Measuring AI Ability to Complete Long Tasks | paper | C | — | S | no | https://arxiv.org/abs/2503.14499 | I2, I4 |

## I. Program corpus (internal)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-129 | Doc 1 — Harness Engineering Frontier Report (v1) | doc | internal | — | P | n/a | docs/1_harness_engineering_frontier_report.md | v1→v2 diff baseline only |
| S-130 | Doc 2 — Genealogy, Anatomy & 2026 Frontier (canonical v2) | doc | internal | — | P | n/a | docs/2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md | starting map, not authority |
| S-131 | Doc 3 — Meta-Plan & Research Ledger | doc | internal | — | P | n/a | docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md | program contract |

---

## Workstream additions

*Workstreams append below, continuing the `S-###` sequence (next: **S-132**). Synthesis passes may promote `S` → `P` and revise `provisional?` when corroboration is recorded.*

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds | added-by |
|---|---|---|---|---|---|---|---|---|---|
