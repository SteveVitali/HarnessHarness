<!-- docs/build/LEDGER.md — machine-state file (BM-LEDGER-01..07). Seeded by decompose-spec.
     STATE ONLY — the manifest is the plan. CURRENT STATE keys in EXACTLY this order. -->
# Build ledger — the machine-state file

> **OPERATING MODE.** Fresh session: read this ledger, then `docs/tickets/00_MANIFEST.md`,
> then `docs/tickets/DEFERRALS.md`. Run the row named by `nextTicket` via `implement-spec`;
> a gate is a pause, not a block. Resume line:
> `implement-spec spec=docs/tickets/<nextTicket file> worktree=<buildWorktree> base_branch=<chainTip>`

## CURRENT STATE

```
projectStatus:   IN_PROGRESS        # NOT_STARTED | IN_PROGRESS | BLOCKED | PAUSED | DONE
nextTicket:      GATE-G1
lastCompleted:   S0.3b
blockedOn:       (nothing)          # REAL blocks only; a pending gate is a RETURN PASS row
pauseRequested:  false
returnPass:      (none)
manifest:        docs/tickets/00_MANIFEST.md
canonicalSpec:   spec/CANONICAL_SPEC.md
memoryRoot:      docs/build
dispatchTarget:  subagent
buildWorktree:   /Users/stevenvitali/MetaHarness-harnessharness
buildBranchBase: svitali/harnessharness
pinnedBaseSha:   85a3960640b5fcdc1c1627b04a50c005a7271f8e
chainTip:        svitali/harnessharness-s0.3b
benchmarkSet:    PENDING_CREATE     # Stage-3 reference suite + exemplars; created when the chain reaches Stage 3
autonomy:        manual
mergePolicy:     OPERATOR           # NONE | OPERATOR | AUTO-BOTTOM-UP
round:           1
updatedAt:       2026-09-15
```

## OPEN FINDINGS

(none — carry cross-ticket findings here; not per-ticket blocks)

## GATE DECISIONS

| date | ticket | gate | item | answer | consequence |
|---|---|---|---|---|---|
| 2026-09-15 | S0.3 | Stage-0 spike budget (operator-gated live stage) | release spike budget | authorized — offline/hermetic only, no external spend | runs the S1/S2 measurement spikes (repeat-scored N≥3); discharges DF-S0.1-1, DF-S0.2-1; produces the measurement sheet feeding GATE-G1 |
| 2026-09-15 | GATE-G1 | Stage-0 acceptance & ecosystem-decision revalidation | disposition | PENDING — authorize an ONLINE spike budget first | insert S0.3b to close the machine cells of DF-S0.3-1/-3 (online reference peers + E2/E3 candidate toolchains, no external model spend); gate stays STOPPED until S0.3b lands and the sheet is re-read; the R2 human cross-camp signature stays an operator residual |

## RETURN PASS

| ticket | gates | what the operator must do | re-run line |
|---|---|---|---|

## PHASE LOG

- 2026-09-15 · S0.3b done — branch `svitali/harnessharness-s0.3b` · PR https://github.com/SteveVitali/MetaHarness/pull/5 · base `svitali/harnessharness-s0.3` (@6ca7edb). Ran the **online** cross-candidate / MCP-ACP measurement spike (operator released the online spike budget: network + E2/E3 toolchains + official MCP/ACP SDKs against LOCAL reference peers, no external model spend) to close the machine cells of DF-S0.3-1 and DF-S0.3-3. Throwaway spike `spikes/s0.3b-online-spike/` (outside the workspace): E1 emits the shared corpus + reference head; E2/E3 implement independent from-spec kernels with their OWN canonical serializers; official SDKs measure M-S1-7; E5b/E5c splits measure C12 transport + cross-ecosystem hash-equality. Repeat-scored **N=5**; extended sheet appended to `docs/build/reports/S0.3-measurement-sheet.md` (S0.3b §1–§5) and the ADR-0050 amendment log (candidate ids only, CC4). **Revalidation (feeds GATE-G1): the winning decision E5a HOLDS/re-confirmed; trigger 5 ARMS on the non-winning candidate E2** (C5 4→1, C7 2→5 outside ±1; E3 within band) — routed to GATE-G1's phase-synthesis for the steps-5–7 re-run, which does NOT move the winner (E2's kernel scores don't affect E5a where E1 is the kernel and E2 the per-run/IO-bound lab; ADR-0050 §4 kernel-family robustness 0.979). **Verify:** `test-gates.sh` exit 0 (G1 E1=E2=E3 head `f0b9620d…` byte-identical + live negative test; M-S1-7 MCP echo + ACP session verified E2/E3; M-S2-7 hash-equality 100% E5b/E5c); headline medians — G1 head identical, M-S1-7 MCP E2 0.71 ms/E3 0.15 ms, ACP E2 220 ms/E3 54 ms, E5b overhead 1.60%/E5c 4.46%; workspace 84 tests/16 suites green (spikes excluded), fmt clean. **Deferrals:** closed DF-S0.3-1; DF-S0.3-3 machine cells DONE with the **R2 human-signature residual OPEN**; DF-S0.3-2 (polyglot CI + cross-ecosystem serialization COST) NOT pulled forward (Stage 3). **Deviations:** single-executor (R1) with the objective G1 byte-identity gate as compensating control (ADR-0225); committed-but-throwaway trees (ADR-0225 extends ADR-0223/0220). No harness claim at Stage 0 (R5). chainTip → svitali/harnessharness-s0.3b · next → GATE-G1 (STOP gate: operator re-reads the extended sheet + verdict; owns the armed-trigger steps-5–7 re-run before Stage 1).

- 2026-09-15 · **inserted** S0.3b (`003a_S0.3b__cross-candidate-online-spike.md`, row 3a) between S0.3 and GATE-G1 — operator dispositioned GATE-G1 as PENDING and authorized an ONLINE spike budget to close the machine cells of DF-S0.3-1 (MCP/ACP official-SDK round-trip + cross-candidate G1 byte-identity) and DF-S0.3-3 (E2/E3 cross-candidate scoring + E5b/E5c). Network egress + candidate toolchains (E2/E3) available; no external model spend. R2 human cross-camp signature stays an operator residual; DF-S0.3-2 (Stage-3 polyglot CI) not pulled forward. GATE-G1 readout `docs/build/readouts/GATE-G1.md` reading 1 written; gate STOPPED until S0.3b lands. nextTicket → S0.3b.

- 2026-09-11 · Ledger created by decompose-spec from `spec/CANONICAL_SPEC.md` (v1.0-rc2, PASS 7/7); 100 implement-spec tickets + 3 milestone gates + 2 human prerequisites + GATE-ACCEPT (106 chain rows total); dispatch=subagent; plan is revisable at run time.
- 2026-09-15 · SETUP done — worktree `MetaHarness-harnessharness` created off 85a3960 on branch `svitali/harnessharness` (operator-chosen prefix, not `whoami`); params resolved (dispatch=subagent, autonomy=manual, mergePolicy=OPERATOR); seed already committed at base (85a3960) so no separate first commit; baseline trivially green (no build system exists pre-S0.1); benchmarkSet left PENDING_CREATE (Stage-3 reference suite, not creatable at Stage 0); check-build-memory exit 0. nextTicket=001_S0.1__toolchain-codegen.md.
- 2026-09-15 · S0.1 done — branch `svitali/harnessharness-s0.1` · PR https://github.com/SteveVitali/MetaHarness/pull/2 · base `svitali/harnessharness` (@4e82e27). Bound the E1 kernel/helper ecosystem (ADR-0050 (e)): hermetic pure-std Rust workspace (hh-wire/hh-embed-schema/hh-kernel/hh-codegen/hh-embed-client-generated), the single schema source (CC7), schema-export→codegen with the drift-checked generated client, the stdio JSON-RPC 2.0 `hello`/`ContractIdentity` boundary skeleton, CI (fmt/build/test/drift), and the AC-R-2.11.4-9 first measurement. **Verify:** 26 tests/11 suites green; fmt+build clean; drift check proven to fail (exit 1) on source change; boundary spike per-crossing median 51 µs / p95 92 µs, hash-equality 100%. **Deferrals:** opened DF-S0.1-1 (durable-frame/ephemeral cols → S0.3), DF-S0.1-2 (idp/1 content address → S1.2); closed none. **Deviations:** Stage-0 content address is interim for idp/1 (ADR-0219); boundary-spike harness retained vs R1–R6 (ADR-0220). chainTip → svitali/harnessharness-s0.1 · next → S0.2.
- 2026-09-15 · S0.3 done — branch `svitali/harnessharness-s0.3` · PR https://github.com/SteveVitali/MetaHarness/pull/4 · base `svitali/harnessharness-s0.2` (@c09a7ca). Ran the Stage-0 S1/S2 measurement spikes (offline/hermetic; operator spike budget released) as the acceptance check: throwaway `s1-kernel-spike` (append-only ledger with the ADR-0027 id model, durable-before-visible, hash chain, blob offload; `project()` + rebuild-equality; one sandboxed call through the `s1-helper` **binary** boundary; N=1/N=50 workload; gates G1–G4 as unit tests; M-S1-1…9) and extended `s2-boundary-spike` (M-S2-1…7, matched E1↔E1). Repeat-scored **N=5**; measurement sheet appended to the ADR-0050 amendment log (candidate ids only, CC4). **Revalidation (feeds GATE-G1): the ADR-0009/ADR-0050 ecosystem decision HOLDS — no trigger fires.** C5=4, C7=4/5 (within assumed ±1); C12 transport favorable and not contradicting assumed 2 (polyglot conjunct deferred to Stage 3); G1–G4 pass, M-S2-6 pass, M-S2-7=100%; C5/C7 `u`-marks cleared, C12 `u`-mark retained. **Verify:** workspace 84 tests/16 suites green (spikes excluded); 5 spike gate tests pass; fmt/clippy/drift clean; headline medians — startup 10.6 ms, footprint 3.74 MiB/session, throughput 3 791 e/s, CPU fan-out 1.58× core-norm, cancel 4.25 ms, tail p95 23 µs, per-run crossing overhead 73 µs (≈0.056%), codegen 26 ms, hash-equality 100%. **Deferrals:** opened DF-S0.3-1/-2/-3 (offline-blocked: MCP/ACP official-SDK + cross-candidate G1; C12 polyglot components → Stage 3; E2/E3 candidate scoring + E5b/E5c + R2 review); closed DF-S0.1-1 (durable-frame/ephemeral), DF-S0.2-1 (helper-binary measurement). **Deviations:** committed-but-throwaway spike trees (R4; ADR-0223 extends ADR-0220); CPU-vs-durable fan-out interpretation for C5 (ADR-0223/0224). No harness claim at Stage 0 (R5). chainTip → svitali/harnessharness-s0.3 · next → GATE-G1 (STOP gate: operator reads out the sheet + verdict before Stage 1).
- 2026-09-15 · S0.2 done — branch `svitali/harnessharness-s0.2` · PR https://github.com/SteveVitali/MetaHarness/pull/3 · base `svitali/harnessharness-s0.1` (@c8272d9). Hand-authored `react/minimal` **throwaway** baseline (`crates/hh-baseline`, deleted at the Stage-0 boundary; not a kernel dependency): validate-only HIR/1 with hand-stamped principal/definition/external; one root `BudgetNode` (hard ceilings, `check`, `budget_exhausted{dimension}`); a model-blind gateway over one wire dialect with a scripted **offline** stub + one static `ModelRoleTable`; one in-process shell/fs `tool_executor` under a `local_host` `EnvHandle` (isolation_class=none, honest); deny-by-default allow-list + masking + leak scan; single-file M1/M3/M7 trace with trace/cost views; the react/minimal control loop + driver-subset envelope; the headless CLI driver; stdio `hello` reused from S0.1 (CC7). **Verify:** 84 tests/16 suites green (S0.1:26 + hh-baseline:58); fmt+clippy clean; drift in sync; live CLI — one headless coding task completes in 3 turns, `run start --jsonl` minus `result` diffs empty vs `run events --from 0`, `MissingBudget`→exit 2, `budget_exhausted{model_calls}`→exit 5, validation→exit 3, 0 secret leaks. AC-R-2.1.6-1 · AC-R-2.3.2-1 · AC-R-2.5.5-14 (in-process half) · AC-R-2.8.3-1 (allow-list half) · AC-R-2.11.1-3 · AC-R-2.11.1-6 all met; no harness claim at Stage 0. **Deferrals:** opened DF-S0.2-1 (helper-binary boundary measurement → S0.3, operator-gated); closed none. **Deviations:** throwaway baseline crate (ADR-0221); Stage-0 exit-class mapping + interim numerals (ADR-0222). chainTip → svitali/harnessharness-s0.2 · next → S0.3.
