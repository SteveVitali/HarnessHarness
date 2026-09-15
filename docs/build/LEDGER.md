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
nextTicket:      S0.3
lastCompleted:   S0.2
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
chainTip:        svitali/harnessharness-s0.2
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

## RETURN PASS

| ticket | gates | what the operator must do | re-run line |
|---|---|---|---|

## PHASE LOG

- 2026-09-11 · Ledger created by decompose-spec from `spec/CANONICAL_SPEC.md` (v1.0-rc2, PASS 7/7); 100 implement-spec tickets + 3 milestone gates + 2 human prerequisites + GATE-ACCEPT (106 chain rows total); dispatch=subagent; plan is revisable at run time.
- 2026-09-15 · SETUP done — worktree `MetaHarness-harnessharness` created off 85a3960 on branch `svitali/harnessharness` (operator-chosen prefix, not `whoami`); params resolved (dispatch=subagent, autonomy=manual, mergePolicy=OPERATOR); seed already committed at base (85a3960) so no separate first commit; baseline trivially green (no build system exists pre-S0.1); benchmarkSet left PENDING_CREATE (Stage-3 reference suite, not creatable at Stage 0); check-build-memory exit 0. nextTicket=001_S0.1__toolchain-codegen.md.
- 2026-09-15 · S0.1 done — branch `svitali/harnessharness-s0.1` · PR https://github.com/SteveVitali/MetaHarness/pull/2 · base `svitali/harnessharness` (@4e82e27). Bound the E1 kernel/helper ecosystem (ADR-0050 (e)): hermetic pure-std Rust workspace (hh-wire/hh-embed-schema/hh-kernel/hh-codegen/hh-embed-client-generated), the single schema source (CC7), schema-export→codegen with the drift-checked generated client, the stdio JSON-RPC 2.0 `hello`/`ContractIdentity` boundary skeleton, CI (fmt/build/test/drift), and the AC-R-2.11.4-9 first measurement. **Verify:** 26 tests/11 suites green; fmt+build clean; drift check proven to fail (exit 1) on source change; boundary spike per-crossing median 51 µs / p95 92 µs, hash-equality 100%. **Deferrals:** opened DF-S0.1-1 (durable-frame/ephemeral cols → S0.3), DF-S0.1-2 (idp/1 content address → S1.2); closed none. **Deviations:** Stage-0 content address is interim for idp/1 (ADR-0219); boundary-spike harness retained vs R1–R6 (ADR-0220). chainTip → svitali/harnessharness-s0.1 · next → S0.2.
- 2026-09-15 · S0.2 done — branch `svitali/harnessharness-s0.2` · PR https://github.com/SteveVitali/MetaHarness/pull/3 · base `svitali/harnessharness-s0.1` (@c8272d9). Hand-authored `react/minimal` **throwaway** baseline (`crates/hh-baseline`, deleted at the Stage-0 boundary; not a kernel dependency): validate-only HIR/1 with hand-stamped principal/definition/external; one root `BudgetNode` (hard ceilings, `check`, `budget_exhausted{dimension}`); a model-blind gateway over one wire dialect with a scripted **offline** stub + one static `ModelRoleTable`; one in-process shell/fs `tool_executor` under a `local_host` `EnvHandle` (isolation_class=none, honest); deny-by-default allow-list + masking + leak scan; single-file M1/M3/M7 trace with trace/cost views; the react/minimal control loop + driver-subset envelope; the headless CLI driver; stdio `hello` reused from S0.1 (CC7). **Verify:** 84 tests/16 suites green (S0.1:26 + hh-baseline:58); fmt+clippy clean; drift in sync; live CLI — one headless coding task completes in 3 turns, `run start --jsonl` minus `result` diffs empty vs `run events --from 0`, `MissingBudget`→exit 2, `budget_exhausted{model_calls}`→exit 5, validation→exit 3, 0 secret leaks. AC-R-2.1.6-1 · AC-R-2.3.2-1 · AC-R-2.5.5-14 (in-process half) · AC-R-2.8.3-1 (allow-list half) · AC-R-2.11.1-3 · AC-R-2.11.1-6 all met; no harness claim at Stage 0. **Deferrals:** opened DF-S0.2-1 (helper-binary boundary measurement → S0.3, operator-gated); closed none. **Deviations:** throwaway baseline crate (ADR-0221); Stage-0 exit-class mapping + interim numerals (ADR-0222). chainTip → svitali/harnessharness-s0.2 · next → S0.3.
