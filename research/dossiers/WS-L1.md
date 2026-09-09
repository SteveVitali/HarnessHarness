# WS-L1 — Language / ecosystem selection spike (Phase 0: framing only)

**Track:** L · **Phase:** 0 (framing) → 1 (ratification) · **Tier(s) fed:** program (outside C-tiers); ADR at end of Phase 1; all implementation; Spec §11 · **Status:** done (framing, synthesized 2026-09-09); ratification pending end Phase 1
**Owner agent:** WS-L1 research subagent (Fable 5.1) · **Date:** 2026-09-09
**Feeds spec section(s):** §7.2 item 11 (deferred/decided items with rationale), §7.2 item 9 (build ladder — toolchain prerequisites), §7.2 item 7 (surfaces — only via K-track), the end-of-Phase-1 language ADR · **Scope register items:** R-2.12.3

> **Neutrality statement.** This dossier is the only artifact in the program permitted to name programming languages, and it names them only as *exemplars of ecosystem classes* in evidence tables. Nothing here is a recommendation for or against any language, runtime, package ecosystem, or framework. Every upstream contract (IR, compiler, composition, event store) must remain language-neutral; this dossier tells the Phase 1 ratification *what evidence to collect and how to score it*, not what to choose.

## 1. Scope & boundary

**Decides (Phase 0):** (i) the decision *criteria* and the evidence each needs; (ii) a weighted criteria matrix with weights left as ranges plus rationale; (iii) a neutral enumeration of candidate ecosystem classes with an evidence table each; (iv) the polyglot-split option treated as a first-class candidate, with its boundary-cost model; (v) the exact inputs the end-of-Phase-1 ratification needs from WS-A3 (IR), WS-A4 (compilation), WS-A5 (composition), WS-B1 (event store); (vi) a decision procedure a later agent can run mechanically; (vii) one proposed ADR carrying the criteria and procedure.

**Does not decide:** the language. It does not rank candidates, does not fill in weights, and does not run the procedure.

**Leaves to neighbours:** what the IR must express (WS-A3, OQ-002/OQ-006); compile targets (WS-A4); the code-vs-config boundary (WS-A5); event serialization and authority scope (WS-B1, OQ-003); durable-execution semantics (WS-B3); sandbox/VM handle model (WS-B5, WS-E5, WS-H4); protocol-edge placement (WS-E4); web dev-tooling (WS-K2); SDK boundary (WS-K4); plugin contracts (WS-L5); packaging/licensing (WS-L6, which is downstream of the language ADR). Where any of those workstreams would be forced into a language assumption to make progress, this dossier gives them the language-neutral question to answer instead (§6.4).

## 2. Key questions

Verbatim from doc 3 §5 (WS-L1 row) and the Phase 0 brief, then additions.

1. What are the decision criteria — research/eval alignment, protocol edges, durability, web tooling, performance, sandboxing, concurrency model, FFI to model-serving stacks, community/hiring, long-term maintenance — and what evidence does each criterion need?
2. What are the candidate ecosystems, enumerated neutrally?
3. What is the decision procedure that runs at the END of Phase 1 once the IR (A3), compilation model (A4), composition model (A5) and event store (B1) contracts are stable?
4. How are all upstream artifacts kept language-neutral?
5. (Added) Which mature harnesses use which ecosystem *for which layer* — core loop, surfaces, SDK, sandbox helper, eval — and through which boundary mechanism when they mix?
6. (Added) What does a polyglot split actually cost, measured from source, and what mitigations do precedents use?
7. (Added) Which criteria are *hard gates* (a candidate failing them is excluded regardless of weights) versus *weighted*?
8. (Added) What is the reversibility cost of the decision, and which Phase 2+ workstreams are blocked until it is ratified?

## 3. Sources consulted (4 streams — mark which were actually opened)

| stream | source (S-id or new) | opened? | what it contributed |
|---|---|---|---|
| Academic | S-074 Barbaste et al., 11-system source-code study (abs + HTML v1) | yes | per-system language, LoC, loop model, sandbox, MCP/ACP/skills adoption; "no general framework; hand-rolled async loops"; language-distribution statement (provisional) |
| Academic | S-030 AIOS paper (abs) | yes | kernel/SDK boundary; 2.1× serving claim (provisional, not used) |
| Academic | S-073 Logos (abs) | yes | process-per-plugin bus; transcript owned by no process; 200-task fault-injection result (provisional mechanism) |
| Academic | S-013 DSPy (code only, see below) | code | research-stack alignment evidence |
| OSS audit | S-107 openai/codex | yes — vertical slice | `codex-cli/bin/codex.js`; `sdk/typescript/src/exec.ts`; `sdk/python-runtime/README.md`; `codex-rs/app-server-protocol/src/export.rs`; `codex-rs/app-server-transport/src/lib.rs`; `codex-rs/exec-server/README.md`; `codex-rs/linux-sandbox/src/landlock.rs`; `codex-rs/code-mode-runtime/Cargo.toml`; `codex-rs/protocol/Cargo.toml` |
| OSS audit | S-112 aaif-goose/goose | yes | `Cargo.toml` (rmcp 3.2.0, agent-client-protocol 2.0.0, axum, sqlx, candle, tokio); `crates/goose-sdk/{README.md,Cargo.toml}` (uniffi → Python/Kotlin); `vendor/v8/Cargo.toml`; `ui/desktop/src/gooseServe.ts`; `ui/goose-acp/package.json` |
| OSS audit | S-113 badlogic/pi-mono | yes | `packages/agent/src/agent-loop.ts`; `packages/protocol/README.md` (CBOR framed protocol v8); `packages/session-backends/sqlite-node`; `packages/coding-agent/package.json` (bun --compile) |
| OSS audit | S-111 anomalyco/opencode | yes | `package.json` (bun workspace); `packages/opencode/src/acp/agent.ts`; `packages/opencode/src/storage/storage.ts`; `packages/sdk/js/package.json` (OpenAPI codegen); `packages/desktop/package.json` (Electron); `packages/containers/rust/Dockerfile` |
| OSS audit | S-110 OpenHands/software-agent-sdk | yes | `openhands-sdk/openhands/sdk/event/base.py`; `openhands-sdk/pyproject.toml`; `openhands-workspace/openhands/workspace/docker/workspace.py`; `openhands-agent-server/openhands/agent_server/conversation_lease.py`; `clients/typescript/package.json` |
| OSS audit | S-031 agiresearch/AIOS | yes | `aios/scheduler/fifo_scheduler.py`; `aios-rs/README.md` (stalled rewrite scaffold) |
| OSS audit | S-118 omnigent-ai/omnigent | yes | `omnigent/acp_cli_harnesses.py`; `omnigent/harnesses/`; `sdks/` |
| OSS audit | S-116 PrimeIntellect-ai/prime-agent | yes | `prime-agent-runtime/src/rlm/`; `packages/coding-agent/package.json` |
| OSS audit | S-109 anthropics/claude-agent-sdk-python | yes | `src/claude_agent_sdk/_internal/transport/subprocess_cli.py` |
| OSS audit | S-117 HKUDS/OpenHarness | yes | `src/openharness/sandbox/{docker_backend.py,adapter.py}` |
| OSS audit | S-012 AutoGen (code) | yes | `protos/agent_worker.proto` (cross-language runtime) |
| OSS audit | S-013 DSPy (code) | yes | `dspy/adapters/base.py`; `dspy/clients/lm.py` |
| OSS audit | new S-143 langchain-ai/langgraph | yes | `libs/checkpoint/langgraph/checkpoint/serde/base.py` |
| Protocol | S-035 ACP repo + docs | yes | `schema-generator/src/main.rs`; `docs/libraries/*.mdx`; transports page |
| Protocol | new S-155 MCP SDK tier page; new S-153 modelcontextprotocol spec repo | yes | `schema/2026-07-28/schema.ts`; SDK tiers |
| Protocol | new S-156 A2A spec 1.0.0 + SDK page | yes | bindings, SDK languages |
| Protocol | S-056 Cloudflare Agents harness docs | yes | runtime/harness split; fibers, state, scheduling |
| Production | S-047 Managed Agents | yes | stateless harness; `execute(name,input)→string`; append-only session |
| Production | S-053 How we contain Claude | yes | gVisor+seccomp, Seatbelt, bubblewrap, hypervisors, vsock, egress proxy; 93%/84% telemetry |
| Production | S-055 Codex App Server post | **no** (HTTP 403) | replaced by reading the app-server crates directly |
| Production/eval | new S-158/S-159/S-160/S-161/S-132/S-162 Temporal, Restate, DBOS, Inspect AI, Harbor, SWE-bench harness | yes | durable-execution SDK coverage; eval-tooling ecosystem |
| Community | new S-163 Octoverse 2025; S-164 SO survey 2025; S-165 WASI 0.3 | yes | contributor/usage/admiration figures; Wasm component-model status |

Streams triangulated: 4/4 (academic, source audit, protocol, production). The source audit is the load-bearing stream; academic performance claims are not used.

## 4. Findings

### 4a. Mechanism evidence (a failure mode exists / an architecture is implementable)

**M1 — No mature harness uses a general-purpose agent framework for its core loop; the loops are hand-rolled async.** S-074 (tier C, mechanism, `provisional` as a 2026 single-team study but *auditable*: the corpus is open source): "Across roughly four million lines of Python, TypeScript, and Rust, no agent runtime imports a general-purpose agentic framework … the field runs on hand-rolled async loops and deterministic retrieval." Our own audit corroborates for every repo opened: Codex is a Tokio async state machine (`codex-rs/core`), Pi is a functional loop over an `EventStream` (`packages/agent/src/agent-loop.ts`), OpenHands is a pydantic event-sourced conversation (`sdk/event/base.py`), goose is a tokio/axum runtime. *Consequence for WS-L1:* "framework availability" is **not** a criterion; what matters is the ecosystem's async/concurrency primitives, its serialization/schema story, and its process-boundary tooling.

**M2 — Core language is a minority axis; mixing is the norm.** Per-system extraction from S-074 HTML v1 (counts provisional; the paper's own summary line says "Python dominates open-source (7/11)" while its per-system table as extracted yields 5 TypeScript / 5 Python / 1 Rust core — the direction, not the exact split, is what we rely on): the single compiled-language core in the corpus is Codex (~1.1M LoC). Every audited system with a product surface is *already polyglot at a boundary*: Codex (Rust core → TS/Python SDKs via spawned binary + JSON-RPC), goose (Rust core → Electron/TS desktop over HTTP; uniffi bindings to Python/Kotlin; embedded V8), Pi (TS core → CBOR wire protocol; sqlite backend; bun-compiled binary), OpenHands (Python SDK/server → generated TS client), opencode (TS core → OpenAPI-generated SDK; Rust toolchain in build containers), prime-agent (TS agent → Python REPL kernel), Claude Agent SDK (Python → TS CLI over stream-json stdio), Omnigent (Python host → any-language harness over ACP stdio), AutoGen (Python ↔ .NET via protobuf/gRPC). *Consequence:* the polyglot split is not exotic; the question is which boundary mechanism and where.

**M3 — Six boundary mechanisms recur, each with a known cost profile.**
- (a) *Subprocess + line/JSON-RPC over stdio.* Codex `sdk/typescript/src/exec.ts:196` (`spawn(executablePath, ["exec","--experimental-json"])`); Claude SDK `subprocess_cli.py:566` (`--output-format stream-json`); ACP stdio (`agent-client-protocol/docs` transports: "The client launches the agent as a subprocess…"); Omnigent `acp_cli_harnesses.py` ("deny-by-default" spawn env). Cost: process lifecycle, no shared memory, serialization per message; benefit: strongest fault and privilege isolation, any-language peers.
- (b) *Localhost HTTP/WebSocket with OpenAPI-generated clients.* OpenHands `clients/typescript` and opencode `packages/sdk/js` both use `@hey-api/openapi-ts`; goose desktop spawns `goosed` and talks HTTP (`ui/desktop/src/gooseServe.ts`); Codex `app-server-transport` exposes `start_stdio_connection` and `start_websocket_acceptor`. Cost: duplicated types unless generated; benefit: multi-client, remote-capable.
- (c) *Schema-first codegen from one source of truth.* Codex `app-server-protocol/src/export.rs` generates TS (ts-rs) and JSON Schema (schemars) with `GENERATED_TS_HEADER = "// GENERATED CODE! DO NOT MODIFY BY HAND!"`, versioned `v1`/`v2` directories, and an explicit `JSON_V1_ALLOWLIST`; ACP `schema-generator/src/main.rs` generates JSON Schema from Rust types under `v1`/`v2` features; MCP does the inverse — `schema/2026-07-28/schema.ts` is the TypeScript source of truth and `tsx scripts/generate-schemas.ts` emits JSON. Cost: a codegen pipeline and CI check (`check:schema:json` in MCP; `schema_fixtures_tests.rs` in Codex); benefit: type parity across languages without hand-maintained duplicates.
- (d) *In-process FFI bindings.* goose `goose-sdk` (`uniffi` feature → Python/Kotlin namespaces `goose` / `io.github.aaif_goose`); AIOS `aios-rs/README.md` plans "pyo3 / JSON-RPC bridge for hybrid operation" but records "No async runtime yet", "No FFI bridge to Python yet". Cost: ABI/lifetime management, build matrix; benefit: zero-copy, shared process.
- (e) *Embedded engine for model-authored code.* Codex `code-mode-runtime/Cargo.toml` (`v8` with `v8_enable_sandbox`); goose `vendor/v8` (v8-goose 145.0.2). S-074 notes Codex "tool calls executed as V8 code". Cost: large native dependency; benefit: sandboxed execution of model-generated code inside the core.
- (f) *Protobuf/gRPC runtime protocol.* AutoGen `protos/agent_worker.proto` (`RpcRequest`, `Payload{data_type,data_content_type,bytes}`), A2A 1.0.0 gRPC binding. Cost: schema toolchain; benefit: language-neutral typed transport.

**M4 — Sandboxing primitives are OS/hypervisor-level and reachable from any ecosystem via (a) or a helper binary; in-process syscall filtering is the exception.** S-053 names gVisor+seccomp, Seatbelt, bubblewrap, Apple Virtualization/HCS, vsock, egress proxies; the same post reports approval-fatigue telemetry (93% of prompts approved; 84% fewer prompts after an OS-level sandbox) — mechanism evidence that the sandbox is a *runtime* concern the language must be able to drive. Codex is the one core that applies primitives *in-thread* (`linux-sandbox/src/landlock.rs`: "Apply sandbox policies inside this thread so only the child inherits them"; `PR_SET_NO_NEW_PRIVS` + seccomp; "Filesystem restrictions are enforced by bubblewrap"; Landlock kept "as legacy/backup"), and it *also* runs a separate `exec-server` process (JSON-RPC, `ws://` default, remote mode via a Noise relay). Claude Code (TS) reaches Seatbelt/bubblewrap without in-process bindings. S-074: OpenCode "None (policy-based syntax-aware permissioning)", Pi "None (deliberate)". *Consequence:* the sandbox criterion should score the ecosystem's ability to *drive* OS primitives and *host a helper binary*, not require in-process syscall bindings — unless WS-B5/WS-E5 decide the sandbox helper is part of the kernel TCB (then it is a hard gate for that component only; see §6.4 Q-L1-09).

**M5 — Durable-execution libraries exist in every candidate ecosystem class, but each imposes a determinism/serialization contract the event store must satisfy.** Temporal ships Go, Java, .NET, PHP, Python, Ruby, TypeScript and Rust (1.0.0) SDKs; Restate (server in a compiled language) ships TS, JVM, Python, Go, Rust SDKs; DBOS ships Python, TS, Go, Java on a Postgres journal; Cloudflare Agents exposes "fibers" for recoverable execution inside its own runtime (S-056). LangGraph's checkpoint layer defines `SerializerProtocol.dumps_typed/loads_typed` (`libs/checkpoint/.../serde/base.py`) — i.e. the durable layer *owns* a typed serialization contract. OpenHands implements leases with a file lock and a 45 s TTL (`conversation_lease.py`), Managed Agents with `emitEvent/wake/getSession` (S-047). *Consequence:* durability is available everywhere; the differentiator is whether WS-B1/B3 want a *library-imposed replay contract* or an *owned event-sourced runtime* (Q-L1-05/06).

**M6 — The eval/benchmark toolchain is concentrated in one ecosystem class, but bridges exist.** Inspect AI, Harbor (the official Terminal-Bench 2.0 harness) and the SWE-bench harness are all implemented in the dynamically-typed research ecosystem; DSPy (`adapters/base.py`, `clients/lm.py` via litellm) likewise. But Inspect "can run arbitrary external agents like Claude Code, Codex CLI, and Gemini CLI" and Harbor integrates agents via `--agent` adapters over containers — the same subprocess/container boundary as M3(a). *Consequence:* eval alignment is a real criterion (the laboratory J-track and I-track will touch these tools constantly) but is *partially* dischargeable by a boundary; its weight range must reflect that.

**M7 — Protocol SDK coverage is now broad enough that no candidate class fails the protocol criterion outright; tiers differ.** MCP (protocol 2026-07-28): Tier 1 SDKs for TypeScript, Python, C#, Go, Rust; Tier 2 Java, Ruby; Tier 3 Swift, PHP, Kotlin. A2A 1.0.0: SDKs for Python, Go, Java, JavaScript, .NET, Rust; bindings JSON-RPC 2.0, gRPC, HTTP+JSON; SSE streaming and push notifications. ACP: stdio JSON-RPC (Streamable HTTP is a draft); official Rust, TypeScript, Python, Kotlin, Java libraries; community Go (six libraries), Swift, .NET, C++, Dart, Crystal, Elixir. Production usage in our audit: goose depends on `rmcp 3.2.0` and `agent-client-protocol 2.0.0`; OpenHands on `agent-client-protocol>=0.10.1,<0.11.0` with a pinned comment ("0.11.0 reordered prompt() args, broke the client") and `fastmcp>=3.2.0`; opencode on `@agentclientprotocol/sdk`. *Consequence:* protocol maturity is a weighted criterion with tier-based scoring, plus a hard gate: an ecosystem with no Tier 1/official MCP SDK and no official ACP library is excluded for the kernel.

**M8 — Community signals point in different directions per axis, so "community" must be decomposed.** Octoverse 2025: TypeScript became #1 by contributor count (August 2025; +66.63% YoY), Python +48.78%, JavaScript +24.79%; Python "maintains dominance in AI and data science". Stack Overflow 2025: Rust most admired for the tenth year (72%); JavaScript most used (66%), Python 57.9%. These are population statistics, not harness-specific; they inform hiring/contributor breadth and *agent-codability* (typed languages "can make agent-assisted coding more reliable" is Octoverse's framing, not an independent measurement — treat as a hypothesis for Q-L1-11).

**M9 — Wasm component model is a plausible plugin/sandbox substrate but not yet at 1.0.** WASI 0.3 (2026-06-11) moved async (`async func`, `stream<T>`, `future<T>`) into the component model; WASI 1.0 is targeted for late 2026/early 2027. No audited harness uses it for plugins today (the corpus embeds V8 or spawns processes). *Consequence:* Wasm-hosting is a *forward-looking* sub-criterion under sandboxing/plugins with low current weight and an explicit revalidation date.

### 4b. Performance evidence (benchmark lifts) — each tagged `provisional` if 2026 single-team/preprint

- `provisional` S-074: "Loop sophistication does not predict benchmark performance. Mini-SWE-Agent's minimal linear loop achieves reported results in the same range as OpenHands's event-sourced conversation engine." Used only to *lower* the weight of raw runtime performance for task success; not load-bearing.
- `provisional` S-073 Logos: cross-process bus "kept every session alive … succeeded on 120 tasks" versus single-process reference "scored 1.5 percent" under fault injection. Mechanism (process isolation aids recovery) is admissible; the numbers are not compared at matched budget and are not used to weight any criterion.
- `provisional` S-030 AIOS "up to 2.1x faster execution" — not used.
- S-053 (production, tier B): 93% approval rate; 84% reduction in prompts after OS-level sandbox; auto mode blocks ~83% of overeager behaviors — used as mechanism evidence that sandbox integration quality is a first-order runtime property.
- No performance claim in this dossier is load-bearing for the language decision. Performance enters only as criterion C7 with a *measured* spike (see §6.3, step 4).

### 4c. Source-code precedents (repo, path, what it does)

| repo | path | what it does (relevant to WS-L1) |
|---|---|---|
| openai/codex | `codex-cli/bin/codex.js` | npm entry resolves a platform-native binary package (`@openai/codex-<target-triple>`) and spawns it — compiled core distributed through a scripting-ecosystem package manager |
| openai/codex | `sdk/typescript/src/exec.ts` L92, L196 | SDK is a thin client: `["exec","--experimental-json"]` over a spawned child; events parsed from JSONL |
| openai/codex | `sdk/python-runtime/README.md` | "wheel-only" platform runtime package pinning "an exact Codex CLI version" — version-pinning discipline across the boundary |
| openai/codex | `codex-rs/app-server-protocol/src/export.rs`; `schema/{json,typescript,precomputed}` | single-source-of-truth types → generated TS + JSON Schema; `v1`/`v2` namespaces; allowlists for experimental methods; fixture tests |
| openai/codex | `codex-rs/app-server-transport/src/lib.rs` | stdio and WebSocket transports for the same JSON-RPC surface; daemon recovery/shutdown modules |
| openai/codex | `codex-rs/exec-server/README.md`, `exec-server-protocol/src/lib.rs` | process/pty control as a *separate JSON-RPC server* (`ws://` default; remote mode via environment registry + Noise relay) — execution plane split from control plane |
| openai/codex | `codex-rs/linux-sandbox/src/landlock.rs` | in-thread `no_new_privs` + seccomp; bubblewrap for filesystem; Landlock legacy; Windows restricted-token/elevated backends in `core/src/sandboxing/mod.rs` |
| openai/codex | `codex-rs/code-mode-runtime/Cargo.toml`, `src/v8_init.rs` | embedded V8 (`v8_enable_sandbox`) to execute model-authored tool code in-core |
| aaif-goose/goose | `Cargo.toml` | workspace deps: `rmcp 3.2.0`, `agent-client-protocol 2.0.0`, `axum`, `sqlx`, `tokio-cron-scheduler`, optional `candle-*` (local inference FFI-free in same language) |
| aaif-goose/goose | `crates/goose-sdk/README.md`, `Cargo.toml` | `uniffi` feature compiles the SDK to Python/Kotlin bindings — in-process FFI boundary precedent |
| aaif-goose/goose | `vendor/v8/Cargo.toml` | embedded V8 (`v8-goose 145.0.2`) |
| aaif-goose/goose | `ui/desktop/src/gooseServe.ts` | Electron desktop spawns the `goosed` server and talks HTTP — surface in a different ecosystem than the core |
| badlogic/pi-mono | `packages/protocol/README.md` | "runtime-neutral routed envelopes, CBOR encoding, and byte-stream framing"; 4-byte length + one CBOR item; "Session and AgentHarness remain process-local" — wire protocol designed for replaceable server processes |
| badlogic/pi-mono | `packages/agent/src/agent-loop.ts` | functional loop returning an `EventStream<AgentEvent, AgentMessage[]>` |
| badlogic/pi-mono | `packages/session-backends/sqlite-node`; `packages/coding-agent/package.json` | sqlite session backend; `bun build --compile` single binary |
| anomalyco/opencode | `packages/opencode/src/acp/agent.ts` | ACP agent implemented on `@agentclientprotocol/sdk` with an effect-system library for typed errors/concurrency |
| anomalyco/opencode | `packages/sdk/js/package.json`; `packages/sdk/openapi.json` | client SDK generated from OpenAPI (`@hey-api/openapi-ts 0.90.10`) — client/server split |
| anomalyco/opencode | `packages/opencode/src/storage/storage.ts` | schema-validated file storage with migrations; `packages/effect-drizzle-sqlite` |
| anomalyco/opencode | `packages/containers/rust/Dockerfile` | second toolchain present in build images even for a single-language core |
| OpenHands/software-agent-sdk | `openhands-sdk/openhands/sdk/event/base.py` | frozen, `extra="forbid"` events with `id`, `timestamp`, `source`, `parent_id` (tree of branches) — schema-validated event model |
| OpenHands/software-agent-sdk | `openhands-workspace/.../docker/workspace.py` | container runs a pre-built agent-server image; workspace operations through the container's HTTP API — execution plane behind HTTP |
| OpenHands/software-agent-sdk | `openhands-agent-server/.../conversation_lease.py` | `FileLock` lease, `DEFAULT_LEASE_TTL_SECONDS = 45.0`, generation counter, takeover — durable-ownership primitive hand-rolled |
| OpenHands/software-agent-sdk | `clients/typescript/package.json` | `@hey-api/openapi-ts 0.99.0` generated client — same codegen family as opencode |
| OpenHands/software-agent-sdk | `openhands-sdk/pyproject.toml` | pinned `agent-client-protocol>=0.10.1,<0.11.0` with breakage note; `fastmcp`; `litellm`; `pydantic` |
| agiresearch/AIOS | `aios/scheduler/fifo_scheduler.py`; `aios-rs/README.md` | thread+queue scheduler; a compiled-language rewrite scaffold that stalled before async and FFI — negative precedent for casual dual-implementation |
| omnigent-ai/omnigent | `omnigent/acp_cli_harnesses.py`; `omnigent/harnesses/*_native` | hosts eleven vendor harnesses "backed by a vendor CLI that speaks the Agent Client Protocol on stdio"; spawn env "deny-by-default"; host language irrelevant to hosted harness language |
| PrimeIntellect-ai/prime-agent | `prime-agent-runtime/src/rlm/{repl.py,harness.py,mcp.py}`; `packages/coding-agent/package.json` | agent in one ecosystem, persistent REPL kernel in another, shipped together ("copy-assets" copies the runtime into `dist/`) |
| anthropics/claude-agent-sdk-python | `_internal/transport/subprocess_cli.py` L566, L783 | SDK spawns the CLI with `--output-format stream-json` / `--input-format stream-json` — JSONL stdio boundary |
| microsoft/autogen | `protos/agent_worker.proto` | `RpcRequest/RpcResponse/Payload` + CloudEvents — protobuf cross-language agent runtime |
| langchain-ai/langgraph | `libs/checkpoint/.../serde/base.py` | `SerializerProtocol.dumps_typed(obj)->(str, bytes)` — checkpoint layer owns typed serialization |
| stanfordnlp/dspy | `dspy/adapters/base.py`; `dspy/clients/lm.py` | `Adapter.format/parse` per model; `litellm` gateway — research-stack idioms |
| agentclientprotocol/agent-client-protocol | `schema-generator/src/main.rs`; `docs/libraries/*.mdx` | JSON Schema generated from typed source with `v1`/`v2` feature gates; official library list |
| modelcontextprotocol/modelcontextprotocol | `schema/2026-07-28/schema.ts`; `package.json` scripts | TS source of truth; `generate:schema:json`; `check:schema:*` CI |

## 5. Disconfirming evidence & negative results

For each element of the framing, the strongest case against it.

**Against treating language choice as high-stakes at all.** S-074 (`provisional`) finds loop sophistication does not predict benchmark outcomes, and the corpus splits roughly evenly between two managed-runtime ecosystems with one compiled core; no evidence links ecosystem to harness quality. If true, the decision is a *cost* decision (toolchain, contributors, boundaries), not a *capability* decision, and the weights should concentrate on C1, C10, C11, C12 rather than C7. The procedure in §6.3 handles this by requiring a sensitivity analysis across the weight ranges; if the winner is invariant, the stakes were low and the decision is cheap to make.

**Against weighting eval alignment (C1) heavily.** Inspect and Harbor run external agents through the same container/stdio boundary that hosted-external participants (ADR-0001) use anyway; SWE-bench consumes a patch + instance id. A kernel in any ecosystem can be evaluated by these tools. Counter-counter: the *laboratory* (J-track) is not an external agent — it wants to call scorers, judges and dataset loaders in-process for factorial sweeps; a boundary there is per-sample, not per-run. Weight range kept wide (10–20%).

**Against the polyglot split as a first-class candidate.** Every clean polyglot precedent is a *product* with a large team (OpenAI, Block, Anthropic); the one research project that attempted a compiled rewrite (AIOS `aios-rs`) stalled at "No async runtime yet … No FFI bridge to Python yet". OpenHands pins ACP to a minor version because "0.11.0 reordered prompt() args, broke the client" — boundary breakage is real even *within* one ecosystem. A small team pays the boundary cost (C12) continuously. The procedure therefore scores C12 as a *penalty on the split* proportional to the number of boundaries crossed on the hot path, and requires the split candidate to name its boundary mechanism from M3 explicitly.

**Against in-process sandbox primitives as a criterion.** Claude Code reaches Seatbelt/bubblewrap without in-process bindings; Codex's own comment says filesystem restriction is delegated to bubblewrap and Landlock is "legacy/backup". The sandbox may legitimately be a *helper binary* in whatever language is best for it, independent of the kernel language — which is exactly the "kernel/helper" split. C4 scoring is therefore split into "can drive OS primitives" (any ecosystem passes) and "can host the helper in-process" (optional, only if WS-B5/E5 say so).

**Against durable-execution libraries as a plus.** Temporal/Restate/DBOS impose determinism and replay contracts on workflow code; a harness whose control loop is *itself* the event-sourced authority (doc 2 §11 "event sourcing selectively, not dogmatically"; Managed Agents' stateless harness over `getEvents()`) may find a library redundant or conflicting. If WS-B1/B3 choose an owned event store, C3 collapses to "has good async + serialization + an embedded/embeddable durable KV/SQL", which every candidate has (sqlite appears in Pi, opencode, goose).

**Against community-size statistics.** Octoverse counts contributors, not maintainers; SO "admired" is preference, not usage. Neither measures the population that will write *harness kernels*. They are kept at 5–10% and require the ratification to supplement them with a *repo-local* measure (Q-L1-11: how many of the reference repos the team must read/port are in each ecosystem).

**Against deciding at end of Phase 1.** Phase 2 workstreams (K2 web tooling, B3 durability, E5 sandbox) may need a runtime assumption to be concrete, and the IR could still shift in Phase 2. Mitigation: the ADR is ratified with a *revalidation trigger* (§6.3 step 8) and the K-track is told which questions are language-independent (§6.4).

## 6. Recommendation for the spec

### 6.1 Criteria matrix (weights as ranges; rationale; evidence each needs)

Weights are **ranges**; the ratification picks a point in each range *before* scoring (blind to candidate scores) and then runs a sensitivity sweep over the hull. Ranges below sum to 78–139, so any point choice is normalised to 100. Hard gates (HG) exclude a candidate before scoring.

| id | criterion | weight range | hard gate? | rationale (evidence) | evidence the ratification must collect |
|---|---|---|---|---|---|
| C1 | Research/eval-tooling alignment | 10–20 | no | Inspect, Harbor, SWE-bench harness, DSPy concentrated in one ecosystem (M6); partly bridgeable (§5) | For each candidate: which of {Inspect, Harbor, SWE-bench, τ-bench, AgentDojo runners, DSPy/GEPA-style optimizers} are callable in-process vs only over a boundary; per-sample vs per-run boundary crossings for a J3 sweep |
| C2 | Protocol-edge SDK maturity (MCP client+server, A2A, ACP) | 8–12 | **HG:** no Tier-1/official MCP SDK *and* no official ACP library ⇒ excluded for kernel | M7; goose/OpenHands/opencode pin real SDKs; breakage note in OpenHands | MCP tier; ACP official vs community; A2A SDK present; date of last release; whether WS-E4 needs *server* as well as client role in-process |
| C3 | Durability / durable-execution fit | 8–14 | no | M5; contract depends on WS-B1/B3 (Q-L1-05/06) | If B3 chooses a library: SDK availability + determinism constraints vs the control loop; if owned: embedded SQL/KV, append-only log libs, serialization determinism (Q-L1-03) |
| C4 | Sandboxing & isolation | 8–14 | **HG (conditional):** if WS-B5/E5 require the sandbox helper in the kernel process, candidates without in-process seccomp/Landlock/Seatbelt/job-object bindings are excluded | M4; S-053 primitives; Codex in-thread vs helper | (a) can spawn/drive bwrap, Seatbelt, gVisor, Firecracker/VM handles, egress proxy — yes/no per platform; (b) in-process syscall-filter bindings; (c) Wasm component hosting maturity (M9, revalidate 2027-Q1) |
| C5 | Concurrency & streaming model | 6–12 | no | M1: loops are hand-rolled async; need structured concurrency, cancellation, backpressure, streaming, fan-out to subagents | Presence of structured concurrency/cancellation primitives; streaming parsers; fan-out cost per concurrent session (measured in spike, §6.3 step 4) |
| C6 | Type-system / IR expressiveness match | 8–15 | **HG (conditional):** if WS-A3 requires properties the ecosystem cannot check (Q-L1-01/02), exclude for the kernel | IR is typed/diffable/verifiable (doc 2 §11); schema-first codegen precedents (M3c) | Answers to Q-L1-01..04; ability to generate JSON Schema/TS/other bindings from the IR types; exhaustiveness checking; immutability |
| C7 | Performance & footprint | 3–8 | no | §4b: not predictive of task success (`provisional`); matters for lab fan-out and cold start | Spike: memory per idle session, startup, throughput of event append + materialized view at N concurrent sessions |
| C8 | FFI to model-serving/tokenizer stacks | 2–6 | no | goose uses same-language local inference; others use HTTP gateways (litellm) | Whether C1/C3 scenarios need in-process tokenizers/embeddings/local inference; else score "via HTTP" |
| C9 | Web dev-tooling affinity | 3–8 | no | K2 is a C2 item; every audited desktop/web surface is in the web-native ecosystem regardless of core (M2) | Whether K2 requires shared types with the kernel (codegen suffices, M3c) or shared runtime |
| C10 | Community, hiring, agent-codability | 5–10 | no | M8: contradictory population signals; repo-local measure needed | Octoverse/SO figures *plus* Q-L1-11 repo-local count; agent-codability spike (same ticket implemented by an agent in each candidate; measure defect rate) |
| C11 | Long-term maintenance & supply chain | 5–10 | no | single-binary distribution (Codex, Pi bun-compile), pinning discipline (Codex Python runtime), lockfiles, signing; doc 3 §9 RK-06 | Reproducible builds; SBOM/signing tooling; dependency-audit tooling; release cadence of critical deps |
| C12 | Boundary cost (polyglot penalty) | 5–12 | no (applies to E5 only; single-ecosystem candidates score full marks) | M3 cost profiles; AIOS stall; OpenHands pin | Number of boundaries on the hot path; mechanism per boundary (M3 a–f); codegen + CI check present; two-toolchain CI time |

### 6.2 Candidate ecosystem classes and evidence tables

Candidates are *classes*; exemplar names appear only as evidence pointers. Nothing here ranks them.

**E1 — Compiled, memory-safe systems ecosystem (exemplar in corpus: Rust).**

| facet | evidence (URL / path) |
|---|---|
| Mature harnesses & layer | Codex core loop, sandbox, app-server, exec-server (`codex-rs/*`, ~1.1M LoC per S-074); goose runtime, providers, MCP, ACP, server (`goose/crates/*`); Restate *server*; AIOS `aios-rs` (scaffold, stalled) |
| Protocol SDKs | MCP Tier 1 (`modelcontextprotocol/rust-sdk`; goose `rmcp 3.2.0`); ACP official (`agentclientprotocol/rust-sdk`; goose `agent-client-protocol 2.0.0`; ACP schema generated from this ecosystem's types); A2A `a2a-rs` |
| Sandbox/isolation | in-thread seccomp/Landlock (`codex-rs/linux-sandbox/src/landlock.rs`); bwrap wrapper crate (`codex-rs/bwrap`); Windows sandbox crates; embedded V8 with sandbox flag (`code-mode-runtime`) |
| Durable execution | Temporal SDK 1.0.0; Restate SDK; `sqlx`/sqlite (goose) |
| Web tooling | none native; all audited surfaces use a separate ecosystem (goose Electron; Codex TS SDK) |
| Eval alignment | none in-process; via boundary only |
| FFI to serving | same-language local inference (goose `candle-*`); uniffi outward bindings |
| Community | SO "most admired" 72% (2025); 1/11 harness cores in S-074 |

**E2 — Managed-runtime, dynamically-typed research ecosystem (exemplar: Python).**

| facet | evidence |
|---|---|
| Mature harnesses & layer | OpenHands SDK/server/workspace (`openhands-sdk`, `openhands-agent-server`); Mistral Vibe, Aider, Mini-SWE-Agent, Hermes (S-074); Omnigent meta-harness host (`omnigent/`); AIOS kernel (`aios/`); prime-agent REPL runtime (`prime-agent-runtime/src/rlm`); Claude Agent SDK client (`claude-agent-sdk-python`) |
| Protocol SDKs | MCP Tier 1 (`python-sdk`; OpenHands `fastmcp`); ACP official (`agentclientprotocol/python-sdk`, pydantic models; OpenHands pins `>=0.10.1,<0.11.0`); A2A `a2a-python` |
| Sandbox/isolation | via containers/HTTP (OpenHands `docker/workspace.py`; OpenHarness `sandbox/docker_backend.py`; Hermes 6 backends per S-074); no in-process syscall filtering observed |
| Durable execution | Temporal, Restate, DBOS SDKs; LangGraph checkpoint (`serde/base.py`); hand-rolled leases (`conversation_lease.py`) |
| Web tooling | none native; OpenHands ships a generated TS client instead |
| Eval alignment | Inspect AI, Harbor, SWE-bench harness, DSPy all native |
| FFI to serving | native for the dominant serving/tokenizer stacks (litellm gateway in DSPy/OpenHands) |
| Community | Octoverse 2025 +48.78% contributors; "dominance in AI and data science"; 5 (or 7, per paper summary) of 11 cores |

**E3 — Managed-runtime, gradually-typed web-native ecosystem (exemplar: TypeScript on Node/Bun/Deno/Workers).**

| facet | evidence |
|---|---|
| Mature harnesses & layer | Claude Code, Gemini CLI, OpenClaw (S-074); Pi (`packages/agent`, `packages/protocol` CBOR, `session-backends/sqlite-node`, bun-compiled binary); opencode (`packages/opencode/src/*`, effect-system library, Electron desktop, OpenAPI SDK); prime-agent agent layer; Codex/goose client surfaces; Cloudflare Agents runtime (S-056) |
| Protocol SDKs | MCP Tier 1 (`typescript-sdk`; MCP schema source of truth is `schema.ts`); ACP official (`@agentclientprotocol/sdk` used by opencode); A2A `a2a-js` |
| Sandbox/isolation | none in-process in corpus (opencode "policy-based", Pi "deliberate none"); Claude Code drives Seatbelt/bubblewrap via subprocess (S-053) |
| Durable execution | Temporal, Restate, DBOS SDKs; Cloudflare "fibers"/state/scheduling (runtime-locked); sqlite backends (Pi, opencode) |
| Web tooling | native |
| Eval alignment | none native; via boundary |
| FFI to serving | via HTTP only in corpus |
| Community | Octoverse 2025 #1 by contributors (+66.63%); 5/11 cores |

**E4 — Managed-runtime, statically-typed enterprise ecosystems (exemplars: JVM, .NET, Go).**

| facet | evidence |
|---|---|
| Mature harnesses & layer | 0/11 in S-074 corpus; AutoGen .NET runtime with protobuf worker protocol (`protos/agent_worker.proto`) |
| Protocol SDKs | MCP Tier 1 C#, Go; Tier 2 Java; ACP official Java, Kotlin; community Go ×6; A2A Go, Java, .NET |
| Sandbox/isolation | via subprocess/containers; no in-process precedent in corpus |
| Durable execution | Temporal (Go/Java/.NET), DBOS (Go/Java), Restate (JVM/Go) — strongest coverage of any class |
| Web tooling | none native |
| Eval alignment | none native |
| FFI to serving | via HTTP |
| Community | strong enterprise hiring; absent from harness corpus |

**E5 — Polyglot split (first-class candidate).** Defined as: *kernel/runtime* (event store, control envelope, reference monitor, sandbox driver, protocol edges) in one ecosystem; *laboratory/analysis* (sweeps, scorers, judges, notebooks) and/or *surfaces* (web, desktop) in another; boundary mechanism chosen from M3. Sub-variants the ratification must instantiate concretely: E5a kernel=E1 + lab=E2 + surfaces=E3 (Codex/goose shape); E5b kernel=E3 + lab=E2 (Pi/opencode + Harbor shape); E5c kernel=E2 + surfaces=E3 (OpenHands shape); E5d any kernel + sandbox helper in E1 (Claude Code/Codex helper shape).

| facet | evidence |
|---|---|
| Precedents | Codex (E1 core → E3/E2 SDKs via spawned binary + JSON-RPC; schema codegen); goose (E1 core → E3 desktop over HTTP; uniffi → E2/JVM; embedded V8); prime-agent (E3 agent → E2 REPL kernel); Claude SDK (E2 → E3 CLI over JSONL); OpenHands (E2 server → E3 generated client); Omnigent (E2 host → any via ACP stdio); AutoGen (E2 ↔ E4 via protobuf) |
| Boundary mechanisms | M3 (a)–(f) with file paths in §4c |
| Cost evidence | AIOS `aios-rs` stall; OpenHands ACP pin breakage note; Codex maintains `v1`/`v2` schema namespaces + allowlists + fixture tests; two toolchains in CI images (opencode `containers/rust`) |
| Mitigations observed | single source of truth + codegen + CI `check` (Codex, MCP, ACP); exact-version pinning of the cross-boundary binary (Codex Python runtime); protocol version handshake (Pi protocol v8 hello) |

### 6.3 Decision procedure (to be executed at end of Phase 1)

**Who.** *Executor:* the Phase 1 synthesis agent (or a fresh WS-L1 ratification subagent it spawns) prepares the scored matrix. *Decider:* the sponsor ratifies (doc 3 §0 constraint 1 makes this a sponsor-level decision); the synthesis agent may only *recommend*. *Vetoes:* WS-A3, WS-B1, WS-H1 (via WS-E5/B5) each hold a hard-gate veto limited to their own criterion (C6, C3, C4).

**Preconditions (all must be true, else the decision is postponed and Phase 2 proceeds language-neutral).** (P1) WS-A3, A4, A5, B1 dossiers are `done` and their ADRs ratified. (P2) The questionnaire in §6.4 is fully answered with citations to those ADRs. (P3) Scope register R-2.12.3 still `open`.

**Steps.**
1. *Freeze weights blind.* The executor picks one point in each C1–C12 range **before** looking at any candidate evidence, records it, and normalises to 100. Rationale for each point must cite the questionnaire (e.g. "B3 chose an owned event store ⇒ C3 at low end").
2. *Instantiate candidates.* List E1–E4 as single-ecosystem candidates and at least two concrete E5 variants (each naming kernel/lab/surface ecosystems and the M3 mechanism per boundary). Candidates may be added only with a source-code precedent for the *kernel* layer.
3. *Apply hard gates.* C2 always; C4 and C6 only if the questionnaire says so. Record exclusions with the ADR that triggered them.
4. *Run the two mandatory spikes* (time-boxed, identical spec, not merged into the product): (S1) *kernel slice* — append-only event log with the B1 ID model + one materialized view + one sandboxed tool call via the B5 handle + one MCP client call + one ACP stdio session, at N=1 and N=50 concurrent sessions; measure C5/C7 evidence. (S2) *boundary slice* (E5 candidates only) — the same slice with the lab side invoking the kernel over the declared boundary; measure per-sample crossing overhead and codegen round-trip. Spikes are throwaway; their outputs are numbers in the matrix, not code in the repo.
5. *Score.* Each surviving candidate gets 0–5 per criterion with a one-line evidence citation (path/URL/spike number). Scores without a citation are void.
6. *Sensitivity sweep.* Recompute the winner at the 2^k corners of the weight hull (k = number of criteria whose range width ≥ 5 points) plus 200 uniform samples. Report the winner's *win share*.
7. *Decide.* If win share ≥ 0.8 → recommend the winner. If < 0.8 → apply tie-breakers in order: (T1) fewer hot-path boundaries (C12 raw count); (T2) higher C6 (IR match — the IR is the program's differentiator); (T3) higher C1; (T4) the candidate whose exclusion would be cheaper to reverse (§6.6). If still tied, the sponsor decides and the ADR records it as a judgment call.
8. *Record & schedule revalidation.* Write the ADR (§11) with the frozen weights, the matrix, the sweep, spike numbers, and a **revalidation trigger**: re-run steps 3–7 if any of (i) WS-A3 amends the IR type discipline, (ii) WS-B5/E5 change where the sandbox helper lives, (iii) WASI 1.0 ships (M9), (iv) a Tier-1 protocol SDK is deprecated. Revalidation is cheap because the procedure is mechanical.

**Failure modes of the procedure itself.** Weights chosen after seeing scores (mitigated: step 1 is logged before step 2); spikes optimised by an ecosystem enthusiast (mitigated: single spec, reviewer from a different candidate camp); scoring without citations (void by rule); deciding under unresolved hard-gate questions (postpone rule).

### 6.4 Exact inputs the ratification needs from A3 / A4 / A5 / B1 (the questionnaire)

Each question is language-neutral; the answering workstream must not name an ecosystem. WS-L1 maps the answer to criteria.

| id | to | question | maps to |
|---|---|---|---|
| Q-L1-01 | WS-A3 | Does the IR require closed sum types (algebraic data types) with exhaustiveness checking for entities/edges, or is an open, schema-validated record model sufficient? | C6 (HG if closed sums with exhaustiveness are *required* for verifiers) |
| Q-L1-02 | WS-A3 | Does the IR require effect typing or capability typing to be *checked* at IR-validation time (e.g. a `Procedure` may not emit an `Effect` it is not `authorizes`-linked to), or only *recorded* for the reference monitor to check at runtime? | C6 (HG if static checking is required) |
| Q-L1-03 | WS-A3 + WS-B1 | Must IR documents and events have a *canonical, deterministic byte encoding* (for content addressing, WS-L4, and tamper evidence, WS-H6)? Which of: canonical JSON, CBOR deterministic encoding, protobuf, or "any, with a hash over a canonical form"? | C3, C6, C12 (boundary must preserve the canonical form) |
| Q-L1-04 | WS-A3 | Are IR diffs computed structurally (typed diff of entities/edges) or textually? Does the compiler need to *round-trip* IR ↔ source form? | C6 |
| Q-L1-05 | WS-B1 | Is the event log the *authoritative* record with materialized views derived (OQ-003)? What is the write path's consistency requirement (single-writer per run, leases, fencing tokens)? Which storage class (embedded SQL, append-only file, external DB) is mandated for Stage 0–1? | C3 |
| Q-L1-06 | WS-B1 (+B3 preview) | Does durability require *deterministic replay of the control loop* (library-style durable execution) or *state reconstruction from events* only? | C3 (library determinism constraints apply only in the first case) |
| Q-L1-07 | WS-A4 | What are the compile targets: (i) model-facing text/tool schemas, (ii) executable control structures — and are those control structures *data interpreted by the runtime* or *code emitted in a host language*? | C6, C12 (if code is emitted, the kernel language is also the compile-target language) |
| Q-L1-08 | WS-A4 | Must the compiler emit protocol-target artifacts (MCP tool schemas, A2A AgentCards, ACP session config) from the IR? | C2 |
| Q-L1-09 | WS-A5 | Where is the code-vs-config boundary: is a harness definition a *data document* (validated against the IR) that any runtime can load, or does composition require host-language code (plugins as code)? | C6, C12, C11; determines whether the lab can be in a different ecosystem than the kernel without loss |
| Q-L1-10 | WS-A5 | Are component variants required to be loadable *in-process* (shared address space) or may they be *out-of-process* participants (subprocess/plugin protocol)? | C12, C4, WS-L5 |
| Q-L1-11 | WS-A1 (available now) | Of the reference systems the program expects to read, port, or host, how many are in each ecosystem class? (Repo-local community measure.) | C10 |
| Q-L1-12 | WS-B5 / WS-E5 (Phase 2 preview; answer provisionally at end of Phase 1) | Does the sandbox helper (seccomp/Landlock/Seatbelt/job-object driver) live inside the kernel process, or as a separate helper binary behind a narrow protocol? | C4 hard gate switch |
| Q-L1-13 | WS-I2 (Phase 1) | Does the eval framework need to call external scorers/judges/benchmark loaders *in-process per sample*, or per run over a boundary? | C1 weight point |
| Q-L1-14 | WS-K4 (preview) | Must the SDK/embedding API expose the kernel *in-process* to host applications, or is a generated client over a local transport acceptable (Codex/OpenHands/opencode shape)? | C9, C12 |

### 6.5 Interface-contract sketch: the ecosystem-boundary contract (language-agnostic)

Regardless of the decision, if any boundary exists (E5, or E1–E4 with a sandbox helper or a surface), the spec should require the following contract so that the boundary is a *designed* object (this is the only contract WS-L1 proposes; it is consumed by WS-K4, WS-L5, WS-B5, WS-J6):

- **Operations.** `hello(version, capabilities) → {serverId, negotiated}`; `open_session(harness_ref, env_handle, budget) → session_id`; `submit(session_id, input) → turn_id`; `stream_events(session_id, from_seq) → stream<Event>`; `request_permission(session_id, capability_request) → decision` (server→client); `cancel(session_id | turn_id)`; `account(session_id) → cost_ledger`; `close(session_id)`. (Deliberately the same verb set as the thin observational ABI in ADR-0001 so that a polyglot boundary and a hosted-external boundary share one shape.)
- **Inputs/outputs.** All payloads are IR-typed documents in the canonical encoding answered by Q-L1-03; every message carries `protocol_version`, `schema_hash`, and `seq`.
- **Invariants.** (I1) Single source of truth for types; all other language bindings are *generated* and CI-checked (Codex `export.rs`, MCP `check:schema:json` precedent). (I2) Version negotiation on `hello`; unknown fields rejected by default (Pi protocol: "All envelope schemas reject unknown object properties"). (I3) The cross-boundary binary/package is pinned to an exact version by the consuming side (Codex Python runtime precedent). (I4) Events crossing the boundary are byte-identical to events in the ledger (no re-serialization that breaks WS-H6 tamper evidence). (I5) The boundary never widens authority: capability decisions are made kernel-side (WS-H1), the far side may only *request*.
- **Failure modes.** Peer crash → session enters `detached`, recoverable via `open_session(resume=…)` from the ledger (Managed Agents `wake`); version mismatch → refuse at `hello`; schema drift → CI fails before release; backpressure → bounded stream with `seq`-based resume.

### 6.6 Data-model sketch (for the ADR and the readiness report)

`LanguageDecisionRecord { decided_at, weights: {C1..C12: point}, weight_ranges, candidates: [{id, class, kernel, lab, surfaces, boundaries: [{from, to, mechanism ∈ M3(a–f)}]}], hard_gates_applied: [{criterion, adr_ref, excluded: [...]}], spikes: [{id, spec_hash, results}], scores: candidate × criterion → {score 0–5, citation}, sweep: {win_share, corners}, decision, tie_breakers_used, revalidation_triggers, reversibility: {cost_estimate, sunk_after_stage} }`.

### 6.7 Acceptance-criteria sketch (for the ADR to be ratified)

- AC1 Every criterion C1–C12 has a frozen weight with a rationale citing a Phase 1 ADR or the questionnaire.
- AC2 Every score has a citation (file path, URL, or spike id); the readiness report can spot-check 10 at random.
- AC3 Hard gates are applied with named triggering ADRs, or explicitly marked "not triggered".
- AC4 Sensitivity sweep reported; win share stated; tie-breakers used are listed.
- AC5 No score or weight relies on a `provisional` performance claim (S-074's loop/perf claim, S-073's fault-injection numbers, S-030's speedup are all excluded).
- AC6 The decision names the ecosystem *per layer* (kernel, lab, surfaces, sandbox helper) and the boundary mechanism per boundary, even for single-ecosystem outcomes ("none").
- AC7 Revalidation triggers recorded; owner named.
- AC8 The grep-based language-leak audit (RK-09) over all Phase 0–1 dossiers/ADRs is clean *before* the decision is recorded.

### 6.8 Build-stage suggestion

Outside C-tiers. The decision is a prerequisite for **Stage 0** of the build ladder (toolchain, repo layout, CI) and therefore must be ratified before `decompose-spec` runs; but it is *not* a prerequisite for any Phase 2–4 *research* workstream, all of which remain language-neutral. The two spikes (§6.3 step 4) are program work, not build tickets. If a boundary exists, the boundary contract (§6.5) is a **C0 / Stage 1** item owned jointly by WS-K4 and WS-L5 (it is the same shape as the ADR-0001 thin ABI, so WS-J6 reuses it at C2).

### 6.9 Dependencies on other workstreams

Blocking for ratification: WS-A3, WS-A4, WS-A5, WS-B1 (all Phase 1). Consulted: WS-I2 (Q-L1-13), WS-A1 (Q-L1-11), WS-B5/E5 (Q-L1-12, provisional), WS-K4 (Q-L1-14). Downstream: WS-L6 (packaging), WS-K1–K4, WS-L5, `decompose-spec` Stage 0.

## 7. Open questions raised

Listed in `WS-L1.additions.md` with temporary ids OQ-040 … OQ-047 (the questionnaire items Q-L1-01…14 are recorded there as OQ rows addressed to their resolvers; the eight OQ-WS-L1 ids group them). Synthesis assigns OQ-030+.

## 8. Conflicts flagged

CF-016 (doc 2 §7 framing "Rust cores vs Python/TS research" is too coarse relative to the corpus tally), CF-017 (potential runtime assumptions in WS-B3/K2/E5 before ratification — pre-registered), CF-018 (S-074 internal inconsistency on language counts). Details in the sidecar. Synthesis assigns CF-010+.

## 9. Ontology terms proposed

`ecosystem class`, `ecosystem boundary`, `polyglot split`, `boundary mechanism`, `boundary cost`, `hard-gate criterion`, `ratification questionnaire`, `language-leak audit`. Definitions in the sidecar.

## 10. Confidence

**Medium-high for the framing; not applicable for the decision (none made).** The criteria and the polyglot analysis rest on primary source reading of nine harness repositories plus three protocol repositories (tier B), on protocol SDK pages (tier B), and on production write-ups (tier B). The only academic input (S-074) is `provisional` but auditable, and we only use its mechanism findings, which our own audit reproduced for every repo we opened. Weakest links: (i) the per-system language tally from S-074 is inconsistent between the paper's summary and its table as extracted (flagged CF-018); (ii) Cloudflare runtime constraints and Codex App Server rationale were not readable from primary pages (403 / thin docs) and are backed by the repo instead; (iii) community statistics are population-level, not harness-specific. None of these affects the procedure's structure, only the eventual scores.

## 11. ADRs produced

- ADR-0009 (**proposed — ratify at end of Phase 1 together with the decision; Phase 0 synthesis accepted the framing and made no decision**) — *Language/ecosystem decision criteria, candidate classes, and ratification procedure* (contains no language choice).

## 12. Status

`done (Phase 0 framing)` — synthesized 2026-09-09; the decision itself is Phase 1 work (ADR-0009 stays proposed). Questionnaire routed as OQ-040…046.
