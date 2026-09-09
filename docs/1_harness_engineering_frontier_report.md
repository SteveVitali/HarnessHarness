A research survey of the model-external systems that turn foundation models into reliable, long-horizon agents - and of the emerging methods for measuring, evolving, and securing those systems.

------------------------------------------------------------------------

| **Central thesis.** Harness engineering is best understood as inference-time systems and control engineering for goal-directed foundation models. Its frontier is shifting from hand-built prompts and loops toward durable, observable, model-conditioned runtimes that can increasingly diagnose and modify their own scaffolding. The strongest evidence also argues against treating “the model” as the unit of capability: in agentic settings, the relevant object is the model-harness-environment configuration. |
|----|

Scope. This report synthesizes academic papers, industry engineering write-ups, protocol specifications, and open-source implementations available through 9 September 2026. Very recent 2026 results are explicitly treated as provisional where they are preprints or single-team reports rather than replicated findings.

# Executive summary

- Harness engineering is new terminology for an old systems problem. The ideas descend from classical agent architectures and control, but the modern lineage runs through transformers, in-context learning, retrieval, tool-augmented models, ReAct-style loops, agent-computer interfaces, context engineering, and long-running agent runtimes.[^1][^2][^3]

- A useful rigorous definition is: the empirical design and optimization of the inference-time/runtime layer around one or more foundation models - interfaces, context and state, tool execution, control flow, verification, security, persistence, and evaluation - so that model intelligence produces reliable goal-directed behavior in an external environment.

- The field crystallized in 2026 because model capability crossed a threshold at which the runtime became a visible bottleneck. Hashimoto used “harness engineering” for systematic prevention of recurring agent mistakes; OpenAI described the Codex harness as the agent loop and execution logic; LangChain popularized the broad formulation “Agent = Model + Harness.” The exact coinage is not reliably settled.[^4][^5][^6]

- The strongest empirical shift is methodological: harnesses are becoming independent experimental variables. Harness-Bench records large differences across model-harness pairings, and controlled same-model studies show that context and stall-handling changes alone can materially alter coding outcomes under constrained conditions.[^7][^8]

- The frontier has two complementary poles. Adaptive-harness research optimizes or generates prompts, tools, middleware, memory, workflows, and subagents from execution traces; deterministic-harness research pushes loops, schemas, permissions, retries, and validation into software. A general-purpose architecture should combine deterministic invariants with model autonomy, rather than choose one extreme.[^9][^10][^11][^12][^13][^14]

- Long-horizon reliability is increasingly a distributed-systems problem: durable event logs, restartability, isolated workspaces, idempotent execution, resource accounting, event-driven wakeups, and fault containment are becoming first-class harness primitives.[^15][^16][^17][^18]

- Multi-agent systems appear most defensible when they buy fresh context, parallelism, specialization, or independent verification. “More agents” is not itself an architecture. Recursive Agent Harnesses and current production systems make the full harness - tools, filesystem, planning, execution - the delegable unit.[^19][^20][^21]

- The most promising novel contributions are not another role-playing orchestrator. They are portable harness representations, safe self-improvement with causal attribution and rollback, durable event-driven runtimes, execution-alignment verification, capability-based security, value-of-compute scheduling for subagents, and rigorous model x harness x environment benchmark science.

# Contents

1\. What harness engineering is

2\. Genealogy: from foundation models to harnesses

3\. Why the discipline crystallized now

4\. Anatomy of a modern harness

5\. The 2026 frontier

6\. What the evidence actually establishes

7\. Industry architectures and open-source systems

8\. Evaluation and benchmark science

9\. What is genuinely new - and what is renamed

10\. Open research problems

11\. Blueprint for a general-purpose harness

12\. Contribution map

13\. Reading and repository syllabus

Sources

# 1. What harness engineering is

At the broadest operational level, an agent is not merely an LLM with a system prompt. It is a closed-loop system in which a model observes a state, decides what to do, acts through tools or an environment, receives consequences, updates its working state, and eventually terminates. The harness is the machinery that creates and governs that loop. OpenAI explicitly describes the Codex harness as the “core agent loop and execution logic”; LangChain’s 2026 formulation expands the term to every code/configuration/execution component outside the model.[^22][^23]

A narrow historical sense is also important. Hashimoto’s February 2026 usage was failure-driven: whenever an agent makes a repeatable mistake, encode a prompt rule or tool that makes the failure unlikely to recur. That conception resembles software reliability engineering applied to stochastic workers. The later broad usage encompasses the entire runtime, including components that are not merely patches for model weakness.[^24]

## A working research definition

| **Definition.** Harness engineering is the empirical design, implementation, measurement, and evolution of the model-external inference/runtime system that mediates perception, context, action, state, control, verification, security, and coordination for one or more foundation models operating toward goals in an external environment. |
|----|

This definition deliberately makes the research object larger than prompt engineering and smaller than “everything in an AI product.” A database may be infrastructure; it becomes part of the harness when its schema, retrieval policy, persistence semantics, or tool interface directly shape the agent’s behavior. A user interface may be product surface; it becomes harness-relevant when it gates actions, supplies observations, or changes the feedback loop.

## A formal view

Let M denote a frozen model, E an environment, D a distribution of tasks, and Hθ a harness parameterized by θ. The harness parameterization can include prompts, context-selection policies, tool schemas, middleware, state machines, memory/retrieval policies, subagent topology, validators, permission rules, retry policies, and stopping conditions. For a resource budget B, harness engineering can be viewed as selecting θ to maximize expected utility:

**maxθ Eτ~D \[ U(success, quality) - λc C(τ) - λl L(τ) - λr R(τ) \] subject to budget, safety, and permission constraints**

The important consequence is that capability is conditional. A leaderboard score for “model M” is actually a score for some model-harness-environment-task configuration. Harness-Bench makes this explicit, and the source-code literature argues for treating harness implementation as a first-class object rather than incidental benchmark plumbing.[^25][^26]

## Boundary with adjacent disciplines

| **Term** | **Primary object** | **Relationship to harness engineering** |
|----|----|----|
| Prompt engineering | Instruction wording for one or a few calls | A subset: system/developer/user instructions are harness parameters. |
| Context engineering | The complete token state presented at inference | A major subset: retrieval, compaction, history, tool descriptions, memories, artifacts. |
| Agent engineering | A goal-directed system that acts through tools/environments | Overlapping umbrella; harness engineering focuses specifically on the external runtime/control layer. |
| Agent framework | Reusable library or SDK | A means for building harnesses; not the configured harness itself. |
| Orchestration | Routing, delegation, concurrency, handoffs | One harness subsystem, especially important in multi-agent and long-running work. |
| AgentOps / LLMOps | Deployment, observability, governance, lifecycle | Overlaps at telemetry/evals/rollouts; usually broader operational scope. |
| Compound AI systems | Systems composed of multiple models/retrievers/tools | Broader category; many compound systems are not autonomous agents. |
| Evaluation harness | Benchmark execution and scoring infrastructure | A different sense of “harness”; can itself be used to measure runtime harnesses. |

# 2. Genealogy: from foundation models to harnesses

The genealogy is not a straight line. Three strands converged: (1) language models became sufficiently general to serve as adaptive decision modules; (2) external memory, tools, and executable environments made model outputs consequential; and (3) software systems learned to keep those interactions reliable over longer horizons.

| **Period** | **Milestone** | **Harness significance** |
|----|----|----|
| Pre-2017 | Classical agents, planners, BDI/control architectures, software test harnesses | Established the abstract loop: observe state, choose action, execute, receive feedback; deterministic software owns invariants and environment interfaces. |
| 2017 | Transformer | Created the scalable sequence-model substrate from which general-purpose foundation models emerged. |
| 2020 | GPT-3 in-context learning; RAG | Showed task adaptation without weight updates and introduced systematic external non-parametric memory/retrieval. |
| 2021 | WebGPT | Put a language model inside an explicit browser/search environment and required evidence collection. |
| 2022 | MRKL, ReAct, PAL; ChatGPT launch | Modular tool/reasoner systems; canonical reason-act-observe loop; code execution as deterministic complement; conversational interface went mainstream. |
| 2023 | Toolformer, Reflexion, Voyager, AutoGen | Learned tool invocation; episodic verbal memory; executable skill libraries and self-verification; multi-agent composition. |
| 2024 | SWE-agent ACI; “Building effective agents”; MCP | Interface design became an explicit performance variable; production guidance favored simple loops; open tool/data protocol began standardizing the agent edge. |
| 2025 | Context engineering, Agent Skills, dynamic tools, multi-agent research, long-running agents | The bottleneck shifted toward token budgeting, procedural knowledge, tool-space scaling, delegation, and state across context windows. |
| 2026 | Term crystallization; model-harness measurement; harness evolution | “Harness engineering” became a named systems discipline; papers began isolating harness effects and automatically optimizing or generating harnesses. |

## 2017-2020: the model becomes programmable at inference time

The Transformer paper is an architectural ancestor, not a harness paper: it supplied the scalable attention architecture behind modern language models. GPT-3 then made a systems shift possible by demonstrating meaningful task adaptation through textual instructions and demonstrations without gradient updates. That transformed natural-language context into a programmable control surface.[^27][^28]

RAG added a second critical idea: model behavior could be coupled to a mutable external knowledge store rather than relying solely on parameters. Modern harness memory systems are much richer, but the conceptual split between parametric intelligence and runtime-retrieved state is foundational.[^29]

## 2021-2023: action, tools, feedback, and persistent procedural state

WebGPT is an unusually direct ancestor of contemporary browsing agents: a model operated a text browser, searched and navigated the web, and gathered references for an answer. MRKL generalized the “systems” intuition by pairing LMs with external knowledge and discrete reasoning modules.[^30][^31]

ReAct supplied the canonical agent loop: interleave reasoning with actions and observations from the environment. PAL showed that a model need not own every computation: it can generate a program while an interpreter executes deterministic operations. These are still two poles of modern harness design - model-directed control and program-directed control.[^32][^33]

ChatGPT’s November 2022 public launch made conversational instruction-following a mass interface. In 2023, Toolformer formalized learned API use; Reflexion stored linguistic feedback in episodic memory; Voyager combined an automatic curriculum, an executable skill library, environment feedback, errors, and self-verification; AutoGen made multi-agent conversations a reusable programming abstraction.[^34][^35][^36][^37][^38]

## 2024-2025: the interface and runtime become first-class

SWE-agent’s key contribution was not merely a coding benchmark result. It named the Agent-Computer Interface (ACI) as an object to design for language-model “users,” showing that navigation/editing/testing interfaces materially affect agent behavior. This is one of the clearest academic precursors to harness engineering.[^39]

Anthropic’s late-2024 production guidance argued for simple, composable patterns and distinguished predetermined workflows from agents that dynamically control their own tool use. MCP then standardized a major boundary: connecting models/agents to external tools and data sources.[^40][^41]

In 2025, context engineering became an explicit discipline; Agent Skills packaged procedural knowledge into discoverable file trees; advanced tool use added deferred loading and dynamic discovery for large tool catalogs; long-running-agent work used durable files, progress records, and version control to bridge context windows. Meanwhile AFlow and related “workflow optimization” research were already automating parts of what would soon be called harness evolution.[^42][^43][^44][^45][^46]

## 2026: a named discipline and a new experimental unit

The terminology crystallized quickly but messily. Hashimoto used “harness engineering” on February 5 and explicitly noted that he did not know whether an accepted term already existed. OpenAI used “harness” for Codex’s core loop in January and published a harness-engineering case study in February. LangChain’s March anatomy article gave the phrase a broad, memorable definition: the harness is the model-external machinery that supplies state, tools, infrastructure, orchestration, and middleware. Claims that any single person definitively coined the term should therefore be treated cautiously.[^47][^48][^49][^50]

By mid-2026, the academic frontier had changed from designing a particular agent to measuring and optimizing harnesses themselves. Harness-Bench isolates configuration effects; Agentic Harness Engineering makes harness components editable and observable; RHO and MemoHarness learn from trajectories; Evo-Bench and HarnessDev evaluate whether models can improve or create their own harnesses; JIT-Agent proposes a model whose output is a task-adaptive harness.[^51][^52][^53][^54][^55][^56][^57]

# 3. Why the discipline crystallized now

1.  Models became capable enough for the wrapper to matter. Weak models fail regardless of tooling. Frontier models can plan, recover, write code, and use computers well enough that context policy, tool ergonomics, execution feedback, and control flow can move the limiting factor outside the weights.[^58][^59]

2.  Agent horizons expanded from seconds to hours or days. Long sessions amplify state drift, context saturation, partial completion, environment failures, retries, and recovery. This turns “prompting” into runtime engineering.[^60][^61]

3.  Coding supplied unusually good feedback. Tests, compilers, linters, diffs, CI, and version control make correctness partially machine-checkable, which is why much of the most mature harness research is concentrated in software engineering.[^62][^63]

4.  Benchmark scores exposed harness variance. Once the same base model appears inside multiple products and open-source agents, differences in tools, context, and runtime are observable rather than theoretical.[^64][^65]

5.  Integration edges became standardized. MCP, A2A, and ACP increasingly separate the internal harness from tool/data servers, remote agents, and editor clients. That modularity makes the harness itself more portable and experimentally replaceable.[^66][^67][^68]

6.  Human attention became the bottleneck. OpenAI’s Symphony and Cursor’s always-on cloud agents move orchestration upward from individual turns to work items, events, goals, and long-lived tasks.[^69][^70]

# 4. Anatomy of a modern harness

A useful architecture is seven interacting planes. This is a synthesis rather than a claim that the field has standardized on exactly seven layers; it aligns closely with the components identified in current production write-ups and the 2026 source-code study.[^71][^72]

| **Plane** | **Core question** | **Typical mechanisms** | **Characteristic failure** |
|----|----|----|----|
| 1\. Observation & context | What does the model see now? | System instructions, history, retrieval, compaction, tool descriptions, artifacts, multimodal observations | Context rot, omitted evidence, stale or poisoned memory |
| 2\. Action & tools | What can it do, and how? | Shell/code, filesystem, browser, MCP, APIs, computer use, dynamic tool discovery | Tool misuse, schema friction, excessive tokens, non-composable interfaces |
| 3\. Control & orchestration | Who decides the next step? | ReAct loop, FSM/workflow, routing, planning, retries, stopping, subagent scheduler | Loops, premature stopping, control-flow hallucination, coordination overhead |
| 4\. Verification & feedback | How does the system know it is right? | Tests, linters, validators, end-state checks, critics, task contracts, proof-of-work | Plausible but ungrounded completion; reward hacking; weak judges |
| 5\. State & durability | What survives a turn/process/window? | Filesystem, git, event log, checkpoints, session store, progress files, memory layers | Lost progress, duplicate side effects, unrecoverable crashes |
| 6\. Security & governance | What is the maximum allowed blast radius? | Sandbox/VM, permissions, capabilities, egress controls, credential isolation, audit | Prompt injection, exfiltration, over-broad privileges, approval fatigue |
| 7\. Measurement & evolution | How does the harness improve? | Traces, evals, A/B tests, attribution, candidate edits, canaries, rollback | Overfitting, regressions, model-specific brittleness, untraceable changes |

## The key architectural question: where should control live?

Every harness allocates cognition among model weights, token context, deterministic code, external tools, durable memory, subagents, and humans. ReAct-like systems let the model choose the loop dynamically; Agentic Programming argues that loops, branching, and sequencing should remain deterministic program structure and that the LLM should be invoked only at explicitly adaptive nodes. In practice, the best boundary is task-dependent.[^73][^74]

| **Design principle.** Put hard invariants in deterministic software: permissions, budgets, idempotency, transaction boundaries, schema validation, and objective stopping constraints. Put open-ended search, decomposition, interpretation, and recovery tactics in the model. Move the boundary only when evaluation demonstrates a benefit. |
|----|

# 5. The 2026 frontier

The most important recent work is not a single architecture. It is a set of pressure fronts where longer-horizon autonomy exposes systems problems that one-shot prompting never had to solve.

## 5.1 Context as an active memory hierarchy

The old pattern was “put everything useful in the prompt.” The frontier is selective, just-in-time context assembly. Anthropic frames context as a finite resource to curate on every inference step; Cursor increasingly turns long tool outputs, chat history, terminals, and tool catalogs into discoverable file-like resources rather than permanent prompt payload. Compaction, retrieval, offloading, and skills become policies rather than static text.[^75][^76][^77]

- Working context: the small, high-signal state needed for the next decision.

- Artifact memory: files, logs, intermediate outputs, plans, patches, screenshots - externally addressable rather than repeatedly tokenized.

- Episodic memory: prior trajectories, failures, diagnoses, and feedback that can be retrieved when structurally similar tasks recur.[^78][^79]

- Procedural memory: reusable skills, scripts, policies, and conventions loaded progressively.[^80]

- Durable session state: event/log structures that survive context resets, process restarts, and handoffs.[^81][^82]

A crucial lesson from Cursor and Anthropic is that context mechanisms are model-conditioned. Guardrails that helped earlier models can become dead weight or even failure sources as models improve. Cursor reports deleting older static-context and tool-call guardrails; Anthropic reports context resets that fixed one model’s “context anxiety” becoming stale assumptions for a stronger model.[^83][^84]

A newer systems result makes the memory problem even more concrete: cross-episode memory behaves like a cache, so correctness requires explicit invalidation semantics. Invalidation Contracts attaches version stamps and cacheability hints to cached recovery suggestions so stale entries can be evicted deterministically rather than rediscovered by trial and error. Across roughly 9,400 episodes spanning seven models, three serving paths, and two domains, the paper reports zero contract failures for version-stamp validity; planner compliance with identical cached advice still varied sharply by model. The broader harness lesson is that durable memory needs validity, provenance, and expiry semantics—not merely retrieval similarity.[^85]

## 5.2 Tool interfaces are a learned ABI

Tool engineering has moved beyond function schemas. Effective tools need names, descriptions, return shapes, error behavior, and token economics that match the model’s learned interaction patterns. Anthropic explicitly treats tools as contracts between deterministic software and a nondeterministic agent; Cursor provisions different edit interfaces for different model families because familiar tool shapes reduce reasoning overhead and errors.[^86][^87]

The scaling problem is tool cardinality. Anthropic’s deferred loading and tool search target hundreds or thousands of tools without stuffing every schema into context. MCP standardizes discovery and invocation at the tool/data boundary. The September 2026 HEART preprint pushes farther, replacing rigid schema-facing interaction with natural-language “tool primitives” and dynamic retrieval over a catalog of 25,519 functions; its headline performance and cost claims are interesting but remain very fresh and should be treated as provisional.[^88][^89][^90]

## 5.3 Verification becomes part of the runtime, not post-hoc QA

The central reliability primitive is grounded feedback from the environment. Tests, type checkers, linters, rendered UIs, browser interactions, logs, metrics, and explicit validators convert hidden model mistakes into observations that the loop can act on. OpenAI’s agent-first repository work exposes application UI and observability directly to Codex; Anthropic’s long-running application harness uses planner/generator/evaluator patterns and live browser feedback.[^91][^92]

Harness-Bench names a particularly useful failure class: execution-alignment failures, where plausible reasoning becomes decoupled from tool feedback, workspace state, evidence, or output contracts. That suggests a research target deeper than “reasoning quality”: continuously reconcile the agent’s claims with externally verifiable state.[^93]

## 5.4 Deterministic envelopes around stochastic cognition

Two 2026 papers sharpen the deterministic side of the design space. Agentic Programming puts all looping, branching, and sequencing in program code and treats LLM calls as adaptive functions inside the execution graph. Dhage’s empirical study wraps agents with finite-state control, forced tool selection, output validation, bounded retry, and structured planning. Its first-pass results were mixed - constraints could help or hurt - but schema-valid structured planning substantially improved reproducibility in the reported synthetic experiments. The lesson is not “more constraints”; it is “measure which invariants belong outside the model.”[^94][^95]

## 5.5 Long-horizon agents become distributed systems

A long-running harness has to reason about crashes, stale state, retries, duplicated side effects, resource leases, isolation, and resumability. Anthropic uses progress artifacts and git across context windows. Managed Agents separates the harness (“brain”), action environments (“hands”), and append-only session state. Cursor’s cloud-agent architecture emphasizes dedicated VMs, durable execution, self-healing environments, event-driven wakeups, and isolated subagent machines. Symphony maps work items to isolated workspaces and continuously reconciles agents against an issue-tracker control plane.[^96][^97][^98][^99][^100]

Logos makes the distributed-systems analogy explicit: plugins are separate processes, shared state is an append-only transcript, and fault isolation/recovery are tested by killing processes at tool-cycle boundaries. The paper is a very recent draft, but it highlights a likely direction for general-purpose harnesses: event sourcing plus failure-domain isolation rather than a single monolithic agent process.[^101]

Shepherd pushes this runtime view into a programmable meta-agent substrate. It records agent-environment interactions as typed, Git-like execution traces that can be forked and replayed, allowing supervisors and optimizers to branch from prior states instead of restarting entire trajectories. Its reported applications include live intervention, counterfactual search, and tree-structured RL; the deeper architectural contribution is reversible execution as a first-class harness primitive.[^102]

Harness-of-Harness goes one level higher: rather than replace a coding harness, it wraps existing harness-model pairs in repeated planning–coding–testing loops with versioned project history and independently evaluated increments. The September 2026 preprint reports a 52.25% average relative gain over standalone harnesses across three benchmarked harness-model pairs after iterative improvement, plus a multi-day run exceeding 70 iterations. The result is very fresh and domain-specific, but it is a useful prototype of hierarchical orchestration where a harness itself becomes the worker controlled by a longer-lived meta-runtime.[^103]

## 5.6 Security moves below the model

As agents gain shell, browser, credentials, and network access, security cannot depend primarily on the model refusing dangerous actions. Anthropic’s 2026 containment write-up argues for hard environment-level bounds using sandboxes, VMs, filesystem boundaries, credential isolation, and egress controls, with probabilistic classifiers as defense-in-depth. Its telemetry is also a warning about human approval loops: users approved roughly 93% of permission prompts, and the reported auto-mode classifier traded very low benign-command blocking for a non-zero miss rate on risky actions. Deterministic containment is the backstop.[^104]

This reframes a general-purpose harness as a security kernel. Every tool and subagent should receive explicit capabilities, not ambient authority; credentials should stay outside untrusted execution environments where possible; observations from the web and tool outputs should carry provenance/taint metadata; and delegation should never silently widen permissions.

## 5.7 Multi-agent: context isolation and parallel compute, not theater

Anthropic’s research system uses an orchestrator-worker pattern in which a lead agent decomposes a query and parallel subagents search independently. Cursor now gives subagents isolated VMs and clean contexts. These designs expose the strongest reasons to use multiple agents: parallelism, fresh context windows, specialization, independent checks, and failure isolation.[^105][^106]

Recursive Agent Harnesses generalizes the unit of delegation from a bare model call to a full harness with filesystem, code execution, planning, and its own ability to spawn children. In its controlled Oolong-Synthetic evaluation, holding GPT-5 fixed, the authors report an increase from a 71.75% Codex-style baseline to 81.36%. Prime Agent operationalizes related ideas with a persistent IPython runtime, continual harness state, recursive subagents, direct agent-to-agent communication, and daemon-backed sessions. Both are promising; both remain fresh 2026 research that needs broader replication.[^107][^108]

## 5.8 Harness self-improvement is the most distinctive new research frontier

Earlier work optimized prompts or workflow graphs. The 2026 shift is to treat the harness itself as an editable compound artifact and to learn from execution trajectories. Several families are emerging:

- Observability-driven evolution. Agentic Harness Engineering exposes editable components, distills trajectory evidence, attaches predictions to edits, and verifies the effect in the next round. Its reported gains transfer across some model families, and its ablations attribute more gain to tools, middleware, and long-term memory than to the system prompt alone.[^109]

- Retrospective self-supervision. RHO selects difficult prior trajectories, re-solves them, generates harness updates, and chooses among candidates using self-validation and pairwise self-preference; the paper reports a one-round SWE-Bench Pro pass-rate increase from 59% to 78% without external grading and is accepted to Findings of EMNLP 2026.[^110]

- Experience-conditioned adaptation. MemoHarness decomposes a harness into six editable control dimensions, stores local diagnoses plus global patterns, and retrieves relevant experience to adapt the harness for each test case without test-time labels or search. Its authors explicitly leave broader statistical robustness and component attribution open.[^111]

- Outer-loop executable-code search. Meta-Harness gives an agentic proposer filesystem access to prior harness source, scores, and traces and searches over harness code end to end. It reports improvements across online classification, retrieval-augmented math, and TerminalBench-2, including a discovered math harness that transfers across five held-out models. This matters because the optimization object is executable runtime code, not only natural-language prompts.[^112]

- Learned failure-conditioned harness editing. Harness-R1 post-trains a dedicated harness engineer with online reinforcement learning: batches of target-agent failures are converted into validated executable runtime patches, then fresh reruns of the frozen target agent supply outcome rewards. Its reported gains across WebShop, ALFWorld, and DBBench suggest that editing the runtime from trajectories can itself be learned as a reusable capability.[^113]

- Evaluation-efficient evolution. Task-CoEvolve treats validation selection as part of harness optimization: it concentrates evaluation on tasks where candidate harnesses disagree and corrects for the resulting sampling distribution. In its reported text-classification and Terminal-Bench experiments, it matches full-set search while cutting harness-optimization evaluations by about 80%. If harness evolution becomes routine, value-of-information scheduling will matter almost as much as the edit operator itself.[^114]

- Harness generation and evolution as a model capability. Evo-Bench measures intrinsic harness-evolving ability; HarnessDev asks models to create runnable harness infrastructure and then evolve it; JIT-Agent trains a dedicated “harness intelligence” model to synthesize task-adaptive harnesses on demand.[^115][^116][^117]

The counterweight is important. HarnessDev finds that generated harnesses still trail mature human references in code and search/research, evolution gains can be unstable, and benefits are strongly dependent on the model executing the harness. A separate controlled evaluation on Terminal-Bench 2.1 compares automatic harness evolution with simple test-time search/discovery under matched feedback and inference budgets; with GPT-5.4 and Claude Opus 4.6, it finds that evolution does not consistently win and generalizes only weakly to held-out tasks. The frontier is real, but benchmark reuse, search-budget confounds, and transfer failure make many headline gains less conclusive than they first appear.[^118]

## 5.9 Model-harness co-design and “assumption debt”

Harnesses encode assumptions about model weaknesses, training interfaces, and environment behavior. Cursor’s model-specific tool formats and prompts show intentional co-design. Anthropic’s Managed Agents story shows the inverse problem: once the model changes, old workarounds can constrain the stronger model. A useful concept is harness assumption debt: every special rule is a hypothesis about a failure mode that should have evidence, ownership, an expiry condition, and a regression test.[^119][^120]

| **Frontier implication.** A mature harness should be able to remove scaffolding, not only add it. Continuous evolution without deletion creates a fossil record of old model deficiencies inside the runtime. |
|----|

The newest papers go beyond tailoring a fixed harness to a fixed model and instead optimize the pair. Co-Harness alternates a harness critic that proposes validated local runtime updates with model fine-tuning on trajectories generated under the improved harness. WHALE alternates weight updates with Meta-Harness search and, on Qwen3.5-2B/4B agents across search QA, mathematics, and chess, reports best-mean@8 gains of 4.15–24.38 percentage points over weight-only, harness-only, and a fast/slow-training baseline. The conceptual move is to treat weights and harness as coupled coordinates of one learning system: changing either can move the bottleneck into the other.[^121][^122]

SafeEvolve extends co-evolution into safety alignment. It converts on-policy safety trajectories into bounded, auditable, reversible updates to safety prompts and hierarchical skills while separately applying harness-use SFT and harness-augmented RL to the policy. The authors report a threefold attack-success-rate reduction for Qwen3.5-4B on AgentDojo while modestly improving benign utility. This is exactly the direction a general-purpose self-modifying harness will need to confront: runtime adaptation cannot be separated from governance over which edits are allowed, reversible, attributable, and safe to deploy.[^123]

## 5.10 Protocolized edges

The harness is becoming more modular because its boundaries are being standardized. MCP primarily covers agent/model to tools and data; A2A covers agent-to-agent discovery and task communication; ACP standardizes editor/client-to-coding-agent interaction and supports session creation/resume, updates, permissions, and cancellation. These protocols do not solve orchestration, safety, or memory by themselves, but they make those concerns replaceable rather than hard-wired.[^124][^125][^126]

# 6. What the evidence actually establishes

Because “harness engineering” is young, evidence quality varies sharply. The table below separates mature findings from intriguing but provisional claims.

| **Claim** | **Best current evidence** | **Assessment** | **Main caveat** |
|----|----|----|----|
| Harness configuration can materially change outcomes with fixed model weights. | Harness-Bench; Same Model, Different Harness; production A/B practice at Cursor. | High confidence | Effect size is task, context-window, model, and harness dependent. |
| Context policy is a major long-horizon performance lever. | Anthropic context engineering; Cursor dynamic context; controlled same-model study. | High confidence | Specific compaction/retrieval recipes become stale as model context capability improves. |
| Tool interface design affects agent performance. | SWE-agent ACI; Anthropic tool engineering; Cursor model-specific tools. | High confidence | General principles transfer better than exact schemas. |
| Deterministic verification and environment feedback improve reliability. | Coding-agent practice; PAL; OpenAI/Anthropic production harnesses. | High confidence | Validation coverage can be incomplete; agents can optimize the checker. |
| Hard containment should bound agent blast radius. | Anthropic production security experience; established OS/container security principles. | High confidence | Containment can reduce observability/usability and still has implementation flaws. |
| Multi-agent decomposition helps when it provides parallelism or clean context. | Anthropic research system; RAH; Cursor isolated subagents. | Medium-high | Coordination overhead and cost can erase gains; benchmarks remain narrow. |
| Harnesses can learn useful improvements from their own trajectories. | AHE; RHO; MemoHarness. | Medium | Limited replication; attribution and self-evaluation can be biased. |
| Models can autonomously create competitive general-purpose harnesses. | Evo-Bench; HarnessDev; JIT-Agent. | Promising but provisional | HarnessDev shows domain/model dependence and unstable transfer; newest claims are preprints. |
| A fully deterministic outer program is superior to model-owned orchestration. | LLM-as-Code; Predictable Agentic Systems. | Open question | Likely task-specific tradeoff between flexibility and reproducibility. |
| Cross-process/event-sourced harnesses are the right default runtime. | Logos; Managed Agents; Symphony/Cursor operational patterns. | Strong systems hypothesis | Academic evidence is early; production details are selective and domain-specific. |

## The benchmark problem

Agent benchmarks often entangle model quality, harness quality, environment reliability, and evaluation infrastructure. Harness-Bench directly addresses configuration effects with shared environments, budgets, and protocols across 5,194 trajectories. SWE-agent showed earlier that the ACI itself changes outcomes. Current benchmark science therefore needs factorial experiments rather than a single score attached to a model name.[^127][^128]

A second problem is evaluating adaptive harnesses without clean labels. The August 2026 continual-learning study proposes teacher-relative lift: sparse corrections from a stronger teacher are used to measure whether a student+harness converges toward the teacher; in its cybersecurity setting, this correlated with a held-out gold standard, while similarly powered LLM-as-judge provided little useful signal. The approach is domain-limited but points toward evaluation methods for deployed systems that cannot constantly obtain ground truth.[^129]

# 7. Industry architectures and open-source systems

Production systems reveal what survives contact with real workloads. They also contain product-specific assumptions, so they should be mined for mechanisms rather than copied wholesale.

## OpenAI: repository legibility, the Codex harness, and orchestration above the session

OpenAI’s 2026 harness-engineering case study treats the repository itself as part of the runtime: short navigational agent instructions point into structured documentation; architectural constraints are enforced by code and tests; browser/UI and observability data are made legible to the agent; and agent-to-agent review loops replace much routine human review. This is “environment engineering” as harness engineering.[^130]

Symphony moves one level higher. Rather than supervising chat sessions, it uses a project tracker as a control plane, gives each issue an isolated workspace, reconciles running agents with work state, retries failures, and asks agents to carry work through CI/review. The reference is intentionally minimal - a spec plus implementation - and is better understood as an orchestration pattern than a complete general-purpose harness.[^131][^132]

## Anthropic: context, tools, long-horizon state, and containment

Anthropic’s engineering sequence is unusually coherent: simple composable agents (2024), multi-agent orchestration and tool design (2025), context engineering and skills, long-running persistence, then 2026 work on specialized harnesses, decoupled managed-agent runtime, and environment-level containment. The recurring principle is to keep model-facing interfaces simple while making state, tools, and safety explicit outside the model.[^133][^134][^135][^136][^137][^138]

## Cursor: continuous model-harness tuning and always-on agents

Cursor describes harness work as a product optimization loop: form hypotheses, evaluate offline, A/B test online, instrument tool errors, and tune prompts/tools per model family and version. It also documents the deletion of old guardrails as models improve. By August 2026, cloud agents could subscribe to PRs/Slack/schedules, retain long-lived goals, wake on events, and run subagents in isolated VMs - a concrete move from interactive assistant to durable agent system.[^139][^140]

## Open-source codebases worth studying

| **Repository** | **Why study it** | **Architectural signal** | **Status / caveat** |
|----|----|----|----|
| openai/codex | Production-grade open coding harness in Rust | Core agent loop, tool execution, sandbox/approval semantics, app-server surface | Best for tracing a mature, model-specific coding harness. |
| openai/symphony | Minimal orchestration spec + reference implementation | Issue tracker as control plane; isolated workspaces; retries/reconciliation | Orchestration layer, not a standalone intelligence layer. |
| OpenHands/software-agent-sdk | Composable agent SDK and server | Agents, tools, conversations, workspaces, events, remote ephemeral execution | Useful service decomposition and production API boundaries. |
| anomalyco/opencode | Large open coding agent | Provider abstraction, plan/build modes, tools, plugins, mature product surface | Large codebase; study targeted subsystems rather than all at once. |
| aaif-goose/goose | General-purpose local agent | Multi-provider, MCP/ACP edges, CLI/desktop/API, Rust runtime | Good non-code-only comparison. |
| badlogic/pi-mono | Compact agent toolkit and coding runtime | Unified model API, core runtime, session/state, extensibility, subagents | Especially useful for readable architecture and experimentation. |
| SWE-agent / mini-SWE-agent | Research baseline emphasizing minimalism | Simple loop, ACI design, hackability; project recommends mini variant | Ideal baseline for ablation experiments. |
| PrimeIntellect-ai/prime-agent | Frontier self-improving/recursive prototype | Persistent REPL, continual harness, recursive subagents, daemon sessions | Very new; claims need independent replication. |
| HKUDS/OpenHarness | Explicit open harness project | Tools, skills, plugins, multi-agent and personal-agent concepts | Early-stage; useful exploration surface, not a canonical reference. |

Repository sources and current descriptions: Codex, Symphony, OpenHands SDK, OpenCode, Goose, Pi, SWE-agent, Prime Agent, and OpenHarness.[^141][^142][^143][^144][^145][^146][^147][^148][^149]

A 2026 source-code study of eleven coding systems is valuable as a map rather than a final taxonomy. It reports recurring hand-rolled async loops, deterministic code retrieval rather than vector-embedding retrieval across its audited systems, high adoption of skills/MCP/ACP, and an evolution from “tool” toward “platform.” Those findings are auditable within its corpus but should not be generalized beyond the studied coding-agent population without further work.[^150]

# 8. Evaluation and benchmark science

A harness is an intervention on a stochastic system. Good harness engineering therefore needs experimental design closer to systems benchmarking than prompt tinkering.

- Version everything: model snapshot/provider, harness commit, system prompt, tools, context policy, sandbox image, protocol versions, benchmark data, graders, and resource budgets.

- Use paired/factorial comparisons. Hold model and task fixed while varying one harness mechanism; repeat across multiple model families to measure transfer.[^151][^152]

- Measure process as well as final success: tool errors, invalid actions, retries, context usage, evidence alignment, state divergence, time, dollar cost, and resource consumption.[^153][^154]

- Prefer executable/end-state oracles when possible. LLM judges are useful for qualitative dimensions but can be correlated with the same failure modes as the tested agent.

- Report distributions, not only means. Long-horizon agents have heavy-tail failures: one runaway loop, corrupted state, or unrecoverable action can dominate operational risk.

- Test robustness across capability changes. A harness improvement that disappears with a larger context window or reverses on a new model should be identified as conditional, not universal.[^155][^156]

- For self-modifying harnesses, separate proposal from deployment. Generate candidate changes from traces, evaluate in an isolated regression suite, perform transfer tests, then canary and retain rollback.

## A useful harness scorecard

| **Dimension** | **Metrics** | **Question** |
|----|----|----|
| Capability | task success, partial credit, coverage, quality rubric | Does it complete the work? |
| Reliability | variance, pass^k, crash/recovery rate, duplicate effects | Does it keep working over repeated long runs? |
| Grounding | validator pass, state/claim consistency, evidence traceability | Is reasoning synchronized with the world? |
| Efficiency | tokens, cache hit, tool calls, compute, latency, dollars | What does each unit of successful work cost? |
| Autonomy | human interventions, approval burden, steering frequency | How much supervision does it consume? |
| Security | permission violations, blocked exfiltration, blast radius, audit completeness | What can go wrong, and how far can it spread? |
| Portability | effect across models/tasks/providers/environments | Is the harness improvement general or overfit? |
| Evolvability | time to diagnose, attribution quality, safe rollback | Can the system improve without accumulating opaque debt? |

# 9. What is genuinely new - and what is renamed

Harness engineering is partly a new label over familiar systems ideas. State machines, retries, sandboxes, observability, workflow engines, capability security, event logs, and multi-process fault isolation are not novel because an LLM is involved. ReAct resembles classical perception-action loops; memory architectures have long histories; planner-worker patterns predate foundation models.

What is genuinely new is the object being controlled: a highly capable, stochastic, instruction-conditioned program synthesizer that can interpret natural language, write new tools, change its own workspace, and reason about arbitrary interfaces at runtime. That creates unusual engineering properties:

- The interface is partly semantic. Tool names/descriptions and context formatting change behavior, so APIs are simultaneously software contracts and learned affordances.

- Control is movable. As the model improves, logic can migrate from deterministic code into model choice - or be pulled back into code for reproducibility.

- The system can help redesign itself. A model can inspect traces, propose prompt/tool/middleware changes, generate evaluators, and sometimes author the next harness version.[^157][^158][^159]

- The harness can change effective capability without changing weights. This makes inference-time systems engineering an orthogonal scaling axis to pretraining and post-training.[^160][^161]

- The runtime is now exposed to adversarial natural-language inputs that can influence action-taking, creating a distinctive coupling between classic security boundaries and prompt-injection risk.[^162]

# 10. Open research problems

- **Harness representation.** What is the right intermediate representation for prompts, context policies, tool interfaces, state machines, validators, permissions, memory, and subagent topology so harnesses can be compared, compiled, and evolved independently of a framework?

- **Causal attribution.** When a run improves after a harness edit, which component caused the gain? Can trajectory interventions support reliable counterfactual attribution rather than post-hoc stories?

- **Safe self-modification.** How can an agent propose and test changes without reward hacking, disabling safeguards, overfitting a local regression suite, or silently widening permissions?

- **Assumption debt.** How should model-specific workarounds carry evidence, expiry criteria, and automated retirement tests as model capabilities change?

- **Memory semantics.** How should agents store conflicting, stale, adversarial, or low-confidence experience? Vector similarity alone is not a truth or trust model.

- **Execution alignment.** How can a harness detect that the model’s internal narrative has diverged from actual workspace/tool state before a false completion propagates?

- **Value-of-compute orchestration.** When is a subagent, alternate model, evaluator, or longer search worth its marginal tokens, latency, and coordination risk?

- **Multi-agent consistency.** How should parallel agents coordinate writes, merge beliefs, reserve resources, and resolve conflicts without a single giant shared context?

- **Security capability systems.** Can agent permissions be modeled as explicit transferable capabilities with taint/provenance and least-privilege delegation across tools and subagents?

- **Durable semantics.** What are the right idempotency, checkpoint, transaction, and exactly-once/at-least-once semantics for real-world agent actions?

- **Benchmark science.** How do we measure harness effect size, interaction effects with models, long-tail failure, and transfer without leaking benchmark-specific policy into the harness?

- **Human-agent organizations.** When work is managed at ticket/goal level instead of turn level, what are the right escalation, review, accountability, and observability interfaces for human teams?

# 11. Blueprint for a general-purpose harness

The most useful starting point is not a multi-agent framework. It is a minimal, inspectable single-agent runtime whose components can be independently measured and replaced. Multi-agent and self-improvement should be later capabilities built on top of durable semantics.

## Core architecture

| **Component** | **Recommended default** | **Why** |
|----|----|----|
| Run/event store | Append-only typed event log with immutable run, turn, tool, artifact and subagent IDs | Replay, resumability, provenance, postmortem analysis, deterministic reconstruction. |
| Model adapter | Common inference interface plus model-specific profiles for prompts, tool shapes, context limits and caching | Preserves portability without pretending models are behaviorally interchangeable. |
| Tool registry | Typed capabilities, schemas, cost/risk metadata, MCP bridge, dynamic discovery | Makes tool space scalable and security-aware. |
| Execution environments | Ephemeral sandbox/VM handles separate from harness process; credentials outside where possible | Hard blast-radius boundary and independent failure/replacement. |
| Context builder | Policy engine for working set, retrieval, compaction, tool-result offload, skills, and provenance | Treats tokens as a budgeted working memory, not a transcript dump. |
| Artifact store | Filesystem/object store + git-like versioning for plans, outputs, evidence and checkpoints | Universal collaboration/debugging surface across humans and agents. |
| Control envelope | Budgets, stop/retry rules, state invariants, timeout, schema checks, idempotency keys | Deterministic constraints around open-ended model tactics. |
| Verification fabric | Task contracts, executable validators, tests, evidence requirements, optional independent evaluators | Continuously reconciles claims with external state. |
| Scheduler | Single-agent initially; later subagent spawning with isolated context/environment and explicit resource budget | Avoids premature coordination complexity. |
| Observability/evals | Trace viewer, component versioning, regression suites, online A/B metrics, cost/latency dashboards | Makes harness changes scientific rather than anecdotal. |
| Evolution service | Offline proposal -\> attribution -\> regression -\> transfer -\> canary -\> rollback pipeline | Allows safe harness learning without direct self-deployment. |

## A staged build sequence

7.  **Stage 0 - minimal baseline:** Implement a ReAct-style loop with filesystem, shell/code execution, one structured tool API, explicit token/tool/time budgets, and a complete trace. Keep it small enough to understand line by line.

8.  **Stage 1 - deterministic runtime semantics:** Add typed events, retry/timeout policy, idempotency keys, checkpoints, artifact IDs, sandbox handles, and a validator interface. Make every side effect attributable to a run and tool call.

9.  **Stage 2 - context and procedural memory:** Implement tool-result offloading, selective retrieval, compaction, skills, and resumable progress artifacts. Record provenance and timestamps for every retrieved memory.

10. **Stage 3 - evaluation first:** Create a private mixed-domain suite before adding clever architecture. Run model x harness factorials, cost-adjusted metrics, and long-horizon failure injection.

11. **Stage 4 - subagents only for measurable reasons:** Add delegation for parallelizable subtasks, clean-context specialization, or independent verification. Use isolated workspaces/contexts and a shared artifact/event layer rather than a giant transcript.

12. **Stage 5 - model-conditioned harness profiles:** Learn which tool shapes, prompts, compaction rules, and context policies differ by model. Explicitly test deletion of obsolete guardrails.

13. **Stage 6 - adaptive harness evolution:** Use traces to propose component changes, attach predicted effects, evaluate offline, test cross-model/task transfer, and canary. Do not permit the same agent that proposes a safety-critical change to unilaterally deploy it.

## The architectural stance to prefer

| **Recommended stance.** Build a deterministic substrate with an increasingly autonomous policy layer. Keep the state model, security boundaries, tool execution, resource accounting, validation interfaces, and recovery semantics explicit in software. Let the model decide how to explore and decompose within those boundaries. Expose enough of the harness as data/configuration that an optimizer can later edit it safely. |
|----|

# 12. Contribution map: where novel work is most likely

The landscape is already crowded with generic orchestration libraries. Novelty is more likely in mechanisms that make harnesses portable, self-correcting, durable, measurable, or safe.

| **Research direction** | **Novel contribution** | **Why it matters now** | **Difficulty / risk** |
|----|----|----|----|
| 1\. Harness IR + compiler | A framework-neutral declarative representation for context, tools, permissions, state, validators, routing and subagents; compile to multiple runtimes/protocols. | Enables reproducible comparison, automated optimization and portability. | High systems-design challenge; risks lowest-common-denominator abstraction. |
| 2\. Assumption-debt manager | Every rule records hypothesized model deficiency, evidence, expected effect, owner, expiry and removal test. | Directly addresses stale scaffolding documented by production teams. | Requires disciplined experiments and regression infrastructure. |
| 3\. Safe adaptive optimizer | Combine trace diagnosis with causal attribution, candidate search, security constraints, transfer tests, canaries and rollback. | Self-evolving harnesses are the clearest 2026 academic frontier but lack mature deployment safety. | High reward-hacking and overfitting risk. |
| 4\. Execution-alignment layer | Continuously reconcile model claims/plans with workspace state, tool outputs, evidence, and task contracts. | Targets a failure class identified directly by Harness-Bench. | Needs domain-general state representations and validators. |
| 5\. Durable agent runtime | Event-sourced goals, wakeups, leases, retries, checkpoints, idempotency and fault injection for multi-day agents. | Production systems are converging on distributed-systems semantics without a canonical open runtime. | Hard correctness work; side effects can be irreversible. |
| 6\. Value-of-compute scheduler | Predict whether to spawn a subagent/evaluator/alternate model based on expected marginal success per token/latency/risk. | Multi-agent use currently relies heavily on heuristics. | Requires online estimation under nonstationary models. |
| 7\. Tool-interface compiler | Retrieve and adapt tool surfaces per task/model; compress schemas; generate wrappers; attach cost/risk/permission metadata. | Tool catalogs are scaling faster than context windows and models are sensitive to interface shape. | Safety and semantic equivalence of generated wrappers. |
| 8\. Trust-aware memory | Memory with provenance, confidence, expiry, conflict handling, taint, and counterfactual utility tests - not just vector similarity. | Persistent agent state creates both capability and poisoning risk. | Evaluation is difficult without long-lived realistic deployments. |
| 9\. Capability security kernel | Explicit least-privilege handles transferable to tools/subagents, credential isolation, audited egress and provenance-aware delegation. | Autonomy expands blast radius; environment containment is becoming central. | Security must be correct against adversarial agents and external inputs. |
| 10\. Harness benchmark science | Factorial model x harness x environment studies, effect sizes, robustness, cost Pareto curves and transfer metrics. | The field still over-attributes system results to model names. | Expensive experiments and fast-moving model versions. |

## Highest-leverage thesis

A particularly strong research program would combine directions 1, 2, 3, and 4: define a portable harness IR; attach explicit hypotheses and telemetry to every editable component; use trajectory evidence to generate candidate changes; evaluate them against execution-alignment and safety constraints; and automatically retire improvements that stop transferring as model capability changes. This would turn harness engineering from an accumulation of clever tricks into a disciplined adaptive control system.

# 13. Reading and repository syllabus

A compact path from foundations to the frontier:

## Tier 1 - conceptual foundations

- Vaswani et al. - Attention Is All You Need: understand the model substrate, while remembering it is not an agent paper.[^163]

- Brown et al. - GPT-3: understand inference-time programmability via context.[^164]

- Lewis et al. - RAG: external memory as a model-external capability.[^165]

- ReAct: the canonical reason-act-observe loop.[^166]

- PAL: deterministic runtime as complement to probabilistic reasoning.[^167]

- SWE-agent: interface design as an independent agent performance lever.[^168]

- Anthropic - Building effective agents: simple workflows vs agent-controlled loops.[^169]

## Tier 2 - production harness engineering

- OpenAI - Unrolling the Codex agent loop; then Harness engineering; then Symphony.[^170][^171][^172]

- Anthropic - Context engineering, tool design, Agent Skills, long-running harnesses, Managed Agents, and containment.[^173][^174][^175][^176][^177][^178]

- Cursor - Dynamic context discovery, Continually improving our agent harness, cloud-agent lessons, and the August 2026 always-on harness update.[^179][^180][^181][^182]

- LangChain - The Anatomy of an Agent Harness, mainly for the broad taxonomy and vocabulary.[^183]

## Tier 3 - 2026 research frontier

- Harness-Bench - start here for the measurement problem.[^184]

- Agentic Harness Engineering and RHO - trajectory-driven harness optimization.[^185][^186]

- MemoHarness - experience-conditioned per-case harness adaptation.[^187]

- Evo-Bench and HarnessDev - whether models can evolve/create harnesses, including negative/conditional evidence.[^188][^189]

- JIT-Agent - dedicated model for just-in-time harness synthesis; very fresh, high-claim preprint.[^190]

- Recursive Agent Harnesses and Prime Agent - full-harness recursion and continual runtime.[^191][^192]

- LLM-as-Code and Predictable Agentic Systems - deterministic-control pole of the design space.[^193][^194]

- Logos - cross-process, append-only, fault-isolated harness runtime.[^195]

- HEART / Tool Primitives - dynamic, agent-native tool-interface frontier; provisional.[^196]

- Source-Code Study of Eleven Systems - broad anatomy of contemporary coding harness implementations.[^197]

## Repositories to read in source

- openai/codex - mature coding harness internals.[^198]

- openai/symphony - minimal durable work orchestration.[^199]

- OpenHands/software-agent-sdk - server/runtime/workspace/event decomposition.[^200]

- badlogic/pi-mono - readable core runtime and embedding surface.[^201]

- SWE-agent / mini-SWE-agent - minimal research baselines and ACI lineage.[^202]

- anomalyco/opencode and aaif-goose/goose - large, active general/product harnesses.[^203][^204]

- PrimeIntellect-ai/prime-agent - frontier recursive/continual experimental architecture.[^205]

# Conclusion

Harness engineering is becoming a real research area, but it is still pre-paradigmatic. Its vocabulary is unsettled, most 2026 academic results are new, many are preprints, and coding tasks dominate the evidence. Yet the field already has a coherent object of study: the external runtime that converts model inference into stateful action. It has independent variables, measurable outcomes, emerging benchmarks, recognizable architecture patterns, and now algorithms that optimize the harness itself.

The deepest shift is conceptual. Foundation-model progress made it tempting to treat intelligence as residing entirely in weights. Agentic systems reveal a more cybernetic picture: useful intelligence emerges from the closed loop among model, context, tools, state, environment, feedback, and constraints. The harness is the engineered part of that loop. As models get stronger, its job is not to micromanage them; it is to make the world legible, actions safe and verifiable, state durable, coordination economical, failures diagnosable, and improvement experimentally accountable.

For a new general-purpose harness, the best opportunity is therefore not maximal scaffolding. It is a small, rigorous runtime with excellent semantics - explicit state, capabilities, artifacts, validation and tracing - that can expose its own policy layer to controlled evolution. That architecture would be simple enough to understand, strong enough to run for days, and structured enough to become a research instrument for discovering what harness intelligence should mean.

# Sources

Selected primary and high-value sources used in this report. Academic status is noted where especially relevant; recent arXiv papers should be read as provisional until replicated or peer reviewed.

## Foundational and pre-harness research

[<u>Vaswani et al. (2017), Attention Is All You Need</u>](https://arxiv.org/abs/1706.03762) https://arxiv.org/abs/1706.03762

[<u>Brown et al. (2020), Language Models are Few-Shot Learners</u>](https://arxiv.org/abs/2005.14165) https://arxiv.org/abs/2005.14165

[<u>Lewis et al. (2020), Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks</u>](https://arxiv.org/abs/2005.11401) https://arxiv.org/abs/2005.11401

[<u>Nakano et al. (2021), WebGPT</u>](https://arxiv.org/abs/2112.09332) https://arxiv.org/abs/2112.09332

[<u>Karpas et al. (2022), MRKL Systems</u>](https://arxiv.org/abs/2205.00445) https://arxiv.org/abs/2205.00445

[<u>Yao et al. (2022/2023), ReAct</u>](https://arxiv.org/abs/2210.03629) https://arxiv.org/abs/2210.03629

[<u>Gao et al. (2022/2023), PAL</u>](https://arxiv.org/abs/2211.10435) https://arxiv.org/abs/2211.10435

[<u>OpenAI (2022), Introducing ChatGPT</u>](https://openai.com/index/chatgpt/) https://openai.com/index/chatgpt/

[<u>Schick et al. (2023), Toolformer</u>](https://arxiv.org/abs/2302.04761) https://arxiv.org/abs/2302.04761

[<u>Shinn et al. (2023), Reflexion</u>](https://arxiv.org/abs/2303.11366) https://arxiv.org/abs/2303.11366

[<u>Wang et al. (2023), Voyager</u>](https://arxiv.org/abs/2305.16291) https://arxiv.org/abs/2305.16291

[<u>Wu et al. (2024), AutoGen</u>](https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/) https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/

[<u>Khattab et al. (2023/2024), DSPy</u>](https://arxiv.org/abs/2310.03714) https://arxiv.org/abs/2310.03714

[<u>Yang et al. (NeurIPS 2024), SWE-agent</u>](https://arxiv.org/abs/2405.15793) https://arxiv.org/abs/2405.15793

[<u>Zhang et al. (ICLR 2025), AFlow</u>](https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html) https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

## Industry engineering and protocols

[<u>Anthropic (2024), Building effective agents</u>](https://www.anthropic.com/engineering/building-effective-agents) https://www.anthropic.com/engineering/building-effective-agents

[<u>Anthropic (2024), Model Context Protocol</u>](https://www.anthropic.com/news/model-context-protocol) https://www.anthropic.com/news/model-context-protocol

[<u>Google (2025), Agent2Agent Protocol</u>](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/) https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/

[<u>Anthropic (2025), Multi-agent research system</u>](https://www.anthropic.com/engineering/multi-agent-research-system) https://www.anthropic.com/engineering/multi-agent-research-system

[<u>Anthropic (2025), Writing effective tools for agents</u>](https://www.anthropic.com/engineering/writing-tools-for-agents) https://www.anthropic.com/engineering/writing-tools-for-agents

[<u>Anthropic (2025), Effective context engineering</u>](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[<u>Anthropic (2025), Agent Skills</u>](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills) https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[<u>Anthropic (2025), Advanced tool use</u>](https://www.anthropic.com/engineering/advanced-tool-use) https://www.anthropic.com/engineering/advanced-tool-use

[<u>Anthropic (2025), Effective harnesses for long-running agents</u>](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents) https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[<u>OpenAI (2026), Unrolling the Codex agent loop</u>](https://openai.com/index/unrolling-the-codex-agent-loop/) https://openai.com/index/unrolling-the-codex-agent-loop/

[<u>Mitchell Hashimoto (2026), My AI Adoption Journey</u>](https://mitchellh.com/writing/my-ai-adoption-journey) https://mitchellh.com/writing/my-ai-adoption-journey

[<u>OpenAI (2026), Harness engineering</u>](https://openai.com/index/harness-engineering/) https://openai.com/index/harness-engineering/

[<u>Vivek Trivedy / LangChain (2026), The Anatomy of an Agent Harness</u>](https://www.langchain.com/blog/the-anatomy-of-an-agent-harness) https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[<u>Anthropic (2026), Harness design for long-running application development</u>](https://www.anthropic.com/engineering/harness-design-long-running-apps) https://www.anthropic.com/engineering/harness-design-long-running-apps

[<u>Anthropic (2026), Scaling Managed Agents</u>](https://www.anthropic.com/engineering/managed-agents) https://www.anthropic.com/engineering/managed-agents

[<u>OpenAI (2026), Symphony</u>](https://openai.com/index/open-source-codex-orchestration-symphony/) https://openai.com/index/open-source-codex-orchestration-symphony/

[<u>Cursor (2026), Continually improving our agent harness</u>](https://cursor.com/blog/continually-improving-agent-harness) https://cursor.com/blog/continually-improving-agent-harness

[<u>Cursor (2026), What we’ve learned building cloud agents</u>](https://cursor.com/blog/cloud-agent-lessons) https://cursor.com/blog/cloud-agent-lessons

[<u>Anthropic (2026), How we contain Claude across products</u>](https://www.anthropic.com/engineering/how-we-contain-claude) https://www.anthropic.com/engineering/how-we-contain-claude

[<u>Cursor (2026), Cloud Agents and Cursor Harness Improvements</u>](https://cursor.com/changelog/08-19-26) https://cursor.com/changelog/08-19-26

[<u>Agent Client Protocol</u>](https://github.com/agentclientprotocol/agent-client-protocol) https://github.com/agentclientprotocol/agent-client-protocol

## 2026 harness research frontier

[<u>Lin et al., Agentic Harness Engineering</u>](https://arxiv.org/abs/2604.25850) https://arxiv.org/abs/2604.25850

[<u>Yao et al., Harness-Bench</u>](https://arxiv.org/abs/2605.27922) https://arxiv.org/abs/2605.27922

[<u>Pan et al., Retrospective Harness Optimization (Findings of EMNLP 2026)</u>](https://arxiv.org/abs/2606.05922) https://arxiv.org/abs/2606.05922

[<u>Lumer et al., Recursive Agent Harnesses</u>](https://arxiv.org/abs/2606.13643) https://arxiv.org/abs/2606.13643

[<u>Qi et al., LLM-as-Code</u>](https://arxiv.org/abs/2606.15874) https://arxiv.org/abs/2606.15874

[<u>Huang et al., MemoHarness</u>](https://arxiv.org/abs/2607.14159) https://arxiv.org/abs/2607.14159

[<u>Huang et al., Evo-Bench</u>](https://arxiv.org/abs/2608.09096) https://arxiv.org/abs/2608.09096

[<u>Luthra et al., Evaluating Agentic Learning Harness Capabilities Without Labels</u>](https://arxiv.org/abs/2608.13608) https://arxiv.org/abs/2608.13608

[<u>Karten et al., Prime Agent</u>](https://arxiv.org/abs/2608.23552) https://arxiv.org/abs/2608.23552

[<u>Dhage, Harness Engineering for Predictable Agentic Systems</u>](https://arxiv.org/abs/2608.26197) https://arxiv.org/abs/2608.26197

[<u>Zhang et al., JIT-Agent</u>](https://arxiv.org/abs/2608.25593) https://arxiv.org/abs/2608.25593

[<u>Lewis, Same Model, Different Harness</u>](https://arxiv.org/abs/2608.26218) https://arxiv.org/abs/2608.26218

[<u>Jia et al., Logos</u>](https://arxiv.org/abs/2608.28553) https://arxiv.org/abs/2608.28553

[<u>Barbaste et al., Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents</u>](https://arxiv.org/abs/2609.00006) https://arxiv.org/abs/2609.00006

[<u>Wu et al., HarnessDev</u>](https://arxiv.org/abs/2609.01437) https://arxiv.org/abs/2609.01437

[<u>Jin et al., Harness Engineering in LLM Tool Use via Agent-Native Reusable Tool Primitives</u>](https://arxiv.org/abs/2609.01736) https://arxiv.org/abs/2609.01736

## Open-source systems

[<u>OpenAI Codex</u>](https://github.com/openai/codex) https://github.com/openai/codex

[<u>OpenAI Symphony</u>](https://github.com/openai/symphony) https://github.com/openai/symphony

[<u>Anthropic Claude Agent SDK for Python</u>](https://github.com/anthropics/claude-agent-sdk-python) https://github.com/anthropics/claude-agent-sdk-python

[<u>OpenHands Software Agent SDK</u>](https://github.com/OpenHands/software-agent-sdk) https://github.com/OpenHands/software-agent-sdk

[<u>OpenCode</u>](https://github.com/anomalyco/opencode) https://github.com/anomalyco/opencode

[<u>Goose</u>](https://github.com/aaif-goose/goose) https://github.com/aaif-goose/goose

[<u>Pi</u>](https://github.com/badlogic/pi-mono) https://github.com/badlogic/pi-mono

[<u>SWE-agent</u>](https://github.com/SWE-agent/SWE-agent) https://github.com/SWE-agent/SWE-agent

[<u>Prime Agent</u>](https://github.com/PrimeIntellect-ai/prime-agent) https://github.com/PrimeIntellect-ai/prime-agent

[<u>OpenHarness</u>](https://github.com/HKUDS/OpenHarness) https://github.com/HKUDS/OpenHarness

## Late-2026 frontier addendum

[<u>Wu & Canedo (2026), Invalidation Contracts for Cross-Episode Agent Memory</u>](https://arxiv.org/abs/2609.00243) https://arxiv.org/abs/2609.00243

[<u>Yu et al. (2026), Shepherd: A Runtime Substrate Empowering Meta-Agents with a Formalized Execution Trace</u>](https://arxiv.org/abs/2605.10913) https://arxiv.org/abs/2605.10913

[<u>Yan et al. (2026), Harness-of-Harness: Multi-Day Autonomous Software Development with Continual Improvement</u>](https://arxiv.org/abs/2609.01481) https://arxiv.org/abs/2609.01481

[<u>Lee et al. (2026), Meta-Harness: End-to-End Optimization of Model Harnesses</u>](https://arxiv.org/abs/2603.28052) https://arxiv.org/abs/2603.28052

[<u>Shao et al. (2026), Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories</u>](https://arxiv.org/abs/2608.02276) https://arxiv.org/abs/2608.02276

[<u>Miyai et al. (2026), Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection</u>](https://arxiv.org/abs/2608.20169) https://arxiv.org/abs/2608.20169

[<u>Wang et al. (2026), Rethinking the Evaluation of Harness Evolution for Agents</u>](https://arxiv.org/abs/2607.12227) https://arxiv.org/abs/2607.12227

[<u>Chen et al. (2026), Co-Harness: Co-Evolving Harnesses and Model Weights for LLM Agents</u>](https://arxiv.org/abs/2607.22688) https://arxiv.org/abs/2607.22688

[<u>Kim et al. (2026), WHALE: A Simple Recipe for Joint Harness-Weight Optimization</u>](https://arxiv.org/abs/2609.00196) https://arxiv.org/abs/2609.00196

[<u>Mao et al. (2026), SafeEvolve: Harness-Policy Co-Evolution from Agent Experience for Safety Alignment</u>](https://arxiv.org/abs/2609.02786) https://arxiv.org/abs/2609.02786

[^1]: Ashish Vaswani et al., “Attention Is All You Need,” arXiv:1706.03762 (2017). https://arxiv.org/abs/1706.03762

[^2]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^3]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^4]: Mitchell Hashimoto, “My AI Adoption Journey,” February 5, 2026. https://mitchellh.com/writing/my-ai-adoption-journey

[^5]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^6]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^7]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^8]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^9]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^10]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^11]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^12]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^13]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^14]: Saransh Dhage, “Harness Engineering for Predictable Agentic Systems: An Empirical Study of Deterministic Execution Constraints,” arXiv:2608.26197, August 25, 2026. https://arxiv.org/abs/2608.26197

[^15]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^16]: Josh Ma, “What we’ve learned building cloud agents,” Cursor, June 2, 2026. https://cursor.com/blog/cloud-agent-lessons

[^17]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^18]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^19]: Anthropic, “How we built our multi-agent research system,” June 13, 2025. https://www.anthropic.com/engineering/multi-agent-research-system

[^20]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^21]: Seth Karten et al., “Prime Agent: A Self-Improving RLM Harness,” arXiv:2608.23552, August 24, 2026. https://arxiv.org/abs/2608.23552

[^22]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^23]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^24]: Mitchell Hashimoto, “My AI Adoption Journey,” February 5, 2026. https://mitchellh.com/writing/my-ai-adoption-journey

[^25]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^26]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^27]: Ashish Vaswani et al., “Attention Is All You Need,” arXiv:1706.03762 (2017). https://arxiv.org/abs/1706.03762

[^28]: Tom B. Brown et al., “Language Models are Few-Shot Learners,” arXiv:2005.14165 (2020). https://arxiv.org/abs/2005.14165

[^29]: Patrick Lewis et al., “Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks,” arXiv:2005.11401 (2020). https://arxiv.org/abs/2005.11401

[^30]: Reiichiro Nakano et al., “WebGPT: Browser-assisted question-answering with human feedback,” arXiv:2112.09332 (2021). https://arxiv.org/abs/2112.09332

[^31]: Ehud Karpas et al., “MRKL Systems: A modular, neuro-symbolic architecture that combines large language models, external knowledge sources and discrete reasoning,” arXiv:2205.00445 (2022). https://arxiv.org/abs/2205.00445

[^32]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^33]: Luyu Gao et al., “PAL: Program-aided Language Models,” ICML 2023; arXiv:2211.10435. https://arxiv.org/abs/2211.10435

[^34]: OpenAI, “Introducing ChatGPT,” November 30, 2022. https://openai.com/index/chatgpt/

[^35]: Timo Schick et al., “Toolformer: Language Models Can Teach Themselves to Use Tools,” NeurIPS 2023; arXiv:2302.04761. https://arxiv.org/abs/2302.04761

[^36]: Noah Shinn et al., “Reflexion: Language Agents with Verbal Reinforcement Learning,” arXiv:2303.11366 (2023). https://arxiv.org/abs/2303.11366

[^37]: Guanzhi Wang et al., “Voyager: An Open-Ended Embodied Agent with Large Language Models,” arXiv:2305.16291 (2023). https://arxiv.org/abs/2305.16291

[^38]: Qingyun Wu et al., “AutoGen: Enabling Next-Gen LLM Applications via Multi-Agent Conversation,” COLM 2024. https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/

[^39]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^40]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^41]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^42]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^43]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^44]: Anthropic, “Introducing advanced tool use on the Claude Developer Platform,” November 24, 2025. https://www.anthropic.com/engineering/advanced-tool-use

[^45]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^46]: Jiayi Zhang et al., “AFlow: Automating Agentic Workflow Generation,” ICLR 2025. https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

[^47]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^48]: Mitchell Hashimoto, “My AI Adoption Journey,” February 5, 2026. https://mitchellh.com/writing/my-ai-adoption-journey

[^49]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^50]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^51]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^52]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^53]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^54]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^55]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^56]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^57]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^58]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^59]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^60]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^61]: Cursor Team, “Expanding our long-running agents research preview,” February 12, 2026. https://cursor.com/blog/long-running-agents

[^62]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^63]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^64]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^65]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^66]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^67]: Google, “Announcing the Agent2Agent Protocol (A2A),” April 9, 2025. https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/

[^68]: Agent Client Protocol project, “Agent Client Protocol,” GitHub. https://github.com/agentclientprotocol/agent-client-protocol

[^69]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^70]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^71]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^72]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^73]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^74]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^75]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^76]: Jediah Katz, “Dynamic context discovery,” Cursor, January 6, 2026. https://cursor.com/blog/dynamic-context-discovery

[^77]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^78]: Noah Shinn et al., “Reflexion: Language Agents with Verbal Reinforcement Learning,” arXiv:2303.11366 (2023). https://arxiv.org/abs/2303.11366

[^79]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^80]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^81]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^82]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^83]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^84]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^85]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^86]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^87]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^88]: Anthropic, “Introducing advanced tool use on the Claude Developer Platform,” November 24, 2025. https://www.anthropic.com/engineering/advanced-tool-use

[^89]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^90]: Haibo Jin et al., “Harness Engineering in LLM Tool Use via Agent-Native Reusable Tool Primitives,” arXiv:2609.01736, September 1, 2026. https://arxiv.org/abs/2609.01736

[^91]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^92]: Prithvi Rajasekaran, “Harness design for long-running application development,” Anthropic, March 24, 2026. https://www.anthropic.com/engineering/harness-design-long-running-apps

[^93]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^94]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^95]: Saransh Dhage, “Harness Engineering for Predictable Agentic Systems: An Empirical Study of Deterministic Execution Constraints,” arXiv:2608.26197, August 25, 2026. https://arxiv.org/abs/2608.26197

[^96]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^97]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^98]: Josh Ma, “What we’ve learned building cloud agents,” Cursor, June 2, 2026. https://cursor.com/blog/cloud-agent-lessons

[^99]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^100]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^101]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^102]: Simon Yu et al., “Shepherd: A Runtime Substrate Empowering Meta-Agents with a Formalized Execution Trace,” arXiv:2605.10913, May 11, 2026. https://arxiv.org/abs/2605.10913

[^103]: Haoyang Yan et al., “Harness-of-Harness: Multi-Day Autonomous Software Development with Continual Improvement,” arXiv:2609.01481, September 1, 2026. https://arxiv.org/abs/2609.01481

[^104]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^105]: Anthropic, “How we built our multi-agent research system,” June 13, 2025. https://www.anthropic.com/engineering/multi-agent-research-system

[^106]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^107]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^108]: Seth Karten et al., “Prime Agent: A Self-Improving RLM Harness,” arXiv:2608.23552, August 24, 2026. https://arxiv.org/abs/2608.23552

[^109]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^110]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^111]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^112]: Yoonho Lee et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 30, 2026. https://arxiv.org/abs/2603.28052

[^113]: Shuai Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^114]: Atsuyuki Miyai, Kiyoharu Aizawa, and Toshihiko Yamasaki, “Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection,” arXiv:2608.20169, August 20, 2026. https://arxiv.org/abs/2608.20169

[^115]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^116]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^117]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^118]: Yike Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, July 14, 2026. https://arxiv.org/abs/2607.12227

[^119]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^120]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^121]: Zhengyu Chen et al., “Co-Harness: Co-Evolving Harnesses and Model Weights for LLM Agents,” arXiv:2607.22688, July 17, 2026. https://arxiv.org/abs/2607.22688

[^122]: Haechan Kim et al., “WHALE: A Simple Recipe for Joint Harness-Weight Optimization,” arXiv:2609.00196, August 31, 2026. https://arxiv.org/abs/2609.00196

[^123]: Qinghua Mao et al., “SafeEvolve: Harness-Policy Co-Evolution from Agent Experience for Safety Alignment,” arXiv:2609.02786, September 2, 2026. https://arxiv.org/abs/2609.02786

[^124]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^125]: Google, “Announcing the Agent2Agent Protocol (A2A),” April 9, 2025. https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/

[^126]: Agent Client Protocol project, “Agent Client Protocol,” GitHub. https://github.com/agentclientprotocol/agent-client-protocol

[^127]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^128]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^129]: Aryan Luthra et al., “Evaluating Agentic Learning Harness Capabilities Without Labels via the Scaling Hypothesis,” arXiv:2608.13608, August 11, 2026. https://arxiv.org/abs/2608.13608

[^130]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^131]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^132]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^133]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^134]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^135]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^136]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^137]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^138]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^139]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^140]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^141]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^142]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^143]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^144]: Anomaly, “opencode,” GitHub. https://github.com/anomalyco/opencode

[^145]: Agentic AI Foundation, “goose,” GitHub. https://github.com/aaif-goose/goose

[^146]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^147]: SWE-agent project, “SWE-agent,” GitHub; project now recommends mini-SWE-agent for most new work. https://github.com/SWE-agent/SWE-agent

[^148]: Prime Intellect, “prime-agent,” GitHub. https://github.com/PrimeIntellect-ai/prime-agent

[^149]: HKUDS, “OpenHarness,” GitHub. https://github.com/HKUDS/OpenHarness

[^150]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^151]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^152]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^153]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^154]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^155]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^156]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^157]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^158]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^159]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^160]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^161]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^162]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^163]: Ashish Vaswani et al., “Attention Is All You Need,” arXiv:1706.03762 (2017). https://arxiv.org/abs/1706.03762

[^164]: Tom B. Brown et al., “Language Models are Few-Shot Learners,” arXiv:2005.14165 (2020). https://arxiv.org/abs/2005.14165

[^165]: Patrick Lewis et al., “Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks,” arXiv:2005.11401 (2020). https://arxiv.org/abs/2005.11401

[^166]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^167]: Luyu Gao et al., “PAL: Program-aided Language Models,” ICML 2023; arXiv:2211.10435. https://arxiv.org/abs/2211.10435

[^168]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^169]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^170]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^171]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^172]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^173]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^174]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^175]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^176]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^177]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^178]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^179]: Jediah Katz, “Dynamic context discovery,” Cursor, January 6, 2026. https://cursor.com/blog/dynamic-context-discovery

[^180]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^181]: Josh Ma, “What we’ve learned building cloud agents,” Cursor, June 2, 2026. https://cursor.com/blog/cloud-agent-lessons

[^182]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^183]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^184]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^185]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^186]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^187]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^188]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^189]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^190]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^191]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^192]: Seth Karten et al., “Prime Agent: A Self-Improving RLM Harness,” arXiv:2608.23552, August 24, 2026. https://arxiv.org/abs/2608.23552

[^193]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^194]: Saransh Dhage, “Harness Engineering for Predictable Agentic Systems: An Empirical Study of Deterministic Execution Constraints,” arXiv:2608.26197, August 25, 2026. https://arxiv.org/abs/2608.26197

[^195]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^196]: Haibo Jin et al., “Harness Engineering in LLM Tool Use via Agent-Native Reusable Tool Primitives,” arXiv:2609.01736, September 1, 2026. https://arxiv.org/abs/2609.01736

[^197]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^198]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^199]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^200]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^201]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^202]: SWE-agent project, “SWE-agent,” GitHub; project now recommends mini-SWE-agent for most new work. https://github.com/SWE-agent/SWE-agent

[^203]: Anomaly, “opencode,” GitHub. https://github.com/anomalyco/opencode

[^204]: Agentic AI Foundation, “goose,” GitHub. https://github.com/aaif-goose/goose

[^205]: Prime Intellect, “prime-agent,” GitHub. https://github.com/PrimeIntellect-ai/prime-agent
