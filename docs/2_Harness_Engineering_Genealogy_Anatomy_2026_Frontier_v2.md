A canonical second-pass research survey of the model-external systems that turn foundation models into reliable, long-horizon agents - with an adversarial audit of the academic literature, production architectures, open-source runtimes, evaluation science, security, memory, and self-improving harnesses.

------------------------------------------------------------------------

| **Central thesis.** Harness engineering is best understood as inference-time systems and control engineering for goal-directed foundation models. Its frontier is shifting from hand-built prompts and loops toward durable, observable, model-conditioned runtimes that can increasingly diagnose and modify their own scaffolding. The strongest evidence also argues against treating “the model” as the unit of capability: in agentic settings, the relevant object is the model-harness-environment configuration. |
|----|

Scope. Canonical v2 retains the substantive base of the first report and adds an independent second-pass audit across academic literature, primary industry engineering sources, protocol specifications, and leading open-source implementations available through 9 September 2026. The audit explicitly searches adjacent literatures that do not use the phrase “harness engineering,” separates peer-reviewed work from fresh preprints and vendor evidence, and gives counterevidence equal weight to positive results. Very recent September 2026 papers are treated as provisional hypotheses unless supported by older or independent evidence.

# Executive summary

- Harness engineering is new terminology for an old systems problem. The ideas descend from classical agent architectures and control, but the modern lineage runs through transformers, in-context learning, retrieval, tool-augmented models, ReAct-style loops, agent-computer interfaces, context engineering, and long-running agent runtimes.[^1][^2][^3]

- A useful rigorous definition is: the empirical design and optimization of the inference-time/runtime layer around one or more foundation models - interfaces, context and state, tool execution, control flow, verification, security, persistence, and evaluation - so that model intelligence produces reliable goal-directed behavior in an external environment.

- The field crystallized in 2026 because model capability crossed a threshold at which the runtime became a visible bottleneck. Hashimoto used “harness engineering” for systematic prevention of recurring agent mistakes; OpenAI described the Codex harness as the agent loop and execution logic; LangChain popularized the broad formulation “Agent = Model + Harness.” The exact coinage is not reliably settled.[^4][^5][^6]

- The strongest empirical shift is methodological: harnesses are becoming independent experimental variables. Harness-Bench records large differences across model-harness pairings, and controlled same-model studies show that context and stall-handling changes alone can materially alter coding outcomes under constrained conditions.[^7][^8]

- The frontier has two complementary poles. Adaptive-harness research optimizes or generates prompts, tools, middleware, memory, workflows, and subagents from execution traces; deterministic-harness research pushes loops, schemas, permissions, retries, and validation into software. A general-purpose architecture should combine deterministic invariants with model autonomy, rather than choose one extreme.[^9][^10][^11][^12][^13][^14]

- Long-horizon reliability is increasingly a distributed-systems problem: durable event logs, restartability, isolated workspaces, idempotent execution, resource accounting, event-driven wakeups, and fault containment are becoming first-class harness primitives.[^15][^16][^17][^18]

- Multi-agent systems appear most defensible when they buy fresh context, parallelism, specialization, or independent verification. “More agents” is not itself an architecture. Recursive Agent Harnesses and current production systems make the full harness - tools, filesystem, planning, execution - the delegable unit.[^19][^20][^21]

- The most promising novel contributions are not another role-playing orchestrator. They are portable harness representations, safe self-improvement with causal attribution and rollback, durable event-driven runtimes, execution-alignment verification, capability-based security, value-of-compute scheduling for subagents, and rigorous model x harness x environment benchmark science.

- The second-pass genealogy adds a missing optimization lineage: OPRO, Promptbreeder, DSPy, GPTSwarm, TextGrad, ADAS, AgentSquare, AFlow, Darwin Gödel Machine, GEPA, and related work progressively turned prompts, LM programs, workflow graphs, and whole agent implementations into editable search spaces. Modern harness evolution is best understood as the convergence of that lineage with production agent runtimes. [^22][^23][^24][^25][^26][^27][^28]

- The strongest v2 synthesis is that a harness is a partially programmable inference-time policy stack, not merely a wrapper. It mediates which observations become model context, which actions are admissible, how state persists, how claims are checked against the world, and which parts of the system may modify themselves. Its realized benefit therefore depends not only on harness quality but on model-harness compatibility and compliance. [^29][^30][^31]

- Security evidence now justifies a stronger architectural claim: prompts should not be treated as an authorization mechanism. A general-purpose harness should behave like a reference monitor or small security kernel, with trusted control/data provenance, least-privilege capabilities, sandbox boundaries, deterministic policy enforcement, and explicit declassification/approval points. [^32][^33][^34][^35]

- Persistent memory should be engineered more like a versioned database/cache and truth-maintenance system than a vector store. Long-lived agents need provenance, freshness, conflict, invalidation, revocation, retention, and merge semantics, and the system must separately measure whether a memory is valid and whether a particular model actually complies with it. [^36][^37]

- Claims about autonomous harness self-improvement require stricter controls than v1 emphasized: compare against matched test-time search/compute, use held-out tasks and models, isolate proposal from validation, measure forgetting, and distinguish candidate-generation skill from the beneficiary model’s ability to use the resulting artifact. Several 2026 papers show large gains; others show unstable transfer, localized value, or no consistent advantage over simpler search. [^38][^39][^40][^41][^42]

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

Second-pass evidence ledger and repository atlas are integrated into Sections 6-8 and the Sources inventory.

## Evidence standard for canonical v2

The report uses an evidence hierarchy rather than treating every citation as interchangeable. Tier A covers peer-reviewed or accepted research and mature benchmarks with inspectable methodology; Tier B covers primary production engineering reports and mature open-source implementations; Tier C covers reproducible preprints with code or unusually clear methods; Tier D covers very fresh, unreplicated preprints and vendor-specific claims. A Tier D result may be important enough to discuss at length, but it is not allowed to silently carry a Tier A conclusion.

The second pass also uses an omission test: for every section, ask which neighboring field would reasonably object that its foundational work has been omitted. This adds programming-languages and automated program optimization to the historical account, operating-systems and distributed-systems semantics to long-horizon runtime design, information-flow control and capability security to agent safety, database/cache invalidation to memory, and test-time-compute baselines to self-evolution evaluation. The result is a broader but more disciplined definition of the field.

# 1. What harness engineering is

At the broadest operational level, an agent is not merely an LLM with a system prompt. It is a closed-loop system in which a model observes a state, decides what to do, acts through tools or an environment, receives consequences, updates its working state, and eventually terminates. The harness is the machinery that creates and governs that loop. OpenAI explicitly describes the Codex harness as the “core agent loop and execution logic”; LangChain’s 2026 formulation expands the term to every code/configuration/execution component outside the model.[^43][^44]

A narrow historical sense is also important. Hashimoto’s February 2026 usage was failure-driven: whenever an agent makes a repeatable mistake, encode a prompt rule or tool that makes the failure unlikely to recur. That conception resembles software reliability engineering applied to stochastic workers. The later broad usage encompasses the entire runtime, including components that are not merely patches for model weakness.[^45]

## A working research definition

| **Definition.** Harness engineering is the empirical design, implementation, measurement, and evolution of the model-external inference/runtime system that mediates perception, context, action, state, control, verification, security, and coordination for one or more foundation models operating toward goals in an external environment. |
|----|

This definition deliberately makes the research object larger than prompt engineering and smaller than “everything in an AI product.” A database may be infrastructure; it becomes part of the harness when its schema, retrieval policy, persistence semantics, or tool interface directly shape the agent’s behavior. A user interface may be product surface; it becomes harness-relevant when it gates actions, supplies observations, or changes the feedback loop.

## A formal view

Let M denote a frozen model, E an environment, D a distribution of tasks, and Hθ a harness parameterized by θ. The harness parameterization can include prompts, context-selection policies, tool schemas, middleware, state machines, memory/retrieval policies, subagent topology, validators, permission rules, retry policies, and stopping conditions. For a resource budget B, harness engineering can be viewed as selecting θ to maximize expected utility:

**maxθ Eτ~D \[ U(success, quality) - λc C(τ) - λl L(τ) - λr R(τ) \] subject to budget, safety, and permission constraints**

The important consequence is that capability is conditional. A leaderboard score for “model M” is actually a score for some model-harness-environment-task configuration. Harness-Bench makes this explicit, and the source-code literature argues for treating harness implementation as a first-class object rather than incidental benchmark plumbing.[^46][^47]

## The harness as a partially programmable policy stack

A useful refinement is to treat the model as a stochastic policy generator embedded inside a larger executable policy. Let π_M be the behavioral policy induced by a frozen model and H be the runtime transformation that assembles observations, constrains and interprets actions, persists state, dispatches tools, invokes validators, and decides continuation. Then the deployed policy is not π_M alone but approximately π_system = H\_{θ,E,B}\[π_M\]. This makes harness changes closer to policy interventions than to cosmetic prompt edits: changing a context selector, tool ABI, retry rule, permission boundary, or verifier can alter the action distribution without changing a single model weight.

This formulation also clarifies two distinct notions that recent evidence often conflates: harness validity and harness compliance. A harness artifact can encode a semantically correct policy or memory while the beneficiary model fails to notice, activate, or follow it. Invalidation-contract experiments make this distinction explicit for memory, while 2026 harness-benefit and co-evolution studies show the same phenomenon more broadly: better artifacts do not monotonically produce better behavior across model capability tiers. [^48][^49][^50]

Accordingly, the proper optimization target is not “the best harness” in isolation. It is a conditional, resource-bounded compatibility surface over model family/version, task distribution, environment, and budget. An edit that helps one model may be neutral or harmful for another; an edit that raises mean success may worsen tail risk or cost; an edit that helps on the tuning distribution may disappear when compared with an equally expensive test-time search baseline. This is why serious harness science requires interaction effects, not single-axis leaderboards. [^51][^52][^53]

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

The genealogy is not a straight line. At least five strands converged: (1) classical agents, planners, control, operating systems, programming languages, and software harnesses supplied the closed-loop and runtime semantics; (2) foundation models became sufficiently general to serve as adaptive decision modules; (3) retrieval, tools, executable environments, and persistent memory made model outputs consequential; (4) LM-program and agent-design research turned prompts, graphs, workflows, and code into optimizable artifacts; and (5) production systems learned to make those interactions durable, observable, secure, and increasingly self-modifying.

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

## Pre-2017: agents, control, operating systems, and software harnesses

The deepest ancestors of harness engineering are older than language models. Classical agent architectures decomposed behavior into perception, internal state, deliberation/planning, action, and feedback; BDI systems separated beliefs, desires, and intentions; reinforcement-learning and control theory formalized policies interacting with environments; operating systems separated applications from scheduling, memory, files, devices, protection, and process isolation. Modern harnesses recombine these abstractions around a radically more general but stochastic component.

The word “harness” also has a mundane software lineage: a test harness owns setup, execution, observation, and verdicts around a system under test. That analogy is more than etymological. A good agent harness similarly creates a controlled environment around an unpredictable worker, makes actions observable, catches failures, and enforces invariants the worker itself should not be trusted to maintain. What changes with LLM agents is that the component under control can interpret natural-language interfaces, synthesize programs, and reason about its own runtime.

Programming-languages research contributes another underappreciated ancestor: interpreters, type systems, effect systems, capability machines, and intermediate representations all distinguish \*what a program means\* from \*how a runtime safely executes it\*. This perspective becomes directly relevant once agents generate or edit executable plans, tools, workflow graphs, and harness code. The emerging Harness IR question is therefore partly a PL question: which behavioral semantics should be represented, type-checked, compiled, diffed, and verified outside the model?

## 2017-2020: the model becomes programmable at inference time

The Transformer paper is an architectural ancestor, not a harness paper: it supplied the scalable attention architecture behind modern language models. GPT-3 then made a systems shift possible by demonstrating meaningful task adaptation through textual instructions and demonstrations without gradient updates. That transformed natural-language context into a programmable control surface.[^54][^55]

RAG added a second critical idea: model behavior could be coupled to a mutable external knowledge store rather than relying solely on parameters. Modern harness memory systems are much richer, but the conceptual split between parametric intelligence and runtime-retrieved state is foundational.[^56]

## 2021-2023: action, tools, feedback, and persistent procedural state

WebGPT is an unusually direct ancestor of contemporary browsing agents: a model operated a text browser, searched and navigated the web, and gathered references for an answer. MRKL generalized the “systems” intuition by pairing LMs with external knowledge and discrete reasoning modules.[^57][^58]

ReAct supplied the canonical agent loop: interleave reasoning with actions and observations from the environment. PAL showed that a model need not own every computation: it can generate a program while an interpreter executes deterministic operations. These are still two poles of modern harness design - model-directed control and program-directed control.[^59][^60]

ChatGPT’s November 2022 public launch made conversational instruction-following a mass interface. In 2023, Toolformer formalized learned API use; Reflexion stored linguistic feedback in episodic memory; Voyager combined an automatic curriculum, an executable skill library, environment feedback, errors, and self-verification; AutoGen made multi-agent conversations a reusable programming abstraction.[^61][^62][^63][^64][^65]

## 2023-2025: prompts, LM programs, workflows, and agents become optimizable artifacts

A second genealogy runs in parallel to the familiar ReAct-to-agent-runtime story. OPRO and Promptbreeder showed in 2023 that natural-language instructions themselves could be searched or evolved by language models rather than written once by humans. DSPy then treated LM applications as parameterized computational graphs that could be compiled against a metric, including programs controlling agent loops. [^66][^67][^68]

In 2024 the optimization target widened. GPTSwarm represented language agents and multi-agent collaborations as optimizable graphs, adjusting both node prompts and graph connectivity. TextGrad propagated textual feedback through compound AI computation graphs. EvoAgent applied evolutionary operators to expand a single expert agent into a multi-agent system. These works matter historically because they make “system structure” - not just model weights - a learnable/searchable object. [^69][^70][^71]

ADAS / Meta Agent Search made the connection even more direct by representing whole agent designs as code in a Turing-complete search space and having a meta-agent invent, execute, score, and archive new agents. AgentSquare searched a modular space spanning planning, reasoning, tool use, and memory; AFlow searched code-represented workflow graphs with Monte Carlo tree search. By 2025, the Darwin Gödel Machine was recursively modifying a coding agent’s own code while empirically validating descendants, and GEPA was using trajectory-level natural-language reflection to evolve prompts efficiently. [^72][^73][^74][^75][^76]

This lineage is the missing bridge between “prompt optimization” and 2026 “harness evolution.” The conceptual novelty of the 2026 papers is not that an LLM can propose a better instruction; it is that the editable object has expanded into a production-like runtime containing prompts, context policy, tools, middleware, memory, control flow, permissions, validators, and sometimes the harness source code itself. Meta-Harness, AHE, RHO, Harness-R1, and JIT-Agent should therefore be read as descendants of both agent-runtime engineering and automated LM-program design. [^77][^78][^79][^80][^81]

## 2024-2025: the interface and runtime become first-class

SWE-agent’s key contribution was not merely a coding benchmark result. It named the Agent-Computer Interface (ACI) as an object to design for language-model “users,” showing that navigation/editing/testing interfaces materially affect agent behavior. This is one of the clearest academic precursors to harness engineering.[^82]

Anthropic’s late-2024 production guidance argued for simple, composable patterns and distinguished predetermined workflows from agents that dynamically control their own tool use. MCP then standardized a major boundary: connecting models/agents to external tools and data sources.[^83][^84]

In 2025, context engineering became an explicit discipline; Agent Skills packaged procedural knowledge into discoverable file trees; advanced tool use added deferred loading and dynamic discovery for large tool catalogs; long-running-agent work used durable files, progress records, and version control to bridge context windows. Meanwhile AFlow and related “workflow optimization” research were already automating parts of what would soon be called harness evolution.[^85][^86][^87][^88][^89]

A distinct “agent operating system” literature also anticipated the runtime turn. AIOS separated scheduling, context, memory, storage, tool resources, and access control into a kernel beneath agent applications, explicitly arguing that resource management should not be reimplemented inside every agent. Its terminology differs from modern harness engineering, but the architectural impulse - externalize shared runtime semantics into a trusted substrate - is strikingly convergent. [^90]

## 2026: a named discipline and a new experimental unit

The terminology crystallized quickly but messily. Hashimoto used “harness engineering” on February 5 and explicitly noted that he did not know whether an accepted term already existed. OpenAI used “harness” for Codex’s core loop in January and published a harness-engineering case study in February. LangChain’s March anatomy article gave the phrase a broad, memorable definition: the harness is the model-external machinery that supplies state, tools, infrastructure, orchestration, and middleware. Claims that any single person definitively coined the term should therefore be treated cautiously.[^91][^92][^93][^94]

By mid-2026, the academic frontier had changed from designing a particular agent to measuring and optimizing harnesses themselves. Harness-Bench isolates configuration effects; Agentic Harness Engineering makes harness components editable and observable; RHO and MemoHarness learn from trajectories; Evo-Bench and HarnessDev evaluate whether models can improve or create their own harnesses; JIT-Agent proposes a model whose output is a task-adaptive harness.[^95][^96][^97][^98][^99][^100][^101]

# 3. Why the discipline crystallized now

1.  Models became capable enough for the wrapper to matter. Weak models fail regardless of tooling. Frontier models can plan, recover, write code, and use computers well enough that context policy, tool ergonomics, execution feedback, and control flow can move the limiting factor outside the weights.[^102][^103]

2.  Agent horizons expanded from seconds to hours or days. Long sessions amplify state drift, context saturation, partial completion, environment failures, retries, and recovery. This turns “prompting” into runtime engineering.[^104][^105]

3.  Coding supplied unusually good feedback. Tests, compilers, linters, diffs, CI, and version control make correctness partially machine-checkable, which is why much of the most mature harness research is concentrated in software engineering.[^106][^107]

4.  Benchmark scores exposed harness variance. Once the same base model appears inside multiple products and open-source agents, differences in tools, context, and runtime are observable rather than theoretical.[^108][^109]

5.  Integration edges became standardized. MCP, A2A, and ACP increasingly separate the internal harness from tool/data servers, remote agents, and editor clients. That modularity makes the harness itself more portable and experimentally replaceable.[^110][^111][^112]

6.  Human attention became the bottleneck. OpenAI’s Symphony and Cursor’s always-on cloud agents move orchestration upward from individual turns to work items, events, goals, and long-lived tasks.[^113][^114]

## A market-level sign: harnesses become infrastructure products

By mid-2026, “harness” had crossed from research vocabulary into cloud-platform product taxonomy. AWS shipped a managed AgentCore harness; Microsoft released a batteries-included Agent Framework Harness; Cloudflare’s documentation explicitly separates the durable Agents SDK runtime (“where the agent lives and stays durable”) from the harness (“what it does on each turn”); and Databricks began offering managed Omnigent, a meta-harness that can host and swap several coding harnesses behind a common layer. [^115][^116][^117][^118]

This commercialization matters conceptually. It suggests an emerging three-layer stack: foundation model, harness/runtime behavior, and durable hosting/control plane. The layers can be fused in a single product, but vendors increasingly expose them separately because enterprises want model portability, runtime policy, persistent identity/state, sandboxing, observability, and governance independent of any one model provider. The field is thus becoming an infrastructure category at the same moment academics are making it an experimental variable.

# 4. Anatomy of a modern harness

A useful architecture is seven interacting planes. This is a synthesis rather than a claim that the field has standardized on exactly seven layers; it aligns closely with the components identified in current production write-ups and the 2026 source-code study.[^119][^120]

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

Every harness allocates cognition among model weights, token context, deterministic code, external tools, durable memory, subagents, and humans. ReAct-like systems let the model choose the loop dynamically; Agentic Programming argues that loops, branching, and sequencing should remain deterministic program structure and that the LLM should be invoked only at explicitly adaptive nodes. In practice, the best boundary is task-dependent.[^121][^122]

| **Design principle.** Put hard invariants in deterministic software: permissions, budgets, idempotency, transaction boundaries, schema validation, and objective stopping constraints. Put open-ended search, decomposition, interpretation, and recovery tactics in the model. Move the boundary only when evaluation demonstrates a benefit. |
|----|

## Three cross-cutting properties the seven-plane view can hide

First, **resource economics is cross-cutting**. Context tokens, model calls, wall-clock time, browser/VM minutes, network calls, subagent fan-out, and human approvals are resources consumed by every plane. A harness is therefore also a scheduler allocating scarce inference-time compute. “Better” orchestration must be evaluated on a capability-cost Pareto frontier, not only task success.

Second, **provenance is cross-cutting**. Every observation, memory, tool result, artifact, instruction, and delegated capability should carry enough origin and version information for the system to decide how much authority and trust it deserves. Security research on context privilege escalation and memory research on invalidation/revocation both reveal failures caused by flattening heterogeneous sources into an undifferentiated token stream. [^123][^124][^125]

Third, **model conditioning is cross-cutting**. The same context policy, edit tool, error format, reflection routine, or memory artifact can have different effects across models. Cursor’s production practice of model-specific harness profiles, controlled harness studies, and new compatibility results all point toward a design principle: portability should mean a stable semantic interface plus model-conditioned compilation, not pretending different models are behaviorally interchangeable. [^126][^127][^128]

# 5. The 2026 frontier

The most important recent work is not a single architecture. It is a set of pressure fronts where longer-horizon autonomy exposes systems problems that one-shot prompting never had to solve.

## 5.1 Context as an active memory hierarchy

The old pattern was “put everything useful in the prompt.” The frontier is selective, just-in-time context assembly. Anthropic frames context as a finite resource to curate on every inference step; Cursor increasingly turns long tool outputs, chat history, terminals, and tool catalogs into discoverable file-like resources rather than permanent prompt payload. Compaction, retrieval, offloading, and skills become policies rather than static text.[^129][^130][^131]

- Working context: the small, high-signal state needed for the next decision.

- Artifact memory: files, logs, intermediate outputs, plans, patches, screenshots - externally addressable rather than repeatedly tokenized.

- Episodic memory: prior trajectories, failures, diagnoses, and feedback that can be retrieved when structurally similar tasks recur.[^132][^133]

- Procedural memory: reusable skills, scripts, policies, and conventions loaded progressively.[^134]

- Durable session state: event/log structures that survive context resets, process restarts, and handoffs.[^135][^136]

A crucial lesson from Cursor and Anthropic is that context mechanisms are model-conditioned. Guardrails that helped earlier models can become dead weight or even failure sources as models improve. Cursor reports deleting older static-context and tool-call guardrails; Anthropic reports context resets that fixed one model’s “context anxiety” becoming stale assumptions for a stronger model.[^137][^138]

A newer systems result makes the memory problem even more concrete: cross-episode memory behaves like a cache, so correctness requires explicit invalidation semantics. Invalidation Contracts attaches version stamps and cacheability hints to cached recovery suggestions so stale entries can be evicted deterministically rather than rediscovered by trial and error. Across roughly 9,400 episodes spanning seven models, three serving paths, and two domains, the paper reports zero contract failures for version-stamp validity; planner compliance with identical cached advice still varied sharply by model. The broader harness lesson is that durable memory needs validity, provenance, and expiry semantics—not merely retrieval similarity.[^139]

### From retrieval memory to lifecycle memory

The second-pass literature changes the recommended abstraction for persistent memory. Similarity search answers “what looks relevant?” but a long-lived agent also needs to answer: Is this record still true? Which version supersedes it? Was it revoked? Does it conflict with a newer policy? Is it tainted by an untrusted source? Who may read it? When should it expire? These are database, cache-coherence, truth-maintenance, and security questions rather than embedding questions.

Invalidation Contracts makes one narrow part of this precise by attaching version stamps and cacheability hints to cross-episode recovery memories and decomposing realized benefit into **validity** (the cached advice is still correct) and **compliance** (the current planner uses it correctly on the first attempt). Its most important general lesson is conceptual: memory correctness can be deterministic at the protocol layer while memory usefulness remains model-conditioned. [^140]

The September “Revoked but Still Authoritative” study identifies a complementary failure: several memory systems retained and surfaced revoked facts such that agents continued to act on them even when replacements existed. The exact results are extremely fresh, but the architectural implication is robust: revocation must be enforced by the retrieval/control layer, not merely represented as text and entrusted to the model. [^141]

A practical general-purpose memory substrate should therefore separate immutable event history from mutable derived memories; attach source, timestamp, version, confidence, scope, and revocation state; support conflict sets and supersession edges; and make retrieval policy capable of filtering by validity and authority before relevance ranking. This is a better foundation for episodic reflection, skills, and learned procedures than a single vector database.

## 5.2 Tool interfaces are a learned ABI

Tool engineering has moved beyond function schemas. Effective tools need names, descriptions, return shapes, error behavior, and token economics that match the model’s learned interaction patterns. Anthropic explicitly treats tools as contracts between deterministic software and a nondeterministic agent; Cursor provisions different edit interfaces for different model families because familiar tool shapes reduce reasoning overhead and errors.[^142][^143]

The scaling problem is tool cardinality. Anthropic’s deferred loading and tool search target hundreds or thousands of tools without stuffing every schema into context. MCP standardizes discovery and invocation at the tool/data boundary. The September 2026 HEART preprint pushes farther, replacing rigid schema-facing interaction with natural-language “tool primitives” and dynamic retrieval over a catalog of 25,519 functions; its headline performance and cost claims are interesting but remain very fresh and should be treated as provisional.[^144][^145][^146]

### Toward a tool-interface compiler

Calling tool schemas a learned ABI has a further implication: the portable object should be the **semantic capability**, not one provider-specific JSON schema. A future harness can maintain a canonical description of effects, preconditions, cost, risk, permissions, observability, and result types, then compile that capability into the tool shape a particular model is most likely to use correctly. Model-specific edit tools in Cursor and large-catalog tool retrieval are early versions of this idea. [^147]

This also creates an optimization target richer than tool selection. The harness can choose how much of the catalog to reveal, whether to expose a high-level composite action or low-level primitives, how to compress outputs, how errors are typed, whether retries are automatic, and whether the model sees raw observations or normalized artifacts. Interface synthesis should be evaluated for semantic equivalence and safety, not just lower token count.

## 5.3 Verification becomes part of the runtime, not post-hoc QA

The central reliability primitive is grounded feedback from the environment. Tests, type checkers, linters, rendered UIs, browser interactions, logs, metrics, and explicit validators convert hidden model mistakes into observations that the loop can act on. OpenAI’s agent-first repository work exposes application UI and observability directly to Codex; Anthropic’s long-running application harness uses planner/generator/evaluator patterns and live browser feedback.[^148][^149]

Harness-Bench names a particularly useful failure class: execution-alignment failures, where plausible reasoning becomes decoupled from tool feedback, workspace state, evidence, or output contracts. That suggests a research target deeper than “reasoning quality”: continuously reconcile the agent’s claims with externally verifiable state.[^150]

### Execution alignment as belief-state reconciliation

The most useful abstraction for verification is not “a critic after the answer.” It is continual **belief-state reconciliation**. The model emits claims about what it has read, changed, tested, or achieved; the harness owns authoritative handles to files, processes, test results, browser state, APIs, and task contracts; verification checks whether the narrative and external state still agree. Harness-Bench’s execution-alignment failures are one empirical manifestation of this broader systems problem. [^151]

This suggests a verification fabric with both local and global checks. Local checks run immediately after side effects (schema validation, exit codes, diff sanity, permissions, postconditions); global checks evaluate task-level invariants and evidence of completion. Independent model critics can add coverage, but they should consume the same authoritative external evidence rather than simply critique another model’s prose.

## 5.4 Deterministic envelopes around stochastic cognition

Two 2026 papers sharpen the deterministic side of the design space. Agentic Programming puts all looping, branching, and sequencing in program code and treats LLM calls as adaptive functions inside the execution graph. Dhage’s empirical study wraps agents with finite-state control, forced tool selection, output validation, bounded retry, and structured planning. Its first-pass results were mixed - constraints could help or hurt - but schema-valid structured planning substantially improved reproducibility in the reported synthetic experiments. The lesson is not “more constraints”; it is “measure which invariants belong outside the model.”[^152][^153]

## 5.5 Long-horizon agents become distributed systems

A long-running harness has to reason about crashes, stale state, retries, duplicated side effects, resource leases, isolation, and resumability. Anthropic uses progress artifacts and git across context windows. Managed Agents separates the harness (“brain”), action environments (“hands”), and append-only session state. Cursor’s cloud-agent architecture emphasizes dedicated VMs, durable execution, self-healing environments, event-driven wakeups, and isolated subagent machines. Symphony maps work items to isolated workspaces and continuously reconciles agents against an issue-tracker control plane.[^154][^155][^156][^157][^158]

Logos makes the distributed-systems analogy explicit: plugins are separate processes, shared state is an append-only transcript, and fault isolation/recovery are tested by killing processes at tool-cycle boundaries. The paper is a very recent draft, but it highlights a likely direction for general-purpose harnesses: event sourcing plus failure-domain isolation rather than a single monolithic agent process.[^159]

Shepherd pushes this runtime view into a programmable meta-agent substrate. It records agent-environment interactions as typed, Git-like execution traces that can be forked and replayed, allowing supervisors and optimizers to branch from prior states instead of restarting entire trajectories. Its reported applications include live intervention, counterfactual search, and tree-structured RL; the deeper architectural contribution is reversible execution as a first-class harness primitive.[^160]

Harness-of-Harness goes one level higher: rather than replace a coding harness, it wraps existing harness-model pairs in repeated planning–coding–testing loops with versioned project history and independently evaluated increments. The September 2026 preprint reports a 52.25% average relative gain over standalone harnesses across three benchmarked harness-model pairs after iterative improvement, plus a multi-day run exceeding 70 iterations. The result is very fresh and domain-specific, but it is a useful prototype of hierarchical orchestration where a harness itself becomes the worker controlled by a longer-lived meta-runtime.[^161]

### Reversible and speculative execution

Once work spans hours or days, replay and branching become more than debugging conveniences. A typed event history plus versioned artifacts can support speculative branches, counterfactual re-execution, forked subagents, and rollback of reversible effects. The hard boundary is irreversible external action: sending messages, changing production state, moving money, or publishing content requires explicit transaction semantics rather than assuming a filesystem-style undo exists.

This motivates separating \*intent\*, \*prepared action\*, \*commit\*, and \*observed result\* in the event model. Idempotency keys and effect identifiers reduce duplicate side effects after retries; leases bound stale workers; checkpoints and deterministic environment snapshots improve replay; compensating actions handle operations that cannot be rolled back. These are standard distributed-systems concepts, but agent runtimes make them behavioral primitives because the decision-maker itself is nondeterministic.

## 5.6 Security moves below the model

As agents gain shell, browser, credentials, and network access, security cannot depend primarily on the model refusing dangerous actions. Anthropic’s 2026 containment write-up argues for hard environment-level bounds using sandboxes, VMs, filesystem boundaries, credential isolation, and egress controls, with probabilistic classifiers as defense-in-depth. Its telemetry is also a warning about human approval loops: users approved roughly 93% of permission prompts, and the reported auto-mode classifier traded very low benign-command blocking for a non-zero miss rate on risky actions. Deterministic containment is the backstop.[^162]

This reframes a general-purpose harness as a security kernel. Every tool and subagent should receive explicit capabilities, not ambient authority; credentials should stay outside untrusted execution environments where possible; observations from the web and tool outputs should carry provenance/taint metadata; and delegation should never silently widen permissions.

### Security as a reference monitor, not a better system prompt

The second pass substantially strengthens the security conclusion. AgentDojo and ToolEmu established that tool-using agents need evaluations that include adversarial or high-stakes environments. CaMeL then demonstrated a more architectural defense: separate trusted control flow from untrusted data flow and enforce capabilities outside the model. Fides formalizes related information-flow-control ideas with confidentiality/integrity labels and deterministic policy enforcement. [^163][^164][^165][^166]

The newest harness-specific security work shows why this boundary matters. Context-privilege-escalation studies examine real harnesses that assemble instructions from many files, tools, roles, persistent scopes, and extension surfaces; low-privilege attacker content can acquire higher model-facing authority simply because the harness re-serializes it in a privileged context location. The vulnerability is therefore partly an **assembly bug in the harness**, not solely a failure of model instruction hierarchy. [^167][^168]

CapScope pushes the argument to authorization: authority should be represented as an out-of-band, typed capability derived from the trusted task, not as a string inside the model context. Its September 8 evidence is too fresh to treat as settled, but the design aligns with decades of capability security and with CaMeL/Fides: the model may propose an action, while a small deterministic monitor decides whether the action is within delegated authority. [^169][^170][^171]

A general-purpose harness should therefore minimize its trusted computing base. The reference monitor needs to know provenance, capability scope, current policy, target effect, and declassification/approval points; it does **not** need to understand the whole task semantically. Tool plugins, hooks, MCP servers, skills, and fetched instructions should be treated like supply-chain dependencies with version/pinning and provenance, not automatically elevated because they appear in a configuration directory.

This creates a clean security principle: **the model can reason about permission, but it cannot grant itself permission**. Prompts can explain policy; only the runtime can confer authority. Subagent delegation should narrow or preserve capabilities by default, never widen them. Credentials should be mediated rather than injected into the model-visible environment whenever possible, and high-risk effects should be auditable even if the model or extension layer is compromised.

## 5.7 Multi-agent: context isolation and parallel compute, not theater

Anthropic’s research system uses an orchestrator-worker pattern in which a lead agent decomposes a query and parallel subagents search independently. Cursor now gives subagents isolated VMs and clean contexts. These designs expose the strongest reasons to use multiple agents: parallelism, fresh context windows, specialization, independent checks, and failure isolation.[^172][^173]

Recursive Agent Harnesses generalizes the unit of delegation from a bare model call to a full harness with filesystem, code execution, planning, and its own ability to spawn children. In its controlled Oolong-Synthetic evaluation, holding GPT-5 fixed, the authors report an increase from a 71.75% Codex-style baseline to 81.36%. Prime Agent operationalizes related ideas with a persistent IPython runtime, continual harness state, recursive subagents, direct agent-to-agent communication, and daemon-backed sessions. Both are promising; both remain fresh 2026 research that needs broader replication.[^174][^175]

### The right abstraction is closer to distributed processes than personas

The multi-agent literature becomes more coherent if agents are treated as processes with isolated state, compute budgets, capabilities, and failure domains rather than as characters with job titles. Recursive Agent Harnesses makes this explicit by delegating a complete runtime; production systems increasingly isolate subagents in separate environments or contexts. The design questions then become familiar: what state is shared, who owns writes, how results are merged, how failures are contained, and when coordination cost exceeds the value of parallelism. [^176]

This framing also clarifies when a “multi-agent” design is not really necessary. If two roles share the same context, tools, model, and sequential control flow, they may be more cleanly represented as phases of one policy. Separate agents earn their complexity when isolation or concurrency is itself an architectural requirement.

## 5.8 Harness self-improvement is the most distinctive new research frontier

Earlier work optimized prompts or workflow graphs. The 2026 shift is to treat the harness itself as an editable compound artifact and to learn from execution trajectories. Several families are emerging:

- Observability-driven evolution. Agentic Harness Engineering exposes editable components, distills trajectory evidence, attaches predictions to edits, and verifies the effect in the next round. Its reported gains transfer across some model families, and its ablations attribute more gain to tools, middleware, and long-term memory than to the system prompt alone.[^177]

- Retrospective self-supervision. RHO selects difficult prior trajectories, re-solves them, generates harness updates, and chooses among candidates using self-validation and pairwise self-preference; the paper reports a one-round SWE-Bench Pro pass-rate increase from 59% to 78% without external grading and is accepted to Findings of EMNLP 2026.[^178]

- Experience-conditioned adaptation. MemoHarness decomposes a harness into six editable control dimensions, stores local diagnoses plus global patterns, and retrieves relevant experience to adapt the harness for each test case without test-time labels or search. Its authors explicitly leave broader statistical robustness and component attribution open.[^179]

- Outer-loop executable-code search. Meta-Harness gives an agentic proposer filesystem access to prior harness source, scores, and traces and searches over harness code end to end. It reports improvements across online classification, retrieval-augmented math, and TerminalBench-2, including a discovered math harness that transfers across five held-out models. This matters because the optimization object is executable runtime code, not only natural-language prompts.[^180]

- Learned failure-conditioned harness editing. Harness-R1 post-trains a dedicated harness engineer with online reinforcement learning: batches of target-agent failures are converted into validated executable runtime patches, then fresh reruns of the frozen target agent supply outcome rewards. Its reported gains across WebShop, ALFWorld, and DBBench suggest that editing the runtime from trajectories can itself be learned as a reusable capability.[^181]

- Evaluation-efficient evolution. Task-CoEvolve treats validation selection as part of harness optimization: it concentrates evaluation on tasks where candidate harnesses disagree and corrects for the resulting sampling distribution. In its reported text-classification and Terminal-Bench experiments, it matches full-set search while cutting harness-optimization evaluations by about 80%. If harness evolution becomes routine, value-of-information scheduling will matter almost as much as the edit operator itself.[^182]

- Harness generation and evolution as a model capability. Evo-Bench measures intrinsic harness-evolving ability; HarnessDev asks models to create runnable harness infrastructure and then evolve it; JIT-Agent trains a dedicated “harness intelligence” model to synthesize task-adaptive harnesses on demand.[^183][^184][^185]

The counterweight is important. HarnessDev finds that generated harnesses still trail mature human references in code and search/research, evolution gains can be unstable, and benefits are strongly dependent on the model executing the harness. A separate controlled evaluation on Terminal-Bench 2.1 compares automatic harness evolution with simple test-time search/discovery under matched feedback and inference budgets; with GPT-5.4 and Claude Opus 4.6, it finds that evolution does not consistently win and generalizes only weakly to held-out tasks. The frontier is real, but benchmark reuse, search-budget confounds, and transfer failure make many headline gains less conclusive than they first appear.[^186]

### A taxonomy of “self-improving harnesses”

The phrase “self-improving harness” currently collapses several mechanisms that should be evaluated separately. **Offline harness search** uses labeled or scored development tasks to discover a reusable configuration. **Continual compilation** turns repeated experience into memories, skills, procedures, or rules. **Test-time adaptation** modifies the runtime for the current task from unlabeled execution evidence. **Learned harness editing** post-trains a dedicated model to propose executable harness patches. **Model-harness co-evolution** alternates changes to runtime and weights. **Meta-harnessing** puts one harness around another so that the outer system plans, supervises, validates, or replaces the inner one.

The methods differ in what constitutes learning. Meta-Harness searches harness source; AHE uses observability and editable components; RHO retrospectively extracts improvements from difficult trajectories; MemoHarness retrieves prior evolution experience; Harness-R1 trains a dedicated harness engineer with online reinforcement learning; JIT-Agent trains a system to synthesize task-conditioned harnesses. These are not interchangeable claims about the same algorithmic object. [^187][^188][^189][^190][^191][^192]

Several newer methods focus less on proposing edits and more on **credit and validation**, which may be the harder problem. Gated semantic quality-diversity assigns improvements to pathology categories and leaves sampling/significance testing to deterministic code. Task-CoEvolve spends evaluation budget on tasks that discriminate among candidates. HarnessLens selectively verifies behavior-relevant evidence. HarnessEvolve aligns failures with reference trajectories and gates changes against shortcut learning and forgetting. [^193][^194][^195]

The negative and conditional evidence is equally important. Rethinking Harness Evolution shows that, under matched feedback and inference budgets, automatic evolution does not consistently beat simpler test-time scaling and often transfers poorly. HarnessDev finds that generated harnesses remain behind mature human references in important domains and that evolution can be unstable. EVOHARNESSBENCH makes retention/adaptation tension explicit, while “Where Does Harness-Optimization Value Live?” reports that gains can be localized to specific control dimensions rather than distributed across the whole harness. [^196][^197][^198][^199]

The resulting scientific standard should be demanding: an evolution paper should report (1) matched total inference/evaluation compute; (2) held-out tasks and, ideally, held-out model families; (3) the persistent post-search artifact separated from temporary search-time computation; (4) component-level attribution; (5) forgetting/regression; (6) security-policy invariance; and (7) whether the beneficiary model actually complies with the learned artifact. PRISM’s resource-bounded framing and repeatability/worst-condition metrics move in this direction. [^200]

This turns the research question from “can an LLM rewrite a prompt and score higher?” into a harder one: **can the system infer a transferable causal defect from trajectories, produce the smallest corrective runtime intervention, demonstrate that the intervention survives held-out conditions, and safely retire it when the defect disappears?** That is much closer to adaptive systems engineering than prompt optimization.

## 5.9 Model-harness co-design and “assumption debt”

Harnesses encode assumptions about model weaknesses, training interfaces, and environment behavior. Cursor’s model-specific tool formats and prompts show intentional co-design. Anthropic’s Managed Agents story shows the inverse problem: once the model changes, old workarounds can constrain the stronger model. A useful concept is harness assumption debt: every special rule is a hypothesis about a failure mode that should have evidence, ownership, an expiry condition, and a regression test.[^201][^202]

| **Frontier implication.** A mature harness should be able to remove scaffolding, not only add it. Continuous evolution without deletion creates a fossil record of old model deficiencies inside the runtime. |
|----|

The newest papers go beyond tailoring a fixed harness to a fixed model and instead optimize the pair. Co-Harness alternates a harness critic that proposes validated local runtime updates with model fine-tuning on trajectories generated under the improved harness. WHALE alternates weight updates with Meta-Harness search and, on Qwen3.5-2B/4B agents across search QA, mathematics, and chess, reports best-mean@8 gains of 4.15–24.38 percentage points over weight-only, harness-only, and a fast/slow-training baseline. The conceptual move is to treat weights and harness as coupled coordinates of one learning system: changing either can move the bottleneck into the other.[^203][^204]

SafeEvolve extends co-evolution into safety alignment. It converts on-policy safety trajectories into bounded, auditable, reversible updates to safety prompts and hierarchical skills while separately applying harness-use SFT and harness-augmented RL to the policy. The authors report a threefold attack-success-rate reduction for Qwen3.5-4B on AgentDojo while modestly improving benign utility. This is exactly the direction a general-purpose self-modifying harness will need to confront: runtime adaptation cannot be separated from governance over which edits are allowed, reversible, attributable, and safe to deploy.[^205]

### Weights and harnesses are two timescales of adaptation

Co-Harness, WHALE, Harness-R1, and SafeEvolve point toward a broader synthesis: model weights and harness state are complementary adaptation media. Weights are slow, amortized, opaque, and expensive to update; harness artifacts are fast, inspectable, reversible, and cheap to patch. In principle a system can use harness adaptation for rapid local learning and periodically consolidate robust lessons into weights. [^206][^207][^208][^209]

But the interaction is not simply additive. A September 8 co-evolution study reports a striking compatibility failure: training a weaker model on full expert trajectories could \*reduce\* performance across all seven enterprise tasks because the updated policy no longer fit the harness that had been optimized around the original model. More targeted on-policy correction fared better. The result is fresh, but it supplies direct evidence for the “assumption debt” thesis: changing the model can invalidate runtime scaffolding, and changing the runtime can alter which model behaviors are useful. [^210]

A future training stack may therefore treat the model and harness as a coupled system with separate plasticity rates. Harness changes should be tagged with the model behaviors they assume; post-training should include harness regression suites; and successful lessons should be consolidated only when they remain beneficial after the beneficiary policy changes.

## 5.10 Protocolized edges

The harness is becoming more modular because its boundaries are being standardized. MCP primarily covers agent/model to tools and data; A2A covers agent-to-agent discovery and task communication; ACP standardizes editor/client-to-coding-agent interaction and supports session creation/resume, updates, permissions, and cancellation. These protocols do not solve orchestration, safety, or memory by themselves, but they make those concerns replaceable rather than hard-wired.[^211][^212][^213]

## 5.11 Meta-harnesses, harness hosting, and a prospective Harness ABI

A new systems layer is appearing above individual harnesses. Omnigent describes itself as a meta-harness capable of running Claude Code, Codex, Cursor, OpenCode, Pi, custom agents, and others behind common sessions, policies, sandboxes, and collaboration surfaces; Databricks now offers a managed version. The 2026 source-code study also identifies “harness hosting” as a distinct role emerging around ACP. [^214][^215][^216]

This is evidence for a prospective **Harness ABI**: a stable boundary through which an external control plane can start/resume/cancel a session, stream events, supply context/tools/skills, request permission, inspect artifacts, and account for cost while allowing the internal loop to differ radically. ACP, MCP, A2A, App Server interfaces, and cloud-agent hosting are partial pieces of this boundary rather than one complete standard.

The architectural opportunity is important for research. If harnesses can be hosted behind a common semantic interface, then model × harness factorial evaluation becomes cheaper; an optimizer can swap harness implementations without rebuilding environments; and organizations can enforce cross-cutting policy outside proprietary agent internals. The risk is a lowest-common-denominator interface that hides exactly the model-conditioned features that drive performance. A useful Harness ABI therefore needs extension/version negotiation and typed behavioral capabilities, not just a chat endpoint.

## 5.12 Procedural representations between free-form memory and hard-coded workflows

Another frontier is emerging between “store a reflection as text” and “rewrite the whole harness as code.” Procedural graphs and similar systems represent reusable behavior as explicit, editable procedures with relations among them. Runtime localization selects relevant procedure fragments; an updater compares successful and failed executions and proposes validated edits. The September evidence is preliminary, but the representation addresses a real gap: agents need procedural memory that is more structured than prose yet more adaptable than a fixed DAG. [^217]

This middle layer may become especially valuable for a general-purpose harness. A procedure can carry preconditions, expected evidence, allowed capabilities, failure handlers, provenance, version, and validation tests. It can then be retrieved like a skill, compiled into model-facing instructions or deterministic workflow nodes, and evolved independently of the entire runtime. That makes procedural knowledge a candidate first-class object in a Harness IR.

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

Agent benchmarks often entangle model quality, harness quality, environment reliability, and evaluation infrastructure. Harness-Bench directly addresses configuration effects with shared environments, budgets, and protocols across 5,194 trajectories. SWE-agent showed earlier that the ACI itself changes outcomes. Current benchmark science therefore needs factorial experiments rather than a single score attached to a model name.[^218][^219]

A second problem is evaluating adaptive harnesses without clean labels. The August 2026 continual-learning study proposes teacher-relative lift: sparse corrections from a stronger teacher are used to measure whether a student+harness converges toward the teacher; in its cybersecurity setting, this correlated with a held-out gold standard, while similarly powered LLM-as-judge provided little useful signal. The approach is domain-limited but points toward evaluation methods for deployed systems that cannot constantly obtain ground truth.[^220]

## Second-pass evidence ledger: what changed relative to v1

The second pass raises confidence in several architectural conclusions while lowering confidence in broad claims about autonomous evolution. The table below summarizes the direction of update.

| **Finding** | **v2 assessment** | **Why the assessment changed** |
|----|----|----|
| Model-harness interaction is first-class | High confidence | Harness-Bench plus newer compatibility and beneficiary-compliance studies show non-monotonic, model-conditioned effects. [^221][^222][^223] |
| Harness optimization can improve systems | Medium-high, conditional | Many independent methods report gains, but search budget, attribution, and held-out transfer are major confounds. [^224][^225][^226][^227][^228] |
| Fully autonomous harness design supersedes human engineering | Not established | HarnessDev and counterevaluation show persistent gaps and unstable transfer. [^229][^230] |
| Security must be enforced below model text | High confidence | CaMeL, Fides, production containment, and new context-privilege attacks independently support deterministic boundaries. [^231][^232][^233][^234] |
| Memory requires lifecycle semantics beyond retrieval | Medium-high, rapidly strengthening | Invalidation and revocation studies expose stale/authoritative-memory failures; underlying systems principle is mature. [^235][^236] |
| Long-horizon agents need distributed-system semantics | High as engineering principle; medium as benchmarked claim | Production runtimes and cloud products independently converge on durable state, recovery, isolation, and event-driven execution. [^237][^238][^239] |
| A common harness-hosting layer is emerging | Medium-high industry evidence | Omnigent, ACP-style hosting, and cloud runtimes make harness interchangeability an explicit product feature. [^240][^241][^242] |

A useful rule for reading the 2026 literature is to distinguish **mechanism evidence** from **performance evidence**. A new paper can be convincing that a failure mode exists or that an architecture is implementable even when its headline benchmark lift is too fresh to trust. For example, context privilege escalation is an important mechanism even before every reported attack rate is replicated; conversely, a large self-evolution benchmark gain should not be generalized until its compute accounting and transfer are clear.

The field is therefore best described as pre-paradigmatic but no longer anecdotal. There are now shared failure classes, repeated production patterns, explicit harness benchmarks, source-code comparative studies, and multiple algorithm families that manipulate the harness. What remains immature is causal theory: which interventions transfer, why a particular model benefits, and how to predict a harness edit before paying the full evaluation cost.

# 7. Industry architectures and open-source systems

Production systems reveal what survives contact with real workloads. They also contain product-specific assumptions, so they should be mined for mechanisms rather than copied wholesale.

## OpenAI: repository legibility, the Codex harness, and orchestration above the session

OpenAI’s 2026 harness-engineering case study treats the repository itself as part of the runtime: short navigational agent instructions point into structured documentation; architectural constraints are enforced by code and tests; browser/UI and observability data are made legible to the agent; and agent-to-agent review loops replace much routine human review. This is “environment engineering” as harness engineering.[^243]

Symphony moves one level higher. Rather than supervising chat sessions, it uses a project tracker as a control plane, gives each issue an isolated workspace, reconciles running agents with work state, retries failures, and asks agents to carry work through CI/review. The reference is intentionally minimal - a spec plus implementation - and is better understood as an orchestration pattern than a complete general-purpose harness.[^244][^245]

## Anthropic: context, tools, long-horizon state, and containment

Anthropic’s engineering sequence is unusually coherent: simple composable agents (2024), multi-agent orchestration and tool design (2025), context engineering and skills, long-running persistence, then 2026 work on specialized harnesses, decoupled managed-agent runtime, and environment-level containment. The recurring principle is to keep model-facing interfaces simple while making state, tools, and safety explicit outside the model.[^246][^247][^248][^249][^250][^251]

## Cursor: continuous model-harness tuning and always-on agents

Cursor describes harness work as a product optimization loop: form hypotheses, evaluate offline, A/B test online, instrument tool errors, and tune prompts/tools per model family and version. It also documents the deletion of old guardrails as models improve. By August 2026, cloud agents could subscribe to PRs/Slack/schedules, retain long-lived goals, wake on events, and run subagents in isolated VMs - a concrete move from interactive assistant to durable agent system.[^252][^253]

## Cloud and platform convergence: harness as a deployable substrate

AWS’s managed AgentCore harness exposes a declarative agent that owns the orchestration loop, context, persistent state, failure recovery, session isolation, filesystem/shell, skills, browsing, and model switching; it can export to code when custom orchestration is required. Microsoft’s Agent Framework Harness packages loop, planning, memory, context management, approvals, and telemetry into a customizable runtime. [^254][^255]

Cloudflare draws a particularly clean boundary: its Agents SDK runtime supplies durable identity, state, sessions, routing, scheduling, fibers, and observability, while a harness supplies model calls, prompt construction, tool selection, persistence strategy, streaming, and lifecycle hooks. This separation is analytically useful even if other systems partition responsibilities differently. [^256]

Omnigent and Databricks make a different bet: instead of one opinionated inner loop, expose a common meta-harness/control layer over several existing harnesses and custom agents. If this pattern persists, “agent platform” will increasingly mean a substrate that hosts heterogeneous harnesses while centralizing security, sandboxes, identity, policy, telemetry, and collaboration. [^257][^258]

## What the 11-system source-code audit changes

Barbaste et al.’s 2026 source-code study is unusually valuable because it inspects implementation rather than architecture diagrams: eleven production coding harnesses totaling roughly four million lines, with a longitudinal re-pin of the original corpus. Its headline absences are informative: none of the eleven imports a general-purpose agent framework for its core loop, and none uses vector embeddings for code retrieval; instead, the systems favor hand-rolled asynchronous control and deterministic repository-navigation mechanisms. [^259]

The same study reports widespread convergence on skills, MCP, and ACP-style edges, and observes behavioral policy moving from long prompt prose into structured configuration. These findings should not be generalized to every agent domain, but they are strong evidence that mature coding harnesses are becoming platforms with explicit extension surfaces rather than thin prompt wrappers. [^260]

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

Repository sources and current descriptions: Codex, Symphony, OpenHands SDK, OpenCode, Goose, Pi, SWE-agent, Prime Agent, and OpenHarness.[^261][^262][^263][^264][^265][^266][^267][^268][^269]

A 2026 source-code study of eleven coding systems is valuable as a map rather than a final taxonomy. It reports recurring hand-rolled async loops, deterministic code retrieval rather than vector-embedding retrieval across its audited systems, high adoption of skills/MCP/ACP, and an evolution from “tool” toward “platform.” Those findings are auditable within its corpus but should not be generalized beyond the studied coding-agent population without further work.[^270]

## Expanded repository atlas: what to read in source, and why

The second-pass source review broadens the code syllabus beyond projects that self-identify with the word “harness.” The goal is not to list every agent repository; it is to sample distinct architectural answers to the same runtime problems.

| **Repository / system** | **Read it for** | **Distinctive architectural signal** |
|----|----|----|
| openai/codex | Production coding loop | Rust core; sandbox/approval semantics; app-server boundary; model-specific tool loop. [^271] |
| google-gemini/gemini-cli | Production coding loop | Independent implementation of terminal agent, tools, skills, MCP and provider-specific behavior. [^272] |
| mistralai/mistral-vibe | Minimal vendor coding loop | Compact CLI; plan/edit/auto modes and ACP-facing integration. [^273] |
| Aider-AI/aider | Pre-harness coding lineage | Repo map, git-first edits, lint/test feedback; useful minimalist comparison to modern autonomous loops. [^274] |
| SWE-agent / mini-SWE-agent | Research-minimal baseline | ACI lineage; small enough for ablation and reproduction. [^275] |
| OpenHands/software-agent-sdk | Composable production SDK | Agents/tools/conversations/workspaces/events; local vs ephemeral Docker/Kubernetes execution. [^276] |
| anomalyco/opencode | Large open coding product | Provider abstraction, plan/build modes, plugins, subagents, mature product surface. [^277] |
| aaif-goose/goose | General local harness | Rust runtime, many model providers, MCP extensions, CLI/desktop/API; useful non-code-only comparison. [^278] |
| badlogic/pi-mono | Readable embeddable runtime | Unified model API, sessions, extensions, subagents; excellent code-reading surface. [^279] |
| browser-use/browser-use | Browser/computer harness | Real browser action space, persistent tools and recovery loops; tests whether coding-harness lessons transfer to web environments. [^280] |
| PrimeIntellect-ai/prime-agent | Recursive/continual experiment | Persistent REPL/state, recursive subagents, skills/memory; close to frontier research concepts. [^281] |
| agiresearch/AIOS | Agent operating-system substrate | Scheduling, context, memory, storage, tools and access control in a shared kernel. [^282] |
| omnigent-ai/omnigent | Meta-harness / host | Runs multiple existing harnesses under common sessions, policies, sandboxes and collaboration. [^283] |
| jennyzzt/dgm | Self-modifying agent research | Open-ended code-level evolution of the agent implementation with empirical selection. [^284] |
| google-research/camel-prompt-injection | Security-kernel research | Control/data separation and capability-mediated tool policy. [^285] |
| microsoft/fides | Information-flow-control research | Formal/deterministic confidentiality and integrity enforcement around agent planning. [^286] |

A useful practical reading strategy is to trace one vertical slice across several repositories rather than trying to understand every product end to end. For example: follow **context construction** through Codex, Pi, OpenHands, Browser Use, and AIOS; then follow **tool dispatch and error representation**; then **permissions/sandboxing**; then **session/event persistence**. This makes model-specific design choices and hidden assumptions much easier to compare.

The repository atlas also reveals a useful division between \*harness code\* and \*harness configuration\*. Some systems move behavior into skills, AGENTS.md/CLAUDE.md-like files, YAML, or plugin manifests; others keep policy in the runtime. A portable Harness IR should probably represent both and make the compilation boundary explicit rather than privileging either code or prose.

# 8. Evaluation and benchmark science

A harness is an intervention on a stochastic system. Good harness engineering therefore needs experimental design closer to systems benchmarking than prompt tinkering.

- Version everything: model snapshot/provider, harness commit, system prompt, tools, context policy, sandbox image, protocol versions, benchmark data, graders, and resource budgets.

- Use paired/factorial comparisons. Hold model and task fixed while varying one harness mechanism; repeat across multiple model families to measure transfer.[^287][^288]

- Measure process as well as final success: tool errors, invalid actions, retries, context usage, evidence alignment, state divergence, time, dollar cost, and resource consumption.[^289][^290]

- Prefer executable/end-state oracles when possible. LLM judges are useful for qualitative dimensions but can be correlated with the same failure modes as the tested agent.

- Report distributions, not only means. Long-horizon agents have heavy-tail failures: one runaway loop, corrupted state, or unrecoverable action can dominate operational risk.

- Test robustness across capability changes. A harness improvement that disappears with a larger context window or reverses on a new model should be identified as conditional, not universal.[^291][^292]

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

## Benchmark families measure different harness capabilities

No single benchmark can establish that one harness is generally better. SWE-bench rewards repository navigation, editing, and executable software feedback; Terminal-Bench emphasizes shell/terminal competence; WebArena and OSWorld stress web/computer interaction; τ-bench stresses multi-turn tool use under domain policies and repeated-run consistency; long-task-horizon work asks how success probability changes with task duration. Harness-specific benchmarks then add the missing factorial question: what changes when the model is held fixed and the runtime changes? [^293][^294][^295][^296][^297][^298][^299]

A canonical evaluation suite for a general-purpose harness should therefore be stratified by environment rather than averaged into one opaque score. At minimum: coding/terminal, browser/computer use, information search/research, structured tool/API tasks, persistent multi-episode tasks, adversarial-security tasks, and long-running state/recovery tasks. Each family should include both ordinary success metrics and harness-specific process metrics.

## The minimum experimental design for harness research

Treat model, harness, environment image, task, resource budget, and random seed as explicit factors. Freeze versions and release them together. Compare harness changes pairwise on the same tasks and models, but also test interactions across model families. Report confidence intervals and per-task distributions; agent systems have heavy tails and a small number of runaway or catastrophic failures can dominate operational risk.

For adaptive/evolving methods, budget accounting is non-negotiable. The relevant comparison is not “evolved harness after N expensive attempts” against “static baseline with one attempt”; it is evolved harness against alternatives that receive comparable inference, feedback, and evaluation resources. Rethinking Harness Evolution demonstrates why this can reverse apparent conclusions. [^300]

Separate **search-time benefit** from **artifact benefit**. After optimization, freeze the resulting harness and evaluate it on unseen tasks without the optimizer’s extra compute. Then test transfer to a different model and, when possible, a different environment. If the improvement disappears, the system may still be a useful test-time search algorithm, but it has not demonstrated portable harness learning.

Security and memory need dedicated invariants rather than being folded into average task success. An agent can complete the task while leaking a secret, acting on revoked policy, bypassing an authority boundary, duplicating a side effect after retry, or leaving inconsistent durable state. A serious benchmark bundle therefore needs safety properties and recovery semantics that can fail independently of nominal task completion. [^301][^302][^303]

## A reproducible harness bundle

The field would benefit from a standard artifact analogous to a containerized benchmark submission: model/provider snapshot identifier; harness commit; model-profile configuration; complete system/developer instructions; tool schemas and versions; skills/plugins; environment/container image; permissions; context/memory policies; resource budgets; grader code; task data; and raw event traces. This turns a leaderboard row into an inspectable scientific object and makes later model-version regressions attributable rather than mysterious.

# 9. What is genuinely new - and what is renamed

Harness engineering is partly a new label over familiar systems ideas. State machines, retries, sandboxes, observability, workflow engines, capability security, event logs, and multi-process fault isolation are not novel because an LLM is involved. ReAct resembles classical perception-action loops; memory architectures have long histories; planner-worker patterns predate foundation models.

What is genuinely new is the object being controlled: a highly capable, stochastic, instruction-conditioned program synthesizer that can interpret natural language, write new tools, change its own workspace, and reason about arbitrary interfaces at runtime. That creates unusual engineering properties:

- The interface is partly semantic. Tool names/descriptions and context formatting change behavior, so APIs are simultaneously software contracts and learned affordances.

- Control is movable. As the model improves, logic can migrate from deterministic code into model choice - or be pulled back into code for reproducibility.

- The system can help redesign itself. A model can inspect traces, propose prompt/tool/middleware changes, generate evaluators, and sometimes author the next harness version.[^304][^305][^306]

- The harness can change effective capability without changing weights. This makes inference-time systems engineering an orthogonal scaling axis to pretraining and post-training.[^307][^308]

- The runtime is now exposed to adversarial natural-language inputs that can influence action-taking, creating a distinctive coupling between classic security boundaries and prompt-injection risk.[^309]

## The genuinely new part is semantic systems engineering

Many individual mechanisms are old, but their coupling is new. A conventional API does not change behavior because its function is renamed; a language-model tool often can. A conventional scheduler does not infer its own workflow from natural-language observations; an agent may. A conventional process cannot rewrite its scheduler after reading its own traces; an agentic optimizer increasingly can. The engineering surface is therefore partly symbolic/semantic and partly ordinary software.

This is why “harness engineering” is not just another name for distributed systems or prompt engineering. It is the discipline of deciding which semantics to entrust to a learned policy and which to externalize into deterministic representation, then measuring that boundary under changing models. The frontier moves as model capabilities move: what required explicit state machines in one generation may become unnecessary scaffolding in the next, while newly autonomous behavior may make stronger security or verification necessary.

The most distinctive research object is consequently **the moving boundary of control**. Harness engineering asks not only how to build the wrapper, but how to allocate cognition among weights, context, code, tools, memories, subagents, and humans; how to make those allocations observable; and how to migrate them safely as the beneficiary model changes.

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

Harness validity versus compliance. Can we predict whether a given model will actually activate and follow a correct harness artifact? What interface properties make a policy legible to the beneficiary model, and can compliance be measured independently from artifact validity? [^310][^311]

Harness compatibility surfaces. Instead of “best harness,” can we learn a response surface over model family/version × task × context budget × tool ABI × environment, and compile model-specific profiles from a common semantic representation? [^312][^313]

Artifact lifecycle and revocation. What semantics should apply when a skill, memory, policy, tool wrapper, or optimizer-generated rule is superseded or revoked? How should downstream artifacts inherit invalidation, and what should be immutable for audit? [^314][^315]

Reference-monitor minimality. What is the smallest deterministic trusted computing base that can enforce authority, provenance, information-flow, and effect policies without becoming an unmaintainable second agent? Can capability and IFC designs scale to messy production tools? [^316][^317][^318]

Safe extension supply chains. Skills, hooks, plugins, MCP servers, repository instructions, and fetched content are executable or behavior-changing dependencies. How should harnesses pin, sign, sandbox, taint, update, and audit them, especially when the agent itself can install new extensions?

Procedural intermediate representations. Is there a representation between prose memory and arbitrary code that supports typed preconditions/effects, composition, retrieval, validation, migration, and automatic repair? [^319]

Counterfactual execution. Can event-sourced traces support reliable “what if this harness component had behaved differently?” experiments without rerunning the entire environment, and where does nondeterminism make such attribution invalid?

Optimization economics. Which harness component deserves the next unit of evaluation compute? Task-CoEvolve and selective verification begin to address this, but a general theory should allocate budget across candidate generation, diagnosis, verification, transfer testing, and model calls. [^320][^321]

Consolidation between harness and weights. Which runtime lessons should remain explicit and reversible, and which should eventually be distilled into the model? How can consolidation avoid breaking the model-harness compatibility that produced the lesson? [^322][^323]

Organizational semantics. Long-lived agent work is increasingly triggered by issues, schedules, webhooks, PRs, messages, and external events rather than by a live chat turn. What accountability, escalation, ownership, and audit model should govern fleets of agents acting as persistent organizational processes? [^324][^325]

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

## Canonical v2 additions to the build blueprint

**Separate the semantic control plane from the execution plane.** The control plane owns typed goals, events, artifacts, policies, budgets, model profiles, procedure/skill metadata, and permission decisions. Execution workers own ephemeral filesystem/browser/shell state and expose effectful operations through narrow handles. This makes it possible to replace a worker or model without losing the authoritative run state.

**Make provenance a first-class field, not logging metadata.** Every context item should be traceable to an origin and authority class; every memory should carry version and validity; every tool result should identify the tool/version/environment; every harness edit should point to the trajectories and failure hypothesis that motivated it. The security monitor and context builder should consume this metadata directly.

**Introduce a model-profile compiler.** Keep the semantic meaning of tools, capabilities, memories, and procedures stable, but let a profile compile them into model-specific prompt layouts, tool names/schemas, error formats, compaction policies, and interaction modes. The profile is then an independently testable harness component whose assumptions can expire.

**Implement the security kernel before autonomous self-modification.** Capability checks, secret mediation, egress policy, sandbox boundaries, extension trust, and audit need to be outside the editable policy surface. The evolution service may propose a narrower permission or a new request for approval, but it should not be able to silently enlarge its own authority.

**Treat evolution as a deployment pipeline.** Candidate generation -\> explicit failure hypothesis -\> targeted counterexample set -\> matched-budget evaluation -\> held-out regression -\> cross-model compatibility test -\> security invariant check -\> canary -\> rollout -\> retirement test. Every accepted change should be reversible and carry an expiry/revalidation policy.

**Use event sourcing selectively, not dogmatically.** The append-only event log should be the authoritative record of decisions and effects, while compact materialized views serve fast context/retrieval. Do not force every large artifact into the event stream; store content-addressed artifact references instead. This keeps replay/provenance without turning the transcript into the database.

## A concrete Harness IR sketch

A research-oriented IR could define typed entities for Goal, Observation, ContextItem, Memory, Procedure, ToolCapability, Permission, Effect, Artifact, Validator, AgentProcess, Budget, and HarnessRule. Each entity carries version/provenance; edges encode depends-on, supersedes, authorizes, produced-by, validates, and delegated-to relations. The runtime compiles the IR into model-facing context and executable control structures.

The important design choice is to keep the IR **behavioral rather than framework-specific**. A ToolCapability should not mean “a LangChain tool”; it should describe semantics/effects and admit compilers to MCP, native function calling, a shell wrapper, or a provider-specific tool. A Procedure should be able to compile to instructions, a deterministic workflow node, or a subagent task depending on model profile and risk.

If this representation is paired with complete event traces and evaluation metadata, harness edits become typed diffs rather than opaque prompt rewrites. That would make automated optimization, causal attribution, compatibility testing, rollback, and cross-runtime portability much more tractable - and would directly address the fragmentation exposed by both production source code and the new meta-harness layer. [^326][^327]

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

## Updated highest-leverage thesis: an evidence-bearing adaptive runtime

The second pass strengthens rather than overturns v1’s recommended research program, but makes it more specific. The strongest novel system would combine a **portable Harness IR**, **event-sourced durable runtime**, **reference-monitor security kernel**, **model-profile/tool compiler**, **trust-aware memory**, and **evidence-bearing evolution service**. The unifying idea is that every adaptive behavior should remain inspectable as an artifact with provenance, authority, validity, predicted effect, evaluation history, and rollback semantics.

This is more ambitious than another agent SDK but narrower than building a frontier model. It targets the layer where current production systems are converging but still highly bespoke, and where academic work is beginning to optimize runtime structure without a stable representation or deployment discipline. The system itself becomes a research instrument: it can run factorial model-harness experiments, collect comparable traces, evolve one component at a time, and publish reproducible harness bundles.

A particularly defensible first paper/project would be **Harness IR + compiler + benchmark harness** rather than immediate self-evolution. Demonstrate that the same semantic harness can compile to two or three materially different runtimes/model profiles, preserve policy and task semantics, and enable controlled ablations across models. Then add a constrained optimizer that edits one typed component class and must pass transfer/security/retirement gates. This produces a clean scientific ladder rather than a monolithic “general agent” claim.

# 13. Reading and repository syllabus

A compact path from foundations to the frontier:

## Tier 0 - systems prehistory and the missing optimization lineage

Read classical agent/control and operating-system concepts selectively rather than exhaustively: perception-action loops, state machines, BDI, schedulers, process isolation, capabilities, event sourcing, transactions, and test harnesses. The goal is vocabulary for invariants and failure semantics, not historical completeness.

Then read OPRO and Promptbreeder for language-level optimization; DSPy for compiled LM programs; GPTSwarm and TextGrad for optimizable computation graphs; ADAS and AgentSquare for automated agent architecture search; AFlow for workflow-code search; and Darwin Gödel Machine / GEPA for self-modifying agent code and reflective evolutionary optimization. This is the direct intellectual runway into harness evolution. [^328][^329][^330][^331][^332][^333][^334][^335][^336][^337]

## Tier 1 - conceptual foundations

- Vaswani et al. - Attention Is All You Need: understand the model substrate, while remembering it is not an agent paper.[^338]

- Brown et al. - GPT-3: understand inference-time programmability via context.[^339]

- Lewis et al. - RAG: external memory as a model-external capability.[^340]

- ReAct: the canonical reason-act-observe loop.[^341]

- PAL: deterministic runtime as complement to probabilistic reasoning.[^342]

- SWE-agent: interface design as an independent agent performance lever.[^343]

- Anthropic - Building effective agents: simple workflows vs agent-controlled loops.[^344]

## Tier 2 - production harness engineering

- OpenAI - Unrolling the Codex agent loop; then Harness engineering; then Symphony.[^345][^346][^347]

- Anthropic - Context engineering, tool design, Agent Skills, long-running harnesses, Managed Agents, and containment.[^348][^349][^350][^351][^352][^353]

- Cursor - Dynamic context discovery, Continually improving our agent harness, cloud-agent lessons, and the August 2026 always-on harness update.[^354][^355][^356][^357]

- LangChain - The Anatomy of an Agent Harness, mainly for the broad taxonomy and vocabulary.[^358]

## Tier 3 - 2026 research frontier

- Harness-Bench - start here for the measurement problem.[^359]

- Agentic Harness Engineering and RHO - trajectory-driven harness optimization.[^360][^361]

- MemoHarness - experience-conditioned per-case harness adaptation.[^362]

- Evo-Bench and HarnessDev - whether models can evolve/create harnesses, including negative/conditional evidence.[^363][^364]

- JIT-Agent - dedicated model for just-in-time harness synthesis; very fresh, high-claim preprint.[^365]

- Recursive Agent Harnesses and Prime Agent - full-harness recursion and continual runtime.[^366][^367]

- LLM-as-Code and Predictable Agentic Systems - deterministic-control pole of the design space.[^368][^369]

- Logos - cross-process, append-only, fault-isolated harness runtime.[^370]

- HEART / Tool Primitives - dynamic, agent-native tool-interface frontier; provisional.[^371]

- Source-Code Study of Eleven Systems - broad anatomy of contemporary coding harness implementations.[^372]

## Repositories to read in source

- openai/codex - mature coding harness internals.[^373]

- openai/symphony - minimal durable work orchestration.[^374]

- OpenHands/software-agent-sdk - server/runtime/workspace/event decomposition.[^375]

- badlogic/pi-mono - readable core runtime and embedding surface.[^376]

- SWE-agent / mini-SWE-agent - minimal research baselines and ACI lineage.[^377]

- anomalyco/opencode and aaif-goose/goose - large, active general/product harnesses.[^378][^379]

- PrimeIntellect-ai/prime-agent - frontier recursive/continual experimental architecture.[^380]

## Tier 4 - second-pass frontier and counterevidence

Read Rethinking the Evaluation of Harness Evolution immediately after the positive evolution papers; it is the cleanest antidote to confusing persistent harness learning with additional test-time search. Then read HarnessDev and EVOHARNESSBENCH for construction, transfer, forgetting, and continual-expansion limits. [^381][^382][^383]

For optimization mechanics, read Harness-R1, gated semantic quality-diversity, Task-CoEvolve, HarnessLens, PRISM, and the co-evolution papers. Focus less on headline scores than on editable representation, credit assignment, validation budget, beneficiary compatibility, and deployment gates. [^384][^385][^386][^387][^388][^389]

For security, read AgentDojo -\> CaMeL -\> Fides -\> context-privilege-escalation -\> CapScope. This sequence moves from measuring prompt injection to designing deterministic control/data and authority boundaries at the harness layer. [^390][^391][^392][^393][^394]

For memory, pair MemGPT/Generative Agents with Invalidation Contracts and revocation research. The contrast shows the field’s shift from “how can agents remember?” to “how can long-lived memory remain valid, authorized, and safely forgotten or superseded?” [^395][^396][^397][^398]

## Source-code study path for a future harness builder

Week 1: mini-SWE-agent, Aider, and Pi - learn the smallest viable loops and repository interfaces. Week 2: Codex and OpenHands - study production boundaries, events, sandboxes, sessions, and APIs. Week 3: Browser Use and AIOS - test whether abstractions transfer beyond coding and what belongs in a shared kernel. Week 4: Omnigent plus DGM/AHE/Meta-Harness research artifacts - study hosting and adaptation above the inner loop. [^399][^400][^401][^402][^403][^404][^405][^406][^407][^408][^409]

# Conclusion

Harness engineering is becoming a real research area, but it is still pre-paradigmatic. Its vocabulary is unsettled, most 2026 academic results are new, many are preprints, and coding tasks dominate the evidence. Yet the field already has a coherent object of study: the external runtime that converts model inference into stateful action. It has independent variables, measurable outcomes, emerging benchmarks, recognizable architecture patterns, and now algorithms that optimize the harness itself.

The deepest shift is conceptual. Foundation-model progress made it tempting to treat intelligence as residing entirely in weights. Agentic systems reveal a more cybernetic picture: useful intelligence emerges from the closed loop among model, context, tools, state, environment, feedback, and constraints. The harness is the engineered part of that loop. As models get stronger, its job is not to micromanage them; it is to make the world legible, actions safe and verifiable, state durable, coordination economical, failures diagnosable, and improvement experimentally accountable.

For a new general-purpose harness, the best opportunity is therefore not maximal scaffolding. It is a small, rigorous runtime with excellent semantics - explicit state, capabilities, artifacts, validation and tracing - that can expose its own policy layer to controlled evolution. That architecture would be simple enough to understand, strong enough to run for days, and structured enough to become a research instrument for discovering what harness intelligence should mean.

Canonical v2 sharpens the central thesis: harness engineering is best understood as **inference-time policy and systems engineering for learned agents**. The harness is where semantic context meets deterministic execution, where model-generated intent becomes authorized effects, where memory becomes durable institutional state, and where a stochastic policy is made observable enough to improve scientifically.

The deepest frontier is no longer “how much scaffolding should we add?” It is how to build a runtime that can **adapt without becoming opaque**: model-conditioned but portable, self-improving but regression-tested, memory-rich but revocable, autonomous but capability-bounded, long-running but recoverable, multi-agent when economically justified, and measurable as a coupled model-harness-environment system.

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

## Canonical v2 second-pass additions

Yang et al. (ICLR 2024), Large Language Models as Optimizers (OPRO) https://arxiv.org/abs/2309.03409

Fernando et al. (ICML 2024), Promptbreeder https://arxiv.org/abs/2309.16797

Zhuge et al. (ICML 2024), GPTSwarm https://proceedings.mlr.press/v235/zhuge24a.html

Yuksekgonul et al. (2024), TextGrad https://arxiv.org/abs/2406.07496

Yuan et al. (2024), EvoAgent https://arxiv.org/abs/2406.14228

Hu, Lu & Clune (ICLR 2025), Automated Design of Agentic Systems https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html

Shang et al. (2024), AgentSquare https://arxiv.org/abs/2410.06153

Zhuge et al. / FoundationAgents, AFlow code https://github.com/FoundationAgents/AFlow

Zhang et al. (2025/ICLR 2026), Darwin Gödel Machine https://arxiv.org/abs/2505.22954

Agrawal et al. (2025/ICLR 2026 Oral), GEPA https://arxiv.org/abs/2507.19457

Wang et al. (2025), Maestro https://arxiv.org/abs/2509.04642

Mei et al. (COLM 2025), AIOS https://arxiv.org/abs/2403.16971

## Security, memory, and runtime semantics

Debenedetti et al. (NeurIPS 2024), AgentDojo https://arxiv.org/abs/2406.13352

Deng et al. (ICLR 2024 Spotlight), ToolEmu https://arxiv.org/abs/2309.15817

Debenedetti et al. (2025), Defeating Prompt Injections by Design / CaMeL https://arxiv.org/abs/2503.18813

Costa et al. (2025), Securing AI Agents with Information-Flow Control / Fides https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

Li et al. (2026), What’s in Your Agent’s Context? Context Privilege Escalation https://arxiv.org/abs/2609.01222

He et al. (2026), When Context Gets Root https://arxiv.org/abs/2608.27299

Bouras, Dai & Mechtaev (2026), Authority Is Not a String / CapScope https://arxiv.org/abs/2609.08371

Packer et al. (2023), MemGPT https://arxiv.org/abs/2310.08560

Park et al. (UIST 2023), Generative Agents https://arxiv.org/abs/2304.03442

Shen, Toyoda & Leung (2026), Revoked but Still Authoritative https://arxiv.org/abs/2609.08258

## Harness evolution, evaluation, and co-evolution

Luo et al. (2026), HarnessBank: Semantic Gene-Bank Search with Gated Verification for Agent-Harness Self-Evolution https://arxiv.org/abs/2607.13683

Harness Updating Is Not Harness Benefit (2026) https://arxiv.org/abs/2605.30621

Verify Smarter, Evolve Further / HarnessLens (2026) https://arxiv.org/abs/2608.27311

HarnessEvolve (2026) https://arxiv.org/abs/2609.00829

Ke et al. (2026), EVOHARNESSBENCH: Can Your Agents Keep Pace with an Evolving Harness? https://arxiv.org/abs/2609.04280

Where Does Harness-Optimization Value Live? (2026) https://arxiv.org/abs/2609.02889

Zhao et al. (EMNLP 2026), Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses (PRISM) https://arxiv.org/abs/2609.05736

Safe Harness Self-Evolution (2026) https://arxiv.org/abs/2609.08175

Procedural Graphs (2026) https://arxiv.org/abs/2609.09153

Co-Evolving Harnesses and Models (2026) https://arxiv.org/abs/2609.09134

## Production platforms and additional repositories

OpenAI (2026), Unlocking the Codex harness: App Server https://openai.com/index/unlocking-the-codex-harness/

Cloudflare (2026), Harnesses https://developers.cloudflare.com/agents/harnesses/

Cloudflare (2026), Bringing more agent harnesses and frameworks to Cloudflare https://blog.cloudflare.com/agents-platform-flue-sdk/

AWS (2026), AgentCore harness generally available https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/

Microsoft (2026), Agent Framework Harness https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/

Databricks (2026), Omnigent on Databricks https://docs.databricks.com/aws/en/omnigent/

Omnigent meta-harness https://github.com/omnigent-ai/omnigent

Google Gemini CLI https://github.com/google-gemini/gemini-cli

Mistral Vibe https://github.com/mistralai/mistral-vibe

Aider https://github.com/Aider-AI/aider

Browser Use https://github.com/browser-use/browser-use

AIOS https://github.com/agiresearch/AIOS

Darwin Gödel Machine https://github.com/jennyzzt/dgm

CaMeL research artifact https://github.com/google-research/camel-prompt-injection

Fides research artifact https://github.com/microsoft/fides

## Benchmark families

Jimenez et al., SWE-bench https://arxiv.org/abs/2310.06770

Terminal-Bench 2.0 https://arxiv.org/abs/2601.11868

Zhou et al., WebArena https://arxiv.org/abs/2307.13854

Xie et al., OSWorld https://arxiv.org/abs/2404.07972

Yao et al., tau-bench https://arxiv.org/abs/2406.12045

METR, Measuring AI Ability to Complete Long Tasks https://arxiv.org/abs/2503.14499

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

[^22]: Chengrun Yang et al., “Large Language Models as Optimizers,” ICLR 2024; arXiv:2309.03409. https://arxiv.org/abs/2309.03409

[^23]: Omar Khattab et al., “DSPy: Compiling Declarative Language Model Calls into Self-Improving Pipelines,” arXiv:2310.03714. https://arxiv.org/abs/2310.03714

[^24]: Mingchen Zhuge et al., “GPTSwarm: Language Agents as Optimizable Graphs,” ICML 2024. https://proceedings.mlr.press/v235/zhuge24a.html

[^25]: Shengran Hu, Cong Lu, and Jeff Clune, “Automated Design of Agentic Systems,” ICLR 2025; arXiv:2408.08435. https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html

[^26]: Jiayi Zhang et al., “AFlow: Automating Agentic Workflow Generation,” ICLR 2025. https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

[^27]: Jenny Zhang et al., “Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents,” arXiv:2505.22954 (2025; ICLR 2026). https://arxiv.org/abs/2505.22954

[^28]: Lakshya A. Agrawal et al., “GEPA: Reflective Prompt Evolution Can Outperform Reinforcement Learning,” arXiv:2507.19457 (2025; ICLR 2026 Oral). https://arxiv.org/abs/2507.19457

[^29]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^30]: “Harness Updating Is Not Harness Benefit,” arXiv:2605.30621 (2026). https://arxiv.org/abs/2605.30621

[^31]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^32]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^33]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^34]: Zichuan Li et al., “What’s in Your Agent’s Context? Context Privilege Escalation Attacks against AI Agent Harness,” arXiv:2609.01222, September 1, 2026. https://arxiv.org/abs/2609.01222

[^35]: Dimitrios Stamatios Bouras, Yihan Dai, and Sergey Mechtaev, “Authority Is Not a String: A Capability-Scoped Harness for Prompt-Injection-Resistant Coding Agents,” arXiv:2609.08371, September 8, 2026. https://arxiv.org/abs/2609.08371

[^36]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^37]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^38]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^39]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^40]: “EVOHARNESSBENCH: Can Your Agents Keep Pace with an Evolving Harness?,” arXiv:2609.04280, September 3, 2026. https://arxiv.org/abs/2609.04280

[^41]: “Where Does Harness-Optimization Value Live?” arXiv:2609.02889 (2026). https://arxiv.org/abs/2609.02889

[^42]: “Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses” (PRISM), arXiv:2609.05736; accepted EMNLP 2026. https://arxiv.org/abs/2609.05736

[^43]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^44]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^45]: Mitchell Hashimoto, “My AI Adoption Journey,” February 5, 2026. https://mitchellh.com/writing/my-ai-adoption-journey

[^46]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^47]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^48]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^49]: “Harness Updating Is Not Harness Benefit,” arXiv:2605.30621 (2026). https://arxiv.org/abs/2605.30621

[^50]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^51]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^52]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^53]: “Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses” (PRISM), arXiv:2609.05736; accepted EMNLP 2026. https://arxiv.org/abs/2609.05736

[^54]: Ashish Vaswani et al., “Attention Is All You Need,” arXiv:1706.03762 (2017). https://arxiv.org/abs/1706.03762

[^55]: Tom B. Brown et al., “Language Models are Few-Shot Learners,” arXiv:2005.14165 (2020). https://arxiv.org/abs/2005.14165

[^56]: Patrick Lewis et al., “Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks,” arXiv:2005.11401 (2020). https://arxiv.org/abs/2005.11401

[^57]: Reiichiro Nakano et al., “WebGPT: Browser-assisted question-answering with human feedback,” arXiv:2112.09332 (2021). https://arxiv.org/abs/2112.09332

[^58]: Ehud Karpas et al., “MRKL Systems: A modular, neuro-symbolic architecture that combines large language models, external knowledge sources and discrete reasoning,” arXiv:2205.00445 (2022). https://arxiv.org/abs/2205.00445

[^59]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^60]: Luyu Gao et al., “PAL: Program-aided Language Models,” ICML 2023; arXiv:2211.10435. https://arxiv.org/abs/2211.10435

[^61]: OpenAI, “Introducing ChatGPT,” November 30, 2022. https://openai.com/index/chatgpt/

[^62]: Timo Schick et al., “Toolformer: Language Models Can Teach Themselves to Use Tools,” NeurIPS 2023; arXiv:2302.04761. https://arxiv.org/abs/2302.04761

[^63]: Noah Shinn et al., “Reflexion: Language Agents with Verbal Reinforcement Learning,” arXiv:2303.11366 (2023). https://arxiv.org/abs/2303.11366

[^64]: Guanzhi Wang et al., “Voyager: An Open-Ended Embodied Agent with Large Language Models,” arXiv:2305.16291 (2023). https://arxiv.org/abs/2305.16291

[^65]: Qingyun Wu et al., “AutoGen: Enabling Next-Gen LLM Applications via Multi-Agent Conversation,” COLM 2024. https://www.microsoft.com/en-us/research/publication/autogen-enabling-next-gen-llm-applications-via-multi-agent-conversation-framework/

[^66]: Chengrun Yang et al., “Large Language Models as Optimizers,” ICLR 2024; arXiv:2309.03409. https://arxiv.org/abs/2309.03409

[^67]: Chrisantha Fernando et al., “Promptbreeder: Self-Referential Self-Improvement via Prompt Evolution,” ICML 2024; arXiv:2309.16797. https://arxiv.org/abs/2309.16797

[^68]: Omar Khattab et al., “DSPy: Compiling Declarative Language Model Calls into Self-Improving Pipelines,” arXiv:2310.03714. https://arxiv.org/abs/2310.03714

[^69]: Mingchen Zhuge et al., “GPTSwarm: Language Agents as Optimizable Graphs,” ICML 2024. https://proceedings.mlr.press/v235/zhuge24a.html

[^70]: Mert Yuksekgonul et al., “TextGrad: Automatic ‘Differentiation’ via Text,” arXiv:2406.07496 (2024). https://arxiv.org/abs/2406.07496

[^71]: Siyu Yuan et al., “EvoAgent: Towards Automatic Multi-Agent Generation via Evolutionary Algorithms,” arXiv:2406.14228 (2024). https://arxiv.org/abs/2406.14228

[^72]: Shengran Hu, Cong Lu, and Jeff Clune, “Automated Design of Agentic Systems,” ICLR 2025; arXiv:2408.08435. https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html

[^73]: Yu Shang et al., “AgentSquare: Automatic LLM Agent Search in Modular Design Space,” arXiv:2410.06153 (2024). https://arxiv.org/abs/2410.06153

[^74]: Jiayi Zhang et al., “AFlow: Automating Agentic Workflow Generation,” ICLR 2025. https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

[^75]: Jenny Zhang et al., “Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents,” arXiv:2505.22954 (2025; ICLR 2026). https://arxiv.org/abs/2505.22954

[^76]: Lakshya A. Agrawal et al., “GEPA: Reflective Prompt Evolution Can Outperform Reinforcement Learning,” arXiv:2507.19457 (2025; ICLR 2026 Oral). https://arxiv.org/abs/2507.19457

[^77]: Xiang Lisa Li et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 2026. https://arxiv.org/abs/2603.28052

[^78]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^79]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^80]: Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^81]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^82]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^83]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^84]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^85]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^86]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^87]: Anthropic, “Introducing advanced tool use on the Claude Developer Platform,” November 24, 2025. https://www.anthropic.com/engineering/advanced-tool-use

[^88]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^89]: Jiayi Zhang et al., “AFlow: Automating Agentic Workflow Generation,” ICLR 2025. https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

[^90]: Kai Mei et al., “AIOS: LLM Agent Operating System,” arXiv:2403.16971; COLM 2025. https://arxiv.org/abs/2403.16971

[^91]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^92]: Mitchell Hashimoto, “My AI Adoption Journey,” February 5, 2026. https://mitchellh.com/writing/my-ai-adoption-journey

[^93]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^94]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^95]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^96]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^97]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^98]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^99]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^100]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^101]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^102]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^103]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^104]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^105]: Cursor Team, “Expanding our long-running agents research preview,” February 12, 2026. https://cursor.com/blog/long-running-agents

[^106]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^107]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^108]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^109]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^110]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^111]: Google, “Announcing the Agent2Agent Protocol (A2A),” April 9, 2025. https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/

[^112]: Agent Client Protocol project, “Agent Client Protocol,” GitHub. https://github.com/agentclientprotocol/agent-client-protocol

[^113]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^114]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^115]: AWS, “AgentCore harness is now generally available,” June 17, 2026. https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/

[^116]: Microsoft, “The Microsoft Agent Framework Harness is now released,” July 22, 2026. https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/

[^117]: Cloudflare, “Harnesses,” Agents documentation, last updated June 3, 2026. https://developers.cloudflare.com/agents/harnesses/

[^118]: Databricks, “Omnigent on Databricks,” updated September 2, 2026. https://docs.databricks.com/aws/en/omnigent/

[^119]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^120]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^121]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^122]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^123]: Zichuan Li et al., “What’s in Your Agent’s Context? Context Privilege Escalation Attacks against AI Agent Harness,” arXiv:2609.01222, September 1, 2026. https://arxiv.org/abs/2609.01222

[^124]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^125]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^126]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^127]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^128]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^129]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^130]: Jediah Katz, “Dynamic context discovery,” Cursor, January 6, 2026. https://cursor.com/blog/dynamic-context-discovery

[^131]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^132]: Noah Shinn et al., “Reflexion: Language Agents with Verbal Reinforcement Learning,” arXiv:2303.11366 (2023). https://arxiv.org/abs/2303.11366

[^133]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^134]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^135]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^136]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^137]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^138]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^139]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^140]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^141]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^142]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^143]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^144]: Anthropic, “Introducing advanced tool use on the Claude Developer Platform,” November 24, 2025. https://www.anthropic.com/engineering/advanced-tool-use

[^145]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^146]: Haibo Jin et al., “Harness Engineering in LLM Tool Use via Agent-Native Reusable Tool Primitives,” arXiv:2609.01736, September 1, 2026. https://arxiv.org/abs/2609.01736

[^147]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^148]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^149]: Prithvi Rajasekaran, “Harness design for long-running application development,” Anthropic, March 24, 2026. https://www.anthropic.com/engineering/harness-design-long-running-apps

[^150]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^151]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^152]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^153]: Saransh Dhage, “Harness Engineering for Predictable Agentic Systems: An Empirical Study of Deterministic Execution Constraints,” arXiv:2608.26197, August 25, 2026. https://arxiv.org/abs/2608.26197

[^154]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^155]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^156]: Josh Ma, “What we’ve learned building cloud agents,” Cursor, June 2, 2026. https://cursor.com/blog/cloud-agent-lessons

[^157]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^158]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^159]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^160]: Simon Yu et al., “Shepherd: A Runtime Substrate Empowering Meta-Agents with a Formalized Execution Trace,” arXiv:2605.10913, May 11, 2026. https://arxiv.org/abs/2605.10913

[^161]: Haoyang Yan et al., “Harness-of-Harness: Multi-Day Autonomous Software Development with Continual Improvement,” arXiv:2609.01481, September 1, 2026. https://arxiv.org/abs/2609.01481

[^162]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^163]: Edoardo Debenedetti et al., “AgentDojo: A Dynamic Environment to Evaluate Prompt Injection Attacks and Defenses for LLM Agents,” arXiv:2406.13352; NeurIPS 2024. https://arxiv.org/abs/2406.13352

[^164]: Zhiwei Deng et al., “ToolEmu: Identifying the Risks of LM Agents with an LM-Emulated Sandbox,” ICLR 2024 Spotlight; arXiv:2309.15817. https://arxiv.org/abs/2309.15817

[^165]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^166]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^167]: Zichuan Li et al., “What’s in Your Agent’s Context? Context Privilege Escalation Attacks against AI Agent Harness,” arXiv:2609.01222, September 1, 2026. https://arxiv.org/abs/2609.01222

[^168]: Xingbang He et al., “When Context Gets Root: Privilege Escalation in LLM Harnesses,” arXiv:2608.27299, August 27, 2026. https://arxiv.org/abs/2608.27299

[^169]: Dimitrios Stamatios Bouras, Yihan Dai, and Sergey Mechtaev, “Authority Is Not a String: A Capability-Scoped Harness for Prompt-Injection-Resistant Coding Agents,” arXiv:2609.08371, September 8, 2026. https://arxiv.org/abs/2609.08371

[^170]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^171]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^172]: Anthropic, “How we built our multi-agent research system,” June 13, 2025. https://www.anthropic.com/engineering/multi-agent-research-system

[^173]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^174]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^175]: Seth Karten et al., “Prime Agent: A Self-Improving RLM Harness,” arXiv:2608.23552, August 24, 2026. https://arxiv.org/abs/2608.23552

[^176]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^177]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^178]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^179]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^180]: Yoonho Lee et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 30, 2026. https://arxiv.org/abs/2603.28052

[^181]: Shuai Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^182]: Atsuyuki Miyai, Kiyoharu Aizawa, and Toshihiko Yamasaki, “Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection,” arXiv:2608.20169, August 20, 2026. https://arxiv.org/abs/2608.20169

[^183]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^184]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^185]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^186]: Yike Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, July 14, 2026. https://arxiv.org/abs/2607.12227

[^187]: Xiang Lisa Li et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 2026. https://arxiv.org/abs/2603.28052

[^188]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^189]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^190]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^191]: Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^192]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^193]: Xiaotian Luo et al., “HarnessBank: Semantic Gene-Bank Search with Gated Verification for Agent-Harness Self-Evolution,” arXiv:2607.13683, July 15, 2026. https://arxiv.org/abs/2607.13683

[^194]: Miyai et al., “Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection,” arXiv:2608.20169, August 20, 2026. https://arxiv.org/abs/2608.20169

[^195]: “Verify Smarter, Evolve Further: Efficient Harness Evolution through Behavior-Aware Verification,” arXiv:2608.27311, August 27, 2026. https://arxiv.org/abs/2608.27311

[^196]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^197]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^198]: “EVOHARNESSBENCH: Can Your Agents Keep Pace with an Evolving Harness?,” arXiv:2609.04280, September 3, 2026. https://arxiv.org/abs/2609.04280

[^199]: “Where Does Harness-Optimization Value Live?” arXiv:2609.02889 (2026). https://arxiv.org/abs/2609.02889

[^200]: “Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses” (PRISM), arXiv:2609.05736; accepted EMNLP 2026. https://arxiv.org/abs/2609.05736

[^201]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^202]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^203]: Zhengyu Chen et al., “Co-Harness: Co-Evolving Harnesses and Model Weights for LLM Agents,” arXiv:2607.22688, July 17, 2026. https://arxiv.org/abs/2607.22688

[^204]: Haechan Kim et al., “WHALE: A Simple Recipe for Joint Harness-Weight Optimization,” arXiv:2609.00196, August 31, 2026. https://arxiv.org/abs/2609.00196

[^205]: Qinghua Mao et al., “SafeEvolve: Harness-Policy Co-Evolution from Agent Experience for Safety Alignment,” arXiv:2609.02786, September 2, 2026. https://arxiv.org/abs/2609.02786

[^206]: Chen et al., “Co-Harness: Co-Evolving Harnesses and Model Weights for LLM Agents,” arXiv:2607.22688 (2026). https://arxiv.org/abs/2607.22688

[^207]: Kim et al., “WHALE: A Simple Recipe for Joint Harness-Weight Optimization,” arXiv:2609.00196, August 31, 2026. https://arxiv.org/abs/2609.00196

[^208]: Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^209]: Qinghua Mao et al., “SafeEvolve: Harness-Policy Co-Evolution from Agent Experience for Safety Alignment,” arXiv:2609.02786, September 2, 2026. https://arxiv.org/abs/2609.02786

[^210]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^211]: Anthropic, “Introducing the Model Context Protocol,” November 25, 2024. https://www.anthropic.com/news/model-context-protocol

[^212]: Google, “Announcing the Agent2Agent Protocol (A2A),” April 9, 2025. https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/

[^213]: Agent Client Protocol project, “Agent Client Protocol,” GitHub. https://github.com/agentclientprotocol/agent-client-protocol

[^214]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^215]: Databricks, “Omnigent on Databricks,” updated September 2, 2026. https://docs.databricks.com/aws/en/omnigent/

[^216]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents — A Source-Code Study of Eleven Systems,” arXiv:2609.00006 (2026). https://arxiv.org/abs/2609.00006

[^217]: “Procedural Graphs: Self-Evolving Execution Structures for LLM Agents,” arXiv:2609.09153, September 8, 2026. https://arxiv.org/abs/2609.09153

[^218]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^219]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^220]: Aryan Luthra et al., “Evaluating Agentic Learning Harness Capabilities Without Labels via the Scaling Hypothesis,” arXiv:2608.13608, August 11, 2026. https://arxiv.org/abs/2608.13608

[^221]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^222]: “Harness Updating Is Not Harness Benefit,” arXiv:2605.30621 (2026). https://arxiv.org/abs/2605.30621

[^223]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^224]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^225]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^226]: Xiang Lisa Li et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 2026. https://arxiv.org/abs/2603.28052

[^227]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^228]: “Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses” (PRISM), arXiv:2609.05736; accepted EMNLP 2026. https://arxiv.org/abs/2609.05736

[^229]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^230]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^231]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^232]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^233]: Anthropic, “How we contain Claude across products,” May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^234]: Zichuan Li et al., “What’s in Your Agent’s Context? Context Privilege Escalation Attacks against AI Agent Harness,” arXiv:2609.01222, September 1, 2026. https://arxiv.org/abs/2609.01222

[^235]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^236]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^237]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^238]: Thomas Gauvin, “Bringing more agent harnesses and frameworks to Cloudflare, starting with Flue,” Cloudflare, June 17, 2026. https://blog.cloudflare.com/agents-platform-flue-sdk/

[^239]: AWS, “AgentCore harness is now generally available,” June 17, 2026. https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/

[^240]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^241]: Databricks, “Omnigent on Databricks,” updated September 2, 2026. https://docs.databricks.com/aws/en/omnigent/

[^242]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents — A Source-Code Study of Eleven Systems,” arXiv:2609.00006 (2026). https://arxiv.org/abs/2609.00006

[^243]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^244]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^245]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^246]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^247]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^248]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^249]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^250]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^251]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^252]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^253]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^254]: AWS, “AgentCore harness is now generally available,” June 17, 2026. https://aws.amazon.com/about-aws/whats-new/2026/06/amazon-bedrock-agentcore-harness-generally-available/

[^255]: Microsoft, “The Microsoft Agent Framework Harness is now released,” July 22, 2026. https://devblogs.microsoft.com/agent-framework/the-microsoft-agent-framework-harness-is-now-released/

[^256]: Cloudflare, “Harnesses,” Agents documentation, last updated June 3, 2026. https://developers.cloudflare.com/agents/harnesses/

[^257]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^258]: Databricks, “Omnigent on Databricks,” updated September 2, 2026. https://docs.databricks.com/aws/en/omnigent/

[^259]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents — A Source-Code Study of Eleven Systems,” arXiv:2609.00006 (2026). https://arxiv.org/abs/2609.00006

[^260]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents — A Source-Code Study of Eleven Systems,” arXiv:2609.00006 (2026). https://arxiv.org/abs/2609.00006

[^261]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^262]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^263]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^264]: Anomaly, “opencode,” GitHub. https://github.com/anomalyco/opencode

[^265]: Agentic AI Foundation, “goose,” GitHub. https://github.com/aaif-goose/goose

[^266]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^267]: SWE-agent project, “SWE-agent,” GitHub; project now recommends mini-SWE-agent for most new work. https://github.com/SWE-agent/SWE-agent

[^268]: Prime Intellect, “prime-agent,” GitHub. https://github.com/PrimeIntellect-ai/prime-agent

[^269]: HKUDS, “OpenHarness,” GitHub. https://github.com/HKUDS/OpenHarness

[^270]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^271]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^272]: Google, “google-gemini/gemini-cli,” GitHub. https://github.com/google-gemini/gemini-cli

[^273]: Mistral AI, “mistralai/mistral-vibe,” GitHub. https://github.com/mistralai/mistral-vibe

[^274]: Aider AI, “Aider,” GitHub. https://github.com/Aider-AI/aider

[^275]: SWE-agent project, “mini-SWE-agent,” GitHub. https://github.com/SWE-agent/mini-swe-agent

[^276]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^277]: Anomaly, “opencode,” GitHub. https://github.com/anomalyco/opencode

[^278]: Agentic AI Foundation, “goose,” GitHub. https://github.com/aaif-goose/goose

[^279]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^280]: Browser Use, “browser-use/browser-use,” GitHub. https://github.com/browser-use/browser-use

[^281]: Prime Intellect, “prime-agent,” GitHub. https://github.com/PrimeIntellect-ai/prime-agent

[^282]: AGI Research, “AIOS,” GitHub. https://github.com/agiresearch/AIOS

[^283]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^284]: Darwin Gödel Machine reference implementation. https://github.com/jennyzzt/dgm

[^285]: Google Research, CaMeL prompt-injection research artifact. https://github.com/google-research/camel-prompt-injection

[^286]: Microsoft, Fides information-flow-control research artifact. https://github.com/microsoft/fides

[^287]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^288]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^289]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^290]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^291]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^292]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^293]: Carlos E. Jimenez et al., “SWE-bench: Can Language Models Resolve Real-World GitHub Issues?” arXiv:2310.06770. https://arxiv.org/abs/2310.06770

[^294]: “Terminal-Bench 2.0: Benchmarking Agents in Interactive Terminal Environments,” arXiv:2601.11868 (2026). https://arxiv.org/abs/2601.11868

[^295]: Shuyan Zhou et al., “WebArena: A Realistic Web Environment for Building Autonomous Agents,” arXiv:2307.13854. https://arxiv.org/abs/2307.13854

[^296]: Tianbao Xie et al., “OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments,” NeurIPS 2024; arXiv:2404.07972. https://arxiv.org/abs/2404.07972

[^297]: Shunyu Yao et al., “τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains,” arXiv:2406.12045 (2024). https://arxiv.org/abs/2406.12045

[^298]: METR, “Measuring AI Ability to Complete Long Tasks,” arXiv:2503.14499 (2025). https://arxiv.org/abs/2503.14499

[^299]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^300]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^301]: Edoardo Debenedetti et al., “AgentDojo: A Dynamic Environment to Evaluate Prompt Injection Attacks and Defenses for LLM Agents,” arXiv:2406.13352; NeurIPS 2024. https://arxiv.org/abs/2406.13352

[^302]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^303]: Dimitrios Stamatios Bouras, Yihan Dai, and Sergey Mechtaev, “Authority Is Not a String: A Capability-Scoped Harness for Prompt-Injection-Resistant Coding Agents,” arXiv:2609.08371, September 8, 2026. https://arxiv.org/abs/2609.08371

[^304]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^305]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^306]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^307]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^308]: Sydney Lewis, “Same Model, Different Harness: Different Coding-Agent Results,” arXiv:2608.26218, August 26, 2026. https://arxiv.org/abs/2608.26218

[^309]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^310]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^311]: “Harness Updating Is Not Harness Benefit,” arXiv:2605.30621 (2026). https://arxiv.org/abs/2605.30621

[^312]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^313]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^314]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^315]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^316]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^317]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^318]: Dimitrios Stamatios Bouras, Yihan Dai, and Sergey Mechtaev, “Authority Is Not a String: A Capability-Scoped Harness for Prompt-Injection-Resistant Coding Agents,” arXiv:2609.08371, September 8, 2026. https://arxiv.org/abs/2609.08371

[^319]: “Procedural Graphs: Self-Evolving Execution Structures for LLM Agents,” arXiv:2609.09153, September 8, 2026. https://arxiv.org/abs/2609.09153

[^320]: Miyai et al., “Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection,” arXiv:2608.20169, August 20, 2026. https://arxiv.org/abs/2608.20169

[^321]: “Verify Smarter, Evolve Further: Efficient Harness Evolution through Behavior-Aware Verification,” arXiv:2608.27311, August 27, 2026. https://arxiv.org/abs/2608.27311

[^322]: Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^323]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^324]: OpenAI, “An open-source spec for Codex orchestration: Symphony,” April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^325]: Thomas Gauvin, “Bringing more agent harnesses and frameworks to Cloudflare, starting with Flue,” Cloudflare, June 17, 2026. https://blog.cloudflare.com/agents-platform-flue-sdk/

[^326]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents — A Source-Code Study of Eleven Systems,” arXiv:2609.00006 (2026). https://arxiv.org/abs/2609.00006

[^327]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^328]: Chengrun Yang et al., “Large Language Models as Optimizers,” ICLR 2024; arXiv:2309.03409. https://arxiv.org/abs/2309.03409

[^329]: Chrisantha Fernando et al., “Promptbreeder: Self-Referential Self-Improvement via Prompt Evolution,” ICML 2024; arXiv:2309.16797. https://arxiv.org/abs/2309.16797

[^330]: Omar Khattab et al., “DSPy: Compiling Declarative Language Model Calls into Self-Improving Pipelines,” arXiv:2310.03714. https://arxiv.org/abs/2310.03714

[^331]: Mingchen Zhuge et al., “GPTSwarm: Language Agents as Optimizable Graphs,” ICML 2024. https://proceedings.mlr.press/v235/zhuge24a.html

[^332]: Mert Yuksekgonul et al., “TextGrad: Automatic ‘Differentiation’ via Text,” arXiv:2406.07496 (2024). https://arxiv.org/abs/2406.07496

[^333]: Shengran Hu, Cong Lu, and Jeff Clune, “Automated Design of Agentic Systems,” ICLR 2025; arXiv:2408.08435. https://proceedings.iclr.cc/paper_files/paper/2025/hash/36b7acf6f6010652b3f2a433774a66fe-Abstract-Conference.html

[^334]: Yu Shang et al., “AgentSquare: Automatic LLM Agent Search in Modular Design Space,” arXiv:2410.06153 (2024). https://arxiv.org/abs/2410.06153

[^335]: Jiayi Zhang et al., “AFlow: Automating Agentic Workflow Generation,” ICLR 2025. https://proceedings.iclr.cc/paper_files/paper/2025/hash/5492ecbce4439401798dcd2c90be94cd-Abstract-Conference.html

[^336]: Jenny Zhang et al., “Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents,” arXiv:2505.22954 (2025; ICLR 2026). https://arxiv.org/abs/2505.22954

[^337]: Lakshya A. Agrawal et al., “GEPA: Reflective Prompt Evolution Can Outperform Reinforcement Learning,” arXiv:2507.19457 (2025; ICLR 2026 Oral). https://arxiv.org/abs/2507.19457

[^338]: Ashish Vaswani et al., “Attention Is All You Need,” arXiv:1706.03762 (2017). https://arxiv.org/abs/1706.03762

[^339]: Tom B. Brown et al., “Language Models are Few-Shot Learners,” arXiv:2005.14165 (2020). https://arxiv.org/abs/2005.14165

[^340]: Patrick Lewis et al., “Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks,” arXiv:2005.11401 (2020). https://arxiv.org/abs/2005.11401

[^341]: Shunyu Yao et al., “ReAct: Synergizing Reasoning and Acting in Language Models,” arXiv:2210.03629 (2022/2023). https://arxiv.org/abs/2210.03629

[^342]: Luyu Gao et al., “PAL: Program-aided Language Models,” ICML 2023; arXiv:2211.10435. https://arxiv.org/abs/2211.10435

[^343]: John Yang et al., “SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering,” NeurIPS 2024; arXiv:2405.15793. https://arxiv.org/abs/2405.15793

[^344]: Erik Schluntz and Barry Zhang, “Building effective agents,” Anthropic, December 19, 2024. https://www.anthropic.com/engineering/building-effective-agents

[^345]: Michael Bolin, “Unrolling the Codex agent loop,” OpenAI, January 23, 2026. https://openai.com/index/unrolling-the-codex-agent-loop/

[^346]: OpenAI, “Harness engineering: leveraging Codex in an agent-first world,” February 11, 2026. https://openai.com/index/harness-engineering/

[^347]: Alex Kotliarskyi, Victor Zhu, and Zach Brock, “An open-source spec for Codex orchestration: Symphony,” OpenAI, April 27, 2026. https://openai.com/index/open-source-codex-orchestration-symphony/

[^348]: Anthropic Applied AI team, “Effective context engineering for AI agents,” September 29, 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^349]: Ken Aizawa et al., “Writing effective tools for agents - with agents,” Anthropic, September 11, 2025. https://www.anthropic.com/engineering/writing-tools-for-agents

[^350]: Anthropic, “Equipping agents for the real world with Agent Skills,” October 16, 2025; updated December 18, 2025. https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills

[^351]: Anthropic, “Effective harnesses for long-running agents,” November 26, 2025. https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

[^352]: Anthropic, “Scaling Managed Agents: Decoupling the brain from the hands,” April 8, 2026. https://www.anthropic.com/engineering/managed-agents

[^353]: Max McGuinness et al., “How we contain Claude across products,” Anthropic, May 25, 2026. https://www.anthropic.com/engineering/how-we-contain-claude

[^354]: Jediah Katz, “Dynamic context discovery,” Cursor, January 6, 2026. https://cursor.com/blog/dynamic-context-discovery

[^355]: Stefan Heule and Jediah Katz, “Continually improving our agent harness,” Cursor, April 30, 2026. https://cursor.com/blog/continually-improving-agent-harness

[^356]: Josh Ma, “What we’ve learned building cloud agents,” Cursor, June 2, 2026. https://cursor.com/blog/cloud-agent-lessons

[^357]: Cursor, “Cloud Agents and Cursor Harness Improvements,” August 19, 2026. https://cursor.com/changelog/08-19-26

[^358]: Vivek Trivedy, “The Anatomy of an Agent Harness,” LangChain, March 10, 2026. https://www.langchain.com/blog/the-anatomy-of-an-agent-harness

[^359]: Yilun Yao et al., “Harness-Bench: Measuring Harness Effects across Models in Realistic Agent Workflows,” arXiv:2605.27922, May 27, 2026. https://arxiv.org/abs/2605.27922

[^360]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^361]: Wenbo Pan et al., “Evolving Agents in the Dark: Retrospective Harness Optimization via Self-Preference,” arXiv:2606.05922; accepted to Findings of EMNLP 2026. https://arxiv.org/abs/2606.05922

[^362]: Yue Huang et al., “MemoHarness: Agent Harnesses That Learn from Experience,” arXiv:2607.14159, July 14, 2026. https://arxiv.org/abs/2607.14159

[^363]: Lisheng Huang et al., “Evo-Bench: Can Language Models Improve Agent Harness?” arXiv:2608.09096, August 10, 2026. https://arxiv.org/abs/2608.09096

[^364]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^365]: Guibin Zhang et al., “JIT-Agent: Scaling Harness Intelligence via Just-in-Time Harness Evolution,” arXiv:2608.25593, August 26, 2026. https://arxiv.org/abs/2608.25593

[^366]: Elias Lumer et al., “Recursive Agent Harnesses,” arXiv:2606.13643, June 11, 2026. https://arxiv.org/abs/2606.13643

[^367]: Seth Karten et al., “Prime Agent: A Self-Improving RLM Harness,” arXiv:2608.23552, August 24, 2026. https://arxiv.org/abs/2608.23552

[^368]: Junjia Qi et al., “LLM-as-Code: Agentic Programming for Agent Harness,” arXiv:2606.15874, June 2026. https://arxiv.org/abs/2606.15874

[^369]: Saransh Dhage, “Harness Engineering for Predictable Agentic Systems: An Empirical Study of Deterministic Execution Constraints,” arXiv:2608.26197, August 25, 2026. https://arxiv.org/abs/2608.26197

[^370]: Hanzhang Jia et al., “Logos: An Agent Harness on a Cross-Process Bus,” arXiv:2608.28553, August 28, 2026. https://arxiv.org/abs/2608.28553

[^371]: Haibo Jin et al., “Harness Engineering in LLM Tool Use via Agent-Native Reusable Tool Primitives,” arXiv:2609.01736, September 1, 2026. https://arxiv.org/abs/2609.01736

[^372]: Paul Barbaste et al., “Harness Engineering: Anatomy, Architecture, and Evolution of Coding Agents - A Source-Code Study of Eleven Systems,” arXiv:2609.00006, 2026. https://arxiv.org/abs/2609.00006

[^373]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^374]: OpenAI, “openai/symphony,” GitHub. https://github.com/openai/symphony

[^375]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^376]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^377]: SWE-agent project, “SWE-agent,” GitHub; project now recommends mini-SWE-agent for most new work. https://github.com/SWE-agent/SWE-agent

[^378]: Anomaly, “opencode,” GitHub. https://github.com/anomalyco/opencode

[^379]: Agentic AI Foundation, “goose,” GitHub. https://github.com/aaif-goose/goose

[^380]: Prime Intellect, “prime-agent,” GitHub. https://github.com/PrimeIntellect-ai/prime-agent

[^381]: Jingxuan Wang et al., “Rethinking the Evaluation of Harness Evolution for Agents,” arXiv:2607.12227, rev. August 27, 2026. https://arxiv.org/abs/2607.12227

[^382]: Yuhao Wu et al., “HarnessDev: Can LLMs Create and Evolve Their Own Agent Harness?” arXiv:2609.01437, September 1, 2026. https://arxiv.org/abs/2609.01437

[^383]: “EVOHARNESSBENCH: Can Your Agents Keep Pace with an Evolving Harness?,” arXiv:2609.04280, September 3, 2026. https://arxiv.org/abs/2609.04280

[^384]: Shao et al., “Harness-R1: Learning to Edit Executable Runtime Harnesses from Agent Failure Trajectories,” arXiv:2608.02276, August 3, 2026. https://arxiv.org/abs/2608.02276

[^385]: Xiaotian Luo et al., “HarnessBank: Semantic Gene-Bank Search with Gated Verification for Agent-Harness Self-Evolution,” arXiv:2607.13683, July 15, 2026. https://arxiv.org/abs/2607.13683

[^386]: Miyai et al., “Task-CoEvolve: Efficient Harness Optimization via Adaptive Validation Task Selection,” arXiv:2608.20169, August 20, 2026. https://arxiv.org/abs/2608.20169

[^387]: “Verify Smarter, Evolve Further: Efficient Harness Evolution through Behavior-Aware Verification,” arXiv:2608.27311, August 27, 2026. https://arxiv.org/abs/2608.27311

[^388]: “Beyond Prompts: Measuring and Optimizing LLM Tool-Agent Harnesses” (PRISM), arXiv:2609.05736; accepted EMNLP 2026. https://arxiv.org/abs/2609.05736

[^389]: “Co-Evolving Harnesses and Models: On-Policy Correction Helps Weaker Models Catch Up Where Imitation Fails,” arXiv:2609.09134, September 8, 2026. https://arxiv.org/abs/2609.09134

[^390]: Edoardo Debenedetti et al., “AgentDojo: A Dynamic Environment to Evaluate Prompt Injection Attacks and Defenses for LLM Agents,” arXiv:2406.13352; NeurIPS 2024. https://arxiv.org/abs/2406.13352

[^391]: Edoardo Debenedetti et al., “Defeating Prompt Injections by Design,” arXiv:2503.18813 (rev. June 2025). https://arxiv.org/abs/2503.18813

[^392]: Manuel Costa et al., “Securing AI Agents with Information-Flow Control,” arXiv:2505.23643 (2025), Microsoft Research. https://www.microsoft.com/en-us/research/publication/securing-ai-agents-with-information-flow-control/

[^393]: Zichuan Li et al., “What’s in Your Agent’s Context? Context Privilege Escalation Attacks against AI Agent Harness,” arXiv:2609.01222, September 1, 2026. https://arxiv.org/abs/2609.01222

[^394]: Dimitrios Stamatios Bouras, Yihan Dai, and Sergey Mechtaev, “Authority Is Not a String: A Capability-Scoped Harness for Prompt-Injection-Resistant Coding Agents,” arXiv:2609.08371, September 8, 2026. https://arxiv.org/abs/2609.08371

[^395]: Charles Packer et al., “MemGPT: Towards LLMs as Operating Systems,” arXiv:2310.08560 (2023). https://arxiv.org/abs/2310.08560

[^396]: Joon Sung Park et al., “Generative Agents: Interactive Simulacra of Human Behavior,” UIST 2023. https://arxiv.org/abs/2304.03442

[^397]: Michael Wu and Arquimedes Canedo, “Invalidation Contracts for Cross-Episode Agent Memory,” arXiv:2609.00243, August 31, 2026. https://arxiv.org/abs/2609.00243

[^398]: Yi Ting Shen, Kentaroh Toyoda, and Alex Leung, “Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems,” arXiv:2609.08258, September 8, 2026. https://arxiv.org/abs/2609.08258

[^399]: SWE-agent project, “mini-SWE-agent,” GitHub. https://github.com/SWE-agent/mini-swe-agent

[^400]: Aider AI, “Aider,” GitHub. https://github.com/Aider-AI/aider

[^401]: Mario Zechner, “pi-mono,” GitHub. https://github.com/badlogic/pi-mono

[^402]: OpenAI, “openai/codex,” GitHub. https://github.com/openai/codex

[^403]: OpenHands, “software-agent-sdk,” GitHub. https://github.com/OpenHands/software-agent-sdk

[^404]: Browser Use, “browser-use/browser-use,” GitHub. https://github.com/browser-use/browser-use

[^405]: AGI Research, “AIOS,” GitHub. https://github.com/agiresearch/AIOS

[^406]: Omnigent, open-source meta-harness. https://github.com/omnigent-ai/omnigent

[^407]: Darwin Gödel Machine reference implementation. https://github.com/jennyzzt/dgm

[^408]: Jiahang Lin et al., “Agentic Harness Engineering: Observability-Driven Automatic Evolution of Coding-Agent Harnesses,” arXiv:2604.25850, revised May 18, 2026. https://arxiv.org/abs/2604.25850

[^409]: Xiang Lisa Li et al., “Meta-Harness: End-to-End Optimization of Model Harnesses,” arXiv:2603.28052, March 2026. https://arxiv.org/abs/2603.28052
