# 27_harness-survey — Harness survey and admission criteria

Stream: C — Model mediation and harness hosting
Researched: 2026-11-06 (UTC)
Author: Claude Code research worker (row C3), session `4210edcb-e16b-4f68-bf87-84c31bfd2161`
Ledger rows covered: C3
Repository baseline: `main` 75883daa (worktree `claude/experiments-spec` at `abf26c97`, which adds only the charter, ledger and outline trace; the coordinator's uncommitted ledger edit was not read for scope and not touched)
Confidence overall: medium

**Answer in brief.** Of 22+ surveyed harnesses, eight are recommended for the paid pilot, in rank order: **mini-swe-agent, Terminus-2, Claude Code, Pi, Hermes Agent, OpenHands, Kimi Code, OpenCode**. Qwen Code and Goose are alternates. Codex is deferred until a Responses-endpoint decision is made. Three harnesses fail a hard gate outright: Gemini CLI, Cursor CLI and Antigravity cannot be pointed at an OpenAI or Anthropic endpoint. Ollama Cloud prices `glm-5.3-flash` at exactly the HF board's basis ($0.15 input / $0.03 cached / $0.50 output per Mtok). The board's measured Terminal-Bench 2.1 costs therefore transfer directly: **$0.023–$0.084 per trial with provider caching**, and roughly **$0.05–$0.34 per trial if Ollama caches nothing**. At no-cache planning values with a 1.5× tail reserve, the $60 pilot covers a 3-task smoke run plus **10 tasks × 2 attempts for all eight harnesses**, about 184 trials, with roughly $14 left over. The pilot measures cost and feasibility. It cannot rank harnesses: on the full 87 × 3 board, Terminal-Bench 2.1 CIs are about ±10 points, and every GLM row overlaps. No harness, benchmark or model was executed for this note, and nothing was spent.

## Scope and source record

The row asks for a candidate survey covering license/ToS, headless mode, pins, Harbor adapter, base-URL/API-format fit with Ollama Cloud, caching dependence, install reproducibility, egress, trajectory export and known issues. It also asks for a pilot shortlist with expected $/trial. I read the founding brief (§3 A-3, A-4, A-5, A-7), the ledger's C3 row, Q-X7, Execution contract and standing limits, the note conventions, and OL-02/OL-09. `bash scripts/deferrals-open.sh` ran read-only at 2026-11-06T23:01:16Z, printed nothing and exited 0: zero open deferral rows.

Every source below was opened and the cited parts read. Downloads went to the session scratchpad under `…/scratchpad/C3/`. Public repositories were shallow-cloned there to **read only**. Nothing was installed, built or executed apart from small local Python parsers I wrote, run with `-I` over the downloaded JSON and HTML. GitHub repository metadata (license, archived flag, latest release) came from the authenticated `gh api` read endpoints. Package registries (npm, PyPI) were **not** queried, because the assignment did not name them.

| ID | Source and locator | Retrieved (UTC) | Version/date |
|---|---|---|---|
| S1 | HF Space files `https://huggingface.co/spaces/joelniklaus/harness-leaderboard/raw/main/{README.md, docs/decisions.md, docs/blog/numbers.md, docs/blog/post.md, CHANGELOG.md, LAB_NOTEBOOK.md}` (decisions read in full; notebook read for the bounded 2026-10-29 entries, the Status section and a grep over costs and caching) | 2026-11-06T22:53:03Z | `raw/main`, no commit pinned; CHANGELOG top entry v0.1.1 (2026-11-06) |
| S2 | Public board JSON `https://joelniklaus-harness-leaderboard.hf.space/api/index` (the payload the board's README says it serves; parsed for GLM-5.3-Flash, full stage, `terminal-bench-2.1` per harness) | 2026-11-06T22:53:52Z | payload `generated_at` 2026-11-06T22:53:28Z, `version` 0.1.1 |
| S3 | Ollama docs index `https://docs.ollama.com/llms.txt`; pages `cloud.md`, `integrations/{index,claude-code,codex,opencode,pi,hermes,openclaw,goose,droid,copilot-cli,cline-cli,deepseek-harness,oh-my-pi,pool}.md`, `api/{openai-compatibility,anthropic-compatibility,cloud-usage}.md` | 22:54:20Z–22:54:29Z | living docs, undated |
| S4 | Ollama pricing `https://ollama.com/pricing` (HTML, text extracted) | 22:55:00Z | living page, undated |
| S5 | Harbor docs `https://docs.harborframework.com/llms.txt`; `agents/{pre-integrated-agents,agent-auth-resolution,base-url-management,acp,atif}.md` | 22:55:11Z–22:55:17Z | living docs |
| S6 | Harbor repo clone `github.com/harbor-framework/harbor` at `ff2a1ef48070a8df374669138e4d4db79b77f013` (commit 2026-11-06T09:57:41-08:00; `pyproject.toml` version 0.25.0): `src/harbor/agents/installed/*.py`, `src/harbor/agents/terminus_2/*` | 22:55:31Z | latest release v0.25.0 (2026-10-25) |
| S7 | GitHub repo metadata (`gh api repos/<r>`): license SPDX, archived, pushed_at; LICENSE files of `anthropics/claude-code`, `charmbracelet/crush`, `github/copilot-cli`; latest releases | 22:56:28Z, 22:56:33Z, 23:00:55Z | as returned |
| S8 | Claude Code docs `https://code.claude.com/docs/llms.txt`; `en/{llm-gateway,llm-gateway-protocol,legal-and-compliance,data-usage,env-vars,gateways}.md` | 22:56:40Z–22:56:56Z | living docs |
| S9 | Anthropic Commercial Terms `https://www.anthropic.com/legal/commercial-terms` (page states "Effective Jun 17, 2025"); Consumer Terms `https://www.anthropic.com/legal/consumer-terms` ("Effective Oct 8, 2025") | 22:57:10Z | as stated |
| S10 | mini-swe-agent `README.md` and `docs/advanced/v2_migration.md` (via `gh api` contents); repo clone at `04d809ceab9df28f9adaed044884180159172930` (2026-09-03), `src/` grep | 22:58:16Z–22:58:28Z | latest release v2.4.6 (2026-07-23) |

The HF Space's "Harness Lens" dictionary (34 harnesses) is cited by S1 but was **not read**. Nothing here relies on it.

## Findings

### F1 — The HF board's nine harnesses are "popular coding agents with a Harbor adapter", pinned at versions already superseded

Claim: The board compares nine harnesses at pinned versions, chosen because they are the popular coding agents with a Harbor adapter. All of the upstreams released newer versions within days of the board's pins.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S1 `docs/decisions.md` §Scope, harness row: "The popular coding agents with a Harbor adapter." S1 `docs/blog/numbers.md` §Setup lists the pins: Claude Code 2.1.300, Codex 0.162.1, Pi 1.2.0, OpenCode 1.19.0, Hermes Agent v0.22.1, mini-swe-agent 2.4.6, Kimi Code 2.2.0, Qwen Code 0.25.2, OpenClaw 2026.10.4. S2 `config.harnesses.*.version` matches these. S1 §Evaluation protocol: "Pinned harness versions, vanilla prompts and tools, benchmark-default timeouts. Adapters change only how the model is reached and how the effort is set." Latest GitHub releases (S7, 23:00:55Z): claude-code v2.1.319, pi v1.5.0, hermes-agent v0.23.0, kimi-code 2.5.2, qwen-code v0.26.9, openclaw v2026.11.1, openhands v1.37.0, goose v1.57.0, codex rust-v0.169.0-alpha.2. mini-swe-agent's latest release is still v2.4.6 (S10).

Retrieved: 2026-11-06T22:53:03Z (S1), 22:53:52Z (S2), 23:00:55Z (S7)

Implication for the spec: The pinned version and install path are part of each participant's identity and must be fixed in the pre-registration packet. HF's numbers are a cost and behaviour prior for older versions. Under Q-X12 they are not a replication target. The spec must choose a version policy (Open questions, Q-C3-4).

Outline delta: EXTENDS OL-09 — candidate pool provenance and a version-pin rule.

Limitations: Release tags are not install-verified. Codex's newest tag is an alpha, so "latest stable" was not determined. Whether newer versions change cost or behaviour is unknown.

### F2 — On Terminal-Bench 2.1 with GLM-5.3-Flash the board measured $0.023–$0.084 per trial, and harness CIs all overlap

Claim: The public board payload gives per-harness Terminal-Bench 2.1 figures for GLM-5.3-Flash (87 tasks × 3 attempts). Resolve rates run 45.6–67.8%, with 95% CIs about ±10 points. Cost per scored trial runs $0.023 (Pi) to $0.084 (OpenClaw) at low effort, priced at the provider's rates with caching.

Status: VERIFIED (as a reading of the published payload; the underlying trials were not recomputed)

Evidence kind: source inspection

Evidence: S2 `leaderboard["zai-org/GLM-5.3-Flash"].stages.full.efforts.{low,high}.harnesses.<h>.benchmarks["terminal-bench-2.1"]`, fields `rate, ci, cost_per_trial, tokens_per_trial, calls_per_trial, agent_s_per_trial, n_infra, n_trials, pass_all3, pass_any3, rerun_flip, n_tasks`. Price basis is in S2 `config.models["zai-org/GLM-5.3-Flash"].pricing.zai-org` = input 0.15, cached_input 0.03, output 0.5 ($/Mtok, as of 2026-08-26). S1 §Evaluation protocol says "cost = metered tokens × the provider's list price, cached input at the cached rate", and per-trial cost averages over scored trials. Low-effort extract, sorted by rate:

| Harness (pin) | TB2.1 rate [95% CI] | $/trial observed | Mtok/trial | calls/trial | agent s/trial | infra/trials | pass^3 / pass@3 | rerun flip |
|---|---|---|---|---|---|---|---|---|
| Hermes v0.22.1 | 67.8 [57.4, 76.7] | 0.0376 | 0.678 | 21.5 | 677 | 0/261 | 51.7 / 83.9 | 21.5% |
| Kimi Code 2.2.0 | 65.1 [54.7, 74.3] | 0.0361 | 0.856 | 20.6 | 639 | 2/263 | 54.0 / 79.3 | 16.9% |
| mini-swe-agent 2.4.6 | 64.0 [53.5, 73.3] | 0.0434 | 1.031 | 31.8 | 760 | 2/263 | 46.0 / 81.6 | 23.8% |
| Claude Code 2.1.300 | 63.2 [52.7, 72.6] | 0.0384 | 0.583 | 18.4 | 690 | 3/264 | 41.4 / 83.9 | 28.4% |
| Pi 1.2.0 | 62.5 [52.0, 71.9] | 0.0227 | 0.342 | 17.4 | 630 | 7/268 | 42.5 / 82.8 | 26.8% |
| Codex 0.162.1 | 60.2 [49.7, 69.8] | 0.0632 | 1.775 | 39.2 | 612 | 134/395 | 39.1 / 80.5 | 27.6% |
| Qwen Code 0.25.2 | 57.1 [46.6, 67.0] | 0.0520 | 0.684 | 22.2 | 611 | 0/261 | 36.8 / 74.7 | 25.3% |
| OpenCode 1.19.0 | 50.6 [40.3, 60.8] | 0.0354 | 0.930 | 30.9 | 834 | 151/412 | 32.2 / 70.1 | 25.3% |
| OpenClaw 2026.10.4 | 45.6 [35.5, 56.0] | 0.0839 | 2.239 | 47.8 | 770 | 6/267 | 28.7 / 62.1 | 22.2% |

High effort: Pi 69.4 ($0.028), Codex 67.4 ($0.071), Kimi 67.0 ($0.043), mini-swe-agent 66.3 ($0.057), Hermes 65.1 ($0.039), Claude Code 62.5 ($0.044), Qwen 59.8 ($0.058), OpenCode 53.3 ($0.034), OpenClaw 50.6 ($0.051). Terminal-Bench is the board's cheaper benchmark: the S1 notebook (2026-10-29 15:30) gives DeepSWE at $0.26–0.36 per task and harness. In the same entry, 19–26 of the 87 TB2.1 tasks "carry no harness information for GLM and DeepSeek (mostly all pass)". The Codex and OpenCode infra counts mostly come from the board's own Responses bridge, which rejected requests before its fix (F11).

Retrieved: 2026-11-06T22:53:52Z

Implication for the spec: These are the best available per-harness token and cost priors for TB2.1 on this model. The overlap of every CI at N = 87 × 3 confirms T4 (ledger §0): an affordable tranche may honestly report harness differences as inconclusive, and the $60 pilot cannot rank anything. D1 should take the per-task "no information" observation into seeded task selection.

Outline delta: EXTENDS OL-02 — cost and variance priors per harness. CONFIRMS T4.

Limitations: These are figures for HF's router and zai-org's serving, HF Sandbox, HF's adapters and HF's effort enforcement. Ollama's serving, latency and caching may differ (F3). Output-token share is not in the payload, so the no-cache bounds in F3 are estimates. The answer-key and cheating rules HF applied (fails counted) are baked into `rate`.

### F3 — Ollama Cloud's `glm-5.3-flash` price equals the board's price basis; cached share is the open variable

Claim: Ollama Cloud lists `glm-5.3-flash` at $0.15 input, $0.03 cached input and $0.50 output per Mtok, which is the board's basis exactly. The board's $/trial therefore transfers to Ollama except for the cached-input share. That share is undocumented on Ollama, and without caching TB2.1 trials would cost roughly 2.3–4× more.

Status: PARTIALLY VERIFIED (prices verified; the no-cache figures are inference)

Evidence kind: source inspection + inference

Evidence: S4 model pricing table: "glm-5.3-flash $0.15 $0.03 $0.50", "Prices are per million tokens. Off-peak pricing apply outside 12:00 and 18:00 UTC on weekdays and all day on weekends". Off-peak rows exist for deepseek models only, not for `glm-5.3-flash`. S3 `api/cloud-usage.md` shows usage records with `cached_input_tokens` and `usage_usd` per model (example group `glm-5.3-flash`), so Ollama meters a cached tier. S3 `api/anthropic-compatibility.md` "Not supported" table: "Prompt caching | `cache_control` blocks for caching prefixes". Neither compatibility page documents automatic prefix caching. S1 decisions record HF's provider caching level: "GLM-5.3-Flash's high-effort smoke: … 98% cached". Inference: with all input uncached, cost per trial ≈ tokens/trial × $0.15/Mtok plus a small output premium. The hard ceiling is tokens/trial × $0.50/Mtok.

| Harness | observed (cached) | no-cache estimate ≈ T×0.15 | ceiling T×0.50 |
|---|---|---|---|
| Pi | 0.023 | 0.051 | 0.171 |
| Claude Code | 0.038 | 0.087 | 0.291 |
| Hermes | 0.038 | 0.102 | 0.339 |
| Qwen Code | 0.052 | 0.103 | 0.342 |
| Kimi Code | 0.036 | 0.128 | 0.428 |
| OpenCode | 0.035 | 0.140 | 0.465 |
| mini-swe-agent | 0.043 | 0.155 | 0.515 |
| Codex | 0.063 | 0.266 | 0.887 |
| OpenClaw | 0.084 | 0.336 | 1.119 |

(Low effort, $/trial.)

Retrieved: 2026-11-06T22:55:00Z (S4), 22:54:29Z (S3), 22:53:52Z (S2)

Implication for the spec: Budget enforcement should plan at the no-cache estimate until pilot P0 measures Ollama's cached share per endpoint (OpenAI-chat versus Anthropic-messages) from `/api/usage`. That measurement is C1's question. The proxy's receipts must record cached and uncached input tokens per call.

Outline delta: EXTENDS OL-02 and OL-03 — a price-basis identity, plus caching as the dominant cost uncertainty.

Limitations: Ollama prices are a living page and may change. Whether Ollama caches automatically on either endpoint is unknown. The tokens/trial figures come from HF's runs and versions.

### F4 — Ollama's compatibility limits hit specific harnesses: Responses is stateless with no freeform-tool replay, and the Anthropic endpoint has no `cache_control`

Claim: Ollama Cloud lacks `tool_choice` on both compatible APIs. Its Anthropic endpoint lacks `cache_control`, `count_tokens` and batches, and accepts but does not enforce `budget_tokens`. Its Responses API is stateless only and cannot replay custom or freeform tool calls in the cloud. This is a direct risk for Codex and a cost risk for Claude Code.

Status: VERIFIED for the endpoint limits; per-harness effects are partly inference

Evidence kind: source inspection + inference

Evidence: S3 `api/openai-compatibility.md`: "`[ ] tool_choice`" under chat request fields; supported `reasoning_effort` and `reasoning` "use `/api/show` to discover each model's supported values"; also "The cloud API does not support stateful Responses, built-in web search through `/v1/responses`, or custom/freeform tool-call replay", and "Only the non-stateful flavor is supported (i.e., there is no `previous_response_id` or `conversation` support)". S3 `api/anthropic-compatibility.md`: "`[ ] tool_choice`"; Not supported: `/v1/messages/count_tokens`, `tool_choice`, "Prompt caching | `cache_control` blocks", batches; "Extended thinking | Basic support; `budget_tokens` accepted but not enforced"; "The cloud endpoint requires `Authorization: Bearer`; it does not accept `x-api-key` alone". S8 `llm-gateway-protocol.md` feature table, prompt caching row: Claude Code "attaches `cache_control` markers to `system` blocks and to `messages` entries"; if the gateway drops them there is "No error: the conversation bills as uncached input on every turn". The token-counting row: Claude Code "falls back to a character-based estimate". S6 `codex.py`: `supported_api_formats=[ModelApiFormat.OPENAI_RESPONSES]`. S1 decisions, Codex row: Codex needed its own model provider so it "compacts long histories locally" because "the built-in OpenAI provider asks OpenAI's server to compact". mini-swe-agent's `src/` contains no `tool_choice` reference (S10 grep, 22:58:28Z).

Retrieved: 2026-11-06T22:54:29Z (S3), 22:56:56Z (S8), 22:55:31Z (S6)

Implication for the spec: (a) The proxy must detect and record `tool_choice` and `cache_control` in requests and decide pass, strip or refuse as a declared, versioned policy. An unannounced strip changes the participant (F11). (b) Plan Claude Code on Ollama's Anthropic endpoint at the no-cache estimate. (c) Codex cannot be admitted on the proxy as briefed, which offers OpenAI-chat and Anthropic-messages only, unless the proxy adds a Responses pass-through or a declared translation. Either way, freeform `apply_patch` replay is an unverified risk.

Outline delta: EXTENDS OL-02 — endpoint-compatibility gate.

Limitations: Whether Ollama ignores or rejects an unsupported `tool_choice` or `cache_control` was not observed, because no inference ran. Which harnesses send `tool_choice` was checked only for mini-swe-agent. Claude Code is closed source.

### F5 — Wire format and base-URL support by candidate (Harbor's adapter contract)

Claim: Harbor's adapters declare each harness's supported API formats and whether a custom base URL is accepted. Gemini CLI (Google format only), Cursor CLI (no base-URL capability; Cursor-hosted models) and Antigravity (Google) cannot be pointed at an OpenAI or Anthropic proxy. Codex is Responses-only and Claude Code is Anthropic-Messages-only. Everything else on the shortlist accepts OpenAI chat.

Status: VERIFIED (adapter declarations at the cited commit, not runtime behaviour)

Evidence kind: source inspection

Evidence: S6 `ModelConnectionSpec.supported_api_formats` and `AgentCapabilities(base_url=…)`:

- claude_code: `[ANTHROPIC_MESSAGES]`, base URL via `ANTHROPIC_BASE_URL`.
- codex: `[OPENAI_RESPONSES]`.
- opencode: maps `@ai-sdk/openai-compatible` → chat and `@ai-sdk/anthropic` → messages.
- openhands: chat and messages; `disable_tool_calls` option "Disable native function calling".
- openhands_sdk: chat, responses and messages.
- goose: chat and messages.
- aider: chat and messages.
- pi: chat, responses and messages.
- hermes: chat (`chat_completions`), responses and messages.
- kimi_code: chat and messages; `KIMI_MODEL_BASE_URL`.
- qwen_code: chat only.
- openclaw: responses, chat, messages and google.
- gemini_cli: `[GOOGLE_GENERATE_CONTENT]`.
- copilot_cli: chat, responses and messages; BYOK through `COPILOT_PROVIDER_BASE_URL`.
- swe_agent: chat and messages.
- trae_agent: chat, responses and messages.
- mini_swe_agent: chat, responses and messages.
- terminus_2: LiteLLM with `api_base`.
- cursor_cli: `AgentCapabilities(reasoning_effort, atif, skills, mcp_servers)` with no `base_url`, and requires `CURSOR_API_KEY`.
- cline: no `base_url` capability declared; default install `cline@nightly`.

S5 `agent-auth-resolution.md`: `--base-url`, `--api-key-name`, `--api-format` overrides and `--skip-model-parsing-and-provider-mapping`. S3 Ollama documents direct-cloud recipes for Claude Code (`ANTHROPIC_BASE_URL=https://ollama.com`, `ANTHROPIC_AUTH_TOKEN`, `claude --model glm-5.3-flash`), Codex (`wire_api = "responses"`), OpenCode (`@ai-sdk/openai-compatible`), Pi (`"api": "openai-completions"`), Hermes (custom endpoint `/v1`), Droid (`generic-chat-completion-api`), Copilot CLI (`COPILOT_PROVIDER_WIRE_API=responses`), Goose (`OLLAMA_HOST=https://ollama.com`, Ollama-native API), Cline CLI (Ollama provider), DeepSeek Harness, Oh My Pi and Pool.

Retrieved: 2026-11-06T22:55:31Z (S6), 22:55:17Z (S5), 22:54:29Z (S3)

Implication for the spec: The endpoint gate is mechanical once the proxy's formats are fixed. With chat + messages only, admissible harnesses use chat or messages. OpenCode should run on `@ai-sdk/openai-compatible` (chat), not the Responses route HF used. Its HF figures are then a weaker prior, and the configuration difference must be recorded. Goose's Ollama-native provider would bypass both proxy formats, so Goose must be configured as an OpenAI-chat provider instead.

Outline delta: EXTENDS OL-09 — endpoint/format admission gate.

Limitations: Declared adapter support is not proof of a working call against Ollama via HH's proxy. Droid, Crush, Kilo CLI, DeepSeek Harness, Pool and Oh My Pi have no Harbor adapter at S6.

### F6 — Claude Code is proprietary under Anthropic's Commercial Terms, which contain no benchmarking clause but do restrict competing use, modification and branding

Claim: Claude Code is closed source and "Use is subject to Anthropic's Commercial Terms of Service". Those Terms forbid building a competing product, reverse engineering or duplicating the Services, and reselling. Claude Code's legal page requires the binary to be run unmodified when it runs in others' products or sandboxes. Its docs state that Anthropic "doesn't support routing Claude Code to non-Claude models through any gateway", which is a support statement, not a stated prohibition. Name use must be plain-text and accurate, and logos need permission. No clause addressing automated benchmarking was found.

Status: PARTIALLY VERIFIED (text verified; legal interpretation is not established and is not legal advice)

Evidence kind: source inspection

Evidence: S7 `anthropics/claude-code/LICENSE.md`: "© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service". GitHub reports SPDX `none`. S9 Commercial Terms (Effective Jun 17, 2025) define "Services" as "Anthropic API keys and any other Anthropic offerings that references these Terms, as well as all related Anthropic tools". §D.4: "Customer may not and must not attempt to (a) access the Services to build a competing product or service, including to train competing AI models or resell the Services except as expressly approved by Anthropic; (b) reverse engineer or duplicate the Services; or (c) support any third party's attempt…". §D.2 incorporates the Usage Policy. S8 `legal-and-compliance.md` §"Can customers offer Claude Code in their products?": "preinstalling or running Claude Code in your products or services (e.g. in hosted sandboxes or other agent infrastructure) requires agreeing to our Commercial Terms … The Claude Code binary must not be modified … Customers may not pay for, resell, or intermediate Claude usage on their end users' behalf". On names: "You can accurately say, in plain text, that your product … runs Claude Code. But you can't use the Claude Code or Anthropic names or logos … in a way that suggests Anthropic built, endorses, or is partnered". S8 `llm-gateway.md` and `gateways.md`: Anthropic "doesn't support routing Claude Code to non-Claude models through any gateway". S9 Consumer Terms (Effective Oct 8, 2025) forbid access "through automated or non-human means … Except when you are accessing our Services via an Anthropic API Key". This applies to subscription logins, which the program does not use. Keyword greps of S9 for "benchmark" found nothing. Telemetry control: S8 `env-vars.md` `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` "disable[s] … auto-updates, telemetry, error reporting…", and Harbor sets it (S6 `claude_code.py:2126`).

Retrieved: 2026-11-06T22:56:33Z (S7), 22:56:56Z (S8), 22:57:10Z (S9)

Implication for the spec: Claude Code can enter only under an **operator-acknowledged legal posture** recorded in its admission record. (1) Run the published binary unmodified, with mediation at the network or proxy level only. (2) Use no Claude account and no consumer login; the credential is the HH proxy token. (3) Use Claude Code only as a black-box participant, never as a Phase-3 HIR replication target, because of D.4(b). (4) On the results site, refer to it by plain-text name only, with no logo (F2 territory). (5) Disclose that Anthropic does not support this configuration. The HF board publishes Claude Code results on GLM and DeepSeek. That is precedent, not authorization.

Outline delta: EXTENDS OL-09 — legal-posture gate; adds a constraint for the site (F2) and for phase 3 (E1/E2).

Limitations: I am not a lawyer. Whether running Claude Code with no Anthropic model in the loop is "use of the Services" under the Commercial Terms, and whether HH counts as a "competing product" in Anthropic's sense, are matters for the operator. Service Specific Terms and the Usage Policy were not read.

### F7 — Other closed or source-available harnesses: Copilot CLI may be run but not modified; Crush is FSL (non-OSI); Cursor and Droid are not admissible as briefed

Claim: GitHub's Copilot CLI license grants running copies and bars modification. Crush is under the Functional Source License 1.1 with an MIT future license, which is not OSI open source today. Cursor CLI needs Cursor credentials and Cursor-served models. Droid is closed and its offline/BYOK login behaviour was not verified.

Status: PARTIALLY VERIFIED

Evidence kind: source inspection

Evidence: S7 `github/copilot-cli/LICENSE.md` §1: "license to install and run copies"; §3: no right to "Modify, adapt, translate, or create derivative works". S6 `copilot_cli.py`: BYOK path "A provider slug or inference configuration selects BYOK", setting `COPILOT_PROVIDER_BASE_URL` and `COPILOT_PROVIDER_TYPE`, versus the GitHub-token path "keeps Copilot's hosted service". S7 `charmbracelet/crush/LICENSE.md`: "Functional Source License, Version 1.1, MIT Future License (FSL-1.1-MIT)". S6 `cursor_cli.py`: requires `CURSOR_API_KEY` and declares no base URL. S3 `integrations/droid.md`: `~/.factory/config.json` `custom_models` with `base_url`, `provider: generic-chat-completion-api`. Install is `curl -fsSL https://app.factory.ai/cli | sh`. No Harbor adapter.

Retrieved: 2026-11-06T22:56:33Z (S7), 22:55:31Z (S6), 22:54:29Z (S3)

Implication for the spec: Copilot CLI is a possible second closed harness, a reported alternate requiring a GitHub-terms review and an egress probe. Cursor fails the endpoint gate. Droid and Crush are deferred: Droid has no adapter and unverified login and egress; Crush is non-OSI and has no adapter.

Outline delta: EXTENDS OL-09.

Limitations: GitHub's product terms beyond the LICENSE file, Factory's terms and Cursor's terms were not read.

### F8 — Harbor's install templates fetch from the network at setup and often default to latest or unpinned, so HH needs pinned, prebuilt images

Claim: Harbor installs most agents inside the task container at trial setup from npm, PyPI/uv, curl scripts or GitHub, and uses latest when no version is given. Some paths are unpinned by construction: Hermes `install.sh` from `main`, Cline `@nightly`, and mini-swe-agent's unpinned `litellm`. HF had to add pins and runtime repairs. Harbor's `skip_install` lets a prebuilt image supply the agent.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S6 install commands:

- claude_code: `npm install -g @anthropic-ai/claude-code@<v>`, or `curl … downloads.claude.ai/claude-code-releases/bootstrap.sh`.
- codex: `npm install -g @openai/codex<spec>`, plus nvm via `raw.githubusercontent.com`.
- opencode: `npm i -g opencode-ai<spec>`.
- pi: `npm install -g --ignore-scripts <pkg><spec>`.
- hermes: `curl … raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup --branch <v>`.
- kimi_code: `npm install --global @moonshot-ai/kimi-code<spec>`.
- qwen_code: `npm install -g @qwen-code/qwen-code@<v|latest>`.
- openclaw: nvm, then `npm install -g openclaw<spec>`.
- openhands: `uv pip install openhands-ai==<v>`, or git.
- goose: `curl … github.com/block/goose/releases/download/<v>/download_cli.sh`.
- mini_swe_agent: `curl astral.sh/uv/install.sh`, then `uv tool install mini-swe-agent==<v> --with 'litellm[mcp]' …` with no litellm pin.
- cline: "installs cline@nightly from npm (default behavior)".

S5 `pre-integrated-agents.md` §Pre-installed agents: "If the task image already contains the agent, for example because the sandbox has no internet access, skip the install with `skip_install` … The recorded agent version is the one the image provides". S1 decisions, Harness adapters: mini-swe-agent "Installed with `litellm==1.103.5` (Harbor installs it unpinned)", because "litellm 1.106.0 … imports `mcp` without depending on it, so mini-swe-agent exited before its first model call". Kimi Code needs Node 22.19+. OpenClaw needs Node ≥ 24.16. Hermes used PyPI. The worker sets `UV_EXCLUDE_NEWER` to the launch time.

Retrieved: 2026-11-06T22:55:31Z (S6), 22:55:17Z (S5), 22:53:03Z (S1)

Implication for the spec: The install-reproducibility gate requires that each admitted harness be installable at **image-build time** with an exact version plus a lock or resolution cutoff (npm lockfile or tarball hash; uv `--exclude-newer` or lock), and that it then run in a cell whose only egress is the proxy. Both O and H surfaces must use the same image (Harbor `skip_install`) for parity. C4 owns the mechanics; B3 sizes image build.

Outline delta: EXTENDS OL-02 — install reproducibility as a hard gate.

Limitations: No image was built. Some harnesses (Hermes, OpenHands) may fetch skills, models or catalogs at first run, which only an egress probe would reveal.

### F9 — Several harnesses carry web search routed through Ollama, telemetry or update checks, so disabling them is an admission obligation

Claim: Ollama's own integrations wire web search into Claude Code, Codex, Pi, OpenClaw and DeepSeek Harness through Ollama's web-search service, which is metered usage. Harnesses also emit telemetry or update checks. Harbor exposes a web-tool disable switch only for a subset. TB2.1 tasks "need the internet and keep it" on the HF board, and HF confirmed 134 answer-key lookups across its three released models. Harness web tools are therefore a validity risk as well as an egress risk.

Status: VERIFIED (sources); the per-harness egress inventory is UNVERIFIED until probed

Evidence kind: source inspection

Evidence: S3:

- `integrations/claude-code.md`: Capabilities "Web search — Search the web through Ollama"; "Hosted WebSearch and advanced tool controls are not fully supported".
- `integrations/codex.md`: "Codex web-search requests sent through the Ollama profile are executed by Ollama"; disable with `-c 'web_search="disabled"'`.
- `integrations/pi.md`: `@ollama/pi-web-search` package.
- `integrations/openclaw.md`: "bundled Ollama `web_search` provider … enabled automatically".
- `integrations/deepseek-harness.md`: "Web search is enabled automatically".
- `api/cloud-usage.md`: "Usage statistics for cloud inference, web search, and web fetch".

S5 capability table, Native web tools ("Disable built-in web search and fetch tools with `disable_web_search`"): `claude-code, codex, copilot-cli, cortex-code, deerflow, gemini-cli, mimo, openclaw, opencode, grok-build`. Telemetry handling in S6 adapters: `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` (claude_code), `KIMI_DISABLE_TELEMETRY=true` (kimi_code), `AGENT_ENABLE_BROWSING=false` and `ENABLE_BROWSER=false` (openhands). S1 README: "Terminal-Bench tasks need the internet and keep it"; "Cheating counts as a fail … calling another model counts as a fail". S1 decisions, Known deviations: 134 confirmed Terminal-Bench answer-key trials (GLM 2, DeepSeek 102, Flint 30).

Retrieved: 2026-11-06T22:54:29Z (S3), 22:55:17Z (S5), 22:55:31Z (S6), 22:53:03Z (S1)

Implication for the spec: (a) The HH proxy must forward **only** model-inference routes and refuse Ollama's web-search and web-fetch routes and any non-model endpoint. This holds the cost line and enforces "no other model / no lookup". (b) Each admission records the harness's own non-model egress attempts, observed by an egress probe in a cell whose only route is the proxy, and the switches used to disable them. (c) Task-level internet (B1/B2, ADR-0421 `declared_deps`) is a separate axis. Where tasks get egress, a cheating scan (D1) is required, as HF found.

Outline delta: EXTENDS OL-02 — egress containment and validity.

Limitations: No harness was probed. Telemetry endpoints of Hermes, Pi, Qwen Code, OpenCode, Goose and OpenHands were not inventoried from source.

### F10 — Trajectory export is uneven, so the proxy's call record must be the primary transcript

Claim: Harbor lists ATIF output for most candidates. HF still needed its own converters for Kimi Code and OpenClaw, lost about a quarter of Hermes trajectories to an export bug, and kept only reconstructed OpenClaw transcripts. HF therefore measured cost and calls "outside the harness" with an in-sandbox meter and ran answer-key detection on what the model received.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S5 ATIF capability list includes `claude-code, cline-cli, codex, copilot-cli, cursor-cli, gemini-cli, goose, hermes, kimi-cli, mini-swe-agent, openclaw, opencode, openhands, openhands-sdk, qwen-coder, swe-agent, terminus-2, trae-agent, pi, …`. It omits `kimi-code`, although S6 `kimi_code.py` declares `atif=True` and ships `kimi_code_trajectory.py`. S1 decisions: "Kimi Code and OpenClaw write ATIF through our converters"; Hermes "trajectories missing for about a quarter of trials (fixed for runs launched after 2026-10-20)"; "Cost and time come from the meter … Harness self-reports differ and lose usage on timeouts"; answer keys "are detected from what the model received … detection must not depend on the harness".

Retrieved: 2026-11-06T22:55:17Z (S5), 22:55:31Z (S6), 22:53:03Z (S1)

Implication for the spec: The hard gate is that **every model call is recorded by the HH proxy**, as a verbatim request/response receipt. A native trajectory (ATIF or own format) is a reported attribute that improves per-step tool attribution. The site's trajectory viewer (F2) must render from proxy receipts when no ATIF exists.

Outline delta: EXTENDS OL-02.

Limitations: ATIF fidelity per harness was not checked.

### F11 — The adapter is part of the participant: translation layers silently changed two harnesses on the HF board

Claim: HF's Responses bridge dropped Codex's and OpenCode's system or developer messages and forced `temperature=1`/`top_p=1` on every call. Joel re-ran every Codex and OpenCode trial and made the adapter build part of each harness's compared setup.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S1 notebook 2026-10-29 09:10: "responses.js drops developer messages and adds temperature 1 and top_p 1 … So Codex and OpenCode ran without their own system prompts on every board run". S1 decisions, Architecture/Responses bridge: "**The adapter is part of the setup:** a harness declares its adapter … reuse … and the board … compare it like the harness version". The same section records 353 bridge-rejected trials (images in tool results, remote compaction) reclassified as infra.

Retrieved: 2026-11-06T22:53:03Z

Implication for the spec: Any proxy behaviour that rewrites a request (role mapping, effort injection, sampling defaults, stripped fields, Anthropic↔chat translation) must be versioned, recorded in each participant's identity, and covered by deterministic request-level conformance fixtures before paid use. This mirrors D2's L1 idea for external harnesses.

Outline delta: EXTENDS OL-02.

Limitations: HH's proxy does not yet exist (T2). Its translation surface is a C2 decision.

### F12 — Terminus-2 parses text actions and runs host-side; mini-swe-agent v2 defaults to native tool calling

Claim: Terminus-2 is a Harbor-internal `BaseAgent` whose loop runs in the Harbor process. It drives a tmux session in the container and parses JSON or XML text responses, so it does not depend on native tool calling. mini-swe-agent v2 uses native tool calling by default and keeps a text-based mode. The other shortlisted harnesses use native tool calling.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S6 `terminus_2/terminus_2.py`: `class Terminus2(BaseAgent)`, `parser_name: str = "json"` ("json" or "xml"), `TerminusJSONPlainParser`/`TerminusXMLPlainParser`, `api_base`, `enable_summarize=True`, `proactive_summarization_threshold=8000`. Installed agents subclass `BaseInstalledAgent`, as in `copilot_cli.py`. S10 `v2_migration.md`: "v2.0 uses native tool calling by default (instead of regex-based text parsing)"; `model_class: litellm_textbased` for text mode; "The CLI uses `mini.yaml` and `swebench.yaml` which are configured for tool calling".

Retrieved: 2026-11-06T22:55:31Z (S6), 22:58:21Z (S10)

Implication for the spec: Terminus-2 is robust to Ollama's missing `tool_choice`. Under the H surface, though, its model calls originate outside the cell, so C4 must choose between running the loop host-side against the proxy and packaging it into the cell. That choice is a hosting difference to declare in parity. mini-swe-agent's mode (tool-calling vs text) is a configuration that must be pinned, and E1 will specify it.

Outline delta: EXTENDS OL-02; feeds E1/C4.

Limitations: No Terminus-2 cost on GLM-5.3-Flash was found, so its $/trial below is an estimate.

### F13 — HF's harness-specific failure modes that HH must expect

Claim: The board recorded harness × provider interactions that would show up as harness effects if not handled.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S1 decisions:

- Pi sends role "developer" with thinking on, which zai rejects, fixed with `compat.supportsDeveloperRole=false`. Pi exits on an instruction beginning with "-", fixed by ending flags with `--`.
- Qwen Code: GLM streamed 32,000 reasoning tokens (its `max_tokens`), then the 15-minute per-call cap, in 24 trials.
- OpenClaw sends `max_completion_tokens` 8,192 on every call, an "as-shipped" cap that ends runs.
- mini-swe-agent's non-streaming LiteLLM calls hit the router's 504 after 120 s, losing 35 pilot trials, until the meter streamed upstream.
- Kimi Code needs `[thinking] enabled = true` or it sends no effort.
- Hermes needs `hermes pm repair` and PyPI.
- Codex needs `web_search` and `multi_agent` off.
- Some harnesses expose their Node to the agent shell.
- "Harness defaults differ (none, `medium`, provider default ≈ 2× `high`), so without pinning the comparison measures effort, not harness."

Retrieved: 2026-11-06T22:53:03Z

Implication for the spec: Admission includes a per-harness configuration record (effort mechanism, role compatibility, output caps, streaming) and a P0 smoke designed to surface these. Effort is pinned per participant and enforced or recorded at the proxy.

Outline delta: EXTENDS OL-02.

Limitations: These failures arose on zai-org via HF's router. Ollama may differ, for example in developer-role acceptance and stall timeouts.

### F14 — Ollama plan concurrency is small, and a $60 figure matches the Pro plan's monthly included credit

Claim: Ollama's pricing page lists 3 concurrent requests for Pay-as-you-go and 10 for Max. Pro includes "$60 of usage credits per month", which "does not roll over". If the operator's $60 is Pro's included credit, it resets monthly and constrains timing.

Status: PARTIALLY VERIFIED (page text verified; which plan the operator holds is unknown)

Evidence kind: source inspection

Evidence: S4: "Pay as you go $0 … 3 concurrent requests"; "Pro … $60 of usage credits per month"; "Max … $300 of usage credits per month … 10 concurrent requests"; "Does unused included usage roll over…? No."; "Every plan … can buy usage credits". S2 shows HF trials average 611–974 s of agent time, with a duty cycle of 0.78 on model waits (S1).

Retrieved: 2026-11-06T22:55:00Z

Implication for the spec: At 3 concurrent requests, about 3–4 trials run at once. A 184-trial pilot then needs about 10–12 h of wall time, which affects B3's VM-hours quote. Budget records must state whether the credit is one-off or monthly. C1 owns verification via `/api/balance`.

Outline delta: EXTENDS OL-03/OL-08.

Limitations: Pro's concurrency is not stated on the page. The account's plan was not read.

## Candidate survey (admission-criteria table)

Legend: **✓** meets; **✗** fails a hard gate; **?** unverified; **G**, gate notes. "HF TB2.1 $" is low-effort $/trial with caching (F2). License per S7 SPDX or LICENSE text. Harbor adapter per S5 and S6. Formats per S6 (C = OpenAI chat, R = Responses, M = Anthropic Messages, G = Google).

| Harness | License | Headless | Pin (HF / latest seen) | Harbor | Formats; Ollama fit | Tool use | Install | Non-model egress | Trajectory | HF TB2.1 rate / $ | Disposition |
|---|---|---|---|---|---|---|---|---|---|---|---|
| mini-swe-agent | MIT ✓ | ✓ (`mini`, Harbor run) | 2.4.6 / 2.4.6 | ✓ | C,R,M; C via LiteLLM ✓ | native (text mode available) | uv/PyPI; pin litellm (F8) | none known; ? | own JSON → ATIF ✓ | 64.0 / 0.043 | **Shortlist #1** |
| Terminus-2 | Apache-2.0 (in Harbor) ✓ | ✓ | Harbor 0.25.0 commit | ✓ (built-in) | LiteLLM `api_base` ✓ | **text JSON/XML** | ships with Harbor; host-side loop | none (tmux only) | ATIF ✓ | n/a | **Shortlist #2** (C4: hosting) |
| Claude Code | Proprietary (Commercial Terms) G | ✓ `-p` | 2.1.300 / 2.1.319 | ✓ | M only; Ollama documents direct cloud ✓; no `cache_control` → uncached risk | native | npm or bootstrap | telemetry/updates (disable var ✓); web via Ollama (disable ✓) | ATIF ✓ | 63.2 / 0.038 | **Shortlist #3** with legal-posture record |
| Pi | MIT ✓ | ✓ | 1.2.0 / 1.5.0 | ✓ | C,R,M; C ✓ (developer-role compat) | native (4 tools) | npm | web only via optional package | ATIF ✓ | 62.5 / 0.023 | **Shortlist #4** |
| Hermes Agent | MIT ✓ | ✓ (oneshot) | v0.22.1 / v0.23.0 | ✓ | C,R,M; C custom ✓ | native, 70+ skills | Harbor: `install.sh` from **main**; pin via PyPI | gateways and skills: ? | ATIF (export bug fixed) | 67.8 / 0.038 | **Shortlist #5** (heavyweight) |
| OpenHands | MIT ✓ | ✓ (`openhands.core.main --task`) | — / v1.37.0 | ✓ (+ sdk) | C,M (sdk adds R) ✓ | native (text fallback option) | uv pip `openhands-ai==v` | browsing off in Harbor; telemetry ? | ATIF ✓ | n/a | **Shortlist #6** (heavyweight beyond HF set) |
| Kimi Code | MIT ✓ | ✓ | 2.2.0 / 2.5.2 | ✓ | C,M ✓ | native | npm, Node ≥ 22.19 | telemetry var ✓; web tools ? | Harbor converter | 65.1 / 0.036 | **Shortlist #7** |
| OpenCode | MIT ✓ | ✓ `run` | 1.19.0 / (no CLI tag seen) | ✓ | C via openai-compatible ✓ (HF used R) | native | npm / curl | webfetch/websearch (disable ✓); catalog fetch ? | ATIF ✓ | 50.6 / 0.035 | **Shortlist #8** (most-starred) |
| Qwen Code | Apache-2.0 ✓ | ✓ | 0.25.2 / 0.26.9 | ✓ | C only ✓ | native | npm | ? | ATIF ✓ | 57.1 / 0.052 | Alternate |
| Goose | Apache-2.0 ✓ | ✓ | — / v1.57.0 | ✓ | C,M ✓ (avoid Ollama-native provider) | native | release script/binary | ? | ATIF ✓ | n/a | Alternate |
| Codex CLI | Apache-2.0 ✓ | ✓ `exec` | 0.162.1 / 0.169.0-α | ✓ | **R only**; Ollama R stateless, no freeform replay (F4) | native | npm | web search via Ollama (disable ✓) | ATIF ✓ | 60.2 / 0.063 | **Deferred** (needs proxy R decision) |
| OpenClaw | MIT ✓ | ✓ (`--yes`) | 2026.10.4 / 2026.11.1 | ✓ | R,C,M,G ✓ | native | npm, Node ≥ 24.16 | gateway daemon; bundled web search | reconstructed only | 45.6 / 0.084 | Deferred (cost, 8,192 cap, traces) |
| SWE-agent | MIT ✓ | ✓ | — / v1.1.0 (2025-05) | ✓ | C,M ✓ | native | uv from repo | ? | ATIF v1.5 | n/a | Reported; stale releases |
| Trae-agent | MIT ✓ | ✓ | — / (pushed 2026-02) | ✓ | C,R,M | native | ? | ? | ATIF | n/a | Reported; stale |
| Aider | Apache-2.0 ✓ | ✓ `--yes --message` | — / (pushed 2026-05) | ✓ | C,M | text edit formats | curl install.sh | ? | none in Harbor ATIF list | n/a | Excluded: edit-assistant, not a terminal agent |
| Copilot CLI | Proprietary (run ✓, modify ✗) G | ✓ `-p --yolo` | — | ✓ | C,R,M BYOK ✓ | native | npm/brew | GitHub services ? | ATIF ✓ | n/a | Closed alternate (terms review + probe) |
| Cline CLI | Apache-2.0 ✓ | ? | `@nightly` default | ✓ (no base_url cap) | Ollama provider | native | npm | ? | ATIF ✓ | n/a | Deferred (pinning/base URL) |
| Gemini CLI | Apache-2.0 ✓ | ✓ | — | ✓ | **G only ✗** | — | npm | — | ATIF | n/a | **Fails endpoint gate** |
| Antigravity CLI/SDK | ? | ? | — | ✓ | G (base_url helper) ✗ | — | — | — | ATIF | n/a | **Fails endpoint gate** |
| Cursor CLI | Proprietary | ✓ | — | ✓ | **no base URL ✗** | — | — | Cursor service | ATIF | n/a | **Fails endpoint gate** |
| Droid (Factory) | Proprietary ? | ? | — | ✗ | C BYOK (Ollama doc) | native | curl script | Factory login ? | ? | n/a | Deferred (no adapter; terms) |
| Crush | FSL-1.1-MIT (non-OSI) | ? | — | ✗ | ? | native | ? | ? | ? | n/a | Deferred |
| DeepSeek Harness (dsh) | MIT ✓ | ? (browser UI) | — ("developer preview") | ✗ | Ollama launch | native | npm | web search auto-on | ? | n/a | Deferred |
| Kilo Code (CLI) | MIT ✓ | ? | — | ✗ | ? | native | ? | ? | ? | n/a | Not surveyed further |
| Roo Code | Apache-2.0, **archived** | — | — | ✗ | — | — | — | — | — | n/a | Excluded (archived) |
| kimi-cli | Apache-2.0, **archived** | — | — | ✓ | — | — | — | — | — | n/a | Excluded (superseded by Kimi Code) |

Harbor also lists harnesses not surveyed here: `deerflow, devin, dspy-rlm, eve, fx, grok-build, junie, langgraph, mcode, mimo, muse-code, nemo-agent, rovodev-cli, strands, vibe, fabric, cortex-code, computer-1` (S5). Each can be screened later with the same criteria.

## Ranked pilot shortlist and expected cost

Ranking weights operator intent (Q-X7) first, then evidence of fitness on this model (F2), then endpoint fit and egress risk (F4, F5, F9), then diversity of design (bash-only, text-parsed, minimal, heavyweight, closed).

| # | Harness | Why | Planning $/trial (no-cache est.) | Prior (cached, HF) | Basis |
|---|---|---|---|---|---|
| 1 | mini-swe-agent 2.4.6 | Named by the operator; HIR target (A-8); ties for first on GLM | 0.155 | 0.043 | F2/F3 |
| 2 | Terminus-2 (Harbor 0.25.0) | Named; Harbor's reference agent; text actions avoid `tool_choice` issues; HIR target #2 | 0.18 (est.) | ~0.05 (est.) | **inference**: bash/tmux full-history loop assumed ≈1.15× mini-swe-agent tokens; pilot measures |
| 3 | Claude Code 2.1.x | Named closed harness; Ollama documents the path | 0.087 (expect no cache on M) | 0.038 | F3/F4/F6 |
| 4 | Pi | Cheapest; tied for first on GLM; minimal 4-tool design | 0.051 | 0.023 | F2/F3 |
| 5 | Hermes Agent | Heavyweight OSS; best TB2.1 rate at low effort | 0.102 | 0.038 | F2/F3 |
| 6 | OpenHands | Canonical heavyweight OSS outside the HF set | 0.25 (est.) | ~0.06 (est.) | **inference**: bounded between Codex and OpenClaw token use |
| 7 | Kimi Code | Top-tier on GLM; distinct vendor design | 0.128 | 0.036 | F2/F3 |
| 8 | OpenCode | Most-starred OSS harness; chat route differs from HF | 0.140 | 0.035 | F2/F3 |

Sum over the eight per (task × attempt): **≈ $1.09** planning (no-cache), **≈ $0.32** cached prior, and **≈ $3.66** ceiling (all tokens at the output price; not a realistic plan).

**What $60 covers** (design proposal; planning value = no-cache estimate × 1.5 tail reserve, which mirrors HF's reserve of the larger of mean and p90):

| Stage | Design | Trials | Expected (cached → no-cache) | Planning (×1.5) |
|---|---|---|---|---|
| P0 smoke | 3 tasks × 1 attempt × 8 harnesses | 24 | $1.0 → $3.3 | $4.9 |
| P1 pilot | 10 tasks × 2 attempts × 8 harnesses | 160 | $6.5 → $21.9 | $32.8 |
| Infra re-runs | ≤ 20% of P1 | ≤ 32 | — | $6.6 |
| **Total** | | **≈ 184–216** | **$7.5 → $25** | **≈ $44** (≈ $16 contingency) |

Alternatives within $60 at planning values: 6 harnesses (drop OpenHands and OpenCode; sum ≈ $0.70) × 15 tasks × 2 attempts ≈ $32. Or 8 harnesses × 20 tasks × 1 attempt ≈ $33. If P0 shows Ollama caching near HF's level, the same $60 would buy roughly 3× more. That is the "measured quote" A-3 asks for. Per-harness caps (for example $6 in P1) stop a runaway harness, as OpenClaw's or Codex's token use could become, from consuming the pilot. P0 must precede P1 so the planning values can be replaced by measured ones. The $0 oracle re-proof (A-5) precedes both.

## Admission criteria the spec should adopt

Hard gates (**G**) must all pass, with an admission record, before a harness enters any paid stage. Reported attributes (**A**) are recorded and published but do not exclude.

| ID | Criterion | Type | Test / evidence |
|---|---|---|---|
| G1 | **Legal posture:** license or terms permit installing and running unattended in an automated evaluation and publishing results; proprietary terms are quoted in the record and the operator acknowledges them; branding limits recorded | G | Source quote + operator decision (F6, F7) |
| G2 | **Endpoint:** accepts a custom base URL and model id on a format the HH proxy serves (OpenAI chat or Anthropic messages; Responses only if the proxy implements it); no vendor-hosted inference or vendor login required | G | Adapter/config inspection + P0 call (F5) |
| G3 | **Headless:** single-instruction non-interactive run, permission prompts disabled inside the cell, a terminal exit state | G | P0 run (F5) |
| G4 | **Pinned, offline-installable:** exact version plus a lock or resolution cutoff, installed at image-build time; runs with no install-time network; version recorded in each receipt | G | Image build + `skip_install`-style run (F8) |
| G5 | **Egress containment:** runs in a cell whose only route is the proxy; telemetry, update, catalog and web tools disabled or fail-soft; denied connection attempts logged | G | Egress probe in P0 (F9) |
| G6 | **Mediation completeness:** every model call (including subagent, background, compaction and title calls) traverses the proxy and receives a receipt; no call to any other model or service | G | Proxy call count vs harness log; refusal of non-model routes (F9, F10) |
| G7 | **Configuration identity:** effort mechanism, sampling, role compatibility, output caps and any proxy rewriting are versioned and part of the participant identity; request-level conformance fixtures pass | G | Fixture battery (F11, F13) |
| G8 | **Smoke feasibility:** completes P0 tasks without protocol errors attributable to the endpoint (`tool_choice`, `cache_control`, developer role, stateless Responses) | G | P0 outcome classes (F4) |
| G9 | **Parity eligibility (only for O-vs-H comparisons):** a Harbor adapter at the pinned Harbor commit runs the same image | G (parity only) / A otherwise | S5/S6 adapter list |
| A1 | Wire format; native tool calling vs text parsing | A | F5, F12 |
| A2 | Measured cached-input share and $/trial cached vs uncached; caching dependence | A | proxy receipts + `/api/usage` (F3) |
| A3 | Native trajectory format (ATIF version or own) and fidelity | A | F10 |
| A4 | Install method, image size, runtime (Node/Python) | A | F8 |
| A5 | Known issues and the repairs applied | A | F13 |
| A6 | External priors (HF board score and $ for the pinned or nearby version), labelled as non-replication | A | F2 |
| A7 | Popularity and maintenance signals (stars, last release) | A | S7 |

## Answer and completion assessment

C3's question is answered. The admissible candidate set, the hard-gate exclusions (Gemini CLI, Antigravity and Cursor CLI on the endpoint gate; Roo and kimi-cli archived) and an 8-harness ranked shortlist are settled at survey level. The shortlist carries per-trial cost priors traceable to HF's published payload and Ollama's published prices. So are an admission-criteria definition split into hard gates and reported attributes, and a staged $60 pilot plan.

Two **technical blockers** remain, and each is dispositioned rather than open-ended. Codex needs a Responses decision on the proxy. Terminus-2 needs a hosting decision because its loop runs outside the cell. The rest is **acceptable uncertainty for this row**: Ollama's cached share, the per-harness egress inventory, and the Terminus-2 and OpenHands costs are all designed to be measured in P0. Claude Code's legal posture is not a technical blocker, but it needs an operator decision before Claude Code enters a paid stage. All per-harness behaviour on Ollama is unobserved: no call was made.

## Open questions

- **Q-C3-1 (operator decision; affects C4, D1, F2, H1):** Accept the Claude Code legal posture in F6? That means the published binary run unmodified, proxy-only credential, black-box participant (no HIR replication), plain-text name and no logo, and disclosure of Anthropic's "not supported" statement. Recommendation: accept, with the record.
- **Q-C3-2 (operator decision; affects C2, C4):** Should the HH proxy add an OpenAI Responses pass-through, which Ollama supports statelessly, so Codex can be admitted? Or should Codex stay deferred? Recommendation: defer Codex from the pilot. Revisit at the tranche gate.
- **Q-C3-3 (C1 owner; operator input):** Is the $60 a one-off purchased credit or Pro's monthly included credit, which does not roll over (F14)? What concurrency does the plan allow?
- **Q-C3-4 (D1 owner; operator input optional):** Version policy: pin HF's versions (better priors, older) or the latest stable at pre-registration (current, no priors)? Recommendation: latest stable at pre-registration, with HF versions as cost priors only.
- **Q-C3-5 (operator decision; affects D1, H1):** Pilot breadth: 8 harnesses × 10 tasks × 2 attempts (recommended), or 6 × 15 × 2?
- **Q-C3-6 (C4 owner):** Terminus-2 hosting under H: host-side loop against the proxy, or an in-cell package? This must be declared for parity.
- **Q-C3-7 (C1 owner):** Does Ollama ignore or reject `tool_choice` and `cache_control`? Is caching automatic on each endpoint?

## Spec requirements emitted

- **REQ-XC-201 — Harness admission record.** Each harness has an admission record (pinned version, install recipe and lock, image digest, wire format, configuration, proxy policy version, gates G1–G8 with evidence) before any paid stage. Rationale: F1, F8, F11. OL-09. Acceptance: record schema + validator rejecting missing gates.
- **REQ-XC-202 — Legal-posture gate.** Proprietary or source-available harnesses carry quoted terms and a recorded operator acknowledgement. Claude Code's record includes F6's five conditions. Rationale: F6, F7. OL-09. Acceptance: record review; check that the results site uses no third-party logos without recorded permission.
- **REQ-XC-203 — Endpoint gate.** Admission requires a working custom base URL on a format the proxy serves. Harnesses limited to Google formats or vendor-hosted inference are excluded with typed reason `n/a{endpoint}`. Rationale: F5. Acceptance: P0 call receipt.
- **REQ-XC-204 — Offline pinned install.** Harnesses install at image-build time with an exact version and a lock or resolution cutoff. Trials run with no install-time network, and O and H surfaces share the image. Rationale: F8. Acceptance: image rebuild reproduces the digest or lock; a trial in a no-egress cell succeeds at setup.
- **REQ-XC-205 — Egress probe.** Each admission includes an egress probe recording every non-model connection attempt and the disable switches used. Rationale: F9. Acceptance: probe log attached to the record.
- **REQ-XC-206 — Proxy route allowlist.** The proxy forwards only model-inference routes and refuses web-search, web-fetch and other non-model routes with a typed refusal recorded in receipts. Rationale: F9. Acceptance: negative test per route.
- **REQ-XC-207 — Mediation completeness.** Every model call, including subagent, background and compaction calls, has a proxy receipt. A trial whose harness log shows a call without a receipt is `failed`/infra, never scored. Rationale: F10. Acceptance: reconciliation test.
- **REQ-XC-208 — Proxy rewrite identity.** Every proxy transformation (effort injection, role mapping, field strip, format translation) is versioned and included in participant identity. Changed versions are new participants, and conformance fixtures pin each rewrite. Rationale: F11, F13. Acceptance: fixture battery; identity hash changes when a rewrite changes.
- **REQ-XC-209 — Unsupported-field policy.** The proxy records `tool_choice`, `cache_control` and stateful-Responses fields per call and applies a declared pass/strip/refuse policy. Rationale: F4. Acceptance: fixtures per field.
- **REQ-XC-210 — Cache-aware cost telemetry.** Receipts record cached and uncached input and output tokens. Budget reserves use no-cache planning values until P0 measures the cached share per endpoint. Rationale: F3. Acceptance: reserve computation test.
- **REQ-XC-211 — Staged pilot with per-harness caps.** The pilot runs P0 (smoke) before P1, with per-harness and total caps, and stops at a cap without overshoot beyond in-flight trials. Rationale: F2, F3, A-3. Acceptance: simulated over-cap run stops.
- **REQ-XC-212 — External priors labelled.** HF board figures may appear only as labelled external priors (version, provider, date) and never as replication targets or HH results. Rationale: F1, F2, Q-X12. Acceptance: claims-language check.
- **REQ-XC-213 — Effort pinning.** Each participant's reasoning effort is set through its own mechanism and enforced or recorded at the proxy. Mismatches are recorded per call. Rationale: F13. Acceptance: per-call effort field in receipts.
- **REQ-XC-214 — Parity eligibility.** O-vs-H parity is attempted only for harnesses with a Harbor adapter at the pinned Harbor commit, running the same image. Rationale: F5, F8. Acceptance: admission gate G9.

## Handoff

- **Dependencies resolved:** none were required (wave 1). This note supplies C4 (hosting), D1 (method and pilot design) and H1 (budget envelopes).
- **For C1:** Verify Ollama's cached share per endpoint, the treatment of `tool_choice` and `cache_control`, `reasoning_effort` values for `glm-5.3-flash` via `/api/show`, plan concurrency, and credit semantics (F3, F4, F14).
- **For C2/C4:** Proxy formats (Responses or not), the route allowlist, rewrite versioning, Terminus-2 hosting, offline images, and egress probes (F4, F8, F9, F11, F12).
- **For D1:** Per-harness cost priors and variance (F2). The pilot cannot rank harnesses. Seeded selection should avoid low-information tasks. A cheating scan is needed where tasks get egress (F9).
- **For E1/E2:** Claude Code is excluded as a replication target (F6). mini-swe-agent's mode and Terminus-2's parser and summarization settings are configuration to pin (F12).
- **For F2:** No third-party logos without recorded permission. The trajectory viewer must render from proxy receipts (F6, F10).
- **Conflicts:** none known with other notes. The brief's proxy shape (OpenAI chat + Anthropic messages) excludes Codex as stated, which is flagged in Q-C3-2.
- **Portfolio disposition changes:** none.
