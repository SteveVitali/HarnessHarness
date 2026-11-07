# 26_ollama-glm — Ollama Cloud and glm-5.3-flash ground truth
Stream: C
Researched: 2026-11-06
Author: research worker (Claude Opus 5.5 subagent), synthesize-spec row C1
Ledger rows covered: C1
Repository baseline: main 75883daa (inspected at worktree HEAD abf26c97 on `claude/experiments-spec`, which adds only program docs over 75883daa; the coordinator's uncommitted ledger edit was not read for this row)
Confidence overall: medium. The published surfaces are high-confidence: Ollama docs, pricing, model page, free metadata endpoints, the HF card and HF leaderboard files. Every runtime-behaviour question that needs an inference call is UNVERIFIED by construction, because this row ran no inference: seed honouring, cache accounting per endpoint, error shapes and default sampling.

Credential: `OLLAMA_API_KEY` was read from the operator's `.env` without being printed, logged or written. provided: yes.

## Sources and retrieval

| Source | Exact locator | Retrieved (UTC) |
|---|---|---|
| Ollama docs index | https://docs.ollama.com/llms.txt | 2026-11-06T22:53:09Z |
| Cloud | https://docs.ollama.com/cloud.md | 2026-11-06T22:53:21Z |
| OpenAI compatibility | https://docs.ollama.com/api/openai-compatibility.md | 2026-11-06T22:53:21Z |
| Anthropic compatibility | https://docs.ollama.com/api/anthropic-compatibility.md | 2026-11-06T22:53:21Z |
| Usage (response metrics) | https://docs.ollama.com/api/usage.md | 2026-11-06T22:53:21Z |
| Cloud usage | https://docs.ollama.com/api/cloud-usage.md | 2026-11-06T22:53:22Z |
| Balance | https://docs.ollama.com/api/balance.md | 2026-11-06T22:53:22Z |
| Errors | https://docs.ollama.com/api/errors.md | 2026-11-06T22:53:22Z |
| Authentication | https://docs.ollama.com/api/authentication.md | 2026-11-06T22:53:22Z |
| Tags / show (thin pages; schemas in OpenAPI) | https://docs.ollama.com/api/tags.md, https://docs.ollama.com/api-reference/show-model-details.md | 2026-11-06T22:53:22Z |
| Thinking, tool calling, chat, FAQ, context length | https://docs.ollama.com/capabilities/thinking.md, …/capabilities/tool-calling.md, …/api/chat.md, …/faq.md, …/context-length.md | 2026-11-06T22:53:23Z |
| Integrations | https://docs.ollama.com/integrations/{claude-code,codex,opencode,goose,droid,pi,hermes,index}.md | 2026-11-06T22:53:24Z–22:53:25Z |
| OpenAPI spec | https://docs.ollama.com/openapi.yaml | 2026-11-06T22:55:42Z |
| Pricing | https://ollama.com/pricing | 2026-11-06T22:53:50Z |
| Model page | https://ollama.com/library/glm-5.3-flash | 2026-11-06T22:53:50Z |
| Cloud catalog | https://ollama.com/search?c=cloud | 2026-11-06T22:53:50Z |
| Pricing blog (dated 2026-08-31) | https://ollama.com/blog/transparent-pricing | 2026-11-06T22:53:50Z |
| HF model card, generation config, config, API metadata, chat template | https://huggingface.co/zai-org/GLM-5.3-Flash/raw/main/README.md, …/generation_config.json, …/config.json, https://huggingface.co/api/models/zai-org/GLM-5.3-Flash, …/raw/main/chat_template.jinja | 2026-11-06T22:54:26Z–22:54:49Z |
| Z.ai model guide | https://docs.z.ai/guides/llm/glm-5.3-flash.md | 2026-11-06T22:54:48Z |
| Z.ai blog | https://z.ai/blog/glm-5.3-flash | 2026-11-06T22:54:26Z: **INACCESSIBLE** (JS-only shell, no content); its text is mirrored in the Z.ai guide above, which was read |
| HF harness leaderboard | https://huggingface.co/spaces/joelniklaus/harness-leaderboard/raw/main/docs/blog/numbers.md (board generated 2026-11-02T12:52:42Z), …/raw/main/docs/decisions.md | 2026-11-06T22:54:26Z |
| Free metadata calls | `GET https://ollama.com/api/tags` (HTTP 200) | 2026-11-06T22:52:49Z |
| | `POST https://ollama.com/api/show {"model":"glm-5.3-flash"}` (HTTP 200) | 2026-11-06T22:52:58Z |
| | `GET https://ollama.com/api/balance` (HTTP 200) | 2026-11-06T22:53:03Z |
| | `GET https://ollama.com/api/usage?range=30d` (HTTP 200) | 2026-11-06T22:53:05Z |
| Repository | `crates/hh-gateway/src/snapshot.rs`, `crates/hh-gateway/src/dialects.rs`, `crates/hh-embed/src/gateway_model.rs`, `crates/hh-budget/src/{pricing,account,quantity}.rs`, `crates/hh-eval/src/{exposure_quote,central_protocol}.rs`, `docs/build/reports/live/{2026-10-21-anthropic-call.json,pricing.json,SPEND.jsonl}` at abf26c97 | 2026-11-06T22:56:20Z–22:59:00Z |

The assignment named `crates/hh-gateway/src/gateway_model.rs`, which does not exist. The dialect documents live in `crates/hh-gateway/src/dialects.rs`, and `GatewayModel` lives in `crates/hh-embed/src/gateway_model.rs`.

No inference endpoint was called: not `/api/chat`, `/api/generate`, `/v1/chat/completions`, `/v1/responses` or `/v1/messages`. The four metadata calls were the only authenticated requests.

## Endpoint-feature matrix (Ollama Cloud, `https://ollama.com`)

| Feature | `/v1/chat/completions` (OpenAI chat) | `/v1/responses` (OpenAI Responses) | `/v1/messages` (Anthropic) | `/api/chat` (native) |
|---|---|---|---|---|
| Auth | `Authorization: Bearer` | Bearer | **Bearer only; `x-api-key` alone rejected** | Bearer |
| Extra headers | none | none | `anthropic-version` ≥ `2023-06-01` (older rejected) | none |
| Streaming | yes (SSE; `stream_options.include_usage`) | yes | yes (all standard events incl. `ping`) | yes (NDJSON) |
| Tools (function calling) | yes | yes (function tools) | yes (`tool_use`/`tool_result`) | yes |
| `tool_choice` | **not supported** | not listed | **not supported** | n/a |
| Custom/freeform tool replay | n/a | **not supported on cloud** | n/a | n/a |
| Stateful (`previous_response_id`, `conversation`) | n/a | **not supported** | n/a | n/a |
| `seed` | **accepted** ("Reproducible outputs" checked) | not listed | no field (Anthropic API has none) | `options.seed` |
| `temperature` / `top_p` | yes / yes | yes / yes | yes / yes (+ `top_k`) | `options.*` (+ `top_k`, `min_p`) |
| `n`, `logprobs`, `logit_bias`, `user` | **not supported** | n/a | n/a | n/a |
| Reasoning control | `reasoning_effort`, `reasoning.effort` | `reasoning.effort`, `think` | `thinking`, `output_config.effort`; `budget_tokens` accepted but **not enforced** | `think` (bool or level) |
| Prompt caching control | none | none | **`cache_control` not supported** | none |
| `count_tokens` | n/a | n/a | **`/v1/messages/count_tokens` not supported** | n/a |
| `metadata` | n/a | n/a | not supported | n/a |
| Batches | n/a | n/a | not supported | n/a |
| Usage fields documented in response | not enumerated by the compat page | not enumerated | **`input_tokens`, `output_tokens` only**; counts are tokenizer approximations | `prompt_eval_count`, **`prompt_eval_cached_count`**, `eval_count`, durations (ns) |
| Mid-stream errors | not documented | not documented | contradictory: `error` listed under streaming events, but "Server-sent errors … not supported (errors return HTTP status)" | NDJSON `{"error":…}` object, status unchanged |
| Documented integrations using it | Droid, Pi, Hermes, OpenCode (`@ai-sdk/openai-compatible`) | Codex (`wire_api = "responses"`) | Claude Code (`ANTHROPIC_BASE_URL=https://ollama.com`, `ANTHROPIC_AUTH_TOKEN`) | Goose (Ollama provider) |

Sources: openai-compatibility.md, anthropic-compatibility.md, authentication.md, usage.md, errors.md, openapi.yaml `ModelOptions`, integrations/*.md (retrieval times above).

## Pricing and cost prior

| Item | Value | Source |
|---|---|---|
| glm-5.3-flash input | **$0.15 / 1M tokens** | pricing page, model page |
| glm-5.3-flash cached input | **$0.03 / 1M tokens** | pricing page, model page |
| glm-5.3-flash output | **$0.50 / 1M tokens** | pricing page, model page |
| Off-peak for glm-5.3-flash | **none listed.** The off-peak rule ("outside 12:00 and 18:00 UTC on weekdays and all day on weekends") has off-peak rows only for deepseek-v4.1-flash and deepseek-v4-pro | pricing page |
| HF board token prices for GLM-5.3-Flash | list price ratios "0.15 : 0.03 : 0.5", the same as Ollama | decisions.md "Endpoint price per token" row |
| HF board GLM-5.3-Flash spend, scored trials | $2,168 both efforts; $1,242 high; $926 low | numbers.md |
| HF trials per model and effort | 198 × 9 × 3 = 5,346 | numbers.md |
| ⇒ mean $/trial (derived) | all ≈ **$0.203**; high ≈ **$0.232**; low ≈ **$0.173** | arithmetic on the two rows above |
| Per-harness $/trial, GLM high | Pi 0.120 · Codex 0.168 · Hermes 0.169 · Claude Code 0.172 · Kimi Code 0.189 · Qwen Code 0.208 · mini-swe-agent 0.229 · OpenClaw 0.344 · OpenCode 0.377 | numbers.md |
| Per-harness $/trial, GLM low | Pi 0.076 · Hermes 0.110 · Claude Code 0.115 · Kimi Code 0.115 · mini-swe-agent 0.142 · Codex 0.146 · Qwen Code 0.149 · OpenClaw 0.312 · OpenCode 0.299 | numbers.md |
| Effort cost driver | median reasoning tokens on a short prompt: `low` 107, `high` 182, unset (= `max`) 342 | decisions.md "Reasoning effort on GLM-5.3-Flash (zai-org route, measured 2026-10-16)" |
| Worst-case single call | ≤ 1,048,576 input × $0.15/M ≈ $0.157, plus ≤ 128K output × $0.50/M ≈ $0.064, so **≈ $0.22** | context from `/api/show`; 128K max output from the Z.ai guide (Ollama does not document a cap) |

**Trials per $60 (gross; net keeps a 20 % reserve for in-flight overshoot, retries and reconciliation slack).** This is a bounded estimate, not a quote.

| Assumed mean $/trial | Scenario it represents | Gross trials | Net (80 %) |
|---|---|---|---|
| $0.10 | cheap harness (Pi-like) at pinned `low`/`high`, cache parity with the HF route | 600 | 480 |
| $0.20 | HF GLM all-effort mean, cache parity | 300 | 240 |
| $0.30 | `high` mean × ~1.3 for Ollama's default `max` effort, or a heavy harness at `high` | 200 | 160 |
| $0.40 | OpenCode/OpenClaw-class harness at `max`, or materially worse cache-hit rates | 150 | 120 |

Two sensitivities dominate. **Caching:** an illustrative agent trial with 2M input tokens (90 % cached) and 30K output costs about $0.10. The same tokens fully uncached cost about $0.32, roughly 3× more. **Effort:** `max` is about 1.9× `high` in reasoning tokens. The HF figures mix Terminal-Bench 2.1 (87 tasks) with DeepSWE (111 tasks, up to 3 h, 400K context), and numbers.md does not split cost by benchmark, so a TB-2.1-only prior may be lower. That is unverified.

## Telemetry and spend-control matrix

| Surface | Fields / behaviour | Granularity, latency, limits | Observed now (2026-11-06T22:53Z) |
|---|---|---|---|
| `GET /api/balance` | `included{balance_usd, allowance_usd, period{from,until}}`, `purchased{balance_usd}`; legacy plans return `session`/`weekly` percentages instead | No params. **10 req/min per user, shared across keys and devices**; 429 + `Retry-After`; 503 "temporarily unavailable" | `included.balance_usd = 60`, `allowance_usd = 60`, period `2026-10-22T16:30:34Z` → `2026-11-22T16:30:34Z`; `purchased.balance_usd = 0` (new-plan shape, not legacy) |
| `GET /api/usage?range=24h\|7d\|30d[&group_by=model][&scope=team]` | `totals` and `buckets[]` of `{request_count, usage_usd, input_tokens (incl. cached), cached_input_tokens, output_tokens}`; final bucket `partial: true`; `groups` (≤10, then `remainder`) by model | 24h → hourly (25 buckets); 7d/30d → daily. **"New usage may take time to appear"** (no SLA). Same 10/min limit. No custom range, **no per-key and no per-request breakdown** ("coming soon") | 30d: `request_count 0`, `usage_usd 0`, all token counters 0, so the account has no prior cloud usage |
| Per-response usage | native: `prompt_eval_count`, `prompt_eval_cached_count`, `eval_count`; Anthropic: `input_tokens`, `output_tokens` (approximate); OpenAI: not enumerated | per call, final chunk when streaming | not exercised (no inference) |
| Account-side caps | Enterprise only: "Set cost budgets for users and API keys". Paid plans: **email reminder at 90 %** of included usage. Team: "add a set amount … turn off automatic usage billing" | none on Pro | n/a |
| Credit order | included credits first, then purchased; included refreshes monthly and **does not roll over** | per plan period | purchased $0 |
| Concurrency | PAYG/Pro **3 concurrent requests**, Max/Team 10; excess queued "up to a fixed limit", then rejected | per account | Pro-style allowance ($60) is consistent with Pro |

---

## Findings

### F1 — Three compatible dialects are hosted on ollama.com, all with bearer auth only
Claim: Ollama Cloud serves OpenAI chat completions and stateless Responses at `https://ollama.com/v1`, Anthropic Messages at `https://ollama.com/v1/messages`, and the native API at `https://ollama.com/api`. All require `Authorization: Bearer <key>`; the Anthropic endpoint rejects `x-api-key` alone and rejects `anthropic-version` older than `2023-06-01`.
Status: VERIFIED
Evidence kind: source inspection
Evidence: authentication.md ("This also applies to the hosted Anthropic-compatible `/v1/messages` endpoint; `x-api-key` alone is not supported"); anthropic-compatibility.md ("The cloud endpoint requires `Authorization: Bearer`", "rejects `anthropic-version` values older than `2023-06-01`"); openai-compatibility.md ("Only the non-stateful flavor is supported", "The cloud API does not support stateful Responses, built-in web search through `/v1/responses`, or custom/freeform tool-call replay"). The four metadata calls succeeded with a Bearer header (HTTP 200). "API keys do not expire."
Retrieved: 2026-11-06T22:53:21Z–22:53:22Z; calls 22:52:49Z–22:53:05Z
Implication for the spec: The HH proxy talks upstream with Bearer on every dialect. A harness configured for the Anthropic wire (Claude Code) sends `x-api-key` or `ANTHROPIC_AUTH_TOKEN` to the *proxy*, and the proxy holds the real key: cells never see `OLLAMA_API_KEY`, and the proxy rewrites auth. The proxy must refuse or rewrite `previous_response_id` on Responses, so stateful Codex flows fail closed rather than silently.
Outline delta: EXTENDS OL-02 — the external-harness surface needs three downstream dialects (OpenAI chat, Responses, Anthropic) in front of one upstream.
Limitations: Docs describe a subset of each API. The behaviour for unsupported fields (ignored, or a 400) is not stated except for `/v1/audio/speech` ("Unsupported values return a `400`"), so it is unknown for chat, Responses and messages.

### F2 — `tool_choice`, `count_tokens`, `cache_control` and `metadata` are unsupported; `seed` exists only on the OpenAI/native surfaces
Claim: No hosted dialect supports forcing or disabling tools (`tool_choice`), Anthropic token counting, Anthropic prompt-caching breakpoints, `n`, logprobs or batches. Extended-thinking `budget_tokens` is accepted but not enforced. `seed` is an accepted field on `/v1/chat/completions` and native `options.seed` only.
Status: VERIFIED (as documented)
Evidence kind: source inspection
Evidence: anthropic-compatibility.md "Not supported" table (`/v1/messages/count_tokens`, `tool_choice`, `metadata`, prompt caching `cache_control`, batches, citations, PDF, server-sent errors) and "Partial support" (`budget_tokens` accepted but not enforced); its "Supported request fields" shows `[ ] tool_choice`, `[ ] metadata`. openai-compatibility.md `/v1/chat/completions` request fields: `[x] seed`, `[ ] tool_choice`, `[ ] logit_bias`, `[ ] user`, `[ ] n`; features `[ ] Logprobs`. openapi.yaml `ModelOptions.seed` ("Random seed used for reproducible outputs").
Retrieved: 2026-11-06T22:53:21Z; openapi 22:55:42Z
Implication for the spec: (a) The proxy cannot offer an Anthropic `count_tokens` endpoint upstream. Harnesses that call it (Claude Code may, for context management) need either a typed `Unsupported` from the proxy or an HH-local estimate labelled `estimate`, never a fabricated exact count. (b) Any HH lowering that emits `tool_choice` must drop it with a recorded notice or refuse; it cannot be relied on. (c) Spend-cap logic cannot use `budget_tokens` as a ceiling; the proxy must enforce `max_tokens` itself (see F10).
Outline delta: EXTENDS OL-02, OL-08.
Limitations: Docs list fields; whether `tool_choice: "required"` is silently ignored or errors was not tested.

### F3 — Per-call cache visibility differs by endpoint, and HH's `openai-compatible@1` declares `uncached`
Claim: Ollama bills cached input automatically at $0.03/M and reports `cached_input_tokens` in account usage. Per call, only the native API documents a cached count (`prompt_eval_cached_count`). The Anthropic endpoint's usage carries only `input_tokens`/`output_tokens`. The OpenAI compat page enumerates no usage fields. HH's `openai-compatible@1` dialect declares `cache_semantics: uncached` and `cache_state_visible: false`.
Status: PARTIALLY VERIFIED. Docs and code were read. Whether `/v1/messages` and `/v1/chat/completions` traffic receives cached-rate billing is UNVERIFIED (inferred from caching being server-side and request-field-free).
Evidence kind: source inspection + inference
Evidence: usage.md (`prompt_eval_cached_count: How many prompt tokens were read from the cache`); openapi.yaml `UsageMetrics.input_tokens` "including cached input tokens", `cached_input_tokens` "already included in input_tokens"; anthropic-compatibility.md "Supported response fields … `usage` (input_tokens, output_tokens)"; pricing page "Cached input" column. Repo: `crates/hh-gateway/src/dialects.rs` lines 92–143 (`openai_compatible()`: "`cache_semantics: uncached` — … Ollama's internal prefix reuse is provider-internal — never claimed"). `anthropic_messages()` lines 409–497 declares `explicit_breakpoints`, `cache_state_visible: true`, `count_tokens: Some(..)`, `auth_kinds: [ApiKeyHeader]`.
Retrieved: docs 2026-11-06T22:53:21Z; repo 22:57Z
Implication for the spec: (a) Pricing each call from the response alone overestimates cost on the Anthropic route, because cached tokens are priced as uncached. That is safe for enforcement but wrong for reporting, so per-call cost from that route must be labelled `estimate` (upper bound) and settled against `/api/usage`. (b) A proxy that translates every downstream dialect onto upstream **native `/api/chat`** gets `prompt_eval_cached_count` per call and can produce `estimated_from_pricing` cost with an actual cache split. This is the strongest accounting option; it costs a translation layer (C2 decision). (c) `anthropic-messages@1` cannot be bound verbatim to Ollama: its auth kind is `x-api-key` and it claims explicit-breakpoint caching and `count_tokens`, all false for this upstream. An Ollama-specific dialect document (or lane-level overrides) is needed, and the claims must be honest.
Outline delta: EXTENDS OL-08 — the receipt must say which usage fields were provider-reported per call and which were settled from account usage.
Limitations: The cached-count field on the OpenAI compat response (`usage.prompt_tokens_details.cached_tokens`) may exist but is undocumented. A one-call paid smoke per dialect settles this (see Open questions).

### F4 — glm-5.3-flash identity: what the catalog exposes, and that none of it fingerprints served weights
Claim: The hosted identifier is `glm-5.3-flash` (no tag suffix on the direct API; `glm-5.3-flash:cloud` in the app/CLI). `/api/tags` gives a 12-hex digest `0c238824336d`, `modified_at 2026-08-26T07:00:00-07:00`, size `328250014584`, capabilities `[completion, thinking, tools, vision]`. `/api/show` gives family `glm5_next`, `parameter_count 321323031390`, `quantization_level FP8`, `context_length 1048576`, `embedding_length 4096`, thinking values `[low, high, max]` with default `max`, and **no digest, license, template or parameters member**. The parameter count matches the HF safetensors total exactly, and HF lists the release weights as FP8 (F8_E4M3).
Status: VERIFIED (catalog fields); the claim that these fields identify the served model is CONTRADICTED by Ollama's own FAQ (see Limitations)
Evidence kind: recorded execution (free metadata calls) + source inspection
Evidence: `GET /api/tags` at 2026-11-06T22:52:49Z, entry `glm-5.3-flash 0c238824336d 2026-08-26T07:00:00-07:00 328250014584 [completion, thinking, tools, vision]` (parameter_size and quantization_level empty in tags). `POST /api/show` at 22:52:58Z (full body, 470 bytes): `details{parent_model: glm-5.3-flash, family: glm5_next, parameter_size: "321323031390", quantization_level: FP8}`, `model_info{general.architecture: glm5_next, general.parameter_count: 321323031390, glm5_next.context_length: 1048576, glm5_next.embedding_length: 4096}`, `thinking{values:[low,high,max], default:max}`, `modified_at 2026-08-26T07:00:00-07:00`. HF API: `sha 46a050bec13002ad29ee795c16e61cf4fa3c4a9c`, `createdAt 2026-08-25T06:43:14Z`, `lastModified 2026-10-15T06:32:52Z`, `safetensors.total 321323031390` (`F8_E4M3 314396639232`, `BF16 6926096640`), tag `license:mit`. HF README: `license: mit`. Ollama model page: "MIT Licensed", "Updated 2 months ago", "1M" context, "321B parameters". openapi.yaml `ModelSummary.digest` "SHA256 digest identifier of the model contents".
Retrieved: 2026-11-06T22:52:49Z–22:54:49Z
Implication for the spec: HH can record a **catalog identity**: `{name, tags_digest, modified_at, parameter_count, quantization_level, context_length, thinking}`. HH must mark it as a `reported` catalog claim and must not use it as a served-model fingerprint. `ModelSnapshotRecord.snapshot_id` can carry the tags digest plus `modified_at` as the provider-reported snapshot coordinate, with `pinned: false`, because Ollama offers no pinnable snapshot for this model (no dated tag exists, unlike `deepseek-v4-pro:0813` or `deepseek-v4.1-flash:1017` in the same listing). A change in either field between phases is a `model_version_change` observable. An unchanged value does not prove the served model is the same.
Outline delta: EXTENDS OL-06 — "same participant" can be asserted only at catalog-identity level.
Limitations: The pricing FAQ says cloud models run "Native weights, as released … On modern NVIDIA hardware, models may use accelerated data formats … (e.g. NVFP4)". The pricing FAQ also says requests may "route to Europe for additional capacity", and the model page says it is "hosted in the United States and Europe". The numerics and serving deployment can therefore differ per request while every catalog field stays the same. The HF `lastModified` (2026-10-15) post-dates Ollama's `modified_at`, and whether Ollama tracked that repo update is unknown.

### F5 — Reasoning is always on, defaults to `max`, and unrecognised effort names silently fall back to `max`
Claim: glm-5.3-flash thinking cannot be disabled. Effort levels are `low|high|max`, default `max`. Ollama resolves unsupported effort names to the model default, so a harness that sends `medium` (or nothing) gets `max`. On the HF zai-org route, `max` used about 1.9× the reasoning tokens of `high`.
Status: VERIFIED (docs/metadata); the effect on Ollama cost is inference
Evidence kind: source inspection + recorded execution (show)
Evidence: `/api/show` `thinking.default: max`. Model page: "Reasoning is always on, with effort tunable per request across low, high, and max levels." Z.ai guide: "`thinking.type` only supports `enabled`; thinking cannot be disabled"; recommended `reasoning_effort: max`. HF README: "defaults to `max` if not passed (or if set to any other value)". `chat_template.jinja` line 2 sets `effective_reasoning_effort` to `max` unless `low`/`high`. openai-compatibility.md: "unsupported names resolve to the model default". anthropic-compatibility.md: "supported `output_config.effort` names are applied exactly and unsupported names resolve to the model default". decisions.md: zai-org "Rejected with code 1210: `none`, `minimal`, `medium`, `xhigh`"; "Median reasoning tokens … `low` 107, `high` 182, unset 342"; "Harness defaults differ (none, `medium`, provider default ≈ 2× `high`), so without pinning the comparison measures effort, not harness"; and "The router's Anthropic Messages route ignores the effort … hence the translator".
Retrieved: 2026-11-06T22:52:58Z–22:54:49Z
Implication for the spec: Effort is an experiment factor. The proxy must **pin** it on every upstream call (overwriting `reasoning_effort` / `reasoning.effort` / `output_config.effort` / `think`) and log what the harness asked for, as HF's meter does. Otherwise harness comparisons confound effort with harness. The proxy must also price reasoning tokens. Reasoning text appears in `message.thinking` (native) and is billed as output (the single output rate). HH's `PricingRow.output_reasoning` absent ⇒ visible rate, which is correct here.
Outline delta: EXTENDS OL-03 — effort is a declared tranche parameter; CONFIRMS OL-07 — effort is a natural one-variable knob.
Limitations: Ollama's behaviour was not measured; the 1.9× figure is from the zai-org route on a short chat prompt. The GLM chat template defaults `clear_thinking=false`, retaining prior turns' reasoning in the prompt when a harness echoes `reasoning_content`. Whether Ollama's cloud renderer or any given harness echoes it is unknown, and it raises input tokens.

### F6 — Native tool calls are an XML-style template that Ollama parses; text-protocol harnesses bypass it
Claim: GLM-5.3-Flash's template renders tools in a `<tools>` block and expects `<tool_call>{name}<arg_key>…</arg_key><arg_value>…</arg_value></tool_call>`. Ollama's server parses this into structured `tool_calls` / `tool_use`. Harnesses that send no `tools` and parse bash blocks or JSON from text (mini-swe-agent, Terminus-2) use only plain completion, so their reliability depends on the model's instruction following, not Ollama's parser.
Status: PARTIALLY VERIFIED (template read; Ollama's parser and the no-tools behaviour UNVERIFIED)
Evidence kind: source inspection + inference
Evidence: `chat_template.jinja` lines 33–50 (tool block and `<tool_call>` format), lines 157–163 (assistant `tool_calls` re-rendered in the same format), 75–92 (`<tool_response>`); Ollama pricing FAQ: "Cloud models that are trained to support tools are tested for tool calling and with real agent workflows before they go live"; tool-calling.md (streaming: gather `thinking`, `content`, `tool_calls` and return them together); HF README: DeepSWE run "using the mini-swe-agent harness with `temperature=0.95`"; numbers.md: mini-swe-agent leads GLM-5.3-Flash low effort (59.9) and is within CI of the leader at high.
Retrieved: 2026-11-06T22:53:23Z–22:54:49Z
Implication for the spec: (a) For native-tool harnesses, parse failures surface as malformed `tool_calls` or tool text leaking into `content`. The proxy's verbatim receipt should keep both the raw upstream body and the parsed form, so a discordance can be attributed to Ollama's parser rather than the harness. (b) For text-protocol harnesses, the request carries no `tools`, so Ollama's tool path is out of the loop. This favours mini-swe-agent and Terminus-2 for Phase 3 L1 request-level conformance (D2/E1), because the wire is just messages in, text out. (c) The receipt must keep `thinking` separately from `content`, since text parsers must not see reasoning.
Outline delta: EXTENDS OL-05, OL-06.
Limitations: Whether the model emits `<tool_call>` markup when no tools are offered is unknown. HF board evidence is from the zai-org route, not Ollama.

### F7 — glm-5.3-flash costs $0.15 / $0.03 / $0.50 per 1M with no off-peak discount; caching cannot be controlled
Claim: Rates are $0.15 input, $0.03 cached input and $0.50 output per 1M tokens, with no off-peak row. Ollama exposes no request control for caching (no breakpoints, no bypass), so cache warmth is provider-determined.
Status: VERIFIED (rates, absence of off-peak row, absence of controls)
Evidence kind: source inspection
Evidence: pricing page row `glm-5.3-flash $0.15 $0.03 $0.50`; off-peak rows exist only for `deepseek-v4.1-flash (Off-Peak)` and `deepseek-v4-pro (Off-Peak)`; model page "Cost /1M tokens $0.15 input $0.03 cached $0.50 output"; F2/F3 for the missing cache controls. Z.ai's off-peak 50 % is a GLM Coding Plan points rule (Z.ai guide), not an Ollama price.
Retrieved: 2026-11-06T22:53:50Z
Implication for the spec: (a) Off-peak scheduling buys nothing for this model. (b) The `central_protocol` warmth posture cannot honestly be `CacheBypassed`, and provider-side prefix reuse across slots sharing a system prompt is unavoidable. Slots must declare `warmth` as cold-start for the *environment* and record provider cache as `uncontrolled`. See F12 for why the existing gate refuses this. (c) Cache-hit rate is a material cost driver (≈3× between fully cached and uncached input, per the cost-prior table) and should be reported per slot from `prompt_eval_cached_count` or from usage settlement.
Outline delta: EXTENDS OL-08.
Limitations: Ollama does not document cache TTL, eligibility floor, cross-key isolation or the off-peak window's exact semantics.

### F8 — The $60 is the plan's monthly included allowance, expires at period end, and is the only credit on the account
Claim: The account is on the new (non-legacy) plan with `included.allowance_usd = 60`, `included.balance_usd = 60` for the period 2026-10-22T16:30:34Z to 2026-11-22T16:30:34Z, and `purchased.balance_usd = 0`. Included credits do not roll over; they refresh each period. Ollama spends included credits first, then purchased credits.
Status: VERIFIED
Evidence kind: recorded execution + source inspection
Evidence: `GET /api/balance` at 2026-11-06T22:53:03Z: `{"included":{"balance_usd":60,"allowance_usd":60,"period":{"from":"2026-10-22T16:30:34Z","until":"2026-11-22T16:30:34Z"}},"purchased":{"balance_usd":0}}`. balance.md ("The included period follows your plan's monthly reset schedule"; "The purchased balance includes only unexpired credits"; legacy plans return `session`/`weekly`). Pricing FAQ ("Does unused included usage roll over…? No."; "Ollama uses included plan credits first, then draws from the purchased usage credit balance"; Pro "$60 of usage credits per month", "3 concurrent requests"). Blog 2026-08-31: "when you use it up, you can keep going at the same per-token rate".
Retrieved: 2026-11-06T22:53:03Z–22:53:50Z
Implication for the spec: (a) The pilot credit is **use-it-or-lose-it by 2026-11-22T16:30:34Z** and refreshes to $60 afterwards while the subscription continues. A pilot spanning the reset would see the account balance rise. HH's budget must therefore be the **operator-authorized amount recorded in HH**, never "whatever balance remains". (b) The pre-pilot balance snapshot (included $60.00, purchased $0.00, 30-day usage $0) is a clean reconciliation baseline. The spec should require recording it, plus a post-run snapshot, in the spend packet. (c) Whether Ollama refuses requests or bills automatically once included credit is exhausted on Pro is not documented. This is an operator question.
Outline delta: CORRECTS OL-03/OL-08 — "$60 of credit" is a monthly allowance with a hard period boundary, not a standing balance; EXTENDS OL-17's lifecycle discipline to the model budget.
Limitations: Billing settings (auto-recharge, payment method) are not visible through any free API.

### F9 — Cost prior: about $0.10–$0.40 per trial, so $60 buys roughly 150–600 trials before reserve
Claim: The HF leaderboard spent about $0.20 per scored GLM-5.3-Flash trial at the same list prices (mean $0.173 at `low`, $0.232 at `high`, per-harness $0.076–$0.377). Allowing for Ollama's default `max` effort and unknown cache-hit parity, a defensible prior is $0.10–$0.40 per trial, or about 150–600 trials gross ($60), and 120–480 with a 20 % reserve.
Status: PARTIALLY VERIFIED (inputs verified; the transfer to Ollama/TB-2.1-only is inference)
Evidence kind: source inspection + inference
Evidence: numbers.md "zai-org/GLM-5.3-Flash at high/low effort" per-harness `$/trial`, "Spend on the scored trials: $1,242" (high) / "$926" (low), "Trials per model and effort: 198 x 9 x 3 = 5,346"; decisions.md "cost = metered tokens × the provider's list price, cached input at the cached rate" and the 0.15 : 0.03 : 0.5 ratio; reasoning-token medians (F5).
Retrieved: 2026-11-06T22:54:26Z
Implication for the spec: The pilot should be sized to *measure* $/trial per harness at the pinned effort, with mean, P90 and cache share, before any tranche quote. That matches A-3's pilot → measured quote → gate. HF's reserve rule is a good template: reserve per trial = max(mean, P90) over ≥9 metered trials, else an assumed upper bound. `exposure_quote` should take Ollama $/trial priors as explicit bound inputs, never defaults.
Outline delta: EXTENDS OL-03 — feeds the pilot design and N quote.
Limitations: numbers.md does not split cost between TB 2.1 and DeepSWE. HF used the zai-org provider (its own caching), pinned `low`/`high` (not `max`), and 3 attempts per task. Terminus-2 is not on the HF board.

### F10 — Ollama provides no hard spend cap on this plan; usage telemetry is coarse and lagging
Claim: On non-Enterprise plans Ollama offers no per-key or per-user budget, only a 90 % email. `/api/usage` is hourly at best (24h range), lags by an unspecified delay, has no per-key or per-request breakdown, and shares a 10 req/min limit with `/api/balance`. So HH's in-proxy budget enforcement is the only hard cap, and account usage is a settlement source, not a control signal.
Status: VERIFIED (documented capabilities); the conclusion is inference
Evidence kind: source inspection + recorded execution
Evidence: pricing page Enterprise ("Set cost budgets for users and API keys") and FAQ ("On paid plans, Ollama sends an email reminder at 90%"); cloud-usage.md (ranges/buckets table; "New usage may take time to appear"; "`partial` indicates whether the hour or day is still in progress, rather than whether all usage has been recorded"; "We plan to add … usage breakdowns by API key"; rate limit 10/min, 429 + `Retry-After`); `/api/usage?range=30d` response at 22:53:05Z (`granularity: day`, 31 + partial buckets, all zero). Repo: `crates/hh-budget/src/account.rs` lines 1–27 (reserve/charge/exhaust appended through the run's writer lease; E1 refuses `exhaust` while an effect is in flight).
Retrieved: 2026-11-06T22:53:05Z–22:53:50Z; repo 22:58Z
Implication for the spec: (a) **Pre-call reservation:** the proxy reserves a worst case per call before forwarding. Input is bounded by request bytes, because byte-level BPE emits at most one token per byte, plus a declared template overhead, priced at the *uncached* rate. Output is bounded by a proxy-enforced `max_tokens` (inserted when absent). The proxy charges actuals after the call and releases the remainder. The cap is then exceeded by at most zero, not by in-flight work. Reserving by worst case is necessary because `count_tokens` is unavailable (F2). (b) **Settlement:** at run end, wait until two consecutive `/api/usage?range=24h&group_by=model` reads one poll interval apart agree. Then compare HH's hourly sums of `{request_count, input_tokens, cached_input_tokens, output_tokens, usage_usd}` against the buckets and record the difference as a reconciliation row. Exclusivity of the key/account during the window is a precondition, because there is no per-key breakdown. (c) Poll at most once a minute (the docs recommend it), sharing that budget between balance and usage. (d) Hourly granularity is only available within 24 hours, so settlement must run within a day of the slots or fall back to daily buckets.
Outline delta: EXTENDS OL-08 — spend telemetry in packets is *HH-metered per call* plus *provider-settled per hour/day*, with the gap recorded.
Limitations: The lag magnitude is unknown. Whether `usage_usd` uses the off-peak or cached rates exactly as the table says was not tested (zero usage so far).

### F11 — Three concurrent requests with a bounded queue, 429 and 502 as the documented failure statuses
Claim: Pro allows 3 concurrent requests per account. Excess requests queue "up to a fixed limit" and are then rejected. Documented statuses include 429 (rate limit), 502 (cloud model unreachable) and 500; mid-stream errors arrive as an `error` object with the original 200 status.
Status: VERIFIED (as documented)
Evidence kind: source inspection
Evidence: pricing page "Pay as you go and Pro include 3 concurrent requests, and Max and Team 10. Requests beyond your plan's concurrency limit are queued … if the queue is full, the request will be rejected"; errors.md status list and streaming-error example; anthropic-compatibility.md "Server-sent errors … (errors return HTTP status)". HH: `dialects.rs` `retry_after_cap_ms: 60_000`, `stream_idle_timeout_ms: 120_000` on both dialects.
Retrieved: 2026-11-06T22:53:22Z–22:53:50Z; repo 22:57Z
Implication for the spec: (a) The proxy should hold its own per-account semaphore of 3 (or fewer) and queue in HH, so queueing time is measured and attributed to infrastructure, not hidden inside "model latency". (b) The HF board ran about 32 agent-minutes per trial on average (15,596 h / 28,849 trials across models and both benchmarks). If model calls occupy fraction *f* of a trial, about 3/*f* trials can run usefully in parallel. A 300-trial pilot is plausibly tens of wall-clock hours (inference), and C2/D1 should quote it from pilot timing. (c) 429/502/queue-full and broken streams are **infrastructure failures** under the reattempt policy, never task failures. HF classifies "a 429/5xx from the provider, or … a provider stream that broke off or was empty" as infra. (d) `stream_idle_timeout_ms: 120_000` may be too short for long `max`-effort thinking with streaming disabled, so it should be revisited after the pilot.
Outline delta: EXTENDS OL-03 (wall-time and compute quote), OL-02.
Limitations: The queue depth, the rejection status code, and whether the 3-slot limit counts streaming connections the same as non-streaming are not documented. No inference-endpoint request-rate limit is published beyond concurrency.

### F12 — Determinism: `seed` is accepted but its honouring on cloud is unverified, and HH's fingerprint probe would mislead here
Claim: `seed` is an accepted field on OpenAI chat and native options. Nothing documents that cloud inference (batched MoE serving, possibly varying hardware and region) is reproducible under a seed. The Anthropic route cannot carry a seed at all. HH's `hh.fingerprint/v1` probe (temperature 0, `max_tokens 128`) would likely produce non-matching digests on an always-thinking model, and `central_protocol` refuses any protocol whose `support_seed`/`support_fingerprint`/`support_cache` are false.
Status: PARTIALLY VERIFIED (docs and code read; honouring and probe behaviour UNVERIFIED, since no inference was allowed)
Evidence kind: source inspection + inference
Evidence: openai-compatibility.md `[x] Reproducible outputs`, `[x] seed`; openapi `ModelOptions.seed`; F4 Limitations (NVFP4, US/EU routing). Repo `crates/hh-gateway/src/snapshot.rs` lines 32–60 (`probe_request()` with `temperature 0`, `max_tokens 128`; `fingerprint_response` digests the canonical response body); lines 84–97 (`fingerprint_verdict`: `Unknown` when absent, `Drift` on any byte mismatch); `observe()` lines 326–378. `crates/hh-eval/src/central_protocol.rs` lines 208–216 (`support_route/cache/seed/fingerprint`, "every one must hold") and 637–649 (returns `SupportMismatch{member}` on any false); `Warmth::Unknown` refuses (lines 150–160, 634–636). Z.ai/HF recommend `temperature 1.0`, `top_p 0.95` (generation_config.json, Z.ai guide).
Retrieved: 2026-11-06T22:53:21Z–22:58Z
Implication for the spec: (a) **Honest declarations for this participant:** `support_seed = unverified`, which counts as not supported until a pilot replay test shows byte-equal outputs under a fixed seed. `support_fingerprint = false` (no provider fingerprint; the catalog identity of F4 is not one). `support_cache = false` (uncontrollable). `support_route` is the HH proxy's own route, so it can be true. (b) Taken together, **`materialize_protocol` as written refuses Phase 2/3 protocols on Ollama.** The spec needs either an ADR amending the §4.1 support gate to admit declared-unsupported members with a correspondingly weaker claim class (e.g. "stochastic participant; verdict-level comparison only"), or a separate protocol kind. It must not flip the booleans. (c) Do not run `hh.fingerprint/v1` against glm-5.3-flash as an identity check. A temperature-0 128-token request to an always-thinking `max`-effort model may return reasoning-only or truncated content, and batching nondeterminism makes byte-equality unlikely. A `Drift` from that probe would be a false alarm. If used at all, it should be descriptive. (d) Phase 3 L1 conformance should replay *recorded* responses (D2 already plans this), not rely on provider determinism.
Outline delta: CONTRADICTS OL-06's implicit assumption that the established parity machinery applies unchanged to an LLM participant. The gates need an amendment or a new admission path. EXTENDS OL-10.
Limitations: A seed-replay smoke (two identical seeded calls) would settle whether the seed is honoured. It is a paid call and was out of scope.

### F13 — Served-model stability: retirement is announced only in account settings, with no stated notice period
Claim: Ollama shows "upcoming retirements for models you've recently used" in usage settings and asks users to switch before the retirement date. No API, minimum notice or snapshot-pinning mechanism is documented for glm-5.3-flash.
Status: VERIFIED (as documented)
Evidence kind: source inspection + recorded execution
Evidence: cloud.md "Retirements" section; `/api/tags` (only `glm-5.3-flash`, no dated variant; `glm-5.2` still listed); pricing FAQ (native weights, possible NVFP4; US with possible EU routing); blog (plan changes can be applied by Ollama at its discretion to new signups).
Retrieved: 2026-11-06T22:52:49Z–22:53:50Z
Implication for the spec: (a) Record catalog identity (F4) at every phase boundary and at the start and end of every run, and stop collection on a change. `drift_observed` then becomes a record-backed boolean. (b) Phases that must share a participant (Phase 2 → 3 → 4) should be scheduled tightly. The spec should state that a retirement or catalog change mid-program ends the "same participant" claim and requires a new cohort, never relabelling. (c) The operator should check the settings page for retirement notices before each gate. No API exposes them.
Outline delta: EXTENDS OL-06, OL-11 (phase ordering is constrained by participant lifetime).
Limitations: Ollama's past notice periods are unknown; no history was available.

### F14 — `hh-budget` cannot represent glm-5.3-flash's per-token rates
Claim: `PricingRow` stores integer micro-units (10⁻⁶ USD) per token. $0.15/M is 0.15 µ$/token, $0.03/M is 0.03 and $0.50/M is 0.5, so all three round to 0 or 1 µ$/token. A table that "prices" glm-5.3-flash with integer µ$/token rates is either zero (silently free, the failure mode `NoPrice` exists to prevent) or overstates cost by 2–33×.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-budget/src/quantity.rs` line 21–26 (`Money{micro_units, currency}` — "integer micro-units (10⁻⁶ of `currency`)"); `crates/hh-budget/src/pricing.rs` lines 301–325 (`input_uncached: i64` "Micro-units per `tokens.input.uncached` token", etc.) and the unit test at line 786 (`input_uncached: 3, // 3 µ$/token`); `price()` lines 550–598 multiplies `r * qty` in µ$. The scripts-level `docs/build/reports/live/pricing.json` uses float `*_per_1k_usd` instead, a separate table that already priced Claude Haiku 5.5 at $0.10/M.
Retrieved: 2026-11-06T22:58Z
Implication for the spec: Before any paid run is priced through `hh-budget`, the pricing schema needs a finer rate unit, e.g. rates in µ$ per 1M tokens (150 000 / 30 000 / 500 000), with rounding defined once at the role-sum level. This changes a durable schema and probably needs an ADR and the CC7 regenerate discipline. The alternative is to keep paid-run spend in the scripts-level float table, which forfeits `exact`/`estimated_from_pricing` provenance in the ledger.
Outline delta: EXTENDS OL-08, OL-15 — the budget record cannot carry this participant's prices today.
Limitations: The search was limited to `hh-budget`/`hh-telemetry` construction sites. A rate-scaling layer elsewhere in the workspace was not found but was not searched exhaustively.

### F15 — Fit of the existing HH dialect documents to Ollama upstream
Claim: `openai-compatible@1` matches Ollama `/v1/chat/completions` on transport (SSE, final-only usage, bearer) but its `plan_schema.allowed_keys` admits `tool_choice` (unsupported upstream) and lacks `reasoning_effort` (needed to pin effort). `anthropic-messages@1` mis-declares auth, caching and `count_tokens` for Ollama. No HH dialect targets Responses or native `/api/chat`.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `dialects.rs` lines 115–123 (`allowed_keys`: temperature, top_p, max_tokens, seed, stream_options, tool_choice), lines 135–147 (`auth_kinds [None, Bearer]`, `snapshot_id_exposed: None`); lines 440–497 (Anthropic: `ApiKeyHeader`, `ExplicitBreakpoints`, `count_tokens: Some`); `crates/hh-embed/src/gateway_model.rs` lines 98–130 (`ModelLane{dialect_ref, endpoint_ref, provider_model_id, credential_ref, substitution_allowed, …}`; served-model mismatch is terminal unless `substitution_allowed`) and 306–325 (only the three admitted dialects). The example live record `docs/build/reports/live/2026-10-21-anthropic-call.json` shows the receipt shape: `token_usage{input_total, input_uncached, cache_read, cache_write, output_total, output_reasoning, convention: hh-inclusive/1, normalizer_ref}`, `cost_usd`, `model_reported`, empty `request_ids` and `response_headers`.
Retrieved: 2026-11-06T22:57Z–22:59Z
Implication for the spec: C2 needs an upstream lane for Ollama, either a new `ollama-native@1` dialect or an `openai-compatible@1` variant with honest members: bearer auth, `reasoning_effort` admitted, `tool_choice` refused, a cache view that admits the provider-reported cached count, and `snapshot_id_exposed: None`. The served-model check should compare the response `model` field to `glm-5.3-flash`. The live-evidence `token_usage` block is the right receipt shape: `cache_read` from `prompt_eval_cached_count` where available, otherwise `null` with `available: false`, never 0.
Outline delta: EXTENDS OL-02, OL-15.
Limitations: The response `model` spelling on each Ollama endpoint was not observed (no inference).

### F16 — The HF board's GLM-5.3-Flash is a different serving setup, so cross-board comparison is descriptive only
Claim: HF served `zai-org/GLM-5.3-Flash` through the HF router on provider `zai-org`. That provider runs Z.ai's own stack on Chinese accelerators with W8A8 weights and hybrid INT8/FP8/BF16 KV-cache quantization, at efforts `low`/`high`, through HF translators that pinned effort and fixed developer-role and sampling handling. Ollama serves the model on NVIDIA Cloud Providers with "native weights" (FP8 metadata, possibly NVFP4), default effort `max`, and its own template/parser and compat translations.
Status: VERIFIED (each side's documented setup); the size of any resulting score difference is UNVERIFIED
Evidence kind: source inspection
Evidence: numbers.md ("Model zai-org/GLM-5.3-Flash: provider zai-org, efforts on the board: low, high"); decisions.md (registered models; effort pinning; "Until 2026-10-29 the router's translation also dropped developer messages … and set temperature 1 and top_p 1 on every call … zai-org refuses the developer role"; the Anthropic route ignored effort, "hence the translator"; FeatureBench unscored after answer-key access); Z.ai guide ("served … on a large-scale cluster of Chinese AI chips … W8A8 quantization, hybrid INT8/FP8/BF16 cache quantization"); Ollama pricing FAQ ("Ollama collaborates with NVIDIA Cloud Providers (NCPs)"; native weights; NVFP4 possible).
Retrieved: 2026-11-06T22:53:50Z–22:54:48Z
Implication for the spec: Results pages may show HF figures as **context** with an explicit "different provider, quantization, effort and adapters" label. The spec must not present HF numbers as a replication target or a pass/fail anchor (consistent with the operator's ruling). The HF experience is still a useful source of pitfalls for the proxy: developer-role handling, effort pinning, sampling left unset unless the harness sets it, and Codex compaction requests rejected by the bridge.
Outline delta: CONFIRMS OL-18 (inspiration-only use of the HF board); EXTENDS OL-09.
Limitations: The HF board's TB 2.1-only scores were not extracted (numbers.md reports macro over two benchmarks).

---

## Answer and completion assessment

**1. Endpoints and dialects.** OpenAI chat, stateless Responses, Anthropic Messages and native `/api/chat` are all hosted, with Bearer auth only (F1). The following are unsupported or unreliable: `tool_choice` everywhere, `count_tokens`, `cache_control`, `metadata`, stateful Responses, freeform-tool replay, `n`, logprobs, and enforced `budget_tokens` (F2). Per-call cached counts are documented only on the native API (F3).

**2. glm-5.3-flash.** Facts (F4–F6):
- Name `glm-5.3-flash`; tags digest `0c238824336d`; modified 2026-08-26.
- 321.3B parameters, FP8, 1,048,576 context, tools, thinking and vision.
- Thinking is always on, levels `low`/`high`/`max`, default `max`.
- MIT licence; HF release 2026-08-25.
- Recommended sampling `temperature 1.0`, `top_p 0.95`.
- No served-weights fingerprint exists. The catalog identity is a reported claim, not a fingerprint.

**3. Pricing.** $0.15 input, $0.03 cached and $0.50 output per 1M tokens, with no off-peak discount (F7). Cached pricing is automatic and uncontrollable. On the Anthropic route it is invisible per call and probably applied (UNVERIFIED). The cost prior is $0.10–$0.40 per trial, so $60 buys about 150–600 trials gross, or 120–480 net (F9).

**4. Spend control.** There is no provider-side cap on this plan. HH proxy reservation is the only hard cap, and `/api/usage` (hourly within 24h, lagging, no per-key breakdown, 10/min) is for settlement (F10). The balance is $60 included for the period ending 2026-11-22T16:30:34Z, with $0 purchased and zero usage in the last 30 days (F8).

**5. Concurrency and errors.** 3 concurrent requests, then a bounded queue, then rejection. 429 and 502 are documented. Throughput is concurrency-bound, so the wall time must be quoted from pilot timing (F11).

**6. Determinism and identity.** Seed honouring is unverified, the fingerprint is unavailable, cache is uncontrollable, and the retirement notice exists only in the account UI. Honest HH declarations make `central_protocol` refuse, so an ADR-level amendment is needed. The fingerprint probe should not be used as an identity gate (F12, F13).

**7. HF comparison.** HF used a different provider, quantization, efforts and adapters at the same list prices. Use it descriptively only (F16).

**Repository implications.**
- `hh-budget` cannot price these rates (F14).
- The dialect documents need an Ollama-honest upstream lane (F15).

**Settled vs open.** Everything published is settled. These runtime facts are unresolved: seed honouring, the cached count on the OpenAI and Anthropic responses, error shapes, response `model` spelling, default sampling when unset, and the queue-full status. They are acceptable uncertainty for this row, because each is a one-call check in the paid pilot's first smoke. There is no technical blocker for C2/D1, but F12 and F14 are spec blockers for paid runs until they are designed.

## Open questions

1. **Overage behaviour on Pro.** Owner: operator. Units: C2, D1, H1. Operator decision: yes. When included credit is exhausted with purchased = $0, does Ollama refuse requests, or is automatic usage billing enabled on this account? If it is enabled, the operator should disable it, or confirm that it stays off, for the experiment window.
2. **Period boundary.** Owner: operator. Units: D1, H1. Operator decision: yes. The $60 lapses or refreshes at 2026-11-22T16:30:34Z. Should the pilot be scheduled to complete before then? Is the post-reset $60 authorized, or is the authorization strictly $60 total?
3. **Account exclusivity.** Owner: operator. Unit: C2. Operator decision: yes. Settlement (F10) assumes no other use of this Ollama account during runs, including Ollama app/CLI use by the operator, because usage has no per-key breakdown. The operator should confirm, or the spec should accept unreconcilable noise.
4. **Effort level.** Owner: operator/D1. Units: D1, D3. Operator decision: yes. The options are to pin `high` (comparable to HF, cheaper), pin `max` (model default and vendor recommendation, about 1.9× reasoning tokens), or make effort a Phase 4 factor.
5. **First-smoke checks.** Owner: C2/B2. Units: C2, D2. Operator decision: no, beyond the spend authorization for the smoke. A paid smoke of a few cents should include: one call per dialect recording the full response (`model` spelling, usage members incl. any cached count), two identical seeded calls (seed honouring), one `tool_choice` request (ignored or 400), and one Anthropic call carrying `cache_control` (tolerated or rejected).
6. **Protocol-gate amendment.** Owner: D1/D2 with an ADR. Units: D1, D2, H1. Operator decision: yes, at ratification. The choice is how §4.1's `support_seed/fingerprint/cache` gate should admit a stochastic hosted participant (F12).
7. **Pricing unit.** Owner: C2. Units: C2, H1. Operator decision: no, unless an ADR route is contested. The choice is a finer `PricingRow` rate unit, or keeping paid spend in the scripts-level table (F14).

## Spec requirements emitted

- **REQ-XC-001** — The model proxy SHALL authenticate upstream to Ollama with `Authorization: Bearer` only. The credential SHALL live only in the proxy process, never in cells, manifests or receipts (`provided: yes/no`). *Rationale:* F1; ADR-0366 D4 posture. *OL:* OL-02, OL-15. *Acceptance:* a test asserts there is no key in the cell environment or receipts, and an Anthropic-wire downstream request reaches upstream with Bearer.
- **REQ-XC-002** — The proxy SHALL pin reasoning effort on every upstream call to the slot-declared level (`low|high|max`), overwriting any harness-supplied effort field, and SHALL record both the requested and the applied values in the call receipt. *Rationale:* F5. *OL:* OL-03, OL-07. *Acceptance:* a fixture in which a harness sends `medium` shows the applied effort equal to the declared level, with both recorded.
- **REQ-XC-003** — The proxy SHALL enforce a slot budget by pre-call reservation of a worst-case cost. Input is bounded by request UTF-8 bytes plus a declared template overhead, priced at the uncached rate. Output is bounded by a proxy-enforced `max_tokens` cap, inserted when absent. The proxy SHALL refuse with a typed budget refusal when the reservation would exceed the remaining budget. *Rationale:* F2, F10 (no `count_tokens`, no provider cap). *OL:* OL-08. *Acceptance:* unit tests show that cumulative charged + reserved never exceeds the cap under 3 concurrent calls.
- **REQ-XC-004** — The program SHALL treat the HH-recorded, operator-authorized amount as the budget ceiling, independent of the provider balance, and SHALL NOT treat a provider period reset as new authorization. *Rationale:* F8. *OL:* OL-03, OL-08, OL-16. *Acceptance:* a review check that budget allocation records cite the verbatim authorization.
- **REQ-XC-005** — Each paid run SHALL record a pre-run and post-run `/api/balance` snapshot and a settlement read of `/api/usage?range=24h&group_by=model`. Settlement is taken after two consecutive agreeing reads at least one minute apart. The run SHALL append a reconciliation row comparing HH per-call sums with provider buckets by count, tokens, cached tokens and USD, keeping the signed difference. *Rationale:* F8, F10. *OL:* OL-08, OL-15. *Acceptance:* a fixture reconciliation with injected lag produces a pending-then-settled row, and a mismatch is recorded, not hidden.
- **REQ-XC-006** — Provider telemetry polling SHALL NOT exceed one request per minute across `/api/usage` and `/api/balance` combined, and SHALL honour `Retry-After` on 429 and tolerate 503. *Rationale:* F10. *OL:* OL-08. *Acceptance:* a rate-limiter unit test.
- **REQ-XC-007** — Per-call cost SHALL be labelled by derivation. With a provider-reported cached count it is `estimated_from_pricing` with the actual cache split. Without one (e.g. the Anthropic route) it is an upper-bound `estimate` with `cache_read` unavailable, never 0. *Rationale:* F3. *OL:* OL-08. *Acceptance:* golden receipts for each upstream dialect.
- **REQ-XC-008** — The budget pricing representation SHALL express per-token rates below 1 µ$/token without rounding (e.g. µ$ per 1M tokens), with one declared rounding rule. glm-5.3-flash SHALL be priced at 0.15 / 0.03 / 0.50 USD per 1M tokens (input / cached / output), with `valid_from` and source cited. *Rationale:* F7, F14. *OL:* OL-08, OL-15. *Acceptance:* a pricing unit test reproduces a hand-computed trial cost to the µ$.
- **REQ-XC-009** — The participant record SHALL capture the Ollama catalog identity `{name, tags digest, modified_at, parameter_count, quantization_level, context_length, thinking values/default}` at run start, run end and each phase boundary. The record carries `provenance = reported` and `pinned = false`. A change SHALL set `drift_observed` and stop collection. *Rationale:* F4, F13. *OL:* OL-06, OL-11. *Acceptance:* a fixture with a changed `modified_at` triggers a stop.
- **REQ-XC-010** — The program SHALL declare `support_fingerprint = false`, `support_cache = false`, and `support_seed = unverified` (treated as false until a recorded replay test passes) for glm-5.3-flash on Ollama. It SHALL NOT use `hh.fingerprint/v1` byte-equality as an identity gate for this participant. Any protocol gate requiring these supports SHALL be amended by ADR or replaced, never satisfied by flipping booleans. *Rationale:* F12. *OL:* OL-06, OL-10, OL-15. *Acceptance:* the spec review confirms the declarations, and a test shows `materialize_protocol` is not invoked with falsified support.
- **REQ-XC-011** — The proxy SHALL enforce a per-account concurrency ceiling of at most 3 in-flight upstream requests, queue excess in HH with measured queue time, and classify 429, 5xx, queue-full rejection and broken or empty streams as infrastructure failures for reattempt policy. *Rationale:* F11. *OL:* OL-02, OL-03. *Acceptance:* a fixture upstream returning 429/502 produces infra-class outcomes, and a concurrency test never exceeds 3.
- **REQ-XC-012** — The proxy SHALL refuse (typed) or strip-with-notice any downstream field Ollama does not support (`tool_choice`, `previous_response_id`, `conversation`, Anthropic `count_tokens` calls, `metadata`). For `cache_control` it SHALL strip and record. The chosen behaviour per field SHALL be declared in the lane document. *Rationale:* F1, F2. *OL:* OL-02. *Acceptance:* a per-field fixture matrix.
- **REQ-XC-013** — Call receipts SHALL keep the verbatim upstream request and response bodies, including `thinking`/reasoning content separately from visible content and the raw tool-call form alongside the parsed form. *Rationale:* F5, F6. *OL:* OL-06, OL-10. *Acceptance:* a receipt schema test plus a replay of one recorded call.
- **REQ-XC-014** — Results publications that cite the HF harness leaderboard SHALL label it as a different serving setup (provider zai-org, quantization, effort levels, adapters) and SHALL NOT present it as a replication target. *Rationale:* F16. *OL:* OL-04, OL-18. *Acceptance:* a publication-gate checklist item.
- **REQ-XC-015** — The pilot SHALL measure, per harness at the pinned effort, mean and P90 $/trial, cache-hit share, calls per trial and model-occupied wall time. These SHALL be the only inputs to the tranche quote's cost and wall-time cells; the HF prior is used only to size the pilot. *Rationale:* F9, F11. *OL:* OL-03. *Acceptance:* the `exposure_quote` inputs cite pilot records.

## Handoff

- **Dependencies resolved:** C1 has no dependencies. It supplies C2 (proxy: F1–F3, F5, F10–F12, F14, F15), D1 (cost prior, effort factor, infra-failure classes, protocol-gate problem: F9, F11, F12), D2 (participant identity and stochasticity: F4, F12, F13) and E1 (text-protocol harness wire simplicity: F6).
- **Proposed follow-up rows:**
  - (a) A **paid first-smoke** unit (≤ $0.05, operator-authorized) executing Open question 5, best placed at the start of the C2 build or as a B2/C2 sub-ticket.
  - (b) An **ADR candidate** for the §4.1 support-gate amendment for stochastic hosted participants (D1/D2 to draft).
  - (c) An **ADR candidate** for the `hh-budget` rate unit (C2 to draft).
- **Conflicts with other notes:** none known. Earlier round-5 notes on the HF board, e.g. `05_ecosystem-and-benchmarks.md`, were not re-read for this row. A coordinator check that their HF characterization agrees with F16 is advisable.
- **Capability/portfolio disposition changes:** none. This row changes no capability label. All Ollama facts here are `source inspection` or `recorded execution` of free metadata calls; nothing is `live-executed` inference evidence.
