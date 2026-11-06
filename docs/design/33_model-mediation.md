# 33_model-mediation — Model-call mediation proxy design
Stream: C
Researched: 2026-11-06
Author: design worker (Claude Opus 5.5 subagent), synthesize-spec row C2
Ledger rows covered: C2
Repository baseline: main 09fe8523 (inspected at worktree HEAD 688c7f0a on `claude/experiments-spec`; `git diff --stat 09fe8523 HEAD -- crates spec plugins scripts` is empty, so every code locator below is valid at 09fe8523; untracked program notes 27/28 were not read)
Confidence overall: medium. The repository facts are high-confidence (source read, line-cited). Everything this note proposes is a design proposal and is UNVERIFIED by construction: nothing was built, compiled or run, no podman was started and no network call was made. The three riskiest assumptions are flagged at their findings: the in-cell relay's static build (F5), harness tolerance of buffered release (F9), and the input-token bound (F10). Each has a named validation.

Inputs read in full: `docs/research/26_ollama-glm.md` (C1; findings F1–F16 and REQ-XC-001…015 are binding). Also read: brief §3 (A-3, A-4, A-10), ledger row C2 with §Q (Q-X4–Q-X6, Q-X11, pending Q-X21–Q-X23), the Execution contract, `docs/research/CONVENTIONS.md`, the OL-03/08/15 rows of `docs/experiments-OUTLINE_TRACE.md`, and the cost, slot-record and repair findings of `docs/research/24_executed-record-audit.md` (F5, F11, REQ-XA-011/012). Root `AGENTS.md` and `crates/AGENTS.md` were read. No crate has a nested `AGENTS.md` (`find crates -name AGENTS.md` returns only `crates/AGENTS.md`).

Refinements of C1's candidate requirements are marked **REFINES REQ-XC-0nn** at the point they occur. No C1 finding is contradicted.

## Sources inspected (repository, 2026-11-06T23:06Z–23:19Z)

| Area | Locators (all at 09fe8523) |
|---|---|
| Hosting ABI | `crates/hh-hosting/src/{lib.rs:1-99, adapter_b.rs:1-329, adapter_c.rs:1-80,361-396, records.rs:1-140, budget.rs:1-240, service.rs:900-945, events.rs:760-800}`, `crates/hh-hosting/Cargo.toml` |
| Gateway | `crates/hh-gateway/src/{lib.rs:1-133, gateway.rs:1-80,180-330, http_transport.rs:1-80,210-470,583-700, dialects.rs:85-260,405-500, dialect.rs:57-70, snapshot.rs:1-400, events.rs:20-60,190-260,420-460, codec.rs:148-171, vocab.rs:259-320,425-456, router.rs:1652-1665,1687,1770-1790}`, `src/bin/hh-gateway-stub.rs:1-60`, `tests/r4_5d.rs:1139-1186`, `Cargo.toml` |
| Embed | `crates/hh-embed/src/{net.rs:1-125,246-259, gateway_model.rs:1-140,300-330,1170-1260, hosting_ops.rs:1-40}`, `Cargo.toml` |
| Wire / TLS | `crates/hh-wire/src/http.rs:1-40,355-500` (API index read in full) |
| Env / helper / containment | `crates/hh-env/src/{egress.rs:1-80, egress_profile.rs:1-186, driver.rs:805-895, study.rs:1-30,820-835,1460-1530, activation.rs:285-320, publication.rs:340-410}`; `crates/hh-helper/src/{podman.rs:1-140, bin/hh_helper.rs:1-125}`; `crates/hh-containment/src/{policy.rs:187-206,411-425,462-485,791-820, backend.rs:440-458,625-700,950-975, admit.rs:200-225}` |
| Budget | `crates/hh-budget/src/{account.rs:1-80,195-230,290-400,505-540,600-690,935-995, pricing.rs:280-420,530-700, quantity.rs:1-60, attribution.rs:55-95, usage.rs:164-180, spec.rs:264-284,405-425}`; `crates/hh-ontology/src/dimensions.rs` (spend unit, l.205, 384) |
| Ledger | `crates/hh-ledger/src/{classes.rs:1-60,158,1254-1259,1450-1469,1636-1741,1911-1965,2038-2043,2164, store.rs:58,1976-2001, leases.rs:1-40, replay.rs:1-40, branch.rs:268-281}` |
| Control | `crates/hh-control/src/driver.rs:2880-2935` |
| Eval | `crates/hh-eval/src/central_protocol.rs:140-225,620-660` |
| Scripts / records | `scripts/check-removability.sh:110-133`, `docs/build/reports/live/pricing.json` (first 40 lines), `rust-toolchain.toml` |
| Spec / ADRs | `spec/CANONICAL_SPEC.md` lines 5807 (units), 8699 (`attribute_spend`), 8720 (`PricingTable`), 8187 (binding budgets); `docs/adr/ADR-0363` D1–D5 (l.29–100); `ADR-0367` D3 (l.80–95); `ADR-0421` header and Decision 1–3 (l.1–95) |

---

## 1. Design overview

### 1.1 Topology

```text
 GCE VM (Linux, rootless podman; one per paid phase — A-10/B3)
 ┌──────────────────────────────────────────────────────────────────────────────┐
 │  runner process (B2) ── spawns, Json control channel ──┐                     │
 │    │ EnvDriver / hh-helper (cells)                     │                     │
 │    │                                                   ▼                     │
 │    │                      hh-model-proxy (hharness-hosting bin, lab_host)    │
 │    │                      ┌──────────────────────────────────────────────┐  │
 │    │                      │ per-slot listener ×N  (unix sockets, 0600)   │  │
 │    │                      │   /run/hh-proxy/<phase>/<slot>.sock          │  │
 │    │                      │        │ admit: token, route, field policy,  │  │
 │    │                      │        │ model+effort pin, reserve (Account) │  │
 │    │                      │        ▼                                     │  │
 │    │                      │ FIFO queue ─► upstream semaphore (≤3)        │  │
 │    │                      │        │  raw forward leg (hh-gateway +      │  │
 │    │                      │        │  hh-wire http + hh-tls ClientTls)   │──┼──► https://ollama.com
 │    │                      │        ▼                                     │  │    /v1/chat/completions
 │    │                      │ ledger actor (sole writer of the PHASE PROXY │  │    /v1/messages
 │    │                      │ RUN; blobs → model.call.* → release)         │  │    /v1/responses (cond.)
 │    │                      │ telemetry poller (≤1 req/min: usage+balance) │──┼──► /api/usage, /api/balance
 │    │                      │ OLLAMA_API_KEY: resolved only inside send    │  │    /api/tags, /api/show
 │    │                      └──────────────────────────────────────────────┘  │
 │    │                                   ▲ bind-mount (one socket per cell)    │
 │  ┌─┴──────────────── podman cell (slot s) ─┼────────────────────────────┐    │
 │  │ --network none  (net.mode = mediated)   │                            │    │
 │  │ main process: /hh/bin/hh-model-relay  127.0.0.1:P  ⇄  /run/hh/model.sock │
 │  │ harness (Terminus-2 / mini-swe-agent / OpenCode / Claude Code …)     │    │
 │  │   OPENAI_BASE_URL=http://127.0.0.1:P/v1   ANTHROPIC_BASE_URL=…:P     │    │
 │  │   *_API_KEY = per-slot proxy token (never the Ollama key)            │    │
 │  │ /work (task workspace)                                               │    │
 │  └──────────────────────────────────────────────────────────────────────┘    │
 │  optional: kernel-native GatewayModel lanes (Phase 3/4 HIR runs) → proxy's   │
 │  token-gated loopback TCP listener (one semaphore, one pin, one capture; F17)│
 └──────────────────────────────────────────────────────────────────────────────┘
```

There is one proxy process per phase VM. It is the only holder of the Ollama key and the sole writer of one **phase proxy run** in the store. That run's budget tree is: root = the phase's authorized cap; one `slice` child per slot. Each cell sees exactly one socket and nothing else on the network.

### 1.2 Wire-surface matrix

| Downstream surface (in cell) | Upstream | Auth in → out | Streaming | Effort pinned at | Declared mutations | Field policy (unsupported upstream) | Usage / cache split per call | Program status |
|---|---|---|---|---|---|---|---|---|
| `POST /v1/chat/completions` (OpenAI chat) | `https://ollama.com/v1/chat/completions` | `Authorization: Bearer <slot token>` → `Bearer <key>` | yes (SSE) | `reasoning_effort` (overwrite or insert) | `model` override; `stream_options.include_usage=true` injected when `stream=true` (F6) | `tool_choice:"auto"` stripped-with-notice; any other `tool_choice`, `n>1`, `logprobs`, `logit_bias` refused (HTTP 400, typed) | final usage chunk; cached count undocumented (C1 F3), so `cache_read` is `available:false` until the first smoke | **required** |
| `POST /v1/messages` (Anthropic) | `https://ollama.com/v1/messages` | `x-api-key` **or** `Authorization: Bearer` (slot token) → `Bearer <key>`; `anthropic-version` forwarded, refused if < `2023-06-01` | yes (SSE events) | `output_config.effort` (overwrite or insert) | `model` override | `cache_control` stripped-with-notice (all paths); `metadata` stripped-with-notice; `tool_choice` `{type:auto}` stripped, others refused; `thinking.budget_tokens` forwarded and recorded as not-enforced | `input_tokens`/`output_tokens` only, so cost is an upper-bound `estimate` (C1 F3, REQ-XC-007) | **required** (Claude Code) |
| `POST /v1/messages/count_tokens` | none | slot token | n/a | n/a | — | typed refusal, HTTP 404 in Anthropic error shape (`not_found_error`, `hh_proxy: unsupported_upstream`); never a fabricated count | n/a | required (refusal only) |
| `POST /v1/responses` (stateless) | `https://ollama.com/v1/responses` | Bearer → Bearer | yes | `reasoning.effort` | `model` override | `previous_response_id`, `conversation`, `store:true` refused; custom/freeform tool replay refused | not enumerated by docs | **conditional**: only if C3/C4 admit a Responses-only harness (Codex) |
| `GET /v1/models` | forwarded verbatim to the same host | slot token → Bearer | no | — | none | — | — | required (free; recorded) |
| anything else | none | — | — | — | — | HTTP 404, receipted as `refused_route` | — | — |
| native `/api/chat` | — (not offered downstream; translation target rejected for the pilot, F6) | — | — | — | — | — | — | not offered |

Pass-through is the default. The upstream body is the downstream body plus exactly the listed mutations, each recorded with its JSON path. Header handling: an inbound closed forward allowlist `{content-type, accept, anthropic-version, anthropic-beta, user-agent}`. All other inbound header **names** are recorded and their values dropped. The auth header is never recorded. The outbound response to the cell carries `content-type`, `retry-after` and the ADR-0363 D3 allowlist (`request-id`, `x-request-id`, `x-ratelimit-*`, …). `set-cookie`, auth and `*-key|*-token|*-secret` names are never forwarded or recorded.

### 1.3 Receipt schema sketch (`hh.model_io/1` + additive `proxy` member)

```text
blob  hh.model_io/1  (media application/json; deposited before the terminal row)
{ schema: "hh.model_io/1", model_call_id,
  request_body:  <downstream body as parsed JSON>          // existing member
  frames:        [{at_ms, kind, data}]                      // existing; decoded by the gateway translators
  outcome:       {stop_reason, served_model, usage_raw} | {error}
  proxy: {                                                  // NEW, additive
    schema: "hh.model_proxy.receipt/1",
    phase_run_id, slot_id, slot_call_seq, mode: live|record|replay, proxy_version,
    surface: openai.chat|anthropic.messages|openai.responses|models|refused_route,
    downstream: {method, path, header_names[], headers_kept{allowlist},
                 body_ref, body_sha256, body_bytes},        // raw bytes, separate blob
    mutations:  {model{requested, applied}, effort{requested|null, applied, path},
                 stripped[{path, value_sha256}], injected[{path, value}],
                 refused{path, reason}|null},
    upstream:   {dispatched: bool, endpoint_host, path, body_ref, body_sha256,
                 status, response_headers{D3 allowlist}, body_ref, body_sha256,
                 content_type, complete: bool, bytes},      // raw upstream bytes, separate blob
    released:   {status, release_mode: buffered|buffered_heartbeat,
                 identical_to_upstream: bool, body_ref?},   // body_ref only when bytes differ
    timing:     {received_ms, reserved_ms, dequeued_ms, first_byte_ms,
                 upstream_done_ms, receipted_ms, released_ms, queue_wait_ms},
    usage:      {view: TokenVector(hh-inclusive/1), cache_read_available,
                 reasoning_available, source: provider_per_call|absent},
    reservation:{reservation_id, reserved_micros,
                 bound{input_bytes, overhead_tokens, max_output_tokens, rates_ref}},
    replay:     null | {recording_ref, key, canonicalizer_id,
                        match: exact|canonical_equal|diverged, first_diff_path?},
    disposition: forwarded|refused_auth|refused_route|refused_policy|refused_budget|
                 refused_phase_stopped|upstream_error|upstream_timeout|
                 stream_broken|replay_served|replay_diverged|proxy_aborted } }
```

Ledger rows in the phase proxy run, per call, in order. Each is appended by the ledger actor through the run's fenced writer.

1. `model.call.requested`. Carries `request_ref` = the downstream body blob, `model_ref{…, effort}` and attribution `{slot_id, charged_to: subject}`.
2. `control.budget.reserved` (`held` or `refused`).
3. `model.call.attempt.started{queue_wait_ms}`.
4. `model.call.attempt.completed|failed`.
5. Blob deposits: raw upstream bytes, then the `hh.model_io/1` document.
6. `model.call.completed|failed{response_ref, usage, timing, served_model}`. Only now is the response released to the cell.
7. Then `measurement.cost.attributed`, `control.budget.consumed` and `control.budget.released`.

Per slot: `security.credential.bound` / `security.credential.revoked` for the proxy token. Per phase: `model.profile.probed` (catalog identity) and two new classes, `measurement.provider.usage.observed` and `measurement.cost.reconciled` (F8, F14).

### 1.4 Budget state machine

```text
 per call                                                   per slot slice            per phase root
 RECEIVED ─auth✗─► REFUSED_AUTH (401)                         ACTIVE                     OPEN
    │ route/policy✗ ─► REFUSED_ROUTE (404) / REFUSED_POLICY (400)  │ reserve refused         │ root level+reserve > cap
    ▼                                                         │  ─► SLOT_EXHAUSTED        │  ─► (Account::check) ─►
 ADMITTED ─reserve refused─► REFUSED_BUDGET (402, typed)       │     (no run stop; runner  │  EXHAUSTED: exhaust() =
    │ phase stopped ─► REFUSED_PHASE_STOPPED (402)              │      closes the hosted    │  control.budget.exceeded +
    ▼ reserve held (worst-case bound)                          │      session              │  escalation + decision{stop}
 RESERVED ─► QUEUED (FIFO, semaphore ≤3) ─► DISPATCHED          │      budget_exhausted)    │  (E1: only after in-flight
    │                    │                     │               │ slot end ─► CLOSED        │   calls reach a terminal)
    │                    │                     ├─► RESPONDED   │   (Account::complete:     │  every later admit ─►
    │                    │                     ├─► UPSTREAM_ERROR / TIMEOUT / STREAM_BROKEN  │    remainder → parent;   │  REFUSED_PHASE_STOPPED
    │                    │                     ▼               │    token revoked)         │
    │                    │                RECEIPTED (blobs + terminal row durable)         │
    │                    │                     ▼                                           │
    │                    │                RELEASED (bytes to cell)                         │
    │                    │                     ▼                                           │
    │                    │                SETTLED (attribute_spend → charge → release)     │
    └── proxy crash after DISPATCHED ─► ORPHANED ─(recovery)─► SETTLED_BOUNDED
                                         (terminal `failed{proxy_aborted}`; spend charged as
                                          bounded{lo:0, hi:reserved}; remainder released)
```

Invariant (the acceptance property): at every durable ledger prefix, for every node on the path, `consumed + reserved ≤ hard`. The bounded overshoot allowed by `charge`'s `over_reservation` can occur only if the input bound is violated (F10). A violated bound raises `reservation_bound_violated`, which halts admission.

### 1.5 The shared slot cost record (stated once; C2 owns `model`, B3 owns `compute`)

```text
slot.costs = { schema: "hh-slot-costs/1",
  model:   { provider: "ollama-cloud", lane_ref, model_id, effort_applied,            // C2
             calls{dispatched, completed, upstream_failed, proxy_refused},
             tokens{input_total, input_uncached, cache_read, cache_read_available,
                    output_total, output_reasoning, output_reasoning_available},
             spend{money: Money{micro_units, currency}, provenance, confidence,
                   coverage_ppm, pricing_ref},
             reservation{peak_reserved_micros, refused_count},
             phase_run{run_id, slice_budget_id, first_seq, last_seq},
             settlement{status: pending|settled|settled_discrepant|unreconcilable, ref} },
  compute: { … B3: instance_id, machine_type, start/stop UTC, attributed_ms,         // B3
             rate_source, money, provenance: estimate|billing_export … },
  total:   { money?, composed_of: [model, compute], note } }                          // only when currencies match;
                                                                                      // provenance never averaged
```

Per-slot `model` values are folds over the phase proxy run's rows filtered by `slot_id`. They are never stored as a separate authoritative counter (CC3). A packet carries the fold plus the run's ledger head, so a reader can recompute it.

---

## 2. Findings

### F1 — Adapter B exists only as a records-in shape; no proxy server, transport or receipt writer exists
Claim: `hharness-hosting` defines Adapter B's policy, lift and adapter-record shapes over an abstract `InterceptTransport`, but nothing in the workspace implements a model proxy server. Adapter C alone can claim only `end_state`, so the phase-2 participant must compose Adapter C (installed process) with Adapter B's proxy channel for `model_io`.
Status: VERIFIED (source); the composition is a design proposal (UNVERIFIED).
Evidence kind: source inspection.
Evidence: `crates/hh-hosting/src/adapter_b.rs:1-13` (proxy "lives inside the environment on the participant's model path"; `model_io` rides the `proxy` channel as `mediated`). `:96-108` (`InterceptTransport{drain_hooks, drain_model_observations, set_model_override, transcript_rows}` has no implementation outside tests and fixtures). `:158-189` (`lift_proxy` stamps `Intercept`/`Mediated`/`EventChannel::Proxy`). `:297-329` (`adapter_b_record` with `ext.model_io_intercept = "base_url"` and placement `InEnvironment` only). `adapter_c.rs:9-19,361-396` (`observability = {end_state}`; `coordinate_model: unsupported`). `records.rs:63-73,107-138` (`ModelIoIntercept::BaseUrl`; `HostingExt` members). `budget.rs:50-163` (`derive_budget_enforcement`: `spend`/`tokens.*`/`model_calls` are `enforced` iff `model_io_intercept ≠ none` for any mechanism, so `ContainerInstalled` + `BaseUrl` gives enforced spend). `service.rs:903-914`: the hosting service's `accumulate_usage` folds one `used` integer into both `tokens.output.visible` and `spend`. That is fixture-grade, so the proxy cannot rely on it.
Retrieved: 2026-11-06T23:07Z
Implication for the spec: The proxy is new machinery. The participant declaration for every phase-2 harness is `hosting_mechanism = container_installed` with `HostingExt{model_io_intercept: base_url, process_placement: in_environment}` and observability `{end_state, model_io}`. Here `model_io` is earned only through the proxy channel. Spend accounting must come from the proxy's own ledger rows, never from `accumulate_usage`. C4 owns the composite record. C2 supplies the `model_io` half.
Outline delta: EXTENDS OL-15. The admission scaffolding needs a composite B+C participant declaration.
Limitations: Test-only implementations of `InterceptTransport` were not enumerated.

### F2 — Placement: the proxy belongs in the hosting tier as a module plus binary, depending downward on the gateway and spawned as a process
Claim: The proxy core should live in `hharness-hosting::proxy` with a binary `hh-model-proxy`. It should depend on `hharness-gateway`, `hharness-budget`, `hharness-ledger`, `hharness-secrets`, `hharness-wire` and `hharness-tls`, all downward. The runner (B2) spawns it and talks to it over a Json control channel, because no crate may name `hharness-hosting` as a normal dependency.
Status: UNVERIFIED (design proposal). Proposed validation: `bash scripts/check-removability.sh` passes after the edge `hharness-hosting → hharness-gateway` is added; a test drives the binary over its control channel.
Evidence kind: design proposal over source inspection.
Evidence: `scripts/check-removability.sh:110-133` ("no crate depends on hharness-hosting"; `hosting_edges = []`). `crates/hh-hosting/Cargo.toml` (deps: wire, identity, provenance, ontology, hir, ledger, budget; no gateway yet). `crates/hh-gateway/src/lib.rs:1-10` (the gateway is "model-blind … transport-only … the sole emitter of usage, cost provenance, attempt timing, served-model drift"). `crates/hh-gateway/Cargo.toml` (depends on ledger, budget, telemetry, secrets, tls, so it is lower than hosting). `crates/hh-embed/src/hosting_ops.rs:1-6` (the boundary "never loads a `hharness-hosting` implementation"). `crates/hh-kernel/Cargo.toml` (kernel = wire + embed-schema + embed only). K-2 rules out the CLI.
Retrieved: 2026-11-06T23:09Z
Implication for the spec: Alternatives weighed:
- (a) A new module in `hh-gateway`. Rejected: Adapter B's policy type and the hosting lift live in `hh-hosting`, and gateway → hosting is an upward edge that would break removability.
- (b) A surface inside `hh-embed`. Rejected: hh-embed cannot load hosting, and binding (c) is serial (F4).
- (c) A new workspace crate. Possible, but it needs its own ADR and adds a member for no gain over (d).
- (d) Hosting module plus binary. **Recommended**: it keeps CC6 removability and puts Adapter B "made real" next to its declared shapes.

The kernel-side gateway semantics stay in hh-gateway, and the proxy *calls* them (F3). An ADR records the new internal edge and the binary (ADR-C2-A, §3).
Outline delta: EXTENDS OL-15 (architecture of the mediation scaffolding).
Limitations: Whether a hosting-tier binary that performs live network I/O conflicts with the crate description's "thin/observational" posture (`lib.rs:19-40`) is an ADR judgement, not a mechanical rule.

### F3 — The gateway's live transport is not byte-faithful; the proxy needs a raw forward leg that reuses its dial, auth, TLS and decoders
Claim: `HttpModelTransport::send` re-serializes the request body canonically, so key order and whitespace change. It returns only decoded `WireFrame`s, not raw bytes. Its Messages family hard-codes `x-api-key`. `ModelGateway::open_call` needs a sealed `ProviderRequestPlan` validated against the dialect's `plan_schema`, which arbitrary harness bodies fail. Verbatim receipts therefore need a new `forward_raw` primitive in `http_transport.rs`. It reuses `dial`, `auth_header`, the hh-wire codec and `ClientTls`, and runs the existing `SseDecoder`/`ChunkTranslator`/`MessagesTranslator` over the captured raw bytes after the fact.
Status: VERIFIED (current behaviour); UNVERIFIED (proposed primitive). Proposed validation: a fixture-upstream test asserts upstream-received bytes equal downstream bytes plus the declared mutations, and that the decoders' usage/stop/served-model output on the captured bytes equals the frames path.
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-gateway/src/http_transport.rs:597-603` (`body.to_canonical_string()`). `:648-700` (SSE → `tr.push(&chunk)` → frames; raw bytes discarded). `:284-296` (`Family::Messages => ("x-api-key", secret)`; `ChatCompletions => Bearer`). `:424-466` (`post_op` is single-shot and parses JSON; it cannot stream). `:271` (`rw_ms = stream_idle_timeout_ms.clamp(30_000, 600_000)`). `gateway.rs:254-321` (`open_call` → `validate_plan` → endpoint admit → credential bind → `MissingReservation` if no reservation). `dialects.rs:115-123` (`allowed_keys` closed set). `openai.rs:143,251` (`SseDecoder`, `ChunkTranslator`). `anthropic.rs:214` (`MessagesTranslator`). C1 F1 (Ollama Messages needs Bearer).
Retrieved: 2026-11-06T23:10Z
Implication for the spec:
- Add `forward_raw(credential, endpoint_ref, op_path, kept_headers, body_bytes) → RawExchange{status, headers, body_bytes, chunk_stamps[]}` to `HttpModelTransport`. This is CC1-clean: the same dial, TLS and codec home.
- Make the auth header form a property of the lane's declared `auth_kinds`, not of the family (ADR-0366 amendment).
- Add an Ollama upstream lane: dialect documents `ollama-chat@1` and `ollama-messages@1` (and `ollama-responses@1` if needed). Each has honest members per C1 F15: Bearer, `tool_choice` refused, `reasoning_effort` / `output_config.effort` admitted, `cache_semantics: uncached` with a provider-reported cached count admitted when present, `count_tokens: None`, `snapshot_id_exposed: None`.
- The proxy builds `model.call.*` payloads with new builders beside `events::call_requested`/`call_completed` that take raw-exchange inputs. It does not call `open_call`.

**REFINES REQ-XC-001** (where the Bearer rewrite lives).
Outline delta: EXTENDS OL-15.
Limitations: The `discover`/`probe_capability` paths were read only in their trait defaults.

### F4 — The HTTP pieces exist in hh-wire; binding (c) is not reusable as the proxy server, but its gate helpers are
Claim: `hharness_wire::http` provides `read_request` (Content-Length and chunked request bodies), `write_response` (Content-Length only), `BodyReader`, `write_request` and `read_response_head`. That is enough for a server and a client. There is no chunked or streaming response writer, and `reason()` lacks phrases for 402/429/502/503/504. Binding (c) is a serial, loopback-only JSON-RPC server, so it is not the proxy. Its `token_matches` (constant-time) and `mint_token` (128-bit from `/dev/urandom`) are exactly the slot-token primitives needed.
Status: VERIFIED (source); relocation proposal UNVERIFIED.
Evidence kind: source inspection.
Evidence: `crates/hh-wire/src/http.rs:1-14` (the one HTTP/1.1 codec home, ADR-0364). `:21,24` (`MAX_HEAD_BYTES` 16 KiB, `MAX_HEADERS` 64). `:355-397` (chunked request bodies decoded). `:402-435` (`read_request`). `:459-475` (`reason`: 200/400/401/403/404/405/411/413/500, else "Status"). `:477-497` (`write_response` writes `Content-Length`). `crates/hh-embed/src/net.rs:14-34` (loopback only, bearer, exact Host, Origin refused, "Concurrency is deliberately serial"). `:64-89` (`token_matches`, `mint_token`). `:246-259` (`serve_net_n` refuses non-loopback).
Retrieved: 2026-11-06T23:11Z
Implication for the spec:
- Add `write_response_head_chunked` + `write_chunk` + `finish_chunked` and the missing reason phrases to `hh-wire::http`, in the same module (CC1).
- Move `token_matches`/`mint_token` to a lower crate (`hh-wire` or `hh-secrets`) and re-export them from `hh-embed::net` byte-identically, so the proxy and binding (c) share one implementation. Hosting cannot depend on embed.

The proxy is threaded (listener per slot, one ledger actor, upstream pool). It does not inherit binding (c)'s serial contract.
Outline delta: none (implementation detail under OL-15).
Limitations: Header-count headroom for real harness traffic (Claude Code sends many `anthropic-beta`/`x-stainless-*` headers) is unmeasured. Exceeding 64 headers is a typed `HttpError`, so the failure would be visible, not silent.

### F5 — Reaching only the proxy: the containment vocabulary already has the honest posture (`mediated` + an allow-listed unix socket); the helper needs a socket-mount profile and an in-cell relay
Claim: `net.mode = mediated` is defined as "the namespace is removed; the only route out is the bridged channel to a host-side mediator". `unix_sockets{mode: allow_listed, allow[]}`, `residual_channels[unix_socket]` and `local_binding` all exist. `mediated` already maps to podman `--network none`.

The helper admits only `--network none|bridge` and always passes `extra_args: vec![]`. A `model_proxy` egress profile therefore needs three things:
- (i) a helper flag that bind-mounts one host socket at `/run/hh/model.sock`;
- (ii) the cell's main process replaced by a tiny std-only relay `hh-model-relay` (127.0.0.1:P ⇄ the unix socket, pure byte pipe), bind-mounted read-only, instead of `sleep infinity`;
- (iii) a policy with `net.mode = mediated`, `local_binding = true`, `unix_sockets.allow = ["/run/hh/model.sock"]` and a declared `unix_socket` residual with owner/closer.
Status: VERIFIED (vocabulary, current argv); UNVERIFIED (profile, relay, static build).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-containment/src/policy.rs:187-195` (`NetMode::{None, Mediated, Public}`). `:411-425` (`UnixSocketMode::{BridgedOnly, AllowListed}`, `allow[]`). `:462-485` (`NetPolicy.local_binding`, `unix_sockets`, `upstream_proxy` "never counts as mediated"). `:791-804` (`ResidualKind::UnixSocket`). `backend.rs:448-458` (`unix_allow_unsupported` / `mediated_unsupported` refusals unless the backend enforces them). `backend.rs:682-689` (proxy env vars are "not mediation", so `OPENAI_BASE_URL` alone is never the claim). `crates/hh-env/src/egress_profile.rs:68-89` (`mediated` leaves the flag absent, so `--network none`). `:107-186` (the transactional `declared_deps` admission pattern to mirror). `crates/hh-helper/src/podman.rs:99-123` (`create_args`: `--network`, `-v ws:/work:rw`, `--init`, `… sleep infinity`; `extra_args` appended). `bin/hh_helper.rs:48-90,114-121` (`--network none|bridge` only; `extra_args: vec![]`). `crates/hh-env/src/driver.rs:815-889` (argv built by `podman_helper_args`; the socket-path length note for darwin). ADR-0421 Decision 1–3 (the precedent shape). `rust-toolchain.toml` (1.94, std-only posture).
Retrieved: 2026-11-06T23:12Z
Implication for the spec:
- The profile `egress_profile: model_proxy` mirrors ADR-0421: a transactional admission, and the record stamps `cell_network: none`, `policy_net_mode: mediated`, `model_socket: /run/hh/model.sock`. One socket per slot gives cross-slot isolation, and the socket path itself binds the slot.
- The relay is the container's main process under `--init`. If the relay dies, the container stops, and that is visible as an infrastructure failure.
- The relay must be a static Linux binary for the cell's architecture. Proposed: `RUSTFLAGS="-C target-feature=+crt-static"` std-only build on the GCE Linux host, connecting only to literal addresses (no NSS). This is UNVERIFIED; validate it in the T5 live leg on the B3 image.

Alternatives considered:
- A podman `--internal` network with a proxy container. Rejected: the key would sit in a container, plus extra network setup.
- pasta/slirp host-loopback forwarding. Rejected: it starts from an outbound-capable namespace. Its exact restriction semantics were not read (INACCESSIBLE here: no podman docs were fetched).
Outline delta: EXTENDS OL-15 (an egress admission for paid phases).
Limitations:
- Rootless-podman socket permissions and SELinux labelling on the B3 image are unverified.
- On macOS `podman machine`, a host unix socket bind-mounted through the VM is expected not to work. This is UNVERIFIED. Paid phases run on GCE Linux (A-10), and development tests use the fixture upstream without podman.
- Whether the podman backend's declared capabilities satisfy `enforce_unix` for `allow_listed` is unverified (`backend.rs:448`).

### F6 — Wire surfaces: pass-through per surface with declared mutations, not translation to native `/api/chat`
Claim: The proxy should forward each downstream surface to the same-dialect Ollama endpoint, applying only declared, recorded mutations: auth, `model`, effort, the unsupported-field policy, and `include_usage` on OpenAI streams. It should not translate everything to native `/api/chat`, even though native would expose `prompt_eval_cached_count` per call.
Status: UNVERIFIED (design proposal). Proposed validation: C4's per-harness runner check (HH cell vs Harbor direct) shows no declared-mutation-free difference in request bytes.
Evidence kind: design proposal over C1 F1–F3, F5, F15.
Evidence: C1 endpoint-feature matrix and F3(b) (native gives the cached count, "it costs a translation layer"). C1 F16 (HF's translators changed sampling and developer-role handling, a documented source of confounds). Harbor-direct runs talk to Ollama's compat endpoints (C1 integrations column), so pass-through keeps the HH-cell surface and the Harbor surface on the same upstream dialect.
Retrieved: 2026-11-06T23:13Z
Implication for the spec:
- Translation would make HH's own translator a treatment difference in C4's runner check and in D2's L1 conformance.
- Cache visibility is instead settled from `/api/usage` (F14) and, if the first smoke (Q-X23) shows `usage.prompt_tokens_details.cached_tokens` on OpenAI chat, read per call.
- `stream_options.include_usage` is injected on streaming OpenAI calls, because without it the stream carries no usage. It is recorded as `injected`; a harness that cannot tolerate the extra chunk gets a declared per-harness override, and its spend then falls back to the reservation upper bound labelled `estimate`.
- `max_tokens` is **not** inserted by default; see F10. **REFINES REQ-XC-003.**
- Field policy: strip-with-notice when the value is the provider-default-equivalent (`tool_choice: auto`, `cache_control`, `metadata`); refuse (typed HTTP 400 in the surface's error shape) when the value demands behaviour upstream cannot honour (`tool_choice: required|none|{name}`, `previous_response_id`, `conversation`, `n>1`). **REFINES REQ-XC-012**, which allowed either; this fixes the choice per value class so harnesses that always send `tool_choice: "auto"` still run.
Outline delta: EXTENDS OL-03 (the treatment is the harness, not HH's translator).
Limitations: Whether Ollama ignores or rejects `anthropic-beta` values and unknown fields is unknown (C1 F1 limitation). These are first-smoke checks.

### F7 — `GenParamPolicy` is flat-keyed and strips every non-forwarded key; the proxy needs a path-qualified, per-surface parameter policy
Claim: `GenParamPolicy::apply` filters only top-level keys of a "requested" object and strips everything not in `forward`. Effort lives at nested paths (`reasoning.effort`, `output_config.effort`) and request bodies carry non-parameter members (`messages`, `tools`), so the policy must be amended in place. It needs path-qualified keys, a declared parameter subset per surface (only those paths are subject to forward/strip), and a `pin` set (overwrite-or-insert, recording the requested value). It must not be reimplemented in the proxy.
Status: VERIFIED (current semantics); UNVERIFIED (amendment).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-hosting/src/adapter_b.rs:27-90` (`forward[]`, `default{}`; `apply` loops `req` top-level keys; "Anything else the participant asks for is stripped and logged"). C1 F5 / REQ-XC-002 (pin effort, record requested and applied).
Retrieved: 2026-11-06T23:13Z
Implication for the spec: `GenParamPolicy` gains `pin{path → value}` and `params_scope[path]` per surface. Default lane policy for this program:
- `pin`: `{model, effort-path}`.
- `forward`: every sampling parameter the harness sets (`temperature`, `top_p`, `seed`, `max_tokens`, `stop`, …), so harness defaults stay part of the treatment, per HF's "sampling left unset unless the harness sets it" (C1 F16).
- `default`: none.

The effort level is a slot-declared parameter. Q-X22 decides its value; see Open questions. The adapter record's `ext.gen_param_policy` keeps carrying the canonical JSON, so the debt record stays attached (T-LCD-05).
Outline delta: CONFIRMS OL-07 (effort is a one-variable knob); EXTENDS OL-03.
Limitations: The fixture tests that pin `apply`'s current behaviour were not read. Making the amendment additive (path keys) keeps flat keys meaning what they mean today.

### F8 — Receipts reuse `hh.model_io/1`, the `model.call.*` classes and the fenced deposit pattern; two new measurement classes are needed for provider telemetry
Claim: The existing pattern covers verbatim per-call receipts. The pattern: blobs deposited through the store seam *before* the terminal row that cites them, with `model.call.requested/completed/failed` and `attempt.*` carrying `request_ref`, usage `{raw, view}`, `cost`, `timing{queue_wait_ms, ttft_ms, latency_ms}`, `served_model` and `credential_binding_id`. Only provider-side usage/balance observations and the reconciliation result lack a class. Every blob deposit is bounded at 1 GiB, and rows offload beyond 64 KiB.
Status: VERIFIED (existing machinery); UNVERIFIED (new classes, sizing).
Evidence kind: source inspection + inference (size).
Evidence:
- `crates/hh-embed/src/gateway_model.rs:1-20,1181-1225` (`record_io` builds `hh.model_io/1{model_call_id, request_body, plan_ref, frames, outcome{stop_reason, served_model, usage_raw}}`).
- `crates/hh-control/src/driver.rs:2886-2902` ("deposit through the fenced store seam *now* — before the terminal row that carries `response_ref` lands (CC3 durable-before-visible)").
- `crates/hh-ledger/src/store.rs:58` (`DEFAULT_BLOB_MAX_BYTES = 1 << 30`) and `:1976-2001` (`put_blob` content-addresses, optionally seals via `BlobCipher`, fsyncs the dir).
- `classes.rs:158` (64 KiB offload); `:1716-1723` (`model.call.*`, `attempt.*`, `model.stream.delta` ephemeral); `:1954-1958` (`security.credential.bound/used/revoked`); `:2038-2043` (`control.budget.*`, `exceeded` audit-grade); `:2164` (`measurement.cost.attributed`); `:1460,1733` (`model.profile.probed`); `:1965` (`security.secret.redacted`).
- `crates/hh-gateway/src/events.rs:20-60,190-260,426-432` (payload members).
- `crates/AGENTS.md` ("Adding a class changes a contract; coordinate its ontology home, schema, producer, projection, and acceptance tests").
Retrieved: 2026-11-06T23:14Z
Implication for the spec:
- The receipt is `hh.model_io/1` plus an additive `proxy` member (§1.3), with downstream and upstream raw bytes as separate content-addressed blobs. This is one receipt scheme, not a second one (CC1).
- New classes, both registered in `classes.rs` with ontology home `measurement`: `measurement.provider.usage.observed` (provider usage/balance snapshot: endpoint, query, read time, body blob ref, HTTP status) and `measurement.cost.reconciled` (HH sums vs provider buckets, signed differences, status). Both are ledgered, audit-grade, `kernel_origin = true`.
- The fallback, if H1 prefers fewer contract changes, is to keep both as packet-level records only. That loses ledger ordering against the calls they settle.
- Sizing (inference): C1's illustrative trial of about 2M input tokens is about 8 MB of request bytes at roughly 4 bytes/token, plus responses, so about 10 MB/trial. A 300-trial pilot is about 3 GB on the VM disk (B3 sizing input).
- Receipts are not committed to git. A committed sha256 manifest plus an archive location satisfies REQ-XA-011's "content-addressed by a committed hash manifest". The location is an operator decision (Open questions).
- Redaction: the Ollama key never enters a body or a recorded header by construction. Every blob is `leak_scan`ned against a mask set {key, all slot tokens} before deposit; a key hit refuses the write and stops the phase (fail closed).
- Slot tokens are dead capabilities after revocation. They are stored verbatim in internal receipts and tombstoned (`REDACTED[known_value:…]`) in any published export, which is labelled sanitized, never byte-identical (CONVENTIONS; ADR-0363 D3).

**REFINES REQ-XC-013.**
Outline delta: EXTENDS OL-15 (receipt scaffolding) and OL-08 (spend telemetry has ledger homes).
Limitations: Whether any reader strictly validates `hh.model_io/1` members (which would make the additive member a breaking change) was not searched. Only producers were found. The byte/token ratio is assumed.

### F9 — Durable-before-visible forces buffered release; streaming harnesses need SSE heartbeats; the upstream read deadline needs a non-streaming override
Claim: To keep "every call receipted before the cell sees it", the proxy must buffer the complete upstream response, deposit it, append the terminal row, and only then release the bytes. For a streaming request the proxy commits a `200` head with chunked SSE immediately, sends SSE comment heartbeats (`: hh-proxy\n\n`) every `heartbeat_ms` while buffering, then writes the recorded event sequence. If upstream fails after the head is committed, the proxy emits the surface's in-stream error event. Separately, the gateway's socket deadline (`stream_idle_timeout_ms` = 120 s) would time out a long non-streaming `max`-effort call that sends no bytes, so the lane needs `nonstream_read_deadline_ms`.
Status: VERIFIED (deadline clamp; ephemeral delta class); UNVERIFIED (heartbeat tolerance per harness).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-gateway/src/http_transport.rs:271` (`rw_ms = clamp(stream_idle_timeout_ms, 30_000, 600_000)`); `dialects.rs` `stream_idle_timeout_ms: 120_000`. `classes.rs:1722` (`model.stream.delta` is `Ephemeral`: the native path shows deltas before the terminal row, which is acceptable *inside* the kernel but not for an external participant whose actions on unreceipted bytes could never be audited). C1 F11(d) (120 s may be short for `max` effort). C1 F1/F11 (Ollama's own mid-stream errors arrive as error objects with status 200).
Retrieved: 2026-11-06T23:15Z
Implication for the spec:
- The release mode is lane-declared: `buffered` (non-streaming) or `buffered_heartbeat` (streaming). A `streamed_with_journal` mode, which forwards deltas while journaling, is explicitly **not admitted** for the program.
- Recorded latency fields (`first_byte_ms` upstream vs `released_ms`) let D1 report model latency separately from release latency.
- Harness timeouts while queued or buffering are an infra-failure risk that C4 must check per harness. The pilot measures it.
Outline delta: EXTENDS OL-03 (wall-time quote must include buffering).
Limitations: SSE comments are part of the SSE grammar, but each harness's client tolerance is untested. Buffering denies harnesses incremental tokens, which could change behaviour in harnesses that act on partial streams. None of the surveyed text-protocol harnesses is known to (E1/C3 to confirm).

### F10 — Budget: `Account` fits as is when the phase is one run with per-slot slices; reservation must be a strict worst case
Claim: `Account::reserve` checks the whole quantity vector along the ancestor chain (`level + requested > hard` refuses and ledgers `reserved{refused}`), and `charge` consumes reservations. `exhaust` is run-level: it emits `control.budget.exceeded`, an escalation and `control.decision{stop}`, refused while effects are in flight. Hence:
- (a) one phase proxy run, with the root `pool` = the authorized phase cap and one `slice` per slot (`BudgetScopeKind::AgentProcess`);
- (b) slot budget exhaustion is a **reserve refusal**, never `exhaust`, because `exhaust` would stop the whole phase;
- (c) `exhaust` fires only when the root is exceeded;
- (d) the per-call reservation is a strict worst case.
Status: VERIFIED (Account semantics); UNVERIFIED (the composition and bound).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-budget/src/account.rs:1-27` (E1–E5; all mutations appended under the run's writer lease). `:200-230` (`allocate(parent, scope, spec)`; slices move ceilings from the parent). `:300-375` (`reserve`). `:389-420` (`charge`: excess claim released, over-claim charged `over_reservation = true`, "never refused"). `:512-533` (`release`). `:539` (`complete`: slice remainder returns to parent). `:606-680` (`exhaust`). `:940-985` (`attribute_spend` appends `measurement.cost.attributed`; idempotent on source event). `spec.rs:264-284,405-425`. `crates/hh-ontology/src/dimensions.rs:205,384` (spend charged in micro-units). `crates/hh-hosting/src/service.rs:930-945` (the hosted session's `exhaust` closes `session.closed` then `budget_exhausted{dimension}`). C1 F2/F10 (no `count_tokens`, no provider cap).
Retrieved: 2026-11-06T23:16Z
Implication for the spec:
- Bound per call: `reserve{spend: ceil(rate_in·(B + O) + rate_out·M), model_calls: 1}`, where:
  - `B` = downstream body UTF-8 bytes. Byte-level BPE gives at most one token per byte; this is UNVERIFIED for GLM's tokenizer.
  - `O` = the declared `template_overhead_tokens` (default 2048, to be recalibrated from first-smoke usage).
  - `M` = the model output cap (131 072 per the Z.ai guide via C1) unless the first smoke shows `max_tokens` bounds reasoning plus visible output on Ollama, in which case `M = min(max_tokens, cap)`.
  - Rates are the uncached input rate and the output rate.
- At glm prices the worst case for a 200 KB request is about $0.03 input + $0.066 output. The absolute worst is about $0.22 (C1).
- After each call, `reported input_tokens ≤ B + O` is checked. A violation sets `reservation_bound_violated` and halts new admissions phase-wide, because the safety invariant is gone.
- Reserve-before-queue: a queued call holds budget, so the queue can never overcommit, and a refusal is immediate.
- The settle sequence runs uninterrupted in the ledger actor: `attribute_spend` → `charge(model_calls, tokens…)` → `release(remainder)`.
- Slot caps must exceed the expected per-trial P90 plus one worst-case call, or the tail refuses calls that would actually fit. Every refusal is recorded with `requested`/`available`, so D1 can tell premature refusals from real exhaustion.
- **REFINES REQ-XC-003**: `max_tokens` insertion becomes optional (`max_tokens_policy: reserve_model_cap` by default), because inserting it is a treatment difference vs the Harbor surface.
Outline delta: EXTENDS OL-08 (per-model ceilings = phase root, per-slot slices).
Limitations: The exact interaction between `charge(…, reservation_id)` releasing excess claims on other dimensions and a subsequent `attribute_spend` was not traced line by line. The settle ordering is to be pinned by a conservation property test (T4). Concurrent reservations at the worst case can make the slot tail refuse early; this is a cost of the strict invariant and is reported, not hidden.

### F11 — Three pricing-unit conventions coexist; a test comment shows the µ$/token unit is misread; `PricingTable` has no codec and no committed rows, so migration is test-only
Claim: `PricingRow` rates are "micro-units per token" (i64), so $0.15/M = 0.15 and is unrepresentable (C1 F14). The router's `DeclaredPricing`/`PriceRate` uses **µUSD per 1k tokens** ("as `pricing.json` spells"), and `pricing.json` uses float USD per 1k. In `tests/r4_5d.rs` the comment "1000 micro/token ⇒ $1/1M" is wrong under the declared unit (1000 µ$/token is $1000/M) and right only under per-1k. `PricingRow` has no JSON codec and no non-test construction site, so no durable rows exist to migrate.
Status: VERIFIED.
Evidence kind: source inspection.
Evidence:
- `crates/hh-budget/src/pricing.rs:301-327` (`input_uncached: i64` "Micro-units per … token") and `:550-618` (`price`: `micro = r * qty`). The `to_json` impls in `pricing.rs` exist only at l.161, 214, 279, 422 (Confidence, Derivation, PricingTableRef, SpendRow; no `PricingRow`).
- `crates/hh-gateway/src/router.rs:1654-1662` (`PriceRate{prompt_per_1k_uusd, completion_per_1k_uusd}`) and `:1772-1787` (`/1000`, floor).
- `docs/build/reports/live/pricing.json` (`prompt_per_1k_usd: 0.0001` for Haiku 5.5).
- `crates/hh-gateway/tests/r4_5d.rs:1142-1159`.
- `PricingRow {` constructions only in `crates/hh-budget/tests/{golden_usage,r5_13,account}.rs` and `crates/hh-gateway/tests/r4_5d.rs` (grep).
- `spec/CANONICAL_SPEC.md:8720` (the `PricingTable` row names roles, not the rate unit); `:5807` (Money is `micro_units` integers).
Retrieved: 2026-11-06T23:17Z
Implication for the spec: Minimal amendment (ADR-C2-B):
- Add `PricingTable.rate_unit ∈ {micro_per_token, micro_per_1k_tokens}`, defaulting to `micro_per_token` so every existing row and test keeps its meaning. `micro_per_1k_tokens` aligns `hh-budget` with the router and `pricing.json` convention (CC1: one spelling across the repo's pricing views, not a new per-1M unit).
- glm-5.3-flash = 150 / 30 / 500 µ$ per 1k (input / cached / output), with `valid_from` and the C1 source.
- One rounding rule: the exact nano-sum `Σ rate·qty` is computed per call, then `money.micro_units = ceil(sum / 1000)`. `roles` itemizes `floor` per role, and a declared `rounding` role carries the remainder, so roles sum exactly to money.
- A rate not integral at the declared unit is `NoPrice{unrepresentable_rate}`, never rounded.
- `Money` and the spend dimension are untouched (§5807 holds).
- Migration: none for durable data. Fix the r4_5d comment, and add a golden test reproducing a hand-computed glm trial to the µ$ (REQ-XC-008).
Outline delta: CORRECTS OL-08 (the budget record *can* carry these prices after a one-member amendment; no schema-wide migration).
Limitations: Rates below $0.001/M are unrepresentable at per-1k. No current Ollama rate is. If one appears, a later ADR adds `micro_per_1m_tokens`.

### F12 — The "$0 rule" is hard-coded in four round-5 evaluators; amend by adding an admitted-spend leg, not by editing frozen recomputes
Claim: `study.rs::evaluate_costs` refuses any packet with `exposure_micros ≠ 0` and pins 14 packets / 12 / 90 subjects. `activation.rs::evaluate_costs`, the pilot leg (`study.rs:829`) and `publication.rs::evaluate_carried` each require exposure `0`. They recompute committed round-5 history and must stay byte-stable. Paid phases need a new costs evaluator that passes admitted spend and still fails unadmitted spend.
Status: VERIFIED (current rules); UNVERIFIED (new leg).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-env/src/study.rs:1466-1505`; `:820-835`; `activation.rs:285-320`; `publication.rs:367-410`; tests naming them: `crates/hh-env/tests/{r5_46,r5_49,r5_54,r5_55}.rs` (grep). A-1 §4 ("an unexecuted study is unmet, not null").
Retrieved: 2026-11-06T23:17Z
Implication for the spec: Add `evaluate_admitted_costs(AdmittedCostFacts)` in `hh-env` (a new leg, `hh-study-report` additive). Facts:
- `per_packet[{packet, exposure_micros, admission_ref?, cap_micros?, settlement_status}]`
- `admissions[{ref, authorization_verbatim_ref, cap_micros, phase}]`
- `program_cap_micros`

Verdicts:
- `exposure = 0` → ok.
- `exposure > 0` without `admission_ref` → `Refused("unadmitted_spend:<packet>")`.
- `exposure > cap` → `Refused("over_cap:<packet>")`.
- Σ over the admission > its cap → refused.
- `settlement_status ∈ {pending}` → `Inconclusive("settlement_pending")`.
- `settled_discrepant` beyond the declared tolerance → `Refused("settlement_discrepant")`, unless an operator-recorded acceptance ref is present.

The round-5 functions stay untouched, which keeps the R5.59 recompute byte-stable.
Outline delta: EXTENDS OL-15 (budget records → a study leg that can pass).
Limitations: B2/H1 decide where the new leg is wired (which report version).

### F13 — Identity: `ModelSnapshotRecord` can honestly carry Ollama's catalog coordinate; drift and "same participant" must be declared-coordinate claims
Claim: `ModelSnapshotRecord{provider, model_id, snapshot_id?, pinned, observed_fingerprint?, provenance}` with a strict codec can carry `snapshot_id = "tags:0c238824336d@2026-08-26T14:00:00Z"`, `pinned = false`, `observed_fingerprint = None` and `provenance = reported`. The full `/api/tags` entry and `/api/show` body travel as a blob on `model.profile.probed`. `observe()` flags a substitution on any byte inequality of `served_model` vs `model_id`. Ollama's response spelling is unobserved (C1 F15), so an un-normalized check could raise a false alarm.
Status: VERIFIED (record/codec/observe); UNVERIFIED (normalization, identity class).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-gateway/src/snapshot.rs:108-128` (record); `:190-212` (unknown members refuse); `:336-378` (`observe`: `served_substituted = s != pinned.model_id`); `:40-53,92-98` (fingerprint probe and verdict; C1 F12 says not to run it here); `crates/hh-embed/src/gateway_model.rs:98-130` (`substitution_allowed`); `crates/hh-eval/src/central_protocol.rs:206-216,637-649` (support gate); C1 F4, F12, F13.
Retrieved: 2026-11-06T23:18Z
Implication for the spec:
- **Catalog identity tuple** `{name, tags_digest, modified_at (compared as an instant, not a string), parameter_count, quantization_level, context_length, thinking{values, default}}` is probed at phase start, at each slot-batch boundary, at phase end and at every phase boundary.
- Any tuple change → a `SnapshotClaim`, which stops collection (REQ-XC-009). Fields outside the tuple are recorded descriptively and never alarm.
- **Served model**: the lane declares `served_model_expected[]`, fixed from the first smoke's verbatim spelling and recorded with that evidence. A per-call mismatch outside the set is the existing terminal `served_model_mismatch`.
- **Support declarations** (from C1; C2 adds `support_route = true` only because the route is the HH lane): `{route: true, cache: false, seed: unverified→false, fingerprint: false}`. They are carried on the participant record. The §4.1 gate amendment belongs to D1/D2.
- **"Same participant"** = identical `{lane_ref (proxy version + Ollama endpoint host + dialect), model_id, catalog tuple, pinned effort, gen_param_policy hash}` across the comparison window. Its class is `participant_identity: catalog_declared`. It does not establish identical served weights, numerics, region or batch context.
Outline delta: EXTENDS OL-15 (model admission record); feeds D1/D2 (OL-06 not owned here).
Limitations: The provider spelling `provider:ollama-cloud` needs a registry provider record (N7) that was not looked up.

### F14 — Reconciliation: HH-metered spend may under-run Ollama's bill, so the root cap needs a settlement margin; hourly bucketing is ambiguous at boundaries
Claim: Enforcement uses HH's meter, but Ollama's bill is the authority (ADR-0363 D4 posture). HH can under-count relative to the bill: Anthropic-compat counts are "tokenizer approximations", and cached-rate application per route is unverified. The phase root cap should therefore be `authorized × (1 − settlement_margin)`, with a declared margin (proposed 10 %) revisited after the first reconciliation. Settlement compares whole-window totals first and hour buckets second. Calls within ±60 s of an hour boundary are flagged `boundary_ambiguous`, and both counting conventions (all dispatched vs successful only) are reported.
Status: UNVERIFIED (design proposal over C1 F8, F10 and ADR-0363 D4).
Evidence kind: design proposal + source inspection.
Evidence: C1 F10 (hourly within 24h, lag, no per-key breakdown, 10 req/min shared), F8 (period reset 2026-11-22T16:30:34Z; included credit first), F3 (per-route usage fields). `docs/adr/ADR-0363…md` D4 ("provider consoles' billing is the authority; the operator reconciles the ledger total"). `docs/build/reports/live/pricing.json` (`authority` member).
Retrieved: 2026-11-06T23:18Z
Implication for the spec:
- The telemetry poller in the proxy (the key holder) reads `/api/balance` before and after each phase, and `/api/usage?range=24h&group_by=model` after the last call until two reads at least one minute apart agree (REQ-XC-005/006). Every read becomes `measurement.provider.usage.observed`.
- A `measurement.cost.reconciled{window, hh{requests, input_total, cache_read, output_total, micro_units}, provider{…}, diff{signed}, status}` row is appended. Status values: `settled` (within tolerance = max(2 %, $0.05)), `settled_discrepant`, `pending`, or `unreconcilable{non_exclusive | straddles_period_reset | hourly_window_expired}`.
- Balance deltas are invalid across the reset boundary, so usage is used alone there.
- Exclusivity (Q-X21) is a precondition for `settled`. Without it the provider figures bound HH's from above only.
- **REFINES REQ-XC-005** (tolerance, boundary rule, status vocabulary) and **REQ-XC-004** (margin).
Outline delta: EXTENDS OL-08 (spend telemetry = HH per-call + provider-settled + the recorded gap).
Limitations: Ollama's bucket assignment rule (request start vs completion) and whether failed (429) requests count are unknown. Both conventions are reported until a reconciliation shows which matches.

### F15 — Replay/record: one recording format (raw upstream exchanges) serves the proxy's replay and a native `RecordedTransport`; the canonicalization hook is an interface only
Claim: A recording is the ordered set of raw upstream exchanges of a slot, plus the downstream request bytes, from the receipts in §1.3. In `replay` mode the proxy has no upstream transport. A `RecordedUpstream` implementing the same upstream port serves the recorded response bytes for key `(slot_recording, k)` and captures the harness's request. A matching `RecordedTransport: hharness_gateway::Transport` decodes the same raw bytes through the existing translators into `WireFrame`s for HH-native runs. D2 can then drive both the original harness and its HIR port against one recording.
Status: UNVERIFIED (design proposal).
Evidence kind: design proposal over source inspection.
Evidence: `crates/hh-gateway/src/bin/hh-gateway-stub.rs:1-60` (`FakeTransport` replays recorded frames through the same decode/attempt path: the precedent). `crates/hh-gateway/src/gateway.rs:31-78` (`Transport` port). `crates/hh-ledger/src/branch.rs:268-281` (`ReplayMode{Exact, Structural, Observational, …}`) and `replay.rs:1-27` (validity ladder: replay of kernel runs is ledger-driven and distinct from this boundary replay). C1 F6(b), F12(d).
Retrieved: 2026-11-06T23:19Z
Implication for the spec: The interface C2 owns:
- `mode ∈ {live, record, replay}`. `record` = `live` plus a sealed `hh.model_proxy.recording/1` manifest `{slot, calls[k → {downstream_sha256, upstream_req_sha256, upstream_resp_ref}]}`. `replay` has no upstream port and spends $0 structurally.
- `keying ∈ {sequence, sequence_then_hash}`. The latter matches concurrent calls by canonical request hash among outstanding recorded calls.
- `trait RequestCanonicalizer { fn id(&self) -> &str; fn canonicalize(&self, surface, bytes) -> Result<Canonical{bytes, dropped_paths[]}, Refusal>; }`. Built-ins: `identity` (byte equality) and `json_canonical` (hh-wire canonical JSON). D2 registers its rules as further canonicalizers.
- Match verdict `{exact | canonical_equal | diverged{first_diff_path}}`, recorded per call.
- `on_divergence ∈ {stop, serve_recorded}`. `stop` returns a typed HTTP 409 with disposition `replay_diverged` and ends the slot.
- `latency ∈ {immediate, recorded}`.

C4 reuses `record` for its runner check. Every C2 acceptance test uses `replay` over synthetic recordings as its fixture upstream, so no live calls are made.
Outline delta: EXTENDS OL-15 (parity scaffolding shared by D2/C4).
Limitations: The canonicalization rules, admissible divergences, and the claim class a replayed run earns belong to D2. Replay is boundary-level only and cannot reproduce cell-side nondeterminism.

### F16 — Failure taxonomy: no proxy retries by default; upstream statuses pass through verbatim; proxy-originated refusals are non-retryable typed 4xx; crashes settle bounded
Claim: The chat-completions rules map 400/401/403/404/422/429/500/503/529 but not 502. The kernel status table maps any 5xx to `ServerError`, which is transient. Harness SDKs typically retry 429/5xx themselves (their behaviour is not read here; C4 verifies it). The proxy therefore sets `max_attempts = 1`, so each harness retry is its own receipted call and nothing is retried silently. It passes upstream error statuses, bodies and `Retry-After` through verbatim. It answers its own refusals with surface-shaped, non-retryable 4xx bodies.
Status: VERIFIED (status tables, retry defaults); UNVERIFIED (status choices, harness reactions).
Evidence kind: source inspection + design proposal.
Evidence: `crates/hh-gateway/src/dialects.rs:168-210` (status rows); `codec.rs:148-163` (`kernel_status_class`); `vocab.rs:259-320,425-456` (`ModelErrorClass`, `default_retry_class`); C1 F11 (429/502, queue-full, mid-stream error objects); `crates/hh-ledger/src/account.rs:1-27` (E1: exhaust refused while effects are in flight).
Retrieved: 2026-11-06T23:19Z
Implication for the spec: The taxonomy.

| Event | Cell sees | Receipt `disposition` / class | Slot consequence (runner, D1 policy) |
|---|---|---|---|
| upstream 429 (+`Retry-After`) | verbatim 429 | `upstream_error` / `rate_limited` | infra; never a task failure |
| upstream 500/502/503/529 | verbatim | `upstream_error` / `server_error` or `overloaded` | infra |
| connect/DNS/TLS failure | synthesized 502, surface error shape `hh_proxy_upstream_unreachable` | `upstream_error` / `network` | infra |
| upstream timeout (connect 5 s; stream idle; non-stream deadline) | synthesized 504 | `upstream_timeout` | infra |
| partial or broken stream, buffered | synthesized 502 `stream_broken`; partial bytes kept in blob | `stream_broken` | infra |
| partial stream after heartbeat head | in-stream surface error event, then close | `stream_broken` | infra |
| slot budget refusal | 402, `hh_budget_exhausted{scope: slot}` | `refused_budget` + `reserved{refused}` | `budget_exhausted{spend, slot}` (session closed by hosting service) |
| phase stopped | 402, `hh_budget_exhausted{scope: phase}` | `refused_phase_stopped` | `budget_exhausted{spend, phase}` |
| field policy refusal | 400, `hh_proxy_refused{path}` | `refused_policy` | harness-caused; recorded, D1 classifies |
| bad/missing token | 401 | `refused_auth` | infra (configuration) |
| unknown route | 404 | `refused_route` | recorded only |
| replay divergence (`stop`) | 409 | `replay_diverged` | conformance finding (D2) |
| proxy crash mid-slot | connection reset at relay | `proxy_aborted` (synthesized at recovery) | infra{proxy}; reservation settled `bounded{0, reserved}` |

Infra classification is a recorded attribute. Reattempt bounds belong to the runner and D1 (declared, finite). The proxy never re-dispatches a call itself.
Outline delta: EXTENDS OL-03 (infra-failure missingness inputs).
Limitations: 402 is a proposal chosen because common SDKs are not expected to retry it. Each admitted harness's reaction to 402/409/heartbeats is a C4 acceptance item.

### F17 — Kernel-native lanes (Phase 3/4) should also route through the proxy: one semaphore, one pin, one capture point
Claim: Ollama's 3-concurrency limit, effort pinning, receipts and reconciliation are account-level. If HIR-native runs used `GatewayModel` directly to Ollama while original harnesses used the proxy, there would be two semaphores (risking more than 3 in flight), two pin implementations, and asymmetric request capture for L1. Proposal: the proxy also offers a token-gated loopback TCP listener, and HH-native lanes point their `endpoint_ref` there with the Ollama dialect document.
Status: UNVERIFIED (design proposal). D2/E2 decide adoption.
Evidence kind: design proposal over source inspection.
Evidence: `crates/hh-embed/src/gateway_model.rs:98-130,306-312` (`ModelLane{endpoint_ref, credential_ref}`; admitted dialects are closed); `http_transport.rs:1-20` (`http://` endpoints admitted); C1 F11 (3 concurrent per account).
Retrieved: 2026-11-06T23:19Z
Implication for the spec: Native runs then carry two receipt rows per call: the native run's `model.call.*` and the proxy run's. They are linked by an `x-hh-model-call-id` request header the proxy records. D2's L1 compares upstream request bytes captured at one point after identical mutations.
Outline delta: EXTENDS OL-15.
Limitations: Adding the Ollama dialects to `ADMITTED_DIALECTS` is an ADR-0366 amendment (counted in ADR-C2-E).

---

## 3. Amendments and ADRs this design requires

| ID (candidate) | Subject | Spec / record sections touched | Why an ADR |
|---|---|---|---|
| **ADR-C2-A** | Model-mediation proxy: placement in `hharness-hosting` (module + `hh-model-proxy` bin), the new internal edge hosting → gateway, phase proxy run topology, buffered release modes, `hh.model_io/1` `proxy` member, the two new classes `measurement.provider.usage.observed` and `measurement.cost.reconciled` (registered in `classes.rs` with hosted lowering `hint`), relocation of `token_matches`/`mint_token` | canonical §6.6 §5 (Adapter B: "proxy lives inside the environment" becomes in-environment *endpoint*, lab-host core); §5b.1 (gateway as sole emitter, with the proxy emitting through gateway builders); §5a.1 class table | new architecture edge and binary; contract change (classes); a reading of §6.6 that needs recording |
| **ADR-C2-B** | `PricingTable.rate_unit` (`micro_per_1k_tokens`), one rounding rule, `NoPrice{unrepresentable_rate}` | canonical §8.2 `PricingTable` row (l.8720); ADR-0039 D4 lineage; ADR-0367 D3 (convention alignment) | durable schema semantics (CC7 if a codec is added) |
| **ADR-C2-C** | `model_proxy` egress profile: helper `--model-socket`, relay binary in `hharness-helper`, `mediated` + `unix_socket` residual + `local_binding` | canonical §5g.4 (net modes, residuals), §5d.5 (helper argv); precedent ADR-0421 | new executor variant admission, as for ADR-0421 |
| **ADR-C2-D** | `GenParamPolicy` path-qualified `pin`/`params_scope` | §6.6 §5 (declared generation-parameter policy) | changes a declared adapter parameter's semantics |
| **ADR-C2-E** | Ollama upstream lane dialects (`ollama-chat@1`, `ollama-messages@1`, conditional `ollama-responses@1`), auth form from `auth_kinds` not family, raw forward leg | ADR-0366 (hosted dialects), ADR-0364 D1 (one codec home) | admitted-dialect set and credential wire form change |
| supplement amendment | the admitted-spend study leg (F12) and the slot cost record (§1.5) | the new experiments supplement (written by O1), not the frozen round-5 supplement | stays out of the frozen evidence-and-adoption spec |
| not C2's | §4.1 support-gate amendment for stochastic participants | central protocol | owned by D1/D2 (C1 handoff (b)) |

No new external dependency is required. Every piece reuses `hh-wire`, `hh-tls`, `hh-gateway`, `hh-budget`, `hh-ledger`, `hh-secrets` and std.

## 4. Ticket shaping hints (one fresh-context `implement-spec` run each; all acceptance offline)

1. **PRICE: rate unit.** Scope: `hh-budget` `rate_unit`, rounding, `NoPrice{unrepresentable_rate}`, glm row fixture, r4_5d comment fix. AC: golden trial priced to the µ$; existing tests unchanged; refusal on a non-integral rate. Small.
2. **LANE: Ollama dialects + raw forward leg.** Scope: dialect docs, `forward_raw`, auth-from-`auth_kinds`, chunked response writer and reason phrases in `hh-wire`. AC: a fixture TCP upstream (std `TcpListener` thread) asserts byte-faithful bodies, Bearer on the Messages family, SSE and JSON captured raw; decoders on captured bytes equal the frames path.
3. **PROXY-CORE: server, admission, receipts, release.** Scope: `hharness-hosting::proxy` + `hh-model-proxy` bin, per-slot unix listeners, token gate, route table, `GenParamPolicy` path amendment, ledger actor, `hh.model_io/1` `proxy` member, buffered and heartbeat release, crash recovery. AC: replay-mode fixture upstream; the terminal row and blobs are durable before the first released byte (kill-point test); `leak_scan` finds no key in any blob; effort `medium` arrives as the pinned level, with both recorded; `tool_choice:auto` stripped and `required` refused. Largest; split 3a (server, gate, receipts) and 3b (release modes, recovery) if the size trigger fires.
4. **BUDGET: reservation, slices, concurrency.** Scope: phase run root and slot slices, worst-case bound, reserve-before-queue, semaphore ≤3, settle ordering, root `exhaust`, bound-violation halt. AC: a property test proving `consumed + reserved ≤ hard` at every prefix under 3 concurrent slots with randomized responses; the slot refusal returns 402 and leaves the phase running; root exhaustion stops the phase; never more than 3 upstream calls in flight.
5. **EGRESS: `model_proxy` profile + relay.** Scope: helper flag, `admit_model_proxy` (transactional, mirroring `admit_declared_deps`), `hh-model-relay` bin, argv tests. AC: argv golden; refused admissions leave the policy byte-identical; a relay unit test over a socketpair. Live leg on GCE (`podman` cell can reach only the socket) gated `#[ignore = "live — …"]`.
6. **REPLAY: record/replay modes + canonicalizer hook + `RecordedTransport`.** AC: a recorded synthetic slot replays byte-identically; a divergence at k=3 stops with 409 and `first_diff_path`; native `RecordedTransport` frames equal the proxy-served bytes decoded.
7. **RECONCILE: telemetry poller, reconciliation, slot cost fold, admitted-spend leg.** AC: a rate-limiter test (≤1/min combined; `Retry-After` honoured); injected-lag fixture gives `pending` then `settled`; a mismatch gives `settled_discrepant` that is recorded and not hidden; `evaluate_admitted_costs` refuses unadmitted spend, passes admitted spend, and leaves the round-5 tests unchanged.
8. **SMOKE (paid, operator-gated, Q-X23).** Executes C1 open question 5 through the built proxy, plus served-model spelling, the OpenAI cached-count field, the `max_tokens`-bounds-reasoning check, and the input bound ratio. ≤ $0.05; `live-executed` only via `scripts/live-verify.sh` (ADR-0363).

Order: 1 ∥ 2 → 3 → 4 → (5 ∥ 6 ∥ 7) → 8. ADR-C2-A lands with 3, B with 1, C with 5, D with 3, E with 2.

---

## Answer and completion assessment

The row's question is how adapter B becomes real, so that external harnesses in HH cells reach `glm-5.3-flash` only through an HH proxy with receipts, budgets, identity and reconciliation. It is answered at design level:

- **Placement:** a hosting-tier proxy binary that depends downward on the gateway and is spawned by the runner (F1, F2).
- **Reachability:** `--network none` cells reach it only through one bind-mounted unix socket and an in-cell byte relay, under the existing `mediated` + `unix_socket` vocabulary (F5).
- **Forwarding:** pass-through per surface with recorded mutations (F3, F6, F7).
- **Receipts:** verbatim, durable before release (F8, F9).
- **Budgets:** enforced by strict worst-case reservation against a phase-root and slot-slice tree (F10), priced after a one-member pricing amendment (F11).
- **Paid studies:** pass through a new admitted-spend leg (F12).
- **Identity:** a declared catalog coordinate (F13), reconciled against Ollama's telemetry with a settlement margin (F14).
- **Replay:** one record/replay format for D2 and C4 (F15), with a closed failure taxonomy (F16).

**Settled:** layering and dependency direction; the reuse map (no new dependency); receipt format and ordering; the budget composition; the pricing amendment shape; the $0-rule amendment shape; the replay interface; the ADR register; the ticket seams.

**Acceptable uncertainty, resolved by named validations:**
- the relay's static build and socket permissions on the B3 image (T5 live leg);
- harness tolerance of buffered release, heartbeats and 402 (C4);
- the byte-bound on input tokens and whether `max_tokens` bounds reasoning (T8 smoke);
- the OpenAI cached-count field and the served-model spelling (T8 smoke).

**Technical blockers:** none for building. For paid runs: the pricing amendment (T1) and the budget machinery (T4) are prerequisites, and Q-X21/Q-X22 must be answered before the pilot gate.

## Open questions

1. **Effort pin (Q-X22, pending).** Owner: operator, with D1. Units: C2, D1, D3. Operator decision: yes.
   - C2's design takes the level as a slot parameter, so any answer is implementable.
   - Recommendation for the record: pin `high` for Phase 2, for cost (C1: `max` ≈ 1.9× reasoning tokens) and comparability with HF's `high` column.
   - Make `max` a Phase-4 factor level.
2. **Account exclusivity and overage (Q-X21, pending).** Owner: operator. Units: C2, D1, H1. Operator decision: yes. `settled` reconciliation requires exclusivity; otherwise the status vocabulary records `unreconcilable{non_exclusive}`.
3. **Settlement margin on the root cap.** Owner: C2/H1. Units: D1, H1. Operator decision: yes, at the pilot spend gate. The proposal is 10 % of the authorized amount withheld from the HH hard cap until the first reconciliation shows the meter-vs-bill gap.
4. **Raw receipt archive location.** Owner: operator/B3. Units: B3, A2-style records, F2. Operator decision: yes. Options: a GCS bucket (a cost and provisioning act) or retention on the VM until the publication gate. Either way a committed sha256 manifest is required.
5. **Responses surface.** Owner: C3/C4. Unit: C2 (T3 scope). Operator decision: no. Build it only if a Responses-only harness (Codex) is admitted.
6. **402 / heartbeat / buffering compatibility per harness.** Owner: C4. Operator decision: no.
7. **Native lanes through the proxy (F17).** Owner: D2/E2. Operator decision: no.

## Spec requirements emitted

| ID | Requirement | Rationale (findings) | OL | Proposed acceptance |
|---|---|---|---|---|
| REQ-XC-101 | The model proxy SHALL be implemented in the hosting tier (`hharness-hosting::proxy` + an `hh-model-proxy` binary) depending only downward, and SHALL be launched by the runner as a separate process controlled over a Json channel; no workspace crate SHALL take a normal dependency on `hharness-hosting`. | F1, F2 | OL-15 | `scripts/check-removability.sh` passes; a control-channel integration test |
| REQ-XC-102 | Paid-phase harness cells SHALL run with `net.mode = mediated` on `--network none`, reaching the model only through one per-slot bind-mounted unix socket and an in-cell byte relay, admitted transactionally as `egress_profile: model_proxy` with a declared `unix_socket` residual (owner, closer) and `local_binding = true`; proxy environment variables SHALL NOT be claimed as mediation. | F5 | OL-15 | Argv golden + refused-admission byte-identity tests; gated live leg showing that no other destination is reachable |
| REQ-XC-103 | The proxy SHALL forward each downstream surface to the same-dialect Ollama endpoint (pass-through) and SHALL apply only declared mutations (auth, `model`, effort, field policy, `include_usage`), each recorded with its JSON path and requested/applied values. | F3, F6 | OL-03, OL-15 | A fixture upstream proves upstream bytes = downstream bytes + recorded mutations |
| REQ-XC-104 | Upstream forwarding SHALL use a raw forward primitive on the gateway's HTTP transport (same dial, TLS and `hh-wire` codec), and the auth header form SHALL derive from the lane's declared `auth_kinds` (Bearer for every Ollama surface). **REFINES REQ-XC-001.** | F3 | OL-15 | Messages-family request reaches the fixture upstream with `Authorization: Bearer` and no `x-api-key` |
| REQ-XC-105 | The supported surfaces SHALL be OpenAI chat completions and Anthropic messages (streaming and non-streaming), `GET /v1/models` forwarded, `count_tokens` answered with a typed refusal, and OpenAI Responses (stateless) only when an admitted harness requires it; every other route SHALL be refused and receipted. | F6, wire matrix | OL-03 | A per-route fixture matrix |
| REQ-XC-106 | Field policy SHALL strip-with-notice provider-default-equivalent values (`tool_choice:auto`, `cache_control`, `metadata`) and SHALL refuse with a typed, surface-shaped 400 any value upstream cannot honour (`tool_choice` other than auto, `previous_response_id`, `conversation`, `n>1`, logprobs). **REFINES REQ-XC-012.** | F6 | OL-15 | A per-field fixture matrix |
| REQ-XC-107 | The generation-parameter policy SHALL support path-qualified `pin` and per-surface `params_scope`; the program lane SHALL pin `model` and the effort path, forward harness-set sampling parameters, and inject no defaults. **REFINES REQ-XC-002.** | F7 | OL-03, OL-07 | Harness sends `medium` at `output_config.effort` → upstream carries the pinned level; receipt shows both |
| REQ-XC-108 | Each call receipt SHALL be an `hh.model_io/1` blob with the additive `proxy` member of §1.3, with raw downstream and upstream bytes as separate content-addressed blobs, deposited with the terminal `model.call.*` row through the phase run's fenced writer **before** any response byte reaches the cell. **REFINES REQ-XC-013.** | F8, F9 | OL-15 | Kill-point test: no released byte without a durable terminal row and blobs |
| REQ-XC-109 | Every receipt blob SHALL pass `leak_scan` against a mask set containing the Ollama key and all slot tokens before deposit; a key hit SHALL refuse the write and stop the phase; published exports SHALL tombstone slot tokens and be labelled sanitized. | F8 | OL-15 | Leak-injection test |
| REQ-XC-110 | Release SHALL be `buffered` for non-streaming and `buffered_heartbeat` (SSE comments every declared interval, surface error event on late failure) for streaming requests; forwarding deltas before the receipt is durable SHALL NOT be admitted. | F9 | OL-03 | Fixture timing test; heartbeat cadence test |
| REQ-XC-111 | Non-streaming upstream calls SHALL use a lane-declared read deadline independent of `stream_idle_timeout_ms`. | F9 | OL-03 | Slow fixture upstream (> 120 s simulated clock) completes |
| REQ-XC-112 | Each paid phase SHALL hold one proxy run whose root budget `pool` equals the authorized phase cap minus the declared settlement margin, with one `slice` per slot; slot exhaustion SHALL be a ledgered reserve refusal (never `exhaust`), and `control.budget.exceeded` SHALL be emitted only by root exhaustion. | F10, F14 | OL-08 | Slot refusal leaves the phase running; root exhaustion stops it with exceeded + decision rows |
| REQ-XC-113 | Every upstream call SHALL be preceded by a reservation of `ceil(rate_in·(body_bytes + template_overhead) + rate_out·max_output_bound)`, reserved before queueing; `max_tokens` insertion is optional and recorded. **REFINES REQ-XC-003.** | F10 | OL-08 | Property test: `consumed + reserved ≤ hard` at every ledger prefix under 3 concurrent slots |
| REQ-XC-114 | After each call the proxy SHALL check reported input tokens against the reservation bound and SHALL halt new admissions phase-wide on a violation (`reservation_bound_violated`). | F10 | OL-08 | Fixture usage above the bound halts admission |
| REQ-XC-115 | The proxy SHALL hold at most the declared upstream concurrency (≤ 3) across all slots and native lanes, queue FIFO in HH, and record `queue_wait_ms` per call. **REFINES REQ-XC-011.** | F10, F17 | OL-03 | Concurrency test never exceeds 3; queue wait recorded |
| REQ-XC-116 | `PricingTable` SHALL carry `rate_unit ∈ {micro_per_token, micro_per_1k_tokens}` (default the former), compute the exact per-call nano-sum, round once (ceil to µ$), itemize roles by floor plus a `rounding` role, and refuse non-integral rates as `NoPrice{unrepresentable_rate}`. **REFINES REQ-XC-008.** | F11 | OL-08, OL-15 | Golden glm trial priced to the µ$; existing pricing tests unchanged |
| REQ-XC-117 | Paid-phase study reports SHALL evaluate costs with a new admitted-spend leg (exposure 0, or admitted and within cap, with settlement status), refusing unadmitted or over-cap spend; the round-5 `$0` evaluators SHALL remain unchanged. | F12 | OL-15 | Leg truth-table tests; round-5 tests byte-unchanged |
| REQ-XC-118 | Every slot record SHALL carry `costs` in the `hh-slot-costs/1` shape of §1.5, with the `model` member a recomputable fold over the phase proxy run filtered by slot and the `compute` member owned by B3. | F8, §1.5 | OL-08, OL-15 | Fold recompute test from the run ledger |
| REQ-XC-119 | The participant record SHALL carry the catalog identity tuple of F13 in `ModelSnapshotRecord.snapshot_id` (`pinned = false`, `provenance = reported`) with the full catalog bodies on `model.profile.probed`; a tuple change SHALL stop collection; served-model checks SHALL use a lane-declared expected set fixed from recorded first-smoke evidence. **REFINES REQ-XC-009.** | F13 | OL-15 | Changed `modified_at` instant stops collection; a cosmetic formatting change does not |
| REQ-XC-120 | "Same participant" SHALL be asserted only as `participant_identity: catalog_declared` over `{lane_ref, model_id, catalog tuple, pinned effort, gen_param_policy hash}`, with its non-guarantees (served weights, numerics, region) stated wherever the claim appears. | F13 | OL-15 | Review check; the publication-gate checklist |
| REQ-XC-121 | Provider usage and balance reads SHALL be ledgered as `measurement.provider.usage.observed` and the settlement as `measurement.cost.reconciled` with status ∈ {pending, settled, settled_discrepant, unreconcilable{reason}}, tolerance max(2 %, $0.05), whole-window totals primary and boundary-flagged hourly buckets secondary. **REFINES REQ-XC-005.** | F8, F14 | OL-08 | Injected-lag and mismatch fixtures |
| REQ-XC-122 | The proxy SHALL provide `record` and `replay` modes over one recording format (raw exchanges), with `replay` having no upstream port, a `RequestCanonicalizer` hook (built-ins `identity`, `json_canonical`), per-call match verdicts and a declared divergence policy; a gateway `RecordedTransport` SHALL serve the same recordings to native runs. | F15 | OL-15 | Byte-identical replay; divergence stop at k with `first_diff_path`; native frames equal the decoded proxy bytes |
| REQ-XC-123 | The proxy SHALL NOT retry upstream calls (`max_attempts = 1`), SHALL pass upstream error statuses, bodies and `Retry-After` through verbatim, and SHALL answer its own refusals and synthesized failures per the F16 table with a receipt disposition each. **REFINES REQ-XC-011** (infra classes). | F16 | OL-03 | A fixture matrix of 429/502/timeout/broken stream/crash |
| REQ-XC-124 | On restart the proxy SHALL synthesize `model.call.failed{proxy_aborted}` for every dispatched call without a terminal row, charge its spend as `bounded{lo: 0, hi: reserved}`, and release the remainder. | F16 | OL-08 | Crash-recovery test conserves the budget invariant |
| REQ-XC-125 | Every acceptance test of the proxy machinery SHALL run against fixture or replay upstreams; live behaviour SHALL be claimed only from `hh-live-evidence/1` records produced by `scripts/live-verify.sh` under an operator spend authorization. | F15; ADR-0363 | OL-15 | CI runs offline; live tests are `#[ignore]` with an ADR-0363 citation |

## Handoff

- **Dependencies resolved:** C1 (all of F1–F16 consumed; REQ-XC-001/002/003/005/008/009/011/012/013 refined as marked; 004, 006, 007, 010, 014 and 015 adopted unchanged).
- **Supplies:**
  - **C4:** the B+C composite declaration, the cell env contract (base URLs, slot token), the `model_proxy` profile, `record` mode for the runner check, and per-harness compatibility items (402/heartbeat/buffering).
  - **D2:** the replay interface, canonicalizer hook, the identity class, and the native-through-proxy proposal.
  - **D1:** the failure taxonomy, budget refusal semantics, the slot cost record, reservation-refusal statistics and the effort parameter.
  - **B3:** shares §1.5 (B3 owns `compute`), plus receipt disk sizing (~10 MB/trial, unverified) and the archive question.
  - **B2:** spawns and controls the proxy and folds slot costs.
  - **H1:** ADR register (§3), Q-X21/Q-X22, settlement margin, archive location.
- **Proposed follow-up rows:** none new. C1's paid first-smoke unit maps to ticket hint 8.
- **Conflicts with other notes:** none found. C1's REQ-XC-003 ("`max_tokens` inserted when absent") and REQ-XC-012 ("refuse or strip") are refined, not contradicted, for the stated parity reason. B3's note does not yet exist, so the shared record shape is stated here first.
- **Capability/portfolio disposition changes:** none. Nothing here is executed evidence. All proposals are UNVERIFIED until their tickets land with the stated acceptance.
