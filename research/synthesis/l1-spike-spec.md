# WS-L1 spike specification — S1 kernel slice, S2 boundary slice (specification only; no code)

**Written:** 2026-09-10 by the WS-L1 ratification subagent (ADR-0049 §5 item ii; binding addendum of the ratification brief). **Status:** specification; execution deferred to **Stage 0 of the build ladder** as an acceptance check, after the Decompose-Readiness Gate. **Consumers:** `decompose-spec` (one Stage-0 ticket per spike, throwaway by rule — ADR-0009 step 4 "spikes are throwaway; their outputs are numbers in the matrix, not code in the repo"); the phase synthesis agent that re-runs ADR-0009 steps 5–7 if trigger 5 of ADR-0050 §6 fires.

> This document contains no implementation, no library names and no code. It states what is measured, how, and what result would revalidate ADR-0050. Language names appear only where ADR-0050 already binds a layer.

## 1. Purpose

ADR-0009 step 4 assigned three criteria's evidence to two time-boxed spikes: C5 (fan-out cost per concurrent session), C7 (memory per idle session, startup, event-append + view throughput at N concurrent sessions) and C12 (per-sample boundary-crossing overhead; codegen round-trip). The program mandate (doc 3 §11.0) forbids framework code, including prototype spikes, before the Decompose-Readiness Gate; ADR-0050 therefore scored those criteria from citable evidence, marked them `unmeasured-by-spike`, swept them with a ±1 band, and recorded revalidation trigger 5: *"the Stage-0 spike result contradicts the assumed scores."* This specification makes that trigger operational.

## 2. Common rules (both spikes)

- **R1 One spec, fresh executors.** Each candidate's spike is implemented by a fresh agent from this specification alone, time-boxed to one working session, in a throwaway directory outside the product tree; nothing is merged (ADR-0049 §5 iii).
- **R2 Cross-camp review.** Each spike is reviewed for ecosystem bias by a reviewer briefed on a *different* candidate (OQ-047, ADR-0049 §5 iv); the reviewer signs the measurement sheet.
- **R3 Same host, same inputs.** All candidates run on one host class (recorded: CPU model, cores, RAM, OS, kernel version, container runtime), same synthetic workload (§3.3), same wall-clock budget; three repetitions, report median and p95.
- **R4 Measurements are numbers, not code.** The output of a spike is the measurement sheet (§6) appended to the ADR-0050 amendment log; the code is deleted.
- **R5 Provisional discipline.** Spike numbers are internal measurements (tier B by construction); they never support a performance *claim* outside the ADR-0009 matrix.
- **R6 Candidates measured.** The candidates named in ADR-0050 §Options whose kernel ecosystem differs: **E1, E2, E3** (E4 only if a sponsor asks — it lost every corner). S2 runs for the winning split (E1 kernel + E2 lab) and for the two nearest rivals' splits (E5b: E3 kernel + E2 lab; E5c: E2 kernel + E3 surface), so the C12 band can be checked on more than one point.

## 3. S1 — kernel slice

### 3.1 Scope (what the slice must do; contracts by ADR)
1. **Append-only ledger** with the ADR-0027 id model: allocated time-ordered `run_id`/`event_id`, per-run dense `seq`, `parent_event_id`, `lease_generation` on every event, atomic multi-event append, durable-before-visible (ADR-0026 §1–6); canonical form of `idp/1` (JCS-class, integers + decimal strings, no doubles — ADR-0029, ADR-0036) and the per-run hash chain `hash = H(leaf_tag ∥ canonical(envelope − hash) ∥ prev_hash)`.
2. **One `project()` view** with a `derived_from` watermark and the rebuild-equality test (ADR-0026): the effect ledger view (B2) or the cost-totals view (L2) — same view for every candidate.
3. **One sandboxed tool call through a helper-binary boundary** (OQ-045 provisional): the kernel spawns a helper process, sends a narrow request (`execute(command, cwd, policy)`), receives `{exit_code, stdout_hash, stderr_hash, duration_ms}`; the helper applies whatever OS primitive the host offers (bubblewrap / Seatbelt / job object) — the *same* external sandbox tool for all candidates so only the drive path is measured.
4. **One MCP client call** to a fixed local reference server (tool `echo`), using the candidate's Tier-1/official MCP SDK (S-155).
5. **One ACP stdio session** (`initialize → session/new → session/prompt → cancel → close`) against a fixed local reference agent, using the candidate's official ACP library (S-157).
6. Steps 3–5 each write their events to the ledger of step 1 and are visible in the view of step 2.

### 3.2 Excluded
No model calls (a stub returns a fixed completion), no compiler, no reference monitor, no real tools beyond `echo`/`true`, no persistence beyond one local embedded store, no UI.

### 3.3 Workload
- **N = 1** and **N = 50** concurrent sessions; each session appends 200 events (mix: 60 % ≤ 1 KiB, 35 % 8–16 KiB, 5 % exactly 64 KiB — the ADR-0029 offload threshold, offloaded to the blob store), performs 20 sandboxed calls, 20 MCP calls and one ACP session; total run ≤ 5 minutes.
- Identical synthetic event payloads (a fixed corpus file with 200 canonical events per session, same bytes for every candidate) so hash chains must match **byte-for-byte across candidates** (correctness gate G1).

### 3.4 Measurements (C5, C7)
| id | metric | unit | how |
|---|---|---|---|
| M-S1-1 | process start to first ledger event visible | ms | monotonic clock, median of 3 |
| M-S1-2 | resident memory per idle session at N = 50 (total RSS / 50 after 30 s idle) | MiB | OS process accounting |
| M-S1-3 | sustained append throughput at N = 50 with durable-before-visible | events/s (aggregate) and per session | count / wall time |
| M-S1-4 | `project()` rebuild latency for 200 events, and incremental update latency per event | ms | monotonic clock |
| M-S1-5 | fan-out cost: wall time of N = 50 vs N = 1 (ideal 1.0 if perfectly parallel on ≥ 50 cores; report also CPU-seconds) | ratio | wall and CPU accounting |
| M-S1-6 | sandboxed call overhead: helper round-trip minus the bare command time | ms | median of 20 |
| M-S1-7 | MCP call round-trip; ACP session setup time | ms | median of 20 / of 3 |
| M-S1-8 | cancellation latency: `cancel` to all session tasks stopped, at N = 50 | ms | monotonic clock |
| M-S1-9 | streaming tail lag: `subscribe` delivery latency at N = 50 | ms | median, p95 |

### 3.5 Correctness gates (fail ⇒ the candidate's C3/C6 scores are re-examined, not only C5/C7)
- **G1** hash chain identical across candidates for the shared corpus (canonical form is implementation-independent — ADR-0029 property 5).
- **G2** rebuild-equality: `project()` from scratch equals incremental view at every `until_seq`.
- **G3** after a simulated crash mid-append (process killed), the ledger replays with no partial multi-event append visible and the lease fence rejects the stale writer (ADR-0026, ADR-0027).
- **G4** a 64 KiB event is offloaded and its blob address is resolvable; a > 2^53 integer and a decimal-string value round-trip unchanged (ADR-0029, OQ-060).

## 4. S2 — boundary slice (polyglot candidates only)

### 4.1 Scope
The S1 kernel of one ecosystem is driven by a *lab side* in another ecosystem over the declared boundary mechanism: **(a)** subprocess + JSON-RPC over stdio carrying the WS-L1 §6.5 verb set (`hello, open_session, submit, stream_events, request_permission, cancel, account, close`) and the measurement operations of ADR-0045 §9; **(c)** the lab side's types are generated from the kernel's exported schema (HIR/1 event envelope, `MetricValue`) and a CI-style drift check is run once. For E5c the surface side is measured instead of the lab side, over **(b)** localhost HTTP/WS with a generated client.

### 4.2 Workload
- Per-run pattern (the admissible one, ADR-0045): open 50 sessions, run each to completion, then per session: fetch end-state handle + ledger cursor, compute one `MetricValue` lab-side, append `measurement.metric.emitted`.
- Per-sample pattern (the *disallowed hot path*, measured only to quantify what the contract forbids): the lab side is called once per model-call stub (200 per session).
- Codegen round-trip: change one field in the kernel schema, regenerate the far-side types, observe the drift check fail, then pass after regeneration; record human/agent time and CI time.

### 4.3 Measurements (C12)
| id | metric | unit |
|---|---|---|
| M-S2-1 | per-run crossing overhead: total wall time E5 split minus the same kernel driven natively (S1) at N = 50 | ms and % |
| M-S2-2 | per-sample crossing overhead (disallowed pattern): median round-trip of one crossing | ms |
| M-S2-3 | serialization + validation cost per event on the far side (canonical form parse + hash verify) at 16 KiB and 64 KiB | µs |
| M-S2-4 | codegen round-trip: schema change → regenerated bindings → drift check green | minutes (agent time) and s (CI) |
| M-S2-5 | two-toolchain CI time: clean build + test of both sides | minutes |
| M-S2-6 | `hello` version-mismatch refusal and `open_session(resume=…)` after a killed peer (I2, failure modes of §6.5) | pass/fail |
| M-S2-7 | hash-equality of events across the boundary (I4 as reworded by CF-058): every far-side event re-canonicalised hashes to the kernel's hash | pass/fail (100 % required) |

## 5. Pass thresholds and the revalidation rule

Thresholds convert measurements into ADR-0009 scores (0–5) so the assumed `u` scores in ADR-0050 §3 can be checked. Bands are deliberately coarse; the decision was insensitive to ±1.

| criterion | score 5 | score 4 | score 3 | score 2 | ≤ 1 |
|---|---|---|---|---|---|
| **C5** (from M-S1-5, M-S1-8, M-S1-9) | fan-out ratio ≤ 1.5× at N = 50 *and* cancel ≤ 100 ms *and* tail p95 ≤ 50 ms | ≤ 2.5× / ≤ 250 ms / ≤ 100 ms | ≤ 5× / ≤ 1 s / ≤ 250 ms | ≤ 10× / ≤ 2 s / ≤ 1 s | worse |
| **C7** (from M-S1-1, M-S1-2, M-S1-3) | start ≤ 50 ms *and* ≤ 10 MiB/session *and* ≥ 5 000 events/s aggregate durable | ≤ 150 ms / ≤ 30 MiB / ≥ 2 000 | ≤ 400 ms / ≤ 80 MiB / ≥ 500 | ≤ 1 s / ≤ 200 MiB / ≥ 100 | worse |
| **C12** (E5 only; from M-S2-1, M-S2-4, M-S2-5, M-S2-7) | per-run overhead ≤ 2 % *and* codegen round-trip ≤ 10 min *and* CI ≤ 1.5× single-toolchain *and* M-S2-7 100 % | ≤ 5 % / ≤ 20 min / ≤ 2× | ≤ 10 % / ≤ 45 min / ≤ 3× | ≤ 25 % / ≤ 2 h / ≤ 5× | worse, or M-S2-7 < 100 % |

The ADR-0026 target ("tens of events/s per run") is far below every C7 row; C7 therefore discriminates only footprint and start-up, as its low weight (4.08) intends.

**Revalidation rule (ADR-0050 §6 trigger 5).** For each measured candidate, compare the spike-derived score with ADR-0050 §3. If any candidate's C5, C7 or C12 score differs from the assumed value by **more than 1**, or any correctness gate G1–G4 / M-S2-6 / M-S2-7 fails for the *winning* candidate, the phase synthesis agent re-runs ADR-0009 steps 5–7 with the measured scores substituted (weights stay frozen — §0 of ADR-0050) and records the result in the ADR-0050 amendment log. Because Stage 0 has produced only toolchain and CI, reversal cost at that point is low (ADR-0050 §5). If every measured score is within the band, the `u` marks are cleared in the amendment log and the decision stands unchanged.

## 6. Measurement sheet (to be appended to ADR-0050's amendment log per candidate)

`{candidate, executor, reviewer (camp), host, commit_of_spec, date, M-S1-1…9, G1…G4, M-S2-1…7 (E5 only), derived_scores: {C5, C7, C12}, within_band: bool, notes}`

## 7. Time-box and ownership

- One session per candidate per spike; three candidates × S1 + three splits × S2 = at most six sessions, run in parallel by fresh agents at Stage 0.
- Owner: the Stage-0 ticket owner named by `decompose-spec`; reviewer assignment by the orchestrator (cross-camp rule R2).
- Outputs: measurement sheets only; spike trees deleted after the sheet is signed.
