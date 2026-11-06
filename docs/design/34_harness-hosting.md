# 34_harness-hosting — Harness hosting in HH cells (adapter C)
Stream: C
Researched: 2026-11-06 (inspection 2026-11-06T23:34Z–23:44Z)
Author: design worker for ledger row C4 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: C4
Repository baseline: main 09fe8523. Inspected at worktree HEAD 58f7a8b7 on `claude/experiments-spec`; `git diff --stat 09fe8523 HEAD -- crates spec scripts plugins docs/adr` is empty, so every code locator below holds at 09fe8523. `bash scripts/deferrals-open.sh` printed nothing and exited 0 (zero open rows) at 2026-11-06T23:38Z.
Confidence overall: medium. The Harbor and HH facts are high-confidence (source read at pinned commits, locators cited). Everything this note proposes is a design proposal, UNVERIFIED by construction: nothing was installed, built or run, and no podman, `harbor`, harness or network command was executed. The three riskiest assumptions are flagged where they occur: the Harbor pin needed for shared images (F3), the Terminus-2 host driver over Harbor internals (F10), and per-harness fail-soft behaviour under denied egress (F7, F15).

**Answer in brief.** A pinned harness enters an HH cell as a **prebuilt layer**, never at run time. The layer is built in two stages at admission time. First, a content-addressed **harness bundle** is built once per (harness, version, lock). Second, a thin **per-(harness × task image) layer** is built `FROM` the task image digest. That second stage replays the system-dependency step of Harbor's install recipe and places the bundle at the paths Harbor's adapter expects. Building needs egress; running does not. Each harness is pointed at the cell's model relay (`127.0.0.1:P_model`) through its own native mechanism, using the per-slot proxy token, the model `glm-5.3-flash` and the protocol's effort level (§1.5 matrix). Telemetry, auto-update and web tools are switched off where a switch exists. Every remaining attempt is denied and logged as a residual, and the harness must survive the denial.

Terminus-2 stays **host-side**, as in Harbor. Harbor's own `Terminus2` class runs in a host process. Its five environment calls are bridged to HH-dispatched effects in the cell. Its model calls go through a host-side relay to the same proxy. Its cell gets no model socket.

Proxy receipts are the authoritative record of model I/O. Native trajectories are collected as artifacts and converted to ATIF post hoc with Harbor's own converters.

Two checks run before any paid trial:
- a **$0 replay smoke** per harness (scripted responses through C2's replay mode);
- the **$0 half of the H-vs-O runner check**, which compares configuration and canonical request sequences under replay.

The paid half of the runner check is a small, descriptive verdict comparison. It is a fidelity smoke, not parity.

One blocking discrepancy surfaced. Harbor's `skip_install`, which lets the O surface run the same prebuilt image, is **absent from the stable tag v0.25.0** that B2 and D1 pin. It first appears one day later, in an untagged commit (F3).

## Scope, inputs and method

Row C4 asks how adapter C becomes real. That covers installing a pinned harness into the cell, configuring it through the model proxy, capturing trajectories, and the per-harness runner check, plus the admission smoke, failure modes and ticket seams.

**Program inputs** (read in full): the brief (§3 A-4, A-7); ledger row C4 and its dependencies; §Q (answered Q-X5, Q-X7, Q-X26, Q-X30, Q-X31, Q-X32; pending Q-X22, Q-X27, Q-X29, Q-X39); the Execution contract and standing limits; `docs/research/CONVENTIONS.md`; and the OL-02/09/15 rows of the outline trace.

**Binding notes** (read in full):
- `research/27` (C3)
- `design/33` (C2)
- `design/31` (B2)
- `research/25` (B1)
- `research/28` (E1)

**Notes read in part:** `design/35` (D1, in progress) F13–F14 and §Q lines; `design/36` (D2) recommendations R1–R6.

Root `AGENTS.md` and `crates/AGENTS.md` were read. No crate has a nested `AGENTS.md`.

| ID | Source and locator | Retrieved (UTC) |
|---|---|---|
| H1 | Harbor `v0.25.0` = `fdba28fc411e47e8765420a1a063b4918fce3def` (2026-10-25). Shallow clone at the tag into `…/scratchpad/C4/`. `LICENSE` and `pyproject.toml` (`license = "Apache-2.0"`, `version = "0.25.0"`) read; adapter index listed. The clone was then deleted to relieve a full disk (Handoff). All further reads use `git show fdba28fc:<path>` in E1's full clone at `…/scratchpad/E1/harbor`. | 2026-11-06T23:34:09Z–23:43Z |
| H2 | At H1: `src/harbor/agents/installed/{base,claude_code,pi,hermes,openhands,kimi_code,opencode,mini_swe_agent,node_install}.py` (install, run and env sections); `agents/terminus_2/{terminus_2,tmux_session}.py` (environment calls, install of tmux/asciinema, LLM construction); `environments/base.py` (`exec` signature, `mounts`); `environments/docker/docker.py` (`_compose_exec`, mounts); `cli/jobs.py` (`--mounts-json`, `--install-only`); `models/trial/config.py:236`; `uv.lock` (`litellm`) | 23:35Z–23:43Z |
| H3 | Harbor HEAD `ff2a1ef48070a8df374669138e4d4db79b77f013` (E1 clone, full history): `git log -S skip_install`, `git tag --contains`, `installed/base.py:1145-1200`, `docs-mintlify/agents/pre-integrated-agents.mdx:154-172`; `git diff --stat fdba28fc ff2a1ef4` on the shortlisted adapters | 23:37Z–23:41Z |
| L1 | Local LiteLLM 1.108.1 (bundled with tb 0.2.18, `~/.local/share/uv/tools/terminal-bench/…/litellm`): `litellm_core_utils/get_model_cost_map.py:1-12,655-700`; `__init__.py:428-434`. This is **not** the version any harness would install (E1 F7). | 23:40Z |
| R1 | HH at 09fe8523: `crates/hh-hosting/src/adapter_c.rs:1-80,355-396`, `records.rs:30-140`; `crates/hh-env/src/driver.rs:3600-3633` (`env_download`); `crates/hh-containment/src/policy.rs:791-826` (`ResidualKind`); `crates/hh-helper/src/protocol.rs` verb index; grep for `podman build` in `hh-helper`/`hh-env` (none) | 23:38Z–23:42Z |

## 1. Design

### 1.1 The hosted participant is a composite: C for the process, B for model I/O, the runner for artifacts

```text
participant (phase 2, per harness)
  hosting_mechanism   = container_installed                         (adapter C, adapter_c.rs)
  HostingExt          = { model_io_intercept: base_url,
                          process_placement:  in_environment        (installed harnesses)
                                            | lab_host              (Terminus-2, §1.6) }
  observability       = { end_state  ← runner (seal, exit, verdict),
                          model_io   ← proxy channel only (C2 F1; receipts on the phase proxy run) }
  native logs         → lifecycle.hosted.native_record{raw_ref}, mediation: unobserved (adapter C honesty rule)
  admission_ref       → hh-harness-admission/1 (§1.3)
```

Adapter C alone can claim only `end_state`, and its lifted rows are `mediation: unobserved` (`adapter_c.rs:9-19`). The composite therefore earns `model_io` from the proxy alone. Native-log lifts never upgrade it.

### 1.2 Installation pipeline (admission time, then offline runs)

```text
 ADMISSION (phase VM, egress allowed, recorded; once per harness × version × lock)
  (a) bundle stage      builder container FROM a declared builder base @digest
                        └ HH recipe = Harbor's install() for that adapter at the pinned Harbor commit,
                          with versions pinned + lock/resolution cutoff (npm lockfile / uv --exclude-newer),
                          runtime included (nvm Node 22.x resolved → exact; uv-managed Python exact)
                        └ output: /opt/hh-harness/<name>-<ver>/ tarball, sha256 = bundle_digest,
                          resolved-manifest (every package@version), build log blob
  (b) layer stage       FROM docker.io/alexgshaw/<task>@sha256:<task digest>   (per task image)
                        └ RUN the adapter's ensure_system_dependencies set via the image's package
                          manager (apt/apk), recorded as a package delta manifest (dpkg/apk before/after)
                        └ COPY bundle → Harbor-expected paths (e.g. ~/.local/bin/claude, ~/.nvm,
                          /opt/openhands-venv, uv tool dir), version marker file
                        └ output: layer image @sha256 = harness_layer_digest  (ForeignDigest, R1 label)
 RUN (every slot; cell on --network none / allowlist_mediated; B2 §1.2)
  S1 provision   image = harness_layer_digest (pulled/loaded by digest only)
  S2 setup       VERIFY-ONLY: version probe == admitted version; bundle tree hash == bundle_digest;
                 config files rendered (§1.5) and hashed — no network, no install
  S3 agent       launch the rendered command; deadline = task-declared × multiplier (B2 F13)
  S4 seal        post-agent version probe + install-tree hash (self-modification check) + /logs/agent copy-out
```

Alternatives, compared under F4:

| Option | Fidelity to Harbor's installed state | Network at run | Per-task cost | O/H same bytes | Verdict |
|---|---|---|---|---|---|
| Runtime install (Harbor's behaviour) | exact | **required at setup** | none up front; install time per trial | no (resolved per trial) | rejected: breaks Q-X30's offline agent cell and pin reproducibility (C3 F8) |
| Bind-mounted bundle only | **misses system deps** that Harbor installs into the task container (`curl`, `git`, `nodejs`, `tmux`, build tools…) | none | none | only if Harbor mounts identically | fallback only, if disk forbids layers (OQ-4) |
| **Two-stage layered image** | exact (system-dep step replayed), plus pins | none | one thin layer per task image | yes, with `skip_install` (F3) | **recommended** |

### 1.3 Harness admission record `hh-harness-admission/1` (refines REQ-XC-201)

```text
hh-harness-admission/1 {
  harness:   { name, role: reference|comparator, harbor_agent: "<-a name>" | null,
               version, version_source: release_tag|pypi|npm (Q-X27 policy applied at freeze),
               freeze_at (UTC), legal{license_spdx|terms_quote_ref, operator_ack_ref?, posture{…}} },
  install:   { recipe_ref (HH-owned, derived from harbor@<commit>:installed/<a>.py install(), attribution),
               deviations[{what, why}]  (e.g. hermes: PyPI instead of install.sh@main),
               bundle{digest, builder_base@digest, resolved_manifest_ref, build_log_ref, built_at},
               system_deps{set (from ensure_system_dependencies), per_image_delta_ref},
               layers{task_image_digest → harness_layer_digest}, install_tree_paths[] },
  runtime:   { node? {version}, python? {version}, litellm? {version} },        // REQ-XE-003
  config:    { renderer_id, renderer_version, template_hash (argv/env/files with token elided),
               effort{mechanism, value_ref: protocol pin (Q-X22)}, limits_ref (Q-X29),
               disable_switches[{switch, purpose}], instruction_passing: argv|env|stdin,
               model_selector, surface: openai.chat|anthropic.messages, base_url_suffix },
  egress:    { expected_denied[{host_or_pattern, purpose}], must_reach: [model relay only] },
  client:    { model_timeout_s?, retry_policy?, streaming: bool, tolerates{heartbeat, 402, 429}? },
  completion:{ detector: process_exit, native_status_parser? },
  artifacts: { native_paths[], atif: converter@harbor-commit | none },
  gates:     { G1…G9 → {result, evidence_ref} }  (C3 table; G8/G5/G6 evidence = §1.9 smoke),
  posture:   { placement: in_environment|lab_host, residuals[] } }
```

Q-X27 is pending, and this record is its **interface**. The policy (latest stable at freeze, per D1's recommendation, or another) only selects `version` and `freeze_at`. The record pins the exact package, version and lock, plus the bundle and layer digests, whatever policy is chosen.

### 1.4 Cell contract for an installed harness

- **Relay pairs** (B2 F18 `hh-cell-relay`): `model` = `127.0.0.1:P_model ⇄ /run/hh/model.sock`; `egress` = `127.0.0.1:3128 ⇄ /run/hh/egress.sock`. `P_model` is a declared constant of the cell spec.
- **Environment** (closed set; everything else stripped):
  - the harness's base-URL and key variables (§1.5), with the slot token as the only credential;
  - B2's `http(s)_proxy` hints, plus `no_proxy=127.0.0.1,localhost` (**mandatory**, F8);
  - the disable switches;
  - `HOME` and the user as the image declares them.
- **Paths:**
  - `/logs/agent/` exists and is writable by the agent user, because Harbor adapters write there;
  - `/installed-agent` exists (Harbor `setup()` creates it);
  - `/tests` and `/solution` are absent (B2 REQ-XB-106).
- **Instruction:** the verbatim bytes of `instruction.md`, passed the way the Harbor adapter passes them (§1.5).
- **Launch:** one helper `exec` of the rendered command as a process group. The task deadline applies from outside (B2 F13). On lapse the group is terminated and the slot proceeds to verification.

### 1.5 Per-harness configuration matrix (design proposal over Harbor v0.25.0 adapter source)

All rows use the model `glm-5.3-flash`, credential = slot token, and effort = the protocol pin (Q-X22; D1 and D2 lean `high`). The harness's native effort mechanism is set to the same value, and the proxy pins it again (C2 F7). Paths are base-URL values as each client expects them. That they arrive at `/v1/chat/completions` or `/v1/messages` is an inference (no wire capture) that the replay smoke must confirm. Every other path is a `refused_route` receipt.

| Harness (Harbor `-a`) | Placement / runtime | Surface | Pointing at the proxy (H2 locators) | Effort mechanism | Disable switches (known) | Headless invocation and instruction passing | Native artifacts |
|---|---|---|---|---|---|---|---|
| mini-swe-agent 2.4.6 | in-cell, uv Python 3.12, LiteLLM (resolved at freeze) | chat, non-streaming | `-c model.model_kwargs.api_base=http://127.0.0.1:P/v1`, model `openai/glm-5.3-flash`, key via `MINI_SWE_AGENT_MODEL_API_KEY_JSON` expanded into `-c model.model_kwargs.api_key=…` (`mini_swe_agent.py` run, l.~986-998) | `-c model.model_kwargs.reasoning_effort=<pin>` | `LITELLM_LOCAL_MODEL_COST_MAP=True` (F7); `MSWEA_CONFIGURED=true`, `MSWEA_COST_TRACKING=ignore_errors` (Harbor, l.882-883) | `mini-swe-agent --yolo --model=… --task=<shlex-quoted> --output=/logs/agent/… --cost-limit 0 -c mini -c model.model_kwargs.timeout=3600 … --exit-immediately` (E1 A.1) | `mini-swe-agent.trajectory.json`, `.txt` |
| Terminus-2 (Harbor 2.0.0) | **lab_host**, host Harbor venv (LiteLLM from `uv.lock`, F11); tmux in cell | chat, non-streaming | `api_base=http://127.0.0.1:P_host/v1` → host relay → slot socket (§1.6); model `openai/glm-5.3-flash` | `reasoning_effort=<pin>` kwarg | `LITELLM_LOCAL_MODEL_COST_MAP=True`; `model_info` declared (D2 R1) | HH host driver calls `Terminus2.setup/run(instruction, env)`; instruction as Python string | ATIF (host), `terminus_2.pane`, asciinema cast |
| Claude Code (pinned) | in-cell, native binary (bootstrap) or npm | messages, streaming | `ANTHROPIC_BASE_URL=http://127.0.0.1:P` (client appends `/v1/messages`; inferred from Ollama's `https://ollama.com` recipe, C3 F5), `ANTHROPIC_AUTH_TOKEN=<token>`, `ANTHROPIC_API_KEY=""`, `ANTHROPIC_MODEL` **and** `ANTHROPIC_DEFAULT_{SONNET,OPUS,HAIKU,FABLE}_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL` = `glm-5.3-flash` (`claude_code.py` `_resolve_auth_env`, `_model_alias_env(all_aliases=True)`) | `--effort <pin>` | `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`; `--disallowedTools WebSearch,WebFetch` (`disable_web_search`); `IS_SANDBOX=1`; `--permission-mode=bypassPermissions` | `printf "%s" "$VAR" \| claude --verbose --output-format=stream-json --print` (instruction via env var → stdin); `CLAUDE_CONFIG_DIR=/logs/agent/sessions` | session JSONL, `claude-code.txt` |
| Pi | in-cell, Node via nvm | chat | `models.json` provider `harbor-endpoint`: `baseUrl …/v1`, `api: "openai-completions"`, `apiKey: "$OPENAI_API_KEY"`; `PI_CODING_AGENT_DIR=/tmp/harbor-pi-agent` (`pi.py` `_build_custom_models_json`) | `--thinking <pin>` (sets `reasoning: true`) | none in adapter; `compat.supportsDeveloperRole=false` is an HF repair (C3 F13) to evaluate in smoke, recorded if used | `pi --print --mode json --session-dir … --provider harbor-endpoint --model glm-5.3-flash <shlex-quoted instruction>`, **no `--` before the instruction** at v0.25.0 (F13) | `pi/sessions/*.jsonl`, `pi.txt` |
| Hermes Agent | in-cell, Python (installer) | chat | `/tmp/hermes/config.yaml` `custom_providers: [{name: runtime, base_url …/v1, key_env: OPENAI_API_KEY, api_mode: chat_completions}]`, `provider: custom:runtime`; memory, checkpoints off (`hermes.py` `_build_config_yaml`) | none in adapter: proxy pin only (recorded `requested: null`) | toolsets `hermes-cli` (web/browser content not inventoried: smoke) | `hermes --yolo chat -q "$HERMES_INSTRUCTION" -Q --model … --provider custom:runtime` | `hermes.txt`, sessions export |
| OpenHands | in-cell, uv Python 3.13 venv `/opt/openhands-venv`, LiteLLM | chat | `LLM_MODEL=openai/glm-5.3-flash`, `LLM_BASE_URL=…/v1`, `LLM_API_KEY=<token>` (`openhands.py` l.136-168, 981-993) | `LLM_REASONING_EFFORT=<pin>` | `AGENT_ENABLE_BROWSING=false`, `ENABLE_BROWSER=false`, `AGENT_ENABLE_PROMPT_EXTENSIONS=false`, `LITELLM_LOCAL_MODEL_COST_MAP=True`; telemetry not inventoried | `SANDBOX_VOLUMES=${PWD}:/workspace:rw … python -m openhands.core.main --task=<quoted>`; `RUNTIME=local` | `openhands.trajectory.json`, `completions/`, `.txt` |
| Kimi Code | in-cell, Node via nvm, `~/.local` prefix | chat | `KIMI_MODEL_BASE_URL=…/v1`, `KIMI_MODEL_API_KEY=<token>`, `KIMI_MODEL_NAME=glm-5.3-flash`, `KIMI_MODEL_PROVIDER_TYPE=openai` (`kimi_code.py` l.115-205, 279-296) | `KIMI_MODEL_THINKING_EFFORT=<pin>` + `KIMI_MODEL_CAPABILITIES=thinking` (adapter docstring l.66-71; HF: no effort sent otherwise, C3 F13) | `KIMI_DISABLE_TELEMETRY=true`, `KIMI_CODE_NO_AUTO_UPDATE=true`, `NO_COLOR` | `kimi --prompt "$VAR" --output-format stream-json` (env var) | `KIMI_CODE_HOME` logs → Harbor converter |
| OpenCode | in-cell, Node via nvm | chat | `~/.config/opencode/opencode.json` provider `npm: "@ai-sdk/openai-compatible"`, `options.baseURL …/v1`, `apiKey: "{env:…}"` (`opencode.py` l.230-262) | `--variant <pin>` (v0.25.0 option name; renamed `reasoning_effort` at HEAD) | `permission: {websearch: deny, webfetch: deny}` (`disable_web_search`, l.847-856); `OPENCODE_FAKE_VCS=git`; catalog/update hosts not inventoried | `opencode --model=<p>/glm-5.3-flash run --format=json --thinking --dangerously-skip-permissions -- <quoted>` | xdg data/state, output JSONL |

Alternates (Qwen Code, Goose) follow the same row shape. Goose must use an OpenAI-chat provider, not Ollama-native (C3 F5).

### 1.6 Terminus-2: host-side driver with an HH exec bridge

```text
 phase VM host
 ┌ slot worker (hh-runner, Rust) ──────────────────────────────────────────────────────────┐
 │  EnvDriver/hh-helper ─► podman cell (allowlist_mediated, NO model socket; tmux in layer) │
 │  exec bridge: /run/hh/<slot>/bridge.sock  (JSON lines; ops: exec{command,cwd,env,       │
 │      timeout_sec,user} · upload_file · download_file · is_dir) → each op = a dispatched  │
 │      effect in the agent phase (receipted; same helper exec path as installed harnesses) │
 │  host relay: hh-cell-relay --pair 127.0.0.1:P_host=/run/hh-proxy/<phase>/<slot>.sock     │
 └──────────────────────────────────────────────────────────────────────────────────────────┘
 ┌ terminus host process (pinned Harbor venv, `uv sync --frozen`) ─────────────────────────┐
 │  hh_terminus_driver.py (HH-owned glue):                                                 │
 │    env = HHBridgeEnvironment(BaseEnvironment)  # exec/upload_file/download_file/is_dir   │
 │    agent = Terminus2(**admitted kwargs verbatim, api_base=http://127.0.0.1:P_host/v1)    │
 │    await agent.setup(env); await agent.run(instruction, env, context)                    │
 │    ATIF + context → /hh/slot/<id>/agent/ (collected)                                      │
 └──────────────────────────────────────────────────────────────────────────────────────────┘
```

Terminus-2 touches its environment only through `exec` (25 call sites), `upload_file`, `download_file`, `is_dir`, `default_user`, `session_id` and `trial_paths` (H2 grep of `terminus_2.py` and `tmux_session.py`). Harbor's podman `exec` is `compose exec [-w cwd] [-e K=V]… [-u user] main bash -c <command>` (`docker.py` `_compose_exec`). The bridge reproduces exactly those arguments.

### 1.7 Trajectory and integrity capture

| Record | Source | Authority | Use |
|---|---|---|---|
| Model I/O | proxy receipts (`hh.model_io/1` + `proxy`, C2 §1.3) | **authoritative** | cost, calls, actions (tool calls in responses), observations (tool results in the next request), leak scan input (what the model received) |
| Agent actions in the cell | Terminus-2: bridge effect rows (every keystroke batch, pane capture). Installed harnesses: not observable per command; inferred from receipts | authoritative for Terminus-2; derived otherwise | integrity scan |
| Egress | B2 mediator `security.egress.requested/decided` rows (slot-tagged) | authoritative | denied-attempt residuals, answer-fetch detection |
| End state | B2 S4 seal (podman diff manifest, `/work` pin, process list) + post-agent version probe and install-tree hash | authoritative | self-modification and tamper evidence |
| Native trajectory | `/logs/agent/*` copied out at S4 (hashed) | reported (A3) | per-step tool attribution, site viewer |
| ATIF | Harbor's converter at the pinned commit, run post hoc on the host over the collected native logs (both surfaces) | reported | L3 trace shape, site |
| Terminal transcript | Terminus-2 `terminus_2.pane` + asciinema cast | reported | scan, viewer |

### 1.8 Per-harness runner check (H vs O)

| Half | Cost | O surface | H surface | Compared | Pass rule |
|---|---|---|---|---|---|
| **R-replay** | $0 inference | `harbor run -a <agent> --ak skip_install=true` on the smoke task, model through the proxy in `replay` with the harness's scripted recording | HH cell, same layer digest, same recording | (1) config equality: argv with secrets elided, env-name set and non-secret values, config-file sha256s, image digest, detected version; (2) canonical equality of the **downstream** request sequence (`json_canonical` + declared masks: ids, timestamps, hostnames) | all equal, or every difference listed in a declared-divergence register |
| **R-live** | paid (inside D1's P0 gate) | P0 tasks × 1 attempt per G9-eligible harness, **one Harbor job per O slot** (per-slot token and budget slice), model through the same proxy via a host relay on the podman bridge gateway | the paired P0 H trials | verdict agreement table, end-cause, calls/tokens/wall per trial, failure classes, turn-1 request equality | descriptive only. Gross differences (a surface fails to run, a systematic request delta) block the harness |

**Label:** "per-harness runner check — fidelity smoke, `parity: none`". Two runs of a stochastic participant share no randomness, so verdict agreement cannot establish parity. Phase 3's L2 (D2) is the parity method, and R-live data are never L2 input.

**Size:** R-replay covers 8 harnesses × 1 task × 2 surfaces, 16 runs at $0. R-live defaults to 3 tasks × 1 attempt × 8 harnesses on O, 24 trials. That is roughly the same as C3's P0 line ($3.3 expected no-cache, $4.9 planning), and D1 may cut it to 1 task per harness (OQ-2).

### 1.9 Admission smoke battery ($0, C2 replay mode)

- **Smoke task.** HH-authored `hh-smoke/write-file`: a tiny image layer per harness family. The instruction is "write `ok` to `/app/hh_smoke.txt`". `test.sh` writes reward 1 iff the file holds `ok`. It is never a TB task.
- **Recording.** One scripted recording per harness, authored by hand in the raw-exchange format, in its own tool schema and streaming mode:
  - response 1: the harness's file-writing tool call (`bash`, `Bash`, `execute_bash`, …);
  - response 2: the harness's completion form. That is a final text with no tool call, mini-swe-agent's `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`, or Terminus-2's `task_complete: true` twice.
- **Replay settings.** `keying: sequence`, `on_divergence: serve_recorded`, `latency: immediate`.

| Variant | Injected | Asserted (deterministic, from records) |
|---|---|---|
| S-pos | none | S2 version and bundle hash equal; config hash equal to admission; receipts = script length; all on the model surface; zero `refused_route` other than declared (e.g. `count_tokens`); mediator rows ⊆ `expected_denied`; zero model-host CONNECTs; harness `exited(0)` before deadline; reward 1; native artifacts present; `leak_scan` clean; post-agent version and install-tree hash unchanged |
| S-429 | first call answered with a scripted 429 + `Retry-After` | the harness retries (each retry a receipt) or exits; the behaviour is recorded in `client.tolerates` |
| S-402 | call 2 answered as a slot budget refusal (C2 F16) | the harness exits within a bound with no hang; `end_cause: budget_refused`; the slot verifies |
| S-slow | `latency: recorded` with a long delay under `buffered_heartbeat` (streaming harnesses) or `buffered` | no client timeout below the declared bound; the `client.model_timeout_s` evidence is recorded |
| S-hang | response 1 = tool call `sleep 100000`, task deadline 60 s | `end_cause: agent_timeout`, process group killed, slot proceeds to verify |

The smoke needs a Linux podman host with socket bind-mounts (C2 F5 expects macOS `podman machine` to fail). It therefore runs on the phase VM in the same session as B2's oracle re-proof (B2 ticket 8), at $0 inference.

### 1.10 Hosting failure taxonomy (extends B2 F16 and C2 F16)

| Event | Detected by | Disposition | Retried? |
|---|---|---|---|
| Bundle or layer build fails at admission | build log, exit | harness not admitted: `n/a{install:<step>}` on the admission record | fixed recipe = new admission |
| S2 verify fails: layer digest or bundle hash or version mismatch | S2 probe | `withheld{admission:harness_layer_mismatch}`; arm halted (systematic) | no |
| S2 probe crashes, cell healthy | S2 exit | `inconclusive{infra:harness_setup:<c>}` | engine replan |
| Harness exits ≠0 and its last model call ended `upstream_error`, `upstream_timeout`, `stream_broken`, `proxy_aborted` or `refused_auth` | proxy dispositions + exit | `inconclusive{infra:model_route:<disposition>}` (Harbor: `model_provider_failed`, invalid) | engine replan |
| Harness exits ≠0 otherwise (crash, protocol error, format-error cap) | exit + receipts | slot **verifies**: `executed{…}` with `end_cause: exited(rc)` (harness-attributable) | no |
| Hang past deadline | helper deadline | `end_cause: agent_timeout` → verify → scored (Harbor: valid) | no |
| Slot budget refusal | 402 receipt | `end_cause: budget_refused` → verify → scored; D1 decides missingness | no |
| Upstream 429/5xx that the harness survives | receipts | scored; infra events recorded as covariates | n/a |
| Malformed model output (bad JSON or tool args) | harness behaviour | task-level: scored | no |
| Proxy-synthesized malformed bytes (replay divergence) | 409 | smoke or conformance finding only; never in paid runs | — |
| Denied egress on `expected_denied` | mediator row | residual only | — |
| Denied egress **not** in `expected_denied` | mediator row | flag `undeclared_egress_denied`; scored; admission re-review | — |
| Relay or cell death | helper | `inconclusive{infra:environment_died}` (B2) | engine replan |
| Self-modification (version or tree changed) | S4 probe | flag `harness_self_modified`; scored; arm review | — |
| Client abandons a call while queued or buffered | relay close before release | receipt `released: client_gone`, spend charged; covariate | — |

## 2. Findings

### F1 — Adapter C is end-state-only by construction, so the phase-2 participant must be declared as a composite with a placement per harness
Claim: `hharness-hosting`'s adapter C fixes `observability = {end_state}`, marks every lifted row `mediation: unobserved`, and supports only `ProcessPlacement::InEnvironment`. The placement vocabulary already has `LabHost`. A phase-2 participant is therefore adapter C for the process plus the proxy channel (adapter B) for `model_io`. The placement is `in_environment` for installed harnesses and `lab_host` for Terminus-2.
Status: VERIFIED (source); composite declaration UNVERIFIED (design proposal)
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-hosting/src/adapter_c.rs:9-19` (honesty rules), `:361-396` (`adapter_c_record`: `placement_supported = {InEnvironment}`, `coordinate_model: unsupported`); `records.rs:30-55` (`ProcessPlacement::{InEnvironment, LabHost, RemoteService}`), `:61-73` (`ModelIoIntercept::BaseUrl`), `:107-138` (`HostingExt`). C2 F1 (the composite; spend only from proxy rows).
Retrieved: 2026-11-06T23:39Z
Implication for the spec: Adapter C's record needs `LabHost` in `placement_supported`. An adapter-C variant could instead declare it, but the extension is the smaller change. Without it, Terminus-2 cannot be declared honestly. Native-log lifts never upgrade `model_io`.
Outline delta: EXTENDS OL-15 — participant declaration scaffolding per placement.
Limitations: Whether any validator ties `container_installed` to `in_environment` beyond `placement_supported` was not searched.

### F2 — Harbor's install templates are Apache-2.0 and run at trial setup with network; they mutate the task container and resolve runtimes unpinned
Claim: At v0.25.0, every shortlisted installed adapter installs during `setup()`. Each first runs `ensure_system_dependencies`, which installs packages into the task image via apt/dnf/yum/apk, and then fetches its runtime and package from the network. Several steps are unpinned:
- Node uses `nvm install 22` (the latest 22.x);
- nvm's installer comes from `raw.githubusercontent.com`;
- uv comes from astral's latest installer;
- Hermes's installer comes from `main`;
- npm defaults to `@latest`.

Claude Code's `install()` returns early if the requested version is already installed. The other adapters do not check.
Status: VERIFIED
Evidence kind: source inspection
Evidence: H1 (`LICENSE`: Apache License 2.0; `pyproject.toml:6-7`; no root `NOTICE` file in `git ls-tree fdba28fc`). H2: `installed/base.py` (`setup` → `install`, l.1117-1160 at v0.25.0); `node_install.py` (`NVM_VERSION = "v0.40.2"`, `nvm install {22}`); `pi.py` install (`ensure_system_dependencies(("curl",))`, `@latest` default); `hermes.py:287-305` (`curl … hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup --branch <v>`, deps `curl git ripgrep xz`); `openhands.py:895-923` (deps `curl git build_tools tmux`, astral installer, Python 3.13 venv, `openhands-ai==<v>`); `kimi_code.py:234-276`; `opencode.py:410-431` (deps `curl bash coreutils nodejs npm`); `claude_code.py:620-660` (`_installed_claude_satisfies_version`, deps `curl bash nodejs npm procps`, bootstrap.sh). E1 A.1 for mini-swe-agent (deps, uv Python 3.12, unpinned `litellm[mcp]`).
Retrieved: 2026-11-06T23:35Z–23:41Z
Implication for the spec:
- Harbor's install is part of the participant's **environment**. The agent can use the `git`, `curl`, `tmux`, Node and build tools that Harbor installed, so an H cell that omits them changes the task, not only the harness.
- HH recipes are HH-owned and derived line by line from the pinned Harbor commit's `install()`. Each recipe cites its source path and carries the Apache-2.0 attribution, and lists every substitution as a deviation: exact versions, lock or cutoff, a PyPI route for Hermes as HF used (C3 F8).
Outline delta: EXTENDS OL-02 (hosting fidelity), OL-09 (G4 evidence).
Limitations: The install commands were read, not run. Whether `ensure_system_dependencies` resolves to the same package versions on rebuild depends on distro mirrors. Snapshot mirrors are not designed here.

### F3 — `skip_install` is not in Harbor's stable v0.25.0; it landed one day later and no tag contains it
Claim: The O surface can run HH's prebuilt layer unchanged only through Harbor's `skip_install` agent option, which skips `install()` but still runs `configure()`. That option was added in commit `0a576fa5` (2026-10-26, "reuse an agent the environment already ships") and is present at HEAD `ff2a1ef4`. No tag contains it: the newest tag is `v0.25.0` (2026-10-25). B2 and D1 pin `v0.25.0`.
Status: VERIFIED
Evidence kind: source inspection
Evidence: H3: `git log -S skip_install -- src/harbor/agents/installed/base.py` → `0a576fa5 2026-10-26T11:41:01-07:00`; `git tag --contains 0a576fa5` → empty; `git tag --sort=-creatordate` → `v0.25.0` first. In my v0.25.0 clone, `grep -rn skip_install src/` matched nothing. At HEAD, `installed/base.py:1145-1200` documents that `install()` is "Skipped with `skip_install`"; `configure()` runs anyway; with `skip_install` "the pinned version is unverified, so record what the environment actually provides". `pre-integrated-agents.mdx:154-172`: "The image must use the paths and runtime expected by that integration". Adapter drift between v0.25.0 and HEAD, from the stat: `claude_code.py` 20 lines, `mini_swe_agent.py` 14, `opencode.py` 35, `tmux_session.py` 6. The `opencode` diff renames `variant` to `reasoning_effort`.
Retrieved: 2026-11-06T23:37Z–23:41Z
Implication for the spec: There are three ways forward.
- **(a)** Pin Harbor at an admitted commit that contains `skip_install`, e.g. `ff2a1ef4`. The tool admission must then record a non-release pin, and the E1/D2 parity references (built on v0.25.0 ≡ HEAD for Terminus-2, E1 F13) must be re-confirmed at that commit.
- **(b)** Wait for the first stable tag that contains it.
- **(c)** Keep v0.25.0. The O surface then installs at setup with network, so O and H harness bytes differ. Every runner check must be labelled "not image-identical", and C3's REQ-XC-204 ("O and H share the image") fails.

This note recommends (b) if a tag appears before the pre-registration freeze, else (a). It is an operator-visible pin decision (OQ-1).
Outline delta: CORRECTS OL-02's reliance on C3 F8 ("Harbor's `skip_install` lets a prebuilt image supply the agent"): that holds only for post-v0.25.0 Harbor.
Limitations: The next Harbor release date is unknown. The 78-line `installed/base.py` diff (E1) was read only in the `skip_install` region.

### F4 — Two-stage layered images give offline, pinned, faithful cells; build needs egress once, run never
Claim: The faithful offline option is a per-(harness × task image) layer. It replays the adapter's system-dependency step and copies a once-built harness bundle into Harbor's expected paths (§1.2). A pure runtime install violates Q-X30's offline agent cell. A pure bind-mounted bundle omits the system packages Harbor installs (F2).
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal over source inspection
Evidence: F2. B2 §1.3 setup row ("nothing for a prebuilt harness layer"). B2 F14 (`ForeignDigest` passes a resolved image verbatim; claim capped at R1). Harbor `--mounts-json` → `environment.mounts` (`cli/jobs.py:1005-1011,1863-1864`; `models/trial/config.py:236-238`) makes the bind-mount fallback expressible on O too. No `podman build` exists in `hh-helper` or `hh-env` (R1 grep), so the build is new runner machinery: a host act like the image pull, recorded with build log and digests.
Retrieved: 2026-11-06T23:38Z–23:42Z
Implication for the spec:
- **Bundle digests.** The bundle stage resolves all versions once, at freeze, into a lock (D2 R5 "resolve once, then freeze"). The bundle's content hash then fixes the harness bytes for every task.
- **Per-task layers.** The layer stage is short and needs only the distro mirror. Its package delta manifest records exactly what entered the task environment.
- **Disk.** Layers on different parents do not share storage, so 8 harnesses × ~30 images means 240 thin layers, each roughly the size of a bundle. B3 must size the disk, and the bind-mount fallback stays admitted for a disk-bound phase (OQ-4).
- **Reproducibility.** A per-task layer can be rebuilt from the bundle tarball plus the cached package files. The claim stays R1.
Outline delta: EXTENDS OL-02.
Limitations: Bundle relocatability holds only if the bundle is built at its final absolute path, because venv shebangs and nvm paths are absolute. That is how the recipe is defined, but it was not validated. Images that are Alpine/musl-based would need a musl bundle. All 89 TB 2.1 bases are glibc (B1 census `FROM`), so none do.

### F5 — The admission record is the version-pin interface; Q-X27 only chooses a value
Claim: Every hosting fact a slot needs fits in one `hh-harness-admission/1` record (§1.3). Any version policy picks only `version` and `freeze_at`; package, lock, bundle digest and layer digests are pinned regardless.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: C3 REQ-XC-201 and G1–G9/A1–A7; D1 F14 (three added fields, Q-X26 terms, latest-stable recommendation); D2 R4–R5 (Harbor defaults plus declared external caps; LiteLLM resolved once); E1 REQ-XE-003 (LiteLLM version in identity).
Retrieved: 2026-11-06T23:36Z
Implication for the spec: The validator refuses an admission in any of these cases:
- a missing gate;
- `@latest`, `stable`, a `main` branch or a nightly;
- a missing lock or cutoff;
- a missing bundle or layer digest;
- a missing config template hash;
- a missing `expected_denied` list;
- for a proprietary harness, a missing legal-posture record.

The participant identity hash covers the full record. A changed field is a new participant (C3 REQ-XC-208).
Outline delta: EXTENDS OL-09, OL-15.
Limitations: The schema home (hh-env record vs runner crate) is H1's call.

### F6 — Each shortlisted harness has a native, Harbor-tested way to reach a custom OpenAI-chat or Anthropic-messages endpoint
Claim: All eight shortlisted harnesses can be pointed at the cell relay through mechanisms Harbor already exercises. These are env variables, config files or CLI flags (§1.5). Seven use OpenAI chat; Claude Code uses Anthropic messages. Hermes is the only one with no adapter-level effort mechanism.
Status: VERIFIED (adapter mechanisms at v0.25.0); per-harness behaviour against the HH proxy UNVERIFIED
Evidence kind: source inspection
Evidence: H2 locators in the §1.5 matrix: `claude_code.py` `_resolve_auth_env` and `run` (l.2010-2233); `pi.py` `_build_custom_models_json`, `run`; `hermes.py:309-378,630-735`; `openhands.py:40-90,136-168,974-1075`; `kimi_code.py:66-71,115-205,279-370`; `opencode.py:100-115,230-262,805-926`; E1 A.1 and B.5 for mini-swe-agent and Terminus-2.
Retrieved: 2026-11-06T23:35Z–23:42Z
Implication for the spec:
- **Renderer.** Each harness gets one pure, golden-tested renderer, admission × slot → `{argv, env, files}`, mirroring the Harbor adapter's output. The slot record carries its hash, with secrets elided.
- **Effort.** The harness-native effort is set to the protocol pin so the client behaves as it would at that level. The proxy still pins the wire value (C2 F7), and requested and applied values are recorded per call.
- **Claude Code aliases.** Claude Code must get all tier-alias and subagent model variables, so background and subagent calls also go to `glm-5.3-flash` (G6).
Outline delta: EXTENDS OL-02, OL-09 (G2/G7 evidence).
Limitations: Base-URL path suffixes are inferred from adapter examples and Ollama recipes. Hermes's and OpenCode's effort semantics were not traced.

### F7 — Telemetry, update and web tools must be disabled where possible and fail-soft otherwise; LiteLLM's remote cost-map fetch is a hidden parity variable
Claim: Harbor's adapters set the known switches:
- Claude Code: `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`, plus WebSearch and WebFetch disallowed;
- Kimi Code: telemetry and auto-update off;
- OpenHands: browsing off;
- OpenCode: web tools denied.

LiteLLM 1.108.1 (as read) fetches its model cost and context map from `raw.githubusercontent.com` at import and falls back to its bundled map on failure, unless `LITELLM_LOCAL_MODEL_COST_MAP=True`. The map drives `get_model_info`, which feeds Terminus-2's context limit (E1 F10). An O surface with internet and an offline H cell can therefore run with different maps.
Status: VERIFIED (switches in adapter source; LiteLLM 1.108.1 behaviour); the installed LiteLLM versions' behaviour UNVERIFIED
Evidence kind: source inspection
Evidence: H2 `claude_code.py` (`env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"] = "1"`, `_disallow_web_tools`); `kimi_code.py:283-286`; `openhands.py:1020-1022`; `opencode.py:847-856`. L1 `get_model_cost_map.py:1-12` ("Pulls the cost + context window … This can be disabled by setting the LITELLM_LOCAL_MODEL_COST_MAP environment variable to True"), `:676` (env check), `:680-700` (fetch then local fallback); `__init__.py:430-433` (default URL). C3 F9 (web-search through Ollama; Q-X30 posture).
Retrieved: 2026-11-06T23:40Z
Implication for the spec:
- `LITELLM_LOCAL_MODEL_COST_MAP=True` is set on **both** surfaces for every LiteLLM-based harness (mini-swe-agent, OpenHands, the Terminus-2 host). It is recorded as a declared configuration.
- Each admission lists its `expected_denied` hosts. Under the agent-phase allowlist, those attempts are denied, logged and attributed to the slot as residuals.
- The admission smoke proves the harness completes with them denied. A harness that needs a non-model host to function fails G5. It is never granted the host.
Outline delta: EXTENDS OL-02, OL-15.
Limitations: The telemetry and update hosts of Pi, Hermes, OpenHands and OpenCode were not inventoried. The S-pos smoke's mediator rows are the inventory. LiteLLM versions other than 1.108.1 were not read.

### F8 — B2's `http_proxy` hints can capture model traffic unless `no_proxy` covers the relay
Claim: B2's cell sets `http_proxy`/`https_proxy` = `127.0.0.1:3128` as routing hints. HTTP clients that honour them would send model calls to the egress mediator, where they are denied, unless `no_proxy` includes `127.0.0.1` and `localhost`. B2 sets that `no_proxy`. The hosting contract must make it mandatory and verify it per harness.
Status: PARTIALLY VERIFIED (B2's env as designed); per-client proxy behaviour UNVERIFIED (inference)
Evidence kind: design proposal over B2 §1.3
Evidence: B2 §1.3 ("`no_proxy=127.0.0.1,localhost`"). C2 §1.1 (model route is the relay).
Retrieved: 2026-11-06T23:36Z
Implication for the spec: The S-pos smoke asserts that every model call arrives on the model socket and that the mediator saw no request for the relay address or the model host. A harness whose client ignores `no_proxy` gets a declared per-harness env override, recorded in its admission.
Outline delta: EXTENDS OL-02.
Limitations: Client proxy semantics (Node `undici`, Python `httpx`, Go) were not read.

### F9 — For installed harnesses the model route is reachable from the agent's own tool commands; receipts stay complete but attribution does not
Claim: An installed harness passes its environment, including base URL and slot token, to the shell commands it runs for the model. An agent could therefore call the proxy directly from a tool. The proxy would still receipt and charge such a call, so mediation completeness holds. It could not tell whether the harness or a tool subprocess made the call. Terminus-2's cell has no model socket, so this route does not exist there.
Status: UNVERIFIED (inference from process-environment inheritance; design proposal)
Evidence kind: inference + design proposal
Evidence: §1.5 env mechanisms. C2 §1.2 (`user-agent` is in the forwarded and recorded header allowlist; header names recorded). C3 F9 ("calling another model counts as a fail" on HF).
Retrieved: 2026-11-06T23:36Z
Implication for the spec:
- Declare residual `side_channel{model_route_reachable_from_agent_tools}` for every `in_environment` harness.
- The admission records each harness's request fingerprint, its `user-agent` and header-name set from S-pos. The integrity scan (D1) flags receipts whose fingerprint differs.
- Stripping the token from tool subprocesses would modify the harness, so it is not proposed.
Outline delta: EXTENDS OL-15 (integrity residual).
Limitations: A determined agent could copy the harness's headers, so the fingerprint check is a heuristic.

### F10 — Terminus-2 can be hosted host-side, as in Harbor, by bridging its five environment operations to HH-dispatched effects
Claim: Terminus-2 uses only a small `BaseEnvironment` surface:
- `exec(command, cwd, env, timeout_sec, user)`;
- `upload_file` and `download_file`;
- `is_dir`;
- the properties `default_user`, `session_id` and `trial_paths`.

An HH-owned Python driver can run Harbor's unmodified `Terminus2` class in a host process against a bridge environment. The bridge forwards each operation over a per-slot socket to the slot worker, which dispatches it as an effect in the cell. Model calls use `api_base` to a host-side relay instance on the slot's proxy socket. This preserves Harbor's placement (host loop, `compose exec`-equivalent calls, tmux in the container). The agent's in-cell processes cannot kill the harness. The cell needs no model socket.
Status: VERIFIED (the environment surface used); the driver UNVERIFIED (design proposal; riskiest assumption)
Evidence kind: source inspection + design proposal
Evidence: H2 grep counts (`terminus_2.py`: `environment.exec` ×2, `default_user`, `is_dir`, `trial_paths`; `tmux_session.py`: `exec` ×23 + 2, `upload_file`, `download_file`, `session_id`). `environments/base.py:1358-1380` (`exec` signature). `docker.py` `_compose_exec` (`exec -w -e -u main` + platform shell wrapper). `tmux_session.py` `_install_recording_tools` (checks `tmux -V` and `asciinema --version` and installs only if missing, bounded by a budget). `terminus_2.py:470-500` (LiteLLM built with `api_base`, `reasoning_effort`, `model_info`). E1 B.7 (Harbor's own tests drive Terminus-2 against a fake `/v1/chat/completions` endpoint).
Retrieved: 2026-11-06T23:42Z
Implication for the spec: Two alternatives were rejected.
- **In-cell Terminus-2.** Rejected for three reasons. It needs Harbor plus Python in every cell. Its model calls would originate in the cell, where Harbor's do not. And a task command such as `pkill python` could kill the harness, a failure mode absent from O.
- **Harbor orchestrating the H trial through a custom environment.** Rejected because it bypasses HH's lifecycle, verdict and receipts (B2 F2).

The driver imports Harbor internals, so it is pinned to the Harbor commit (F3). It is tested by the replay smoke and by R-replay against Harbor's own host-side run.

The Terminus layer pre-installs tmux, and asciinema when `record_terminal_session` is on (Harbor's default). Harbor's setup install then becomes a no-op on both surfaces, as it is on O whenever the image already has them.

Bridge exec latency differs from `compose exec`. The difference is declared, because pane timing depends on it (E1 F11).
Outline delta: EXTENDS OL-02, OL-05 (answers C3 Q-C3-6, B2 OQ-6).
Limitations: `BaseEnvironment.__init__` requires task and trial configuration objects; their construction in the shim was not traced. Abstract methods Terminus-2 never calls must be implemented as typed refusals. The ADR status of committed Python glue run by an external tool is H1's call.

### F11 — Terminus-2's LiteLLM is fixed by Harbor's lockfile only if the host venv is built from it
Claim: Harbor v0.25.0 declares `litellm>=1.92.0` and its `uv.lock` resolves `litellm 1.93.0`. A host Harbor installed by resolving the requirement, rather than syncing the lock, can carry a different LiteLLM than the one the E1 and D2 references assume.
Status: VERIFIED
Evidence kind: source inspection
Evidence: H2 `pyproject.toml:9,21` (`requires-python = ">=3.12"`, `"litellm>=1.92.0"`); `uv.lock:2652-2653` (`name = "litellm"`, `version = "1.93.0"`).
Retrieved: 2026-11-06T23:43Z
Implication for the spec: The host Harbor environment is built with `uv sync --frozen` from the pinned checkout, with its lock hash recorded. That one venv serves both `harbor run` (O) and the HH Terminus driver (H), so Terminus-2's transport library is identical on both surfaces by construction. This refines REQ-XE-003 for Terminus-2.
Outline delta: EXTENDS OL-06 (same participant includes the host transport).
Limitations: Whether `uv sync --frozen` succeeds offline on the phase VM was not checked. It needs package egress at build time, like the bundles.

### F12 — Proxy receipts are the authoritative model-I/O record; native logs need a container copy-out and post-hoc ATIF conversion
Claim: The proxy captures everything the model saw and emitted, so it is the transcript of record (C3 F10). Native logs live at fixed paths under `/logs/agent` inside the container (§1.5). Collecting them needs a kernel-mediated container-path copy-out at S4. ATIF for the H surface can come from Harbor's own converters, run post hoc on the host, which keeps the format symmetric with O.
Status: VERIFIED (log paths; `env_download` exists); copy-out scope and post-hoc conversion UNVERIFIED
Evidence kind: source inspection + design proposal
Evidence: §1.5 artifact column (H2 adapter `run`/`populate_context_post_run`). `pi.py` `populate_context_post_run` converts session JSONL to `trajectory.json` on the host. `kimi_code.py` uses `build_trajectory(self.logs_dir, …)`. `crates/hh-env/src/driver.rs:3600-3633` (`env_download` reads via `fs_read`, deposits a blob, mints `action.environment.downloaded`; refuses provider and hosted handles). B2 F8 (the same open question for `/tests` copy-in).
Retrieved: 2026-11-06T23:41Z
Implication for the spec:
- **S4 copy-out.** `/logs/agent` is copied out at S4, after the agent ends and before the verifier phase, so the artifacts are those the agent left. They are hashed into the slot record's `trajectory` section (B2 §1.4).
- **Integrity-scan bundle.** The scan reads receipts, egress rows, seal, native artifacts and the pane log. Its leak detection runs on what the model **received** (request bodies), as HF did (C3 F10), not on native logs.
- **ATIF.** A missing ATIF is a reported attribute, never a failure.
Outline delta: EXTENDS OL-02, OL-15.
Limitations: Whether `fs_read` reaches container paths outside `/work` is unestablished. Running Harbor's converters on H logs assumes the converters depend only on the log directory. That holds for the two adapters read; the rest are unchecked.

### F13 — Instruction passing differs per adapter, and Pi passes the instruction positionally without `--`
Claim: Adapters pass the instruction in three ways:
- shell-quoted argv: mini-swe-agent `--task=`, OpenHands `--task=`, OpenCode after `--`, Pi positional;
- an environment variable: Hermes `-q "$HERMES_INSTRUCTION"`, Kimi `--prompt "$VAR"`;
- environment variable to stdin: Claude Code.

At v0.25.0 Pi's command places the quoted instruction after the flags with no `--`. HF found that Pi exits on an instruction beginning with `-` (C3 F13).
Status: VERIFIED (commands); the Pi failure is HF's report
Evidence kind: source inspection
Evidence: H2 `pi.py` `run` (`f"{cli_flags}" f"{escaped_instruction} "`); `opencode.py` run (`--dangerously-skip-permissions -- {escaped_instruction}`); `claude_code.py` run (instruction env var → `printf | claude … --print`); `hermes.py:667,711`; `kimi_code.py:353-366`; `openhands.py:1063-1064`; E1 A.1.
Retrieved: 2026-11-06T23:42Z
Implication for the spec: H reproduces each adapter's passing byte for byte. R-replay compares the instruction bytes in the first request across surfaces. A deterministic pre-registration check flags any selected task whose instruction begins with `-` for Pi. The fix (`--`) is then applied to both surfaces as a declared deviation, or the pair is pre-registered as a known hazard. D1 decides.
Outline delta: EXTENDS OL-09.
Limitations: Whether any TB 2.1 instruction starts with `-` was not checked. Harbor's reading of `instruction.md` (trailing-newline handling) was not traced.

### F14 — The runner check is a $0 replay equality test plus a small paid descriptive comparison, with O routed through the same proxy
Claim: Running the same original binary on two hosting surfaces under C2's replay mode yields a deterministic comparison of configuration and canonical downstream request sequences, at $0. A live comparison can only be descriptive, because the two trials share no randomness. Routing O through the HH proxy is required for three reasons: budget enforcement, symmetric receipts, and the Ollama key staying in the proxy. Calling Ollama directly from Harbor ("Harbor direct", C2 F6's proposed validation) would break all three.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal over source inspection
Evidence: C2 F15 (replay, keying, canonicalizers, divergence policy), C2 F6 (validation wording), C2 §1.1 (key held only by the proxy). B1 F12 (`--base-url`, `--api-key-name`, `--api-format`; Harbor auto-allowlists the configured base URL under a restricted agent phase). B2 §1.7 (job-config writer, read-only ingest). D2 (L2 is the parity method).
Retrieved: 2026-11-06T23:36Z
Implication for the spec:
- **Reaching the proxy from O.** O trials connect through a host relay instance bound to the podman bridge gateway, token-gated, one per O slot. The runner writes **one Harbor job per O slot**, so tokens and budget slices stay per slot.
- **Posture.** The agent-phase posture is the same allowlist digest on both surfaces. Enforcement differs (Harbor's gost sidecar vs HH's mediator), and that difference is declared.
- **Correction.** This refines C2 F6's proposed validation: the comparison is of *downstream* request bytes, with both surfaces behind the same proxy.
Outline delta: EXTENDS OL-02, OL-06 (fidelity check distinct from parity).
Limitations: Harbor container access to a host listener on the bridge gateway under Harbor's egress sidecar was not traced. Per-slot Harbor jobs add per-job overhead that was not measured.

### F15 — A $0 replay smoke per harness can discharge C3's G3, G5, G6 and part of G8 deterministically before any paid trial
Claim: Using C2's `replay` mode with a hand-scripted recording in each harness's own tool schema, one HH smoke task exercises install verification, configuration, proxy routing, completion detection and egress containment for that harness. Four negative variants (§1.9) answer C2's open question 6 per harness: 402, 429, heartbeat or slow release, and hang.
Status: UNVERIFIED (design proposal; scripts are per-harness work)
Evidence kind: design proposal
Evidence: C2 F15 (`replay` has no upstream port and costs $0 structurally; `serve_recorded`; `latency: recorded`), C2 F16 (402, 429, 409 shapes), C2 open question 6. C3 G3/G5/G6/G8. E1 B.7 (Harbor's precedent of scripted fake-LLM integration tests).
Retrieved: 2026-11-06T23:37Z
Implication for the spec:
- **Gate.** The smoke is a hard precondition of any paid stage, and its records are the G3/G5/G6 evidence in the admission.
- **G8 split.** G8's endpoint-compatibility half (`tool_choice`, developer role, `cache_control`) cannot be proven against a replay. It remains for P0, with the proxy's field-policy receipts as evidence.
- **Recordings.** Recordings for streaming harnesses are authored as raw SSE bytes. A harness that does not expect them shows it at once, as a typed divergence or exit.
Outline delta: EXTENDS OL-09 (admission gates made executable), OL-15.
Limitations: Authoring per-harness scripts needs a first capture of each harness's request shape, i.e. a discovery run against a replay that records requests. It needs a Linux podman host (C2 F5).

### F16 — Hosting failures map onto B2's disposition vocabulary with one addition: model-route-terminated runs are infrastructure, not task failures
Claim: The §1.10 table maps every hosting failure without relabelling. The one rule B2 F16 does not already state: when a harness exits non-zero and its last model call ended in a proxy- or upstream-side failure, the slot is `inconclusive{infra:model_route:*}` (Harbor's `model_provider_failed`, an invalid reward). Otherwise, any harness exit leads to verification and a scored verdict with its end cause.
Status: UNVERIFIED (design proposal over VERIFIED vocabularies)
Evidence kind: design proposal
Evidence: B2 F16 (outcome-class mapping; only `infrastructure_failure` replans), C2 F16 (dispositions; "infra; never a task failure"), B1 F13 (Harbor causes: `model_provider_failed` invalid, `AgentTimeoutError` valid).
Retrieved: 2026-11-06T23:37Z
Implication for the spec: The classifier is a pure function of the slot's recorded facts (exit code, ordered receipt dispositions, mediator rows, S2/S4 probes), so it can be recomputed from the records. Client abandonment is recorded and charged, never hidden. Pilot data must show `queue_wait + latency` p99 below each harness's model-client timeout.
Outline delta: EXTENDS OL-02, OL-03 (missingness inputs).
Limitations: Harness model-client timeouts are known only for mini-swe-agent (3600 s, Harbor) and Terminus-2 (LiteLLM default, not read).

### F17 — Claude Code's admission terms (Q-X26) translate into a concrete, checkable configuration
Claim: The five Q-X26 terms map onto mechanisms that can be checked:
- **Unmodified pinned binary.** Install by the Harbor recipe at an exact version; record the binary's sha256; check at S2 and S4.
- **No Claude account.** `ANTHROPIC_API_KEY=""`, `CLAUDE_CODE_OAUTH_TOKEN=""`, `ANTHROPIC_AUTH_TOKEN=<slot token>`, so no OAuth or subscription path is used.
- **Black-box arm only.** Not an HIR target.
- **Name without logo.**
- **Disclosure.** State Anthropic's "not supported" statement.

The mediation-completeness side needs all alias model variables, so background and subagent calls route to `glm-5.3-flash`. The Anthropic `count_tokens` route is refused by the proxy, and Claude Code falls back to an estimate (C3 F4).
Status: VERIFIED (adapter mechanisms); posture realization UNVERIFIED
Evidence kind: source inspection + design proposal
Evidence: H2 `claude_code.py` `_resolve_auth_env` (external-provider branch sets `ANTHROPIC_AUTH_TOKEN`, empties `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN`), `run` (alias env, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`, `IS_SANDBOX`), `_installed_claude_satisfies_version`. Ledger Q-X26 (verbatim answer). C3 F6. C2 wire matrix (`count_tokens` typed refusal).
Retrieved: 2026-11-06T23:41Z
Implication for the spec: The admission's `legal` member quotes the terms and the Q-X26 answer. The S-pos smoke shows no request to an Anthropic host; any attempt is denied and logged. The site and records name Claude Code in plain text only.
Outline delta: EXTENDS OL-09, OL-15.
Limitations: Claude Code is closed source. Its full non-model egress is known only from the probe.

### F18 — Harbor's installed-agent version detection and HH's S2/S4 probes should agree, which closes the self-update loop
Claim: Harbor at HEAD records the version the environment provides when `skip_install` is used, and warns on mismatch. Each adapter defines a `get_version_command` (e.g. `pi --version` after sourcing nvm). HH's S2 probe and S4 re-probe use the same command, so a harness that self-updates or is modified mid-run is detected on H. The detected-version comparison in R-replay covers O.
Status: VERIFIED (Harbor behaviour); HH probes UNVERIFIED
Evidence kind: source inspection + design proposal
Evidence: H3 `installed/base.py:1191-1200` (HEAD); H2 `pi.py` `get_version_command`, `openhands.py:188`, `kimi_code.py:224`, `claude_code.py:306`.
Retrieved: 2026-11-06T23:41Z
Implication for the spec: S2 and S4 run the adapter's version command and hash the install tree. Any difference flags `harness_self_modified`.
Outline delta: EXTENDS OL-15.
Limitations: The install-tree hash budget for large trees (OpenHands venv) is unmeasured. It can be restricted to the harness's entry-point files.

## 3. ADRs and amendments this design requires

| ID (candidate) | Subject | Why |
|---|---|---|
| **ADR-C4-A** | `hh-harness-admission/1` record, bundle and layer build (runner sub-command over `podman build`, recorded as a host act), S2 verify-only setup, Harbor-derived recipes with attribution | new record schema and a new image-construction path |
| **ADR-C4-B** | Terminus-2 `lab_host` hosting: per-slot exec bridge (effects dispatched through the existing helper exec), HH-owned Python driver executed by the pinned external Harbor venv, host relay instance; `LabHost` added to adapter C's `placement_supported` | new placement for an existing adapter; first committed glue run by an external tool's interpreter |
| **ADR-C4-C** (or folded into B2's Harbor tool admission, B2 OQ-8) | Harbor pin containing `skip_install` (F3), host venv from `uv.lock` (F11) | changes B2/D1's pinned `v0.25.0` |
| none | per-harness renderers, smoke battery, runner-check record `hh-runner-check/1` | implementation within ADR-C4-A/B and B2's runner ADR |

No new Rust dependency is required. Harbor stays an external pinned tool. The bundles' contents (Node, Python, npm and PyPI packages) are participant content inside the cell, not workspace dependencies.

## 4. Ticket shaping hints (each one fresh-context `implement-spec` run; acceptance deterministic, no paid calls)

1. **ADMIT-CORE: admission record, validator, recipe and Containerfile renderer, build sub-command, S2/S4 probes.** ADR-C4-A. AC:
   - the validator truth table: each refusal of F5, plus a complete record passing;
   - golden Containerfiles for two harnesses × two task bases;
   - a `podman` stub on PATH (the `r5_56` pattern) pins the build argv and digest capture;
   - S2 refuses a mismatched layer as `withheld{admission:harness_layer_mismatch}`.

   Live leg (`#[ignore = "live — ADR-0363: …"]`): build one real bundle and layer on the phase VM.
2. **ADAPT-CHAT-PY: renderers for mini-swe-agent, OpenHands and Hermes.** AC: golden `{argv, env, files}` per harness, reviewed against the cited Harbor lines; secrets elided in the recorded form; `LITELLM_LOCAL_MODEL_COST_MAP` present where LiteLLM is used; `no_proxy` present.
3. **ADAPT-CHAT-NODE: Pi, Kimi Code and OpenCode.** AC as in 2, plus the Pi leading-dash pre-check (F13) as a pure function over instruction bytes.
4. **ADAPT-MESSAGES: Claude Code, plus its legal-posture record.** AC: golden env with every alias variable; the Q-X26 fields required by the validator; the binary-hash check.
5. **TERMINUS-HOST: exec bridge in the slot worker, Python driver, host relay instance.** ADR-C4-B. AC (offline):
   - bridge protocol tests against a fixture executor;
   - every op becomes a receipted effect;
   - a refused op (abstract methods) is typed.

   Live $0 leg: the driver runs Harbor's `Terminus2` against a replay recording in a podman cell.
6. **CAPTURE: `/logs/agent` copy-out, post-hoc ATIF conversion through Harbor, integrity-input bundle, slot-record `trajectory` fields.** AC: copy-out refuses `/work` aliasing; the fixture native logs convert, with the ATIF version recorded; the bundle manifest digests recompute.
7. **SMOKE: the §1.9 battery, 5 variants × 8 harnesses.** Committed scripted recordings and smoke task. Live $0 on the phase VM, co-scheduled with B2's re-proof. AC: the assertion list per variant, computed records-in by a committed checker that also runs offline over fixture slot records.
8. **RUNNER-CHECK: O-surface routing (host relay on the bridge gateway, one job per slot), R-replay comparison, `hh-runner-check/1`, R-live folded into D1's P0.** AC: the offline comparator over two fixture slot-record sets (equal, masked-equal, divergent); job-config golden.

Order: B2 tickets 1, 3, 4 and C2 tickets 3, 5, 6 come first. Then 1 → (2 ∥ 3 ∥ 4 ∥ 5) → 6 → 7 → 8. Tickets 2–4 can merge if the size trigger allows; 5 stands alone.

## Answer and completion assessment

C4's question is answered at design level:
- **Installation (F2–F5).** A pinned harness enters the cell as a two-stage prebuilt layer: a once-built content-addressed bundle plus a thin per-task layer that replays Harbor's system-dependency step. Both are built with egress at admission and run offline, and the admission record pins everything.
- **Configuration (F6–F9, F13, F17).** Each harness is configured through the proxy by a golden-tested renderer that mirrors its Harbor adapter (§1.5). Disable switches are set, and the residual egress is fail-soft and logged.
- **Terminus-2 (F10, F11).** It stays host-side, as Harbor runs it, through a bridge to HH-dispatched effects.
- **Trajectories (F12).** Proxy receipts are authoritative. Native logs are collected at S4 and converted to ATIF with Harbor's code.
- **Runner check (F14).** A $0 replay equality test plus a small descriptive paid comparison, labelled `parity: none`.
- **Admission smoke and failures (F15, F16).** A $0 five-variant replay smoke per harness discharges G3, G5 and G6 before any paid trial. Hosting failures map onto B2's dispositions with one added infrastructure rule.

**Settled:** the installation strategy, the admission-record interface (Q-X27-agnostic), the per-harness configuration mechanisms, Terminus-2's placement, the capture authority order, the runner-check definition and size, the smoke design, the failure taxonomy and the ticket seams.

**Technical blockers before paid runs:**
1. the Harbor pin containing `skip_install` (F3), without which O/H image identity fails;
2. the kernel-mediated container-path copy-in/out (F12, shared with B2 F8);
3. the Terminus-2 bridge driver (F10);
4. B2/C2 machinery (relay, proxy replay mode, mediator).

**Acceptable uncertainty for this row:** per-harness telemetry hosts, client proxy and timeout behaviour, base-URL suffixes, Hermes and OpenCode effort semantics, and disk sizing. The smoke and B3 resolve each.

## Open questions

1. **OQ-1. Harbor pin (F3).** Owner: H1 with B2. Affects B2, C4, D1, D2, E1/E2. **Operator decision: yes**, because it replaces a stable-tag pin with a commit pin or a wait. Options: (b) the first stable tag containing `skip_install`, if one exists by the freeze (recommended); (a) an admitted commit such as `ff2a1ef4`; (c) v0.25.0 with non-identical O images, labelled.
2. **OQ-2. R-live funding and size (§1.8).** Owner: D1. Affects H1. Operator decision: at the pilot gate. Choose 24 O trials (3 tasks × 8 harnesses, ≈ P0 cost again) or 8 (1 task each).
3. **OQ-3. Terminus-2 `lab_host` (F10).** Owner: C4 (recommends), with E2/H1 to confirm. Operator decision: no. Answers C3 Q-C3-6 and B2 OQ-6.
4. **OQ-4. Disk-bound fallback (F4).** Owner: B3. Operator decision: no. If 240 thin layers exceed the VM disk, admit the bind-mounted bundle plus a system-deps layer per task *base* instead of per task image, declared.
5. **OQ-5. Hermes install source.** Owner: D1/C4. Operator decision: no. PyPI at an exact version (HF's route) instead of Harbor's `install.sh@main --branch <v>`, recorded as a deviation on both surfaces.
6. **OQ-6. Pi leading-dash hazard (F13).** Owner: D1. Operator decision: no. Apply `--` on both surfaces if the pre-check fires on a selected task.
7. **Pending §Q interplay.** Q-X22: the effort value enters each renderer, and the proxy pins it. Q-X27: the record is the interface; the policy picks the version. Q-X29: `limits_ref`, with LiteLLM resolved once into the bundle lock. Q-X39: hosting adds **no** agent-phase hosts, and harness egress is denied, never granted. None blocks this design.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale | OL | Acceptance |
|---|---|---|---|---|
| REQ-XC-301 | Every phase-2 participant SHALL be declared `container_installed` with `HostingExt{model_io_intercept: base_url, process_placement: in_environment \| lab_host}`; `model_io` SHALL be earned only from proxy receipts; adapter C's `placement_supported` SHALL include `lab_host`. | F1; C2 F1 | OL-15 | Declaration fixture validates; native-log lift never sets `mediation: mediated` |
| REQ-XC-302 | Each harness SHALL have an `hh-harness-admission/1` record (§1.3), validated before any slot; the validator SHALL refuse a missing gate, an unpinned or floating version, a missing lock/cutoff, bundle or layer digest, config template hash, `expected_denied` list, or legal-posture record. **REFINES REQ-XC-201.** | F5 | OL-09, OL-15 | Validator truth-table tests |
| REQ-XC-303 | A harness bundle SHALL be built once per (harness, version, lock) at admission, at its final absolute path, from an HH recipe derived from the pinned Harbor commit's `install()` with attribution and listed deviations, and SHALL be content-addressed with its resolved manifest and build log. | F2, F4 | OL-02 | Golden recipe; bundle digest recorded; manifest lists every package@version |
| REQ-XC-304 | Each (harness × task image) layer SHALL be built `FROM` the task image digest, replay the adapter's system-dependency step, place the bundle at the adapter's expected paths, and record its digest and package delta manifest; cells SHALL run it by digest only. **REFINES REQ-XC-204.** | F2, F4 | OL-02 | Stub-podman argv golden; live build leg records digest |
| REQ-XC-305 | No harness SHALL install or fetch anything at slot run time; S2 SHALL be verify-only (version probe, bundle tree hash, rendered-config hash), and a mismatch SHALL be `withheld{admission:harness_layer_mismatch}`. | F4, F18 | OL-02, OL-16 | Mismatch fixture yields withheld; no network in S2 |
| REQ-XC-306 | The O surface SHALL run the same layer digest via Harbor `skip_install` at an admitted Harbor pin containing it; if no such pin is admitted, every runner check SHALL be labelled not image-identical. | F3 | OL-02, OL-06 | Job config carries `skip_install=true` and the digest; label check |
| REQ-XC-307 | Each harness SHALL have a pure configuration renderer (admission × slot → argv, env, files) mirroring its Harbor adapter at the pinned commit; the slot record SHALL carry the rendered-config hash with secrets elided. | F6 | OL-02, OL-09 | Golden renderer tests per harness |
| REQ-XC-308 | The hosted-cell environment SHALL be a closed set (harness base-URL/key variables with the slot token as sole credential, B2 routing hints, `no_proxy=127.0.0.1,localhost`, declared disable switches, image `HOME`/user); `/logs/agent` and `/installed-agent` SHALL exist and `/tests`, `/solution` SHALL be absent. | F8; B2 §1.3 | OL-02, OL-15 | Env-projection and fs-listing fixture tests |
| REQ-XC-309 | Each harness's native effort mechanism SHALL be set to the protocol's pinned level on both surfaces, in addition to the proxy pin; requested and applied values SHALL be recorded per call. | F6; C2 F7; Q-X22 | OL-03, OL-07 | Renderer golden; receipt fields |
| REQ-XC-310 | Every LiteLLM-based harness and the Terminus-2 host SHALL run with `LITELLM_LOCAL_MODEL_COST_MAP=True` on both surfaces, recorded as declared configuration. | F7 | OL-06 | Renderer golden; R-replay config equality |
| REQ-XC-311 | Each admission SHALL list known disable switches and `expected_denied` hosts; every agent-phase non-model attempt SHALL be a logged residual; a harness that cannot complete with its non-model egress denied SHALL fail G5 and SHALL NOT be granted hosts. | F7; Q-X30, Q-X39 | OL-02, OL-15 | S-pos smoke: mediator rows ⊆ expected_denied, harness completes |
| REQ-XC-312 | The S-pos smoke SHALL prove every model call arrived on the model socket and that the egress mediator saw no request for the relay address or model host. | F8 | OL-02 | Smoke assertion from records |
| REQ-XC-313 | `in_environment` harnesses SHALL declare residual `side_channel{model_route_reachable_from_agent_tools}`; admissions SHALL record each harness's request fingerprint (user-agent, header-name set) and the integrity scan SHALL flag receipts that do not match. | F9 | OL-15 | Fingerprint-mismatch fixture flagged |
| REQ-XC-314 | Terminus-2 SHALL run `lab_host`: Harbor's unmodified `Terminus2` in a host process over an HH bridge whose exec/upload_file/download_file/is_dir operations are dispatched as receipted effects in the cell; its model calls SHALL use a host relay to the slot's proxy socket; its cell SHALL have no model socket; its kwargs SHALL be the admitted options verbatim with `model_info` declared. | F10; D2 R1 | OL-02, OL-05 | Bridge protocol tests; live $0 replay leg |
| REQ-XC-315 | The host Harbor environment SHALL be built from Harbor's `uv.lock` (`uv sync --frozen`) with its lock hash recorded, and SHALL serve both O and the Terminus-2 driver. **REFINES REQ-XE-003** for Terminus-2. | F11 | OL-06 | Record carries lock hash and resolved LiteLLM version |
| REQ-XC-316 | The Terminus-2 layer SHALL pre-install tmux (and asciinema when recording is on) so Harbor's setup install path is a no-op on both surfaces. | F10 | OL-02 | S-pos shows no install exec in setup |
| REQ-XC-317 | Proxy receipts SHALL be the authoritative model-I/O record; `/logs/agent` SHALL be copied out by a kernel-mediated container-path download after agent end and before verification, hashed into the slot record. | F12; C3 F10 | OL-02, OL-15 | Copy-out fixture; slot-record digests recompute |
| REQ-XC-318 | ATIF for H slots SHALL be produced post hoc by Harbor's converter at the pinned commit over the collected logs, with its schema version recorded; its absence SHALL be a reported attribute, never a failure. | F12 | OL-06, OL-18 | Fixture conversion; missing-ATIF slot still scored |
| REQ-XC-319 | Each slot SHALL assemble an integrity-input bundle (receipts, egress rows, seal, native artifacts, terminal transcript) for D1's required scan; leak detection SHALL run on model-received request bodies. | F12; D1 F13 | OL-15 | Seeded-leak fixture detected from receipts alone |
| REQ-XC-320 | Instruction passing SHALL reproduce each adapter's mechanism byte for byte; a deterministic pre-check SHALL flag harness-specific instruction hazards (Pi leading `-`) before selection freezes. | F13 | OL-09 | Pre-check unit test; R-replay instruction-byte equality |
| REQ-XC-321 | Completion SHALL be detected as harness process exit (exit code recorded), with harness-native terminal status parsed descriptively; nothing but the task deadline and budget refusal SHALL stop a harness. | F13, F16 | OL-02 | Smoke S-pos/S-hang/S-402 end causes |
| REQ-XC-322 | Admissions SHALL record each harness's model-client timeout and retry behaviour where known; calls abandoned by the client before release SHALL be receipted `released: client_gone` and charged; the pilot SHALL report `queue_wait + latency` p99 against each client timeout. | F16; C2 F9 | OL-03, OL-08 | Fixture: client close before release → receipt + charge |
| REQ-XC-323 | Hosting failures SHALL map to dispositions by the §1.10 table through a pure classifier over recorded facts; a non-zero exit following a model-route failure SHALL be `inconclusive{infra:model_route:*}`. | F16; B2 F16; C2 F16 | OL-02, OL-03, OL-16 | Classifier truth-table tests; records-in recompute |
| REQ-XC-324 | Before any paid stage, each harness SHALL pass the §1.9 $0 replay smoke battery (S-pos, S-429, S-402, S-slow, S-hang) on the phase backend; its records SHALL be the admission's G3/G5/G6 evidence. | F15 | OL-09, OL-15 | Smoke records reviewed at the gate; offline checker over fixture records |
| REQ-XC-325 | The per-harness runner check SHALL consist of R-replay ($0: config equality and canonical downstream request-sequence equality modulo declared masks) and R-live (descriptive verdict comparison on P0 tasks); it SHALL be labelled `fidelity check, parity: none` and SHALL NOT be used as L2 parity input. | F14; D2 | OL-02, OL-06 | Comparator tests; phrase pin |
| REQ-XC-326 | O-surface trials SHALL reach the model only through the HH proxy (host relay, one Harbor job per O slot, per-slot token and slice), never Ollama directly. | F14; C2 §1.1 | OL-08, OL-15 | Job-config golden; no key in any Harbor env |
| REQ-XC-327 | Claude Code's admission SHALL realize Q-X26: exact version with binary sha256 checked at S2/S4, no OAuth/API key (slot token as `ANTHROPIC_AUTH_TOKEN` only), all alias model variables set to the participant model, non-essential traffic and web tools disabled, black-box only, plain-text name, disclosure. | F17; Q-X26 | OL-09, OL-15 | Validator requires fields; S-pos shows no Anthropic-host request succeeded |
| REQ-XC-328 | S4 SHALL re-run the adapter's version command and hash the harness install tree; any change SHALL flag `harness_self_modified`. | F18 | OL-15 | Fixture with altered tree flagged |
| REQ-XC-329 | All acceptance of hosting machinery SHALL be offline (renderers, validator, classifier, comparator, copy-out fixtures); smoke, build and runner-check legs SHALL be `#[ignore = "live — ADR-0363: …"]` $0 legs, and R-live data SHALL be claimed only under a recorded spend authorization. | F15; ADR-0363 | OL-15 | CI offline; ignore reasons cite ADR-0363 |

## Handoff

- **Dependencies resolved.**
  - B2: S2 hook, relay pairs, slot envelope `participant` and `trajectory` members, failure vocabulary.
  - C2: composite declaration, replay/record, failure shapes, open question 6 answered by the §1.9 battery.
  - C3: shortlist, G1–G9; REQ-XC-201/204 refined.
  - B1: install templates, Harbor flags, ATIF.
  - E1: anatomy, Terminus-2 host placement.
  - D1 (in progress) and D2: version policy, Harbor defaults, LiteLLM freeze, `model_info`.
- **Supplies.**
  - **B2:** the S2 verify-only check, the S4 copy-out and re-probe, the Terminus bridge ops in the slot worker, and one-job-per-slot Harbor configs.
  - **C2:** host relay instances for Terminus-2 and O slots, the `client_gone` release state, and request fingerprints.
  - **B3:** bundle and layer build time and disk (OQ-4), and the Linux host needed for the smoke.
  - **D1:** the runner-check size (OQ-2), the integrity-input bundle, the failure classifier and the Pi hazard check.
  - **D2/E2:** the Terminus-2 driver as the O-equivalent H host, and the frozen host LiteLLM.
  - **F2:** the native artifact list and ATIF provenance.
  - **H1:** ADR-C4-A/B/C and OQ-1.
- **Conflicts with other notes.**
  - B2 §1.7, B2 OQ-8 and D1 F14 pin Harbor `v0.25.0`, which lacks `skip_install`. That contradicts C3 F8's implication and REQ-XC-204 as written (F3). H1 must rule.
  - C2 F6 names "Harbor direct" as its validation baseline; this note routes O through the proxy (F14).
  - B2 OQ-6 suggested C2's loopback TCP listener for Terminus-2; this note uses a host relay on the slot socket instead. Either satisfies C2's design, and the relay needs no proxy change.
- **Proposed follow-up rows.** None. A per-harness request-shape discovery capture is folded into ticket 7.
- **Environment observation (for the coordinator).** During this row the host data volume reported 145 MiB free (`df` at about 23:43Z). It later showed 11 GiB free, after I deleted my own 144 MB scratch clone and something outside this session freed space. Workers cloning repositories should check free space first.
- **Capability/portfolio dispositions.** None changed. Nothing here is executed evidence.
