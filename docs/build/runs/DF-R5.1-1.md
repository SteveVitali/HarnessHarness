# Run ledger — DF-R5.1-1 (GATE-G17 contributor evidence: guard ran at every dispatch boundary; no unguarded accumulating loop)

- **Harness:** devin-desktop/swe-2-high/subagent
- **Observed runtime identity:** Devin Desktop subagent context (parent: devin-desktop coordinator); declared tier swe-2-high; exact model id is not exposed to this session — no independent self-report available, recorded honestly. Nesting depth 1: no subagent spawning this run.
- **Skills:** agent-skills @ e981ec5 — build-memory + orchestrate-build (ci-boundary)
- **Started:** 2026-11-05T14:45Z
- **Closed:** 2026-11-05
- **Spec:** `docs/tickets/DEFERRALS.md` row DF-R5.1-1 (verbatim scope: GATE-G17 contributor evidence for the ten R5.1-owned targets REQ-C-104/107/108 + REQ-H-001/002/003/005/006/007/011 — verify the retained guard ran before/after the round's actual dispatch boundaries and no unguarded accumulating loop ran). Records-in evidence collection — never a forward dispatch dependency.
- **Authorization:** the row's own discharge lane — GATE-G17 carried it forward OPEN with owner/trigger intact; records-in verification over the landed record, USD $0.00.
- **Base:** codex/harnessharness-evidence-df-cap5-1 tip (chain tip; LEDGER `chainTip` agrees)
- **Branch:** codex/harnessharness-evidence-df-r5-1 → PR targets codex/harnessharness-evidence-df-cap5-1
- **Config:** committed build-memory mode (root docs/build); offline records-in verification + offline fixture re-execution; no provider/backend/dependency; no outreach/spend/publication
- **ci:** pending at close — recorded, never counted green

## Gate status

No new gate. Deferral work, not a manifest row: the retained guard (`bash docs/build/tools/check-supplement-guard.sh`, AGENT_SKILLS_ROOT set) returns **PASS, route `HOLD — active round fully landed`** — the manifest chain is complete (nextTicket DONE), so no `DISPATCH — <id>` route exists. Same posture DF-CAP5.1-1 recorded. GATE-G17 PASSED 2026-11-05T00:13:07Z (acceptance on the honest record) carried DF-R5.1-1/DF-H9-1 forward OPEN — this run discharges the DF-R5.1-1 leg it names.

## Deferrals read

`bash scripts/deferrals-open.sh` at start — `DF-R5.1-1 | OPEN` ← **this run's target**; `DF-H9-1 | OPEN` (withheld participant/outreach set — not this work; minted 2026-11-04 CAP5.1).

## What this run verified (each honest verdict)

**EV-1 — guard ran before/after every actual dispatch boundary: VERIFIED.** All 67 work-row run ledgers (R5.1–R5.55 incl. legs R5.14b/R5.15b/R5.16b/R5.42a; CAP5.1–3; REC5.1–3; DOC5.1–2) record the guard's pre-boundary + post-boundary invocations with verdict PASS and route `DISPATCH — <that row>`; every round-5 PHASE LOG close entry repeats the guard verdict. R5.1 is the GATE-G8 bootstrap (self-route); R5.2's coordinator stub legs carry the transcript-contract `guard.pre → spawn → report → guard.post` ordering — validated VALID on the actual transcripts (`transcripts/actual-{happy,resume}.json`). Post-close routes naming the next unlanded row (R5.8 post → `DISPATCH — R5.9`; R5.39 post → `DISPATCH — R5.40`) are expected routing semantics, not stray dispatches.

**EV-2 — counts/versions/digests recorded: VERIFIED.** `hh-supplement-guard/1` v1.0.0 throughout. Counts: candidates=180 (adopted 167 / merged-with 12 / superseded-by 1), coverage_md_rows=180, active_round_rows 79→82 (mid-round appends), chain_rows=287 at close, stock_violations=0, warnings=8 class-exact baseline-matched. Frozen supplement sha256 `8f540171…` invariant across every boundary. Example recorded digests — R5.11: coverage.md `1dd62a49…`, LEDGER `9fad816b…`, BUILD_INDEX `c5421f74…`; today's live re-run: BUILD_INDEX `8823b9b7…`, LEDGER `38b1557d…`, coverage `{md eee5d89f…, json 769a17fd…}`, baseline `835e00c2…`, G1 readout `e286311b…`, manifest `ff81f72d…`.

**EV-3 — old GATE-G1 dispatched zero times: VERIFIED.** (a) `legacy-completion` byte-preserves `readouts/GATE-G1.md` (sha256 `e286311bbaa98966…` — a recorded guard input at every boundary; requires the second 2026-09-15 PASSED/ADR-0226 text). (b) `old-gate-routing` refuses an out-of-round nextTicket — proven by both fixture cases (`old-gate-g1` rejects `[old-gate-routing]` in the guard battery; the driver refuses the spawn spawn-free). (c) grep over runs/LEDGER/BUILD_INDEX: every recorded nextTicket/route names a round-5 row — zero GATE-G1 dispatch routes exist.

**EV-4 — refused pending/failed states on live routes: VERIFIED.** HOLD human HUMAN-H8 (R5.42 post, RETURN PASS); HOLD gate GATE-G11 (R5.42a post); `FAIL — route HOLD` GATE-G13/G14 + `[nextticket]` (R5.47/R5.48 pre-fleshing — owed decision rows appended verbatim, then PASS); PAUSE at HUMAN-H9 (LEDGER 2026-11-04T09:59:37Z); GATE-G15 BLOCKED on DF-H9-1 (2026-11-04T13:01:19Z); `REFUSED — skeleton` R5.44 and R5.46 pre-fleshing; terminal `HOLD — active round fully landed` (post-round FAIL→repair on guard-done; DF-CAP5.1-1 PASS+HOLD pre+post). CAP5.2's pre-dispatch FAIL was a real catch (correction rows indexing as gate decisions — F-1/F-2, repaired append-only; `correction-mask` fixture added → battery 14). The brief's paraphrased examples reconcile to these exact recorded routes: 'GATE-G15 HOLD' ↔ the H9-pause/G15-blocked hold chain; 'skeleton R5.50 REFUSED' ↔ the recorded skeleton refusals at R5.44/R5.46 (the refusal class holds — R5.50's own boundary record begins post-fleshing at `PASS DISPATCH — R5.50`).

**EV-5 — fixture batteries: VERIFIED.** `run-fixtures.sh` (supplement-guard): **14 rejected / 0 failures** re-run today. `run-fixtures.sh` (dispatch-driver): **22 passed / 0 failures** re-run today — after the era-binding repair in F-1 below.

**EV-6 — no unguarded accumulating loop: VERIFIED.** (a) Guard read-only: today's live run left `git status` byte-identical (same recorded at R5.1). (b) Single advancing pointer: `nextticket` requires nextTicket == first unlanded row — every recorded PASS proves pointer/exercise agreement at that boundary. (c) Fresh worker per dispatch (subagent harness headers); driver emits exactly one envelope per leg and validate enforces the contracted spawn count, prompt-sha identity, no-progress detection, and `ledger_reread` before a resumed stub acts. (d) Identical pre/post input digests + `tree_status: clean` in the transcript contract — state untouched between guard runs. (e) Post-round posture retained — DF-CAP5.1-1's deferral work ran the guard PASS+HOLD pre+post under explicit operator authorization; no unguarded path exists even after manifest completion.

## Finding F-1 — fixture era-binding staleness (found + repaired records-in, this run)

The dispatch-driver battery's three plan-mode cases (`happy`/`pending-gate`/`skeleton`) were era-bound: the runner copied the live LEDGER into the scratch tree, and once the live `nextTicket`/`chainTip` advanced past the R5.2-era scenario the cases pin (`170_R5.2`, GATE-G9, the r5.1 tip branch), they could no longer reach their named diagnostics — all three still REFUSED safely (never an emit). Repaired in `run-fixtures.sh`: an era-synthesis step rewinds the scratch ledger to the recorded R5.2 boundary (nextTicket R5.2 / lastCompleted R5.1 / chainTip `codex/harnessharness-evidence-r5.1` / GATE DECISIONS stripped to through-G8 / BUILD_INDEX trimmed to seq ≤169) so the scenario is deterministic at any chain position. Re-run: **22 passed / 0 failures**. Same disposition class as CAP5.2's F-1/F-2 and the guard-done terminal repair — tooling repair recorded, never hidden.

## Evidence log

- `docs/build/reports/2026-11-05-df-r5.1-evidence.json` — `hh-guard-boundary-evidence/1` v1.0.0: the six checks above with verbatim route records, counts, digests, and the F-1 finding.

## Verification

- `AGENT_SKILLS_ROOT=$HOME/agent-skills bash docs/build/tools/check-supplement-guard.sh` — **PASS**, route `HOLD — active round fully landed` (v1.0.0; 82 rows / 180 cand / 0 viol / 8 warn; digests recorded in the artifact).
- `bash docs/build/fixtures/supplement-guard/run-fixtures.sh` — **14 rejected / 0 failures**.
- `bash docs/build/fixtures/dispatch-driver/run-fixtures.sh` — **22 passed / 0 failures** (post F-1 repair; was 19/3 on the stale era state).
- `git status` before/after guard run — identical (read-only verified live).
- `grep -rn "DISPATCH — GATE-G1\|nextTicket → GATE-G1" docs/build/` — zero matches.
- `bash scripts/check-bm-tools.sh` — OK (1 open deferral DF-H9-1 resolves to exactly one row; DF-R5.1-1 discharged from the open index).
- `check-build-memory.sh .` — 0 violations / 8 warnings (the reviewed baseline classes, unchanged). `--staged` history mode reports the known inherited BM-DEFER-01 quirk on this row edit (`line 138 removed or rewritten`): the checker's `^\|\s*D-` row pattern never matches `DF-*` ids, so every in-place DONE growth lands in the raw append-only branch — verified identical on the immediately-prior close (`check-history.sh --range ef35f501^..ef35f501` flags `DEFERRALS.md:141` the same way). Recorded as a tooling limitation, not a record defect; the row grew per the established convention (every old cell preserved, dated `**DONE**` note appended).
- `check-living-docs.sh` — OK (456 files).
- `python3 -m json.tool` on the artifact — valid JSON.
- Load-bearing note (honest): this run's first PHASE LOG append ran 2,153 bytes — over the ≤2 KiB entry budget; the stock checker's `ledger-budget` class fired as a new `unexplained-diagnostic` and the guard FAILED until the entry was trimmed (1,545 B). The guard caught this run's own record defect — the same class the deferral verifies.

## Deferral-row disposition

**DF-R5.1-1 → DONE** — every named leg verified against the landed record: each dispatch boundary's guard invocation is recorded (67 work-row boundaries, pre+post, PASS DISPATCH, v1.0.0, counts/digests emitted); old GATE-G1 dispatched zero times (legacy-completion byte-preserved + old-gate-routing fixture + zero recorded routes); refused pending/failed states are on live routes (HOLD/REFUSED/FAIL inventory); no unguarded accumulating loop ran (read-only guard + single advancing pointer + fresh workers + identical pre/post digests + one-envelope-per-leg). The ten targets' contributor cells cite `contributor_evidence→DF-R5.1-1`, which this record discharges — evidence class records-in verification + offline fixture re-execution; no live-service, dispatch-authority, or gate re-decision claim. DF-H9-1 stays OPEN untouched.
