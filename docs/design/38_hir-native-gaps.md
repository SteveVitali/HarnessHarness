# 38_hir-native-gaps — HIR and kernel gap closure for native harness representation
Stream: E
Researched: 2026-11-06 (inspection 2026-11-06T23:33Z–23:47Z)
Author: Claude Code design worker (Opus 5.5), row E2 of the experiments research ledger
Ledger rows covered: E2
Repository baseline: main 09fe8523. Read from worktree `claude/experiments-spec` at HEAD 58f7a8b7; `git diff --stat 09fe8523 HEAD -- crates spec scripts docs/adr` is empty, so every code and spec locator below holds at 09fe8523. The only non-`docs/` working-tree change is the coordinator's `.gitignore` edit, which is not mine.
Confidence overall: medium. The repository facts are high-confidence: each was read at the cited lines. The closures, the HIR document sketch, the staging advice and the ticket seams are design proposals. They are UNVERIFIED by construction, because nothing was built, compiled or run.

## Scope, inputs and method

Row E2 asks three things. First, what it would take for an HIR/1 document to represent mini-swe-agent v2.4.6 faithfully enough to pass D2's L1 gate (byte-equal canonical requests). Second, what Terminus-2 adds as stage two. Third, how the phase-4 knobs become first-class swept slot parameters. This note designs the minimal closure of each E1 gap inside HH's architecture and decides where harness-specific behaviour should live. It sketches the mini-swe-agent document, chooses a template-rendering approach that needs no new dependency, sizes the Terminus-2 stage, and shapes tickets.

**Program inputs read in full:**
- `docs/experiments-brief.md`, including addendum A-8;
- `docs/experiments-research-ledger.md`: row E2, §Q (Q-X13, Q-X14, Q-X15; pending Q-X22, Q-X29), the Execution contract and the standing limits;
- `docs/research/CONVENTIONS.md`;
- the OL-05/06/07/10 rows of `docs/experiments-OUTLINE_TRACE.md`;
- `docs/research/28_reference-harness-anatomy.md` (E1);
- `docs/design/36_hir-parity-method.md` (D2).

**Program inputs read in the relevant parts:**
- `docs/design/31_experiment-runner.md` (B2): §1.1–1.4, F2–F4, ticket hints, open questions, handoff;
- `docs/design/33_model-mediation.md` (C2): §1.1–1.2, F6, F7, F15–F17, §3, §4.

**Repository instructions.** I read root `AGENTS.md` and `crates/AGENTS.md`. No crate has a nested `AGENTS.md`: `find crates -name AGENTS.md` returns only `crates/AGENTS.md`. I ran `bash scripts/deferrals-open.sh`. It prints no `DF-*` row. `--bl` lists BL-50…53, none of which touches the HIR, model-port, exec or compaction seams.

**Canonical spec read.** All locators are lines of `spec/CANONICAL_SPEC.md`.
- §3.1 HIR/1, l.610–777, read whole: the ceiling (l.624), profile-owned surfaces (l.633), the entity table (l.650–668), AC-IR-08 (l.765).
- §3.2 compilation, l.777–925, read whole: purity (l.798), the closed-world rule (l.802), profile rules (l.814), "C0 admits `interaction_mode = native_fc` only".
- §5b.3 `ProfileRuleInventory/1`, l.2292–2310.
- §5e.1 control strategies, l.3864–3935: parameters (l.3894), `react/minimal` (l.3906), AC-R-2.6.1-2 (l.3933).
- §5d.1 `observation_contract`, l.3260 (grep hit, read in context).

**Code read at 09fe8523** (line ranges):

| Crate | Locators |
|---|---|
| hh-embed | `gateway_model.rs` 1–140, 483–1110; `open.rs` 2560–2600, 2715–2740, 3036–3150, 3440–3470, 3505–3545; `runtime.rs` 395–560, 761, 895–930, 2100–2200; `service.rs` 1565–1800; `envops.rs` 200–345 |
| hh-gateway | `openai.rs` 1–140, 472; `dialects.rs` 100–152; `http_transport.rs` 584–630, 725–750; `grammar.rs` 834–837; `message.rs` 126–148 |
| hh-control | `strategy.rs` 140–290; `driver.rs` 255–330, 815–845, 3655–3680, 3800–3840; `react.rs` 1–40, 1327–1342 |
| hh-compiler | `lib.rs` 1–60; `lower.rs` 120–420, 700–810; `tests/s3_2.rs` 20–60 |
| hh-hir | `records.rs` 355–400, 540–560, 1060–1100; `leaves.rs` 25–38 |
| hh-env | `capture.rs` 95–135, 355–395; `executor.rs` 455–590; `helper.rs` 480–620; `dispatch.rs` 2780–2860; `events.rs` 673–713 |
| hh-helper | `server.rs` 640–700; `podman.rs` 140–165; `exec.rs` symbol index and 430–452 |
| hh-context | `compact_family.rs` 1–60, 290–330 plus symbol index; `policy_family.rs` 60–95 |
| hh-assembly | `space.rs` 1–60, 95–130, 200–275, 380–400; `catalog.rs` 17, 40–49 |
| hh-lab | `experiment.rs` 100–135, 1630–1680, 1995–2030 |
| hh-embed-schema | `ops.rs` 290–320, 1040–1125; `types.rs` 570–600, 1076–1110 |
| hh-varhost | `lib.rs` 1–40 |

**Consumer searches** (grep, read-only):
- `arm_exec_capabilities`: only `service.rs`, `runtime.rs` and `tests/r5_8.rs`.
- `space::enumerate`: only `crates/hh-assembly/tests/s3_2.rs:19`.
- `SummarizeRolling`, `SUMMARIZE_ROLLING_REF` and `FreshWindowWithNotes` outside `compact_family.rs`: no consumer in any `crates/*/src`.
- `compaction_strategy` in `hh-embed/src`: no hit.
- `tmux|send_keys|capture_pane|tiktoken|cl100k` in `hh-env`, `hh-helper` and the other crate sources: no hit.
- `mini-swe` in crates: one compiler test fixture (`s3_2.rs:28`). It is a "mini-SWE-agent-class" `edit_file`/`get_file` document, not mini-swe-agent 2.4.6.

Nothing was built, run, installed or fetched. No network was used.

---

## Part 1 — Where harness-specific behaviour lives, and what "HIR represents the harness" means

### 1.1 The architecture already names the home

The spec does not leave "where do harness prompts and loop choices live" open. It answers it in three places.

1. **Prose.** `Text` is "the only construct for model-facing prose" (§3.1.2, l.624–632).
2. **Surfaces and rendering.** "Model-facing names, description templates, argument naming/order, error rendering and placement live in `surface`" and are excluded from `semantic_id` (l.633). The profile's closed rule set covers `prompt_layout` (with `section_templates{slot_id → Text leaf}` and `role_map`), `description_template`, `error_format`, `result_render`, `transcript_render`, `interaction_mode` and `sampling_defaults` (§5b.3, l.2292–2310).
3. **Executable behaviour.** "Control strategies, compaction strategies, context builders and routers are … component variants whose implementations are `CompiledPayload`s behind Core contracts while their parameters and decisions are typed" (l.632). A construct must be typed if the Lab must ablate, diff or stratify it (ceiling T3, l.624).

The spec's own baseline already is a mini-SWE-agent-class harness. `react/minimal` is defined as one exec `ToolCapability` with `input_schema: {command}` and `output_schema: {output, returncode}`, two `Text` leaves, a `Budget`, and `stop_rule = submit` meaning "marker on the first line of the effect's observed output with `returncode == 0`, detected by the driver" (§5e.1, l.3906). AC-IR-08 and AC-R-2.6.1-2 require that this anchor "round-trips the mini-SWE-agent-class anchor behaviourally" (l.765, l.3933).

**Phase 3 is therefore the spec's T-LCD-03 claim made concrete for one named upstream version.** It is not a new kind of claim.

### 1.2 Three candidate homes, weighed

| Option | What carries mini/Terminus behaviour | L1 exactness | Maintenance | Honesty of "HIR represents X" |
|---|---|---|---|---|
| **A. Generic kernel features + one HIR document (plus a neutral profile) per harness** | Harness choices are data: `Text` leaves, authored surface records, profile rules, typed slot params. Code is generic, registered variants. | Achievable. Every wire-visible choice becomes a typed field that a golden test can pin. | One kernel feature serves N harnesses and every sweep variant. Upstream drift is a document diff. | Strong. The document *is* the harness, and the code is shared machinery. |
| B. Out-of-process control-strategy variants (hh-varhost `plugin_abi/1`) | A per-harness plugin implementing `decide` | **Does not reach the gaps.** A strategy never holds a model handle and never renders a request (I2, §5e.1). Twelve of the gaps below are request-rendering or observation-rendering gaps that live in the model port and renderer. | A plugin crate per harness, plus ABI conformance. No `control_strategy` plugin exists today: `plugins/` holds compaction, memory-store and tracker variants only. | Weak. The behaviour sits in plugin code, not HIR. |
| C. A dedicated in-process strategy per harness (`hh/mini-swe-agent@1`) | Bespoke Rust | Possible, but it still needs the same rendering changes as A | Every harness and sweep variant forks code. Hybrids violate "Hybrids are parameter points, never a fifth variant" (l.3904). | The HIR becomes a label on Rust code. |

**Decision (design proposal): Option A.** The rationale has three parts.

1. **The gaps are mostly not in decision logic.** Of the 23 gaps in Part 2, 15 are in the model port, transcript rendering, tool surface or observation rendering. Five are in exec and limits. Only three (G9's streak semantics, G10's stop rule, G15's handshake) touch strategy decisions, and even those are parameters of `react/minimal`.
2. **Phase 4 needs typed knobs.** Sweeps need typed, diffable parameters (ceiling T3). Bespoke code (B, C) makes every knob a code change and defeats declared-delta conformance (D2 REQ-XD-125).
3. **CC1 favours reuse.** `lower_profile` already compiles descriptions, schemas, layout, error and result specs (F1). A harness-specific renderer would be the second implementation that CC1 forbids.

Two harness-specific code bodies remain unavoidable, and both are Terminus-2's:
- the JSON-plain response grammar, which the spec classifies as a `parse`-grammar `CompiledPayload` (l.632);
- the three-call handoff compaction, which is a compaction variant with typed parameters.

Both are admitted forms. They are disclosed, not hidden (§1.3).

### 1.3 The claim, defined precisely (design proposal)

"HIR document *D* represents harness *X@v* at level L1" holds iff all four conditions hold.

1. ***D* is complete.** *D* is a sealed HIR/1 document, bound to a declared profile. Every behaviour item in the E1 anatomy of *X@v* is assigned to exactly one locus in a committed **representation ledger** (`hh-representation/1`):
   - `data`: an HIR field, `Text` leaf, authored surface record, profile rule or slot parameter;
   - `generic_variant`: a registered variant whose code names no harness, used through typed parameters;
   - `harness_variant`: a registered variant written for *X*, with a typed interface and its opacity counted;
   - `declared_divergence`: not reproduced, with a reason and the L1 branches it exempts.
2. **No harness-named code path.** Kernel code contains no harness-named conditional. A grep-level CI check refuses `mini`, `terminus` and `swe` identifiers outside variant crates and fixtures.
3. **L1 passes.** *D*, at a pinned HH commit, passes D2's L1 against *X@v*.
4. **The claim reports its split.** The published claim states the counts per locus class: "mini-swe-agent 2.4.6: *n* items `data`, *m* `generic_variant`, 0 `harness_variant`, *k* declared divergences; L1 pass `<report id>`."

A document with a non-zero `harness_variant` share is still a representation, but the share is part of the claim. For Terminus-2 it will be non-zero: the parser grammar and possibly the handoff compaction.

---

## Part 2 — Gap-by-gap closure

The rows below are E1's G1–G17, re-verified at 09fe8523, plus six gaps found here (G18–G23).

**Spec/ADR/schema column legend:**
- **none**: implementation of a spec'd but unwired field.
- **ADR**: an architecture or interpretation record, with no change to frozen text.
- **AMEND**: a canonical-spec amendment through an ADR and the amendment protocol.
- **SCHEMA**: an `hh-embed-schema` change, then regeneration (CC7).

**Compatibility rule.** Every closure is **opt-in** by a declared field. A definition that declares none of them renders byte-identically to today, and each ticket's regression battery pins that.

| # | Gap (E1 wording, re-verified) | Stage | Owning crate · symbol (verified locator) | Closure (design proposal) | Spec / ADR / schema | Test strategy |
|---|---|---|---|---|---|---|
| G1 | No `system` message. The transcript starts with `user(instruction)`. | mini | `hh-embed` `GatewayModel::transcript`, `gateway_model.rs:735`. `hh-compiler` `lower_layout`, `lower.rs:700–810`, maps sections only by `role_map` over **authority class** (746–787). | Render the request preamble from the compiled `ModelSurface.layout` (G21). Implement the spec'd `prompt_layout.section_templates{slot_id → Text leaf}` (§5b.3 row 8), so that two `definition`-authority leaves can land in different sections. Section → provider role comes from the rule's declared role per section. | none (implements §5b.3 row 8); covered by the ADR-E2-A rendering record | Golden turn-1 request bytes for a fixture document (system + user); legacy single-user render byte-identical |
| G2 | Instance prompt is a template (`{{task}}`, uname). No template engine. | both | `GatewayModel::new` takes a raw `instruction: String` (`gateway_model.rs:576–620`) | `hh-tmpl/1`: a std-only template subset over `Text` leaves (§Part 4). It fills the declared run-time slots `task` and `uname.{system, release, version, machine}`. The fill values are ledgered: uname comes from a setup-phase read-only exec probe inside the cell. | ADR-E2-C (the grammar is an interpretation of "Text leaf with typed slots"); none for the spec | Template unit goldens; StrictUndefined refusal; L1 turn-1 census (D2) as the binding golden |
| G3 | Tool description = `semantic_id`. Parameters render `type`/`enum`/`required` only. Host surfaces have empty params. | mini | `render_tools` uses `&s.semantic_id` as the description (`gateway_model.rs:892`). The schema is rebuilt from `ParamSpec` (864–888). `driver_surfaces` sets `params: BTreeMap::new()` (`open.rs:3529–3541`). The compiler already produces `CompiledToolSurface{description, schema}` from `description_template`/`purpose` and `input_schema` (`lower.rs:319–363`). | `tools[]` render from `ModelSurface.tools`. `input_schema` is carried verbatim, including `description` members, with no re-derivation from `ParamSpec`. | none; ADR-E2-A | Golden `tools[]` bytes = mini's `BASH_TOOL` (E1 A.4); a mutation test on the description |
| G4 | `hh.submit` is always present; `tool_choice: "auto"` is always sent. | mini | `driver_surfaces` prepends `submit_surface_spec()` (`open.rs:3529–3531`). `chat_request_body` inserts `tool_choice` whenever tools are present (`openai.rs:62–65`). | (a) The completion surface is armed only when the definition declares it, or when `stop_rule` needs it (`no_action`/`either` with a claim surface). A definition with `stop_rule = submit{marker}` (G10) and no completion capability arms none. (b) `tool_choice` is rendered only when the `model_port` slot declares it. `tool_choice` is already an admitted plan key (`dialects.rs:115–128`). The default stays `auto`, so legacy bytes do not change. | none for the spec; the C2 proxy strips `auto` upstream anyway (D2 F8) | Downstream golden without `tool_choice`; legacy golden with it; a no-`hh_submit` assertion |
| G5 | `stream: true` and `stream_options` are always sent. | both | `request_body` passes `stream = true` (`gateway_model.rs:695–709`); `stream_options` is inserted at 917–921; the plan sets `streaming: true` (932). The transport already reads non-streaming bodies (`http_transport.rs:730–745`, `openai::completion_frames`, `openai.rs:472`). | A `model_port` slot param `transport ∈ {stream (default), non_stream}` with a `stream_member ∈ {omit, false}` choice. The value is fixed by the O-recorded body; whether LiteLLM sends `stream` at all is unobserved (E1 F7). | none | Golden non-stream body; the decoder path tested on a recorded `chat.completion` fixture. D2 rule C05 then stays unused. |
| G6 | Assistant prose (and `reasoning_content`) is dropped from the replayed history. | mini | `assistant_tool_msg` → `assistant_tool_call_msg(None, …)` (`gateway_model.rs:636–645`). The `content` parameter already exists (`openai.rs:76–96`). The text survives only in the `hh.model_io/1` blob (`finish_call`, 1048–1049). | The transcript renderer dereferences the durable `response_ref` blob. A typed `transcript_render.assistant_replay{content: keep\|drop, reasoning: keep\|drop, extra_members: [...]}` choice is set from what O re-sends on the dev partition (D2 R3). | none (§5b.3 row 9 choices); ADR-E2-A | L1 branch M18 golden |
| G7 | Text-only assistant turns are skipped. | Terminus | `if calls.is_empty() { continue; }` (`gateway_model.rs:783–785`) | The same `assistant_replay` choice, plus `text_only_turns: keep\|drop`. mini needs `drop` on format-error turns, which today's behaviour happens to give. Terminus needs `keep`. | none | Terminus T1/T2 goldens |
| G8 | The tool result is not a template-rendered string; `OutputPolicy.model_view ∈ {full, tail, summary}` is declared but unconsumed. | mini (char head/tail), Terminus (byte head/tail) | `capture.rs:365–388`. No consumer of `model_view` outside `capture.rs` (grep). | A typed `ResultRenderSpec` family (§5b.3 row 7), with a variant `json_head_tail@1{threshold, head, tail, unit ∈ {chars, utf8_bytes}, fields, json_style}`. mini uses chars, 10 000/5 000/5 000, `json_style = python_jinja_tojson`. A plain variant `text_head_tail@1{…, marker: Text}` serves Terminus. Executed at run time by the transcript renderer over the raw output (G18). | ADR-E2-C. AMEND only if O2 reads §5b.3 row 7's `mode` set as closed (it lists `{full, truncate, concise, offload}`). | M1/M2/M11/M15 goldens, multi-byte boundaries |
| G9 | Format-error text is a fixed `kernel_note`; the bad assistant turn must be omitted; 3-strike exit. | mini | `kernel_note: the prior output was declined…` (`gateway_model.rs:829–842`). `react/minimal` already nudges and counts no-intent `end_turn` replies (streak++, `stop{format_failure}` at the cap) (`react.rs:14`). `max_consecutive_format_errors` is 3 by default (`strategy.rs:241–253`). | (a) A typed `error_format` spec (§5b.3 row 6) that maps format-failure classes to `Text` leaves with slots: `no_tool_calls`, `args_unparseable{error}`, `unknown_tool{name}`, `missing_param{name}`, `output_truncated{finish_reason}`. (b) Batch semantics: the first malformed call fails the whole response (mini), as a `format_failure_scope = batch` param. (c) `transcript_render.on_format_error = omit_assistant`. | none for (a, c); (b) folds into G11's parameter work. A **declared-divergence candidate** is the Python `json.loads` error text inside `args_unparseable{error}` (E1 `actions_toolcall.py:60`), which HH cannot reproduce by construction. | M4–M10 goldens |
| G10 | No configurable submit marker; submission is detected only via `hh.submit`. | mini | `DispatchGate`/`EmbedGate` set `submission_ref` only for `SUBMIT_SURFACE` (`runtime.rs:415–424`). `StopRuleParam::Submit` is documented as marker-on-observed (`strategy.rs:193–206`), but no marker literal or detection exists. | `stop_rule = submit{marker, line: first, require_returncode: 0}`. Detection runs in the dispatch gate on the settled observation's raw output (G18), per ADR-0104's "detected by the driver, never the environment". The submission payload is the remaining lines. | **AMEND** §5e.1 parameter `stop_rule`: the value shape gains `marker`. The literal is harness data and must not be a kernel constant. | M12/M13 fixtures: marker on line 1 with rc 0 ⇒ `Submitted`; line 2 or rc ≠ 0 ⇒ continue |
| G11 | `StrategyParams` are hard-coded per variant. | both | `StrategyParams::default()` at `open.rs:2579–2583` and `2728–2732`. `SlotBinding.params` exists (`records.rs:540–549`), and the `model_port` slot's params are already read (`model_port_slot_params`, `gateway_model.rs:483–500`). | Decode `control_strategy` slot params into `StrategyParams` through `hh-control`'s existing `wire.rs` codec (`params_from_json` with `stop_rule_parse`, `wire.rs:89–91, 182–186`). Unknown members are a typed refusal at arm time. An absent slot param keeps the variant preset. The resume path re-reads the same persisted values. Budgets already flow: HIR `Budget.dimensions.{model_calls, turns, retries, time.*}.hard` reach `DriverConfig.budget_ceiling` (`open.rs:3036–3066`; `driver.rs:831–837`). | none (§5e.1 already calls these MUST-data parameters). The variant's `ParameterSpec` declaration is the §3.3 home. | Arm-time decode tests; refusal on an unknown member; resume parity; "step limit N ⇒ exactly N calls" fixture |
| G12 | Sampling and request params per harness | both | `params = self.lane.provider_params.clone()` (`gateway_model.rs:911`). `allowed_keys` = {temperature, top_p, max_tokens, seed, stream_options, tool_choice} (`dialects.rs:115–128`). | mini sends none: `provider_params = {}`, no `sampling_defaults` rule. Terminus sends `temperature` only when configured. Effort is pinned by the proxy for both arms (C2 F7). | none | Golden: no sampling keys present |
| G13 | No persistent interactive terminal (tmux) | Terminus | No tmux or pty code in `hh-env`/`hh-helper` (grep) | **No new kernel primitive.** A `terminal_session@1` executor in `hh-env` wraps `HelperExecutor` and issues `tmux new-session/send-keys/capture-pane` as exec calls inside the cell. This matches how Terminus itself reaches the container: it only calls `environment.exec` (E1 B). The duration wait is a host-side sleep, recorded. | ADR-E2-T1 (executor admission, ADR-0421 precedent style) | Podman-stub argv goldens; recorded-pane replay goldens (T6–T8, T12, T13) |
| G14 | No text-parsed JSON action protocol | Terminus | The only action channel is `ParsedCall` from native tool calls. `ToolArgs::Raw` is "reserved for the C2 `freeform` interaction mode (T7)" (`message.rs:136–137`). | `interaction_mode = shim{extraction_grammar_ref: terminus-json-plain@1}`. The grammar is a `parse` `CompiledPayload` (l.632) and yields typed calls `terminal.send{keystrokes, duration}`, a `completion` flag, and `warnings[]`/auto-fix notes. Warnings render through `error_format`. | **ADR-E2-T2.** It admits `shim` ahead of its tier, because §3.2.3 says "C0 admits `interaction_mode = native_fc` only". | Harbor `invalid_json` scenario replay (T1–T3) |
| G15 | No two-step completion handshake | Terminus | Completion is `stop{completed}` or `hh.submit`; `max_continue_nudges` exists (`strategy.rs:169–171`) | An envelope `stop` guard (C0-admissible: a deterministic predicate over ledger state, §5e.1 Guards) that refuses the first completion proposal of a streak. A `HarnessRule` nudge carries the verbatim confirmation `Text` with a `{terminal_output}` slot. `max_continue_nudges` is set to the turns bound. **Fallback** if O2 finds the guard form inadequate: a `completion_protocol` parameter. | none for the guard route; AMEND (§5e.1 parameters) for the fallback | T4 goldens (once → confirm; twice → end; reset) |
| G16 | Three-call summarize/questions/answers handoff that replaces history | Terminus (and phase-4 compaction sweeps) | `KernelCompaction{variants: Vec::new()}` — "empty at C0" (`runtime.rs:2111–2134`). Wired at drive (`service.rs:1765–1768`). The `compaction_strategy` class is registered (`catalog.rs:40–49`). The C1 variants (`hh/summarize-rolling@1` …, `compact_family.rs:300–313`) have no `src` consumer. | (a) Generic: populate `KernelCompaction.variants` from the sealed `compaction_strategy` slot binding and its params (the first consumer of the C1 family). (b) Terminus: register `hh/handoff-summarize@1` (`harness_variant`) with typed params `{free_token_threshold, unwind_target, prompts: Text×4 (summary, questions, answer_request, handoff), fallbacks}`. Its three calls use the `compaction` model role, and it ends with history replacement. Token counting needs a decision (G16t, Part 5). | (a) none. (b) ADR-E2-T3 (variant + tokenizer decision). A tokenizer port as committed data with std code needs no crate dependency, but needs an ADR if contested (D2 open question 6). | (a) Small-window fixture compacts; slot absent ⇒ byte-identical. (b) Harbor summarization scenario (T9/T10). |
| G17 | `subprocess` exec semantics: fresh shell, merged streams, 30 s timeout with partial output, env, cwd | mini | The helper runs `command` as `/bin/sh -c <s>` (`server.rs:645–647`), matching `shell=True`. **Differences:** stdout and stderr are separate pipes pumped by two threads (`exec.rs:442–452`), not one fd; cwd defaults to `"."` (`helper.rs:552–557`) under podman `--workdir` (`podman.rs:146–158`; B2 F8); env is fixed at executor construction (`helper.rs:565`); a deadline kill targets the host-side pgid of `podman exec`, so whether the in-container process group dies is UNVERIFIED. | (a) Helper exec member `stdio = merged` (dup stderr onto the stdout pipe at spawn). (b) cwd = image WORKDIR (with B2's CELL-SPEC). (c) Capability-declared `env` constants merged into the exec request (mini's `PAGER=cat` …). (d) In-container kill: run via `setsid`/`timeout --kill-after` inside the cell, so the whole tree dies. (e) Timeout maps to `returncode = -1` plus the `exception_info` text through the result renderer. | ADR-E2-B (helper protocol member + executor semantics). The helper protocol is internal and not an `hh-embed` schema change. | Podman-stub argv; a merged-order fixture (`echo a; echo b >&2; echo c`); live leg `#[ignore = "live — ADR-0363 …"]` |
| **G18** (new) | **The model-visible tool result is a ledger record, not the command output.** | both | `context.observation.recorded` is minted only for a `flow_contract` capability or a leaf refusal (`dispatch.rs:2823–2856`). Its payload is `{kind, effect_id, capability_ref, capture_manifest_ref, outcome, admission, label}` (`events.rs:673–702`). `GatewayModel` puts that canonical JSON in the `tool` message, or `{"kind":"no_observation_recorded"}` when none was minted (`gateway_model.rs:796–812`; `openai.rs:103–113`). Raw output lives behind the manifest's `raw_output_ref`; `output_chunk` is ephemeral (`capture.rs:95–103`). `tests/r5_8.rs` uses a scripted model and `flow_contract: None` (l.243), so the live-model plus armed-exec path is unexercised. | Every armed exec observation yields a model-view record. The transcript renderer resolves `raw_output_ref` (a durable blob), applies the capability's `ResultRenderSpec` (G8), and renders the result **string**. The `context.observation.recorded` row carries the rendered view's content address, so replay re-serves the recorded rendering (ADR-0135-style). Record rendering, not just admission. | ADR-E2-A. Payload members are additive under CC8 absent-not-null. A new class is not needed. | Fixture: armed exec + `GatewayModel` over a recorded transport ⇒ the `tool` message equals the rendered string |
| **G19** (new) | Tool-call ids on the wire are kernel ids, not the provider's ids | mini | The decoder mints `tool_call_id = "{model_call_id}.tc{index}"` and keeps `surface_ids.provider_call_id` (`grammar.rs:834–837`). The transcript renders the kernel id (`gateway_model.rs:771, 789–799`). | `transcript_render.id_normalisation = provider_verbatim` renders `provider_call_id` in both `assistant.tool_calls[].id` and `tool.tool_call_id`. The §5b.3 row 9 note says `id_normalisation` is "derived from capabilities". | none | M1 golden (D2 says ids are compared strictly) |
| **G20** (new) | Tool-call arguments are replayed re-canonicalized, not verbatim | mini | `ToolArgs::Parsed(j) => j.to_canonical_string()` (`gateway_model.rs:1035`). `Parsed` keeps no raw bytes (`message.rs:126–135`). | `ToolArgs::Parsed` gains a `raw` member (gateway-internal type), or the renderer re-reads the raw argument string from the `model_io` frames. Either way the replayed `arguments` equal the provider's bytes. | none | Golden with `{"command": "ls -la"}` (space after the colon) round-tripped byte-exact |
| **G21** (new, structural) | **The live request is built by a hand-written renderer that ignores the compiled `ModelSurface`.** | both | `GatewayModel` builds messages and tools itself (`gateway_model.rs:684–902`). `lower_profile` (`lower.rs:127–410`) already emits `layout`, `tools{description, schema, error_format, result_render}` and `transcript_renderer`. `lab.assembly.compile` is `stage_pending` until the "`CompileInputs`/`ModelProfile` wire codec lands" (`ops.rs:1119–1123`). §3.2.2 requires that "the context builder executes the compiled renderer spec and fills the `transcript` of the one `ProviderRequestPlan`" (l.802). | Arm `GatewayModel` with the session's compiled `ModelSurface` and renderer spec, compiled at arm from the sealed definition and the bound profile through `hharness_compiler::compile`, already called in `assembly_ops.rs:311` and `bundle_ops.rs:1306`. G1, G3, G6–G9 and G18–G20 become renderer-spec features. No surface ⇒ today's render, byte-identical. | **ADR-E2-A** (who renders; durable rendering refs). Spec: none, because this implements l.802. | Two-path golden: legacy bytes unchanged; surface-bound bytes match the compiled spec |
| **G22** (new) | Exec arming does not exist over `hh-embed/1` | both | `arm_exec_capabilities` is a Rust method (`service.rs:1580–1604`), called only by `tests/r5_8.rs`. `open_session` supplies only `host_capabilities` (`types.rs:586–592`), and those become param-less host surfaces (`open.rs:3085–3131, 3529–3541`). | An additive optional member on `OpenSessionParams`: `exec{env: "session", surfaces: [capability semantic_id …]}`. It arms `ExecCap`s from the sealed definition's `ToolCapability` nodes over the session's env handle (`env.open{connection_info}` already provisions classed environments, `envops.rs:239–300`), re-running the existing `bind` check. It is explicit and never inferred, matching the `ExecCap` doc (`runtime.rs:436–442`). | **SCHEMA** (`hh-embed-schema` → `hh-codegen` → `scripts/check-drift.sh`; CC7) + ADR-E2-B | Schema/codegen drift check; RPC-level arm test over binding (b); refusal when the executor cannot satisfy `execution_requirement` |
| **G23** (new) | Transport-retry parity | both | Retries are code-owned `retry` decisions under `retries_ceiling` (`driver.rs:838–839`; I5). mini retries 10 attempts with exponential backoff (E1 A.2); Terminus nests 3×3 (E1 B.1). | Declare the envelope's transport-retry policy in the definition (attempt count and retryable classes) so the request **count** matches M17/T11. Backoff waits affect only wall time; they are recorded and not compared. | none (envelope policy is data, R-2.6.2) | M17 fixture: 429, 502, success ⇒ equal retry count and identical bytes |

**What breaks mini-swe-agent's first request today.** Request 1 differs in six ways: G1 (no system message), G2 (raw instruction), G3 (description = semantic id), G4 (extra `hh_submit` tool), G5 (`stream`) and G21 (renderer). E1's F12 named four of these; G2 and G5 complete the list. From request 2 on, G6, G18, G19 and G20 also break it.

---

## Part 3 — The mini-swe-agent 2.4.6 HIR document

### 3.1 Shape (design proposal; `docs/…/hir/mswea-2.4.6.hir.json` at implementation)

```text
HirDocument{ hir_version: "HIR/1", root: Ref<AgentProcess:mswea>, assembly: {resolved}, nodes:
  AgentProcess:mswea  native{
    profile: ProfileRef(hh/profile/harness-faithful@1)   // neutral profile: prompt_layout +
                                                          // transcript_render + no sampling rules
    slots:
      control_strategy: hh/react-minimal@1 params{
        stop_rule: {submit: {marker: "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT",
                             line: "first", require_returncode: 0}},        // G10 (AMEND)
        max_consecutive_format_errors: 3, max_continue_nudges: 0,
        tool_batch_mode: "sequential", format_failure_scope: "batch",       // G9b
        completion_surface: "none" }                                        // G4a
      context_policy:  hh/default@1                       // full window; mini has no windowing
      compaction_strategy: (absent)                       // mini has none; overflow aborts
      model_port: params{ dialect_ref: "openai-compatible@1", endpoint_ref: <C2 loopback>,
        transport: "non_stream", stream_member: <from O bytes>, tool_choice: "omit",
        provider_params: {}, retry: {attempts: 10, classes: [rate_limited, server_error, …]} }
    budget: Ref<Budget:mswea>, permissions: Ref<Permission:mswea-exec>, environment: <cell> }
  Budget:mswea  dimensions{ time.wall_ms: {hard: <task agent timeout × m_agent>},   // Harbor's
                            spend: {hard: <proxy slot slice>} }   // external caps (D2 R4); no
                                                                  // model_calls cap = step_limit 0
  Goal:task  statement: Text(slot "task"), success_criteria: [], unverifiable_reason: Text("…")
  Text:system_template    content = mini.yaml system_template (verbatim), grammar hh-tmpl/1
  Text:instance_template  content = mini.yaml instance_template (97 lines), grammar hh-tmpl/1
  Text:fmt.no_tool_calls / fmt.tool_call_error / fmt.output_truncated   // from format_error_template
  ToolCapability:bash  semantic{
    purpose: Text("Execute a bash command"),
    input_schema: {type: object, properties: {command: {type: string,
                   description: "The bash command to execute"}}, required: [command]},
    output_schema: {output, returncode, exception_info?},
    effects: {exec{destructive, non_idempotent, closed, irreversible}},
    execution_requirement: {environment_class: local_container, …},
    observation_contract: {…, error_classes: [timeout, …]},
    exec_defaults: {timeout_ms: 30000, stdio: merged, cwd: image_workdir,
                    env: {PAGER: cat, MANPAGER: cat, LESS: "-R", PIP_PROGRESS_BAR: off,
                          TQDM_DISABLE: "1"}} }                             // G17 (ADR-E2-B)
    surface: ToolSurface{ name: "bash", namespace: "", description_template: Text("{purpose}"),
      argument_order: [command],
      result_renderer: {kind: "json_head_tail@1", unit: chars, threshold: 10000, head: 5000,
                        tail: 5000, json_style: "python_jinja_tojson",
                        fields: [returncode, output|(output_head, output_tail, elided_chars,
                                 warning:"Output too long."), exception_info?]},
      error_format: {kind: "classes@1", classes: {no_tool_calls → Text:fmt.tool_call_error{error},
                     output_truncated → Text:fmt.output_truncated{finish_reason}, …}} }
  Permission:mswea-exec  grants: [{effect{exec}, scope: cell:/**}]   // yolo: no approval
}
profile hh/profile/harness-faithful@1:
  prompt_layout{ order: [system, task], section_templates{system → Text:system_template,
                 task → Text:instance_template}, roles{system → system, task → user},
                 slots{task: [task, uname.system, uname.release, uname.version, uname.machine]} }
  transcript_render{ assistant_replay{content: keep, reasoning: <dev-partition>, extra_members:
                     <dev-partition>}, text_only_turns: drop, on_format_error: omit_assistant,
                     id_normalisation: provider_verbatim, tool_args: verbatim }
```

The sketch's member spellings are proposals. Several encodings follow from what the code and spec already provide:
- `exec_defaults` is a typed member proposed for ADR-E2-B. It is not an existing field.
- The renderer and error-format kinds are typed records inside the existing `ToolSurface.result_renderer`/`error_format` JSON slots. The compiler's fallback reads those slots when no profile rule overrides them (`lower.rs:333–347`).
- The `{purpose}` substitution in `description_template` is already implemented (`lower.rs:319–331`).

**Templates by reference and hash.** Each `Text` leaf's content is the exact template source as Jinja receives it, after YAML parsing of `config/mini.yaml` (sha256 `b539a896…`, E1 A.3). Its `content_hash` enters `semantic_id`, so a template edit is a semantic diff, as a sweep variant should be.

**Provenance.** The document is **human-authored**: `origin = human(author, role = author)`, `authority = definition`, with `derived_from` naming the mini.yaml digest and JSON pointer. It is not imported. Imported text is `authority = unverified` (l.699), and the role-placement invariant renders a leaf "only into roles ≤ `role_map(authority)`" (§3.1.11). An imported system template therefore could not legally render as `system` (F18).

### 3.2 How each E1 mini behaviour maps (representation ledger, first draft)

| E1 item | Locus | Class |
|---|---|---|
| system + instance messages (A.2-1, A.3) | `prompt_layout` sections + two `Text` leaves (hh-tmpl/1) | data |
| uname/environment variables (F3) | declared slots filled by a setup probe (ledgered) | data + generic_variant (probe) |
| one `bash` tool, no `tool_choice` (A.4) | `ToolCapability` + `ToolSurface`; `model_port.tool_choice = omit`; `completion_surface: none` | data |
| ≥1 tool calls per response, sequential exec (A.4) | `react/minimal`, `tool_batch_mode = sequential` | generic_variant |
| fresh `sh -c`, merged streams, 30 s timeout, env, cwd (A.4) | `exec_defaults` + helper `stdio = merged` | data + generic_variant (helper) |
| observation template (A.5) | `json_head_tail@1` params | data + generic_variant (renderer) |
| format errors: template, omit turn, 3-strike, reset (A.2, F4) | `error_format` classes + `on_format_error` + `max_consecutive_format_errors` | data |
| marker submit with rc 0 (A.2) | `stop_rule.submit{marker}` | data (needs AMEND) |
| limits: step 0, cost 0, wall 0 under Harbor (F1) | no `model_calls` cap; wall and spend as external caps | data |
| transport retries 10× (A.2) | envelope retry policy | data |
| assistant `model_dump()` replay (A.6, F8) | `transcript_render.assistant_replay` | data |
| no sampling params (A.6) | empty `provider_params` | data |
| `ContextWindowExceededError` aborts (A.6) | no compaction; overflow ⇒ `stop{context_exhausted}` / failure class | generic_variant (react) |
| Python `json.loads` error text in format errors (M5) | — | **declared_divergence candidate** (resolved when the L1 dev partition shows the actual strings) |
| Harbor's `AgentTimeoutError` from outside (A.1) | B2 runner agent deadline (REQ-XB-119) | runner, not HIR |

---

## Part 4 — Rendering templates identically without a new dependency

**Options weighed.**

1. **Pre-render at compile time.** Rejected. Compile is pure ("no clock, randomness, environment or network", l.798), and uname is a run-time fact.
2. **Run the original's Jinja2 in the cell.** Rejected. It puts a Python dependency in the representation, and "HIR represents" would then mean "HH calls mini's code".
3. **Port all of Jinja.** Rejected. It is a large surface, and most of it is unused.
4. **Chosen: typed renderers where the template encodes logic, plus a minimal template subset for prose.** The observation and format-error templates encode *logic*: thresholds, slicing, JSON escaping, branch on `finish_reason`. They become typed `ResultRenderSpec`/`error_format` records (G8, G9). That is also what the sweep needs: the 10 000/5 000 constants become typed, sweepable params, never string edits (ceiling T3). Only the system and instance templates stay templates.

**`hh-tmpl/1` (design proposal; ADR-E2-C; std-only, home `hharness-compiler` next to `lower_layout`, or `hharness-context` if O2 prefers the renderer side).**
- **Syntax.** `{{ ident(.ident)* }}` substitution, and `{% if ident == "literal" %} … {% elif … %} … {% else %} … {% endif %}`. Jinja's whitespace-control `-` markers on either delimiter are honoured, with Jinja's default `trim_blocks = false`, `lstrip_blocks = false` and `keep_trailing_newline = false`. There are no filters, loops or expressions beyond `==` on string literals.
- **Strict undefined.** A template whose variable set is not a subset of the section's declared slots is refused at compile time (`UnexpressibleSurface`, an error, l.792). A slot unfilled at run time is a typed refusal that stops before any model call, never an empty string.
- **Subset fit.** The mini instance template contains `{%- if system == "Darwin" -%}` (E1 A.3). If the pinned template uses any construct beyond this subset, the doc ticket extends the grammar under the same ADR before it lands. A unit test parses the pinned mini.yaml templates and lists their variables.
- **Fill values are ledgered.** `task` is the instruction bytes the runner also hands Harbor (B2/C4). The uname values come from a setup-phase `uname -s/-r/-v/-m` read-only exec in the cell, the same source D2 rule C04 checks.
- **The golden is O's recorded bytes, not my reading of Jinja.** E1 states the rendered system template "ends with a newline". Jinja's default `keep_trailing_newline = false` may strip it. I could not check this without the source (F19). D2's turn-1 census (one O request per task) is the binding golden, and the engine's whitespace rules are fitted to it on the dev partition only.

**`json_style = python_jinja_tojson` (for `json_head_tail@1`).** The rendered observation embeds `tojson` output *inside* a string value. D2 rule C01 decodes only the outer JSON's escapes, so the inner escape spelling is compared literally. My recollection of Jinja2 3.x is that `tojson` is `json.dumps` (`ensure_ascii`, default separators) followed by HTML-safe replacement of `<`, `>`, `&` and `'` with `<`-style escapes. Non-BMP characters would encode as surrogate pairs. **This is recollection, not a read source (UNVERIFIED).** The renderer is accepted only when goldens over M1, M2 and M15 (including `<`, `&`, `'`, emoji and U+FFFD from `errors="replace"`) match O's recorded bytes. Truncation counts Unicode scalar values: Python's `str` from strict-or-replace UTF-8 decoding holds no surrogates, so Python slices and Rust `char_indices` agree.

---

## Part 5 — Terminus-2 (stage two)

### 5.1 What must additionally exist

| Need (E1 B) | Closure | Class |
|---|---|---|
| Persistent `bash --login` in tmux, 160×40, `history-limit`, pipe-pane | `terminal_session@1` executor: setup-phase `tmux new-session …` via helper exec, as `environment.default_user` (G13). Requires tmux in the image, which Harbor also requires. | generic_variant |
| Keystroke send (one key argument; tmux key names; >16 000 bytes via base64 `load-buffer`/`paste-buffer`) | `terminal.send{keystrokes, duration}` capability; the executor reproduces Harbor's quoting and paste-buffer threshold (32 768 at `ff2a1ef4`, E1 pins) | generic_variant + data (threshold) |
| `min(duration, 60)` sleep, no completion detection | Executor param `max_wait_ms = 60000`; a host-side sleep recorded in the effect timing | data |
| `capture-pane -p -S -`, incremental diff, `New Terminal Output:`/`Current Terminal Screen:` headers, the `_find_new_content` quirk | `terminal_incremental@1` result renderer over the current capture and the previous recorded capture (read from the ledger, so deterministic on replay). Parameter `prefix_rule ∈ {harbor_rfind_quirk, aligned}`, with `harbor_rfind_quirk` the reference value (E1 B.4). | generic_variant |
| 10 000-**byte** head/tail with marker text | `text_head_tail@1{unit: utf8_bytes, marker: Text}` | data |
| JSON-plain parser, warnings, two auto-fixes, no error cap, bad turn kept | `interaction_mode = shim{terminus-json-plain@1}` grammar (G14); `max_consecutive_format_errors = turns bound`; `on_format_error = keep_assistant`; warnings via `error_format` | harness_variant (grammar) + data |
| Two-consecutive `task_complete` handshake | G15 guard + `HarnessRule` nudge | data |
| `finish_reason: length` re-query with `limit_str` from `model_info` | `error_format.output_truncated{limit_str}`; `model_info` declared in the participant record (D2 R1) | data |
| No system message; the rendered template is `user[0]` | `prompt_layout` single `task` section → `user`; `str.format` placeholders `{instruction}`/`{terminal_state}`, with doubled braces as escapes. A `hh-tmpl/1` *mode* `py_format` or a pre-transformed leaf (UNVERIFIED which is cleaner). | data |
| Three-call summarization, proactive and reactive triggers, unwind by pairs, short and no-LLM fallbacks | `hh/handoff-summarize@1` (G16b) under the `compaction` role | harness_variant |
| `litellm.token_counter` triggers | **G16t decision.** (i) Port the BPE tokenizer as std code plus committed vocabulary data (no crate dependency; ADR needed if contested), or (ii) declare a divergence and run T9/T10 in a trigger-forced L1 configuration. | divergence or generic_variant |
| 3×3 nested retries | G23 policy with the inner and outer counts | data |
| Model calls from the host, not the cell | Kernel-native lane to C2's token-gated loopback listener (C2 F17); no cell model socket | runner topology |

### 5.2 Sizing and the staging recommendation

**Size.** Beyond mini's seams, Terminus-2 needs about six implement-spec tickets (Part 7, E2-T12…T17) and three ADRs (terminal executor, `shim` admission, handoff variant plus tokenizer). Then come D2's own Terminus L1 seams (K10–K14) and a separate pilot, G-L1 and G-L2. E1 F14 measures the original at about 4 400 lines against mini's 1 100. G13 and G16 hold most of the risk.

**Recommendation (design proposal; operator decision because it sets scope).** Do **not** put Terminus-2 in the first Round-6 build. Land it as a later block, gated on mini-swe-agent's G-L1 pass and preferably on its L2 verdict. Four reasons:

1. D2 already sequences the Terminus L1 and L2 after mini's L2 (D2 §5.3). Building H early buys nothing until then.
2. Six of Terminus's needs reuse seams that mini proves first: rendering, typed renderers, exec arming, slot params, the compaction slot and sweep glue. Building them once against mini's stricter wire, with native tool calls, ids and arguments, de-risks Terminus.
3. The tokenizer question (G16t) is only decidable once D3 says whether a summarization-threshold sweep is pre-registered. Under the reference configuration the trigger is unreachable (E1 F10; D2 R1).
4. Phase 4 needs only one parity-checked baseline (D2 F16), and mini supplies it.

The `shim` ADR (E2-T12's prerequisite) can still be written in Round 6 at docs-only cost, so the stage-two block starts dispatch-ready.

---

## Part 6 — Sweep enablement (input to D3)

### 6.1 Knobs, homes and candidate level sets

D3 owns the final factor registry and level choices. The baseline marked "B" is the parity-checked configuration.

| Factor | Typed path (HIR or profile) | Candidate levels | Needs | Exercised when |
|---|---|---|---|---|
| Observation truncation | `ToolSurface.result_renderer.{threshold, head, tail}` | 2 500 / 5 000 / **10 000 (B)** / 20 000 / none | G8, G18 | outputs ≥ threshold occur. L3 truncation counts show it. |
| Step limit | `Budget.dimensions.model_calls.hard` | **∞ (B, Harbor default)** / 100 / 50 / 25 | G11 (already flows) | trajectories exceed the cap (pilot calls/trial ≈ 31.8, D2) |
| Cost limit | `Budget.dimensions.spend.hard` + proxy slot slice | **external cap (B)** / ½ / ¼ of pilot P90 | C2 budget | binding frequency > 0 |
| Command timeout | `ToolCapability.exec_defaults.timeout_ms` | 10 s / **30 s (B)** / 120 s | G17 | long commands occur (M11-class) |
| Format-error cap | `control_strategy.max_consecutive_format_errors` | 1 / **3 (B)** / 5 | G11 | streaks ≥ 1 observed |
| Prompt variants | `Text:system_template`, `Text:instance_template` (alternative leaves; `ablate` op, l.704) | **verbatim (B)** / ablate the "recommended workflow" block / ablate `system_information` | G2 | always (wire-visible) |
| Tool surface | `ToolSurface.description_template`, an added capability | **bash only (B)** / bash + `edit_file` (the existing reference capability) / reworded description | G3, G21 | always |
| Interaction mode | profile `interaction_mode` | **native_fc (B)** / `shim` text actions (mini's own `litellm_textbased` mode, E1 A.4) | G14 (stage two) | always |
| Assistant reasoning replay | `transcript_render.assistant_replay.reasoning` | **as O (B)** / drop | G6 | reasoning present (glm thinking is always on, C1) |
| Compaction strategy | `compaction_strategy` slot | **none (B)** / `hh/summarize-rolling@1` / `hh/fresh-window-with-notes@1` / `evict_oldest` | G16a + a declared `window_cap` | **only** under a declared window well below 1 M tokens (e.g. 32 k/64 k) crossed as a fixed condition. At the catalog window it never fires (D2, E1 F10). |
| Context policy | `context_policy` slot | **default (B)** / `recency_plus_pins` / `priority_eviction` | G21 (F14) | only after the assembled view reaches the wire |

Model effort (Q-X22) is a participant property pinned by the proxy, not a harness slot. D3 may cross it, but it does not belong in the harness-knob registry.

### 6.2 Lab OFAT → hh-runner glue

1. **Declare the space.** The baseline document's `assembly` section declares the sweepable `ParameterSpec`s above. `space(sealed)` reads them (`space.rs:42–51, 71`). It is "never inferred from code".
2. **Materialize levels as sealed documents.** `enumerate(space, OverrideList[…], catalog, profiles, kernel)` (`space.rs:260–266`) is the **first production consumer** of `enumerate`; today only `tests/s3_2.rs:19` calls it. Each OFAT perturbation is one override spelling. Each point passes `validate_assembly` before spend; failures route to `rejected`, never silently (`space.rs:14–20`). Each point is then resolved and sealed, and the sealed `version_id` is the level's identity.
3. **Bind levels to the Lab design.** The Lab document uses `DesignKind::OneFactorAtATime`, whose validator requires that arms equal the base point plus single-level perturbations (`experiment.rs:1650–1672`). Each `LevelSpec.ref_` is the sealed definition `version_id`. `LevelSpec.overrides` carries the override spellings **for attribution only** (`experiment.rs:112–131`). No runtime override interpreter is added: the kernel opens the sealed document named by the ref. This avoids a second interpreter of `LevelSpec.overrides` (CC1). The Lab's existing per-arm parameter-difference check reads the rebinds (`experiment.rs:2008–2015`).
4. **Runner.** B2's engine loop is `next/claim/launch/settle`, with arms as `ArmSpec`s (B2 F2). An HH-native slot's participant is the kernel child process of §6.4, opened on the level's sealed definition.
5. **Declared-delta and liveness check before any paid slot** (extends D2 REQ-XD-125). Run L1.1 on the synthetic corpus for the baseline and the variant. The canonical request diff must be (a) confined to the paths the factor declares and (b) **non-empty on the factor's exercising branch**. A variant with an empty diff is refused as `inert{factor, branch}`. That turns thesis T6 into a gate: a knob that never changes model input is not a factor.

### 6.3 Recording: every result names its exact document

The `hh-experiment-slot/1` `participant` member (B2 §1.4) gains, for `kind: native` (design proposal):

```text
hir{ definition_version_id, definition_semantic_id, profile_ref + content_hash,
     bundle_id, derivation_key, hh_commit, kernel_binary_sha256,
     representation_ledger_ref, l1_report_id?, declared_delta_report_id?,
     level_assignment{factor → level_id}, override_spellings[] }
```

The slot record cites the `bundle_id` recorded in the run manifest (l.798: "recorded in the run manifest but never a coordinate of `configuration_id`"). The participant record of D2 §2.1 ("Harness, H") gets the same members.

### 6.4 HH-native participant topology inside a slot (design proposal; reconciles B2 REQ-XB-103)

B2 forbids a runner edge into `hh-embed`. An HH-native participant is therefore an **`hh-kernel` child process over binding (b)**, which is symmetric with a hosted harness process:

- **S1–S3 (provision through agent).** The slot worker spawns `hh-kernel` with `HH_STORE_ROOT` set to the slot store. The kernel child is that store's sole writer during the agent phase, so no store root has two writers (B2 F4). Over `hh-embed/1`, the worker then:
  - calls `open_session` with the sealed definition and the G22 `exec` member;
  - provisions the cell with `env.open{connection_info}`, reusing B2's helper cell-spec and egress admissions in hh-env/hh-helper, with no second provisioning path (CC1);
  - drives the session;
  - at agent end or deadline, calls `env.close{mode: "detach"}`, "the survivable form" (`envops.rs:231–239`), and lets the child exit.
- **S4–S7 (seal through teardown).** The worker reopens the slot store and re-attaches the detached cell through `EnvDriver` for seal, verify and teardown.
- **Model lane.** The model lane points at C2's token-gated loopback listener (C2 F17, adopted here). This gives one semaphore, one pin and one capture point for H and O.

The handoff crosses process boundaries on one store root (detach, then reattach). It is UNVERIFIED and is the first spike in E2-T8.

---

## Part 7 — Ticket shaping hints

Each seam is sized for one fresh-context implement-spec run. Acceptance is deterministic and offline: golden request bytes, stub argv, fixture transports. Live legs are `#[ignore = "live — ADR-0363: scripts/live-verify.sh <leg>"]`. "O goldens" means the committed outputs of D2's K3/K5 capture work. Until those land, a ticket may commit provisional goldens labelled `provenance: hand_derived` (from E1's verbatim templates), which must be replaced before G-L1. A hand-derived golden is never evidence.

| # | Seam | Contents | Prerequisite ADR / amendment | Deterministic acceptance |
|---|---|---|---|---|
| E2-T0 | **ADR and amendment bundle** (docs only) | ADR-E2-A (model port renders the compiled `ModelSurface`; durable rendering refs; G1/G3/G6/G18–G21). ADR-E2-B (exec over `hh-embed/1`, helper `stdio = merged` and `exec_defaults`, HH-native participant topology). ADR-E2-C (`hh-tmpl/1`, `json_head_tail@1`, `text_head_tail@1`, `error_format` classes). §5e.1 amendment: `stop_rule.submit{marker, line, require_returncode}`. | — | `check-build-memory.sh`, ADR index regeneration, amendment-protocol checks |
| E2-T1 | Wire fidelity in the model port | `transport = non_stream`, `stream_member`, `tool_choice = omit`, provider ids (G19), verbatim args (G20), assistant content/reasoning replay (G6), all as opt-in `model_port`/`transcript_render` members | T0 (A) | Goldens on fixture prefixes; every existing gateway/embed request golden byte-identical |
| E2-T2 | Compiled-surface rendering | `GatewayModel` armed with `ModelSurface`; `section_templates` in `lower_layout`; `tools[]` from `CompiledToolSurface`; completion-surface suppression (G4a); `error_format` and `transcript_render` executed | T0 (A), T1 | Turn-1 golden for a mini-shaped fixture; legacy path unchanged |
| E2-T3 | `hh-tmpl/1` + slot fill | Engine, strict undefined, whitespace control, ledgered uname probe | T0 (C) | Template unit goldens; refusal tests; subset parse of the pinned mini.yaml leaves |
| E2-T4 | Typed renderers + observation plumbing | `json_head_tail@1` (chars, `python_jinja_tojson`), `error_format` classes, G18 (raw output → rendered `tool` content, durable rendering ref) | T0 (A, C), T2 | M1/M2/M11/M15 goldens incl. 9 999/10 000/10 001 chars and multi-byte boundaries |
| E2-T5 | Strategy params from slots + submit marker + format-error semantics + retry policy | G9b/c, G10 detection in the gate, G11 decode, G23 | T0 (amendment) | Scripted-model fixtures M4–M10, M12–M14, M17; exact-N step-limit fixture |
| E2-T6 | Exec over `hh-embed/1` to podman cells | G22 schema member + `hh-codegen` + `check-drift.sh`; arming from definition capabilities; G17 helper `stdio = merged`, env constants, cwd, in-cell kill, timeout → rc −1 | T0 (B); B2 CELL-SPEC (WORKDIR, limits) | Drift check green; podman-stub argv; merged-order fixture; RPC arm test over binding (b); live leg ignored |
| E2-T7 | `compaction_strategy` slot wiring | `KernelCompaction.variants` from the sealed slot and params; declared `window_cap` | none | Small-window fixture compacts with `summarize-rolling`; slot absent ⇒ byte-identical |
| E2-T8 | HH-native participant in hh-runner | Kernel child per slot; detach/reattach handoff; C2 F17 loopback lane; slot `participant.hir` members | B2 RUNNER-CORE, C2 PROXY-CORE + REPLAY; T6 | Spike first: the two-process store-ownership handoff. Then a fixture slot end-to-end against a replay proxy. |
| E2-T9 | mini-swe-agent 2.4.6 HIR document + representation ledger | The document of Part 3, the profile, `hh-representation/1`, and the harness-name grep check | T1–T6 | `validate` + seal + compile; L1.1 over synthetic M1–M19 against O goldens through D2 K2/K4 (observation-replay executor, L1 runner) |
| E2-T10 | Sweep glue | `enumerate` consumer → sealed levels → `LevelSpec.ref_`; declared-delta + liveness refusal (`inert`); slot recording | T9; D2 K4; D3 registry | An OFAT fixture with one inert and one live factor: the inert one is refused, the live diff is confined to declared paths |
| E2-T11 | (execution, $0 model) mini L1 gate inputs | Hand-off to D2's K5; not an E2 build | — | D2 gate G-L1 |
| E2-T12 | Terminus: `interaction_mode = shim` + `terminus-json-plain@1` grammar | Parser, warnings, auto-fixes, keep-bad-turn, no cap | ADR-E2-T2 | Harbor `invalid_json` scenario request sequence (T1–T3, T5) |
| E2-T13 | Terminus: `terminal_session@1` executor + `terminal_incremental@1` + `text_head_tail@1` (bytes) | tmux via exec, paste-buffer path, duration clamp | ADR-E2-T1 | Stub argv; recorded-pane replay goldens (T6–T8, T12, T13) |
| E2-T14 | Terminus: completion handshake | Guard + `HarnessRule` nudge (G15) | none, or AMEND (fallback) | T4 goldens |
| E2-T15 | Terminus: `hh/handoff-summarize@1` + tokenizer decision + `model_info` | G16b, G16t | ADR-E2-T3 | Harbor summarization scenario under a synthetic `max_input_tokens` (T9/T10), or trigger-forced if the divergence route is chosen |
| E2-T16 | Terminus HIR document + representation ledger | — | T12–T15 | L1.1 over T1–T13 |

**Dependency order.** T0 → (T1 ∥ T3) → T2 → T4 → T5 → T6 → T7 ∥ T8 → T9 → T10. The Terminus block T12–T16 follows a later gate. Two tickets need the full CI mode (crates touched) plus `check-drift.sh`: T6 (schema) and T0's amendment (spec protocol). T6, T7 and T8 also need `check-removability.sh`.

---

## Findings

### F1 — The live request renderer is a second implementation beside the compiled `ModelSurface`
Claim: `GatewayModel` hand-builds messages and `tools[]` from the durable prefix and `SurfaceSpec`s. It never consumes the `ModelSurface` that `lower_profile` already compiles, with descriptions, input schemas, layout sections, error and result specs and a transcript renderer. Closing the wire gaps inside `GatewayModel` would deepen a CC1 violation of §3.2.2's single-renderer rule.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-embed/src/gateway_model.rs:684–902` (request body, transcript, tools); `crates/hh-compiler/src/lower.rs:127–410` (ModelSurface production; description 319–331; error/result 333–347; transcript renderer 393–396); `crates/hh-embed-schema/src/ops.rs:1119–1123` (`compile` stage_pending pending the `ModelProfile` codec); spec l.802 (the context builder executes the compiled renderer spec).
Retrieved: 2026-11-06T23:40Z
Implication for the spec: A rendering ADR (ADR-E2-A) and ticket E2-T2 make the compiled surface the request's source, opt-in and byte-identical when absent. G1, G3, G6–G9 and G18–G20 then close as renderer-spec features, not as `GatewayModel` special cases.
Outline delta: EXTENDS OL-05 — the representation runs through the spec's compile pipeline, not around it.
Limitations: I did not trace how `assembly_ops`/`bundle_ops` bind a profile at compile, or whether a session can obtain a compiled bundle without the pending wire codec. E2-T2 must check this.

### F2 — The spec already defines a mini-SWE-agent-class harness as HIR data plus `react/minimal`
Claim: §5e.1 defines `react/minimal` as one exec capability (`{command}` → `{output, returncode}`), two `Text` leaves, a `Budget` and a marker-on-first-line `stop_rule = submit`. AC-IR-08 and AC-R-2.6.1-2 require it to round-trip "the mini-SWE-agent-class anchor". Prompts belong in `Text` leaves, rendering choices in profile-owned surfaces, and loop code in typed-parameter variants.
Status: VERIFIED
Evidence kind: source inspection
Evidence: spec l.632–633, l.765, l.814, l.2292–2310, l.3894, l.3906, l.3933; the existing "mini-SWE-agent-class reference fixture" is a compiler test document with `edit_file`/`get_file` (`crates/hh-compiler/tests/s3_2.rs:28–60`).
Retrieved: 2026-11-06T23:38Z
Implication for the spec: Option A (§1.2) is the spec's own architecture. Phase 3 instantiates T-LCD-03 for a named upstream version, and the claim is defined as in §1.3.
Outline delta: CONFIRMS OL-05 — the target is representable in the existing ontology; EXTENDS OL-05 with the representation-ledger claim form.
Limitations: The existing anchor fixture is not mini-swe-agent 2.4.6 and proves nothing about it.

### F3 — In the live exec path the model never sees command output
Claim: An armed exec effect produces a `context.observation.recorded` row only for a `flow_contract` capability or a leaf refusal. The row carries a capture-manifest reference and labels, not output. `GatewayModel` sends that record's canonical JSON, or `{"kind":"no_observation_recorded"}`, as the `tool` message content.
Status: VERIFIED (code path); the end-to-end effect is UNVERIFIED (no test drives a live model over armed exec)
Evidence kind: source inspection
Evidence: `crates/hh-env/src/dispatch.rs:2823–2856`; `crates/hh-env/src/events.rs:673–702`; `crates/hh-env/src/capture.rs:95–103` (output chunks ephemeral; manifest holds `raw_output_ref`); `crates/hh-embed/src/gateway_model.rs:741–812`; `crates/hh-gateway/src/openai.rs:103–113`; `crates/hh-embed/tests/r5_8.rs:243` (`flow_contract: None`, scripted model).
Retrieved: 2026-11-06T23:42Z
Implication for the spec: G18 must close before any HH-native trial, paid or not. The model-view rendering becomes a durable, content-addressed record so that replay re-serves it.
Outline delta: EXTENDS OL-05 — a gap E1 did not list (E1 G8 assumed the payload was the output).
Limitations: I did not read the capture-manifest builder, so whether `raw_output_ref` holds interleaved stdout/stderr bytes or per-stream blobs is unverified.

### F4 — Tool-call ids and argument bytes are rewritten on replay
Claim: The decoder mints kernel tool-call ids (`<model_call_id>.tcN`), keeping the provider id only as an alias. The transcript replays the kernel id and replays arguments as the canonical re-serialization of the parsed JSON. Both differ from mini-swe-agent, which re-sends the provider's id and argument string.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-gateway/src/grammar.rs:834–837`; `crates/hh-gateway/src/message.rs:126–138`; `crates/hh-embed/src/gateway_model.rs:765–799, 1035`; D2 §1.5 ("Tool-call ids … a rewrite by H is a real wire difference").
Retrieved: 2026-11-06T23:43Z
Implication for the spec: G19 and G20 are additional L1-breaking gaps from request 2 on. They close through `transcript_render` choices and a raw-argument member.
Outline delta: EXTENDS OL-05.
Limitations: Whether glm-5.3-flash's argument strings ever differ from their canonical form is empirical. If they never do, G20 is latent, but L1 still needs the guarantee.

### F5 — Strategy parameters are hard-coded although the slot-param channel already exists
Claim: Both arm paths construct `StrategyParams::default()`, with a `plan_execute` tweak only. HIR `SlotBinding.params` exists, the `model_port` slot's params are already read at arm, and HIR budget dimensions already reach the driver's hard ceilings. The closure is a decode, not a new channel.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-embed/src/open.rs:2579–2583, 2728–2732, 3036–3066`; `crates/hh-control/src/strategy.rs:166–253`; `crates/hh-hir/src/records.rs:540–549`; `crates/hh-embed/src/gateway_model.rs:483–500`; `crates/hh-control/src/driver.rs:831–837`.
Retrieved: 2026-11-06T23:39Z
Implication for the spec: E2 resolves E1 G11's "did not verify whether HIR Budget reaches them": it does, for `model_calls`, `turns`, `retries` and `time.*`. mini's step limit maps to `model_calls.hard`, and step limit 0 maps to the dimension's absence.
Outline delta: EXTENDS OL-07 — step and format-error limits are typed slot parameters, sweepable once decoded.
Limitations: The exact "N permits exactly N calls" semantics under `remaining ≤ 0` is inferred, not run. E2-T5 tests it.

### F6 — The submit marker is documented but unimplemented, and its literal has no declared home
Claim: Submission is detected only for the `hh.submit` surface. `StopRuleParam::Submit` is documented as marker-on-observed with rc 0, but no code detects a marker. The spec's MUST-data parameter list has no member for the marker literal.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-embed/src/runtime.rs:415–424`; `crates/hh-control/src/strategy.rs:193–206`; a grep for `COMPLETE_TASK|submit_marker` in `hh-control/src` and `hh-embed/src` finds nothing; spec l.3894, l.3906.
Retrieved: 2026-11-06T23:41Z
Implication for the spec: An additive §5e.1 amendment, `stop_rule.submit{marker, line, require_returncode}`, through an ADR. The marker is harness data and must not be a kernel constant. This is the only canonical-spec amendment that stage one needs.
Outline delta: EXTENDS OL-05.
Limitations: ADR-0104's text is not in `docs/adr/` (the series starts at 0217). It may already fix a literal, which would make the amendment a parameterisation of it.

### F7 — Non-streaming transport and `tool_choice` control are small, already-admitted changes
Claim: `stream = true`, `stream_options` and `tool_choice: "auto"` are render-time constants. The transport already reads non-streaming `chat.completion` bodies, and `tool_choice` is already an admitted plan key.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-embed/src/gateway_model.rs:695–709, 911–932`; `crates/hh-gateway/src/openai.rs:49–68, 472`; `crates/hh-gateway/src/http_transport.rs:599–602, 730–745`; `crates/hh-gateway/src/dialects.rs:115–128`.
Retrieved: 2026-11-06T23:40Z
Implication for the spec: G4 and G5 close with opt-in `model_port` members, so D2 rule C05 stays unused. The proxy's `tool_choice:auto` strip (C2) remains as a defence, not as the fix.
Outline delta: EXTENDS OL-05.
Limitations: Whether LiteLLM sends `stream: false` or omits `stream` is unobserved (E1 F7). The O bytes decide `stream_member`.

### F8 — Exec semantics differ from `subprocess` in four checkable ways
Claim: The helper's `command` path is `/bin/sh -c`, matching `shell=True`. It differs from mini's `LocalEnvironment` in four ways: separate stdout/stderr pipes rather than one fd, cwd defaulting to `"."` under `podman exec --workdir`, environment fixed at executor construction, and a deadline kill that targets the host-side `podman exec` process group.
Status: VERIFIED (code); UNVERIFIED (the runtime effect of the kill path and the observable interleaving)
Evidence kind: source inspection + inference
Evidence: `crates/hh-helper/src/server.rs:645–647`; `crates/hh-helper/src/exec.rs:430–452`; `crates/hh-helper/src/podman.rs:146–158`; `crates/hh-env/src/helper.rs:552–565`; E1 A.4.
Retrieved: 2026-11-06T23:44Z
Implication for the spec: ADR-E2-B adds helper `stdio = merged`, capability `exec_defaults` (env, cwd, timeout) and an in-cell kill. The live leg and D2's M19/M11 branches check them.
Outline delta: EXTENDS OL-05; resolves E1 G17's "I did not verify".
Limitations: Not executed. The podman exec signal semantics are standard behaviour I did not test.

### F9 — No `hh-embed/1` path arms exec, and B2's layering forces a kernel-child participant
Claim: `arm_exec_capabilities` is a Rust method used only by one test. `open_session` carries only param-less host capabilities. Because B2 forbids a runner edge into `hh-embed`, an HH-native slot must run `hh-kernel` as a child process over binding (b). Cell ownership then moves from the kernel child (agent phase) to the worker (seal and verify) via the existing `env.close{mode: detach}`.
Status: PARTIALLY VERIFIED (the absence of arming and the existence of the detach/attach ops are verified; the topology is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-embed/src/service.rs:1571–1604`; grep `arm_exec_capabilities` (three files); `crates/hh-embed-schema/src/types.rs:586–592`; `crates/hh-embed-schema/src/ops.rs:1051–1117`; `crates/hh-embed/src/envops.rs:231–345`; B2 REQ-XB-103, F4.
Retrieved: 2026-11-06T23:45Z
Implication for the spec: An `hh-embed-schema` change (CC7: schema → `hh-codegen` → drift check) plus ADR-E2-B. The cross-process handoff is the first spike of E2-T8.
Outline delta: EXTENDS OL-05; EXTENDS OL-06 — the participant identity for H includes the kernel binary digest.
Limitations: Whether `env.attach` can reattach a detached handle from a different process on the same store root was not established.

### F10 — Logic-bearing templates should become typed renderers; only prose needs a template subset
Claim: mini's observation and format-error templates encode thresholds, slicing, JSON escaping and branches. Typed `ResultRenderSpec`/`error_format` records reproduce them while making the constants sweepable, which the T3 ceiling demands. The system and instance templates need only substitution, an equality conditional and Jinja whitespace control, with no new dependency.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal over source inspection
Evidence: spec l.624 (ceiling T3), l.798 (purity), l.2292–2310 (rows 6–8); E1 A.3 and A.5 templates; `crates/hh-hir/src/leaves.rs:25–38` (`Text`); `crates/hh-compiler/src/lower.rs:319–347`.
Retrieved: 2026-11-06T23:46Z
Implication for the spec: ADR-E2-C covers `hh-tmpl/1`, `json_head_tail@1` and `error_format` classes, golden-tested against O's recorded bytes (D2 census and M-branches). My description of `tojson` escaping is recollection, so it is a test obligation and not a fact.
Outline delta: EXTENDS OL-05; EXTENDS OL-07 (truncation constants become typed factors).
Limitations: I did not read the full pinned mini.yaml or Jinja2's source. The subset may need extending once the pinned template is parsed.

### F11 — The compaction family exists but is unwired; the slot is registered and ignored
Claim: `compaction_strategy` is a catalog class, and six C1 compaction variants are implemented. The kernel's `KernelCompaction` ladder is constructed with no variants ("empty at C0"), and `hh-embed` never reads the `compaction_strategy` slot.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-embed/src/runtime.rs:2111–2134`; `crates/hh-embed/src/service.rs:1765–1768`; `crates/hh-assembly/src/catalog.rs:40–49`; `crates/hh-context/src/compact_family.rs:300–313`; greps for consumers and for `compaction_strategy` in `hh-embed/src` (none).
Retrieved: 2026-11-06T23:43Z
Implication for the spec: E2-T7 wires the slot (generic). A compaction sweep is admissible only under a declared small window (T6), and the declared-delta liveness check (§6.2 step 5) refuses it otherwise.
Outline delta: EXTENDS OL-07.
Limitations: I did not assess whether `compact_model_owned` fits Terminus's handoff (E1 G16). I propose a dedicated variant instead.

### F12 — The sweep vocabulary exists on both sides; only the glue is missing
Claim: hh-lab validates `one_factor_at_a_time` designs and rebinds per-level params from `LevelSpec.overrides`. hh-assembly's `space`/`enumerate` produces validated override points with no production consumer.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-lab/src/experiment.rs:112–131, 1650–1672, 2008–2015`; `crates/hh-assembly/src/space.rs:1–21, 42–51, 101–129, 207–218, 260–266`; grep (only `tests/s3_2.rs:19` calls `enumerate`).
Retrieved: 2026-11-06T23:42Z
Implication for the spec: Levels are sealed documents materialized by `enumerate`, with `LevelSpec.ref_` set to `version_id` and overrides kept for attribution only. There is no runtime override interpreter (CC1), and every result names its document hash (§6.3).
Outline delta: EXTENDS OL-07 — OFAT explore runs over sealed, identity-bearing variants.
Limitations: Whether `lab.assembly.*` ops already expose `enumerate` over `hh-embed/1` was not checked. E2-T10 picks the reuse path.

### F13 — A `context_policy` factor may be inert on the wire today
Claim: The `context_policy` slot is bound at run time, and the fold assembles a plan with it. But `GatewayModel`'s transcript reads only the latest `retained_selection/1` from compaction rows, not the assembled plan. A context-policy change may therefore never change request bytes.
Status: PARTIALLY VERIFIED (both code paths read; the end-to-end effect was not run)
Evidence kind: source inspection + inference
Evidence: `crates/hh-context/src/policy_family.rs:60–85`; `crates/hh-embed/src/runtime.rs:895–930`; `crates/hh-embed/src/gateway_model.rs:44–66, 724`.
Retrieved: 2026-11-06T23:44Z
Implication for the spec: Context-policy factors wait for G21. The liveness check catches them if they are registered earlier.
Outline delta: EXTENDS OL-07 (T6).
Limitations: The assembled plan may influence the wire through a path I did not read.

### F14 — Where behaviour lives: generic features plus one document per harness; the claim reports its locus split
Claim: Option A is the only option that reaches the rendering gaps, keeps sweep knobs typed and honours CC1. The claim is precise when stated as L1 pass plus a representation ledger with counts per locus class, plus a CI check that no harness-named kernel code exists.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: §1.2–1.3; spec l.632; l.3885 (I2 strategy purity); `plugins/` contents; `crates/hh-control/src/react.rs:1327–1342` (closed variant table).
Retrieved: 2026-11-06T23:46Z
Implication for the spec: The publication text for phase 3 cites the ledger counts. A non-zero `harness_variant` share (Terminus) is disclosed, not hidden.
Outline delta: EXTENDS OL-05 — defines what "represent in HIR" means as a checkable claim.
Limitations: The grep-level check is a heuristic; review remains the backstop.

### F15 — Terminus-2 needs no new kernel terminal primitive, but needs a pre-tier interaction mode
Claim: Terminus reaches its container only via exec of tmux commands, so HH can do the same through a helper-backed executor. Its text-JSON protocol is §5b.3's `shim` (or `freeform`) mode, which "C0 admits … native_fc only". Its handoff summarization and token counting need a dedicated compaction variant and a tokenizer decision.
Status: PARTIALLY VERIFIED (the spec, the code absences and E1's anatomy are verified; the closure is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: spec l.814, l.2297; `crates/hh-gateway/src/message.rs:136–137`; grep for tmux/pty/tokenizer (none); `crates/hh-embed/src/runtime.rs:761` (estimator `est/canonical_bytes_div4@1`); E1 B.1–B.5; D2 F14.
Retrieved: 2026-11-06T23:45Z
Implication for the spec: Three Terminus ADRs (terminal executor, `shim` admission, handoff plus tokenizer). The parser grammar and the handoff variant are `harness_variant` loci.
Outline delta: EXTENDS OL-05.
Limitations: tmux key semantics and the incremental-diff quirk remain inferred (E1 F5, F11).

### F16 — Staging: Terminus-2 belongs in a later, gated block, not the first Round-6 build
Claim: About six extra build tickets, three ADRs, a separate D2 cohort and the open tokenizer question make Terminus-2 a separate block. Its value depends on seams that mini-swe-agent proves first.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: Part 5; D2 §5.3; E1 F14; Part 7 counts.
Retrieved: 2026-11-06T23:46Z
Implication for the spec: H1 sequences E2-T12…T16 behind mini's G-L1 (and preferably L2). The `shim` ADR may land earlier as docs only.
Outline delta: CONFIRMS OL-05 (A-8 staging).
Limitations: Sizing is by seam count, not measured effort.

### F17 — Pre-existing request semantics already match some mini behaviours by accident; they must become declared
Claim: `react/minimal` already counts no-intent replies toward the format-error streak and stops at 3. `GatewayModel` already drops text-only assistant turns. Both match mini today, but by default rather than by declaration. A later default change would silently break L1.
Status: VERIFIED (code); the match to mini rests on E1's account
Evidence kind: source inspection
Evidence: `crates/hh-control/src/react.rs:14`; `crates/hh-embed/src/gateway_model.rs:783–785`; E1 A.2, F4.
Retrieved: 2026-11-06T23:43Z
Implication for the spec: The document declares `on_format_error = omit_assistant` and the streak parameters explicitly. Mutation tests pin them.
Outline delta: EXTENDS OL-05.
Limitations: mini's handling of `finish_reason: length` *with* tool calls (execute or fail) versus HH's `on_truncated_response = fail_calls` (the only value, `strategy.rs:211–216`) needs a read of mini's parser at implementation time. A divergence may need a new `on_truncated_response` value, which would be an ADR.

### F18 — The mini HIR document must be human-authored, not imported
Claim: Imported content is `authority = unverified`, and `Text` leaves render only into roles at or below `role_map(authority)`. A document lifted from mini.yaml could not place its system template in the `system` role. The document must be authored at `definition` authority, with `derived_from` citing the mini.yaml digest.
Status: VERIFIED (spec rules); the role consequence is an inference from them
Evidence kind: source inspection + inference
Evidence: spec l.699 (import ⇒ unverified), l.752 (§3.1.11 role-placement invariant), l.633.
Retrieved: 2026-11-06T23:47Z
Implication for the spec: The representation ledger records the human author and reviewer. That is also what makes the "HH-native" label honest: someone vouches for the transcription, and L1 checks it.
Outline delta: EXTENDS OL-05.
Limitations: I did not read the `role_map` defaults for `unverified`.

### F19 — E1's "rendered system template ends with a newline" needs the census, not reasoning
Claim: The YAML block scalar yields a trailing newline, but Jinja's default may strip one at render. The final byte of request 1's system message is therefore not determinable from the sources available to this row.
Status: UNVERIFIED (possible correction to E1 A.3)
Evidence kind: inference
Evidence: E1 A.3 ("It is a YAML block scalar, so the rendered text ends with a newline"); Jinja default behaviour from recollection, not read.
Retrieved: 2026-11-06T23:47Z
Implication for the spec: The `hh-tmpl/1` whitespace rules are fixed against D2's turn-1 census bytes on the dev partition. The census is the binding golden for every template.
Outline delta: EXTENDS OL-06 (the L1 golden source).
Limitations: This could be wrong in either direction until one O request is recorded.

---

## Answer and completion assessment

**Answer.**
- **Architecture.** Close the gaps as **generic kernel features plus one HIR document (with a neutral profile) per harness** (§1.2, F14). The spec already prescribes this architecture and already names a mini-SWE-agent-class anchor (F2).
- **Gap count.** Seventeen E1 gaps plus six new ones (G18–G23) have closures in Part 2. Fifteen of the 23 are request- or observation-rendering gaps, and they close by making the model port render the compiled `ModelSurface` (G21, F1).
- **The worst new gap.** In the live exec path the model never sees command output (G18, F3).
- **Spec changes for stage one.** Exactly **one canonical-spec amendment** (`stop_rule.submit{marker}`, F6), **three ADRs** (A rendering, B exec plus participant topology, C templates and renderers) and **one `hh-embed` schema change** (exec arming, G22, CC7).
- **The mini document and templates.** Part 3 sketches the mini-swe-agent document. Templates render through a std-only `hh-tmpl/1` subset for prose and typed renderers for logic-bearing templates, golden-tested against O's recorded bytes (Part 4).
- **Terminus-2.** It needs a helper-backed tmux executor, a `shim` interaction mode, a handoff compaction variant and a tokenizer decision. It belongs in a **later gated block** (Part 5, F16).
- **Sweeps.** They become real through slot-param decoding, typed renderers and the compaction-slot wiring. Lab OFAT designs bind to sealed `enumerate`d documents, recorded by HIR hash and gated by a declared-delta liveness check (Part 6).

**Settled.** The verified state of every E1 gap at 09fe8523, including G11's budget path (F5) and G17's exec differences (F8). The six new gaps. The architectural home. The amendment, ADR and schema count. The ticket order.

**Acceptable uncertainty for this row:**
- what LiteLLM actually puts on the wire (`stream`, re-sent assistant members, argument spelling; F7, G6, G20);
- the Jinja whitespace and `tojson` details (F10, F19);
- the cross-process cell handoff (F9);
- the context-policy wire effect (F13);
- the tokenizer route.

Each is resolved by a named ticket's golden or spike, or by D2's dev partition.

**Technical blockers** (all owned by named tickets):
- G21 (rendering) gates almost everything;
- G22 needs a schema change;
- G10 needs the amendment;
- E2-T8 needs B2 RUNNER-CORE and C2 PROXY-CORE/REPLAY.

## Open questions

1. **Terminus-2 staging.** Owner: operator via H1. Affects H1, D2 (K10–K14) and D3. **Operator decision: yes.** Is stage two a later gated block (recommended, F16) or part of the first Round-6 build?
2. **Q-X29 interplay.** Owner: operator at ratification. Affects E2-T9. **Operator decision: yes (already pending).** The Part 3 document encodes Harbor's in-agent defaults (no step or cost cap, external wall and spend caps) per D2 R4. A different ruling changes only `Budget` and the `stop_rule` parameters.
3. **Tokenizer for Terminus triggers (G16t).** Owner: E2-T15 with D3. **Operator decision: only if** a vocabulary-data port ADR is contested. The route is a port only if D3 pre-registers a threshold sweep; otherwise it is a declared divergence plus a trigger-forced L1.
4. **Surface home: authored surface records with a neutral profile (recommended) versus a harness-named profile.** Owner: H1/O2. Operator decision: no.
5. **The declared-divergence budget for mini.** Owner: D2/E2. Operator decision: no. Is a Python `json.loads` error-text divergence (M5) acceptable inside an "L1 pass", or must the renderer reproduce the common messages?
6. **Placement of `hh-tmpl/1`** (compiler vs context crate). Owner: E2-T3 under CC1. Operator decision: no.
7. **The cross-process detach/attach handoff** (F9). Owner: H1 with B2. Operator decision: no. If the spike fails, the fallback is to relax B2's REQ-XB-103 to allow an in-process binding (a) for HH-native slots only. That conflicts with B2 and needs H1's ruling.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale / findings | OL | Acceptance |
|---|---|---|---|---|
| REQ-XE-101 | An HH-native harness SHALL be expressed as a sealed HIR/1 document bound to a declared profile. Harness-specific choices SHALL be data (`Text` leaves, surface records, profile rules, slot params) or registered variants with typed params. No kernel code path SHALL name a harness. | F2, F14 | OL-05 | Harness-name grep check in CI; review |
| REQ-XE-102 | Every HH-native representation SHALL ship a representation ledger `hh-representation/1` assigning each E1 anatomy item to `data`, `generic_variant`, `harness_variant` or `declared_divergence`. Published claims SHALL cite its counts. | F14 | OL-05, OL-06 | The ledger covers every E1 item; a claims-language check |
| REQ-XE-103 | When a session is armed with a compiled `ModelSurface`, the model port SHALL render the request (layout sections, `tools[]`, transcript, error and result renderings) from that surface. With none armed it SHALL render byte-identically to the pre-change path. | F1 (G21) | OL-05 | Two-path golden tests; existing request goldens unchanged |
| REQ-XE-104 | `prompt_layout.section_templates` SHALL place specific `Text` leaves into specific sections with declared provider roles, including a `system` role. | G1 | OL-05 | Turn-1 golden with system + user messages |
| REQ-XE-105 | `tools[]` SHALL carry the compiled description and the capability `input_schema` verbatim, including member descriptions. | G3 | OL-05 | Golden equal to mini's `BASH_TOOL` |
| REQ-XE-106 | The completion surface SHALL be exposed only when the definition declares it or its `stop_rule` requires it. `tool_choice` SHALL be sent only when declared. | G4 | OL-05 | Goldens with and without; legacy unchanged |
| REQ-XE-107 | The `model_port` slot SHALL declare `transport ∈ {stream, non_stream}` and the `stream` member's presence. Non-streaming SHALL use the existing completion decoder. | G5, F7 | OL-05 | Non-stream golden and decode test |
| REQ-XE-108 | `transcript_render` SHALL declare assistant replay (`content`, `reasoning`, `extra_members`), text-only-turn handling, format-error-turn handling, `id_normalisation` and verbatim tool arguments. Replayed bytes SHALL come from durable `model_io` blobs. | G6, G7, G19, G20, F4, F17 | OL-05 | M18, M4–M10 goldens; a spaced-arguments round trip |
| REQ-XE-109 | Every armed exec observation SHALL produce a model-view rendering. The rendering is applied from the capability's `ResultRenderSpec` to the raw output, and its content address is recorded durably before the request that carries it. | G18, F3 | OL-05 | Live-model-over-replay fixture; kill-point test |
| REQ-XE-110 | `ResultRenderSpec` SHALL include `json_head_tail@1{threshold, head, tail, unit ∈ {chars, utf8_bytes}, fields, json_style}` and `text_head_tail@1`, whose constants are typed parameters. | G8, F10 | OL-05, OL-07 | M1/M2/M15 goldens against O bytes |
| REQ-XE-111 | `error_format` SHALL render typed format-failure classes from `Text` leaves with slots. Batch-scope failure SHALL be declarable. | G9 | OL-05 | M4–M8 goldens |
| REQ-XE-112 | Prompt templates SHALL use `hh-tmpl/1` (substitution, `==`-literal conditionals, Jinja whitespace control, strict undefined) with no new dependency. Templates outside the subset SHALL fail compile as `UnexpressibleSurface`. | G2, F10 | OL-05 | Unit goldens; refusal tests; subset parse of the pinned templates |
| REQ-XE-113 | Template slot values that depend on the environment (uname) SHALL be filled at run time from a ledgered in-cell probe, never at compile time. | G2; spec l.798 | OL-05, OL-06 | Purity test; probe rows present; C04 cross-check |
| REQ-XE-114 | Template and renderer acceptance goldens SHALL be O-recorded bytes (D2 census and corpus). Hand-derived goldens SHALL be labelled and replaced before G-L1. | F10, F19 | OL-06 | Golden provenance check |
| REQ-XE-115 | `control_strategy` slot params SHALL decode into `StrategyParams` through the existing wire codec at arm and resume. Unknown members SHALL refuse. | G11, F5 | OL-05, OL-07 | Decode, refusal and resume-parity tests |
| REQ-XE-116 | `stop_rule = submit{marker, line, require_returncode}` SHALL be detected by the dispatch gate on the settled raw output (canonical-spec amendment of §5e.1). | G10, F6 | OL-05 | M12/M13 fixtures |
| REQ-XE-117 | The envelope's transport-retry policy (attempts, retryable classes) SHALL be declarable per definition, and the request count SHALL be reproducible. | G23 | OL-05 | M17 fixture |
| REQ-XE-118 | `open_session` SHALL accept an optional `exec` member that arms the definition's exec capabilities over the session's environment handle with the existing `bind` check. The member is added in `hh-embed-schema` and regenerated (CC7). | G22, F9 | OL-05 | `check-drift.sh`; RPC arm test; refusal test |
| REQ-XE-119 | Exec capabilities SHALL declare `exec_defaults{timeout_ms, stdio, cwd, env}`. The helper SHALL support merged stdio and an in-cell process-tree kill. A timeout SHALL render as `returncode = -1` plus the declared exception text. | G17, F8 | OL-05 | Podman-stub argv; merged-order fixture; live leg (ignored, ADR-0363) |
| REQ-XE-120 | An HH-native slot SHALL run `hh-kernel` as a child participant process over binding (b), the sole writer of the slot store during the agent phase. Cell custody SHALL pass to the slot worker by `env.close{detach}` and re-attach. | F9; B2 REQ-XB-103/104 | OL-05, OL-06 | Two-process ownership spike; fixture slot end to end |
| REQ-XE-121 | HH-native model lanes SHALL route through the C2 proxy's loopback listener (C2 F17), with the same pins and capture points as O. | C2 F17; D2 F8 | OL-06 | A lane-config check; proxy receipts carry the native call id |
| REQ-XE-122 | The `compaction_strategy` slot SHALL populate the kernel compaction ladder with the bound variant and params. An absent slot SHALL be byte-identical to today. | G16a, F11 | OL-07 | Small-window fixture; legacy unchanged |
| REQ-XE-123 | The mini-swe-agent 2.4.6 document SHALL be human-authored at `definition` authority, with `derived_from` citing the mini.yaml digest. It SHALL encode the reference configuration ratified under Q-X29. | F18; E1 F1; D2 R4 | OL-05 | Document validates and seals; provenance check |
| REQ-XE-124 | OFAT sweep levels SHALL be sealed documents materialized by `hh-assembly::enumerate`, with `LevelSpec.ref_` = `version_id` and override spellings kept for attribution only. No runtime override interpreter SHALL be added. | F12 | OL-07 | Enumerate-to-Lab fixture; identity check |
| REQ-XE-125 | Before any paid sweep slot, each variant SHALL pass a declared-delta and liveness check: its L1.1 request diff against the baseline is confined to declared paths and non-empty on the factor's exercising branch. Otherwise the variant is refused as `inert`. | F12, F13; D2 REQ-XD-125 | OL-07 | Fixture with one inert and one live factor |
| REQ-XE-126 | Every HH-native slot record SHALL carry `participant.hir{definition_version_id, semantic_id, profile hash, bundle_id, derivation_key, hh_commit, kernel_binary_sha256, representation_ledger_ref, l1_report_id?, level_assignment, override_spellings}`. | §6.3 | OL-07, OL-06 | Schema check; a recompute from the store |
| REQ-XE-127 | A compaction or context-policy factor SHALL be admissible only under a declared condition that exercises it (e.g. a declared `window_cap`), recorded in the pre-registration. | F11, F13; T6 | OL-07 | Pre-registration check; liveness check |
| REQ-XE-128 | Terminus-2's terminal SHALL be a helper-backed `terminal_session@1` executor issuing tmux commands in the cell, with Harbor's quoting, paste-buffer threshold, 60 s wait clamp and capture semantics. No kernel pty primitive SHALL be added. | G13, F15 | OL-05 | Stub argv; recorded-pane goldens |
| REQ-XE-129 | Terminus-2's text protocol SHALL be `interaction_mode = shim` with the `terminus-json-plain@1` parse grammar as a `CompiledPayload`, admitted by ADR ahead of its tier. | G14, F15 | OL-05 | `invalid_json` scenario replay |
| REQ-XE-130 | Terminus-2's completion handshake SHALL be an envelope `stop` guard plus a `HarnessRule` nudge carrying the verbatim confirmation text, unless O2 requires the parameter route (amendment). | G15 | OL-05 | T4 goldens |
| REQ-XE-131 | Terminus-2's summarization SHALL be the `hh/handoff-summarize@1` compaction variant with typed thresholds and prompt leaves. Its token counting SHALL be either a declared port or a declared divergence with trigger-forced L1. | G16b, G16t | OL-05, OL-07 | Summarization scenario (T9/T10) |
| REQ-XE-132 | The Terminus-2 build block SHALL be scheduled after mini-swe-agent's G-L1 pass, behind its own gate. | F16 | OL-05 | Manifest order review |

## Handoff

**Dependencies resolved.**
- E1: every G1–G17 re-verified at 09fe8523. G11's budget path is resolved (F5) and G17's exec semantics are specified (F8).
- B2: the runner topology is consumed and REQ-XB-103 honoured (F9).
- C2: F17's native-through-proxy proposal is adopted (REQ-XE-121), and the replay interface is consumed through D2.
- D2: the L1 surface, rules C01–C07, M/T branches, census and REQ-XD-125 are consumed. Rule C05 becomes unused once G5 closes.

**Supplies:**
- **D2:** observation rendering is a durable model-view record (G18), which is what K2's observation-replay executor must feed. Ids and arguments become verbatim (G19, G20). A possible divergence on M5's Python error text.
- **D3:** the §6.1 factor table, the liveness gate (REQ-XE-125), the window-condition rule (REQ-XE-127) and the tokenizer dependency.
- **B2:** `exec_defaults`/`stdio = merged` in the helper alongside CELL-SPEC, and the kernel-child participant topology (E2-T8).
- **C4:** none directly. HH-native runs need no install hook.
- **H1:** three ADRs (E2-A/B/C) plus three Terminus ADRs, one §5e.1 amendment, one schema change, and the ticket order of Part 7.

**Proposed follow-up rows.** None. The cross-process handoff spike is the first step of E2-T8, not a research row.

**Conflicts with other notes:**
- **E1 A.3** says the system template "ends with a newline". That is unverifiable from available sources and is deferred to the census (F19).
- **E1 G8** assumed the tool result is the recorded output. It is a ledger record (F3).
- **D2 F8** treats `tool_choice` as neutralised by the proxy. This note also removes it downstream (G4b) so that the diagnostic downstream comparison stays clean. That is complementary, not contradictory.
- **B2 REQ-XB-103** is preserved, but only if the F9 spike succeeds. Otherwise H1 must rule (open question 7).

**Capability and portfolio dispositions.** None changed. Nothing in this note is fixture-, live- or foreign-run-verified.
