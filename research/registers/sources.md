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
| S-012 | Wu et al. (2024), AutoGen (COLM 2024) | paper | A | 1 | P | no | https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/ | A1, F3 |
| S-013 | Khattab et al. (2023/24), DSPy | paper | A | 0 | P | no | https://arxiv.org/abs/2310.03714 | A1, A3, A4 |
| S-014 | Yang et al. (NeurIPS 2024), SWE-agent (ACI) | paper | A | 1 | S | no | https://arxiv.org/abs/2405.15793 | E1, E2, I4 |
| S-015 | Zhang et al. (ICLR 2025), AFlow | paper | A | 0 | S | no | https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html | A3, I5 |
| S-016 | Packer et al. (2023), MemGPT | paper | C | 4 | S | no | https://arxiv.org/abs/2310.08560 | D3 |
| S-017 | Park et al. (UIST 2023), Generative Agents | paper | A | 4 | S | no | https://arxiv.org/abs/2304.03442 | D3 |

## B. Tier 0 — systems prehistory & the optimization lineage

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-018 | Yang et al. (ICLR 2024), Large Language Models as Optimizers (OPRO) | paper | A | 0 | S | no | https://arxiv.org/abs/2309.03409 | I5 |
| S-019 | Fernando et al. (ICML 2024), Promptbreeder | paper | A | 0 | S | no | https://arxiv.org/abs/2309.16797 | I5 |
| S-020 | Zhuge et al. (ICML 2024), GPTSwarm: Language Agents as Optimizable Graphs | paper | A | 0 | P | no | https://proceedings.mlr.press/v235/zhuge24a.html | A3, I5 |
| S-021 | Yuksekgonul et al. (2024), TextGrad | paper | C | 0 | S | no | https://arxiv.org/abs/2406.07496 | I5 |
| S-022 | Yuan et al. (2024), EvoAgent | paper | C | 0 | S | no | https://arxiv.org/abs/2406.14228 | I5, F3 |
| S-023 | Hu, Lu & Clune (ICLR 2025), Automated Design of Agentic Systems (ADAS) | paper | A | 0 | S | no | https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html | A3, I5 |
| S-024 | Shang et al. (2024), AgentSquare | paper | C | 0 | P | no | https://arxiv.org/abs/2410.06153 | A3, J2, I5 |
| S-025 | FoundationAgents, AFlow code | repo | B | 0 | P | no | https://github.com/FoundationAgents/AFlow | A3, I5 |
| S-026 | Zhang et al. (2025; ICLR 2026), Darwin Gödel Machine | paper | A | 0 | S | no | https://arxiv.org/abs/2505.22954 | I5 |
| S-027 | jennyzzt/dgm — Darwin Gödel Machine reference implementation | repo | B | 0 | S | no | https://github.com/jennyzzt/dgm | I5 |
| S-028 | Agrawal et al. (2025; ICLR 2026 Oral), GEPA | paper | A | 0 | S | no | https://arxiv.org/abs/2507.19457 | I5 |
| S-029 | Wang et al. (2025), Maestro | paper | C | 0 | S | no | https://arxiv.org/abs/2509.04642 | I5 |
| S-030 | Mei et al. (COLM 2025), AIOS: LLM Agent Operating System | paper | A | 0 | P | no | https://arxiv.org/abs/2403.16971 | A1, A2, B5, L2 |
| S-031 | agiresearch/AIOS | repo | B | 0 | P | no | https://github.com/agiresearch/AIOS | A1, B5, L2 |

## C. Tier 2 — production harness engineering (posts, docs, protocols)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-032 | Anthropic (2024), Building effective agents | post | B | 1 | S | no | https://www.anthropic.com/engineering/building-effective-agents | F1 |
| S-033 | Anthropic (2024), Introducing the Model Context Protocol | post | B | 2 | S | no | https://www.anthropic.com/news/model-context-protocol | E4, K3 |
| S-034 | Google (2025), Announcing the Agent2Agent Protocol (A2A) | post | B | 2 | S | no | https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/ | E4 |
| S-035 | Agent Client Protocol (ACP) | spec | B | 2 | P | no | https://github.com/agentclientprotocol/agent-client-protocol | E4, J6, K4 |
| S-036 | Anthropic (2025), How we built our multi-agent research system | post | B | 2 | P | no | https://www.anthropic.com/engineering/multi-agent-research-system | F3, F5 |
| S-037 | Aizawa et al. / Anthropic (2025), Writing effective tools for agents | post | B | 2 | P | no | https://www.anthropic.com/engineering/writing-tools-for-agents | E1, E2 |
| S-038 | Anthropic (2025), Effective context engineering for AI agents | post | B | 2 | S | no | https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | D1, D2 |
| S-039 | Anthropic (2025), Equipping agents for the real world with Agent Skills | post | B | 2 | S | no | https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills | D5, H5 |
| S-040 | Anthropic (2025), Introducing advanced tool use (deferred loading, tool search) | post | B | 2 | S | no | https://www.anthropic.com/engineering/advanced-tool-use | E3 |
| S-041 | Anthropic (2025), Effective harnesses for long-running agents | post | B | 2 | P | no | https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents | B3, D3 |
| S-042 | Bolin / OpenAI (2026-01-23), Unrolling the Codex agent loop | post | B | 2 | S | no | https://openai.com/index/unrolling-the-codex-agent-loop/ | F1, F2 |
| S-043 | Hashimoto (2026-02-05), My AI Adoption Journey | post | B | 2 | S | no | https://mitchellh.com/writing/my-ai-adoption-journey | A1, I6 |
| S-044 | OpenAI (2026-02-11), Harness engineering: leveraging Codex in an agent-first world | post | B | 2 | S | no | https://openai.com/index/harness-engineering/ | G1, A1 |
| S-045 | Trivedy / LangChain (2026-03-10), The Anatomy of an Agent Harness | post | B | 2 | P | no | https://www.langchain.com/blog/the-anatomy-of-an-agent-harness | A1, A2 |
| S-046 | Rajasekaran / Anthropic (2026-03-24), Harness design for long-running application development | post | B | 2 | S | no | https://www.anthropic.com/engineering/harness-design-long-running-apps | G1, G3 |
| S-047 | Anthropic (2026-04-08), Scaling Managed Agents: Decoupling the brain from the hands | post | B | 2 | P | no | https://www.anthropic.com/engineering/managed-agents | B1, B5, I6 |
| S-048 | Kotliarskyi, Zhu & Brock / OpenAI (2026-04-27), Symphony (post) | post | B | 2 | S | no | https://openai.com/index/open-source-codex-orchestration-symphony/ | B3, F3, L-org |
| S-049 | Heule & Katz / Cursor (2026-04-30), Continually improving our agent harness | post | B | 2 | P | no | https://cursor.com/blog/continually-improving-agent-harness | C3, E2, I6, I2 |
| S-050 | Katz / Cursor (2026-01-06), Dynamic context discovery | post | B | 2 | S | no | https://cursor.com/blog/dynamic-context-discovery | D1, D2, E3 |
| S-051 | Cursor (2026-02-12), Expanding our long-running agents research preview | post | B | 2 | S | no | https://cursor.com/blog/long-running-agents | B3 |
| S-052 | Ma / Cursor (2026-06-02), What we've learned building cloud agents | post | B | 2 | P | no | https://cursor.com/blog/cloud-agent-lessons | B3, B5, F3 |
| S-053 | McGuinness et al. / Anthropic (2026-05-25), How we contain Claude across products | post | B | 2 | P | no | https://www.anthropic.com/engineering/how-we-contain-claude | H3, H4, H7 |
| S-054 | Cursor (2026-08-19), Cloud Agents and Cursor Harness Improvements (changelog) | post | B | 2 | S | no | https://cursor.com/changelog/08-19-26 | B3, F3 |
| S-055 | OpenAI (2026), Unlocking the Codex harness: App Server | post | B | 2 | S | no | https://openai.com/index/unlocking-the-codex-harness/ | J6, K4 |
| S-056 | Cloudflare (2026-06-03), Harnesses (Agents SDK docs) | doc | B | 2 | P | no | https://developers.cloudflare.com/agents/harnesses/ | A2, B3, J6 |
| S-057 | Gauvin / Cloudflare (2026-06-17), Bringing more agent harnesses and frameworks to Cloudflare (Flue) | post | B | 2 | S | no | https://blog.cloudflare.com/agents-platform-flue-sdk/ | J6 |
| S-058 | AWS (2026-06-17), AgentCore harness generally available | post | D | 2 | P | vendor | https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/ | A1, J6 |
| S-059 | Microsoft (2026-07-22), Agent Framework Harness released | post | D | 2 | P | vendor | https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/ | A1, J6 |
| S-060 | Databricks (2026-09-02), Omnigent on Databricks | doc | D | 2 | S | vendor | https://docs.databricks.com/aws/en/omnigent/ | J6 |

## D. Tier 3 — 2026 research frontier

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-061 | Lin et al. (2026-04/05), Agentic Harness Engineering (AHE) | paper | C | 3 | P | yes | https://arxiv.org/abs/2604.25850 | I5, I7 |
| S-062 | Yao et al. (2026-05-27), Harness-Bench | bench | C | 3 | P | yes (mech) | https://arxiv.org/abs/2605.27922 | I2, I4, G2, J4 |
| S-063 | Pan et al. (2026), Retrospective Harness Optimization (RHO) — Findings of EMNLP 2026 | paper | A | 3 | S | yes (accepted; single-team) | https://arxiv.org/abs/2606.05922 | I5 |
| S-064 | Lumer et al. (2026-06-11), Recursive Agent Harnesses | paper | D | 3 | S | yes | https://arxiv.org/abs/2606.13643 | F3 |
| S-065 | Qi et al. (2026-06), LLM-as-Code: Agentic Programming for Agent Harness | paper | D | 3 | S | yes | https://arxiv.org/abs/2606.15874 | F1 |
| S-066 | Huang et al. (2026-07-14), MemoHarness | paper | D | 3 | S | yes | https://arxiv.org/abs/2607.14159 | I5, D3 |
| S-067 | Huang et al. (2026-08-10), Evo-Bench | bench | D | 3 | S | yes | https://arxiv.org/abs/2608.09096 | I5 |
| S-068 | Luthra et al. (2026-08-11), Evaluating Agentic Learning Harness Capabilities Without Labels | paper | D | 3 | P | yes | https://arxiv.org/abs/2608.13608 | I2 |
| S-069 | Karten et al. (2026-08-24), Prime Agent: A Self-Improving RLM Harness | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.23552 | F3, B3 |
| S-070 | Dhage (2026-08-25), Harness Engineering for Predictable Agentic Systems | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.26197 | F1, F2 |
| S-071 | Zhang et al. (2026-08-26), JIT-Agent | paper | D | 3 | S | yes | https://arxiv.org/abs/2608.25593 | I5 |
| S-072 | Lewis (2026-08-26), Same Model, Different Harness | paper | D | 3 | P | yes (mech) | https://arxiv.org/abs/2608.26218 | I2, J4 |
| S-073 | Jia et al. (2026-08-28), Logos: An Agent Harness on a Cross-Process Bus | paper | D | 3 | P | yes | https://arxiv.org/abs/2608.28553 | B1, B3, L5 |
| S-074 | Barbaste et al. (2026), Harness Engineering: Source-Code Study of Eleven Systems | paper | C | 3 | P | yes (mech) | https://arxiv.org/abs/2609.00006 | A1, A5, E4, L5 |
| S-075 | Wu et al. (2026-09-01), HarnessDev | paper | D | 3 | S | yes | https://arxiv.org/abs/2609.01437 | I5 (counter) |
| S-076 | Jin et al. (2026-09-01), HEART / Agent-Native Reusable Tool Primitives | paper | D | 3 | P | yes | https://arxiv.org/abs/2609.01736 | E2, E3 |

## E. Late-2026 frontier addendum & Tier 4 counterevidence

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-077 | Wu & Canedo (2026-08-31), Invalidation Contracts for Cross-Episode Agent Memory | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2609.00243 | D4, A2, B1, I2 |
| S-078 | Yu et al. (2026-05-11), Shepherd: formalized execution trace substrate | paper | D | 4 | P | yes | https://arxiv.org/abs/2605.10913 | B4, I7 |
| S-079 | Yan et al. (2026-09-01), Harness-of-Harness | paper | D | 4 | P | yes | https://arxiv.org/abs/2609.01481 | F3, J6 |
| S-080 | Lee et al. (2026-03-30), Meta-Harness: End-to-End Optimization of Model Harnesses | paper | D | 4 | P | yes | https://arxiv.org/abs/2603.28052 | I5 |
| S-081 | Shao et al. (2026-08-03), Harness-R1 | paper | D | 4 | S | yes | https://arxiv.org/abs/2608.02276 | I5 |
| S-082 | Miyai et al. (2026-08-20), Task-CoEvolve | paper | D | 4 | P | yes | https://arxiv.org/abs/2608.20169 | F4, I5 |
| S-083 | Wang et al. (2026-07-14, rev. 08-27), Rethinking the Evaluation of Harness Evolution | paper | C | 4 | P | yes (counter-evidence; matched-budget) | https://arxiv.org/abs/2607.12227 | I5, I2 |
| S-084 | Chen et al. (2026-07-17), Co-Harness | paper | D | 4 | S | yes | https://arxiv.org/abs/2607.22688 | I8 |
| S-085 | Kim et al. (2026-08-31), WHALE | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.00196 | I8 |
| S-086 | Mao et al. (2026-09-02), SafeEvolve | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.02786 | I5, I8, H1 |
| S-087 | Luo et al. (2026-07-15), HarnessBank: gated semantic quality-diversity | paper | D | 4 | S | yes | https://arxiv.org/abs/2607.13683 | I5 |
| S-088 | (2026), Harness Updating Is Not Harness Benefit | paper | D | 4 | P | yes (mech: validity vs compliance) | https://arxiv.org/abs/2605.30621 | A2, I2, B1, G1, G3 |
| S-089 | (2026-08-27), Verify Smarter, Evolve Further / HarnessLens | paper | D | 4 | S | yes | https://arxiv.org/abs/2608.27311 | I5, G3 |
| S-090 | (2026), HarnessEvolve | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.00829 | I5 |
| S-091 | Ke et al. (2026-09-03), EVOHARNESSBENCH | bench | D | 4 | P | yes (counter) | https://arxiv.org/abs/2609.04280 | I5, I2 |
| S-092 | (2026), Where Does Harness-Optimization Value Live? | paper | D | 4 | P | yes | https://arxiv.org/abs/2609.02889 | I5, I7 |
| S-093 | Zhao et al. (EMNLP 2026), PRISM: Beyond Prompts — Measuring & Optimizing Tool-Agent Harnesses | paper | A | 4 | P | yes (accepted; single-team) | https://arxiv.org/abs/2609.05736 | I2, I5 |
| S-094 | (2026-09), Safe Harness Self-Evolution | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.08175 | I5, H1 |
| S-095 | (2026-09-08), Procedural Graphs: Self-Evolving Execution Structures | paper | D | 4 | P | yes | https://arxiv.org/abs/2609.09153 | D5 |
| S-096 | (2026-09-08), Co-Evolving Harnesses and Models: On-Policy Correction | paper | D | 4 | S | yes | https://arxiv.org/abs/2609.09134 | I8, I6 |

## F. Security, memory & runtime semantics (Tier 4 security/memory path)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-097 | Debenedetti et al. (NeurIPS 2024), AgentDojo | bench | A | 4 | P | no | https://arxiv.org/abs/2406.13352 | H1, I4, I2 |
| S-098 | Deng et al. (ICLR 2024 Spotlight), ToolEmu | paper | A | 4 | S | no | https://arxiv.org/abs/2309.15817 | H1, G3 |
| S-099 | Debenedetti et al. (2025), Defeating Prompt Injections by Design (CaMeL) | paper | C | 4 | P | no | https://arxiv.org/abs/2503.18813 | H1, L3 |
| S-100 | google-research/camel-prompt-injection | repo | B | 4 | P | no | https://github.com/google-research/camel-prompt-injection | H1 |
| S-101 | Costa et al. (2025), Securing AI Agents with Information-Flow Control (Fides) | paper | C | 4 | P | no | https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/ | H2, L3 |
| S-102 | microsoft/fides | repo | B | 4 | P | no | https://github.com/microsoft/fides | H2 |
| S-103 | Li et al. (2026-09-01), Context Privilege Escalation Attacks against AI Agent Harness | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2609.01222 | H5, L3, D1 |
| S-104 | He et al. (2026-08-27), When Context Gets Root | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2608.27299 | H5, L3 |
| S-105 | Bouras, Dai & Mechtaev (2026-09-08), Authority Is Not a String (CapScope) | paper | D | 4 | P | yes | https://arxiv.org/abs/2609.08371 | H1 |
| S-106 | Shen, Toyoda & Leung (2026-09-08), Revoked but Still Authoritative | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2609.08258 | D4 |

## G. Open-source systems (repository atlas)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-107 | openai/codex | repo | B | 2 | P | no | https://github.com/openai/codex | B1, E5, F2, K1, B2, H7 |
| S-108 | openai/symphony | repo | B | 2 | P | no | https://github.com/openai/symphony | B3, L-org, B2 |
| S-109 | anthropics/claude-agent-sdk-python | repo | B | 2 | P | no | https://github.com/anthropics/claude-agent-sdk-python | K4 |
| S-110 | OpenHands/software-agent-sdk | repo | B | 2 | P | no | https://github.com/OpenHands/software-agent-sdk | A1, B1, B5, K4, B2, H7 |
| S-111 | anomalyco/opencode | repo | B | 2 | P | no | https://github.com/anomalyco/opencode | C1, K1, L5 |
| S-112 | aaif-goose/goose | repo | B | 2 | P | no | https://github.com/aaif-goose/goose | C1, E4, K1 |
| S-113 | badlogic/pi-mono | repo | B | 2 | P | no | https://github.com/badlogic/pi-mono | C1, K4, L5 |
| S-114 | SWE-agent/SWE-agent | repo | B | 2 | S | no | https://github.com/SWE-agent/SWE-agent | I4, K1 |
| S-115 | SWE-agent/mini-swe-agent | repo | B | 2 | S | no | https://github.com/SWE-agent/mini-swe-agent | F1, K1 |
| S-116 | PrimeIntellect-ai/prime-agent | repo | D | 3 | P | yes | https://github.com/PrimeIntellect-ai/prime-agent | F3, B3 |
| S-117 | HKUDS/OpenHarness | repo | D | 3 | P | yes | https://github.com/HKUDS/OpenHarness | A1 |
| S-118 | omnigent-ai/omnigent (meta-harness) | repo | B | 3 | P | no | https://github.com/omnigent-ai/omnigent | J6, A1 |
| S-119 | google-gemini/gemini-cli | repo | B | 2 | S | no | https://github.com/google-gemini/gemini-cli | C1, E4, K1 |
| S-120 | mistralai/mistral-vibe | repo | B | 2 | S | no | https://github.com/mistralai/mistral-vibe | K1 |
| S-121 | Aider-AI/aider | repo | B | 2 | S | no | https://github.com/Aider-AI/aider | D1, G1 |
| S-122 | browser-use/browser-use | repo | B | 2 | S | no | https://github.com/browser-use/browser-use | E1, B5 |

## H. Benchmark families

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-123 | Jimenez et al., SWE-bench | bench | A | — | S | no | https://arxiv.org/abs/2310.06770 | I4 |
| S-124 | Terminal-Bench 2.0 (2026) | bench | C | — | P | no | https://arxiv.org/abs/2601.11868 | I4 |
| S-125 | Zhou et al., WebArena | bench | A | — | S | no | https://arxiv.org/abs/2307.13854 | I4 |
| S-126 | Xie et al. (NeurIPS 2024), OSWorld | bench | A | — | S | no | https://arxiv.org/abs/2404.07972 | I4 |
| S-127 | Yao et al. (2024), τ-bench | bench | A | — | P | no | https://arxiv.org/abs/2406.12045 | I4, I2, J4 |
| S-128 | METR (2025), Measuring AI Ability to Complete Long Tasks | paper | C | — | P | no | https://arxiv.org/abs/2503.14499 | I2, I4 |

## I. Program corpus (internal)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-129 | Doc 1 — Harness Engineering Frontier Report (v1) | doc | internal | — | P | n/a | docs/1_harness_engineering_frontier_report.md | v1→v2 diff baseline only |
| S-130 | Doc 2 — Genealogy, Anatomy & 2026 Frontier (canonical v2) | doc | internal | — | P | n/a | docs/2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md | starting map, not authority |
| S-131 | Doc 3 — Meta-Plan & Research Ledger | doc | internal | — | P | n/a | docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md | program contract |

---

## Workstream additions

*Workstreams append below, continuing the `S-###` sequence (next: **S-167** — see synthesis note at end of file). Synthesis passes may promote `S` → `P` and revise `provisional?` when corroboration is recorded.*

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds | added-by |
|---|---|---|---|---|---|---|---|---|---|
| S-132 | harbor-framework/harbor — "Harbor: a framework for evaluating and optimizing agents and models in container environments" (v0.22.0; audited 7d5285b 2026-09-09; official Terminal-Bench 2.0 harness; also reachable as laude-institute/harbor) | repo | B | 2 | P | no | https://github.com/harbor-framework/harbor | A1, L1, J6, J3, J5, I3, I4 | WS-A1, WS-L1 |
| S-133 | Harbor Adapters and Harbor-Index: Infrastructure and a Curated Meta-Dataset for Large-Scale Agentic Evaluation (2026) | paper | C | 4 | P | yes (mech) | https://arxiv.org/abs/2609.04298 | A1, J6, I4 | WS-A1 |
| S-134 | princeton-pli/hal-harness (audited 16bb03e 2026-07-01) — `agent_function` runner, VM/Docker, Weave cost | repo | B | 2 | P | no | https://github.com/princeton-pli/hal-harness | A1, L7, J3, J5, L2 | WS-A1, WS-L7 |
| S-135 | Holistic Agent Leaderboard (HAL): The Missing Infrastructure for AI Agent Evaluation (ICLR 2026, accepted) — companion repo S-134 | paper | A | 4 | P | no (accepted; single-team) | https://arxiv.org/abs/2510.11977 | A1, L7, I2, J4, J5, J6, L2 | WS-A1, WS-L7 |
| S-136 | UKGovernmentBEIS/inspect_ai (audited 75f4891 2026-09-09) — agent bridge / sandbox agent bridge | repo | B | 2 | P | no | https://github.com/UKGovernmentBEIS/inspect_ai | A1, J6, I4, H7 | WS-A1 |
| S-137 | Inspect AI docs — Agent Bridge (in-process and sandbox model-call interception) | doc | B | 2 | P | no | https://inspect.aisi.org.uk/agent-bridge.html | J6, L7, I2, I4 | WS-A1, WS-L7 |
| S-138 | Open Agent Specification (Agent Spec): A Unified Representation for AI Agents (Oracle, arXiv 2510.04173 v4, 2025-11) | paper | C | 0 | P | no | https://arxiv.org/abs/2510.04173 | A1, A3, A4, A5, L7 | WS-A1 |
| S-139 | oracle/agent-spec — pyagentspec/tsagentspec + adapters (LangGraph, AutoGen, CrewAI, OpenAI Agents, WayFlow, MS Agent Framework) (audited 6f0b6ae 2026-08-31) | repo | B | 0 | P | no | https://github.com/oracle/agent-spec | A3, A4, A5, J6 | WS-A1 |
| S-140 | ClawGym II: Exploring Black-Box RL on Agent Harness (2026-08) | paper | D | 4 | P | yes | https://arxiv.org/abs/2608.16798 | J6, I8, L2 | WS-A1 |
| S-141 | Winder.AI — A Comparison of AI Agent Harnesses in 2026 (2026-08-20; vendor blog) | post | D | — | P | vendor | https://winder.ai/ai-agent-harness-comparison/ | A1, L7 | WS-A1 |
| S-142 | AgentManifest: a declarative spec where the harness is the first-class decision (personal blog RFC v0.3, 2026-04) | post | D | — | P | yes (proposal only) | https://dev.to/mouserider/agentmanifest-a-declarative-spec-where-the-harness-is-the-first-class-decision-lnc | A5, L7 | WS-A1 |
| S-143 | langchain-ai/langgraph (audited e539ac1 2026-09-09) — Pregel runtime, checkpoint layer (`libs/checkpoint/.../serde/base.py`), `StateGraph` | repo | B | 2 | P | no | https://github.com/langchain-ai/langgraph | A1, L7, L1, B1, B3, F1 | WS-A1, WS-L7, WS-L1 |
| S-144 | stanfordnlp/dspy (audited ca54a85 2026-09-09) — Signature/Adapter/teleprompt; code companion to S-013 | repo | B | 0 | P | no | https://github.com/stanfordnlp/dspy | A1, L7, A3, A4, C3, I5 | WS-A1, WS-L7 |
| S-145 | microsoft/autogen (audited 027ecf0 2026-04-06) — `ComponentModel`, `ModelInfo`, agbench; code companion to S-012 | repo | B | 1 | P | no | https://github.com/microsoft/autogen | A1, L7, A5, C3, F3, F5 | WS-A1, WS-L7 |
| S-146 | From Failed Trajectories to Reliable LLM Agents: Diagnosing and Repairing Harness Flaws (HarnessFix / HTIR), 2026-06 | paper | D | 4 | P | yes | https://arxiv.org/abs/2606.06324 | L7, A3, I7, I5 | WS-L7 |
| S-147 | Seong et al., The Last Harness You'll Ever Build (meta-evolution blueprint), 2026-04 | paper | D | 4 | P | yes (counter to hand-designed instruments) | https://arxiv.org/abs/2604.21003 | L7, I5 | WS-L7 |
| S-148 | Measuring Harness-Induced Belief Divergence in Multi-Step LLM Agents (BIWM), 2026-07 | paper | D | 4 | P | yes (mech) | https://arxiv.org/abs/2607.04528 | L7, G2, I2 | WS-L7 |
| S-149 | ruvnet/metaharness — "meta-harness" CLI/Studio that scaffolds branded agent harnesses (MIT) | repo | D | — | P | vendor/naming | https://github.com/ruvnet/metaharness | L7, L6 | WS-L7 |
| S-150 | SuperagenticAI/metaharness — unofficial Meta-Harness implementation (FSL-1.1-ALv2) | repo | D | — | P | vendor/naming | https://github.com/SuperagenticAI/metaharness | L7, L6, I5 | WS-L7 |
| S-151 | stanford-iris-lab/meta-harness — reference code for S-080 | repo | C | 4 | P | yes | https://github.com/stanford-iris-lab/meta-harness | L7, I5 | WS-L7 |
| S-152 | Language Server Protocol specification 3.17 (capabilities, `experimental`, `$/` methods) | spec | A | — | P | no | https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/ | L7, J6, E4, L5 | WS-L7 |
| S-153 | modelcontextprotocol/modelcontextprotocol — spec repo, protocol 2026-07-28; `schema/2026-07-28/schema.ts` source of truth; `_meta` prefixed keys; JSON generation + CI checks | spec | B | 2 | P | no | https://github.com/modelcontextprotocol/modelcontextprotocol | L7, L1, E4, K3, A4 | WS-L7, WS-L1 |
| S-154 | "Write once, run anywhere" / AWT peer model — lowest-common-denominator history (Wikipedia; Morelli & Walde "From AWT to Swing", LibreTexts) | doc | C (historical) | — | P | no | https://en.wikipedia.org/wiki/Write_once,_run_anywhere ; https://eng.libretexts.org/Bookshelves/Computer_Science/Programming_Languages/Java_Java_Java_-_Object-Oriented_Programming_(Morelli_and_Walde)/13%3A_Graphical_User_Interfaces/13.02%3A_Java_GUIs-_From_AWT_to_Swing | L7, A3, J6 | WS-L7 |
| S-155 | Model Context Protocol — official SDKs and SDK tiers (protocol 2026-07-28) | doc | B | 2 | P | no | https://modelcontextprotocol.io/docs/sdk | L1, E4, K3 | WS-L1 |
| S-156 | A2A Protocol Specification 1.0.0 and SDK list (JSON-RPC, gRPC, HTTP+JSON bindings) | spec | B | 2 | P | no | https://a2a-protocol.org/latest/specification/ ; https://a2a-protocol.org/latest/sdk/ | L1, E4 | WS-L1 |
| S-157 | ACP — Transports page (stdio JSON-RPC; Streamable HTTP draft) and libraries index (repo `docs/libraries/*.mdx`) | spec | B | 2 | P | no | https://agentclientprotocol.com/protocol/transports | L1, E4, J6 | WS-L1 |
| S-158 | Temporal — developer SDKs (eight ecosystems; Rust SDK 1.0.0) | doc | B | — | P | no | https://docs.temporal.io/develop | L1, B3 | WS-L1 |
| S-159 | Restate — SDK repositories (five ecosystems; server in a compiled language) | repo | B | — | P | no | https://github.com/restatedev | L1, B3 | WS-L1 |
| S-160 | DBOS — documentation (four ecosystems; Postgres-backed durable execution) | doc | B | — | P | no | https://docs.dbos.dev/ | L1, B3 | WS-L1 |
| S-161 | Inspect AI (UK AISI) — main documentation; sandboxes Docker/K8s/Modal/Proxmox/Vagrant; runs external agents (Agent Bridge page is S-137) | doc | B | — | P | no | https://inspect.aisi.org.uk/ | L1, I2, I4 | WS-L1 |
| S-162 | SWE-bench/SWE-bench — containerized evaluation harness; predictions interface (companion to S-123) | repo | B | — | P | no | https://github.com/SWE-bench/SWE-bench | L1, I4 | WS-L1 |
| S-163 | GitHub Octoverse 2025 — contributor-count growth figures by language | post | B | — | P | no (population statistic) | https://github.blog/news-insights/octoverse/octoverse-a-new-developer-joins-github-every-second-as-ai-leads-typescript-to-1/ | L1, L6 | WS-L1 |
| S-164 | Stack Overflow Developer Survey 2025 — Technology (admired/used) | post | B | — | P | no (population statistic) | https://survey.stackoverflow.co/2025/technology | L1, L6 | WS-L1 |
| S-165 | WASI 0.3 release (2026-06-11) — native async in the component model; WASI 1.0 targeted late 2026/early 2027 | spec | B | — | P | yes (roadmap) | https://wasi.dev/releases/wasi-p3 | L1, L5, E5, H4 | WS-L1 |
| S-166 | Barbaste et al. — HTML v1 of S-074 (per-system table: language, LoC, loop, sandbox, MCP/ACP/skills) | paper | C | 3 | P | yes (mech) | https://arxiv.org/html/2609.00006v1 | L1, A1, A5, E4 | WS-L1 |

### Synthesis notes on folded rows (Phase 0, 2026-09-09)

- **Dedup map (temp id → final):** WS-A1 01–14 → S-132…S-145 in order. WS-L7 01→S-135 (paper; repo S-134), 02→S-137, 03→S-146, 04→S-147, 05→S-148, 06→S-149, 07→S-150, 08→S-151, 09→S-152, 10→S-153, 12→S-154, 14→S-144, 15→S-145, 16→S-143. WS-L1 01→S-155, 02→S-156, 03→S-157, 04→S-158, 05→S-159, 06→S-160, 07→S-161, 08→S-132, 09→S-162, 10→S-163, 11→S-164, 12→S-165, 13→S-143, 14→S-153, 15→S-166.
- **Promoted S → P (opened directly in Phase 0):** S-012, S-013, S-024, S-030, S-031, S-035, S-045, S-047, S-049, S-053, S-056, S-058, S-059, S-062, S-072, S-073, S-074, S-079, S-080, S-083, S-088, S-093, S-107, S-109, S-110, S-111, S-112, S-113, S-116, S-117, S-118. **Not opened:** S-055 (HTTP 403 for both WS-A1 and WS-L1; substituted by S-107 `codex-rs/app-server-*` source) — remains `S`.
- **Corrections to seeded rows (WS-A1):** S-118 Omnigent now self-describes as "the open-source meta-harness" hosting Claude Code, Codex, Cursor, OpenCode, Hermes, Pi and custom agents; its bench is a *conformance* suite, not a performance benchmark (relevant to J6/J4). S-062 Harness-Bench evaluates in a Harbor-style shared environment with native execution preserved (relevant to I4). S-074's per-system language tally is internally inconsistent (CF-018); rely on direction only.

### Phase 1 additions (folded 2026-09-10; temp ids renumbered)

| id | title | kind | tier | syllabus | P/S | provisional? | url | feeds | added-by |
|---|---|---|---|---|---|---|---|---|---|
| S-167 | Box & Wilson (1951), "On the Experimental Attainment of Optimum Conditions" — response surface methodology (reference page: definition, first/second-order models, factorial and central-composite designs, interaction terms) | paper | A | 0 | P (reference page; original via JRSS-B) | no | https://en.wikipedia.org/wiki/Response_surface_methodology ; https://doi.org/10.1111/j.2517-6161.1951.tb00067.x | A2, J4, I2 | WS-A2 |
| S-168 | Sutton, Precup & Singh (1999), "Between MDPs and semi-MDPs: a framework for temporal abstraction in reinforcement learning", *Artificial Intelligence* 112 | paper | A | 0 | P (front matter; body not extracted) | no | https://doi.org/10.1016/S0004-3702(99)00052-1 | A2 (lineage only) | WS-A2 |
| S-169 | Banu (2026-05-12), "Harness Engineering as Categorical Architecture" — Architecture triple (G, Know, Φ); memory as coalgebraic state; skills as operads; compiler functors to five frameworks | paper | D | 4 | P (abstract) | yes (single-author preprint) | https://arxiv.org/abs/2605.12239 | A2 (competing formalism; disconfirming), A3 | WS-A2 |
| S-170 | ShengranHu/ADAS — reference code for S-023 (audited @ 2702bee, 2026-09-09; re-verified 2026-09-10): `_arc/search.py` L237–332 (archive `{thought, name, code, fitness, generation}`), `_arc/arc_prompt.py` (init archive) | repo | B | 0 | P | no | https://github.com/ShengranHu/ADAS | A3, I5 | WS-A3 |
| S-171 | Unison — "The big idea": definitions identified by hash of their syntax tree; names as separately stored metadata; dependencies referenced by hash | doc | B | 0 | P | no | https://www.unison-lang.org/docs/the-big-idea/ | A3, L4 | WS-A3 |
| S-172 | RFC 8785 — JSON Canonicalization Scheme (JCS): deterministic serialization for hashing/signing; I-JSON constraints; no Unicode normalization | spec | A | 0 | P | no | https://www.rfc-editor.org/rfc/rfc8785.html | A3, B1, L4, H6 | WS-A3 |
| S-173 | MLIR Language Reference — operations/attributes/regions, dialects, unregistered operations, per-operation verifiers | doc | B | 0 | P | no | https://mlir.llvm.org/docs/LangRef/ | A3, A4 | WS-A3 |
| S-174 | Falleri, Morandat, Blanc, Martinez, Monperrus (ASE 2014), "Fine-grained and Accurate Source Code Differencing" (GumTree): AST edit scripts with move actions; two-phase matching with structure hashes | paper | A | 0 | P (search summary + project page) | no | https://github.com/GumTreeDiff/gumtree ; https://www.labri.fr/perso/xblanc/data/papers/ASE14.pdf | A3, I7, L4 | WS-A3 |
| S-175 | Leijen (MSFP 2014), "Koka: Programming with Row-polymorphic Effect Types" | paper | A | 0 | P (search summary) | no | https://arxiv.org/abs/1406.2061 | A3, L1 | WS-A3 |
| S-176 | Sakizli (2026-05-04), TSCG: Deterministic Tool-Schema Compilation for Agentic LLM Deployments — deterministic JSON-schema → structured-text compiler at the API boundary; eight composable operators; three model-specific operator-response profiles; ~19k calls, 12 models | paper | D | 3 | P (abstract) | yes (single author, preprint) | https://arxiv.org/abs/2605.04107 | A4, C3, E2 | WS-A4 |
| S-177 | Lee, Song, Han, Pyun, Jo (ACL 2026 main; arXiv 2025-10-08), "Don't Adapt Small Language Models for Tools; Adapt Tool Schemas to the Models" — PA-Tool; schema misalignment; pretraining-aligned renaming (+17% MetaTool/RoTBench; −80% misalignment errors) | paper | A (accepted) | 3 | P (abstract) | no (accepted; single-team; small models) | https://arxiv.org/abs/2510.07248 | A4, C3, E2 | WS-A4 |
| S-178 | Xu, Wen, Li (2026-05-21, rev. 05-27), "Adapting the Interface, Not the Model: Runtime Harness Adaptation for Deterministic LLM Agents" (Life-Harness) — harness learned from trajectories then frozen for evaluation; 116/126 model–environment settings; transfer from one 4B model to 17 others | paper | D | 3 | P (abstract) | yes (WIP preprint) | https://arxiv.org/abs/2605.22166 | A4, I5, I8 | WS-A4 |
| S-179 | Lattner et al. (CGO 2021; arXiv 2002.11054), "MLIR: A Compiler Infrastructure for the End of Moore's Law" — multi-level IR, dialects, progressive lowering, location tracking | paper | A | 0 | P (abstract) | no | https://arxiv.org/abs/2002.11054 | A4, A3 | WS-A4 |
| S-180 | a2aproject/A2A — protocol repository; `specification/a2a.proto` (`Task`, `TaskState`, `Message`, `Artifact`, `AgentCard`, `AgentExtension`, `AgentSkill`) (audited 98853be 2026-09-09) | spec/repo | B | 2 | P | no | https://github.com/a2aproject/A2A | A4, E4, K3 | WS-A4 |
| S-181 | reproducible-builds.org — "Definitions": "given the same source code, build environment and build instructions, any party can recreate bit-by-bit identical copies of all specified artifacts"; verified by cryptographic hash comparison; authors/distributors define the relevant environment attributes | doc | B | — | P | no | https://reproducible-builds.org/docs/definition/ | A4, I3, L4 | WS-A4 |
| S-182 | NixOS RFC 0062 — Content-addressed store paths: input-addressed vs content-addressed identity; early cut-off; separation of trust from storage | doc | B | — | P | no | https://github.com/NixOS/rfcs/blob/master/rfcs/0062-content-addressed-paths.md | A4, I3, L4 | WS-A4 |
| S-183 | Claude Code docs — *Settings files and precedence* (managed > `--settings` > project-local > project > user; "Lists merge instead of overriding"; security keys where a stricter lower-scope value wins: `crossSessionInbound` accept<hold<refuse, `disableClaudeAiConnectors`; `allowManagedPermissionRulesOnly`) | doc | B | 2 | P | no (vendor doc; mechanism only) | https://code.claude.com/docs/en/settings | A5, H1, L5, K1 | WS-A5 |
| S-184 | Claude Code docs — *Plugins reference* (`plugin.json`: `name` namespacing, `version` pin, `dependencies` with semver, `userConfig` typed prompts incl. `sensitive`, path fields with replace / add-to / merge semantics, `$schema`; unknown top-level fields ignored; `claude plugin validate --strict`) | doc | B | 2 | P | no | https://code.claude.com/docs/en/plugins-reference | A5, L5, J2 | WS-A5 |
| S-185 | Hydra docs — *Basic Override syntax* (`key=v`, `+key`, `++key`, `~key`; config-group/Defaults-List overrides vs config-object overrides; `key=a,b` choice sweep for multirun) | doc | B | 0 | P | no | https://hydra.cc/docs/advanced/override_grammar/basic/ | A5, J3 | WS-A5 |
| S-186 | Kubernetes community — *API Conventions* (`spec` = complete desired state vs `status`; `apiVersion`/`kind` on every object; declarative field semantics; defaulting only sets unset fields/adds keys/adds array values and never overrides user values; validation errors, never silent correction) | doc | B | 0 | P | no | https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md | A5, A3, J1, J6 | WS-A5 |
| S-187 | OCI Image Format Specification — *Content Descriptors* (`mediaType`, `digest`, `size`, `annotations`; "The digest property of a Descriptor acts as a content identifier"; implementations MUST NOT modify content in ways that change content identifiers) | spec | A | 0 | P | no | https://github.com/opencontainers/image-spec/blob/main/descriptor.md | A5, L4, J2, I3 | WS-A5 |
| S-188 | OpenTelemetry Trace API specification — trace/span identity (16-byte trace id, 8-byte span id), single parent, `Link`s for non-hierarchical causality, span immutability after end, timestamped span events | spec | B | 0 | P | no | https://opentelemetry.io/docs/specs/otel/trace/api/ | B1, I1, F3 | WS-B1 |
| S-189 | CloudEvents v1.0.2 specification — `source` + `id` uniqueness rule, producer-defined `type`, `dataschema`, extension attributes | spec | B | 0 | P | no | https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md | B1, I1, J6 | WS-B1 |
| S-190 | RFC 6962 — Certificate Transparency: domain-separated Merkle tree hash (leaf `0x00`, node `0x01`), Signed Tree Head, inclusion and consistency proofs | spec | A | 0 | P | no | https://www.rfc-editor.org/rfc/rfc6962 | B1, H6, I3 | WS-B1 |
| S-191 | RFC 9562 — Universally Unique IDentifiers (UUIDs): UUIDv7 48-bit ms timestamp + random; three monotonicity methods (§6.2); index-locality rationale | spec | A | 0 | P | no | https://www.rfc-editor.org/rfc/rfc9562 | B1, L4 | WS-B1 |
| S-192 | Kulkarni, Demirbas, Madeppa, Avva, Leone (2014), "Logical Physical Clocks and Consistent Snapshots in Globally Distributed Databases" (Hybrid Logical Clocks); read via Demirbas' summary post | paper | A | 0 | P (summary) | no | https://cse.buffalo.edu/tech-reports/2014-04.pdf (summary: https://muratbuffalo.blogspot.com/2014/07/hybrid-logical-clocks.html) | B1, B3, F3 | WS-B1 |
| S-193 | Temporal documentation — "Workflow definition": deterministic constraints (same Workflow API calls in the same sequence), non-determinism errors from code change or intrinsic randomness, side effects through Activities, versioning/patching | doc | B | 0 | P | no | https://docs.temporal.io/workflow-definition | B1, B3, B4, L1 | WS-B1 |
| S-194 | Fowler (2005), "Event Sourcing" — pattern definition; complete rebuild, temporal query, event replay; snapshots as caches; the external-gateway problem; the code-versioning problem | doc | B | 0 | P | no | https://martinfowler.com/eaaDev/EventSourcing.html | B1, B4 | WS-B1 |
| S-195 | Kiehl (2019-02-03), "Don't Let the Internet Dupe You, Event Sourcing is Hard" — practitioner retrospective: each projection "doubles the amount of code that touches your event stream"; schema-evolution fidelity loss; eventual-consistency anomalies; "a history table gets you 80% of the value of a ledger with essentially none of the cost" (re-verified 2026-09-10) | post | C | 0 | P | no | https://chriskiehl.com/article/event-sourcing-is-hard | B1 (disconfirming) | WS-B1 |
| S-196 | Helland (CIDR 2015), "Immutability Changes Everything" — append-only data, derived views, content-addressed storage | paper | A | 0 | S (PDF not readable in this environment; cited from prior knowledge; **not load-bearing**) | no | https://www.cidrdb.org/cidr2015/Papers/CIDR15_Paper16.pdf | B1, B2 | WS-B1 |
| S-197 | Garcia-Molina & Salem (SIGMOD 1987), "Sagas" — long-lived transactions as T1…Tn with compensating C1…Cn−1; compensation is a semantic approximation; backward/forward recovery; coordinator records checkpoints reliably | paper | A | 0 | P (abstract + secondary summary; PDF unrenderable) | no | https://dl.acm.org/doi/10.1145/38713.38742 | B2, B3, B4 | WS-B2 |
| S-198 | Gray & Cheriton (SOSP 1989), "Leases: An Efficient Fault-Tolerant Mechanism for Distributed File Cache Consistency" — time-bounded contracts; short terms bound recovery delay; non-Byzantine clock assumption | paper | A | 0 | P (abstract + secondary summary) | no | https://dl.acm.org/doi/10.1145/74850.74870 | B2, B3, B1 | WS-B2 |
| S-199 | Helland (ACM Queue 2012), "Idempotence Is Not a Medical Condition" — retries produce duplicates; idempotence is the application's obligation; exactly-once is not a transport property | paper | A/B | 0 | S (abstract only; full text HTTP 403 on three mirrors) | no | https://queue.acm.org/detail.cfm?id=2187821 | B2, F2, E4 | WS-B2 |
| S-200 | Stripe Engineering, "Designing robust and predictable APIs with idempotency" — client-minted idempotency key; server stores and replays first response; backoff + jitter | post | B | 2 | P | no | https://stripe.com/blog/idempotency | B2, E4, F2 | WS-B2 |
| S-201 | Temporal docs — Activities, Retry Policies, Detecting Activity failures (Start-To-Close forces retry after worker crash; heartbeat details; non-retryable errors; "we recommend that it be idempotent") — extends S-158 | doc | B | 2 | P | no | https://docs.temporal.io/encyclopedia/detecting-activity-failures ; https://docs.temporal.io/encyclopedia/retry-policies | B2, B3, F2 | WS-B2 |
| S-202 | Chang & Geng (2025-03), "SagaLLM: Context Management, Validation, and Transaction Guarantees for Multi-Agent LLM Planning" (arXiv 2503.11951) — saga pattern + persistent memory + automated compensation + independent validators | paper | D | 3 | P (abstract) | yes | https://arxiv.org/abs/2503.11951 | B2, F3 | WS-B2 |
| S-203 | Perera, Hapuarachchi, Leymann & Khalaf (2026-05), "Robust Agent Compensation (RAC): Teaching AI Agents to Compensate" (arXiv 2605.03409; ACM CAIS 2026) — log-based recovery/compensation as framework-agnostic extension; τ-bench/REALM-Bench; "1.5–8×" latency/token vs LLM-based recovery | paper | C | 3 | P (abstract) | yes (single team; accepted venue) | https://arxiv.org/abs/2605.03409 | B2, B3 | WS-B2 |
| S-204 | Mansoor, Phadke & Rana (2026-07-31), "Verified Tool Calls Improve LLM Agent Reliability Under Non-Atomic Failures" (arXiv 2608.02645) — failure model (timeouts post-dispatch, delayed visibility, partial updates); postcondition verification + verify-before-retry + idempotency keys; simulated evaluation | paper | D | 3 | P (abstract) | yes | https://arxiv.org/abs/2608.02645 | B2, E5, F2 | WS-B2 |
| S-205 | Zhai, Li & Wang (2026-04-25), "Revisable by Design: A Theory of Streaming LLM Agent Execution" (arXiv 2604.23283) — action taxonomy Idempotent/Reversible/Compensable/Irreversible; theorem: conflicting irreversible actions make full specification satisfaction impossible | paper | D | 3 | P (abstract) | yes (mechanism/theorem admissible) | https://arxiv.org/abs/2604.23283 | B2, B4, H7 | WS-B2 |
| S-206 | tianpan.co (2026-04), "The Idempotency Problem in Agentic Tool Calling" — key from durable state `{runId}:{stepId}`; dedup store; idempotent compensations. Unsourced practitioner post; vocabulary only | post | D | — | P | yes (unsourced) | https://tianpan.co/blog/2026-04-idempotency-agentic-tool-calling | B2 | WS-B2 |
| S-207 | Kapoor, Stroebl, Siegel, Nadgir & Narayanan (2024), *AI Agents That Matter* — accuracy-only benchmarking produces needlessly costly agents; joint accuracy–cost optimisation; Pareto framing (abstract read 2026-09-10) | paper | C | 4 | P | no (2024; widely replicated practice) | https://arxiv.org/abs/2407.01502 | L2, I2, J4 | WS-L2 |
| S-208 | Ye & Tan (2026-01-13, rev. 03-25), *Agent Contracts: A Formal Framework for Resource-Bounded Autonomous AI Systems* (COINE@AAMAS 2026 workshop) — multi-dimensional resource constraints; conservation law Σ_j c_j(r) ≤ B(r); `ACTIVE → VIOLATED` on any dimension; soft (prompt) + hard (monitor) enforcement; zero conservation violations / 50 trials (abstract + HTML) | paper | D | 4 | P | yes (mech admissible; numbers provisional) | https://arxiv.org/abs/2601.08815 | L2, F3, F5, H7 | WS-L2 |
| S-209 | Khan (2026-06-02), *Token Budgets: An Empirical Catalog of 63 LLM-Agent Budget-Overrun Incidents, with an Affine-Typed Rust Mitigation as a Case Study* — eight-cluster overrun taxonomy across 21 frameworks; delegation-fanout race (11 incidents; "overshoots 30/30" without reservation); reserve-before-spend, single accounting authority, affine handles (abstract + HTML) | paper | D | 4 | P | yes (mech: incident catalogue quotes GitHub issues; mitigation numbers provisional) | https://arxiv.org/abs/2606.04056 | L2, F2, F3 | WS-L2 |
| S-210 | Yang, Luo, Liu, Lou & Chen (2025-11-26), *BAMAS: Structuring Budget-Aware Multi-Agent Systems* (AAAI 2026 oral) — ILP model selection under budget + RL topology; up to 86 % cost reduction at comparable performance (abstract) | paper | C | 4 | P | yes (accepted venue; single team; performance) | https://arxiv.org/abs/2511.21572 | L2, C2, F4 | WS-L2 |
| S-211 | Chen et al. (2026-05-09), *Token Economics for LLM Agents: A Dual-View Study from Computing and Economics* — micro/meso/macro/security taxonomy; tokens as production factor, medium of exchange, unit of account (abstract) | paper | D | 4 | P | yes (survey; vocabulary only) | https://arxiv.org/abs/2605.09104 | L2, A2 | WS-L2 |
| S-212 | OpenTelemetry GenAI semantic conventions — `gen_ai.usage.input_tokens/output_tokens`, `cache_read.input_tokens`, `cache_write.input_tokens`, `reasoning.output_tokens`, per-modality splits; status Development; no cost attribute (repo doc read 2026-09-10) | spec | B | 2 | P | no | https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-spans.md | L2, I1 | WS-L2 |
| S-213 | Anthropic, *Prompt caching* (platform docs) — `input_tokens` excludes cached; total = input + cache_read + cache_creation; write 1.25× (5 min) / 2× (1 h); read 0.1× (0.025× newer models); TTL from request start; mixed-TTL billing positions | doc | B | 2 | P | vendor (mechanism; prices drift) | https://platform.claude.com/docs/en/docs/build-with-claude/prompt-caching | L2, C4, C1 | WS-L2 |
| S-214 | Anthropic, *Task budgets* (platform docs; beta `task-budgets-2026-03-13`) — advisory budget over one agentic turn; counts novel tokens the model sees, not resent payload; server-side countdown invisible in `usage`; `remaining` carry-over across client compaction; minimum 20k; too-small budgets cause refusal-like behaviour; not supported on Claude Code | doc | B | 2 | P | vendor (mechanism) | https://platform.claude.com/docs/en/build-with-claude/task-budgets | L2, F2, C3, D2, J6 | WS-L2 |
| S-215 | OpenAI, *Prompt caching* (developer docs) — `input_tokens_details.cached_tokens` (subset of input count), `cache_write_tokens` on newer models; 0.1× read; ~30 min retention | doc | B | 2 | P | vendor (mechanism; prices drift) | https://developers.openai.com/api/docs/guides/prompt-caching | L2, C4, C1 | WS-L2 |
| S-216 | Denning (1976), A lattice model of secure information flow, CACM 19(5) — security classes form a lattice; flow relation; join = least upper bound; program certification | paper | A | 0 | P (CACM page + summaries) | no | https://dl.acm.org/doi/10.1145/360051.360056 | L3, H2 | WS-L3 |
| S-217 | Biba (1977), Integrity considerations for secure computer systems (MITRE TR-3153) — no-read-down/no-write-up; low-water-mark policy (subject label drops to the minimum after reading lower-integrity data) | paper | A | 0 | P (summaries) | no | https://en.wikipedia.org/wiki/Biba_Model (canonical report: MITRE ESD-TR-76-372) | L3, H1, D1 | WS-L3 |
| S-218 | Myers & Liskov (2000), Protecting privacy using the decentralized label model, ACM TOSEM 9(4) — owner/reader labels; restriction-only relabeling; join; "only the owner can declassify its own policy"; dual integrity model | paper | A | 0 | P (PDF) | no | https://www.cs.cornell.edu/andru/papers/iflow-tosem.pdf | L3, H2 | WS-L3 |
| S-219 | Krohn, Yip, Brodsky, Cliffer, Kaashoek, Kohler, Morris (SOSP 2007), Information flow control for standard OS abstractions (Flume) — endpoints carry declassification policy; trusted vs untrusted processes; user-level reference monitor | paper | A | 0 | P (search summaries) | no | https://pdos.csail.mit.edu/papers/flume-sosp07.pdf | L3, H1, H2 | WS-L3 |
| S-220 | Wallace, Xiao, Leike, Weng, Heidecke, Beutel (2024), The Instruction Hierarchy: Training LLMs to Prioritize Privileged Instructions — model-side privilege levels; complementary to harness-side labels | paper | C | 4 | P (abstract) | no | https://arxiv.org/abs/2404.13208 | L3, D1, C3 | WS-L3 |
| S-221 | W3C PROV-DM: The PROV Data Model (Recommendation, 2013) — Entity/Activity/Agent; wasDerivedFrom (revision, quotation, primary source), wasAttributedTo, actedOnBehalfOf | spec | A | 0 | P | no | https://www.w3.org/TR/prov-dm/ | L3, B1, L4, H6 | WS-L3 |
| S-222 | in-toto Attestation Framework — Statement{subject, predicateType, predicate} in a signed envelope (DSSE); "authenticated metadata about one or more software artifacts" | spec | B | 2 | P | no | https://github.com/in-toto/attestation/blob/main/spec/README.md | L3, H5, H6 | WS-L3 |
| S-223 | SLSA v1.0 Provenance predicate — buildDefinition/runDetails; builder.id as trust anchor; "Consumers MUST accept only specific signer-builder pairs" | spec | B | 2 | P | no | https://slsa.dev/spec/v1.0/provenance | L3, H5, H6, L4 | WS-L3 |
| S-224 | Pro Git (2nd ed.) §10.2 "Git Internals — Git Objects": object id = SHA-1 over `"<type> <size>\0"` ∥ content; blob/tree/commit/tag; trees and commits reference by hash; identical content → identical id; refs are mutable names over immutable objects | doc | B | 0 | P | no | https://git-scm.com/book/en/v2/Git-Internals-Git-Objects | L4, B1, J2, I3 | WS-L4 |
| S-225 | Nix Reference Manual — *Store Path* protocol: `fingerprint = type ":sha256:" inner-digest ":" store ":" name`, SHA-256 truncated to 160 bits; input-addressed (digest over the derivation) vs content-addressed (digest over contents) store objects; the name participates in the digest | doc | B | 0 | P | no | https://nix.dev/manual/nix/stable/protocols/store-path.html | L4, I3, B5 | WS-L4 |
| S-226 | multiformats/multihash — self-describing hash `<varint fn code><varint length><digest>`; rationale: hash-function agility, "future-proof their use of hashes, and allow multiple hash functions to coexist" | spec | B | 0 | P | no | https://github.com/multiformats/multihash | L4, H6 | WS-L4 |
| S-227 | Semantic Versioning 2.0.0: MAJOR/MINOR/PATCH rules; "Once a versioned package has been released, the contents of that version MUST NOT be modified"; pre-release precedence; build metadata ignored for precedence | spec | B | 0 | P | no | https://semver.org/spec/v2.0.0.html | L4, J2, L5, L6 | WS-L4 |
| S-228 | OpenTelemetry Semantic Conventions for Generative AI (repo `open-telemetry/semantic-conventions-genai`, 0c87594, 2026-09-10) — inference / agent / tool spans, `gen_ai.usage.*` (inclusive input-token rule, cache_read/cache_write/reasoning buckets), `gen_ai.conversation.compacted`, `gen_ai.evaluation.result` event, metrics (`gen_ai.client.token.usage` `{token}`, `gen_ai.client.operation.duration` `s`, `time_to_first_chunk`, `invoke_agent.*`, `execute_tool.duration`), content-capture modes and upload hook; Development stability | spec | B | 2 | P | no (spec; Development stability noted) | https://github.com/open-telemetry/semantic-conventions-genai | I1, I2, C1, C4, J6, K2 | WS-I1 |
| S-229 | OpenTelemetry Tracing SDK specification — Sampling (`DROP / RECORD_ONLY / RECORD_AND_SAMPLE`, decision at span start, `ParentBased`, `TraceIdRatioBased` deprecated in favour of `ProbabilitySampler`), span limits (128 events/links per span), tail-based sampling outside the SDK | spec | A | — | P | no | https://opentelemetry.io/docs/specs/otel/trace/sdk/ | I1, H6 | WS-I1 |
| S-230 | OpenTelemetry Semantic Conventions for MCP — context propagation via unprefixed `traceparent`/`tracestate`/`baggage` in `params._meta` (SEP-414); server span parented on `_meta` context with transport context as a link; `mcp.client/server.operation.duration`, session duration metrics | spec | B | 2 | P | no | https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/mcp.md ; https://modelcontextprotocol.io/community/seps/414-request-meta | I1, E4, A4, K3 | WS-I1 |
| S-231 | W3C Trace Context (traceparent / tracestate) — read via S-107 `codex-rs/otel/src/trace_context.rs` and S-230; identity surface for exported spans | spec | A | — | S (cited via implementations; not load-bearing beyond id shape) | no | https://www.w3.org/TR/trace-context/ | I1, E4 | WS-I1 |
| S-232 | OpenInference semantic conventions (Arize; `openinference.span.kind ∈ {LLM, EMBEDDING, CHAIN, RETRIEVER, RERANKER, TOOL, AGENT, GUARDRAIL, EVALUATOR, PROMPT}`) and Arize's 2026 native GenAI↔OpenInference normalization | doc | B | — | P (web search summary + spec index) | no | https://github.com/Arize-ai/openinference/blob/main/spec/semantic_conventions.md ; https://arize.com/blog/arize-ax-opentelemetry-genai-semantic-conventions/ | I1, K2 (export target evidence) | WS-I1 |
| S-233 | Laminar (lmnr) as OpenHands' observability backend — `openhands-sdk/openhands/sdk/observability/laminar.py` (env-driven OTLP init, `LLM`/`TOOL` span types, `openhands.operation` metadata) | repo/doc | B | — | P (source) | no | https://github.com/OpenHands/software-agent-sdk/blob/main/openhands-sdk/openhands/sdk/observability/laminar.py | I1 | WS-I1 |
| S-234 | Miller (2024), "Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations" — CLT standard errors, clustered SEs by question, paired differences, power analysis, resampling | paper | C | 0 | P (abs) | no (2024; implemented in Inspect) | https://arxiv.org/abs/2411.00640 | I2, J4 | WS-I2 |
| S-235 | Chen et al. (2021), "Evaluating Large Language Models Trained on Code" (Codex) — pass@k unbiased estimator 1 − C(n−c,k)/C(n,k) | paper | A | 1 | P (abs; formula verified in Inspect `reducer.py`) | no | https://arxiv.org/abs/2107.03374 | I2, J4 | WS-I2 |
| S-236 | "Beyond pass@1: A Reliability Science Framework for Long-Horizon LLM Agents" (2026-03) — RDC/VAF/GDS/MOP; 23,392 episodes; capability vs reliability rank divergence with horizon | paper | D | 4 | P (abs) | yes | https://arxiv.org/abs/2603.29231 | I2, I4, J4 | WS-I2 |
| S-237 | "Efficient Benchmarking of AI Agents" (2026-03) — evaluate on tasks with 30–70 % historical pass rates (IRT-motivated); 44–70 % fewer tasks with rank fidelity; absolute scores shift under scaffold change | paper | D | 4 | P (abs) | yes | https://arxiv.org/abs/2603.23749 | I2, J3, F4 | WS-I2 |
| S-238 | "Beyond Pass@k: Measuring Reliability and Security of Agentic Code Generation" (2026-08) — pass@k operationalization error (n = unit tests vs independent rollouts); reliability@k; security-adjusted reliability@k | paper | D | 4 | P (abs) | yes | https://arxiv.org/abs/2608.14711 | I2, H-track | WS-I2 |
| S-239 | "Don't Pass@k: A Bayesian Framework for Large Language Model Evaluation" (2025-10) — Dirichlet/Beta posterior over per-task success; credible intervals; rank stability with fewer samples | paper | C | 4 | P (abs) | yes (single team) | https://arxiv.org/abs/2510.04265 | I2, J4 | WS-I2 |
| S-240 | sierra-research/tau-bench (repo; audited 59a200c 2026-09-10) — `tau_bench/run.py` L181–203 pass^k reference implementation (`C(c,k)/C(n,k)` averaged over tasks); companion to S-127 | repo | B | — | P | no | https://github.com/sierra-research/tau-bench | I2, I4 | WS-I2 |
| S-241 | ethz-spylab/agentdojo (repo; audited 089ed46 2026-09-10) — `src/agentdojo/benchmark.py` (`SuiteResults`, utility/security per (user task, injection task), DoS rule), `src/agentdojo/base_tasks.py` (`utility/security(model_output, pre_env, post_env)`, `*_from_traces`, `ground_truth`); companion to S-097 | repo | B | — | P | no | https://github.com/ethz-spylab/agentdojo | I2, I4, H1 | WS-I2 |
| S-242 | RFC 8785 (JCS) verified implementations per ecosystem — `serde_json_canonicalizer` (crates.io), `rfc8785` (trailofbits/rfc8785.py, PyPI), `canonicalize` / `json-canonicalize` (npm), `gowebpki/jcs` (Go), `erdtman/java-json-canonicalization` (Java); RFC 8785 Appendix G list | doc | B | — | P | no | https://datatracker.ietf.org/doc/rfc8785/ ; https://crates.io/crates/serde_json_canonicalizer ; https://github.com/trailofbits/rfc8785.py ; https://www.npmjs.com/package/json-canonicalize ; https://github.com/gowebpki/jcs | L1, B1, L4 | WS-L1 (ratification) |
| S-243 | Node.js `node:sqlite` built-in module — stability "release candidate" (1.2) on Node 24+; unflagged since v22.13.0 | doc | B | — | P | no | https://nodejs.org/api/sqlite.html ; https://github.com/nodejs/node/issues/57445 | L1, B1 | WS-L1 (ratification) |
| S-244 | Python typing documentation — "Unreachable Code and Exhaustiveness Checking" (`assert_never`, pyright `reportMatchNotExhaustive`, mypy `exhaustive-match`) | doc | B | — | P | no | https://typing.python.org/en/latest/guides/unreachable.html | L1, A3 | WS-L1 (ratification) |
| S-245 | OpenJDK JEP 525 — Structured Concurrency (Sixth Preview, JDK 26); finalization expected JDK 27/28 | spec | B | — | P | no | https://openjdk.org/jeps/525 | L1 | WS-L1 (ratification) |
| S-246 | TC39 proposal "JSON.parse source text access" / `JSON.rawJSON` (stage 3; shipped in V8) — the only standard path to arbitrary-precision integers in JSON for the web-native ecosystem; `json-bigint` as the library alternative | spec | B | — | P | no | https://tc39.es/proposal-json-parse-with-source/ ; https://github.com/tc39/proposal-json-parse-with-source | L1, B1, A3 | WS-L1 (ratification) |
| S-247 | Wasmtime — "Using the Wasmtime API": official embeddings Rust, C/C++, Python (`wasmtime` on PyPI), Go (`wasmtime-go`), .NET (NuGet), Ruby | doc | B | — | P | no | https://docs.wasmtime.dev/lang.html | L1, L5, E5 | WS-L1 (ratification) |
| S-248 | Node.js WASI docs (`node:wasi`) and Bytecode Alliance `jco` — WASI Preview 2 support in Node (experimental), P3 tests passing in jco 1.20 | doc | B | — | P | yes (roadmap) | https://nodejs.org/api/wasi.html ; https://github.com/bytecodealliance/jco | L1, L5 | WS-L1 (ratification) |
| S-249 | npm docs — Trusted publishing and provenance attestations (Sigstore; `npm audit signatures`); PyPI Trusted Publishers; `cargo-vet`/`cargo-audit` as the compiled-ecosystem audit tooling | doc | B | — | P | no | https://docs.npmjs.com/trusted-publishers/ ; https://docs.npmjs.com/generating-provenance-statements/ | L1, L6, H5 | WS-L1 (ratification) |
| S-250 | json-schema.org tooling index — draft 2020-12 validators by language (e.g. `ajv`, `@hyperjump/json-schema`, `JsonSchema.Net`); used to confirm first-class JSON-Schema manipulation exists in every candidate class | doc | B | — | P | no | https://json-schema.org/tools | L1, A4, E2 | WS-L1 (ratification) |

### Synthesis notes on folded rows (Phase 1, 2026-09-10)

- **Dedup map (temp id → final):** WS-A2 01–03 → S-167…S-169; WS-A3 01–06 → S-170…S-175 (S-172 = RFC 8785 JCS, also opened by WS-B1 as S-WS-B1-04); WS-A4 01–07 → S-176…S-182 (S-181 = reproducible-builds definition, also WS-L4-06); WS-A5 01–05 → S-183…S-187 (S-187 = OCI descriptors, also WS-L4-04); WS-B1 01–03, 05–10 → S-188…S-196; WS-B2 01–10 → S-197…S-206; WS-L2 01–09 → S-207…S-215; WS-L3 01–08 → S-216…S-223 (S-223 = SLSA v1.0 provenance, also WS-L4-05); WS-L4 01, 02, 03, 07 → S-224…S-227; WS-I1 01–06 → S-228…S-233; WS-I2 01–08 → S-234…S-241.
- **Promoted S → P (opened directly in Phase 1):** S-077, S-020, S-025, S-095, S-099, S-037, S-076, S-052, S-078, S-108, S-041, S-092, S-100, S-101, S-102, S-103, S-104, S-105, S-106, S-036, S-082, S-061, S-068, S-091, S-097, S-127, S-128, S-124 (28 rows). S-196 (Helland, CIDR 2015) stays `S` — PDF unreadable, cited from prior knowledge, not load-bearing. S-231 (W3C Trace Context) is `S` — read via implementations only.
- **Row corrections applied:** `feeds` extended on S-088 (+B1, G1, G3), S-077 (+B1, I2), S-108 (+B2), S-110 (+B2, H7), S-107 (+B2, H7), S-127 (+I2, J4), S-097 (+I2). **Notes recorded (not edited in-row):** S-088 — metric names SLR/HFR/LPR are in the body, not the abstract; S-156 — the A2A protocol repository S-180 is its normative companion (`specification/a2a.proto`); S-102 — the repository is a tutorial notebook (`Tutorial.ipynb`), kind should read `repo (notebook)`; S-035 — ACP v2 `ToolCallStatus` has five values (`pending, in_progress, completed, failed, cancelled`), not four (WS-B1 §4a F9 corrected); S-135 — reports no confidence intervals or variance (negative result, WS-I2 §5); S-134 — also carries `utils/{fault_injection,error_classifier,taubench_perturbations,compliance_checkers}.py` (reliability/security process-metric precedents); S-212 (OTel GenAI spans doc) is one page of the S-228 repository.
- **Provisional discipline check:** every `yes`/`mech` row added in Phase 1 (S-169, S-176, S-178, S-202–S-206, S-208–S-211, S-236–S-239) is cited for mechanism or as a competitor/counter-example only; no C0 ADR ratified in Phase 1 rests on any of them (see `synthesis/phase-1.md` §3).

### WS-L1 ratification notes (2026-09-10)

- S-242…S-250 added by the WS-L1 ratification pass (ADR-0050); all opened directly. Re-verified on 2026-09-10 without new rows: S-155 (MCP SDK tiers: Tier 1 TypeScript/Python/C#/Go/Rust; Tier 2 Java/Ruby; Tier 3 Swift/PHP/Kotlin), S-156 (A2A official SDKs Python/Go/Java/JavaScript/.NET/Rust, spec v1.0), S-157 (ACP stdio stable, Streamable HTTP draft; official libraries Rust/TypeScript/Python/Java/Kotlin per repo 9b00c27 `docs/libraries/`). Repository evidence for ADR-0050 read at pinned commits codex 0735c51, goose fae91d0, software-agent-sdk 3fc7b22, opencode 9f8db11, pi-mono 400d690, harbor 7d5285b, inspect_ai 75f4891, modelcontextprotocol aa8ce04, a2a 98853be.

*Next id: **S-251**.*
