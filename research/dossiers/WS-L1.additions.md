# WS-L1 — register additions sidecar (folded into `registers/*.md` at Phase 0 synthesis, 2026-09-09; ids final)

## Sources

New sources (temporary ids):

| temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-155 | Model Context Protocol — official SDKs and SDK tiers (protocol 2026-07-28) | doc | B | 2 | P | no | https://modelcontextprotocol.io/docs/sdk | L1, E4, K3 |
| S-156 | A2A Protocol Specification 1.0.0 and SDK list (JSON-RPC, gRPC, HTTP+JSON bindings) | spec | B | 2 | P | no | https://a2a-protocol.org/latest/specification/ ; https://a2a-protocol.org/latest/sdk/ | L1, E4 |
| S-157 | ACP — Transports page (stdio JSON-RPC; Streamable HTTP draft) and libraries index | spec | B | 2 | P | no | https://agentclientprotocol.com/protocol/transports ; repo `docs/libraries/*.mdx` | L1, E4, J6 |
| S-158 | Temporal — developer SDKs (Go, Java, .NET, PHP, Python, Ruby, Rust 1.0.0, TypeScript) | doc | B | — | P | no | https://docs.temporal.io/develop | L1, B3 |
| S-159 | Restate — SDK repositories (TS, JVM, Python, Go, Rust; server in a compiled language) | repo | B | — | P | no | https://github.com/restatedev | L1, B3 |
| S-160 | DBOS — documentation (Python, TypeScript, Go, Java; Postgres-backed durable execution) | doc | B | — | P | no | https://docs.dbos.dev/ | L1, B3 |
| S-161 | Inspect AI (UK AISI) — eval framework; sandboxes Docker/K8s/Modal/Proxmox/Vagrant; runs external agents | doc | B | — | P | no | https://inspect.aisi.org.uk/ | L1, I2, I4 |
| S-132 | laude-institute/harbor — official Terminal-Bench 2.0 harness; agent adapters; multi-cloud sandboxes | repo | B | — | P | no | https://github.com/laude-institute/harbor | L1, I4 |
| S-162 | SWE-bench/SWE-bench — containerized evaluation harness; predictions interface | repo | B | — | P | no | https://github.com/SWE-bench/SWE-bench | L1, I4 |
| S-163 | GitHub Octoverse 2025 — TypeScript #1 by contributors; growth figures | post | B | — | P | no (population statistic) | https://github.blog/news-insights/octoverse/octoverse-a-new-developer-joins-github-every-second-as-ai-leads-typescript-to-1/ | L1, L6 |
| S-164 | Stack Overflow Developer Survey 2025 — Technology (admired/used) | post | B | — | P | no (population statistic) | https://survey.stackoverflow.co/2025/technology | L1, L6 |
| S-165 | WASI 0.3 release (2026-06-11) — native async in the component model; WASI 1.0 targeted late 2026/early 2027 | spec | B | — | P | yes (roadmap) | https://wasi.dev/releases/wasi-p3 | L1, L5, E5, H4 |
| S-143 | langchain-ai/langgraph — checkpoint layer (`libs/checkpoint/.../serde/base.py` SerializerProtocol) | repo | B | — | P | no | https://github.com/langchain-ai/langgraph | L1, B1, B3, A1 |
| S-153 | modelcontextprotocol/modelcontextprotocol — spec repo; `schema/2026-07-28/schema.ts` source of truth; JSON generation and CI checks | repo | B | 2 | P | no | https://github.com/modelcontextprotocol/modelcontextprotocol | L1, E4, K3 |
| S-166 | Barbaste et al. — HTML v1 of S-074 (per-system table: language, LoC, loop, sandbox, MCP/ACP/skills) | paper | C | 3 | P | yes (mech) | https://arxiv.org/html/2609.00006v1 | L1, A1, A5, E4 |

Existing S-ids actually opened (promote S → P): S-013 (DSPy code), S-012 (AutoGen code), S-030 (AIOS abs), S-031 (AIOS code), S-035 (ACP repo/docs), S-047 (Managed Agents), S-053 (containment), S-056 (Cloudflare harnesses), S-073 (Logos abs), S-074 (abs + HTML), S-107 (codex), S-109 (claude-agent-sdk-python), S-110 (OpenHands SDK), S-111 (opencode), S-112 (goose), S-113 (pi-mono), S-116 (prime-agent), S-117 (OpenHarness), S-118 (omnigent). Attempted but not opened: S-055 (HTTP 403).

## Open questions

| temp-id | question | resolver | due | blocking? |
|---|---|---|---|---|
| OQ-040 | Q-L1-01/02/04: Does the IR require closed sum types with exhaustiveness checking, statically checked effect/capability typing, and structural (typed) diffs with source round-trip? (C6 hard-gate switch) | WS-A3 | Phase 1 | yes (language ratification) |
| OQ-041 | Q-L1-03: Must IR documents and events have a canonical deterministic byte encoding (canonical JSON / deterministic CBOR / protobuf / hash-over-canonical-form) for content addressing and tamper evidence? | WS-A3 + WS-B1 (consult L4, H6) | Phase 1 | yes |
| OQ-042 | Q-L1-05/06: Is the event log authoritative with derived views; what write-path consistency (single writer, leases, fencing) and storage class are mandated at Stage 0–1; does durability need deterministic control-loop replay or event reconstruction only? | WS-B1 (B3 preview) | Phase 1 | yes |
| OQ-043 | Q-L1-07/08: Are compiled control structures runtime-interpreted data or emitted host-language code; must the compiler emit protocol-target artifacts (MCP schemas, A2A AgentCards, ACP session config)? | WS-A4 | Phase 1 | yes |
| OQ-044 | Q-L1-09/10: Is a harness definition a data document loadable by any runtime or does composition require host-language code; must component variants load in-process or may they be out-of-process participants? | WS-A5 (consult L5) | Phase 1 | yes |
| OQ-045 | Q-L1-12: Does the sandbox helper (seccomp/Landlock/Seatbelt/job-object driver) live in the kernel process or as a separate helper binary behind a narrow protocol? (C4 hard-gate switch; provisional answer needed at end of Phase 1) | WS-B5 / WS-E5 (consult H4) | Phase 1 (provisional) / Phase 2 (final) | yes (provisional) |
| OQ-046 | Q-L1-11/13/14: repo-local ecosystem tally of reference systems (A1); per-sample vs per-run external scorer calls (I2); in-process vs generated-client SDK boundary (K4) | WS-A1, WS-I2, WS-K4 | Phase 1 (K4 preview) | no |
| OQ-047 | Should the two ratification spikes (kernel slice; boundary slice) be run by a fresh subagent per candidate under one spec, and who reviews for ecosystem bias? | Phase 1 synthesis / orchestrator | end Phase 1 | no |

## Conflicts

| temp-id | parties | contradiction | proposed resolution |
|---|---|---|---|
| CF-016 | doc 2 §7 / doc 3 §5 WS-L1 anchor ("Rust cores in Codex/goose vs Python/TypeScript research stacks") vs S-074 corpus tally and our audit | The framing implies a binary "compiled core vs scripting research" split; the corpus shows one compiled core out of eleven, two managed-runtime classes splitting the rest, and *every* product-grade system polyglot at a boundary | Replace the binary framing with the five candidate classes E1–E5 and the boundary-mechanism taxonomy M3(a–f); doc 2 remains a map, not authority |
| CF-017 | WS-L1 neutrality mandate vs Phase 2 workstreams that need runtime concreteness (WS-B3 durable execution, WS-K2 web tooling, WS-E5 sandbox helper, WS-K4 SDK) | Those workstreams may be pushed to assume a runtime before ratification (RK-09) | Pre-registered: they answer the language-neutral questionnaire items (Q-L1-06/09/12/14) instead; synthesis greps their dossiers; violations logged here |
| CF-018 | S-074 summary statement ("Python dominates open-source (7/11)") vs S-074 per-system table as extracted (5 TypeScript / 5 Python / 1 Rust) | Internal inconsistency in a provisional source | Treat exact counts as provisional; rely only on the direction (compiled core is the minority; two managed-runtime classes split the rest); WS-A1 to re-extract from the PDF if the count becomes load-bearing |

## Ontology terms

| term | definition | notes |
|---|---|---|
| ecosystem class | A family of language + runtime + package ecosystems grouped by the properties that matter to the criteria (compilation model, typing discipline, native web affinity, research-tool concentration), not by language name | E1–E4 in WS-L1 §6.2; owner WS-L1; status proposed |
| ecosystem boundary | A designed process, network, or FFI seam across which two components implemented in different ecosystem classes exchange IR-typed messages under the boundary contract (WS-L1 §6.5) | Same verb set as the thin observational ABI (ADR-0001) |
| polyglot split | A candidate architecture that assigns different ecosystem classes to the kernel/runtime, the laboratory/analysis layer, the surfaces, and/or the sandbox helper, with a named boundary mechanism per boundary | Candidate E5; sub-variants E5a–d |
| boundary mechanism | One of the recurring implementation patterns for an ecosystem boundary: subprocess + JSON-RPC/JSONL over stdio; localhost HTTP/WebSocket with generated clients; schema-first codegen from a single source of truth; in-process FFI bindings; embedded engine for model-authored code; protobuf/gRPC | M3(a)–(f), each with source-code precedent |
| boundary cost | The recurring cost of an ecosystem boundary: serialization and type duplication (mitigated by codegen), two toolchains, cross-boundary version pinning, cross-boundary debugging, contributor barrier; scored as criterion C12 | Penalty applies to polyglot candidates only |
| hard-gate criterion | A criterion whose failure excludes a candidate before weighted scoring; some gates are conditional on Phase 1 answers | C2 unconditional; C4, C6 conditional |
| ratification questionnaire | The fixed list of language-neutral questions (Q-L1-01…14) that upstream workstreams answer so the language decision can be scored mechanically | WS-L1 §6.4 |
| language-leak audit | The grep-based check (RK-09) run over all dossiers/ADRs before the language ADR is recorded, confirming no upstream artifact commits to a language, runtime, package ecosystem or framework | Acceptance criterion AC8 |
