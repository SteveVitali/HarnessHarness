# 28_reference-harness-anatomy — mini-swe-agent and Terminus-2 anatomy
Stream: E
Researched: 2026-11-06
Author: Claude Code research worker (Opus 5.5), row E1 of the experiments research ledger
Ledger rows covered: E1
Repository baseline: main 75883daa (read from worktree `claude/experiments-spec` at abf26c97, which differs from 75883daa only in `docs/`; `git diff --stat 75883daa abf26c97` shows four docs files). The coordinator's uncommitted edit to `docs/experiments-research-ledger.md` was present and is not mine.
Confidence overall: high for the source-inspection findings; medium for the LiteLLM wire-shape inferences (F7, F8), which were not executed.

## Scope, method and pins

This note describes, from source, the two harnesses that Phase 3 re-expresses in HIR. It covers what each sends to the model, how it parses replies, how it acts in the environment, and how it stops. It also covers how Harbor runs each harness, and which of those behaviours HH cannot express today. Nothing was installed, built or executed. Both public repositories were cloned read-only into the session scratch directory, and the requested tag or commit was checked out inside the clone.

| Source | Pin read | Retrieved |
|---|---|---|
| github.com/SWE-agent/mini-swe-agent | tag `v2.4.6` = `a83fcae82d2a08f0ee0c688f9d137b3566c097f8` (2026-07-22); `__version__ = "2.4.6"` at `src/minisweagent/__init__.py:11` | 2026-11-06T22:53Z |
| github.com/harbor-framework/harbor | `main` HEAD `ff2a1ef48070a8df374669138e4d4db79b77f013` (2026-11-06), `git describe` = `v0.25.0-93-gff2a1ef4`; `pyproject.toml:3` `version = "0.25.0"` | 2026-11-06T22:54Z |
| Harbor `v0.25.0` = `fdba28fc411e47e8765420a1a063b4918fce3def` (2026-10-25) | compared only. Between `v0.25.0` and HEAD there is no change under `src/harbor/agents/terminus_2/` except `tmux_session.py` (the paste chunk length went from 65 536 to 32 768). `llms/` is unchanged. In `installed/mini_swe_agent.py` the only change is the install line `--with litellm` → `--with 'litellm[mcp]'`. `installed/base.py` changed by 78 lines; I did not review that diff, and it is not on any path quoted below. | 2026-11-06T23:00Z |
| HF harness leaderboard `docs/blog/numbers.md` (https://huggingface.co/spaces/joelniklaus/harness-leaderboard/raw/main/docs/blog/numbers.md; the page's "Live board … generated 2026-11-02T12:52:42Z") | It pins "mini-swe-agent 2.4.6" and names "Terminal-Bench 2.1: 87 tasks". It does **not** mention Harbor, Terminus or any mini-swe-agent config, step limit or cost limit. | 2026-11-06T22:53Z (via fetch agent) |
| Local `terminal-bench` 0.2.18 (`~/.local/share/uv/tools/terminal-bench/lib/python3.12/site-packages/terminal_bench/`) and its bundled `litellm` 1.108.1 | used only for comparison. The bundled LiteLLM is not the version Harbor would install. | 2026-11-06T22:58Z |
| HH: `crates/hh-hir/src/records.rs`, `crates/hh-control/src/{driver,strategy}.rs`, `crates/hh-embed/src/{work,open,service,runtime,gateway_model}.rs`, `crates/hh-gateway/src/openai.rs`, `crates/hh-env/src/capture.rs` | main 75883daa | 2026-11-06T22:59Z–23:00Z |

Unless stated otherwise, the Harbor locators below are against `ff2a1ef4` and the mini-swe-agent locators against `v2.4.6` (prefix `src/minisweagent/`). One correction to the dispatch brief: the gateway model file is `crates/hh-embed/src/gateway_model.rs`, not `crates/hh-gateway/src/gateway_model.rs`. The cited line ranges (726–736, 890–898) match the `hh-embed` file.

---

## Part A — mini-swe-agent 2.4.6 as Harbor runs it

### A.1 How Harbor launches it

Harbor's adapter is `src/harbor/agents/installed/mini_swe_agent.py` (1 017 lines, sha256 `5a7b4b61…`). It is an *installed* agent, so mini-swe-agent runs **inside the task container**.

- **Install** (`:755-789`): ensures curl, bash, build tools and git. Installs `uv` if missing, then `uv python install 3.12` and `uv tool install --python 3.12 mini-swe-agent==<version> --with 'litellm[mcp]' --with orjson --with fastapi`. The LiteLLM version is **unpinned**: it is whatever PyPI resolves at install time.
- **Run** (`:848-1017`, decorated `@with_prompt_template`):
  - Harbor's optional prompt template is applied first (`installed/base.py:178-196`, `:1143-1147`). MCP server text may then be appended to the instruction (`:853-861`).
  - The run sets `MSWEA_CONFIGURED=true` and `MSWEA_COST_TRACKING=ignore_errors` (`:880-881`).
  - The final command (`:1006-1017`) is:

```text
mini-swe-agent --yolo --model=<model> --task=<shlex-quoted instruction> \
  --output=/logs/agent/mini-swe-agent.trajectory.json <cli flags> \
  -c mini -c model.model_kwargs.timeout=3600 [-c /tmp/mswea-config/custom.yaml] \
  [-c model.litellm_model_registry=…] [-c model.model_kwargs.extra_headers.<H>=<session>] \
  [-c model.model_kwargs.reasoning_effort=…] [-c model.model_kwargs.max_tokens=N] \
  [-c model.model_kwargs.api_key=…] [-c model.model_kwargs.custom_llm_provider=…] \
  [-c model.model_class=litellm|litellm_response] [-c model.model_kwargs.api_base=<base_url>] \
  --exit-immediately 2>&1 </dev/null | tee /logs/agent/mini-swe-agent.txt
```

  - `<cli flags>` comes from `MiniSweAgentOptions`. Its only CLI-annotated field is `cost_limit`, which defaults to the string `"0"` (`:487-490`). It is rendered as `--cost-limit 0` by `compile_cli_from_options` (`agents/options.py:135-152`).
  - The leading `-c mini` matters (`:999-1004`). mini-swe-agent's `-c` replaces its default config file rather than merging over it (`run/mini.py:33-47`, `:63`), so Harbor names the builtin `mini.yaml` explicitly.
  - `--base-url` on `harbor jobs` (`cli/jobs.py:641-649`) becomes `model.model_kwargs.api_base` (`mini_swe_agent.py:993-997`).
  - A user config, given as `config` or `config_file` kwargs, is written to `/tmp/mswea-config/custom.yaml` and layered after `mini` (`:893-922`). Harbor's example job (`examples/configs/agents/mini-swe-agent-job.yaml`) shows only a commented `agent.step_limit: 20` override.
- **Wall clock**: Harbor wraps the whole `run` in `asyncio.wait_for(…, timeout=timeout_sec)` and raises `AgentTimeoutError` (`trial/trial.py:776-787`).
- **Post-run** (`:801-846`): Harbor reads the native trajectory, sums token usage and converts it to ATIF (`/logs/agent/trajectory.json`) via `convert_mini_swe_agent_to_atif` (`:268-446`).

The CLI (`run/mini.py:55-105`) then does the following:

- It builds the config by recursive merge of the specs plus the CLI values. `--yolo` sets `agent.mode=yolo`. `--exit-immediately` sets `agent.confirm_exit=False`. `--cost-limit 0` sets `agent.cost_limit=0`.
- It builds the model with `get_model`. That defaults to `LitellmModel` (`models/__init__.py:92-113`), and it auto-enables `set_cache_control="default_end"` only when the model name contains anthropic, sonnet, opus or claude (`:55-60`).
- It builds the environment with default type **`local`**: `LocalEnvironment`, i.e. `subprocess` inside the container (`run/mini.py:100`).
- It builds the agent with default type **`interactive`**: `InteractiveAgent` (`run/mini.py:101`; `agents/__init__.py:8-28`).

### A.2 Control loop

`DefaultAgent.run` (`agents/default.py:88-124`; sha256 `e8ef8aa3…`) works as follows:

1. It renders and appends `system` (system_template) and `user` (instance_template) messages. The templates are Jinja2 with `StrictUndefined` (`:66-67`), and the template variables are the merge of:
   - the agent config,
   - the environment vars, which for `LocalEnvironment` are config + `platform.uname()._asdict()` + `os.environ` (`environments/local.py:58-59`),
   - the model config,
   - `n_model_calls`, `model_cost` and `elapsed_seconds`,
   - `task` (`:52-64`).
2. It then loops on `step()` = `execute_actions(query())` until the last message has `role == "exit"`. It saves the trajectory after **every** step (`finally: self.save(...)`, `:120-121`).
3. `query()` (`:130-152`):
   - It raises `LimitsExceeded` if `0 < step_limit <= n_calls` or `0 < cost_limit <= cost`, and `TimeExceeded` if `0 < wall_time_limit_seconds <= elapsed`.
   - Otherwise it calls `model.query(messages)`, adds the cost and appends the assistant message.
   - The limit check happens **before** the call, so `step_limit = N` permits exactly N model calls.
4. `InteractiveAgent.execute_actions` (`agents/interactive.py:124-139`):
   - In yolo mode no confirmation is asked (`:162-166`).
   - It executes each action sequentially via `env.execute`. In a `finally` block it always formats the observation messages for the executed outputs.
   - When `Submitted` is raised mid-batch, the unexecuted remainder is padded with `{"output": "", "returncode": -1, "exception_info": "action was not executed"}` (`models/utils/actions_toolcall.py:88-89`).
   - With `confirm_exit=False` the `Submitted` is re-raised (`interactive.py:144-160`) and becomes the `exit` message.
5. `InteractiveAgent.query` (`:58-94`):
   - When stdin is not a TTY, which is Harbor's case (`</dev/null`), `LimitsExceeded` is re-raised rather than prompting.
   - `TimeExceeded` is always re-raised.

**Termination conditions and exit statuses** (all under Harbor):

| Condition | Mechanism | Exit status | Under Harbor's defaults |
|---|---|---|---|
| Submit marker | `LocalEnvironment._check_finished` (`environments/local.py:45-56`): first line of `output.lstrip()` equals exactly `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` **and** `returncode == 0`; the submission is the remaining lines | `Submitted` | primary path |
| Step limit | `agent.step_limit` (`mini.yaml:101` = `0` → unlimited) | `LimitsExceeded` | **disabled** |
| Cost limit | `agent.cost_limit` (`mini.yaml:102` = `3.`; Harbor passes `--cost-limit 0`) | `LimitsExceeded` | **disabled**; costs also read 0 under `ignore_errors` (`litellm_model.py:108-126`) |
| Wall time | `agent.wall_time_limit_seconds` (default 0) | `TimeExceeded` | disabled; Harbor's `AgentTimeoutError` kills from outside |
| Format errors | `max_consecutive_format_errors` (default 3, `default.py:32-33`); counter reset after any clean step (`:99`) | `RepeatedFormatError` | active |
| Uncaught exception | `handle_uncaught_exception` appends `exit` and **re-raises** (`:74-86`, `:117-119`) | exception class name | e.g. `ContextWindowExceededError` (an abort exception, `litellm_model.py:50-57`) ends the run |
| Global limits | `MSWEA_GLOBAL_COST_LIMIT` / `MSWEA_GLOBAL_CALL_LIMIT` env (`models/__init__.py:13-31`) | `RuntimeError` | unset |

**Format-error handling** (`default.py:100-114`; `litellm_model.py:81-98`):

- On a `FormatError` the assistant reply is **not** appended to the message history. Only the rendered `role: "user"` error message is appended, and the raw response is kept in its `extra.response` for the trajectory.
- The next request therefore carries `…, user(observation or previous format error), user(format error)`, and the bad assistant turn is omitted.
- This is the behaviour an L1 replay must reproduce exactly.

**Transport retries** (`models/utils/retry.py:19-25`): tenacity, `stop_after_attempt(MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT or 10)`, `wait_exponential(multiplier=1, min=4, max=60)`. There is no retry on `UnsupportedParamsError`, `NotFoundError`, `PermissionDeniedError`, `ContextWindowExceededError`, `AuthenticationError` or `KeyboardInterrupt`.

### A.3 Prompts (config `config/mini.yaml`, 151 lines, sha256 `b539a8965f5bf41dd0f32daafa5d2db22581923d13812fa378fd09569b9d6b0c`)

System template (`mini.yaml:2-3`), verbatim. It is a YAML block scalar, so the rendered text ends with a newline:

```text
You are a helpful assistant that can interact with a computer.
```

Instance template (`mini.yaml:4-100`): 97 lines. The task is inserted at the first line, `Please solve this issue: {{task}}`. Excerpts that matter for parity:

```text
    6. Submit your changes and finish your work by issuing the following command: `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`.
       Do not combine it with any other command. <important>After this command, you cannot continue working on this task.</important>
…
    - Directory or environment variable changes are not persistent. Every action is executed in a new subshell.
…
    <system_information>
    {{system}} {{release}} {{version}} {{machine}}
    </system_information>
…
    {%- if system == "Darwin" -%}
```

The `system_information` line renders `platform.uname()` **of the process inside the container**. Containers share the host kernel, so `release` and `version` are the host VM's kernel strings. The first user message therefore varies with the compute host.

Observation template (`mini.yaml:112-128`), verbatim. It is JSON-shaped, with a 10 000-character threshold and a 5 000-character head and tail:

```jinja
{%- if output.output | length < 10000 -%}
{
  "returncode": {{ output.returncode }},
  "output": {{ output.output | tojson }}
  {%- if output.exception_info %}, "exception_info": {{ output.exception_info | tojson }}{% endif %}
}
{%- else -%}
{
  "returncode": {{ output.returncode }},
  "output_head": {{ output.output[:5000] | tojson }},
  "output_tail": {{ output.output[-5000:] | tojson }},
  "elided_chars": {{ output.output | length - 10000 }},
  "warning": "Output too long."
  {%- if output.exception_info %}, "exception_info": {{ output.exception_info | tojson }}{% endif %}
}
{%- endif -%}
```

Format-error template (`mini.yaml:129-149`), verbatim:

```jinja
{% if finish_reason is defined and (finish_reason == "length" or (finish_reason == "tool_calls" and not has_tool_calls)) -%}
Your previous response reached the output token limit (finish_reason={{ finish_reason }}) before you produced a tool call, so it was cut off. Respond more concisely and finish with exactly one bash tool call. If you need to think more, do so briefly.
{%- else -%}
Tool call error:

<error>
{{error}}
</error>

Here is general guidance on how to submit correct toolcalls:

Every response needs to use the 'bash' tool at least once to execute commands.

Call the bash tool with your command as the argument:
- Tool: bash
- Arguments: {"command": "your_command_here"}

If you want to end the task, please issue the following command: `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`
without any other command.
{%- endif %}
```

The `{{error}}` strings come from `actions_toolcall.py:45` ("No tool calls found in the response. Every response MUST include at least one tool call."), `:60` ("Error parsing tool call arguments: …"), `:62` ("Unknown tool '…'.") and `:64` ("Missing 'command' argument in bash tool call."). `mini.yaml:104-110` adds environment variables to every command: `PAGER=cat`, `MANPAGER=cat`, `LESS=-R`, `PIP_PROGRESS_BAR=off`, `TQDM_DISABLE=1`. `mini.yaml:150-151` sets `model_kwargs.drop_params: true`.

No benchmark-specific config is involved. Harbor uses the generic `mini` config. The SWE-bench YAMLs (`config/benchmarks/*.yaml`) are not used.

### A.4 Action interface

The interface is **native function calling**:

- `LitellmModel._query` always sends `tools=[BASH_TOOL]` (`litellm_model.py:64-74`). `BASH_TOOL` (`models/utils/actions_toolcall.py:11-27`) has `name: "bash"`, `description: "Execute a bash command"` and a single required string parameter `command` described as "The bash command to execute".
- `tool_choice` is not set.
- Parsing (`actions_toolcall.py:30-76`) accepts **one or more** tool calls. The first malformed call raises `FormatError` for the whole response. Each action is `{"command", "tool_call_id"}`.
- A text-parsed mode exists (`litellm_textbased` class: regex ``` ```mswea_bash_command\s*\n(.*?)\n``` ```, exactly one action, `models/litellm_textbased_model.py:7-13`), but Harbor does not select it.

Execution (`environments/local.py:24-43`, `:72-92`):

- Each action is a fresh `subprocess.Popen(command, shell=True, stdout=PIPE, stderr=STDOUT, start_new_session=True)`, with environment `os.environ | config.env`.
- The working directory is `config.cwd or os.getcwd()`. Harbor sets no cwd for the run exec (`mini_swe_agent.py:1006-1017`), so it is the exec's default directory, presumably the image's working directory. I infer this; I did not trace the environment backends.
- The timeout is `LocalEnvironmentConfig.timeout = 30` s (`:13-16`), which `mini.yaml` does not override. On timeout the whole process group is SIGKILLed. Partial output is kept, with `returncode -1` and `exception_info = "An error occurred while executing the command: Command '…' timed out after 30 seconds"`. That last string is inferred from `subprocess.TimeoutExpired.__str__`.
- Output is decoded as UTF-8 with `errors="replace"`. No ANSI stripping is done: `PAGER=cat` and `TQDM_DISABLE` are the only mitigations.

### A.5 Observation formatting

`format_toolcall_observation_messages` (`actions_toolcall.py:79-113`) renders `observation_template` with `output` plus all template variables. It emits `{"role": "tool", "tool_call_id": <id>, "content": <rendered>}` per action. Truncation is on Python **characters**, not bytes: below 10 000 characters the full output; otherwise head 5 000 + tail 5 000 and `elided_chars`. Return code and exception info are rendered as JSON fields.

### A.6 Model request shape

- **Library**: `litellm.completion(model=model_name, messages=…, tools=[BASH_TOOL], **model_kwargs)` (`litellm_model.py:64-71`). It is synchronous and non-streaming.
- **Parameters**:
  - From `model_kwargs`: `drop_params: true` (from `mini.yaml`), `timeout: 3600` (Harbor), and optionally `api_base`, `api_key`, `reasoning_effort`, `max_tokens`, `extra_headers`.
  - **No** temperature, top_p, seed or stop are set by either mini-swe-agent or Harbor, so provider defaults apply.
- **Messages**: the full history every turn: `system, user(instance), assistant, tool…, assistant, tool…`, with `user(format error)` interleaved as above.
  - `_prepare_messages_for_api` strips only the `extra` key (`:76-79`) and reorders Anthropic thinking blocks (`models/utils/anthropic_utils.py:11-28`).
  - The assistant message replayed is `response.choices[0].message.model_dump()` (`:99`). It therefore carries whatever fields LiteLLM's `Message` keeps. In the locally readable LiteLLM 1.108.1 those are `role`, `content`, `tool_calls`, `function_call`, `provider_specific_fields`, and `reasoning_content` when present; `audio`, `annotations`, `refusal`, `thinking_blocks` and `reasoning_content` are deleted when None (`litellm/types/utils.py:1451-1540`).
  - No summarization, truncation or windowing of the history exists. The context grows until the provider refuses with `ContextWindowExceededError`, which aborts the run.
- **Caching**: there are no cache hints for a non-Anthropic model name (`models/__init__.py:55-60`; `models/utils/cache_control.py:49-67`).

### A.7 Configuration knobs (phase-4 sweep candidates)

- `agent.step_limit`
- `agent.cost_limit`
- `agent.wall_time_limit_seconds`
- `agent.max_consecutive_format_errors`
- `agent.system_template` / `instance_template` (prompt variants)
- `model.observation_template`: the truncation threshold and head/tail lengths are literal in the template
- `model.format_error_template`
- `model.model_kwargs.*`: temperature, max_tokens, reasoning_effort
- `environment.timeout`, `environment.env`, `environment.cwd`
- `model.model_class` (tool-call vs text-based actions)
- `model.set_cache_control`

All are reachable from Harbor through `--ak 'config={…}'` or `config_file`. There is no context strategy knob.

### A.8 Observability and determinism hooks

- **Native trajectory**: `mini-swe-agent.trajectory.json`, written after every step (`default.py:159-190`, format `mini-swe-agent-1.1`). It contains every message *including* `extra.response` (the full provider response) and the format-error raw responses. The request message list sent at turn *k* can be reconstructed exactly from it: take the messages before the *k*-th assistant message and drop `extra`.
- **In-process fakes**: `DeterministicModel` / `DeterministicToolcallModel` (`models/test_models.py:104-200`; registry key `deterministic`, `models/__init__.py:88`). These replace the whole model class, so they bypass LiteLLM and the wire format. They suit a loop test, not a request-shape test.
- **Replay endpoint**: `api_base` is honoured end to end (Harbor `--base-url` → `model.model_kwargs.api_base`). The process runs inside the container, so the replay server must be reachable from the cell's network namespace.

---

## Part B — Terminus-2 as implemented in Harbor

Terminus-2 is a Harbor-native `BaseAgent` (`agents/terminus_2/terminus_2.py`, 2 191 lines, sha256 `34ab4600…`; `version()` returns `"2.0.0"`, `:579-581`). It runs **in the Harbor host process**. It reaches the container only through `environment.exec` (tmux commands) and calls the model from the host.

### B.1 Control loop

1. **Setup** (`:584-603`, `tmux_session.py:400-413`, `:505-546`):
   - It starts the tmux session:

     ```text
     export TERM=xterm-256color && export SHELL=/bin/bash && tmux new-session [-e K=V…] -x 160 -y 40 -d -s terminus-2 'bash --login' \; pipe-pane -t terminus-2 'cat > /logs/agent/terminus_2.pane'
     ```

   - It sets `history-limit 10000000` and, by default, starts an `asciinema rec` recording followed by `clear`.
   - It runs as `environment.default_user`.
2. **Initial prompt** (`run`, `:1783-1869`):
   - It takes `terminal_state = _limit_output_length(get_incremental_output())`. The first call returns `"Current Terminal Screen:\n" + visible pane`.
   - MCP text and an `<available_skills>` XML block may be appended to the instruction (`:1803-1816`, `:649-700`).
   - It renders `initial_prompt = template.format(instruction=…, terminal_state=…)` (`:1818-1821`).
3. **Loop** `for episode in range(max_episodes)` (`:1468-1754`). `max_episodes` defaults to 1 000 000; it is set by `max_turns`, or the deprecated `max_episodes`/`episodes` (`:509-530`). Each episode:
   - (a) Return if `tmux has-session` fails (`:1470-1472`).
   - (b) If summarization is enabled, check proactive summarization (B.5).
   - (c) `_handle_llm_interaction` → `_query_llm` → `Chat.chat(prompt)`. Parse the reply with the JSON parser and clamp each command's duration with `min(duration, 60)` (`:1371-1411`).
   - (d) If the parse result has an **error**: the next prompt is `Previous response had parsing errors:\nERROR: <error>[\nWARNINGS: <warnings>]\n\nPlease fix these issues and provide a proper JSON response.` (`:1569-1574`). Record a step and `continue`; nothing is executed. The episode still counts, and there is **no consecutive-error cap**.
   - (e) Otherwise execute the commands (B.3) and build the observation (B.4).
   - (f) **Completion handshake** (`:1619-1631`, `:1745-1752`):
     - The first `task_complete: true` sets `_pending_completion`, and the next prompt becomes the confirmation message.
     - A second consecutive `task_complete: true` returns, which ends the agent.
     - Any episode without `task_complete` resets the pending flag.
     - The confirmation message (`:716-725`) is, verbatim: `Current terminal state:\n{terminal_output}\n\nAre you sure you want to mark the task as complete? This will trigger your solution to be graded and you won't be able to make any further corrections. If so, include "task_complete": true in your JSON response again.`
   - (g) Dump the ATIF trajectory after every episode (`:1743`).
4. **Termination**:
   - the double `task_complete` handshake;
   - `max_episodes`;
   - the tmux session dying;
   - Harbor's `AgentTimeoutError` (`trial/trial.py:776-787`);
   - an exception escaping `_query_llm`. That is any non-context, non-output-length error after the retries, or `ContextLengthExceededError` with summarization off.
   - There is no cost limit and no step limit unless `max_turns` is set.

**Retries**:

- `_query_llm` is `@retry(stop_after_attempt(3))` except on `ContextLengthExceededError` (`:1197-1208`).
- Inside it, `LiteLLM.call` is `@retry(stop_after_attempt(3), wait_exponential(multiplier=1, min=4, max=15))` except on context-length, output-length and authentication errors (`llms/lite_llm.py:275-291`).
- One failing turn can therefore issue up to nine requests. (Inference from the nesting; not executed.)

**Output-length handling** (`:1301-1365`):

- On `finish_reason == "length"` (`lite_llm.py:449-458`), the JSON parser has no salvage method (only the XML parser defines `salvage_truncated_response`).
- So Terminus appends `user(prompt)` and `assistant(truncated)` to the history.
- It then re-queries with this message (verbatim): `ERROR!! NONE of the actions you just requested were performed because you exceeded {limit_str}. Your outputs must be less than {limit_str}. Re-issue this request, breaking it into chunks each of which is less than {limit_str}.`, plus any parser warnings.
- `limit_str` is `"<N> tokens"` from LiteLLM model info, or `"the maximum output length"` when that is unknown.

### B.2 Prompts

The main template is `templates/terminus-json-plain.txt` (2 916 bytes, sha256 `89a3dc3a15752b748a99fb907c9251ab31a464e13386583fb48476b078b54a34`). It uses Python `str.format` placeholders `{instruction}` and `{terminal_state}`, with braces doubled. The task is inserted after `Task Description:\n`. Excerpts:

```text
You are an AI assistant tasked with solving command-line tasks in a Linux environment. You will be given a task description and the output from previously executed commands. Your goal is to solve the task by providing batches of shell commands.
…
- You must end every command with a newline (\n) or it will not execute.
- For special key sequences, use tmux-style escape sequences:
  - C-c for Ctrl+C
  - C-d for Ctrl+D
…
It is better to set a smaller duration than a longer duration. It is always possible to wait again if the prior output has not finished, by running {{"keystrokes": "", "duration": 10.0}} on subsequent requests to wait longer. Never wait longer than 60 seconds; prefer to poll to see intermediate result status.
…
Task Description:
{instruction}

Current terminal state:
{terminal_state}
```

The tb 0.2.18 copy (sha256 `a52a860b…`) differs in exactly one line (line 34: "Most bash commands should end with a newline (\n) to cause them to execute"). The XML variant is `templates/terminus-xml-plain.txt` (sha256 `721c0ada…`).

**There is no system message.** The rendered template is the first `user` message (`llms/chat.py:83-123`; `lite_llm.py:317`).

Timeout template `templates/timeout.txt` (365 bytes, sha256 `32bf9aa7…`), verbatim. It is identical in tb 0.2.18:

```text
Previous command:
{command}

The previous command timed out after {timeout_sec} seconds

It is possible that the command is not yet finished executing. If that is the case, then do nothing. It is also possible that you have entered an interactive shell and should continue sending keystrokes as normal.

Here is the current state of the terminal:

{terminal_state}
```

This template is effectively unreachable in Harbor's current code path. Commands are sent with `block=False` (`terminus_2.py:1429-1433`). The non-blocking path only sleeps (`tmux_session.py:773-785`) and never raises `TimeoutError`; only the blocking path does (`:755-768`). I infer this from code; it was not executed.

The summarization prompts are inline f-strings in `terminus_2.py`:

- summary (`:1021-1038`): it begins `You are about to hand off your work to another AI agent.`, and its continuation lines carry 12 literal leading spaces;
- questions (`:1059-1072`);
- answer request (`:1127-1130`);
- handoff (`:1160-1167`);
- short-summary fallback (`:1263`) and ultimate fallback (`:1279-1281`).

### B.3 Action interface

- **Text-parsed JSON. There is no function calling and no `tools` parameter.**
- `TerminusJSONPlainParser` (`terminus_json_plain_parser.py`, sha256 `ba7eeaeb…`):
  - Extracts the **first balanced `{…}` object** with a quote- and escape-aware brace scanner. Text before or after it is a warning (`:165-212`).
  - Requires `analysis`, `plan` and `commands`. `commands` must be an array; `analysis` and `plan` must be strings, or a warning is raised (`:214-249`). It also warns when field order differs from analysis → plan → commands (`:357-398`).
  - `task_complete` accepts a bool or a string ("true"/"1"/"yes") (`:124-126`).
  - For each command, `keystrokes` is a required string. `duration` must be a number; otherwise, or if it is missing, it defaults to 1.0 with a warning. Unknown fields produce warnings. A missing trailing newline before another command produces a warning (`:251-308`).
  - Command errors are downgraded to warnings when `task_complete` is true (`:135-146`).
  - Two auto-fixes are tried on error (`:310-348`): appending missing `}` characters, and a regex search for a nested JSON object. A successful fix adds `AUTO-CORRECTED: … - please fix this in future responses` to the warnings (`:40-62`).
- **Execution** (`terminus_2.py:1413-1443`; `tmux_session.py`, sha256 `d5937cf4…`):
  - Each command's `keystrokes` string is sent as **one** key argument via `tmux send-keys -t terminus-2 -- '<keystrokes>'` (shell-quoted; `:425-440`).
  - Without `-l`, tmux will interpret a whole-argument key name such as `C-c`. Literal text is typed and `\n` presses Enter. This is my inference about tmux semantics, not verified.
  - Keys longer than about 16 000 bytes go through a base64-staged `load-buffer`/`paste-buffer` (`:661-699`).
  - After sending, the agent sleeps `max(0, duration − send time)` (`:773-785`), with duration clamped to 60 s. There is no completion detection.
  - The shell is one persistent interactive `bash --login`, so working directory, environment and background processes persist across turns, unlike mini-swe-agent.

### B.4 Observation formatting

`get_incremental_output` (`tmux_session.py:849-884`) works as follows:

- It captures the whole scrollback with `tmux capture-pane -p -S - -t terminus-2` (`:488-503`). There is no `-e`, so tmux emits text without escape sequences.
- If the stripped previous buffer is a substring, it returns `"New Terminal Output:\n" + current_buffer[idx:]`.
- Otherwise, or when nothing new appeared, it returns `"Current Terminal Screen:\n" + visible pane` (160×40).
- **Quirk**: `_find_new_content` (`:839-847`) sets `idx = pb.rfind("\n")`, an index into the *previous* buffer, not the current one. The "new" slice is only aligned when the previous buffer starts at offset 0. When it is, the slice re-includes the last line of the previous buffer, typically the prompt line. An HH-native re-expression must reproduce this or declare a divergence. (Inference from code; not executed.)

The observation is then:

- `_limit_output_length(…, max_bytes=10000)` (`terminus_2.py:745-781`): a **UTF-8 byte** limit, keeping the first 5 000 and last 5 000 bytes with this marker in between: `\n[... output limited to 10000 bytes; N interior bytes omitted ...]\n`;
- prefixed `Previous response had warnings:\nWARNINGS: …\n\n` when the parser warned (`:1632-1636`);
- or replaced by the completion confirmation (B.1 f).

### B.5 Model request shape, history and summarization

- **Library**: Harbor's `LiteLLM` wrapper → `litellm.acompletion` (`llms/lite_llm.py:293-469`, sha256 `bc39378d…`). It is async and non-streaming; a streaming wrapper raises `NotImplementedError` (`:427-428`).
- **Request kwargs**:
  - `model`, `messages`, `drop_params: True`, `api_base`, `logger_fn` (None in Terminus, because `Chat.chat` passes no `logging_path`), `response_format: None` and `reasoning_effort` (`:662-673`, `:322-327`).
  - `temperature` is sent **only if configured**: the default is `None` (`terminus_2.py:109-112`, `:298-299`; `lite_llm.py:328-329`). tb 0.2.18 defaulted to `temperature=0.7` (`terminal_bench/agents/terminus_2/terminus_2.py:38`), so it differs here.
  - Optional additions: `logprobs` and `extra_body.return_token_ids` (rollout details), Anthropic `thinking` (`max_thinking_tokens`), `extra_body.session_id` plus header `X-Session-ID` when `session_id` is set, and `llm_call_kwargs` (`:331-386`).
  - There is no tools parameter, no stop sequence and no seed.
- **Messages**: `history + [user(prompt)]`. `Chat` appends `user(prompt)` and `assistant(content)` after each call. `reasoning_content` is kept only when `interleaved_thinking=True` (`chat.py:112-122`). The steady-state request is `[user(initial), assistant, user(obs), assistant, …, user(obs_k)]`.
- **Caching**: `add_anthropic_caching` applies only to anthropic or claude model names (`llms/utils.py:14-38`).
- **Context limit**: `get_model_context_limit` uses `litellm.get_model_info` → `max_input_tokens`, falling back to `max_tokens` and then **1 000 000** (`lite_llm.py:168-198`). The token count is `litellm.utils.token_counter(model=model_name, messages=…)` (`terminus_2.py:739-743`).
- **Proactive summarization** (`:1171-1195`):
  - Before each turn, if `context_limit − tokens < proactive_summarization_threshold` (default 8 000; 0 disables it), run `_summarize`.
  - Its handoff prompt replaces the next prompt.
  - Errors are logged and swallowed.
- **Reactive summarization** (`:1228-1299`): on `ContextLengthExceededError`, when `enable_summarize` is on (the default):
  - It first unwinds history by dropping the last two messages until 4 000 free tokens remain (`:783-807`).
  - It then tries, in order:
    - (1) the full `_summarize`;
    - (2) a short single call, `Briefly continue this task: …\n\nCurrent state: <last 1000 chars>\n\nNext steps (2-3 sentences):`;
    - (3) a no-LLM fallback.
  - It then chats the resulting prompt. If that also fails, it substitutes `LLMResponse(content="Technical difficulties. Please continue with the task.")`.
  - Context-overflow detection matches LiteLLM's `ContextWindowExceededError` or seven provider phrases (`lite_llm.py:687-707`).
- **`_summarize`** (`:955-1169`) makes three model calls:
  - (S1) unwound history + `summary_prompt`;
  - (S2) empty history + `question_prompt` (original task, S1 text, visible screen);
  - (S3) unwound history + `[user(summary_prompt), assistant(S1)]` + `answer_request_prompt`.
  - Afterwards the chat is **replaced** by `[messages[0], user(question_prompt), assistant(S2)]`, and the next turn's prompt is the handoff `Here are the answers the other agent provided.\n\n<S3>\n\nContinue working on this task from where the previous agent left off. You can no longer ask questions. Please follow the spec to interact with the terminal.`
  - tb 0.2.18 issued S1 and S3 through `chat.chat` and so appended them to the history; it also hard-coded the 8 000 threshold (`terminal_bench/agents/terminus_2/terminus_2.py:230-317`). Harbor's version is semantically different.

### B.6 Configuration knobs

From `Terminus2Options` (`terminus_2.py:95-191`):

- `max_turns`
- `parser_name` (json|xml, which changes the template and parser)
- `temperature`
- `reasoning_effort`
- `enable_summarize`
- `proactive_summarization_threshold`
- `model_info` (`max_input_tokens` drives both summarization triggers)
- `max_thinking_tokens`
- `interleaved_thinking`
- `tmux_pane_width` / `height`
- `record_terminal_session`
- `llm_kwargs` / `llm_call_kwargs`
- `use_responses_api`
- `trajectory_config.{raw_content, linear_history}`

Hard-coded, so not knobs today:

- the 10 000-byte observation limit;
- the 60 s duration clamp;
- the 4 000-token unwind target;
- the summarization prompt texts;
- the completion handshake.

### B.7 Observability and determinism hooks

- The ATIF trajectory `/logs/agent/trajectory.json` is dumped after every episode. Its step 1 is the full initial prompt, and each step records the observation (the next prompt). Subagent trajectories `…summarization-<n>-{summary,questions,answers}` are separate files (`:876-953`, `:2115-2191`). `store_all_messages=True` copies the chat into trial metadata (`:1865-1866`). tb 0.2.18 additionally wrote `episode-N/{debug.json,prompt.txt,response.txt}` (`terminal_bench/agents/terminus_2/terminus_2.py:143-148`); Harbor's Terminus does not.
- **Harbor already drives Terminus-2 against a deterministic fake OpenAI endpoint in its own integration tests.** An aiohttp server on `/v1/chat/completions` returns scripted completions (`tests/integration/test_deterministic_terminus_2_invalid_json.py:33-192`). The agent is configured with `kwargs={"api_base": "http://localhost:<port>/v1", …}` (`:210-233`), and golden ATIF trajectories are compared after `normalize_trajectory` (`tests/integration/test_utils.py:160-190`).
- The summarization test forces the overflow path with a fake `context_length_exceeded` error and a monkeypatched `get_model_info` returning `max_input_tokens: 7000`. It sets `proactive_summarization_threshold: 0` (`test_deterministic_terminus_2_context_summarization.py:291-301`, `:392-416`, `:451`).

---

## Side-by-side anatomy

| Dimension | mini-swe-agent 2.4.6 (Harbor `mini-swe-agent`) | Terminus-2 (Harbor `terminus-2`, v2.0.0) |
|---|---|---|
| Where it runs | inside the task container (uv tool, Python 3.12) | Harbor host process; container only via `environment.exec` |
| Core logic size | ~1 120 lines: `default.py` 190, `interactive.py` 209, `litellm_model.py` 164, `actions_toolcall.py` 113, `local.py` 92, `retry.py` 25, `cache_control.py` 67, `run/mini.py` 109, `mini.yaml` 151; plus Harbor adapter 1 017 | ~4 440 lines: `terminus_2.py` 2 191 (about half is trajectory bookkeeping), JSON parser 398, `tmux_session.py` 884, `chat.py` 155, `lite_llm.py` 810 |
| Runtime deps | litellm (unpinned by Harbor), jinja2, pydantic, tenacity, typer, rich, … (`pyproject.toml:33-48`) | litellm, tenacity (Harbor's); tmux and asciinema in the container |
| Messages | `system` + `user(instance)` + (`assistant`, `tool`×n)* | no system; `user(template)` + (`assistant`, `user(obs)`)* |
| Prompt engine | Jinja2 `StrictUndefined`; vars include `uname` and `os.environ` | `str.format`; vars `instruction`, `terminal_state` |
| Action channel | native tool call `bash(command)`; ≥1 call per turn | text JSON `{analysis, plan, commands[{keystrokes,duration}], task_complete}` |
| Execution | fresh `bash` subprocess per action, stdout+stderr merged, 30 s timeout, process-group kill | keystrokes into persistent tmux `bash --login`; sleep `min(duration,60)` s; no completion detection |
| Observation | Jinja JSON object `{returncode, output}`; >10 000 chars → head/tail 5 000 + `elided_chars` | `New Terminal Output:`/`Current Terminal Screen:` + text; 10 000-**byte** head/tail cap; warnings prefix |
| Submit | first output line `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` and rc 0 | `task_complete: true` twice in a row (confirmation prompt between) |
| Format error | user message rendered from `format_error_template`; bad assistant turn **omitted**; exit after 3 consecutive | user message `Previous response had parsing errors:…`; bad assistant turn **kept**; auto-fixes; no cap |
| Limits (Harbor defaults) | none in-agent (step 0, cost 0); Harbor wall timeout | `max_turns` 10⁶; Harbor wall timeout |
| Sampling params sent | none (provider default) | none unless `temperature` configured (tb 0.2.18: 0.7) |
| Context management | none; overflow aborts | proactive (threshold 8 000 free tokens) and reactive 3-call summarization + handoff; unwind |
| Transport retries | 10 attempts, exp 4–60 s | 3×3 nested, exp 4–15 s |
| Native record | `mini-swe-agent.trajectory.json` (full messages + raw responses) → ATIF | ATIF `trajectory.json` (+ subagent files), pane log, asciinema |
| Base-URL redirect | `model.model_kwargs.api_base` (Harbor `--base-url`) | `api_base` kwarg / `--base-url`; used by Harbor's own deterministic tests |
| Natural sweep knobs | step_limit, cost_limit, observation template (truncation), prompt templates, cmd timeout, max_tokens, temperature | max_turns, summarization on/off + threshold, `model_info.max_input_tokens`, parser json/xml, temperature, interleaved_thinking, pane size |

---

## L1 feasibility: driving each original against a replay endpoint

**Mini-swe-agent: yes.** Set `--base-url` to a replay server; Harbor emits `-c model.model_kwargs.api_base=…`. The server records each request body and returns the recorded response for turn *k*. Constraints:

- The requests originate **inside the cell**, so the replay server must be on the cell's egress allowlist.
- The request body depends on several moving parts:
  - the installed LiteLLM version, which Harbor leaves unpinned (F7);
  - the host kernel's `uname` (F3);
  - environment-dependent observations. Commands really execute, so a recorded response replayed against a different container state yields different `tool` contents.

  Full L1 determinism therefore needs a pinned image, a pinned LiteLLM, and either a deterministic environment or an L1 scope that compares request *construction* given recorded observations.
- An alternative that removes environment nondeterminism: build the expected request sequence from the native trajectory (messages minus `extra`), then compare HH's request at each turn with HH fed the same recorded observations.

**Terminus-2: yes, and Harbor already does it.** Its integration tests drive Terminus-2 against an aiohttp fake `/v1/chat/completions` with scripted completions and compare golden ATIF trajectories. Requests originate from the host, so no cell egress is needed. Constraints:

- tmux timing: durations are real sleeps.
- The incremental-output heuristic.
- The pane text depends on shell prompt, user and hostname.
- Summarization triggers depend on LiteLLM model info and `token_counter`. For `glm-5.3-flash` under an `openai/` prefix the context limit almost certainly falls back to 1 000 000 (F10), so proactive summarization will never fire unless `model_info` is supplied.

For both harnesses an L1 harness can also be built at the HTTP layer as a recording proxy in front of the real model. That only captures; it does not make the run deterministic.

---

## HH cannot express today

This is a gap inventory against main 75883daa. It is not a design; E2 owns fixes.

| # | Original behaviour | Needed by | HH today (evidence) |
|---|---|---|---|
| G1 | A `system` message (or any templated preamble) before the task | mini | The live transcript starts with `user(instruction)` (or a caller-directed invoke note); there is no system role (`crates/hh-embed/src/gateway_model.rs:720-736`). There is no `system_template`/prompt slot in the request path. |
| G2 | Instance prompt = a template with `{{task}}`, `uname` and other variables | both | The instruction is a raw string passed to `GatewayModel::new` (`gateway_model.rs:576-620`; `service.rs:1719-1724`). There is no templating engine or variable set. |
| G3 | Tool description and parameter descriptions on the wire (`bash`: "Execute a bash command"; `command`: "The bash command to execute") | mini | The description is the surface's `semantic_id` (`gateway_model.rs:890-898`). Parameters render only `type`/`enum`/`required` from `ParamSpec`, with no descriptions (`:864-888`). Host-executor surfaces are built with **empty** params (`crates/hh-embed/src/open.rs:3529-3540`). HIR's `ToolSurface.description_template` and `ToolCapability.input_schema` exist (`crates/hh-hir/src/records.rs:363-397`, `:1069-1095`) but do not reach the wire. |
| G4 | Exactly one tool (`bash`), no `tool_choice` | mini | `driver_surfaces` always prepends `hh.submit` (`open.rs:3529-3531`, `:3136-3143`). `chat_request_body` always adds `tool_choice: "auto"` when tools are present (`crates/hh-gateway/src/openai.rs:62-65`). |
| G5 | Non-streaming request without `stream_options` | both | `stream: true` and `stream_options.include_usage` are always set for chat-completions (`gateway_model.rs:911-921`; `openai.rs:66`). This is wire-shape only; it could be a canonicalization rule rather than a gap. |
| G6 | The assistant's prose content replayed in history alongside its tool calls (and `reasoning_content` per LiteLLM's message dump) | mini | The transcript replays `model.call.completed` as `assistant_tool_call_msg(None, calls)`, so the content is dropped (`gateway_model.rs:636-645`, `:765-799`). |
| G7 | Text-only assistant turns (no tool calls) kept in history | Terminus | Completed calls with no tool calls are skipped from the transcript (`if calls.is_empty() { continue; }`, `gateway_model.rs:783-785`). There is no text-action parser path. |
| G8 | Tool result = a rendered string from an observation template (JSON-shaped, head/tail on characters) | mini | The tool result content is the recorded observation payload JSON (`gateway_model.rs:800-813`). `OutputPolicy.model_view ∈ {full, tail, summary}` and a 1 MiB retention cap (`crates/hh-env/src/capture.rs:366-390`) do not express a head+tail split with an `elided_chars` field. |
| G9 | Format-error message from a template, the bad assistant turn omitted, 3-strike exit | mini | HH has `max_consecutive_format_errors = 3` (`crates/hh-control/src/strategy.rs:172-174`, `:241-253`). Its rendering is the fixed `kernel_note: the prior output was declined by the kernel ({kind}); correct it` (`gateway_model.rs:829-842`). |
| G10 | Submit by marker on the first output line with rc 0 | mini | `StopRuleParam::Submit` is *documented* as marker-on-observed with rc 0 (`strategy.rs:191-203`). The live embed gate detects submission only via the `hh.submit` surface (`crates/hh-embed/src/runtime.rs:415-424`). A search of `crates/hh-control/src` for `COMPLETE_TASK` found nothing, and I found no configurable marker string. |
| G11 | Strategy parameters configurable per HIR document (format-error cap, stop rule, step limit) | both | `StrategyParams::default()` is used for every variant except `plan_execute` tweaks (`open.rs:2578-2583`; a second default at `:2728`). Turn and model-call ceilings exist as `DriverConfig.budget_ceiling` (`crates/hh-control/src/driver.rs:832-837`); I did not verify whether HIR `Budget` reaches them. |
| G12 | Sampling and request params per harness (temperature, max_tokens, timeout; Terminus's optional temperature) | both | Params come from the model lane's `provider_params` (`gateway_model.rs:911`), not from an HIR slot. I did not check whether a lane can omit or set each one. |
| G13 | Persistent interactive terminal (tmux): keystroke sends, special keys, timed waits, pane capture, incremental-diff observation | Terminus | No tmux or pty primitive exists in `crates/hh-env/src` (grep for `tmux`, `send_keys`, `capture_pane`: none). Execution is per-call `ToolExecutor` dispatch with a deadline ladder (`crates/hh-embed/src/runtime.rs:443-468`). |
| G14 | Text-parsed JSON action protocol with auto-fix and warning feedback | Terminus | The only action channel is the gateway's parsed tool calls (`ParsedCall`, `crates/hh-control/src/output.rs:136-150`). There is no free-text parser with recovery. |
| G15 | Two-step completion confirmation handshake | Terminus | Completion is a `stop{completed}` proposal or `hh.submit`; `max_continue_nudges` converts a refused stop (`strategy.rs:169-171`). I found no "ask to confirm, require a repeat" protocol. |
| G16 | Three-call summarize → questions → answers handoff that *replaces* history with fixed prompts; proactive/reactive triggers; unwind-by-pairs | Terminus | HH has a compaction port and a `compact_model_owned` family (`crates/hh-control/src/driver.rs:464`, `:4180`; `crates/hh-context/src/compact_family.rs:1029`). I did not establish whether it can be parameterized to these exact calls and prompts. Treat it as unverified, not as present. |
| G17 | `subprocess` exec semantics: fresh shell per action, merged stdout/stderr, 30 s timeout with partial output, `PAGER=cat` etc. env, cwd default | mini | The R5.8 exec surfaces route through `Dispatcher` to an executor (`service.rs:1700-1702`; `runtime.rs:443-468`). I did not verify the podman-cell executor's stream merge, timeout-partial-output and env-injection behaviour. E2/B2 should confirm. |

---

## Findings

### F1 — Harbor runs mini-swe-agent with its generic `mini.yaml` and no in-agent limits
Claim: Under Harbor, mini-swe-agent 2.4.6 runs as `InteractiveAgent` in yolo mode with `LocalEnvironment` inside the container and the builtin `mini.yaml`. Its step limit is 0, its cost limit is forced to 0, and its wall limit is 0, so only the submit marker, three consecutive format errors, an uncaught exception or Harbor's agent timeout end a run.
Status: VERIFIED
Evidence kind: source inspection
Evidence: harbor@ff2a1ef4 `src/harbor/agents/installed/mini_swe_agent.py:487-490,848-1017`, `agents/options.py:135-152`, `trial/trial.py:776-787`; mini-swe-agent@v2.4.6 `run/mini.py:55-105`, `agents/interactive.py:58-94,124-160`, `agents/default.py:88-152`, `config/mini.yaml:101-103`.
Retrieved: 2026-11-06T22:56Z
Implication for the spec: The HH-native mini-swe-agent must, by default, have no step or cost cap and stop on marker, format-error streak or external wall clock. A phase-4 step-limit sweep is a deliberate departure from the Harbor baseline and must be labelled as such.
Outline delta: EXTENDS OL-05 — fixes the reference configuration ("as Harbor runs it"), not the upstream package's defaults.
Limitations: The HF board does not say which Harbor version or config it used. Its "2.4.6" pin may have been run with overrides that I cannot see.

### F2 — mini-swe-agent's request is a function-calling chat with one `bash` tool and verbatim templates
Claim: Every request is `litellm.completion(model, messages=[system, user(instance), (assistant, tool…)*], tools=[BASH_TOOL], drop_params=True, timeout=3600, …)`. No temperature, top_p, seed, stop or tool_choice is set, and observations are rendered by the quoted Jinja JSON template.
Status: VERIFIED
Evidence kind: source inspection
Evidence: mini-swe-agent@v2.4.6 `models/litellm_model.py:64-106`, `models/utils/actions_toolcall.py:11-113`, `config/mini.yaml:1-151` (sha256 b539a896…); harbor@ff2a1ef4 `mini_swe_agent.py:890,958-970`.
Retrieved: 2026-11-06T22:55Z
Implication for the spec: L1 must compare `messages`, `tools`, and absent-vs-present params. The canonicalization must state how it treats `drop_params` effects and provider-default sampling.
Outline delta: EXTENDS OL-06 — defines the L1 comparison surface for harness 1.
Limitations: The final wire body after LiteLLM's OpenAI or Anthropic transform was not observed.

### F3 — mini-swe-agent's first user message depends on the host kernel
Claim: `instance_template` renders `{{system}} {{release}} {{version}} {{machine}}` from `platform.uname()` inside the container, so the bytes of request 1 change with the compute host's kernel.
Status: VERIFIED
Evidence kind: source inspection
Evidence: mini-swe-agent@v2.4.6 `config/mini.yaml:51-53`, `environments/local.py:58-59`, `agents/default.py:52-64`.
Retrieved: 2026-11-06T22:55Z
Implication for the spec: L1 canonicalization must either pin the uname values (render the same values in the HH cell) or mask that line. Parity runs on different VMs are not byte-comparable otherwise.
Outline delta: EXTENDS OL-06.
Limitations: That containers share the host kernel is standard Linux behaviour, not inspected here.

### F4 — mini-swe-agent omits the faulty assistant turn on format errors
Claim: On a `FormatError` only the rendered `user` error message enters the history. The assistant reply is persisted in the trajectory's `extra.response` but not re-sent, and the run exits after three consecutive errors.
Status: VERIFIED
Evidence kind: source inspection
Evidence: mini-swe-agent@v2.4.6 `agents/default.py:96-114`, `models/litellm_model.py:87-98`, `models/utils/actions_toolcall.py:40-74`.
Retrieved: 2026-11-06T22:55Z
Implication for the spec: The HIR mapping needs an "error turn excluded from transcript" rule, which HH's kernel_note path does not express (G9). L1 fixtures should include a malformed-tool-call turn.
Outline delta: EXTENDS OL-05.
Limitations: none.

### F5 — Terminus-2 is a text-JSON, tmux-keystroke harness with no system message and a two-step completion
Claim: Terminus-2 sends `[user(template), (assistant, user(obs))*]` with no tools. It parses the first balanced JSON object for `{analysis, plan, commands, task_complete}`, types keystrokes into one persistent tmux `bash --login` with sleep-based waits capped at 60 s, and finishes only after `task_complete: true` on two consecutive turns.
Status: VERIFIED
Evidence kind: source inspection
Evidence: harbor@ff2a1ef4 `agents/terminus_2/terminus_2.py:1371-1443,1445-1754,1783-1869`; `terminus_json_plain_parser.py:29-308`; `tmux_session.py:400-413,773-828`; `llms/chat.py:83-123`; template sha256 89a3dc3a….
Retrieved: 2026-11-06T22:57Z
Implication for the spec: Terminus-2 needs a terminal-session primitive and a text-action protocol in HH (G13, G14, G15). It is materially harder than mini-swe-agent, which supports the A-8 staging.
Outline delta: CONFIRMS OL-05 — staging mini-swe-agent first is warranted.
Limitations: tmux key-name interpretation was inferred, not tested.

### F6 — Terminus-2's history-replacing summarization
Claim: When fewer than 8 000 tokens are free (proactive) or on a context-length error (reactive, after unwinding pairs to 4 000 free), Terminus-2 makes three summarization calls (summary, questions on empty history, answers). It replaces the history with `[first user msg, user(question_prompt), assistant(questions)]`, and the next prompt is the answers handoff. Short and no-LLM fallbacks back this up.
Status: VERIFIED
Evidence kind: source inspection
Evidence: harbor@ff2a1ef4 `terminus_2.py:783-807,955-1169,1171-1195,1228-1299`; `llms/lite_llm.py:168-198,648-707`.
Retrieved: 2026-11-06T22:57Z
Implication for the spec: The `compaction_strategy` slot must express a model-driven multi-call handoff with fixed prompts and a free-token trigger computed with a named tokenizer. L1 must cover summarization requests as separate request streams.
Outline delta: EXTENDS OL-05; EXTENDS OL-10 — trace-level parity must cover subagent calls as well as main-loop turns.
Limitations: HH's existing compaction family (`compact_model_owned`) was not assessed for fit.

### F7 — The LiteLLM version under each harness is not pinned by Harbor
Claim: Harbor installs mini-swe-agent with `--with 'litellm[mcp]'` and no version, and mini-swe-agent requires only `litellm >= 1.75.5, != 1.82.7, != 1.82.8`. The LiteLLM that serializes mini-swe-agent's requests is therefore whatever resolves at install time. Terminus-2 uses the host Harbor environment's LiteLLM.
Status: VERIFIED
Evidence kind: source inspection
Evidence: harbor@ff2a1ef4 `mini_swe_agent.py:779-787`; mini-swe-agent@v2.4.6 `pyproject.toml:33-48`; `llms/litellm_config.py:15` comment "Verified against LiteLLM 1.105.0".
Retrieved: 2026-11-06T22:56Z
Implication for the spec: The reference participant's identity must include the resolved LiteLLM version, recorded from the cell (`uv tool list` / lockfile). L1 recordings are valid only for that version.
Outline delta: EXTENDS OL-06 — "same participant" includes the transport library.
Limitations: I did not inspect Harbor's own `uv.lock` LiteLLM pin.

### F8 — mini-swe-agent re-sends LiteLLM's full assistant message dump, Terminus-2 only role and content
Claim: mini-swe-agent replays `choices[0].message.model_dump()` minus `extra`, which includes `reasoning_content` when the provider returns it, `provider_specific_fields` and `function_call`. Terminus-2 replays `{role, content}` only, unless `interleaved_thinking`.
Status: PARTIALLY VERIFIED
Evidence kind: source inspection + inference
Evidence: mini-swe-agent@v2.4.6 `models/litellm_model.py:76-79,99`; harbor@ff2a1ef4 `llms/chat.py:112-122`; local litellm 1.108.1 `types/utils.py:1451-1540`. That is not the installed version, and whether LiteLLM's outbound transform forwards these fields was not checked.
Retrieved: 2026-11-06T22:59Z
Implication for the spec: Whether glm-5.3-flash's reasoning text re-enters mini-swe-agent's context is an empirical L1 question. The HH-native mapping must decide, and record, which assistant fields are replayed (G6).
Outline delta: EXTENDS OL-06.
Limitations: Requires one captured request through the actual proxy, which needs a C1/D2-authorized pilot.

### F9 — Both originals can be driven against a replay endpoint
Claim: Both harnesses honour a base URL end to end. mini-swe-agent uses Harbor `--base-url` → `model.model_kwargs.api_base`, and its requests originate inside the container. Terminus-2 uses the `api_base` kwarg, and its requests originate from the host. Harbor's own integration tests already run Terminus-2 against a scripted aiohttp `/v1/chat/completions` and compare golden trajectories.
Status: VERIFIED (mechanism); UNVERIFIED (end-to-end replay of a full TB 2.1 task)
Evidence kind: source inspection
Evidence: harbor@ff2a1ef4 `cli/jobs.py:641-649`; `mini_swe_agent.py:993-997`; `terminus_2.py:485-487`; `tests/integration/test_deterministic_terminus_2_invalid_json.py:33-233`; `tests/integration/test_deterministic_terminus_2_context_summarization.py:291-301,392-451`; `tests/integration/test_utils.py:160-190`.
Retrieved: 2026-11-06T22:58Z
Implication for the spec: L1 is feasible for both without modifying the originals. The recorded-response server is a $0 component. For mini-swe-agent it must be reachable from the cell under an egress profile.
Outline delta: EXTENDS OL-06 — L1 has an upstream precedent to reuse.
Limitations: Real command execution makes observations environment-dependent (F11).

### F10 — For `glm-5.3-flash`, Terminus-2's proactive summarization effectively never fires by default
Claim: LiteLLM's bundled model map has no `glm-5.3-flash`, `openai/glm-5.3-flash`, `ollama/…` or `anthropic/…` key. `get_model_context_limit` then falls back to 1 000 000 tokens, so the free-token threshold of 8 000 is reached only near 992 000 tokens. Only provider-reported overflow (reactive) would summarize, unless `model_info.max_input_tokens` is supplied.
Status: PARTIALLY VERIFIED
Evidence kind: source inspection + inference
Evidence: harbor@ff2a1ef4 `llms/lite_llm.py:168-198`; local litellm 1.108.1 `model_prices_and_context_window_backup.json`: the five proxy-style keys are absent; only provider-prefixed keys such as `zai/glm-5.3-flash` exist (checked with a read-only `python3 -I` lookup).
Retrieved: 2026-11-06T22:59Z
Implication for the spec: A summarization-threshold sweep (D3) is a no-op unless `model_info` is set. The spec must fix `model_info` for the reference and record it in the participant identity.
Outline delta: EXTENDS OL-10.
Limitations: The installed LiteLLM version and its `get_model_info` name-normalization were not inspected; C1 should confirm the served context window.

### F11 — The two harnesses' observations depend on the environment in different ways
Claim: mini-swe-agent observations are command stdout+stderr with return code, deterministic given container state. Terminus-2 observations are tmux pane text that depends on timing (sleep durations), prompt and hostname strings, and a substring-diff heuristic that, by inspection, re-includes the previous buffer's last line.
Status: VERIFIED (code); UNVERIFIED (runtime effect of the `_find_new_content` index quirk)
Evidence kind: source inspection + inference
Evidence: mini-swe-agent@v2.4.6 `environments/local.py:24-43,72-92`; harbor@ff2a1ef4 `tmux_session.py:488-503,839-884`; `terminus_2.py:745-781`.
Retrieved: 2026-11-06T22:58Z
Implication for the spec: L1 for Terminus-2 should replay recorded observations (pane captures) rather than re-execute. L2/L3 parity must tolerate timing-induced pane differences.
Outline delta: EXTENDS OL-10 — supports the layered design: request-level conformance on recorded observations, verdicts end to end.
Limitations: The quirk's practical frequency is unknown.

### F12 — HH today cannot express the mini-swe-agent request shape
Claim: Four gaps are wire-visible in mini-swe-agent's very first request and break L1 by construction. They are G1 (no system message), G3 (tool description = semantic id, host-exec params empty), G4 (`hh.submit` always present, `tool_choice: auto`) and G6 (assistant prose dropped from replay). G2, G5 and G8–G12 are further gaps.
Status: VERIFIED
Evidence kind: source inspection
Evidence: HH main 75883daa `crates/hh-embed/src/gateway_model.rs:636-645,720-736,765-813,853-898,904-921`; `crates/hh-embed/src/open.rs:3136-3143,3529-3540,2578-2582`; `crates/hh-gateway/src/openai.rs:34-68`; `crates/hh-embed/src/runtime.rs:415-424`; `crates/hh-control/src/strategy.rs:166-253`; `crates/hh-hir/src/records.rs:363-397,1069-1095`.
Retrieved: 2026-11-06T23:00Z
Implication for the spec: E2 must close G1–G4 and G6 before any L1 attempt on harness 1. Most of these are request-path plumbing of fields HIR already declares (`ToolSurface.description_template`, `ToolCapability.input_schema`), not new HIR kinds.
Outline delta: EXTENDS OL-05.
Limitations: G11, G12, G16 and G17 were not traced to completion; each is marked unverified in the table.

### F13 — Harbor v0.25.0 and HEAD are behaviourally equivalent for both agents
Claim: Between Harbor `v0.25.0` (fdba28fc) and HEAD (ff2a1ef4) the agent-relevant changes are a tmux paste chunk length and the LiteLLM install extra. Prompts, parsers, loops and LLM wrappers are unchanged. tb 0.2.18's versions differ materially: Terminus temperature 0.7, template line 34, chat-appended summarization, a fixed 8 000 threshold, history-limit 50 000, and mini run as `mini -m … -y --exit-immediately` with a pip install.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `git diff --stat v0.25.0 HEAD` on the agent paths; tb 0.2.18 `agents/terminus_2/terminus_2.py:38,143-148,230-317`, `terminal/tmux_session.py:97-98`, `agents/installed_agents/mini_swe_agent/mini_swe_agent.py:49-60`, `mini-swe-setup.sh.j2`.
Retrieved: 2026-11-06T23:00Z
Implication for the spec: Pin Harbor by tag `v0.25.0` or commit, and never use the `tb` 0.2.18 Terminus as the parity reference.
Outline delta: EXTENDS OL-06.
Limitations: I did not review the `installed/base.py` diff (78 lines); it is not on a quoted path.

### F14 — Size and representability
Claim: mini-swe-agent's behaviour fits in about 1 100 lines and maps onto HH's existing react-style loop once G1–G12 are closed. Terminus-2's behaviour (about 4 400 lines, roughly half trajectory bookkeeping) additionally needs a terminal-session environment primitive, a text-action parser with recovery, a completion handshake and a multi-call compaction strategy (G13–G16).
Status: VERIFIED (line counts); UNVERIFIED (representability judgement is inference)
Evidence kind: source inspection + inference
Evidence: `wc -l` on the files listed in the side-by-side table; the HH gap table above.
Retrieved: 2026-11-06T23:00Z
Implication for the spec: Budget E2 and its ticket sizing on the asymmetry. Terminus-2 parity is a separate, larger tranche.
Outline delta: CONFIRMS OL-05.
Limitations: The line counts include comments and docstrings.

---

## Answer and completion assessment

E1 asked for the exact anatomy of both reference harnesses. It is settled at source level:

- loop and termination (A.2, B.1);
- prompts, verbatim or by path, hash and excerpt (A.3, B.2);
- action interface (A.4, B.3);
- observation rendering and truncation (A.5, B.4);
- request shape and history/summarization (A.6, B.5);
- knobs (A.7, B.6);
- observability hooks (A.8, B.7);
- an HH gap inventory (G1–G17).

**L1 feasibility: yes for both.** Each original can be pointed at a replay endpoint with no code change. Terminus-2 already is, in Harbor's own tests. Two things are not settled:

- The exact on-the-wire bodies after LiteLLM's transform, which depend on an unpinned LiteLLM (F7, F8).
- Whether HH's compaction family and exec executor can match G16 and G17.

These are acceptable uncertainty for this row. They are resolved by one $0 recording against a local replay server, which is D2/E2 design work, and by C1 for model-side facts. There is no technical blocker to the Phase-3 plan. The main design risk is Terminus-2's terminal primitive (G13).

## Open questions

1. **Reference configuration for "mini-swe-agent as the HF board ran it".** Owner: coordinator → operator. Affected: D2, E2, D1. The board pins 2.4.6 but discloses neither Harbor version nor config overrides. Decision needed: adopt Harbor's defaults as the reference (recommended; F1), with any overrides recorded as declared deviations. **Operator decision: yes**, if the HF numbers are to be cited as a comparison anchor (Q-X12 already says no formal replication).
2. **LiteLLM pin.** Owner: C4/D2. Affected: C4, D2. Pin by recording the resolved version in the cell image versus forcing `--with 'litellm[mcp]==X'`. Forcing a version deviates from "as Harbor runs it". Operator decision: no, unless the two options diverge.
3. **Terminus-2 `model_info`.** Owner: C1/D3. Affected: D3, E2. Supply `max_input_tokens` for glm-5.3-flash, which makes summarization reachable and the threshold a real sweep knob, or keep Harbor's default fallback. Operator decision: no; a methods choice.
4. **The mini-swe-agent uname line.** Owner: D2. Affected: D2. Canonicalize (mask) versus reproduce (HH cell renders the same uname). Operator decision: no.
5. **Assistant `reasoning_content` replay (F8).** Owner: D2/E2. Affected: D2, E2. This is determined empirically on the first $0 local replay capture, using a recorded glm-5.3-flash response, which is itself a spend-gated artifact from C1. Operator decision: no.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale / findings | OL | Acceptance |
|---|---|---|---|---|
| REQ-XE-001 | The Phase-3 mini-swe-agent reference participant SHALL be mini-swe-agent 2.4.6 as launched by Harbor's `mini-swe-agent` adapter at a pinned Harbor tag or commit. Its configuration is builtin `mini.yaml` plus Harbor's flags (`--cost-limit 0`, `--yolo`, `--exit-immediately`, `model_kwargs.timeout=3600`); every override is recorded as a declared deviation. | F1, F13 | OL-05, OL-06 | Participant record lists the Harbor commit, the mini version, the full `-c` spec list and the sha256 of `mini.yaml` (b539a896…). |
| REQ-XE-002 | The Terminus-2 reference SHALL be Harbor's `terminus-2` (version "2.0.0") at the same pinned Harbor commit, not `terminal-bench` 0.2.18's implementation. | F13 | OL-05 | Participant record names the Harbor commit and the template sha256s (89a3dc3a…, 32bf9aa7…). |
| REQ-XE-003 | A reference participant's identity SHALL include the resolved LiteLLM version used by the harness process (in-cell for mini-swe-agent, host for Terminus-2). | F7 | OL-06 | Slot record carries `litellm_version` read from the installed environment; L1 recordings are keyed by it. |
| REQ-XE-004 | The L1 conformance harness SHALL drive each original unmodified against a recorded-response endpoint via its base-URL setting. For mini-swe-agent the endpoint SHALL be reachable from the cell under a declared egress profile. | F9 | OL-06 | One $0 run per harness records ≥1 request body per turn from the original into a committed fixture. |
| REQ-XE-005 | L1 canonicalization SHALL declare its treatment of: the `uname` line (F3); `stream`, `stream_options` and `tool_choice` (G4, G5); absent vs explicit sampling params (F2); assistant `reasoning_content`, `provider_specific_fields` and `function_call` (F8); session-id headers and `extra_body.session_id`; and tool-call id spelling. | F2, F3, F8, F12 | OL-06, OL-10 | The canonicalization spec enumerates each field with keep/mask/normalize, and its mutation tests fail when any listed field differs unmasked. |
| REQ-XE-006 | The HH-native mini-swe-agent SHALL emit `system(system_template)` and `user(instance_template)` rendered with mini-swe-agent's variable set, one `bash` tool with the upstream description and parameter schema, and no additional tools on the wire. | F2, F12 (G1–G4) | OL-05 | L1 turn-1 request equals the original's under REQ-XE-005. |
| REQ-XE-007 | The HH-native mini-swe-agent SHALL render tool observations with the verbatim `mini.yaml` observation template (character-based 10 000 threshold, 5 000 head and tail, `elided_chars`). | A.5, G8 | OL-05 | L1 fixtures include an output above 10 000 characters, and the rendered tool message is byte-equal. |
| REQ-XE-008 | The HH-native mini-swe-agent SHALL reproduce format-error handling: the bad assistant turn is omitted from the transcript, the error is rendered from `format_error_template` with `finish_reason` and `has_tool_calls`, and the run exits after `max_consecutive_format_errors` (3) consecutive errors. | F4, G9 | OL-05 | L1 fixture with malformed and no-tool-call responses; request k+1 equals the original's. |
| REQ-XE-009 | The HH-native mini-swe-agent SHALL detect submission by the first-line marker `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` with return code 0, configurable as a slot parameter. | A.2, G10 | OL-05 | Run ends with exit status `Submitted` on the marker fixture, and does not end on rc≠0 or non-first-line marker fixtures. |
| REQ-XE-010 | The HH-native Terminus-2 SHALL provide a persistent terminal-session environment with keystroke send (literal text plus tmux key names), duration-based waits clamped at 60 s, full-history pane capture without escapes, and the upstream incremental-output rule. Any declared divergence from `_find_new_content` must be recorded. | F5, F11, G13 | OL-05 | L1 on recorded pane captures reproduces the observation strings; a divergence register entry exists if the quirk is not reproduced. |
| REQ-XE-011 | The HH-native Terminus-2 SHALL implement the JSON-plain response protocol: first balanced object, required fields, the two auto-fixes, verbatim error and warning feedback strings, no error cap, and the two-consecutive-`task_complete` handshake with the verbatim confirmation text. | F5, G14, G15 | OL-05 | L1 replay of Harbor's `hello-world-invalid-json` scenario produces identical request sequences. |
| REQ-XE-012 | The HH-native Terminus-2 SHALL implement its summarization as a declared compaction strategy: proactive free-token threshold, reactive unwind to 4 000, three calls with verbatim prompts, history replacement, and short/no-LLM fallbacks. Each summarization call SHALL be a distinct L1 request stream. | F6, G16 | OL-05, OL-10 | L1 replay of Harbor's context-summarization scenario (`max_input_tokens: 7000`, threshold 0) matches all main and subagent requests. |
| REQ-XE-013 | The Terminus-2 participant record SHALL state the `model_info` (context window) used for summarization triggers and the tokenizer used for counting. A summarization-threshold sweep is admissible only when that window makes the trigger reachable. | F10 | OL-07, OL-10 | Sweep pre-registration cites the window and shows the trigger fired in ≥1 development trajectory. |
| REQ-XE-014 | The sweep factor registry SHALL draw harness knobs from the A.7/B.6 inventories and mark the hard-coded constants (10 000-byte cap, 60 s clamp, 4 000 unwind target) as non-upstream knobs when varied. | A.7, B.6 | OL-07 | Factor registry entries cite the upstream locator or "HH-only". |

## Handoff

- **Dependencies resolved:** E1 has none. This note supplies E2's anatomy input and D2's L1 surface.
- **Proposed follow-up (coordinator to decide):**
  - a $0 "L1 capture spike" row, after B2 and C2, that runs each original once against a local replay server with a hand-written recorded response and commits the captured request bodies. It settles F7 and F8 and the G5/G6 canonicalization choices.
  - E2 should explicitly assess G16 (`compact_model_owned` fit) and G17 (podman-cell executor semantics).
- **Conflicts with other notes:** none known; notes 24–27 and 29 were not read for this row.
- **Capability and portfolio disposition:** none changed. The finding that Harbor's tests already contain a deterministic fake-LLM harness (F9) should be reused by B2/D2 rather than reinvented, subject to CC1-style "one implementation" judgement at design time.
