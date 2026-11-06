# 36_hir-parity-method — Layered HIR parity method
Stream: D
Researched: 2026-11-06 (inspection and computation 2026-11-06T23:07Z–23:22Z)
Author: Claude Code design worker (Opus 5.5), row D2 of the experiments research ledger
Ledger rows covered: D2
Repository baseline: main 09fe8523. Read from worktree `claude/experiments-spec` (HEAD c12b96b1 at writing; it differs from main only under `docs/`). `docs/design/33_model-mediation.md` (C2) and `docs/research/25_tb21-harbor.md` (B1) did not exist when this note was written, so the proxy and Terminal-Bench 2.1 facts they will own are consumed here as stated interface assumptions, not as cited findings.
Confidence overall: medium. The constraints taken from landed code, the spec and the accepted notes are high-confidence. The precision tables are exact computations under stated assumptions. The method itself is a design proposal: it is UNVERIFIED until the validations listed per finding are run.

## Scope, inputs and method

D2 designs the method that would admit an HH-native HIR representation of mini-swe-agent (first) and Terminus-2 (second, per Q-X13) as equivalent to the original harness. The participant is `glm-5.3-flash` on Ollama Cloud, called through the HH model proxy, and the tasks are Terminal-Bench 2.1. The method has three layers, per Q-X14 "Layered: conformance + verdicts":

- **L1:** deterministic request-level conformance at $0 model spend.
- **L2:** paired task-verdict KL parity, paid and stochastic.
- **L3:** descriptive trace shape.

This note does not design the proxy (C2), the HIR gap closures (E2), the experiment runner (B2) or the compute backend (B3). It states what L1 and L2 need from each of them.

Inputs read in full:

- the brief, including addenda A-4 and A-8;
- the ledger: row D2, §Q (Q-X13/14, pending Q-X22/Q-X29), the Execution contract and the standing limits;
- `docs/research/CONVENTIONS.md`;
- `docs/research/28_reference-harness-anatomy.md` (E1);
- `docs/research/26_ollama-glm.md` (C1);
- `docs/research/24_executed-record-audit.md` (A1);
- `docs/evidence-and-adoption-spec.md` §4.4–§4.5 (lines 282–341);
- `crates/hh-eval/src/parity_admission.rs` (whole file, 1 306 lines);
- `crates/hh-eval/src/directional_kl.rs`: header, types, `binary_endpoint`, `verdict`, the conjunction rule at l.1599–1622;
- `crates/hh-eval/src/stats.rs`: `kl_directional_bound`, l.492–540;
- `crates/hh-eval/src/benefits.rs`: `RegisteredMargin`, `registered_margins`, l.355–391;
- `crates/hh-eval/src/central_protocol.rs`: l.200–220 and 630–652;
- `crates/hh-analysis/src/kernel.rs`: the routes at l.1909–2115 and l.2117–2392.

Read in part: `docs/research/27_harness-survey.md` (C3), F2 and its TB2.1 table (l.55–100), for the per-harness rerun-flip and $/trial priors.

Local computations, all with no network and no repository execution, were run with `python3 -I` from the session scratchpad:

- `kl.py`: a float reimplementation of `K_η` and the TOST combination;
- `tables.py` and `t2c.py`: the precision and power tables;
- `pilot.py`: a simulation of how precisely the pilot can estimate the flip rate.

The formulas are given inline where each table appears. Before using `kl.py`, I validated it against every number published in spec §4.5. All seven match to six decimals:

- {.308497, .115703, .024293} at zero discordance for N = 10, 30, 150;
- {.042944, .065355, .100601, .190899} at N = 150 with equal directional counts 1, 3, 8 and 38.

The Rust implementation additionally rounds outward to integer ppm. That can only widen an interval, by ≤ 1 ppm per endpoint.

---

## Part 1 — L1 request-level conformance

### 1.1 What L1 claims

L1 claims the following. Suppose the HH-native harness (**H**) is fed the same model responses and the same environment observations as the unmodified original (**O**). Then it sends the model byte-identical requests after a declared canonicalization, at every step, in the same order and number. It performs the same parsed actions and ends with the same termination class and submission. This holds on a frozen corpus whose branch coverage is enumerated.

L1 is deterministic: one failure is a fail. It is a **precondition gate**, not parity evidence, and the landed admission code agrees (F1).

### 1.2 Observation identity: replay observations, and also execute on a measured stable prefix

**The problem.** Actions really execute. A recorded response sequence replayed against a fresh container yields observations that depend on container state, time, hostnames, PIDs and scheduling (E1 F11). Once an observation differs, every later request differs, so comparing requests after the first environmental divergence is meaningless. Two pure options exist, and neither is sufficient alone:

- *Execute in identical fresh cells only.* This tests HH's exec semantics, which E1 lists as gap G17: merged stdout/stderr, the 30 s timeout with partial output, the `PAGER=cat` environment and cwd. But it confounds harness divergence with environment nondeterminism.
- *Replay observations only.* This makes the comparison exact, but it never exercises H's executor. It also cannot be applied to the original without modifying it, because mini-swe-agent's `LocalEnvironment` runs `subprocess` inside the container (E1 A.4).

**Decision (design proposal).** L1 has three legs. Each runs per response sequence *S*. A sequence is a natural recording or a synthetic fixture (§1.4).

| Leg | O side | H side | Compared | Gate semantics |
|---|---|---|---|---|
| **L1.0 self-replay control** | O run twice against REPLAY(*S*) in fresh cells from the same image on the L1 host. For a natural *S*, the RECORD run counts as one of the two. | — | O-run *a* vs O-run *b*, request by request | Not a gate. It defines the **replay-stable prefix** *s(S)*: the number of leading requests on which the two O runs are canonically equal. A divergence locates environment nondeterminism, never an H fault. |
| **L1.1 construction conformance** | the reference request sequence of O on *S* (from L1.0's run *a*) | H with REPLAY(*S*) responses **and** an observation-replay executor fed O's recorded raw observations | every request of the whole trajectory, plus actions and termination | **Gate.** First divergence = fail. |
| **L1.2 executed conformance** | the same reference | H with REPLAY(*S*) responses and **live execution** in a fresh HH cell from the same task image | requests 1…*s(S)*+1. Request *s(S)*+1 is the first request that carries a post-divergence observation; it is excluded when *s(S)* < total. | **Gate on the stable prefix.** A divergence at k ≤ *s(S)* = fail. Beyond it, the result is reported but not evaluated. |

Rationale:

- L1.1 isolates what HIR represents: templates, message assembly, tool schema, observation rendering, format-error and submit logic, limits. It does so with no environmental noise, and it compares against the unmodified original's actual bytes.
- L1.2 adds H's executor, but only where the environment has been shown to be reproducible.
- L1.0 is the control that keeps L1.2 honest. Without it, a divergence could be blamed on "the environment" after the fact.
- For natural sequences recorded on another host, the O reference is the **self-replay on the L1 host**, not the recording. Both O legs then share the kernel, image and podman with H. The recording remains the source of responses, and of the O reference only when *S* was recorded on the same host.

**Raw observations for the L1.1 executor.** H must render observations with its own template. Rendering is part of what L1 tests (G8), so H needs O's *raw* `(output, returncode, exception_info)`, not O's rendered tool message. The source is the original's native trajectory (`mini-swe-agent.trajectory.json`) if its `extra` members carry raw outputs. E1 did not establish whether they do (UNVERIFIED).

The fallback is a declared, tested inverse of the mini.yaml observation template:

- below 10 000 characters, the JSON `output`/`returncode`/`exception_info` members are exact;
- at or above it, `output_head`, `output_tail` and `elided_chars` determine everything the template reads (`[:5000]`, `[-5000:]`, `length`). The executor synthesizes a string with that head, tail and length. The interior filler is unobservable by the template.

The inverse must itself be golden-tested against the template, including multi-byte characters at the 5 000- and 10 000-character boundaries.

### 1.3 Capture points and the equality criterion

Each request is captured at two points, both inside the proxy (§1.6):

1. **Downstream:** the bytes the harness sent.
2. **Upstream-bound:** the request after the proxy's declared lane transforms — effort pin, `max_tokens` insertion, stripping of unsupported fields — computed in REPLAY mode without being sent.

The **gate compares upstream-bound requests**. That is what the model conditions on, and the lane transforms are one implementation applied identically to both arms in L1 and L2. Downstream equality is reported as a diagnostic, with its own smaller rule set. This choice makes some HH wire differences harmless by construction rather than by masking. The clearest case is E1's G4 `tool_choice: "auto"`: Ollama does not support `tool_choice` (C1 F2), and the proxy strips it with notice (REQ-XC-012). Gaps that change model input are never neutralised this way. The extra `hh.submit` tool (G4), the missing system message (G1), the semantic-id tool description (G3) and the dropped assistant prose (G6) all still fail.

Equality is defined at four levels:

- **Request:** `canon(H_k) == canon(O_k)` byte-for-byte, where `canon` is `hh-l1-canon/<v>` (§1.5).
- **Step:** request k is equal, and H's parsed action list for response k equals O's executed command list for step k. O's list is read from the native trajectory. For mini this means the `command` strings and `tool_call_id`s in order. For Terminus it means the `keystrokes`/`duration` pairs after clamping.
- **Trajectory:**
  - the same number of requests;
  - the same termination class (mini `exit_status` ∈ {Submitted, LimitsExceeded, RepeatedFormatError, TimeExceeded, <exception class>}; Terminus double-`task_complete`, `max_episodes`, session death, exception);
  - an identical submission payload;
  - **no extra request.** A request after the sequence end gets a typed exhausted refusal from the proxy and counts as a fail.
- **Corpus:** every trajectory in the frozen gate corpus passes. There is no tolerance and no rate.

**Failure semantics.** In strict REPLAY mode the proxy compares each upstream-bound canonical digest with the expected digest *before* serving response k. On a mismatch it returns a typed divergence error and stops the trajectory. The diff record is a durable artifact `hh-l1-diff/1`. It holds the sequence id, step k, both canonical bodies, a JSON-pointer diff, the rule-set version, and H's and O's identity records. A lenient mode serves anyway for diagnosis, but its results are never gate evidence.

**Resolving a fail.** Fix H. Alternatively, amend the allowlist with a version bump and a written justification; the gate then reruns on a *fresh* gate partition (§1.4). Never relabel a fail as a pass.

### 1.4 Corpus: three sources, partitioned before H is frozen

| Source | How obtained | Model spend | Purpose |
|---|---|---|---|
| **Natural recordings** | O-side trials of the paid **pilot** (§2.6) run with the proxy in RECORD mode. Every paid call is recorded verbatim anyway (REQ-XC-013). | Pilot only. There is no separate L1 spend. | realistic branch mix, long histories, real glm-5.3-flash outputs (reasoning, prose with tool calls) |
| **Synthetic branch fixtures** | hand-authored or edited response sequences in the recorded format, labelled `provenance: synthetic` and served by REPLAY to the unmodified O (L1.0) to obtain O's reference requests | $0 | forcing rare branches |
| **Turn-1 census** | for every task in the pilot ∪ tranche manifests, a one-response synthetic sequence that submits at once (mini: one `bash` call `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`; Terminus: two `task_complete: true` replies) | $0 | instruction-dependent bytes for every task that L2 will use: shlex quoting of `--task`, Jinja-sensitive characters, the uname line, MCP/skills appendices |

**Partitioning.** The natural recordings are split by a committed seed into **dev**, **gate-A** and **gate-B**. Gate-B is reserved for one allowlist amendment. The digests of the gate partitions are committed before H development uses the dev partition. Held-out status is by commitment and process, not secrecy, and the record says so.

The synthetic fixtures and the census are not held out: their branch list is the coverage claim. They are frozen with the allowlist.

**Branch coverage, mini-swe-agent.** Each item is drawn from E1 A.2–A.6 and must be present, natural or synthetic, before the gate run:

| # | Branch |
|---|---|
| M1 | single tool call, observation < 10 000 characters |
| M2 | observation of 9 999, 10 000 and 10 001 characters, including multi-byte characters straddling 5 000 (character vs byte truncation) |
| M3 | several tool calls in one response |
| M4 | no tool call |
| M5 | malformed arguments JSON |
| M6 | unknown tool |
| M7 | missing `command` |
| M8 | `finish_reason: length` with no tool call (the truncation template branch) |
| M9 | three consecutive format errors → `RepeatedFormatError` |
| M10 | format error, then a clean step (counter reset) |
| M11 | command exceeding the 30 s timeout (partial output, rc −1, `exception_info`) |
| M12 | marker on the first line with rc 0 → `Submitted` |
| M13 | marker not on the first line, and marker with rc ≠ 0 → no submit |
| M14 | submit mid-batch → padded "action was not executed" |
| M15 | non-UTF-8 output (`errors="replace"`) |
| M16 | provider context-window error → abort |
| M17 | 429/502 then success → transport retry, with identical bytes and an equal retry count |
| M18 | assistant prose plus a tool call, and `reasoning_content` present |
| M19 | `env \| sort`, `pwd`, `hostname`: exposes exec-environment differences for L1.2 |

**Branch coverage, Terminus-2 (stage 2; E1 B.1–B.5):**

| # | Branch |
|---|---|
| T1 | parse error |
| T2 | each warning class (extra text, field order, missing or non-numeric duration, missing newline) |
| T3 | both auto-fixes |
| T4 | `task_complete` once, then confirmation; twice → end; reset path |
| T5 | `finish_reason: length` (`limit_str` with and without model info) |
| T6 | observation over 10 000 bytes (byte-limit marker) |
| T7 | special keys (`C-c`) and keystrokes over ~16 000 bytes (paste-buffer path) |
| T8 | duration > 60 clamped |
| T9 | proactive summarization (three calls S1–S3 + handoff) under a synthetic small `max_input_tokens` configuration |
| T10 | reactive summarization on a provider overflow error, with unwind and both fallbacks |
| T11 | the nested 3×3 retries |
| T12 | the `_find_new_content` index quirk |
| T13 | tmux session death |

Harbor's own deterministic Terminus tests (invalid JSON; context summarization with `max_input_tokens: 7000`, threshold 0) are seed fixtures to reuse, not reinvent (E1 F9).

**Sizing (design proposal).** These are minimums.

*Natural recordings:* ≥ 12 dev + 8 gate-A + 8 gate-B trajectories. At the HF board's 31.8 calls per mini-swe-agent TB2.1 trial (C3 F2), that is about 900 requests. They come from the pilot's 48 O trials (§2.6), so the corpus needs no spend beyond the pilot.

*Synthetic fixtures:* ≈ 25 sequences for mini, and ≈ 30 for Terminus later.

*Turn-1 census:* one sequence per manifest task.

The census and synthetic legs cost container and VM time only: one Harbor trial each, plus O self-replay and H runs. The verifier runs after each, and that cannot be skipped.

### 1.5 Canonicalization: `hh-l1-canon/1`, a declared and versioned allowlist

**Principles.** Each rule has an id, a field path, an action, a justification, scope (mini/terminus) and two tests:

- a positive test: the masked difference passes;
- a mutation test: a difference in an adjacent, unmasked field fails.

A rule may only cover:

- (a) encodings with no semantic content in the protocol;
- (b) values sourced from the environment that are *verified* equal to a recorded fact of the respective run;
- (c) transport metadata that never reaches the model.

**Message text is never normalized:** no whitespace, Unicode or case folding inside string values. Terminus's 12-space indented summary prompts, for example, are harness behaviour.

Rules are frozen before the gate run. The only "calibration" is during H development on the dev partition. Any change after a gate run bumps the version and burns that gate partition. The library reuses the repository's canonical JSON from `hharness-wire`/`hharness-identity` and does not implement a second canonicalizer (CC1). The strict proxy mode calls the same library.

| Rule | Field / scope | Action | Justification |
|---|---|---|---|
| C01 | whole body | parse JSON; re-serialize with the repository canonical JSON (sorted keys, no inter-token whitespace, escapes decoded so `é` and `é` compare equal) | JSON object order, inter-token whitespace and escape spelling carry no meaning. String *values* compare as exact decoded code-point sequences. |
| C02 | numbers | compare by the canonicalizer's numeric form (`1` vs `1.0` where the canonicalizer admits floats; otherwise the rule refuses and the field is compared textually) | number spelling is encoding |
| C03 | HTTP headers | compare only `content-type`, method and path, exactly. Drop `authorization`/`x-api-key` (secret-bearing; the proxy rewrites them, REQ-XC-001), `user-agent`, any `x-stainless-*`, `content-length` and connection headers | client-library telemetry and secrets never reach the model. The client library's identity is recorded in the participant record instead (§2.1). |
| C04 | mini user(instance) message, the exact line `{{system}} {{release}} {{version}} {{machine}}` inside `<system_information>` | **verified substitution:** replace the four values with `⟨uname⟩` *only if* they equal that run's recorded cell `uname -s`, `-r`, `-v` and `-m`. Otherwise the line is left unchanged, so it fails. | The values are environment facts (E1 F3) that differ across hosts. A wrong rendering by H still fails. |
| C05 | `stream`, `stream_options` (downstream capture only) | drop, and record the transport mode in the declared-deviation register | Transport framing is not model input. This only matters downstream: the upstream-bound capture is whatever the lane emits for both arms. The preferred fix is that H matches non-streaming (E1 G5); then the rule is unused. |
| C06 | optional assistant-message members with value `null` (`function_call`, `tool_calls`, `refusal`, `audio`, `annotations`) | `null` ≡ absent | OpenAI chat schema optionality. Scoped to these members only. **Provisional:** confirmed or deleted against the dev partition before freeze. |
| C07 | Terminus `extra_body.session_id` and `X-Session-ID` | drop | Per-trial volatile identifiers, present only if `session_id` is configured. The reference configuration leaves it unset, so the rule should be unused. |

**Explicitly not rules (strict equality required):**

- *Tool-call ids.* They come from the replayed responses, so a rewrite by H is a real wire difference. A bijective renaming rule is held in reserve and needs a version bump.
- *Re-sent reasoning text* (`reasoning_content`, `provider_specific_fields`). H must reproduce what O sends; see the recommendation in §6.
- *`model`, `max_tokens`, `reasoning_effort`, `temperature` upstream.* They are pinned by the proxy for both arms. A difference means a lane fault.
- *Tool schema order and content*, including descriptions.
- *System/user prompt bytes.*
- *Observation contents*, including dates in command output. L1.1 makes these equal by construction, and L1.2 handles them through the stable prefix, never by masking.

### 1.6 What L1 needs from the C2 proxy (interface, not design)

| ID | Requirement |
|---|---|
| P1 | **Trial session binding.** Every request is attributable to exactly one trial and sequence position. The recommended binding is a per-trial bearer token issued by the proxy (the harness's `api_key`); the real key never enters a cell (REQ-XC-001). |
| P2 | **RECORD mode.** Record the downstream request (headers + body bytes), the upstream-bound request, and the upstream response: status, headers, body bytes, and stream chunks if streamed. Also record timing, queue wait and the sequence index. |
| P3 | **REPLAY mode (sequence).** Serve the recorded response k (status, headers, body; error responses included) to the k-th request of the bound session, regardless of request content. The responses' own `id`/`created` are served verbatim. Make no upstream contact and no budget charge, and mark receipts `mode: replay`. |
| P4 | **Strict/lenient modes**, as in §1.3, through the shared canonicalization library. |
| P5 | **Typed end-of-sequence refusal** for any request beyond the sequence. |
| P6 | **Synthetic sequences** accepted in the recorded format, with `provenance: synthetic`. |
| P7 | **Replay into the requested transport.** If a harness requests streaming, re-chunk the recorded body deterministically under a declared rule. |
| P8 | **Upstream-bound transform in replay.** The lane transforms run on replayed requests, so upstream-bound bytes are capturable at $0. |
| P9 | **Optional recorded-latency replay**, for timing-sensitive Terminus legs. It defaults to off. |

---

## Part 2 — L2 paired task-verdict parity

### 2.1 "Same participant/configuration" for a served LLM without a fingerprint

Spec §4.4 requires the **same participant/configuration** under H and O, with pinned model and environment and matched budgets and cache. C1 shows that `glm-5.3-flash` has no served-weights fingerprint, no pinnable snapshot and uncontrollable caching, and that seed honouring is unverified (C1 F4, F7, F12). The design therefore defines sameness at the **configuration-record level**. One participant record is written per trial, and the two records of a pair must be equal on every member below except the harness-implementation members, which are what is being compared. The pre-registered check refuses a pair whose records differ elsewhere.

| Member | Value / source | Equal across H and O? |
|---|---|---|
| Model lane | proxy lane document digest, upstream dialect, endpoint `https://ollama.com`, model id `glm-5.3-flash` | yes |
| Catalog identity | `{name, tags digest 0c238824336d, modified_at, parameter_count, quantization_level, context_length, thinking values/default}`, snapshotted at tranche start and end and at every block boundary; `provenance: reported`, `pinned: false` (REQ-XC-009) | yes. A change stops collection. |
| Served-model echo | the response `model` field on every call (C1 F15) | must equal the pinned id; a mismatch makes the pair uncertain |
| Reasoning effort | proxy-pinned level (Q-X22), with requested and applied values per call (REQ-XC-002) | yes |
| Sampling | `temperature`, `top_p` and `seed`: **unset** on both sides (the reference harnesses send none; E1 F2, B.5). Recorded as `unset`, never as a provider default value. | yes |
| Output cap / budget | the proxy `max_tokens` cap and per-trial budget ceiling | yes |
| Account | the Ollama account is exclusive during the tranche (C1 open question 3) | yes |
| Task environment | task image digests, verifier identity (the same Harbor verifier and reward channel on both sides, `pins.verifier_ref`), VM machine type, kernel, podman version | yes |
| Harness, O | mini-swe-agent 2.4.6 at Harbor's pinned commit (E1 REQ-XE-001), `mini.yaml` sha256 `b539a896…`, Harbor flag list, frozen O image digest with the resolved LiteLLM version (§6) | this *is* the contrast |
| Harness, H | HIR document digest, compiled-artifact digest, HH commit, the L1 report id that passed | this *is* the contrast |

Several §4.4 predicates cannot be satisfied for this participant. Each becomes a **typed unknown**, with the consequence the claim must carry:

| §4.4 predicate | Status for this participant | Consequence |
|---|---|---|
| pinned model | `unknown{no_served_fingerprint}`: only the catalog identity is pinned, as `reported` | The claim reads "under catalog identity ⟨…⟩ during ⟨window⟩". It does not transfer to another window or catalog. A catalog change ends the cohort; it is never relabelled (C1 F13). |
| matched cache | `unknown{provider_cache_uncontrolled}` | Within-pair simultaneous launch (§2.2) makes cache exposure symmetric in expectation. Cost and latency are excluded from the required conjunction (§4). |
| requested/honoured seeds | requested none; honoured n/a | `seed_records = (0,0)`. No CRN or honoured-seed coupling is claimed (`FORBIDDEN_USES`). |
| determinism | none claimed | L2 is a two-sample comparison under sampling noise. All determinism claims live in L1. |
| `central_protocol` support gate | `support_seed`, `support_fingerprint` and `support_cache` are false, so `materialize_protocol` refuses (`central_protocol.rs:637-649`) | The L2 manifest needs the same §4.1 ADR that D1 needs (C1 open question 6). D2 adds no second selection routine (CC1). |

### 2.2 Pair construction, scheduling and the independence rationale

**Pair construction:**

- A pair is `(task_id, repeat, pair_id)`, with one H and one O **first attempt** (attempt = 1), each in a fresh cell from the same task image.
- The subject is the pair id, which is distinct across pairs.
- `replay_ref` is the digest of the pair's two verbatim proxy receipt bundles plus both native trajectories. This is what makes each pair re-derivable through L1 machinery and satisfies "original/replay evidence".
- A pair lacking it is unmatched (`MissingReplay`), never a pass.

**Interleaving and time-blocking (design proposal):**

1. **Schedule.** The N pairs run in a seeded random permutation over tasks × repeats, so the repeats of one task are spread across the tranche window, and provider drift spreads across tasks.
2. **Simultaneous launch.** H and O of a pair launch together, on the same VM, against the same proxy. They therefore face the same provider state, load and cache warmth.
3. **Blocks.** Pairs run in blocks of k = 1–2 pairs, giving 2–4 trials in flight. The proxy enforces ≤ 3 in-flight upstream requests and fair-queues the rest (REQ-XC-011). k is fixed at the L2 gate from pilot queue-time data, so that median queue wait is ≤ 10 % of median call latency. Queueing counts against Harbor's wall timeout, so heavy queueing would let load decide outcomes.
4. **Snapshots.** A catalog-identity snapshot is taken at every block boundary.

**Independence rationale.** This is the `independence_rationale_ref` content. It is a declared design assumption, never something proved by logs.

- Across pairs, trials share no filesystem, process, conversation or provider state. Chat calls are stateless, and every cell is fresh.
- Provider prefix caching may make later pairs cheaper. It is assumed not to change sampling distributions. That assumption is UNVERIFIED, because KV-cache reuse could in principle alter numerics.
- Provider drift is treated as part of the realized conditions. The estimand is the finite-manifest Δ under the realized serving path during the tranche window. Time-matched pairing puts H and O under the same conditions, but it does not make drift disappear from the claim's scope.

**Rejected alternative.** A "response-coupled" pair would feed H the responses O received until the trajectories diverge. That is a shared-shock or CRN design. `parity_admission` refuses it (`UnsupportedDependence`), and it would need its own admitted method. It is not proposed.

### 2.3 Floors, N, and infra-failure missingness

**Floors and N.** The landed validator enforces `repeats ≥ 3` (`AdapterFloor`) **and** `repeats ≥ 5` (`S3Floor`) unconditionally, along with `pairs.len() == T·R` and equal R per task (`parity_admission.rs:866-910`). The design therefore uses **R ≥ 5**, and N = T·R. T is bounded by the number of TB2.1 tasks that are feasible on the VM and disjoint from the pilot/dev tasks. That number is owned by B1 and still unknown; the HF board uses 87 tasks.

**Missingness.** First attempts only means a failed attempt can never be repaired inside the tranche: a reattempt is `NotFirstAttempt`, and top-ups are forbidden (§4.5). An infrastructure failure on either side makes the pair **uncertain** (`h` or `o` = `None`). It enters the §4.4 completion envelope at the planned N, `[K_η(c/N).L, K_η((c+u)/N).U]`, and is never treated as missing at random.

The infrastructure classes are declared before data and are symmetric across arms:

- provider 429/5xx/queue-full, or a broken stream, that persists after the harness's own retries are exhausted (C1 F11);
- proxy budget-ceiling refusal;
- cell or Harbor environment provisioning failure;
- verifier unavailable;
- served-model mismatch.

Harness-internal outcomes are task outcomes, not infra. Examples: `ContextWindowExceeded`, format-error exit, and wall timeout with a healthy provider.

**The u-rate is a first-class precision driver (F12).** At N = 300 and q = 0.117, three uncertain pairs (1 %) cut the probability of an `equivalent` verdict at ε = 0.15 from 0.926 to 0.832. Nine pairs (3 %) cut it to 0.42. The HF board's mini-swe-agent infra rate on TB2.1 was 2/263 (C3 F2).

### 2.4 Discordance prior

Under exact parity, H and O are two independent draws for the same task. P(W+) = P(W−) = mean over tasks of p_t(1−p_t) = f/2, where f is the rerun flip rate, `2k(n−k)/(n(n−1))` averaged over tasks (HF estimator; F1 note 29 F2).

The HF board reports f = 23.8 % for mini-swe-agent 2.4.6 on TB2.1 at low effort with GLM-5.3-Flash, and 16.9–28.4 % across nine harnesses (C3 F2). That gives **q± ≈ 0.10–0.14 under the null, central value 0.117**.

This prior comes from a different serving setup (C1 F16). The pilot measures it on Ollama (§2.6).

### 2.5 Precision, power and cost

The bound is `K_η` per spec §4.4 with η = .025 and `[L_Δ, U_Δ] = [L+ − U−, U+ − L−]`, implemented in `kl.py` and validated as described above.

**Table A — expected .90 TOST half-width** at symmetric x+ = x− = q (continuous counts qN), u = 0:

| N | q=.05 | q=.08 | q=.10 | **q=.117** | q=.12 | q=.15 |
|---|---|---|---|---|---|---|
| 30 | .228 | .270 | .294 | .312 | .315 | .342 |
| 50 | .173 | .209 | .229 | .244 | .246 | .269 |
| 100 | .120 | .148 | .162 | .173 | .175 | .192 |
| 150 | .098 | .120 | .133 | **.142** | .143 | .157 |
| 200 | .084 | .104 | .115 | .123 | .124 | .136 |
| 300 | .069 | .085 | .094 | **.101** | .102 | .112 |
| 435 | .057 | .071 | .078 | .084 | .085 | .093 |
| 600 | .048 | .060 | .066 | .071 | .072 | .079 |
| 870 | .040 | .050 | .055 | .059 | .060 | .066 |

With uncertain pairs at q = .117, the adverse half-width grows:

| N | u = 0 | u = 1 % | u = 3 % |
|---|---|---|---|
| 150 | .142 | .154 | .178 |
| 300 | .101 | .112 | .135 |
| 600 | .071 | .082 | .105 |

**Table B — P(`equivalent`)** when true Δ = 0 and q+ = q− = q. Exact trinomial enumeration over (c+, c−), with the verdict rule strictly `L_Δ > −ε ∧ U_Δ < ε` (`directional_kl.rs` `verdict`):

| q | N | ε=.10 | ε=.15 | ε=.20 |
|---|---|---|---|---|
| .05 | 150 | .196 | .948 | 1.000 |
| .05 | 300 | .902 | 1.000 | 1.000 |
| .10 | 150 | .000 | .402 | .939 |
| .10 | 300 | .201 | .971 | 1.000 |
| .10 | 600 | .934 | 1.000 | 1.000 |
| **.117** | 150 | .000 | .212 | **.868** |
| **.117** | 300 | .050 | **.926** | 1.000 |
| **.117** | 435 | .532 | .996 | 1.000 |
| **.117** | 600 | .858 | 1.000 | 1.000 |
| **.117** | 870 | .987 | 1.000 | 1.000 |
| .15 | 150 | .000 | .025 | .684 |
| .15 | 300 | .000 | .785 | .995 |
| .15 | 435 | .223 | .972 | 1.000 |

Further checks:

- **Size, at true Δ = +ε and q = .117:** P(`equivalent`) ≤ .0011 for every N ∈ {150, 300} and ε ∈ {.10, .15, .20}. The concentration bound is conservative, far below α = .05.
- **True Δ = +.05 at N = 300:** P(`equivalent`) = .013 at ε = .10, .52 at ε = .15 and .968 at ε = .20.
- **The two-endpoint conjunction (§4):** its power is at most the smaller member's power.

The u-rows of the power calculation approximate the envelope by treating u as an extra adverse count on N planned pairs.

**Table C — cost per design.** Both surfaces are paid, so cost = 2N trials × $/trial, plus the 20 % reserve (C1).

The $/trial columns span:

- the TB2.1 mini-swe-agent observation on the HF board ($0.043 at low effort, $0.057 at high, with ≈98 % caching on HF's provider; C3 F2);
- its no-cache estimate (≈ $0.155 at low; C3 F3);
- C1's $0.10–$0.40 all-benchmark prior. At `max` effort, the reasoning-token multiplier is ≈ 1.9× `high` (C1 F5).

| Design | T×R | N | Trials | $0.05 | $0.10 | $0.20 | $0.40 | ε at power ≥ .85 (q=.117) |
|---|---|---|---|---|---|---|---|---|
| A | 30×5 | 150 | 300 | $15 (18) | $30 (36) | $60 (72) | $120 (144) | .20 (.868) |
| B | 60×5 or 30×10 | 300 | 600 | $30 (36) | $60 (72) | $120 (144) | $240 (288) | .15 (.926) |
| C | 29×15 or 87×5 | 435 | 870 | $44 (52) | $87 (104) | $174 (209) | $348 (418) | .15 (.996); .10 (.532) |
| D | 87×10 or 58×15 | 870 | 1 740 | $87 (104) | $174 (209) | $348 (418) | $696 (835) | .10 (.987) |

Figures in parentheses include the reserve. Wall time at the HF board's ≈ 760 agent-seconds per mini trial (C3 F2), with 4 trials in flight: design A ≈ 16 h and design B ≈ 32 h of VM time. At ≈ $0.30/h that is $5–10 of compute, quoted separately by B3. These are planning figures. The binding quote is the pilot's.

**What the table means.**

- **ε ≤ .10 is unaffordable inside the pilot budget at any plausible $/trial.** It needs N ≈ 600–870, that is 1 200–1 740 paid trials.
- **ε = .15 needs N ≈ 300** (design B). That is ≈ $36–72 with reserve at $0.05–0.10 per trial. It is plausible for TB2.1 only if Ollama caching resembles HF's.
- **ε = .20 needs N ≈ 150** (design A).

For scale, the nine harnesses on the HF board span 45.6–67.8 % on TB2.1 (C3 F2). A ±.15 parity therefore excludes a gross infidelity, not a harness-sized difference, and the claim text must say so. L1 carries the fine-grained fidelity claim.

### 2.6 Staged funding: the pilot, then the gate

Per A-3 and Q-X4, **ε, N, T and R are fixed at an operator gate (G-L2) after a pilot quote**, and never before or after confirmatory data.

The pilot is **O-only**: the original harness only. It needs no H, so it can run while E2 builds H. It is also the L1 natural corpus.

**Design (design proposal):**

- 16 development tasks × 3 repeats = 48 O trials, on tasks disjoint from the tranche partition. All tasks come from one seeded split of the B1-feasible TB2.1 set.
- Cost: $2.4 / $4.8 / $9.6 / $19.2 at $0.05 / 0.10 / 0.20 / 0.40 per trial.

**What the pilot measures:**

1. the rerun flip f for capability and for the reliability endpoint (§4), with a task-bootstrap interval;
2. mean and P90 $/trial and the cache share (REQ-XC-015);
3. calls per trial;
4. model-occupied and queue time;
5. the infra-class rate u;
6. per-trial wall time;
7. budget-ceiling binding frequency;
8. the full RECORD corpus.

**Precision of the pilot itself.** Simulated with `pilot.py`, using a task mixture calibrated to mean f = .234 (41.5 % deterministic tasks; 58.5 % with p ~ Beta(2,2)) and 20 000 replicates, the estimate of f has:

| Tasks × repeats | sd | p10–p90 |
|---|---|---|
| 12 × 3 | .092 | .111–.333 |
| 16 × 3 | .080 | .125–.333 |
| 24 × 3 | .065 | .139–.306 |
| 16 × 4 | .070 | .146–.323 |

**How the gate uses the pilot.** The pilot cannot pin q. The G-L2 quote should therefore choose N against the upper end of a q range, for example the task-bootstrap 80 % upper bound on f/2, using Tables A and B. Pilot outcomes are never pooled into the tranche, so pilot data are not relabelled as confirmatory.

**An optional rehearsal.** A pipeline rehearsal of 2–3 live H/O pairs on dev tasks may follow L1. It is disclosed, and its Δ is not analysed. The gate record states whether its verdicts were viewed.

---

## Part 3 — L3 descriptive trace shape

L3 reports, per arm, the **distributions** of the following. It reports the median, P10/P90, mean with a whole-task paired bootstrap descriptive interval, and the per-pair H−O difference. It makes **no equivalence claim** unless an endpoint is separately admitted through §4.4 with a known physical range.

- Steps / model calls per trial, including summarization calls and retries, counted separately.
- Tokens by role: input, cached input (when provider-reported; otherwise `unavailable`, never 0; REQ-XC-007), output, and reasoning output.
- Cost (`estimated_from_pricing` vs upper-bound `estimate`).
- Wall time, model-occupied time and queue time.
- Format errors per trial and the consecutive-error streak distribution.
- Observation truncations (count and elided size).
- Termination class histogram (submit path vs format-error exit vs timeout vs exception vs infra).
- Submit path, for Terminus: confirmations requested vs accepted.
- Summarization events (Terminus).
- Per-task verdict grid with trajectory links.

L3 also reports the discordant pairs individually, as task, repeat, direction and links to both trajectories. This is spec §4.4's "recorded discordances".

**Population note.** L3 distributions are over all planned pairs, with uncertain pairs shown as a separate class. Conditional "eligible-only" summaries are labelled conditional and carry no inference.

---

## Part 4 — Dimension map for this participant

Signed before the first L2 outcome (`map_seq < first_outcome_seq`). The vocabulary is the closed set `DIMENSIONS` in `parity_admission.rs:65-73`.

| Dimension | Required? | Mapping |
|---|---|---|
| capability | **required** | Binary first-attempt task success from the same Harbor verifier/reward channel. Estimand Δ = Σ_t(p_Ht − p_Ot)/T over the planned pairs N = T·R. Absolute margin ε (gate-set). Unit: ppm of success. |
| reliability | **required** | Binary "ended through the harness's submit path" (mini `exit_status == Submitted`; Terminus double `task_complete`). Same population and the same absolute ε. pass^k stays `unavailable` (`UNAVAILABLE_ESTIMANDS`) and is not used. |
| grounding | not required; `inapplicable` | "No admitted observable grounding detector exists for terminal tasks. L1 establishes that the model receives identical observations given identical environments. Reported descriptively in L3." |
| efficiency | not required; `inapplicable` for the equivalence claim | "Cost and latency parity is not claimed. Provider caching is uncontrollable and shared within a pair (C1 F7). Queueing is shared. No affordable bounded inference exists at the pilot N: tokens are unbounded except by the proxy ceiling, whose range is wide relative to the mean. Descriptive only (L3)." |
| autonomy | not required; `inapplicable` | "No approval or intervention channel exists in either configuration (mini `--yolo`, `confirm_exit=False`; Terminus has none). A per-trial zero-intervention record is kept, and any intervention is a validity veto." |
| security_compliance | not required as an equivalence endpoint; `inapplicable` | "No symmetric H/O integrity metric exists. HH-cell sentinel/observer/seal predicates have no Harbor-side counterpart. Enforced instead through the pre-registered **validity-veto layer** below." |
| portability_evolvability | not required; `inapplicable` | "Fixed configuration: one model, one benchmark, one VM class, one harness version. No portability claim." |

**Validity-veto layer.** This is pre-registered and sits outside the KL conjunction. Spec §4.4 says vetoes and unknown checks "cannot be removed/averaged away". The parity claim therefore requires an empty veto list. The per-trial predicates are:

- model egress only through the proxy (proxy receipts plus the cell's declared egress profile), for both arms;
- no answer-key or test-file access, per D1's cheating-scan policy, for both arms;
- H-side HH integrity: `sentinel_intact`, `observer_survived`, `seal: sealed`;
- a zero-intervention record.

A predicate that cannot be determined is listed before data as `not_checked{reason}`. A1 found `probe: ok:undeterminable` on all 300 S4 slots, so the probe is the obvious example; it is not silently dropped. An observed veto blocks the claim and is published with the trial.

**Conjunction.** One pre-registered IUT conjunction over {capability, reliability}, with `selected_claims = 1`. Per spec §4.4, no endpoint Bonferroni is applied. A missing member is `inconclusive` (`directional_kl.rs:1612-1621`).

---

## Part 5 — Admission packet, freeze sequence and the L1 → L2 gate

### 5.1 Freeze sequence

1. **Reference configuration frozen** (operator ratifies Q-X29/Q-X22):
   - the O image built once, with the resolved LiteLLM recorded (§6);
   - Harbor commit, effort, budget ceiling and model info (Terminus);
   - the proxy lane document digest.
2. **`hh-l1-canon/1` landed** with golden and mutation tests (fixture-verified, $0).
3. **Pilot spend gate (operator).** The O-only pilot runs in RECORD mode.
   - Its outputs are the pilot quantities packet and the natural corpus.
   - The seeded dev / gate-A / gate-B split is committed, together with the gate digests.
   - The task partition is committed: pilot/dev vs tranche.
4. **Synthetic branch fixtures and turn-1 census** authored. Their O references are captured through L1.0 ($0 model spend).
5. **H developed against the dev partition only.**
6. **L1 freeze and gate run.**
   - H is frozen: HIR document digest, compiled-artifact digest, HH commit. The allowlist version is frozen.
   - L1.0, L1.1 and L1.2 run on gate-A plus the synthetic fixtures plus the census.
   - The run emits `hh-l1-conformance/1` (pass/fail, coverage table, stable-prefix histogram, every diff record).
   - **Operator gate G-L1.**
7. **L2 pre-registration signed** (§5.2). **Operator gate G-L2:** ε, N, T, R, k, schedule seed, spend authorization and the compute quote. The `PreRegistration.equivalence_margin` record is the only margin source (`benefits::registered_margins`).
8. **Tranche executes** on a fresh VM (B3).
   - Catalog snapshots are taken at block boundaries.
   - Stop rules: catalog change; budget exhaustion; an infra rate above the pre-declared threshold, checked at pre-declared operational checkpoints; never outcome-based. Remaining planned pairs become uncertain.
9. **Post-tranche, in order:**
   - (a) **L1-on-L2 audit.** A seeded sample of tranche O recordings, e.g. 10 % of pairs, is replayed to the frozen H under L1.1 at $0. This extends L1 to natural trajectories on the tranche tasks. A failure fails the parity claim, and the L2 verdict stays recorded.
   - (b) `parity_admission` and `kl_equivalence` run through the `hh-analysis` kernel routes.
   - (c) The veto layer and the L3 report.
   - (d) The admission packet is assembled.
   - (e) Publication gate.

**L1 gates L2.** No paid L2 pair runs until G-L1 records a pass for the exact H version that L2 uses. Any change to H after G-L1 needs a full L1 rerun. If the change happens during the tranche, the tranche ends, and a new cohort starts after the L1 rerun. The O-only pilot is the only paid work before G-L1.

### 5.2 Admission packet contents

The packet contains:

- **Identity:** participant record schema and the both-arms equality check (§2.1), with the typed-unknown table and the claim-scope sentence it forces.
- **L1 evidence:**
  - the `hh-l1-conformance/1` report id and digest;
  - allowlist version and rule table;
  - corpus manifests (natural partitions with digests, synthetic list, census list);
  - the coverage map M1–M19 (T1–T13 later).
- **Pair design:**
  - the pair manifest (`PairManifest`: T, R, weights equal, phase, stratum, dependence `independent_pairs`);
  - the independence-rationale text and ref;
  - the seeded task selection and schedule;
  - within-pair simultaneous launch, block size, and first-attempt-only policy.
- **Pre-registered analysis:**
  - the dimension map (§4), signed with `map_seq`;
  - registered margins (ε for capability and reliability, `absolute`);
  - the conjunction declaration;
  - the analysis spec: kinds `parity_admission` and `directional_kl_equivalence`, `confidence_ppm = 950000`, `selected_claims = 1`.
- **Validity and stopping:**
  - the infra-class table, completion-envelope statement, u threshold and stop rules;
  - the veto predicates and their `not_checked` list.
- **Policy declarations:**
  - policy set `["reference"]` with `BothPolicies{[reference → L1 report]}`. One policy is declared, so no transfer is claimed (`transfer.claim = "none"`).
  - `use_as` absent. There is no headline, CRN or s3_claim use.
- **Pins:** `source_ref` (the TB2.1 dataset digest from B1), `verifier_ref`, `rights_ref`.
- **Gates:** the G-L1 and G-L2 authorizations, recorded verbatim.
- **Claim templates**, fixed before data, one per outcome:
  - "L1-conformant and verdict-equivalent within ±ε"
  - "L1-conformant; verdict parity inconclusive at ±ε"
  - "not equivalent (95 % reporting interval outside ±ε)"
  - "L1 not passed". This one blocks L2.

An inconclusive outcome is as publishable as an equivalent one (brief §5).

### 5.3 How Terminus-2 differs (stage 2, after mini-swe-agent's L2)

| Aspect | Terminus-2 difference (E1) | Method consequence |
|---|---|---|
| Where it runs | Harbor host process; the container only via `environment.exec` (B intro) | Model calls originate on the host, so the proxy must be reachable from the host, and no cell egress is needed. H runs in the kernel with a terminal-session primitive (G13). |
| Observation seam | tmux `capture-pane` text with sleep-based waits and an incremental-diff heuristic (B.4) | The L1.1 observation-replay seam is the **terminal session**. Replay serves recorded `capture-pane` buffers per step, so H's own diff and 10 000-byte truncation logic run. Recording O's buffers needs exec-level capture on O's environment, which is a B2/C4 dependency. Timing makes stable prefixes short, so L1.1 carries more weight than for mini. |
| Summarization | three calls S1–S3 with history replacement; proactive and reactive triggers (B.5) | Each summarization call is a separately sequenced request in the replay. Coverage comes from a **synthetic L1 configuration** (small `max_input_tokens`, Harbor's own scenario), applied identically to H and O. Under the L2 reference configuration, summarization is effectively unreachable (E1 F10). |
| Token counting | triggers and unwinding use `litellm.utils.token_counter(model=…)` | Exact reproduction of the trigger requires the same tokenizer behaviour in H. That is an E2 gap and may need a dependency ADR, or else a declared divergence. Without one of these, T9/T10 cannot pass L1. |
| `model_info` | drives triggers and `limit_str` | See the recommendation in §6. |
| Re-sent assistant fields | `{role, content}` only, unless `interleaved_thinking` | Keep the default (`false`). The reasoning-text question does not arise. |
| Floors and design | — | Same as mini: a separate pilot, corpus, G-L1 and G-L2. The cohort is separate, and no pooling with mini. |

---

## Part 6 — Phase-4 handoff and recommendations to E1's methods questions

**"Parity-verified baseline."** This means one specific frozen configuration, not "HH-native mini-swe-agent" in general: the H configuration whose L1 gate passed and whose L2 tranche returned `equivalent` with an empty veto list. Its record holds:

- HIR document digest, compiled-artifact digest and HH commit;
- the participant record (lane, catalog identity and window, effort, budget ceiling, model info);
- task-image digests and the VM class.

The label is dated and catalog-scoped: "parity-verified on ⟨catalog digest, window⟩". A later catalog change does not revoke it, but sweeps must state that their runs fall outside the window. If L2 was inconclusive, phase 4 may still sweep from the L1-conformant configuration, labelled as **L1-conformant baseline, verdict parity inconclusive**, never as parity-verified.

**Swept variants.**

- A variant is an HIR document that differs from the baseline in exactly one registered slot (D3's factor/level registry).
- Variants are **never parity-claimed** against the original. §4.5 forbids baseline-only transfer, and `parity_admission` refuses `BaselineOnly` and transfer claims.
- Variants are compared with a baseline arm **re-run inside the sweep**, on the sweep's tasks and time window. L2 data are not reused.
- Cheap check (design proposal): **declared-delta conformance.** Run the variant and the baseline through L1.1 on the synthetic corpus. The canonical request diffs must be confined to the fields the factor declares it touches. For example, an observation-truncation factor may change only tool-message contents on M2. Variants that touch undeclared fields are refused before any paid sweep run.

**Recommendations (design proposals; Q-X22 and Q-X29 remain operator decisions).**

- **R1 — Terminus-2 `model_info`: supply it.** Use `max_input_tokens = 1 048 576` (the catalog `context_length`) and `max_output_tokens =` the proxy's declared output cap, both recorded in the participant record.
  - This removes a hidden dependency on whatever LiteLLM's model map contains at install time. The fallback is 1 000 000 today and would change silently if a future map gains the key.
  - It makes `limit_str` truthful.
  - Behaviourally it is almost identical to Harbor's default here: the proactive trigger is unreachable either way on TB tasks.
  - Record it as a declared deviation. Summarization coverage comes from the separate synthetic L1 configuration. A threshold *sweep* (D3) needs a small window and is a variant.
- **R2 — mini-swe-agent uname: reproduce and verify, don't mask.** H renders the actual cell's uname, because the model must see what O's model sees. L1 uses rule C04's *verified substitution* only to compare across hosts. In L2 both arms share a VM, so the bytes are equal anyway.
- **R3 — re-sent reasoning text: keep.** H reproduces exactly the assistant members O re-sends (`reasoning_content`, `provider_specific_fields`, prose content), as observed on the dev partition. Stripping is a harness change that would alter model conditioning and cost (C1 F5 notes `clear_thinking=false` in the GLM template). It belongs in phase 4 as a registered factor, not in the baseline.
- **R4 — Harbor defaults vs declared limits: use Harbor's defaults for every in-agent limit,** with the external caps declared and recorded.
  - mini: step limit 0, `--cost-limit 0`, wall limit 0, format-error cap 3. Terminus: `max_turns` default.
  - External caps: Harbor's agent wall timeout as the task declares it, and the proxy's per-trial budget ceiling. Set the ceiling from pilot P99 × a safety factor so that it binds rarely. A binding counts as an infra-class uncertain pair, never as a task failure.
  - Adding a step limit would be a deviation from the reference. It belongs to phase 4.
- **R5 — LiteLLM: resolve once, then freeze.** Evaluate Harbor's install line once, on a recorded date. Record every resolved version (LiteLLM and transitives), commit the lock or constraints file, and use that one O image for the pilot, L1 and L2.
  - "As Harbor runs it" is honoured on the freeze date. The request bytes cannot drift mid-programme (E1 F7), which would invalidate the L1 recordings.
  - For Terminus, record the Harbor host environment's LiteLLM from Harbor's lock.
- **R6 — Effort (Q-X22): any pin is valid for parity.** D2 prefers the level phase 2 uses, for continuity of participant identity across phases. On cost grounds it leans `high`: ≈ 0.53× the reasoning tokens of `max` (C1 F5), which moves the feasible design from A toward B at the same spend. Pin it in the proxy for both arms.

---

## Part 7 — Ticket-shaping hints

Each item below is sized for one fresh-context `implement-spec` run and has deterministic acceptance unless marked paid.

| Seam | Contents | Deterministic acceptance |
|---|---|---|
| K1 `l1-canon` library | `hh-l1-canon/1` rules C01–C07 over captured request records, using the existing canonical JSON (CC1). Home: an `hh-eval` module, unless E2/H1 prefer `hh-gateway`. Shared with the proxy's strict mode. | golden tests per rule; a mutation battery (each masked field passes, each adjacent field fails); a version-bump test |
| K2 observation-replay executor | an HH exec-seam test double that serves recorded raw observations per step; the tested inverse of the mini.yaml observation template | golden boundary tests (M2); refusal on a sequence overrun |
| K3 synthetic corpus and census | fixtures M1–M19; turn-1 census generator; recorded-format schema with `provenance` | schema tests; fixtures load and validate ($0) |
| K4 L1 runner | orchestrates proxy REPLAY, the L1.0/L1.1/L1.2 legs, stable-prefix computation, `hh-l1-diff/1` and `hh-l1-conformance/1` packets | fixture-verified run on a toy harness pair with a seeded injected divergence, detected at the right step; a full run is an execution ticket |
| K5 L1 gate run (execution, $0 model) | gate-A + synthetic + census against frozen H | packet committed; **operator gate G-L1** |
| K6 O-only pilot (**paid**) | RECORD capture, pilot-quantities packet, seeded partitions | packet schema checks; **operator spend gate** |
| K7 L2 packet and analysis wiring | build `ParityAdmissionDecl` and the `directional_kl_equivalence` filters from tranche records; pre-registration `equivalence_margin`; validity-veto layer; claim-template fold | fixture-verified: synthetic pair records route through `hh-analysis` `kernel.rs` (l.1909, l.2117) to the expected typed refusals (`S3Floor`, `MissingReplay`, `NotFirstAttempt`, `LateMap`) and to `equivalent`/`inconclusive` on constructed counts reproducing Table B cells; a veto blocks the claim |
| K8 L2 tranche (**paid**) | schedule, simultaneous launch, block snapshots, stop rules | **operator gate G-L2** before; the packet recompute check after |
| K9 L1-on-L2 audit + L3 report | seeded audit sample; L3 distributions and discordance list | recompute from committed records |
| K10–K14 Terminus-2 stage | the same seams over T1–T13, once E2's terminal primitive and tokenizer decision exist | as above |

**Operator gates:**

- **G-pilot:** spend authorization for the O-only pilot.
- **G-L1:** L1 pass review, including the allowlist rule table.
- **G-L2:** ε, N, T, R, k, spend and the compute quote.
- **Publication gate:** the HUMAN row, per A-9.
- The same four for Terminus-2.

---

## Findings

### F1 — Landed admission refuses recorded model I/O; L1 must be a separate precondition gate
Claim: `parity_admission` accepts only `AuthenticFirstAttempt` evidence and refuses `RecordedModelIo` as `InauthenticEvidence`. Request-level replay conformance therefore cannot itself be parity admission evidence and must gate L2 rather than substitute for it.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-eval/src/parity_admission.rs:168-221` (evidence enum and `shape`), `:856-864` (only `AuthenticFirstAttempt` passes); module doc `:19-23`; spec §4.4 "supplied model-I/O fixtures … alone fail".
Retrieved: 2026-11-06T23:10Z
Implication for the spec: L1 is reported as `hh-l1-conformance/1` and gates L2. The parity claim is L1 pass ∧ L2 `equivalent` ∧ no veto.
Outline delta: CORRECTS OL-06. "The same parity machinery that verified 30/30" applies only at L2, and L1 is new machinery. EXTENDS OL-10.
Limitations: none.

### F2 — The landed validator fixes the L2 design skeleton
Claim: The validator enforces R ≥ 5 unconditionally (both floors), `pairs == T·R` with equal R per task, attempt == 1, distinct subjects, a `replay_ref` on every pair, a non-empty independence rationale, `independent_pairs` dependence, equal weights, a signed pre-data map, and margins only from the pre-registration.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `parity_admission.rs:48-59`, `:866-910`, `:919-1026`, `:1029-1171`; `benefits.rs:366-391`; `kernel.rs:2117-2392`.
Retrieved: 2026-11-06T23:12Z
Implication for the spec: Reattempts are impossible inside a tranche, so infrastructure failures become uncertain pairs in the completion envelope. `replay_ref` is defined as the pair's verbatim receipt-bundle digest.
Outline delta: EXTENDS OL-15 (parity declarations).
Limitations: The validator checks declarations, not truth. For example, `map_seq` ordering is the producer's authority.

### F3 — The precision engine reproduces §4.5, and ε ≤ .10 is unaffordable for this participant
Claim: A float reimplementation of `K_η` reproduces all seven §4.5 numbers. At the expected null discordance q± ≈ .117, the .90 TOST half-width is .142 at N = 150, .101 at N = 300 and .071 at N = 600. P(`equivalent` | Δ = 0) reaches ≥ .85 at ε = .20 with N = 150, at ε = .15 with N = 300, and at ε = .10 only with N ≈ 600–870.
Status: VERIFIED (computation); applying it depends on the q prior (F4)
Evidence kind: recorded execution (local `python3 -I`, scratchpad `d2/kl.py`, `tables.py`, `t2c.py`, run 2026-11-06T23:17Z–23:20Z; formulas as in spec §4.4)
Evidence: validation output `[.308497, .115703, .024293]` and `[.042944, .065355, .100601, .190899]`; Tables A and B.
Retrieved: 2026-11-06T23:20Z
Implication for the spec: ε and N are set at G-L2 from Table B evaluated at the pilot's conservative q, and the claim text states ε's scale against harness spread.
Outline delta: EXTENDS OL-06 (declared margins).
Limitations: The Rust code's integer-ppm outward rounding was not executed. It can only widen intervals, by at most 1 ppm per endpoint. The u-rows approximate the envelope.

### F4 — Null discordance prior q± ≈ .10–.14, and the pilot cannot pin it
Claim: Under exact parity, P(W±) = f/2. The HF board's TB2.1 rerun flip for mini-swe-agent 2.4.6 on GLM-5.3-Flash is 23.8 % (16.9–28.4 % across nine harnesses), so q± ≈ .117. A 16 × 3 O-only pilot estimates f with sd ≈ .08 under a calibrated task mixture.
Status: PARTIALLY VERIFIED (HF figures as read by C3; the mixture simulation is inference)
Evidence kind: source inspection + recorded execution (`pilot.py`, 20 000 replicates, seed 20261106)
Evidence: `docs/research/27_harness-survey.md` F2 table; HF estimator `2k(n−k)/(n(n−1))` per `docs/research/29_reference-sites.md` F2 (l.134); pilot sd .092/.080/.065/.070 for 12×3/16×3/24×3/16×4.
Retrieved: 2026-11-06T23:21Z
Implication for the spec: The quote picks N against an upper q bound. The pilot also measures the reliability endpoint's discordance.
Outline delta: EXTENDS OL-06.
Limitations: These are HF serving figures (C1 F16). The task mixture is illustrative.

### F5 — The cost per design spans one to five pilot budgets
Claim: Both surfaces are paid, so a design costs 2N × $/trial. At $0.05–0.40 per trial, design A (N = 150) costs $15–120 and design B (N = 300) costs $30–240, both before the 20 % reserve.
Status: UNVERIFIED (design proposal; arithmetic on cited priors)
Evidence kind: design proposal
Evidence: Table C; priors from C3 F2/F3 and C1 F9.
Retrieved: 2026-11-06T23:21Z
Implication for the spec: Only the pilot is fundable within the current $60 authorization with certainty. The tranche is a separate spend gate (A-3).
Outline delta: EXTENDS OL-15 (budget records).
Limitations: The $/trial on Ollama is unmeasured. Caching parity with HF is the dominant unknown (C1 F3, F7).

### F6 — L1 needs observation replay and live execution, separated by a self-replay control
Claim: L1 has three legs. L1.1 (responses and raw observations replayed into H, compared with the unmodified O's requests) is the exact gate. L1.2 (live H execution) is gated only on the replay-stable prefix measured by L1.0 (O against O). This is the only arrangement that tests H's executor without blaming H for environment nondeterminism.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal, grounded on E1 F9, F11, A.4, A.8
Evidence: E1 F11 (environment-dependent observations); E1 A.8 (the native trajectory allows request reconstruction); E1 "L1 feasibility".
Retrieved: 2026-11-06T23:14Z
Implication for the spec: The proxy needs strict REPLAY with an end-of-sequence refusal, and HH needs an observation-replay executor.
Outline delta: EXTENDS OL-10.
Limitations: Whether the native trajectory carries raw outputs is unverified; the fallback is the template inverse. Proposed validation: K4 fixture run with an injected divergence, then the K5 gate run.

### F7 — Canonicalization is a seven-rule, frozen, mutation-tested allowlist that never touches message text
Claim: `hh-l1-canon/1` (C01–C07) normalizes only encodings, verified environment facts and transport metadata. Tool-call ids, re-sent reasoning, prompts, tool schemas and observations are compared exactly. Rules change only by a version bump that burns the gate partition.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: E1 F2, F3, F7, F8, G4, G5 and REQ-XE-005; C1 F2 (no `tool_choice` upstream).
Retrieved: 2026-11-06T23:15Z
Implication for the spec: The rules are a declared, versioned artifact with golden and mutation tests (K1).
Outline delta: EXTENDS OL-06, OL-10.
Limitations: LiteLLM's actual wire fields are unobserved (E1 F7/F8). C06 is provisional until checked against the dev partition.

### F8 — Gate on the upstream-bound request, captured inside the proxy
Claim: Comparing the request after the proxy's lane transforms tests exactly what the model conditions on, and neutralizes HH wire differences that never reach the model (`tool_choice`). Differences that do reach the model still fail.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: C1 F2 and REQ-XC-012; E1 G1, G3, G4, G6.
Retrieved: 2026-11-06T23:15Z
Implication for the spec: C2 must run lane transforms in REPLAY mode and expose both capture points (P2, P8).
Outline delta: EXTENDS OL-06.
Limitations: This depends on C2's upstream dialect choice (C1 F3 suggests native `/api/chat`).

### F9 — Corpus: pilot-harvested recordings, synthetic branches and a turn-1 census
Claim: L1 needs no model spend of its own. Natural trajectories come from the O-only pilot's RECORD capture (about 900 requests). Rare branches come from ≈ 25 synthetic sequences replayed to the unmodified original. Every tranche task's instruction-dependent first request comes from a one-response census.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: E1 A.2–A.6 (branch inventory), C3 F2 (31.8 calls per trial), REQ-XC-013.
Retrieved: 2026-11-06T23:16Z
Implication for the spec: The partitions are committed before H development, and the coverage map is part of the L1 claim.
Outline delta: EXTENDS OL-10.
Limitations: Synthetic sequences test mechanics, not realistic distributions.

### F10 — Participant sameness holds at configuration-record level, with five typed unknowns
Claim: For this participant, "same participant/configuration" can be asserted only as equality of the pair's participant records. The served weights, the cache, seeds, determinism and the `central_protocol` support gate are typed unknowns or refusals. They force a claim scoped to the catalog identity and the tranche window.
Status: PARTIALLY VERIFIED (C1 facts and `central_protocol.rs:637-649` verified; the scoping is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: C1 F4, F7, F12, F13, F15; `central_protocol.rs:208-216, 637-649`.
Retrieved: 2026-11-06T23:13Z
Implication for the spec: Both arms carry the same pre-registered record check. L2 depends on the §4.1 ADR shared with D1.
Outline delta: CORRECTS OL-06 — the "same participant" wording needs this scoping for an LLM participant.
Limitations: The provider may vary numerics and region per request with no observable signal (C1 F4).

### F11 — Dimension map: two required binary endpoints plus a validity-veto layer
Claim: Capability and reliability (submit path) are required, with one ε and an IUT conjunction. The other five dimensions are typed `inapplicable` with reasons. Integrity, egress, cheating and intervention are checked through a veto layer that blocks the claim.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal, grounded on spec §4.4 and `parity_admission.rs:1029-1171`
Evidence: closed dimension set `:65-73`; `RequiredInapplicable` `:1072-1086`; conjunction `directional_kl.rs:1612-1621`.
Retrieved: 2026-11-06T23:18Z
Implication for the spec: The veto layer is new claim-assembly logic (K7). The landed estimator has no veto member.
Outline delta: EXTENDS OL-06, OL-15.
Limitations: Making efficiency non-required narrows the claim, and that narrowing must be stated.

### F12 — Infrastructure missingness is a first-class precision driver
Claim: At N = 300, q = .117 and ε = .15, P(`equivalent`) falls from .926 at u = 0 to .832 at 1 % uncertain pairs and .42 at 3 %.
Status: VERIFIED (computation, with the stated approximation)
Evidence kind: recorded execution (`t2c.py`, 2026-11-06T23:20Z)
Evidence: power output `u=1%: [0.0, 0.832, 0.999]`, `u=3%: [0.0, 0.42, 0.983]`.
Retrieved: 2026-11-06T23:20Z
Implication for the spec: The pilot must measure u. The quote uses it, and an operational stop fires on a pre-declared u threshold.
Outline delta: EXTENDS OL-06.
Limitations: The u rows approximate the envelope.

### F13 — Conflict: C1 F6 calls mini-swe-agent a text-protocol harness, but E1 shows native tool calling
Claim: E1 verified from source that mini-swe-agent 2.4.6 under Harbor always sends `tools=[BASH_TOOL]`. Ollama's GLM tool-call parser is therefore in mini's loop: symmetric for both arms in L2, invisible in L1 because replayed responses are already parsed. Only Terminus-2 is a plain-text protocol.
Status: VERIFIED (by reading both accepted notes; E1 is source-based)
Evidence kind: source inspection (notes)
Evidence: E1 F2 and A.4 (`litellm_model.py:64-74`); C1 F6, Claim and Implication (b).
Retrieved: 2026-11-06T23:11Z
Implication for the spec: Receipts must keep raw and parsed tool calls (REQ-XC-013), so that parser-attributed discordances are visible in L3.
Outline delta: CORRECTS a C1 statement; no OL change.
Limitations: none.

### F14 — Terminus-2 adds a terminal observation seam and a tokenizer dependency
Claim: Terminus-2's L1 needs replay at the tmux `capture-pane` level and O-side exec capture. Its summarization branches can pass L1 only if H reproduces LiteLLM's `token_counter`, or registers a declared divergence.
Status: UNVERIFIED (inference from E1 B.4–B.5 plus design proposal)
Evidence kind: inference + design proposal
Evidence: E1 B.4, B.5, F6, F10, F11, G13, G16.
Retrieved: 2026-11-06T23:19Z
Implication for the spec: E2 must decide the tokenizer, possibly through a dependency ADR, before the Terminus L1 gate.
Outline delta: EXTENDS OL-10.
Limitations: HH's compaction-family fit was not assessed (E1 G16).

### F15 — Methods recommendations on E1's open questions
Claim: R1–R6 (Part 6): supply `model_info`; reproduce uname with verified substitution; keep re-sent reasoning; Harbor defaults plus declared external caps; resolve LiteLLM once and freeze; any effort pin, leaning `high`.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: E1 F1, F3, F7, F8, F10; C1 F5.
Retrieved: 2026-11-06T23:21Z
Implication for the spec: These feed the Q-X29 and Q-X22 ratification questions.
Outline delta: EXTENDS OL-06.
Limitations: R3 depends on what the dev partition shows O re-sending.

### F16 — Phase-4 baseline is one frozen, dated configuration, and variants carry no parity claim
Claim: "Parity-verified baseline" names the exact frozen H record that passed G-L1 and L2. Variants differ by one registered slot, must pass declared-delta conformance, and are compared with a re-run baseline, never with the original.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal, grounded on spec §4.5 and the `BaselineOnly`/transfer refusals (`parity_admission.rs:1173-1262`)
Evidence: as cited.
Retrieved: 2026-11-06T23:21Z
Implication for the spec: D3 consumes the baseline record and the declared-delta check.
Outline delta: EXTENDS OL-06, OL-10.
Limitations: none.

---

## Answer and completion assessment

**Answer.** The layered method is:

- **L1:** three legs over a frozen, partitioned corpus (pilot recordings, synthetic branches, turn-1 census), compared at the proxy's upstream-bound capture under a seven-rule frozen allowlist. One divergence fails.
  - L1.0 is the O self-replay control that defines stable prefixes.
  - L1.1 is exact construction conformance, with observation replay.
  - L1.2 is executed conformance on the stable prefixes.
- **L2:** §4.4 paired first-attempt KL parity. It uses R ≥ 5, simultaneous within-pair launch, seeded interleaving and configuration-record sameness with five typed unknowns. Infra failures enter the completion envelope. ε and N are set at G-L2 from an O-only pilot. The precision tables show ε ≈ .15 at N ≈ 300, or ε ≈ .20 at N ≈ 150, under the expected null discordance; ε ≤ .10 is not fundable at pilot scale.
- **L3:** distributions only.
- **Dimension map:** two required binary endpoints and five reasoned inapplicables, plus a validity-veto layer.
- **Gating:** L1 gates all paid L2 work; only the O-only pilot precedes it.

**Settled:** the constraints imposed by landed code (F1, F2), the computations (F3, F12), and the conflict F13.

**Acceptable uncertainty for this row:**

- LiteLLM's wire fields (the C06 calibration);
- whether raw observations are present in mini's trajectory;
- Ollama's $/trial and q (the pilot);
- B1's feasible T;
- C2's dialect choice.

**Technical blockers, all owned by other rows:**

- for any L2: the §4.1 support-gate ADR (D1/C1), and an `hh-budget` pricing unit able to represent these rates (C1 F14);
- for the mini L1: the E2 gaps G1–G4, G6 and G8–G10;
- for the Terminus L1: G13 and the tokenizer.

## Open questions

1. **ε/N selection authority.** Owner: operator at G-L2. Affects: D2, H1. Operator decision: **yes**. Should the operator accept a ±.15 or ±.20 verdict margin, which excludes only gross infidelity, as the phase-3 parity claim, given that ±.10 costs several pilot budgets?
2. **Q-X29.** Owner: operator at ratification. Affects: D2, E2, C4. Operator decision: **yes**. Adopt R4 (Harbor in-agent defaults plus declared external caps) and R5 (resolve LiteLLM once, then freeze)?
3. **Q-X22 effort.** Owner: operator. Affects: C2, D1, D2. Operator decision: **yes**. D2 is indifferent for validity and leans `high` on cost.
4. **Pilot spend.** Owner: operator at G-pilot. Affects: D1, D2, H1. Operator decision: **yes**. The O-only pilot (48 trials, ≈ $2–19) shares the $60 with D1's phase-2 pilot. Should it run as part of, or alongside, the phase-2 mini-swe-agent pilot arm? Recommended: yes, the same O configuration; that also needs Q-X21 (credit mechanics).
5. **Raw observations in the mini trajectory.** Owner: E2/B2. Affects: K2. Operator decision: no.
6. **Terminus tokenizer.** Owner: E2. Affects: the Terminus L1. Operator decision: only if a dependency ADR is contested.
7. **Home of `l1-canon`.** Owner: E2/H1. Affects: K1. Operator decision: no.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale / findings | OL | Acceptance |
|---|---|---|---|---|
| REQ-XD-101 | Phase-3 parity SHALL be claimed only as the conjunction: L1 gate pass ∧ L2 IUT conjunction `equivalent` ∧ an empty validity-veto list, under one pre-registered admission packet. | F1, F11 | OL-06, OL-10 | claim-fold fixture: each conjunct false ⇒ claim withheld |
| REQ-XD-102 | No paid L2 pair SHALL run before G-L1 records a pass for the exact H version used. Any H change after G-L1 requires a full L1 rerun, and a new cohort if a tranche is in progress. | F1 | OL-06, OL-15 | runner refuses to schedule L2 without a matching `hh-l1-conformance/1` pass id |
| REQ-XD-103 | L1 SHALL run legs L1.0 (O self-replay, stable prefix), L1.1 (response + observation replay into H, whole trajectory) and L1.2 (live H execution, gated on the stable prefix), with first-divergence-fails semantics. | F6 | OL-10 | K4 fixture: an injected divergence is detected at the correct step and leg |
| REQ-XD-104 | L1 equality SHALL be byte equality of canonicalized upstream-bound requests per request, plus equal parsed actions per step, equal request count, termination class and submission, and no extra request. | F6, F8 | OL-06 | K4 tests for each equality level |
| REQ-XD-105 | Canonicalization SHALL be the versioned allowlist `hh-l1-canon/<v>` (initially C01–C07), using the repository canonical JSON. It never alters string values except through verified environment substitution. Each rule SHALL have a positive and a mutation test. | F7 | OL-06, OL-15 | K1 golden + mutation battery |
| REQ-XD-106 | Allowlist changes after a gate run SHALL bump the version and rerun on an unused gate partition. Gate partitions SHALL be committed by digest before H development. | F7, F9 | OL-15 | partition-digest commit precedes the H freeze commit; version-bump test |
| REQ-XD-107 | The L1 corpus SHALL comprise pilot RECORD trajectories (≥ 12 dev, 8 gate-A, 8 gate-B), synthetic branch fixtures covering M1–M19 (T1–T13 for Terminus-2), and a turn-1 census of every manifest task. | F9 | OL-10 | coverage map in the L1 packet; a missing branch ⇒ gate refuses |
| REQ-XD-108 | The proxy SHALL provide RECORD, sequence-REPLAY (strict and lenient), synthetic-sequence ingestion, typed end-of-sequence refusal, and both capture points, with lane transforms applied in REPLAY. (Interface for C2.) | F6, F8 | OL-06 | C2 fixture tests P1–P9 |
| REQ-XD-109 | Every L1 divergence SHALL produce a durable `hh-l1-diff/1` record. L1 results SHALL be published as `hh-l1-conformance/1`, with the coverage table and stable-prefix histogram. | F6 | OL-15 | schema tests; a diff record exists for every failed trajectory |
| REQ-XD-110 | L2 pairs SHALL be `(task, repeat, pair_id)` first attempts in fresh cells, with R ≥ 5, N = T·R, equal weights and distinct subjects. `replay_ref` is the digest of both arms' verbatim receipt bundles and trajectories. | F2 | OL-06 | `parity_admission` admits the fixture and refuses the S3Floor/MissingReplay/NotFirstAttempt mutants |
| REQ-XD-111 | Pairs SHALL run in a seeded random order with simultaneous H/O launch on the same VM and proxy, in blocks fixed at G-L2, with a catalog-identity snapshot at each block boundary. | F10 | OL-06 | schedule recompute from seed; snapshot count = blocks + 1 |
| REQ-XD-112 | The two participant records of each pair SHALL be equal on every non-harness member (lane, catalog identity, served-model echo, effort, sampling-unset, caps, account, environment, verifier). A pair failing the check is uncertain. | F10 | OL-06 | equality-check fixture |
| REQ-XD-113 | The packet SHALL declare the typed unknowns: served weights, cache, seeds, determinism and the support gate, with the claim-scope sentence (catalog identity, window), and SHALL NOT claim CRN or seed coupling. | F10 | OL-06, OL-15 | review checklist; `use_as` absent |
| REQ-XD-114 | Infrastructure classes SHALL be declared before data and be symmetric across arms. An infra-affected pair is uncertain within the completion envelope at the planned N, never reattempted. | F2, F12 | OL-06 | envelope fixture equal to the hand computation |
| REQ-XD-115 | ε, N, T, R and block size SHALL be fixed at operator gate G-L2 from an O-only pilot. The pilot measures f (capability and reliability), $/trial mean and P90, cache share, u, queue time and wall time. N is quoted against a conservative q bound. | F3, F4, F12 | OL-06, OL-15 | the quote packet cites the pilot records and Table B rows |
| REQ-XD-116 | The dimension map SHALL make capability and reliability (submit path) required binary endpoints with one absolute ε, and type grounding, efficiency, autonomy, security_compliance and portability_evolvability `inapplicable` with the reasons in §4. | F11 | OL-06 | `parity_admission` admits the map; making a required dimension inapplicable refuses |
| REQ-XD-117 | A pre-registered validity-veto layer (proxy-only egress, cheating scan, HH integrity on H, zero interventions) SHALL block the claim on any observed veto. Undeterminable predicates SHALL be listed before data as `not_checked{reason}`. | F11 | OL-06, OL-15 | claim-fold fixture with one veto |
| REQ-XD-118 | Stop rules SHALL be operational only: catalog change, budget exhaustion, and the infra threshold at declared checkpoints. Remaining planned pairs become uncertain. | F2, F10 | OL-06 | stop fixture leaves N unchanged |
| REQ-XD-119 | A seeded post-tranche L1-on-L2 audit SHALL replay sampled O recordings to the frozen H under L1.1. A failure SHALL fail the parity claim. | F6 | OL-10 | audit packet; failure fixture |
| REQ-XD-120 | L3 SHALL report the distributions listed in Part 3, per arm and per-pair difference, plus every discordant pair, without equivalence wording. | — | OL-10 | publication claims-language check |
| REQ-XD-121 | The O reference SHALL use one image built once, with all resolved Python dependency versions (LiteLLM included) recorded and frozen, for pilot, L1 and L2. | F15 (R5), E1 F7 | OL-06 | image digest and lock recorded in every participant record |
| REQ-XD-122 | The reference configuration SHALL use Harbor's in-agent defaults, with the agent wall timeout and proxy budget ceiling declared. A ceiling binding is infra-uncertain. | F15 (R4), E1 F1 | OL-06 | config record matches the Harbor flags; binding fixture |
| REQ-XD-123 | H SHALL render the cell's actual uname, and SHALL re-send exactly the assistant members O re-sends, as determined on the dev partition. | F15 (R2, R3) | OL-06 | L1.1 turn-1 census and M18 pass |
| REQ-XD-124 | The Terminus-2 reference SHALL declare `model_info` (`max_input_tokens = 1 048 576`; `max_output_tokens` = the proxy cap). Summarization coverage SHALL use a separately declared synthetic L1 configuration applied identically to H and O. | F14, F15 (R1) | OL-06, OL-10 | participant record; T9/T10 pass under the synthetic config |
| REQ-XD-125 | A swept variant SHALL differ from the parity-verified baseline in exactly one registered slot, SHALL pass declared-delta conformance on the synthetic corpus, and SHALL NOT carry any parity claim against the original. | F16 | OL-07 | variant-diff fixture: an undeclared-field change refuses |
| REQ-XD-126 | "Parity-verified baseline" SHALL name the frozen H record (HIR digest, artifact digest, HH commit, participant record, catalog digest and window). It is dated; L2 `inconclusive` yields the label "L1-conformant baseline" only. | F16 | OL-06, OL-12 | claims-language mutation test |

## Handoff

**Dependencies resolved.** C1 (participant facts) and E1 (anatomy) are consumed in full. A1 is consumed for the S3 contrast: no landed estimator ever ran, and the probe is undeterminable.

**Interfaces asserted on other rows:**

- C2: P1–P9 and REQ-XD-108.
- B2/C4: O-side exec capture for Terminus; the Harbor verifier channel identical across arms; the infra classification.
- E2: G1–G4, G6, G8–G10 and G17 for mini; G13, G16 and the tokenizer for Terminus; the observation-replay executor seam.
- B1: the feasible T and the dataset digest.
- B3: the fresh VM and the compute quote.
- D1: the shared §4.1 support-gate ADR, the cheating-scan policy, the shared pilot, and Q-X22.

**Proposed follow-ups.** H1 sequences the tickets K1–K14 (Part 7) and the operator gates G-pilot, G-L1, G-L2 and publication. E1's proposed "$0 L1 capture spike" is subsumed by K3–K5 plus the pilot RECORD capture.

**Conflicts.** C1 F6 vs E1 F2 (F13): E1 governs. The brief's phrase "the same parity machinery that just verified 30/30" (§2) is corrected by A1 F2/F3 and by F1 here.

**Capability and portfolio dispositions.** None changed. Nothing in this note is fixture-, live- or foreign-run-verified. The computations are local arithmetic over published formulas.
