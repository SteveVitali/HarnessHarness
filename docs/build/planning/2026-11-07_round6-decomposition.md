# Round 6 — experiments first pass decomposition (partition plan)

**Status:** decomposition draft, revised after its independent Phase-4 split review (SR-1…SR-27) under the coordinator's binding rulings, then **re-partitioned to a coarser granularity by operator decision** (planning conversation 2026-11-07: "~45 tickets (Recommended)" and "3-run tail (Recommended)"; see [Re-partition (operator Q 2026-11-07)](#re-partition-operator-q-2026-11-07)). Written 2026-11-07T03:42:36Z, revised 2026-11-07T04:19:14Z, re-partitioned 2026-11-07T04:49:42Z. It carries Phases 0–2, the [Phase-4 review record](#phase-4-review-record-2026-11-07) and the Phase-5 coverage register. No ticket files, manifest rows or ledger edits are made here. Contract writers (Phase 3) follow. Nothing in this plan authorizes spend, provisioning, publication, external writes or merges, and it answers no gate.

**Invocation (frozen hand-off).** `decompose-spec spec=docs/experiments-spec.md build_name=experiments mode=extend tickets_dir=docs/tickets dispatch_target=subagent granularity=balanced tail=full`, per [the hand-off](../../experiments-decomposition-prompt.md) (sha256 `fd7fe273d559ec1e…`). **Operator override (2026-11-07):** rows are sized to a focused estimate ≤ ~95k + a 10k verification reserve (a 90–110k working set, under half of SWE-2's 256k window), and the round's tail is the decompose-spec minimal tail — capstone, capstone, gate, docs — which the Round-6 manifest banner declares with `tail=minimal` (the coordinator's retained-guard patch, `TAIL_KINDS_MINIMAL`).

## Boundary and source identity

- **Spec:** [experiments supplement](../../experiments-spec.md), canonical v0.3, sha256 `0bcfb3572dcc0bc921014f265ee34876f2f74bad7b6cd685abe214ebcf78362e`. This equals the hand-off's digest. `req_id_pattern: REQ-X[A-H]-[0-9]+`; Appendix A lists 456 candidates (424 source + 32 synthesis-allocated), 370 adopted.
- **Base:** worktree branch `claude/experiments-spec` at `6bd6d99d`; `main` `185c16c7` is an ancestor. The manifest tail is still row 251 (`251_R5.59__efficacy-closure.md`), so Round 6 opens at **sequence 252** and, re-partitioned, ends at **324**. Re-check at integration, because `main` moves.
- **State reads (mode=extend).** LEDGER CURRENT STATE: `round: 5`, `nextTicket: DONE`, `lastCompleted: R5.59`, `chainTip: codex/harnessharness-bl-reconcile`, `harness: devin-desktop/swe-2-high/subagent`. `bash scripts/deferrals-open.sh` printed no rows, which is a valid zero-open-rows result. Hand-off §4 gives the ledger values for the extension (`round: 6`, `nextTicket: GATE-G20`, `chainTip: main`, with `buildBranchBase` and `pinnedBaseSha` unchanged). The coordinator writes them, not this plan.
- **Guard mechanics honoured** (`docs/build/tools/supplement_guard.py` at `185c16c7`, sha256 `18ef79a8…`, plus the coordinator's minimal-tail patch). Chain-id grammar; three-digit sequence filenames; kinds from the closed set; backward `Depends on` (written `nothing` for the bootstrap gate, never `—`); the contiguous tail block at the end of the round — here the declared minimal tail, whose gate filename contains `accept`; landed = BUILD_INDEX id/seq (tickets, skeletons), a GATE DECISIONS row containing `PASSED` and neither `FAILED` nor `NOT PASSABLE` (gates), or a readout `Status:` other than `PENDING` (HUMAN). A pending gate or HUMAN row at the head of the chain is a HOLD; a recorded non-PASSED gate is `failed-gate` (FAIL); an unlanded skeleton at the head is `skeleton-dispatch` (REFUSED).

## Phase 0 — extracts

### Requirements by block (370 adopted; one primary owner each, see the [coverage register](2026-11-07_round6-coverage.md))

| block | adopted ids | rows | content |
|---|---|---|---|
| GUARD | 8 | R6.1 | guard extension: register, routing and baseline (XH-001…003) and gate/HUMAN/skeleton/window-pattern rules (XH-004, 008, 024…026) |
| P1 | 31 | R6.2, R6.3 | phase-1 provenance + hardening run; corrections + recompute + AR-01/02/03 + record + cards + checker (XA-006…125 less XA-126/127; XB-222; XH-019, 028, 032) |
| SITE | 48 | R6.4–R6.6, R6.20, R6.21 (F2-h rides in R6.49) | identity, generator + phase-1 pages, claims checker + Pages workflow, comparative/noise/grid components (XF-*, plus XC-014, XC-120, XD-003, 092, XH-018, 020, 022) |
| PUB | 2 | R6.31, R6.50 | publication records after each publication gate (XF-136, XH-105) |
| INFRA | 98 | R6.7–R6.17 | gateway price + lane; runner core + topology; cell spec + deadline + verdict; proxy core + canonicalizer; release + budget; replay + archive index; compute recipe; cost + reconcile; egress; schedule + supervision; Harbor-O + re-proof machinery |
| METHODS | 21 | R6.18, R6.19, R6.27 | task selection + comparative protocol and quote; estimators; battery + method dossier + integrity scan |
| HOSTING | 36 | R6.22–R6.26 | admission + two-stage build; carried binary sets + archive upload kit; first-pass harness renderers; capture + smoke battery; PS0 + live legs + runner check |
| L1 | 4 | R6.28 (K1 rides in R6.10, K7 in R6.46) | observation replay + corpus + census and the L1 runner |
| HIR | 34 | R6.34–R6.39, R6.41–R6.43 | E2-T0…T10 gap closure (T0+T1, T3+T2a, T2b+T4a, T4b, T5+T6a, T6b+T8a, T7+T8b), FP-RECORD-PARTITION + the mini-swe-agent document, sweep glue + D3-FP |
| FP | 27 | R6.29, R6.30, R6.33, R6.46 | first-pass runner rules (continuation + holds; seal + selection commit), pre-registration, FP-E quote (with K7's L2 wiring) |
| W1 / W2 / W3 windows | 2 / 4 / 5 | R6.32, GATE-G26, R6.40, R6.44 | R-layer evidence that is a window's own record (re-proof, W2 stages, L1 gate run) plus FP-S readout fields |
| RESULTS | 11 | R6.48, HUMAN-H16, R6.49 | first-pass results packets, audit, L3, flagged-pass review, F2-h pages + freeze |
| TAIL | 5 + 34 | CAP6.2 | $100 closure, per-kind spend, live reads, decision-register history, and the later-pass routing records — Round 7: phase-4 explore → confirm, tranches, full re-proof, phase 1b (22); Round 8: Terminus-2 (12) — written and re-read live before GATE-ACCEPT |

Layers among the 370: E 257, D 80, R 32, P 1. The ids per row are in the row table below and, literally, in the coverage register.

### Acceptance and the cross-cutting invariants (every Round-6 row re-checks these)

1. **Spend ceiling** ⚑: the first pass costs ≤ $100 in all (Q-X47, Q-X50). Model ≤ $70, compute ≤ $20, and a $10 reserve that is never authorized (Q-X54). Purchased credit counts in full (Q-X63). No ticket authorizes spend; only the recorded gates (PROV-W1, FP-S, PROV-W3, PROV-Wc, FP-E) do, verbatim (REQ-XH-004, REQ-XH-110).
2. **Skeleton rule** ⚑: every W1–W4 and Wc run body is `Kind: skeleton`, with no Run line until its gate passes. A scoped `decompose-spec` insert then fleshes it in place (Q-X58, REQ-XH-001, REQ-XH-024; mechanics in [Decomposition decisions](#skeleton-fleshing-mechanics-sr-27)). A pending gate is RETURN PASS and is never pre-answered.
3. **Guard first** ⚑: GATE-G20, then GUARD-X (R6.1; REQ-XH-002, AR-31). No product ticket dispatches before it lands.
4. **Firewall**: no gate or continuation rule reads an outcome field. First-pass agent-slot outcomes stay sealed until FP-RESULTS (REQ-XH-112, REQ-XH-121). Every cost-triggered drop is labelled "cost-triggered; cost correlates with failure length".
5. **Repository invariants** (root `AGENTS.md`): K-2 (no kernel semantics in `hh`), CC1 (one implementation per codec, store, class table and renderer), CC7 (schema → regenerate; never hand-edit generated files), CC10 (one home per kind), durable-before-visible, honest typed refusal (`Unsupported`/`Refused`/`n/a{reason}`/`withheld{…}`), no new dependency without an ADR, `provided: yes/no` only for secrets, append-only history, dispositions never relabelled, `mergePolicy: OPERATOR`, and build-memory writes by shell/Python with `git diff` verification.
6. **Evidence labels**: `live-executed` only through a registered `scripts/live-verify.sh` leg (REQ-XH-027, ADR-0363); the legs are built by R6.17 (`reproof`), R6.26 (`ps0`, `pp0`, `corpus`, `w4-main`) and R6.29 (`w4-lane-probe`). Fixture-verified and recorded-execution labels are kept distinct. No favourable effect is an acceptance criterion (§0.2).
7. **Namespace**: the Appendix D spellings (W1–W4/Wc, AG1–AG9, SM-*, PS0/PP0, V1–V9, ST0–ST7). Round-5 S1–S4 are never reused for program stages (REQ-XH-022, S-12).
8. **Requirement ids**: kept verbatim and append-only. Each adopted id has exactly one primary owner, and aliases keep their literal predicate bytes at the target (REQ-XH-003). The round-5 `REQ-[A-H]-[0-9]+` register is untouched.
9. **Universal phase-gate AC** (BM-TICKET-02) on every implement-spec row, plus the Round-5 operating clauses (dates from the clock, PR checks read and recorded, AC layers stated, protected records only appended, gate words verbatim, harness in the run-ledger header).
10. **Merged-row stages** (re-partition): a row that merges former rows keeps each former row as an **ordered stage** with its own AC block, regression tests and commit, in the merge map's order. A measured overflow (XO-03, OL-16) therefore stops at a stage boundary and hands a clean remainder to a scoped insert; a stage is never left half-landed.

### Out of scope for Round 6 (every later-round item named by spec §10.6, §1.2 and §10.1.6)

- Later-pass phase 2: tranches TR1 → TR2 → TR3, tranche quotes and gates, the band census, the full shared pilot and pilot set P, R-live, and up to 8 harnesses (Hermes, OpenHands, Kimi Code and OpenCode adapters; Codex and a Responses surface, C-32).
- Later-pass phase 3: the full L2 at N ≈ 150–300 and G-L2. The **Terminus-2 stage** goes to Round 8: `lab_host` hosting (AR-19, REQ-XC-314, REQ-XC-316), E2-T12…T16, K10–K14 and AR-30.
- Later-pass phase 4: exploration X0/X1, frozen selection, held-out confirmation, G-P4-X/G-P4-C, D3-b/d…i, effort and compaction as factors (AR-26, AR-27), and `edit_file`.
- The full 87-task re-proof (it precedes any later-pass draw; REQ-XH-033). Phase 1b is out too (Q-X24 default: unscheduled; REQ-XA-126).
- The optional PS0 early gate (Q-X23 default: PS0 runs in W2 on the VM), vendored fonts (AR-29) and a custom domain (Q-X43 default: project URL).
- Discharging BL-50's P-layer legs. REQ-XA-114 corrects only the phrasing of its dated note.
- Any guard-enforced first-pass exception to the skeleton rule (Q-X58 option (b), an operator decision).

These 34 adopted ids keep one owner (CAP6.2, the closure capstone), which writes their routing record and re-reads it live before GATE-ACCEPT, with `round_scope: routed(round-7|round-8)`. No implementing row for them exists in Round 6, as REQ-XH-001 requires.

## Phase 1 — dependency DAG and coupling seams

### Window and gate sequence (FP-10 staged spending, as linearized; linearization A)

```text
GATE-G20 bootstrap ─► R6.1 GUARD-X ─► P1 (R6.2 P1-A+B, R6.3 P1-C+D+E) ─► R6.4 F2-a ─► GATE-G21 mock ─► R6.5 F2-b+e ─► R6.6 F2-f+g
─► P2-infra (R6.7…R6.27; GATE-G23 after R6.27, the D1-b (b) + D1-c row) ─► R6.28 K2+K3+K4 ─► FP rules (R6.29; R6.30 incl. SELECT commit + W1 quote)
─► operator batch: HUMAN-H12 Pages ─► GATE-G22 pub-P1 ─► R6.31 PUB-P1 ─► HUMAN-H13 billing alert ─► GATE-G25 PROV-W1
─► [W1 skeleton R6.32] ─► R6.33 FP-PREREG (+ FP-S readiness record: the Q-X62 clock can start from here)
─► E2: R6.34 T0+T1 ─► R6.35 T3+T2a ─► R6.36 T2b+T4a ─► R6.37 T4b ─► GATE-G24 AR-24 ─► R6.38 T5+T6a ─► R6.39 T6b+T8a (the C-05 rung record, landed in every outcome)
─► operator batch: HUMAN-H14 account setup (Ollama settings, HF dataset, optional dedicated key) ─► GATE-G26 FP-S
─► [W2 skeleton R6.40: ARCHIVE-UPLOAD-W1 first (operator-accepted deviation) … teardown, ARCHIVE-UPLOAD-W2 + SETTLE-W2]
─► R6.41 T7+T8b ─► R6.42 FP-RECORD-PARTITION + T9 ─► R6.43 T10 + D3-FP ─► GATE-G27 PROV-W3 {run | not_run} ─► [W3 skeleton R6.44, incl. ARCHIVE-UPLOAD-W3]
─► GATE-G28 PROV-Wc {triggered | not triggered} ─► [Wc skeleton R6.45, incl. ARCHIVE-UPLOAD-Wc — or unused + n/a{wc_unused}]
─► GATE-G29 G-L1 {full | fallback} ─► R6.46 FP-E quote (per disposition) + K7 ─► HUMAN-H15 credit purchase (only if needed) ─► GATE-G30 FP-E
─► [W4 skeleton R6.47: lane probe, rounds 1–2, automatic checkpoint, rounds 3–5, project deletion, ARCHIVE-UPLOAD-W4 + SETTLE-W4]
─► R6.48 FP-RESULTS (P2-FP, P4-FP, P3-L2, audit, L3) ─► HUMAN-H16 flagged review ─► R6.49 F2-h + freeze
─► GATE-G31 pub-FP ─► HUMAN-H17 dataset flip ─► R6.50 PUB-FP
─► HUMAN-H18 consolidated billing reconciliation (W1, W2, W3, Wc, W4) ─► minimal tail (CAP6.1, CAP6.2 closure, GATE-G32 accept, DOC6.1)
```

The W4 checkpoint is automatic and pre-registered, so it gets no row (§10.3). The fallback (REQ-XH-123) is not a row: it is GATE-G29's pre-registered `fallback` disposition, and every row after G29 is written so that it lands honestly under either disposition ([Disposition gates and fallback routing](#disposition-gates-and-fallback-routing-sr-1)).

### P2-infra DAG (spec §10.2, with the merges applied; edges = "must land before")

```text
PRICE+LANE(R6.7) ∥ RUNNER-CORE a+b(R6.8) ──► PROXY-CORE-a + K1(R6.10) ──► PROXY-CORE-b + BUDGET(R6.11) ──► REPLAY + ARCHIVE-a(R6.12)
R6.8 ──► CELL-SPEC + DEADLINE + VERDICT(R6.9) ──► EGRESS a+b(R6.15, also after R6.10) ──► HARBOR-O + REPROOF-KIT(R6.17)
{R6.8, R6.11, R6.15} ──► RUNNER-SCHEDULE + SUPERVISION(R6.16) ──► ADMIT a+b(R6.22, also after R6.12)
RECIPE a+b(R6.13) ──► COST-RECORD + RECONCILE(R6.14, also after R6.11, R6.12) ──► SELECT + D1-a(R6.18) ──► D1-b (a)(R6.19)
{R6.19, R6.22, R6.12} ──► D1-b (b) + D1-c(R6.27) ──► GATE-G23 method admission
{R6.5, R6.19} ──► F2-c (a)(R6.20) ──► F2-c (b) + F2-d(R6.21, also after R6.12)
{R6.22, R6.12, R6.13} ──► BINARY-SETS + ARCHIVE-b(R6.23);   R6.22 ──► ADAPT chat + messages(R6.24) ──► CAPTURE + SMOKE(R6.25) ──► PS0+LEGS + RUNNER-CHECK(R6.26, also after R6.17)
{R6.10, R6.12, R6.24, R6.15} ──► K2+K3+K4(R6.28) ──► K7 (inside R6.46, after G-L1)
{R6.16, R6.11} ──► FP-CONT + FP-HOLDS(R6.29) ──► FP-SEAL + FP-SELECT(R6.30, also after R6.5, R6.18, GATE-G23, R6.17, R6.14) ──► … W1 ──► FP-PREREG(R6.33)
R6.34 (T0+T1) ──► R6.35 (T3+T2a) ──► R6.36 (T2b+T4a) ──► R6.37 (T4b) ──► GATE-G24 ──► R6.38 (T5+T6a, also after R6.9) ──► R6.39 (T6b+T8a rung record, also after R6.16, R6.12)
     ──► GATE-G26 FP-S (edge waived by a recorded Q-X62 trigger)
R6.39 ──► R6.41 (T7+T8b, also after R6.28, R6.16);   {W2 harvest (R6.40), R6.30, R6.41} ──► R6.42 (partition + T9) ──► R6.43 (T10 + D3-FP, also after R6.33)
```

The former T1 ∥ T3 fork is serialized (T3 now rides with T2a after T0+T1); in a linear chain this costs no wall time.

### Coupling seams (where the cuts fall, and where they must not)

- **Cut on:** crate and binary boundaries: `hharness-runner`, `hh-model-proxy`, `hh-egress-mediator` / `hh-cell-relay`, `scripts/gce-backend.sh`, `scripts/site/`, `hh-eval` modules (`task-selection/1`, `comparative_protocol/1`), and the `hharness-wire` module `hh-l1-canon`. Also on schema owners (one schema, one row) and on the window boundaries, which are strict. The re-partition merges only rows that read the same crate, schema or script, or that share a decision (merge map); no merge crosses a gate that must sit between its parts (GATE-G21 after F2-a, GATE-G23 before the selection commit, GATE-G24 between E2-T4 and E2-T5, every window and publication gate) or a window boundary.
- **Named seam rows (SR-4…SR-6), now merged with their nearest shared-surface neighbour, never split.** Runner ↔ proxy/mediator supervision (slot-token lifecycle, socket grant, carry-in) rides with scheduling in R6.16 (one `hharness-runner` crate); the two carried HH binary sets and the layer export/load/verify path ride with the archive upload kit in R6.23 (both controller-side artifact paths over `hh-receipt-index/1`'s `artifacts` section, owned by R6.12); the PS0 driver and the W2/W4 `live-verify.sh` legs ride with the runner check in R6.26. R6.29 builds the W4 lane-probe driver and its leg.
- **Do not cut:** the single compiled-surface renderer (R6.35 owns it, as the E2-T2 (a) stage; R6.36's E2-T2 (b) stage extends it and never re-implements it, CC1), the receipt deposit ordering (it stays with PROXY-CORE-a in R6.10, and the release modes in R6.11 build on it), the slot record schema with its closed `withheld{…}` vocabulary (RUNNER-CORE (a), R6.8), and the projection formulas p̂_h/t̂_h (FP-CONT, R6.29, which FP-E's quote and the W4 checkpoint consume).
- **Out-of-chain:** none. Every row is in this repository. The Hugging Face dataset, GCP project and Ollama account are external accounts reached through HUMAN rows and gated execution acts, never through a stacked branch.

## Phase 2 — the Round-6 row table

Chain order, starting at 252 and ending at 324. Each size budget is the Round-5 contract header form, `<n>k estimated focused context + 10k verification reserve; ≤<lines> changed lines (split trigger, not target)`. Under the operator's re-partition rule *n* ≤ 95: a merged row's *n* is the sum of its former rows' calibrated estimates (the Round-5-measured ×3 basis) less the reading they share — 10k of fixed orientation per absorbed row (XO-01: root `AGENTS.md`, the invariants, the contract, deferrals, dependency run records) plus the named shared code or spec surface; the arithmetic is in the [merge map](#merge-map). Line limits are the former limits summed. One row, the closure capstone CAP6.2, exceeds the bound by operator decision and carries `operator-ruled` in its size cell ([tail record](#tail-minimal-operator-q-2026-11-07--read-decisions)). The gate cell is `STOP` on every gate; the coordinator later writes the outcome tokens there (never this plan). HUMAN rows carry `—`, because they are skipped by kind. Chain ids are renumbered compactly in chain order (no ticket files exist); gate ids are unchanged, because all 13 gates are kept; HUMAN ids are renumbered H12–H18 in chain order. The pre-re-partition ids appear only in the merge map and the Phase-4 review record.

| seq | file | chain id | kind | block | scope | spec §§ | primary REQ IDs owned | depends on | size budget | live stage | gate | alias |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 252 | `252_GATE-G20__bootstrap.md` | GATE-G20 | gate | B0 | Review the re-partitioned plan and coverage register; operator decides Q-X46 (A recommended, B′ offered), the W1 upload deviation (now hosted as W2’s first step) and the consolidated reconcile placement, and accepts verbatim the re-partition departures from §10.6 (3-run tail instead of `tail=full`; RECIPE a/b merged; row count below the first-cut band); authorizes only R6.1, routed by the retained guard in `--no-checker` mode with the stock validator run separately at 0 violations (GATE-G8 precedent) | §10.3, §10.6 | — | nothing | — (marker) | n/a | STOP | R6 plan review + bootstrap (B0) |
| 253 | `253_R6.1__guard-x.md` | R6.1 | ticket | GUARD | GUARD-X (a): Extend `hh-supplement-guard/1`: second frozen source, REQ-X register legs over this JSON (Appendix A id source; § anchors + Appendix-A hook resolution), Round-6 routing, the 46 legacy `reqcov` warnings classified into the reviewed baseline (OPEN FINDING), coverage checker absorbed (AR-31) ‖ GUARD-X (b): Guard rules: verbatim gate markers and disposition-gate routing (GATE-G27…G30 tokens), HUMAN readout tokens, skeleton + fleshing-in-place, window pattern (REQ-XH-008), paid-row gate refusal | §10.6, §0.2, §10.3, §3.9 | 8: XH-001, XH-002, XH-003, XH-004, XH-008, XH-024, XH-025, XH-026 | GATE-G20 | 90k + 10k; ≤1700 lines | offline-only | — | GUARD-X (a) + GUARD-X (b) |
| 254 | `254_R6.2__p1-provenance-hardening.md` | R6.2 | ticket | P1 | P1-A: Commit the scratch archive (operator-disk input, manifest sha256 pinned; typed refusal if absent), raw records, content digests and `p1_records.py` with its battery ‖ P1-B: Records-in cross-check (16/14/2; 26/30) and estimator ladders as committed packets | §2.2, §2.3 | 15: XA-101, XA-102, XA-103, XA-104, XA-105, XA-106, XA-006, XA-107, XA-108, XA-109, XA-110, XA-111, XA-112, XA-113, XH-032 | R6.1 | 80k + 10k; ≤1500 lines | offline-only | — | P1-A + P1-B |
| 255 | `255_R6.3__p1-corrections-record.md` | R6.3 | ticket | P1 | P1-C + P1-D: Dated corrections, S4 deviation record, AR-01 (+AR-02/AR-03 records; Q-X25 default) and the dated study recompute folding DF-CAP5.1-1; R5.59 bytes unchanged ‖ P1-E: Phase-1 record, bounds block, `[replay-record]` card, README spine, `record` leg (AR-32) | §2.4, §10.5, §2.5–§2.7, §9.6 | 16: XA-114, XA-115, XA-116, XA-117, XB-222, XH-028, XA-008, XA-118, XA-119, XA-120, XA-121, XA-122, XA-123, XA-124, XA-125, XH-019 | R6.2 | 95k + 10k; ≤1700 lines | offline-only | — | P1-C + P1-D + P1-E |
| 256 | `256_R6.4__site-shell.md` | R6.4 | ticket | SITE | Instrument design system, tokens test, glyph rasterizer, shell and all-absent site | §9.1, §9.2 | 8: XF-101, XF-102, XF-103, XF-104, XF-105, XF-106, XF-107, XF-114 | R6.1 | 60k + 10k; ≤1000 lines | offline-only | — | F2-a |
| 257 | `257_GATE-G21__mock-review.md` | GATE-G21 | gate | SITE | Operator views the rendered mock (both themes, four viewports); approve "Instrument" or redirect (Q-X43) | §9.1, §10.3 | — | R6.4 | — (marker) | n/a | STOP | site identity mock review |
| 258 | `258_R6.5__site-generator-phase1.md` | R6.5 | ticket | SITE | F2-b: Stdlib generator core, `hh-site-data/1`, named inputs, derivations D-1…D-8, drift ‖ F2-e: Methodology, honesty, spend, limits, changelog, data pages and phase-1 content | §9.3, §9.2 | 14: XF-021, XF-117, XF-118, XF-119, XF-120, XF-121, XF-122, XF-125, XF-139, XF-108, XF-109, XF-116, XF-141, XH-020 | GATE-G21, R6.3 | 95k + 10k; ≤1900 lines | offline-only | — | F2-b + F2-e |
| 259 | `259_R6.6__site-claims-pages.md` | R6.6 | ticket | SITE | F2-f: `claims-lexicon.json` (C-25 byte-identical refactor), `check-site.sh` legs, mutation battery, ci step ‖ F2-g: `pages.yml` (AR-28), `hh-site-publication/1` validator, public-profile refusals | §9.4, §9.5, §9.6 | 18: XF-009, XF-126, XF-127, XF-134, XF-135, XF-137, XF-138, XH-022, XD-003, XD-092, XC-014, XF-128, XF-129, XF-130, XF-140, XF-142, XH-018, XC-120 | R6.5 | 90k + 10k; ≤1700 lines | offline-only | — | F2-f + F2-g |
| 260 | `260_R6.7__price-lane.md` | R6.7 | ticket | INFRA | PRICE: `PricingTable.rate_unit`; the integer-µ$ money unit and the one rounding rule (shared owner); `NoPrice{unrepresentable_rate}` (AR-09) ‖ LANE: Ollama lane dialects, raw forward leg, auth from `auth_kinds` (AR-11) | §4.5, §4.1, §4.2 | 2: XC-116, XC-104 | R6.1 | 70k + 10k; ≤1200 lines | offline-only | — | PRICE + LANE |
| 261 | `261_R6.8__runner-core.md` | R6.8 | ticket | INFRA | RUNNER-CORE (a): `hharness-runner` crate, engine drive, K-2 edge check, `hh-experiment-slot/1` with the closed `withheld{…}` member set, outcome map (AR-04) ‖ RUNNER-CORE (b): Process-topology spike, process-level concurrency, two-process kill-point battery, durable-only resume | §3.2, §3.3 | 6: XB-101, XB-102, XB-103, XB-108, XB-120, XB-105 | R6.1 | 85k + 10k; ≤1700 lines | offline-only | — | RUNNER-CORE (a) + RUNNER-CORE (b) |
| 262 | `262_R6.9__cell-deadline-verdict.md` | R6.9 | ticket | INFRA | CELL-SPEC: Helper WORKDIR, cpu/memory limits, kernel-mediated copy-in/out, digest-only images (AR-06) ‖ DEADLINE: Task-declared deadlines above 900 s; legacy path byte-identical (AR-07) ‖ VERDICT: `harbor_reward_file/1`, `tb10_pytest_log/1`, bootstrap classifier, regrade bound, ANSI/workdir | §3.3, §3.1, §3.6, §3.4, §2.3 | 10: XB-106, XB-107, XB-117, XB-118, XB-119, XB-109, XB-110, XB-111, XB-112, XA-127 | R6.8 | 95k + 10k; ≤1800 lines | offline-only | — | CELL-SPEC + DEADLINE + VERDICT |
| 263 | `263_R6.10__proxy-core-canon.md` | R6.10 | ticket | INFRA | PROXY-CORE-a: `hh-model-proxy` server, Json control channel, token gate, surfaces, field policy, `GenParamPolicy`, durable receipts (AR-08, AR-10) ‖ K1: `hh-l1-canon/1` rules C01–C07 as a `hharness-wire` module over `Json::to_canonical_string` (the proxy-admissible home), golden + mutation tests | §4.1–§4.3, §7.4 | 10: XC-007, XC-101, XC-103, XC-105, XC-106, XC-107, XC-108, XC-109, XC-208, XD-105 | R6.7, R6.8 | 95k + 10k; ≤1700 lines | offline-only | — | PROXY-CORE-a + K1 |
| 264 | `264_R6.11__proxy-release-budget.md` | R6.11 | ticket | INFRA | PROXY-CORE-b: Release modes, non-stream deadline, failure taxonomy, crash recovery, catalog identity stop ‖ BUDGET: Root cap A_eff×(1−m), slices, worst-case reservation, ≤3 concurrency, carried root pool | §4.2, §4.6, §4.8, §5.3, §4.4, §10.1.1 | 15: XC-110, XC-111, XC-119, XC-123, XC-124, XC-322, XC-004, XC-112, XC-113, XC-114, XC-115, XC-126, XC-210, XH-005, XH-120 | R6.10 | 93k + 10k; ≤1700 lines | offline-only | — | PROXY-CORE-b + BUDGET |
| 265 | `265_R6.12__replay-archive-index.md` | R6.12 | ticket | INFRA | REPLAY: Proxy record/replay modes, `RequestCanonicalizer` (identity, json_canonical, `hh-l1-canon`), `RecordedTransport` ‖ ARCHIVE (a): `hh-receipt-index/1`, condenser, pass-level artifacts section, fold recompute (AR-20) | §4.9, §4.10 | 4: XC-122, XC-125, XF-124, XH-101 | R6.11, R6.10 | 85k + 10k; ≤1600 lines | offline-only | — | REPLAY + ARCHIVE (a) |
| 266 | `266_R6.13__recipe.md` | R6.13 | ticket | INFRA | RECIPE-a: `gce-backend.sh` quote (declaration quantities only, labelled `pre-quote`) / provision / verify, declaration schema, receipts, fake-shim battery (AR-15) ‖ RECIPE-b: Teardown nine-step, inventory, `compute-open.sh`, prepull, harvest, restart-only, deliver-secret | §3.9, §3.11 | 18: XB-201, XB-202, XB-203, XB-204, XB-205, XB-206, XB-208, XB-209, XB-212, XB-223, XB-207, XB-210, XB-211, XB-219, XB-220, XB-224, XH-009, XH-023 | R6.1 | 95k + 10k; ≤1900 lines | offline-only | — | RECIPE-a + RECIPE-b |
| 267 | `267_R6.14__cost-reconcile.md` | R6.14 | ticket | INFRA | COST-RECORD: `hh-compute-cost/1`, `hh-live-pricing/2` compute rows, phase spend ledger, the §3.10 ceiling formula in µ$ (sole compute pricing; AR-14) ‖ RECONCILE: Telemetry poller, settlement, `evaluate_admitted_costs`, `hh-slot-costs/1` fold (AR-16) | §3.10, §10.1.5, §4.5, §4.7 | 11: XB-213, XB-215, XB-216, XB-217, XH-006, XH-118, XC-006, XC-117, XC-118, XC-121, XC-127 | R6.13, R6.11, R6.12 | 90k + 10k; ≤1700 lines | offline-only | — | COST-RECORD + RECONCILE |
| 268 | `268_R6.15__egress.md` | R6.15 | ticket | INFRA | EGRESS (a): `allowlist_mediated` admission, model socket, static-musl `hh-cell-relay`, residuals (AR-05) ‖ EGRESS (b): `hh-egress-mediator` CONNECT tunnel, decide-before-byte ordering, phase swap, allowlist freeze record | §3.5, §3.3 | 5: XB-113, XB-115, XB-129, XB-114, XB-116 | R6.9, R6.10 | 95k + 10k; ≤1800 lines | offline-only | — | EGRESS (a; absorbs C2 EGRESS) + EGRESS (b) |
| 269 | `269_R6.16__runner-schedule-supervision.md` | R6.16 | ticket | INFRA | RUNNER-CORE (c): Packing, schedule kinds (k-round, pair-coupled), reattempt policy, admission cut-off, C rule ‖ RUNNER-SUPERVISION: Runner supervision: launch proxy + mediator over Json control, sole-writer store roots, per-slot token mint/revoke, cell socket grant, root-pool carry-in from harvested ledgers | §3.3, §3.9, §3.10, §7.5, §3.2, §4.1, §4.3, §10.1.1 | 10: XB-121, XB-127, XB-128, XB-214, XD-060, XD-111, XH-016, XH-017, XH-125, XB-104 | R6.8, R6.11, R6.15 | 95k + 10k; ≤1700 lines | offline-only | — | RUNNER-CORE (c) + RUNNER-SUPERVISION (seam row) |
| 270 | `270_R6.17__harbor-reproof.md` | R6.17 | ticket | INFRA | HARBOR-O: Harbor pin (Q-X41 rule at dispatch, S-4), job-config writer, read-only ingest, image-drift, host venv ‖ REPROOF machinery: Re-proof protocol (observe → freeze → prove; O/H-pub/H-allow), discordance register, packet label, `reproof` live-verify leg | §3.7, §5.9, §3.1, §5.5, §3.8, §10.3 | 7: XB-123, XB-124, XC-306, XC-315, XH-014, XB-126, XH-027 | R6.15 | 95k + 10k; ≤1600 lines | live-read | — | HARBOR-O + REPROOF machinery (B2 #8 code half) |
| 271 | `271_R6.18__select-protocol.md` | R6.18 | ticket | METHODS | SELECT: `task-selection/1` materializer in hh-eval; seeded split; outcome-field refusals ‖ D1-a: `comparative_protocol/1`, `comparative-quote/1` (compute lines priced by R6.14), support gate and envelopes (AR-12, AR-13) | §3.1, §6.2, §6.12, §6.3, §6.7, §6.1, §4.6 | 11: XB-122, XD-011, XH-012, XC-010, XD-002, XD-004, XD-020, XD-021, XD-050, XD-051, XH-030 | R6.1, R6.14 | 95k + 10k; ≤1700 lines | offline-only | — | SELECT + D1-a |
| 272 | `272_R6.19__estimators.md` | R6.19 | ticket | METHODS | Single-arm FM rate, flips, $/solve, tied-with-leader route, Holm family, `hh-comparative-results/1` (C-33) | §6.4, §6.5, §6.8, §6.11, §9.3 | 6: XD-001, XD-022, XD-030, XD-031, XD-061, XF-123 | R6.18 | 65k + 10k; ≤1000 lines | offline-only | — | D1-b (a) |
| 273 | `273_R6.20__site-leaderboard.md` | R6.20 | ticket | SITE | Leaderboard and frontier components over `hh-comparative-results/1` fixtures | §9.2 | 3: XF-110, XF-111, XF-115 | R6.5, R6.19 | 55k + 10k; ≤900 lines | offline-only | — | F2-c (a) |
| 274 | `274_R6.21__site-noise-grid-viewer.md` | R6.21 | ticket | SITE | F2-c (b): Noise, pilot/smoke, quote and refusal views over fixtures ‖ F2-d: Per-task grid, static trial pages, gated transcript viewer (XSS, sha-mismatch) | §9.2 | 5: XF-112, XF-113, XF-131, XF-132, XF-133 | R6.20, R6.5, R6.12 | 90k + 10k; ≤1600 lines | offline-only | — | F2-c (b) + F2-d |
| 275 | `275_R6.22__admit.md` | R6.22 | ticket | HOSTING | ADMIT-CORE (a): `hh-harness-admission/1` record + validator (AG1–AG9), terms, versions, declarations (AR-18) ‖ ADMIT-CORE (b): Two-stage bundle + per-task layer build, ST2 verify-only | §5.1, §5.3, §5.2 | 8: XC-202, XC-214, XC-301, XC-302, XD-081, XC-303, XC-304, XC-305 | R6.16, R6.12 | 95k + 10k; ≤1800 lines | offline-only | — | ADMIT-CORE (a) + ADMIT-CORE (b) |
| 276 | `276_R6.23__binary-sets-upload-kit.md` | R6.23 | ticket | HOSTING | BINARY-SETS: Carried artifacts: infrastructure and H binary sets plus layers and bundles — pinned-container build recipe, export with sha256 into `artifacts`, IAP load path, image-ID/binary digest verify, slot-identity digests ‖ ARCHIVE (b): Upload-tool admission, secret + live-credential scan, read-back verify, remediation procedure | §5.2, §4.10, §10.1.4, §3.11 | 5: XC-330, XH-102, XH-103, XH-104, XH-107 | R6.22, R6.12, R6.13 | 95k + 10k; ≤1600 lines | offline-only | — | BINARY-SETS (seam row) + ARCHIVE (b) |
| 277 | `277_R6.24__adapters.md` | R6.24 | ticket | HOSTING | ADAPT-CHAT-PY + ADAPT-CHAT-NODE: Golden renderers for original mini-swe-agent and Pi; closed env; effort; instruction pre-check ‖ ADAPT-MESSAGES: Claude Code renderer and Q-X26 admission terms (binary sha256, no OAuth/key) | §5.3, §7.1, §6.10, §5.4 | 9: XC-307, XC-308, XC-309, XC-310, XC-320, XE-001, XD-082, XD-121, XC-327 | R6.22 | 85k + 10k; ≤1400 lines | offline-only | — | ADAPT-CHAT-PY + ADAPT-CHAT-NODE (first-pass harnesses) + ADAPT-MESSAGES |
| 278 | `278_R6.25__capture-smoke.md` | R6.25 | ticket | HOSTING | CAPTURE: `/logs/agent` copy-out, post-hoc ATIF, integrity bundle, receipt reconciliation, self-modification flag ‖ SMOKE-BATTERY: SM-pos/429/402/slow/hang scripted recordings, offline checker, hosting-failure classifier | §5.6, §4.3, §5.7, §5.8, §5.3, §5.1 | 11: XC-207, XC-317, XC-318, XC-328, XC-203, XC-311, XC-312, XC-321, XC-323, XC-324, XC-329 | R6.24 | 95k + 10k; ≤1600 lines | offline-only | — | CAPTURE + SMOKE-BATTERY |
| 279 | `279_R6.26__ps0-legs-runner-check.md` | R6.26 | ticket | HOSTING | PS0 + LEGS: PS0 probe driver (≤ $0.05 plan: one call per dialect + seed, `tool_choice`, `cache_control`) and `live-verify.sh` legs `ps0`, `pp0`, `corpus`, `w4-main` ‖ RUNNER-CHECK: O-surface routing via host relay, R-replay comparator, `hh-runner-check/1` (R-live dropped) | §6.6, §10.1.4, §10.3, §5.7, §7.5 | 3: XC-325, XC-326, XH-031 | R6.25, R6.17 | 80k + 10k; ≤1400 lines | offline-only | — | PS0 + LEGS (seam row) + RUNNER-CHECK |
| 280 | `280_R6.27__method-dossier-answer-scan.md` | R6.27 | ticket | METHODS | D1-b (b): Comparative battery incl. first-pass T = 10 / H = 3 / H = 2 grid; review record; AR-17, AR-33 dossier ‖ D1-c: `answer-scan/1` integrity scanner, request-fingerprint check, review-record schema | §6.4, §10.1.3, §10.5, §6.9, §5.3 | 4: XD-023, XD-024, XD-070, XC-313 | R6.19, R6.22, R6.12 | 90k + 10k; ≤1500 lines | offline-only | — | D1-b (b) + D1-c |
| 281 | `281_GATE-G23__method-admission.md` | GATE-G23 | gate | METHODS | Admit or refuse the protocol, quote, estimators, schedule randomization and AR-33 departures | §6.4, §10.3 | — | R6.27 | — (marker) | n/a | STOP | GATE method-admission |
| 282 | `282_R6.28__l1-corpus-runner.md` | R6.28 | ticket | L1 | K2 + K3: Observation-replay executor (original run in a cell), synthetic fixtures M1–M19, turn-1 census generator, recorded-response driver ‖ K4: L1 runner: L1.0/L1.1/L1.2 legs, stable prefix, `hh-l1-diff/1`, `hh-l1-conformance/1` | §7.4, §7.1 | 4: XE-004, XD-103, XD-104, XD-109 | R6.10, R6.12, R6.24, R6.15 | 95k + 10k; ≤1900 lines | offline-only | — | K2 + K3 + K4 |
| 283 | `283_R6.29__fp-continuation-holds.md` | R6.29 | ticket | FP | FP runner rules: Cost-only continuation, projection formulas, per-arm caps (expected-cost admission), wall-time rule, stop rules ‖ FP holds: Hosted-only fallback schedule; HH-native lane-probe driver, `w4-lane-probe` leg and hold; PP0 hold dispositions | §10.1.4, §6.6, §10.1.2 | 8: XH-112, XH-117, XH-126, XD-040, XD-042, XH-123, XH-124, XH-127 | R6.16, R6.11 | 95k + 10k; ≤1700 lines | offline-only | — | FP runner rules (REQ-XH-112/117/126) + FP holds (REQ-XH-123/124/127) |
| 284 | `284_R6.30__fp-seal-select.md` | R6.30 | ticket | FP | FP seal: Outcome seal: sealed projections for readouts, checkpoint, dev partition and internal site profile ‖ SELECT commit: Commit first-pass split, draw, within-cell replacement order and dev list; W1 declaration + quote | §10.1.4, §10.1.2, §6.2, §3.10 | 4: XH-121, XD-010, XD-012, XD-014 | R6.29, R6.5, R6.18, GATE-G23, R6.17, R6.14 | 75k + 10k; ≤1100 lines | offline-only | — | FP seal (REQ-XH-121) + SELECT commit (D1-d part 1) |
| 285 | `285_HUMAN-H12__pages-enable.md` | HUMAN-H12 | human | SITE | Enable GitHub Pages (Source: GitHub Actions) | §10.3 | — | R6.6 | — (marker) | n/a | — | HUMAN Pages |
| 286 | `286_GATE-G22__pub-p1.md` | GATE-G22 | gate | PUB | Phase-1 publication: `hh-site-publication/1` pinning the phase-1 input set; labels; card transition | §9.6, §10.3 | — | HUMAN-H12 | — (marker) | n/a | STOP | GATE pub-P1 |
| 287 | `287_R6.31__pub-p1-record.md` | R6.31 | ticket | PUB | Commit the phase-1 publication record; README site link; `[results-site]` transition; deploy read | §9.6 | 1: XF-136 | GATE-G22 | 25k + 10k; ≤200 lines | operator-gated:GATE-G22 | — | PUB-P1 |
| 288 | `288_HUMAN-H13__billing-alert.md` | HUMAN-H13 | human | W1 | GCP billing alert (default $20; 50/90/100 %); billing account and controller `gcloud` auth `provided: yes`; recipients `provided: yes` | §10.3 | — | R6.30 | — (marker) | n/a | — | HUMAN billing-alert |
| 289 | `289_GATE-G25__prov-w1.md` | GATE-G25 | gate | W1 | PROV-W1: compute quote, dead-man + restart, network posture (Q-X55), re-proof scope, export estimate, Q-X39b | §10.3, §10.1.4 | — | HUMAN-H13 | — (marker) | n/a | STOP | GATE PROV-W1 |
| 290 | `290_R6.32__w1-run.md` | R6.32 | skeleton | W1 | W1 window body: infrastructure-set + bundle + layer builds and export, observe/freeze/prove re-proof, census, smokes, R-replay, teardown, harvest manifest verified (W1’s ARCHIVE-UPLOAD runs as W2’s first step, after FP-S authorizes uploads — SR-18 deviation) | §10.1.4, §3.8, §3.9 | 2: XB-125, XH-116 | GATE-G25 | — (fleshed after gate) | operator-gated:W1 | — | W1{PROV … TEARDOWN} (REPROOF, HOST-SMOKE, CENSUS) |
| 291 | `291_R6.33__fp-prereg.md` | R6.33 | ticket | FP | Post-replacement list frozen; pre-registration packet hashed (V5 declaration, fallback trigger); W2 quote; FP-S readiness record (Q-X62 clock) | §10.1.2, §10.1.3, §7.5 | 4: XH-111, XH-113, XH-115, XD-115 | R6.30, R6.32, GATE-G23 | 55k + 10k; ≤700 lines | offline-only | — | FP-PREREG (D1-d part 2) |
| 292 | `292_R6.34__e2-adr-wire.md` | R6.34 | ticket | HIR | E2-T0: Docs-only ADR bundle AR-21, AR-22, AR-23 and the AR-24 §5e.1 amendment text ‖ E2-T1: Model-port wire fidelity: non-stream transport, `tool_choice` omit, provider ids, verbatim args, replay | §7.3, §10.5 | 2: XE-107, XE-108 | R6.1 | 90k + 10k; ≤1400 lines | offline-only | — | E2-T0 + E2-T1 |
| 293 | `293_R6.35__tmpl-surface.md` | R6.35 | ticket | HIR | E2-T3: `hh-tmpl/1` engine, strict undefined, whitespace control, ledgered uname probe ‖ E2-T2 (a): Model port renders the compiled `ModelSurface` (the single renderer): render path, sections, `system` role, byte-identical legacy | §7.3 | 4: XE-112, XE-113, XE-103, XE-104 | R6.34 | 95k + 10k; ≤1600 lines | offline-only | — | E2-T3 + E2-T2 (a) |
| 294 | `294_R6.36__surface-tools-renderers.md` | R6.36 | ticket | HIR | E2-T2 (b): Extends R6.35's renderer: `tools[]` verbatim, completion surface, `tool_choice` only when declared ‖ E2-T4 (a): `json_head_tail@1`, `text_head_tail@1`, `error_format` classes | §7.3 | 4: XE-105, XE-106, XE-110, XE-111 | R6.35 | 85k + 10k; ≤1500 lines | offline-only | — | E2-T2 (b) + E2-T4 (a) |
| 295 | `295_R6.37__observation-plumbing.md` | R6.37 | ticket | HIR | G18: durable model-view rendering of exec observations, replay re-serves it | §7.3 | 1: XE-109 | R6.36 | 60k + 10k; ≤800 lines | offline-only | — | E2-T4 (b) |
| 296 | `296_GATE-G24__ar24-amendment.md` | GATE-G24 | gate | HIR | AR-24 canonical amendment `stop_rule.submit{marker, line, require_returncode}`: approve or refuse | §7.3, §10.3, §10.5 | — | R6.37 | — (marker) | n/a | STOP | GATE AR-24 amendment |
| 297 | `297_R6.38__strategy-exec.md` | R6.38 | ticket | HIR | E2-T5: Strategy params from slots, submit marker detection, format-error semantics, retry policy ‖ E2-T6 (a): `open_session.exec` member (AR-25), regenerate (CC7), `bind`-checked arming, refusal test | §7.3 | 4: XE-115, XE-116, XE-117, XE-118 | GATE-G24, R6.9 | 95k + 10k; ≤1600 lines | offline-only | — | E2-T5 + E2-T6 (a) |
| 298 | `298_R6.39__exec-defaults-topology.md` | R6.39 | ticket | HIR | E2-T6 (b): `exec_defaults`, helper merged stdio, in-cell process-tree kill, `returncode = -1` rendering ‖ E2-T8 (a): HH-native topology spike: two-process ownership, `env.close{detach}` + re-attach; the C-05 rung record lands at the rung reached, in every outcome | §7.3, §7.2 | 2: XE-119, XE-120 | R6.38, R6.16, R6.12 | 90k + 10k; ≤1500 lines | offline-only | — | E2-T6 (b) + E2-T8 (a; rung record) |
| 299 | `299_HUMAN-H14__account-setup.md` | HUMAN-H14 | human | W2 | Pre-FP-S account setup, one operator batch with three items: (1) Ollama automatic usage billing off (or meter-only recorded), exclusivity per window, `OLLAMA_API_KEY provided: yes` in `HH_OPERATOR_ENV_FILE`; (2) HF dataset repo (private) + fine-grained write token `provided: yes`; (3) optional dedicated, revocable Ollama key (Q-H1-5), item recorded `DECLINED` when not taken. Lands when (1) and (2) are recorded | §10.3, §4.10 | — | R6.33 | — (marker) | n/a | — | HUMAN Ollama-settings + HF-dataset + dedicated key (optional) |
| 300 | `300_GATE-G26__fp-s.md` | GATE-G26 | gate | W2 | FP-S first-pass spend: verbatim A ≤ $70, m, A_eff, caps, wall-time rule, fallback trigger, W2 quote, prereg hash, upload authorization (covers every window’s ARCHIVE-UPLOAD, W1’s included as W2’s first step); waits for R6.39’s rung record unless a Q-X62 trigger is recorded | §10.3, §10.1 | 2: XH-007, XH-013 | HUMAN-H14, R6.39 | — (marker) | n/a | STOP | GATE FP-S |
| 301 | `301_R6.40__w2-run.md` | R6.40 | skeleton | W2 | W2 window body: first, ARCHIVE-UPLOAD-W1 from the controller under the FP-S upload authorization (operator-accepted deviation; read-back verify, index commit), refusing PROV-W2 until W1’s index is `archive_verified`; then load, PS0, PP0 (3 × 1 × 3 hosted arms), L1 RECORD corpus (28), teardown, harvest, ARCHIVE-UPLOAD-W2 and SETTLE-W2 (controller-side settlement) | §10.1.4, §4.10, §4.7 | 2: XD-041, XH-010 | GATE-G26, R6.32, R6.23, HUMAN-H14 | — (fleshed after gate) | operator-gated:FP-S | — | W2{UPLOAD-W1, PROV, LOAD, PS0, PP0, CORPUS, TEARDOWN, UPLOAD-W2, SETTLE-W2} (absorbs ARCHIVE-UPLOAD-W1 and ARCHIVE-UPLOAD-W2 + SETTLE-W2) |
| 302 | `302_R6.41__compaction-participant.md` | R6.41 | ticket | HIR | E2-T7: `compaction_strategy` slot wiring; absent slot byte-identical ‖ E2-T8 (b): HH-native participant: loopback lane through the proxy, `participant.hir` slot fields, L2-without-L1-pass refusal | §7.3, §7.2, §7.4 | 4: XE-122, XE-121, XE-126, XD-102 | R6.39, R6.28, R6.16 | 85k + 10k; ≤1300 lines | offline-only | — | E2-T7 + E2-T8 (b) |
| 303 | `303_R6.42__partition-mini-hir.md` | R6.42 | ticket | HIR | FP-RECORD-PARTITION: Partition the W2 corpus by task (dev/gate-A/gate-B); commit digests; sealed dev projection ‖ E2-T9: mini-swe-agent 2.4.6 HIR document, representation ledger, O goldens from census and corpus | §10.1.2, §7.4, §6.6, §7.2, §7.3 | 8: XH-011, XH-114, XD-106, XE-101, XE-102, XE-114, XE-123, XD-123 | R6.40, R6.30, R6.41 | 90k + 10k; ≤1300 lines | offline-only | — | FP-RECORD-PARTITION + E2-T9 |
| 304 | `304_R6.43__sweep-d3fp.md` | R6.43 | ticket | HIR | E2-T10: Enumerate → sealed levels → `LevelSpec.ref_`; declared-delta and liveness refusal ‖ D3-FP: `sweep-registry/1` (V1–V9 recorded), V5 level sealed equal to R6.33's declaration, V5 arm in the W4 schedule | §8.1, §8.2, §8.7, §10.1.2 | 5: XE-124, XE-125, XD-201, XD-202, XD-220 | R6.42, R6.33 | 90k + 10k; ≤1500 lines | offline-only | — | E2-T10 + D3-FP (D3-a + D3-c, first-pass scope) |
| 305 | `305_GATE-G27__prov-w3.md` | GATE-G27 | gate | W3 | PROV-W3: compute quote incl. H-set build/export and load; frozen H record named; disposition `run` \| `not_run{fallback\|pp0_hold}` | §10.3 | — | R6.43, R6.28, R6.42, R6.41, R6.23 | — (marker) | n/a | STOP | GATE PROV-W3 |
| 306 | `306_R6.44__w3-run.md` | R6.44 | skeleton | W3 | W3 window body: load, H-set build + export, L1.0/L1.1/L1.2 on gate-A, teardown, harvest, ARCHIVE-UPLOAD-W3 (FP-S upload authorization); or fleshed in place to `unused{…}` with `n/a{w3_not_run}` under `not_run` | §10.1.4, §7.4, §4.10 | 5: XE-006, XE-007, XE-008, XE-009, XD-107 | GATE-G27 | — (fleshed after gate) | operator-gated:W3 | — | W3{PROV, LOAD, H-SET BUILD+EXPORT, L1 run, TEARDOWN, UPLOAD-W3} (K5, E2-T11; absorbs ARCHIVE-UPLOAD-W3) |
| 307 | `307_GATE-G28__prov-wc.md` | GATE-G28 | gate | Wc | PROV-Wc (at most once): `triggered{W1′\|W3′\|restart}` within Wc’s reservation, Σ ≤ $20 \| `PASSED — not triggered; Wc unused` | §10.3, §10.1.4 | — | R6.44 | — (marker) | n/a | STOP | GATE PROV-Wc |
| 308 | `308_R6.45__wc-run.md` | R6.45 | skeleton | Wc | Wc contingency body (W1′ repair re-proof, W3′ L1 rerun on gate-B, or restart overflow) with its harvest and ARCHIVE-UPLOAD-Wc; not triggered → fleshed in place to a ticket emitting `unused{not_triggered}` and `n/a{wc_unused}` | §10.1.4, §4.10 | — | GATE-G28 | — (fleshed after gate) | operator-gated:Wc | — | Wc{…, UPLOAD-Wc} (absorbs ARCHIVE-UPLOAD-Wc) |
| 309 | `309_GATE-G29__g-l1.md` | GATE-G29 | gate | W3 | G-L1 decision gate: `full` (pass for the exact H, frozen by digest) \| `fallback` (Q-X62 trigger fired, or both partitions failed); either PASSED with its basis verbatim | §10.3, §7.4, §10.1.4 | — | R6.44, R6.45 | — (marker) | n/a | STOP | GATE G-L1 |
| 310 | `310_R6.46__fp-quote-e-l2.md` | R6.46 | ticket | FP | FP-E quote: FP-E quote for the recorded G-L1 disposition (`full`: five arms; `fallback`: three hosted arms + `withheld{l1_not_passed}`); W2 measurement packet; D_max, caps, A_eff, carried root ‖ K7: L2 packet and analysis wiring: `ParityAdmissionDecl`, KL filters, veto layer, claim fold | §10.1.4, §10.1.5, §6.6, §6.8, §7.5 | 11: XD-043, XD-044, XD-062, XD-101, XD-110, XD-112, XD-113, XD-114, XD-116, XD-117, XD-118 | GATE-G29, R6.40, R6.43, R6.28, R6.19 | 87k + 10k; ≤1400 lines | offline-only | — | FP-E quote (D1-f first-pass form) + K7 |
| 311 | `311_HUMAN-H15__credit-purchase.md` | HUMAN-H15 | human | W4 | Ollama credit purchase (≤ $10) only if the FP-E quote needs it; A_eff re-declared; readout `Status: NOT_NEEDED` otherwise | §10.3 | — | R6.46 | — (marker) | n/a | — | HUMAN Ollama credit purchase (only if needed) |
| 312 | `312_GATE-G30__fp-e.md` | GATE-G30 | gate | W4 | FP-E main window: arm inclusion by priority, D_max, caps, m, A_eff, W4 quote, holds; `run{full\|hosted-only}` \| `end_after_w2` (REQ-XH-127(b)) | §10.3, §10.1.4 | — | R6.46 | — (marker) | n/a | STOP | GATE FP-E |
| 313 | `313_R6.47__w4-run.md` | R6.47 | skeleton | W4 | W4 window body: load, lane probe, rounds 1–2, checkpoint, rounds 3–5, teardown, GCP project deletion (last window), harvest, ARCHIVE-UPLOAD-W4 + SETTLE-W4; fleshed per step; hosted-only under `fallback`; `unused{scope_end}` with `n/a` upload/settle under `end_after_w2` | §10.1.4, §3.9, §4.10, §4.7 | — | GATE-G30 | — (fleshed after gate) | operator-gated:FP-E | — | W4{…, UPLOAD-W4, SETTLE-W4} (K8 first-pass form, D1-e; absorbs ARCHIVE-UPLOAD-W4 + SETTLE-W4) |
| 314 | `314_R6.48__fp-results.md` | R6.48 | ticket | RESULTS | FP-RESULTS (a): Seal opens; answer-scan; P2-FP packet; P4-FP packet, or typed `withheld{l1_not_passed}` under `fallback` ‖ FP-RESULTS (b): P3-L2 packet, L1-on-L2 audit, L3 report — or typed `withheld{l1_not_passed}` packets under `fallback` | §6.11, §6.5, §8.8, §10.1.3, §7.5, §7.6 | 5: XD-005, XD-090, XD-224, XD-119, XD-120 | R6.47, R6.27, R6.19, R6.46 | 90k + 10k; ≤1000 lines | offline-only | — | FP-RESULTS (a; D1-g first-pass form) + FP-RESULTS (b; K9) |
| 315 | `315_HUMAN-H16__flagged-review.md` | HUMAN-H16 | human | RESULTS | Flagged-pass review: dispositions with excerpt hashes before the results freeze; `Status: DEFERRED` + DF id withholds the freeze claim | §6.9, §10.3 | 1: XD-071 | R6.48 | — (marker) | n/a | — | HUMAN flagged-pass review |
| 316 | `316_R6.49__site-phase34-freeze.md` | R6.49 | ticket | RESULTS | F2-h: `/phase-3/` and `/phase-4/` pages on fixtures; first-pass labels; §8.8 phrases; typed `withheld` renderings ‖ results freeze: Apply review dispositions; freeze results; first-pass site data (internal profile); publication draft | §9.3, §7.7, §8.8, §10.1, §9.6, §6.9 | 5: XF-143, XD-126, XD-222, XH-119, XH-122 | R6.21, R6.46, R6.48, HUMAN-H16 | 85k + 10k; ≤1200 lines | offline-only | — | F2-h + results freeze |
| 317 | `317_GATE-G31__pub-fp.md` | GATE-G31 | gate | PUB | First-pass publication: `hh-site-publication/1`; flip as separate HUMAN act; terms read (Q-H1-6) | §9.6, §10.3 | — | R6.49 | — (marker) | n/a | STOP | GATE pub-FP |
| 318 | `318_HUMAN-H17__dataset-flip.md` | HUMAN-H17 | human | PUB | HF dataset visibility flip after both scans; revision pinned; `Status: DEFERRED` + DF id withholds the viewer claim | §10.3, §4.10 | — | GATE-G31 | — (marker) | n/a | — | HUMAN dataset flip |
| 319 | `319_R6.50__pub-fp-record.md` | R6.50 | ticket | PUB | Commit the first-pass publication record; pinned dataset revision; deploy read | §9.6, §4.10 | 1: XH-105 | GATE-G31, HUMAN-H17 | 30k + 10k; ≤200 lines | operator-gated:GATE-G31 | — | PUB-FP |
| 320 | `320_HUMAN-H18__billing-reconcile.md` | HUMAN-H18 | human | TAIL | Consolidated billing reconciliation: one dated entry per window (W1, W2, W3, Wc, W4), each a billing read ≥ 24 h after that window’s teardown, appended as soon as it is due; W3/Wc entries `N/A` when the window did not run / was unused; lands when all five are recorded (expected HOLD only while the W4 read is not yet due) | §10.3 | — | R6.32, R6.40, R6.44, R6.45, R6.47 | — (marker) | n/a | — | HUMAN reconcile-W1 + W2 + W3 + Wc + W4 (consolidated) |
| 321 | `321_CAP6.1__cap1.md` | CAP6.1 | capstone | TAIL | Independent whole-round gap analysis (fresh-context verdicts over the REQ-X `COVERAGE_MATRIX.csv` before any run ledger; seam hunt over the shared-decision table) plus the composed offline verification of Round 6 as one unit; accepted deviations checked as deviations | tail | — | R6.50 | 95k + 10k; tail template | offline-only | — | CAP.1 + CAP.2 |
| 322 | `322_CAP6.2__closure.md` | CAP6.2 | capstone | TAIL | Closure capstone: two sums, the $100 recompute from billed figures (HUMAN-H18), spend per kind and phase, `compute-open.sh` empty, `DELETE_REQUESTED`; then `reconcile-build` backlog + readiness with live reads (compute-open, Ollama balance/usage, dataset visibility, operator-disk retention); writes the later-pass routing register (34 ids) and re-reads it live | tail, §10.1.1, §10.6 | 39 (34 routing records + 5 closure): XA-126, XC-314, XC-316, XD-013, XD-052, XD-204, XD-205, XD-206, XD-207, XD-208, XD-209, XD-210, XD-211, XD-212, XD-213, XD-214, XD-215, XD-216, XD-217, XD-218, XD-219, XD-221, XD-225, XE-002, XE-010, XE-011, XE-012, XE-013, XE-128, XE-129, XE-130, XE-131, XE-132, XH-033, XH-021, XH-110, XH-029, XH-106, XH-015 | CAP6.1, HUMAN-H18 | 90k + 10k; tail template | live-read | — | CAP.3 + REC.1 (closure capstone) |
| 323 | `323_CAP6.3__reconcile.md` | CAP6.3 | capstone | TAIL | Reconciliation capstone: `reconcile-build` spec reconciliation (landed code, ADRs, REQ-X coverage, §0.3 history), then the integration plan from actual stack CI and branches (recommend, never merge); owns no ids — contributes its records to CAP6.2's closure and to GATE-G32 | tail, §10.6 | — | CAP6.2 | 75k + 10k; tail template | offline-only | — | REC.2 + REC.3 (operator 2026-11-07: "Split into 4-run tail (Recommended)") |
| 324 | `324_GATE-G32__accept.md` | GATE-G32 | gate | TAIL | Round-6 acceptance (GATE-ACCEPT role) over the minimal tail: CAP6.1’s verdicts, CAP6.2’s closure, live reads and routing register, and CAP6.3’s reconciliation and integration plan; accepted deviations listed | tail, §10.3 | — | CAP6.3 | — (marker) | n/a | STOP | GATE-ACCEPT |
| 325 | `325_DOC6.1__docs.md` | DOC6.1 | docs | TAIL | Docs refresh: `refresh-repo-docs` (reader docs from accepted evidence), then `agent-docs` (agent docs from the landed implementation) | tail | — | GATE-G32 | 75k + 10k; tail template | offline-only | — | DOC.1 + DOC.2 |

### Totals

| kind | rows |
|---|---|
| ticket | 45 (of which 2 are no-new-code post-gate record rows: R6.31, R6.50) |
| skeleton | 5 (W1 R6.32, W2 R6.40, W3 R6.44, Wc R6.45, W4 R6.47 — each also carries its window's ARCHIVE-UPLOAD act, W1's as W2's first step, and SETTLE for W2/W4) |
| gate | 13 (GATE-G20…G31 + GATE-G32 accept) |
| human | 7 (HUMAN-H12…H18) |
| capstone / reconcile / docs | 3 / 0 / 1 |
| **total rows** | **74** (sequences 252–325) |
| **implement-spec rows** | **49** (45 tickets + 4 tail runs) |

The operator asked for about 45 implement-spec rows (band 40–50). The 45 tickets carry 3,765k of focused estimate: the 84 former ticket rows they absorb summed to 4,450k, less 685k of shared reading (mean 84k, 16 tickets at the 95k ceiling). Three more merges would reach 45, but every remaining row under 80k is pinned by a constraint this plan keeps: R6.4 (60k) and R6.37 (60k) sit against GATE-G21 and GATE-G24, which the spec's §10.2 DAG places between them and their neighbours; R6.31 (25k) and R6.50 (30k) are the post-gate publication records, one per publication gate; R6.33 (55k) is the only row between W1 and the E2 block and shares no surface with either; R6.19 (65k) and R6.20 (55k) are halves of the D1-b and F2-c pre-splits, and R6.19 + R6.20 = 65 + 55 − 10 − 10 = 100k; R6.7 (70k) and R6.30 (75k) have no shared-surface neighbour with room under 95k. Spec §10.6's first-cut estimate (100–110 rows, 85–90 implement-spec) is a decomposer's sizing note, and the operator's re-partition puts the round below it by decision; skeleton fleshing adds window rows later, as before, and they are not counted here.

## Decomposition decisions

The plan can be revised at run time. `orchestrate-build` may split an overflowing row (OL-16 size trigger, stopping at the next stage boundary), merge trivial neighbours, or move the Wc block. Each such change goes through a scoped `decompose-spec mode=extend` insert with a suffix-letter file and a `## Plan extensions` line. Ids are never re-bound to another slug.

### Re-partition (operator Q 2026-11-07)

**Decision (verbatim, planning conversation 2026-11-07).** Concern raised: "On the face of it 97 implement-spec runs feels like way too many. Are we really sure we want to do the full, very heavy-duty implement-spec process individually across 97 tasks when using SWE-2 (kimi k3) with 256k context and high effort? We should be very critical about whether this level of granularity is necessary versus, e.g. 60 tickets, 45 tickets, 20 tickets, or even less than that". Answer 1: "~45 tickets (Recommended)" — each ticket sized to a ~90–110k working set (focused estimate ≤ ~95k + 10k verification reserve), merging along shared code, files and decisions, with the record-only rows folded into the work they record. Answer 2: "3-run tail (Recommended)" — CAP6.1 (independent gap analysis), CAP6.2 (closure capstone absorbing the REC duties and the later-pass routing records), the acceptance gate, and one docs row invoking `refresh-repo-docs` then `agent-docs`.

**Before → after.**

| measure | before (Phase-4-reviewed) | after |
|---|---|---|
| rows (sequences) | 128 (252–379) | 73 (252–324) |
| implement-spec rows | 97 (89 tickets + 8 tail) | **48** (45 tickets + 3 tail) |
| no-new-code record rows | 7 (R6.12, R6.59, R6.61, R6.69, R6.71, R6.74, R6.78) | 2 (R6.31, R6.50: one per publication gate) |
| skeletons / gates | 5 / 13 | 5 / 13 (none merged) |
| HUMAN rows | 13 | 7 |
| tail | `tail=full`: CAP ×3, gate, REC ×3, DOC ×2 (8 runs) | minimal: CAP6.1, CAP6.2, GATE-G32, DOC6.1 (3 runs) |
| largest focused estimate (non-tail) | 70k | 95k (16 rows) |
| implement-spec rows before PROV-W1 / FP-S / G-L1 | 59 / 71 / 81 | 31 / 38 / 41 |
| summed focused estimate, tickets | 4,600k (89 rows) | 3,765k (45 rows) |

**Method.** Adjacent rows were merged when they read the same crate, schema or script, or share a decision; a merged row's estimate is the sum of its former estimates less shared reading (10k orientation per absorbed row plus the named shared surface). No merge crosses a gate that the spec's §10.2 DAG or the window sequence puts between two rows, and no shared decision moves after one of its consumers (machine-checked). The former rows survive as ordered stages inside the merged row (invariant 10), so the Phase-4 review's measured surfaces and seams still bound each stage.

#### Merge map

Every non-gate row, new id first (gates keep their ids and are not listed). Old ids are the pre-re-partition ids of the reviewed draft.

| new row | kind | old rows (pre-re-partition ids) | focused estimate arithmetic (k) |
|---|---|---|---|
| R6.1 | ticket | R6.1, R6.1a | 60 + 55 − 10 orientation − 15 (`supplement_guard.py` (41 KB) and its battery read once) = 90 |
| R6.2 | ticket | R6.2, R6.3 | 40 + 55 − 10 − 5 (`p1_records.py` and the raw records just committed, read in-row) = 80 |
| R6.3 | ticket | R6.4, R6.6 | 60 + 60 − 10 − 15 (R5.59 records, the P1-A/B packets and `check-position.sh` read once) = 95 |
| R6.4 | ticket | R6.7 | unchanged (F2-a must precede the mock gate) |
| R6.5 | ticket | R6.8, R6.9 | 65 + 55 − 10 − 15 (shell tokens and `hh-site-data/1` read once; the pages are emitted by the generator written in-row) = 95 |
| R6.6 | ticket | R6.10, R6.11 | 70 + 45 − 10 − 15 (`check-site.sh`/`check-position.sh` and the CI workflow read once) = 90 |
| R6.7 | ticket | R6.13, R6.14 | 30 + 50 − 10 (both in the gateway pricing/lane surface; orientation only) = 70 |
| R6.8 | ticket | R6.15a, R6.15b | 55 + 50 − 10 − 10 (slot record and engine drive, written in-row, then exercised by the kill-point battery) = 85 |
| R6.9 | ticket | R6.16, R6.17, R6.23 | 45 + 40 + 45 − 20 (two orientations) − 15 (helper cell spec and the Harbor/TB task declaration format read once) = 95 |
| R6.10 | ticket | R6.20a, R6.41 | 70 + 40 − 10 − 5 (`hharness-wire` `Json::to_canonical_string`, used by receipt digests and the canonicalizer) = 95 |
| R6.11 | ticket | R6.20b, R6.21 | 60 + 55 − 10 − 12 (proxy server, receipt and control surfaces read once) = 93 |
| R6.12 | ticket | R6.22, R6.31a | 50 + 55 − 10 − 10 (proxy recording and receipt format read once; the index condenses it) = 85 |
| R6.13 | ticket | R6.19a, R6.19b | 60 + 55 − 10 − 10 (`gce-backend.sh`, declaration schema and the fake-shim battery read once) = 95 (departure from §10.6 pre-split; B3 seam kept as the in-row stage boundary) |
| R6.14 | ticket | R6.25, R6.26 | 50 + 60 − 10 − 10 (µ$ unit, `hh-live-pricing` and the phase spend ledger read once) = 90 |
| R6.15 | ticket | R6.24a, R6.24b | 60 + 60 − 10 − 15 (profile, residual vocabulary and relay read once by the mediator half) = 95 |
| R6.16 | ticket | R6.15c, R6.24c | 60 + 55 − 10 − 10 (`hharness-runner` crate and process topology read once) = 95 |
| R6.17 | ticket | R6.27, R6.28 | 55 + 55 − 10 − 5 (Harbor job-config writer and ingest read once) = 95 |
| R6.18 | ticket | R6.18, R6.29 | 45 + 65 − 10 − 5 (`hh-eval` module layout; `task-selection/1` consumed in-row) = 95 |
| R6.19 | ticket | R6.30a | unchanged (D1-b pre-split kept) |
| R6.20 | ticket | R6.32a | unchanged (F2-c pre-split kept) |
| R6.21 | ticket | R6.32b, R6.33 | 45 + 60 − 10 − 5 (R6.20’s component conventions and the generator read once) = 90 |
| R6.22 | ticket | R6.34a, R6.34b | 60 + 55 − 10 − 10 (`hh-harness-admission/1`, written in-row, consumed by the build half) = 95 |
| R6.23 | ticket | R6.34c, R6.31b | 55 + 55 − 10 − 5 (`hh-receipt-index/1` `artifacts` section and the controller-side harvest scripts read once) = 95 |
| R6.24 | ticket | R6.35, R6.36 | 55 + 45 − 10 − 5 (renderer interface, owned in-row, extended for Claude Code) = 85 |
| R6.25 | ticket | R6.37, R6.38 | 50 + 60 − 10 − 5 (capture outputs consumed by the offline checker in-row) = 95 |
| R6.26 | ticket | R6.38a, R6.39 | 45 + 50 − 10 − 5 (smoke recordings and `live-verify.sh` read once) = 80 |
| R6.27 | ticket | R6.30b, R6.40 | 60 + 45 − 10 − 5 (`hh-eval` comparative module read once) = 90 |
| R6.28 | ticket | R6.42, R6.43 | 55 + 65 − 10 − 15 (observation-replay executor and census outputs consumed by the runner in-row) = 95 |
| R6.29 | ticket | R6.44, R6.45 | 60 + 55 − 10 − 10 (§10.1.4 rules and projection formulas, owned in-row, consumed by the holds) = 95 |
| R6.30 | ticket | R6.46, R6.47 | 50 + 40 − 10 − 5 (the dev list the seal’s dev-partition projection covers) = 75 |
| HUMAN-H12 | human | HUMAN-H12 | marker (no budget) |
| R6.31 | ticket | R6.12 | unchanged (post-gate record row; one per publication gate) |
| HUMAN-H13 | human | HUMAN-H13 | marker (no budget) |
| R6.32 | skeleton | R6.48 | skeleton (no budget; fleshed after its gate) |
| R6.33 | ticket | R6.58 | unchanged (needs W1; nothing else sits between W1 and the E2 block) |
| R6.34 | ticket | R6.49, R6.50 | 45 + 55 − 10 (the AR-21 text drafted in-row is the wire-fidelity half’s contract) = 90 |
| R6.35 | ticket | R6.51, R6.52 | 50 + 60 − 10 − 5 (model-port render path read once) = 95 |
| R6.36 | ticket | R6.52a, R6.53a | 50 + 50 − 10 − 5 (R6.35’s single renderer read once and extended) = 85 |
| R6.37 | ticket | R6.53b | unchanged (E2-T4 must precede the AR-24 gate; T5 sits after it) |
| R6.38 | ticket | R6.54, R6.55 | 60 + 50 − 10 − 5 (dispatch gate and `open_session` surface read once) = 95 |
| R6.39 | ticket | R6.55a, R6.57 | 50 + 55 − 10 − 5 (helper session lifecycle read once) = 90 |
| HUMAN-H14 | human | HUMAN-H15, HUMAN-H16, HUMAN-H17 | marker (no budget) — consolidated: shared trigger |
| R6.40 | skeleton | R6.60, R6.59, R6.61 | skeleton (no budget; fleshed after its gate) — absorbs the window’s record-only rows |
| R6.41 | ticket | R6.56, R6.57a | 40 + 55 − 10 (orientation only; both wire slot fields into the participant) = 85 |
| R6.42 | ticket | R6.62, R6.63 | 35 + 70 − 10 − 5 (the W2 corpus and its digests, read once: partitioned, then used for the O goldens) = 90 |
| R6.43 | ticket | R6.64, R6.65 | 55 + 50 − 10 − 5 (sweep enumeration and sealed levels read once) = 90 |
| R6.44 | skeleton | R6.68, R6.69 | skeleton (no budget; fleshed after its gate) — absorbs the window’s record-only rows |
| R6.45 | skeleton | R6.70, R6.71 | skeleton (no budget; fleshed after its gate) — absorbs the window’s record-only rows |
| R6.46 | ticket | R6.72, R6.66 | 45 + 60 − 10 − 8 (G-L1 disposition record, R6.33’s pre-registration and `hh-l1-conformance/1` read once by both halves) = 87 |
| HUMAN-H15 | human | HUMAN-H21 | marker (no budget) — renumbered |
| R6.47 | skeleton | R6.73, R6.74 | skeleton (no budget; fleshed after its gate) — absorbs the window’s record-only rows |
| R6.48 | ticket | R6.75, R6.76 | 55 + 55 − 10 − 10 (seal opening and the results-packet conventions read once) = 90 |
| HUMAN-H16 | human | HUMAN-H23 | marker (no budget) — renumbered |
| R6.49 | ticket | R6.67, R6.77 | 55 + 45 − 10 − 5 (generator internal profile and `hh-site-data/1` read once) = 85 |
| HUMAN-H17 | human | HUMAN-H24 | marker (no budget) — renumbered |
| R6.50 | ticket | R6.78 | unchanged (post-gate record row; one per publication gate) |
| HUMAN-H18 | human | HUMAN-H14, HUMAN-H18, HUMAN-H19, HUMAN-H20, HUMAN-H22 | marker (no budget) — consolidated: shared trigger |
| CAP6.1 | capstone | CAP6.1, CAP6.2 | 60 + 60 − 10 − 15 (`COVERAGE_MATRIX.csv` and the landed `BUILD_INDEX` read once) = 95 |
| CAP6.2 | capstone | CAP6.3, REC6.1, REC6.2, REC6.3 | 60 + 50 + 55 + 45 (+ ≈10 routing register, from CAP6.1) − 30 (three orientations) − 40 (build memory, spend ledgers and spec §10.6 read once) = 150 — **over the 95k bound by operator decision** (3-run tail); see the tail record |
| DOC6.1 | docs | DOC6.1, DOC6.2 | 50 + 45 − 10 − 10 (docs tree and accepted evidence read once by both passes) = 75 |

**Folded record-only rows (operator rule) and where their obligations move.**

| former row | folded into | how | REQ ownership |
|---|---|---|---|
| R6.59 ARCHIVE-UPLOAD-W1 | R6.40 (W2 skeleton) | first step of W2's fleshed body, controller-side, under the FP-S upload authorization, before PROV-W2 (SR-18 deviation kept; see ambiguity 1) | none owned (`—`); REQ-XH-102's "execution act of that window's ARCHIVE-UPLOAD row" is a pre-registered step of the fleshing; register contributors re-pointed to R6.40 |
| R6.61 ARCHIVE-UPLOAD-W2 + SETTLE-W2 | R6.40 (W2 skeleton) | post-harvest controller session closing W2's body | none owned; REQ-XC-127's settlement act and REQ-XH-102's upload act are fleshing steps |
| R6.69 ARCHIVE-UPLOAD-W3 | R6.44 (W3 skeleton) | after W3's harvest; `n/a{w3_not_run}` inside the `unused{…}` fleshing | none owned |
| R6.71 ARCHIVE-UPLOAD-Wc | R6.45 (Wc skeleton) | after Wc's harvest; `n/a{wc_unused}` inside the `unused{not_triggered}` fleshing | none owned |
| R6.74 ARCHIVE-UPLOAD-W4 + SETTLE-W4 | R6.47 (W4 skeleton) | after W4's harvest, before the round's closure; `n/a` under `end_after_w2` | none owned |
| R6.12 PUB-P1 record | kept as R6.31 | the record pins GATE-G22's decision and reads back the deploy, which exists only after the gate, so a post-gate row remains (one per publication gate) | REQ-XF-136 → R6.31 (same row, renumbered) |
| R6.78 PUB-FP record | kept as R6.50 | as above, after GATE-G31 and HUMAN-H17 | REQ-XH-105 → R6.50 (same row, renumbered) |

None of the five folded rows owned an adopted id, so no requirement changes owner through the fold. Every other ownership change is a renumbering: each merged row owns the union of its former rows' ids. The cross-row moves are all in the tail: CAP6.1's 34 routing ids, CAP6.3's REQ-XH-021 and REQ-XH-110, REC6.1's REQ-XH-029 and REQ-XH-106, and REC6.2's REQ-XH-015 all move to CAP6.2. HUMAN-H23's REQ-XD-071 stays with the same row, now numbered HUMAN-H16.

**Consolidated HUMAN rows (shared trigger).**

| new row | former rows | shared trigger | exit / tokens |
|---|---|---|---|
| HUMAN-H14 account setup | HUMAN-H15 Ollama settings, HUMAN-H16 HF dataset, HUMAN-H17 dedicated key (optional) | the FP-S presentation (all three already formed one operator batch before GATE-G26) | lands when item 1 (`OLLAMA_API_KEY provided: yes`, billing off or meter-only, exclusivity) and item 2 (dataset repo + token `provided: yes`) are recorded; item 3 is recorded `DECLINED` when not taken |
| HUMAN-H18 billing reconciliation | HUMAN-H14, H18, H19, H20, H22 (reconcile W1, W2, W3, Wc, W4) | round closure (CAP6.2's billed-figure recompute) | one dated entry per window, appended as soon as its ≥ 24 h read is due; W3/Wc entries `N/A` when the window did not run / was unused; lands when all five are recorded |

Kept separate, because their triggers differ: HUMAN-H12 Pages (the Pages workflow has landed; precedes pub-P1), HUMAN-H13 billing alert (precedes the first paid compute at PROV-W1), HUMAN-H15 credit purchase (only if the FP-E quote needs it; `NOT_NEEDED`), HUMAN-H16 flagged-pass review (precedes the freeze; `DEFERRED` + DF id), HUMAN-H17 dataset flip (after GATE-G31; `DEFERRED` + DF id). **Placement of HUMAN-H18.** The operator asked for it immediately before the closure capstone; the tail must stay one contiguous block (capstone, capstone, gate, docs), so HUMAN-H18 sits immediately before the tail block, CAP6.2 depends on it and the HUMAN placement rule (before a capstone) holds. CAP6.1 does not need it, but in practice the W4 read falls due during R6.48 … R6.50, so the expected HOLD is short. **REQ-XH-126(b) unchanged.** FP-E's D_max uses "billed figure where a reconcile read exists and at their ceiling otherwise": R6.46 cites any per-window entry already appended to HUMAN-H18's readout (dated, append-only) and the ceiling for the rest.

**Gates.** All 13 are kept. No two are the same decision point: GATE-G27 and GATE-G29 fall on either side of the W3 run; GATE-G22 and GATE-G31 are different publications; GATE-G25, GATE-G26 and GATE-G30 authorize different spend; GATE-G28 is at most once and may be `not triggered`; GATE-G24 is a canonical amendment; GATE-G23 admits methods; GATE-G21 the identity; GATE-G20 the bootstrap; GATE-G32 acceptance. GATE-G20's content gains the re-partition departures for verbatim acceptance.

**Departures from the frozen §10.6 hand-off (listed at GATE-G20).** (1) `tail = full` → the minimal tail (operator decision; the manifest banner declares `tail=minimal`). (2) RECIPE's pre-split (a/b) is merged into R6.13 at 95k; B3's seam is the row's internal stage boundary. (3) The row and implement-spec counts fall below the first-cut band (operator decision). PROXY-CORE (R6.10 / R6.11), F2-c (R6.20 / R6.21) and D1-b (R6.19 / R6.27) stay split. The watch items: ADMIT-CORE (a)+(b) merged at 95k (R6.22, watched), with (c) in R6.23; RUNNER-CORE (a)+(b) merged (R6.8), with (c) in R6.16; E2-T2 and E2-T4 stay split across rows (R6.35 / R6.36 / R6.37).

**Owed outside this plan (coordinator).** The invariants file's XO-02 names R6.1a; it should read "from R6.2 on, `PASS` with the stock checker is required", because R6.1 is now the whole GUARD-X. The retained guard's minimal-tail patch and the Round-6 banner token are the coordinator's.

### Skeleton fleshing mechanics (SR-27)

The guard routes on the skeleton row itself, and an unlanded skeleton at the head of the chain is `skeleton-dispatch` REFUSED. Fleshing therefore **rewrites the skeleton row in place** (kind `skeleton` → `ticket`, the same id, sequence and filename slug, with a dated manifest note row, as in Round-5 rows 215/217/223) and adds suffix-letter rows only after it (e.g. `R6.47a`, `R6.47b`). The in-place row inherits the skeleton's owned ids (rule O2). **Each window's fleshing now also carries its post-harvest controller acts**: ARCHIVE-UPLOAD for every window and SETTLE for W2 and W4; W2's fleshing additionally starts with W1's ARCHIVE-UPLOAD (SR-18), and its in-place row refuses PROV-W2 until W1's index is `archive_verified`. W4's body (D = 22.5 h) is expected to flesh into one row per step: load + lane probe, rounds 1–2, checkpoint readout, rounds 3–5, teardown + project deletion + upload + settlement. A window that does not run (Wc not triggered, W3 `not_run`, W4 `end_after_w2`) is fleshed in place into a ticket that emits a typed `unused{…}` record and its `n/a{…}` upload record and nothing else (Round-5 precedent: manifest notes 223 and 229). Fleshed rows are implement-spec runs added at fleshing time; the counts above exclude them, as before.

### Bootstrap and the legacy guard warnings (SR-2, coordinator ruling)

With the stock checker the retained guard returns `verdict FAIL` (46 legacy `reqcov` `unexplained-diagnostic` warnings on rows 083–164f; they predate this program). GATE-G20's pre-registered content therefore authorizes **R6.1's dispatch only**, with the retained guard's ROUTE computed in `--no-checker` mode and the stock build-memory validator run separately with 0 violations — the round-5 GATE-G8 precedent (its scoped bootstrap authorization of R5.1/R5.2). This is never a silent substitution: G20 quotes it. The 46 warnings are an **OPEN FINDING owned by R6.1**. R6.1 is now the whole GUARD-X (the former R6.1 and R6.1a as two ordered stages), and its AC requires that after it lands, `check-supplement-guard.sh` **with** the stock checker returns `PASS` with `DISPATCH — R6.2`, the 46 warnings classified into the reviewed baseline with dated provenance (an append-only amendment citing the list's sha256), never silently. REQ-XH-002's AC is phrased against that first product row, R6.2.

### Disposition gates and fallback routing (SR-1)

The guard has no branch: a gate either lands (`PASSED`) or stops the chain. Every pre-declared alternative path is therefore a **pre-registered disposition** of a gate, recorded as `PASSED — disposition <token>; basis: <verbatim>` (precedent: round-5 GATE-G15/G16 "PASSED — out-of-scope disposition"). Only an operator refusal stops the chain. Disposition basis text never contains the tokens `FAILED` or `NOT PASSABLE` (the guard reads them as non-PASSED); R6.1 carries a fixture for every disposition below.

| gate | pre-registered dispositions | rows that branch |
|---|---|---|
| GATE-G26 FP-S | waits for R6.39's rung record, **unless** a Q-X62 trigger record exists (third specific of §10.1.4: "once the trigger fires, FP-S no longer waits for the rung record") | the marker names the waiver |
| GATE-G27 PROV-W3 | `run` ‖ `not_run{fallback}` (a Q-X62 trigger is recorded before PROV-W3 is presented) ‖ `not_run{pp0_hold}` (REQ-XH-127(b): a held original mini-swe-agent withholds the corpus) | R6.44 fleshed in place to `unused{…}` with `n/a{w3_not_run}` for its upload; HUMAN-H18's W3 entry `N/A` |
| GATE-G28 PROV-Wc | `triggered{W1′\|W3′\|restart}` ‖ `PASSED — not triggered; Wc unused` | R6.45 fleshed in place to `unused{not_triggered}` with `n/a{wc_unused}`; HUMAN-H18's Wc entry `N/A` |
| GATE-G29 G-L1 | `full` (L1 passed for the exact H, frozen by digest) ‖ `fallback` (the Q-X62 trigger fired, or L1 did not pass on either gate partition) | R6.46 quotes five arms or the three hosted arms; R6.47's fleshing builds the matching schedule; R6.48 emits P4-FP and P3-L2 packets or typed `withheld{l1_not_passed}` packets (precedent rows 229–237) |
| GATE-G30 FP-E | `run{full}` ‖ `run{hosted-only}` ‖ `end_after_w2` (REQ-XH-127(b)'s scope decision default) | R6.47 fleshed in place to `unused{scope_end}` with `n/a` upload and settlement; R6.48 withholds by the hold's typed reason |

`fallback` is the disposition name; the typed reason every withheld packet carries is the spec's own member `withheld{l1_not_passed}` (REQ-XH-123), never a new vocabulary item. K7 (now R6.46's second stage) and F2-h (now R6.49's first stage) are $0 code that lands under either disposition (their pages render the typed withholdings).

**Q-X62 sequencing kept (coordinator ruling).** "FP-S waits for the E2-T8 rung record" stays: GATE-G26 depends on R6.39, whose second stage is the E2-T8 (a) spike. R6.33 FP-PREREG depends on no E2 row — it needs only R6.30, W1 and GATE-G23 (its H is fixed by rule, "the version that passes G-L1"). The **rung record is a landed output in every outcome**: rung 1, 2 or 3 is recorded honestly at the rung reached (rung 3 raises REQ-XE-120's AR-04 amendment ADR to the operator and is not itself a trigger). A stalled or failed spike therefore never holds FP-S. R6.33 also commits the **FP-S readiness record** computation: an offline, $0 recompute over landed records (R6.33 itself, HUMAN-H14's readout date, W1's verified harvest manifest) that yields the instant at which every FP-S precondition except the rung record was met and, from it, the Q-X62 14-day trigger instant. The GATE-G26, GATE-G29 and R6.46 records re-run and cite it; nothing is pre-dated.

**Contingency (pre-registered scoped insert).** If the Q-X62 trigger fires while an E2 row ahead of GATE-G26 (R6.34…R6.39, GATE-G24) is unlanded, or GATE-G24 is refused (an operator refusal: the spec's "a refusal stops E2-T5 and is raised to the operator"), the operator's recorded decision authorizes a scoped insert that permutes {HUMAN-H14, GATE-G26, R6.40} ahead of the first unlanded E2 row. It is a pure permutation: none of those rows depends on an E2 row except GATE-G26's R6.39 edge, which the recorded trigger waives. The HH-native arms then follow G-L1's disposition. FP-RECORD-PARTITION now rides with E2-T9 in R6.42 and stays after E2-T7/T8 (b); it was never needed before W2's harvest and GATE-G27, so the smaller permuted set loses nothing.

### Conscious tradeoffs

1. **Linearization A recommended; Q-X46 is the operator's decision at GATE-G20 (SR-22).** Q-X46 has no recorded default, so G20 presents both. The plan's order is A (phase 1 and site shell first; H1 recommends it). 38 implement-spec rows precede FP-S (31 precede PROV-W1) at unmeasured throughput, so racing the credit reset at 2026-11-22T16:30:34Z is not attempted. **B′ (re-expressed over the coarser rows) is a checked pure permutation:** R6.6, R6.20, R6.21, HUMAN-H12, GATE-G22 and R6.31 move, in that order, to just after GATE-G29, so neither FP-S nor G-L1 waits on them (machine-checked backward-only from the marker below). The re-partition shrinks B: R6.3 (P1-C+D+E) and R6.5 (F2-b + F2-e) can no longer move, because R6.5 carries F2-b, which FP-SEAL (R6.30) needs, together with F2-e, which needs R6.3. B′ takes three code rows (≈ 235k) off the pre-FP-S path, where the former B took eight (≈ 470k); under B′, 27 implement-spec rows precede PROV-W1 and 34 precede FP-S. If the operator chooses B and wants the former reach, a scoped split of R6.5 at its stage boundary restores it.
   <!-- linearization-B-prime: R6.6 R6.20 R6.21 HUMAN-H12 GATE-G22 R6.31 -->
2. **Operator batches (SR-22, SR-15/16 ruling).** Under A, phase-1 publication (HUMAN-H12, GATE-G22, R6.31) sits just before HUMAN-H13, so the Pages, pub-P1, billing-alert and PROV-W1 acts form one operator batch; the FP-S acts (HUMAN-H14, GATE-G26) form another. No HUMAN row sits in front of a code row that does not depend on it (machine-checked).
3. **E2 to the rung record sits between W1 and FP-S; E2-T7 and E2-T8 (b) sit after W2.** W1 ($0 model, ≈ $5 compute ceiling) runs as soon as P2-infra is complete, and re-proof exclusions are known before FP-PREREG. R6.41 (T7 + T8 (b): compaction slot, loopback lane, slot fields, L1-pass refusal) is needed only before W3/W4, so it does not pad the path to FP-S.
4. **$0 rows needed only before W3 or W4 run after W2** (R6.41, R6.42, R6.43); **K7 and F2-h run after G-L1** (SR-17): K7 rides with the FP-E quote in R6.46 and F2-h with the freeze in R6.49, so neither pads the FP-S → G-L1 path that Q-X62's 14-day trigger times; the G-L1 → FP-E path shrinks from three rows to one. GATE-G27 depends on D3-FP (R6.43; the H set includes the phase-4 level, REQ-XC-330), on T7 (R6.41), on the binary sets (R6.23), on K4 (R6.28) and on the partition (R6.42).
5. **Wc block between W3 and G-L1.** Its most likely trigger is W3′, an L1 rerun on gate-B after a gate-A failure, so G-L1 is presented after any W3′. A W1′ repair or a post-W4 restart overflow moves the block through a scoped insert. If no trigger exists, GATE-G28 records `PASSED — not triggered; Wc unused` and R6.45 lands as typed `unused`/`n/a` records (SR-16). A gate-A L1 failure also needs an allowlist version bump (REQ-XD-106), a scoped-insert code row before PROV-Wc.
6. **Record-only rows folded into the work they record (operator rule).** Each window's ARCHIVE-UPLOAD and, for W2 and W4, SETTLE (REQ-XC-127, controller-side, $0) are steps of that window's fleshed body in the same post-harvest controller session; W1's upload is W2's first step. This removes five dispatches whose only content was one controller act each.
7. **Publication records as their own post-gate rows** (R6.31, R6.50; one per publication gate). The gate records the operator's decision; a ticket commits the `hh-site-publication/1` record, adds the README link (REQ-XF-136, only after the first record) and reads back the deploy. No gate cell writes a file.
8. **Trigger-only HUMAN rows are not chain rows.** These are the credential rotation after a post-upload secret finding (REQ-XH-107, Q-X59), key rotation and custom-domain DNS (only if chosen). Each is created by a scoped insert when its trigger fires and is listed under the manifest's `## Human prerequisites`. The optional dedicated Ollama key (Q-H1-5) is item 3 of HUMAN-H14, recorded `DECLINED` when not taken.
9. **Sizes at the operator's bound.** Merged estimates are the calibrated former estimates summed less shared reading; 16 tickets sit at 95k and none above it. A measured working set above the bound still stops at the next boundary (XO-03, OL-16); invariant 10 makes that boundary a stage boundary inside the merged row, so a scoped insert takes over the remaining stages without re-doing landed ones.
10. **Reconcile billing in one pre-tail row (coordinator ruling on SR-15/16, consolidated by the operator).** HUMAN-H18 sits immediately before the tail, whose CAP6.2 consumes the billed figures (§10.3: reconcile rows withhold "round closure totals"). It is an **expected HOLD** there only while a read is not yet due; the operator appends each window's entry as soon as its ≥ 24 h have elapsed, and R6.46's D_max uses any entry already recorded (REQ-XH-126(b)). Window execution stays strictly sequential (each paid window waits for the previous window's verified harvest, REQ-XH-120). REQ-XH-008's "GATE-PROV → PROV → runs → TEARDOWN → HUMAN-reconcile" is enforced by R6.1 as an ordered per-window subsequence (each window's reconcile entry after that window's skeleton, which now holds its teardown and archive acts), not as a contiguous block; G20 presents this interpretation for operator confirmation.
11. **Row count below the §10.6 band** (Totals): an operator decision trading per-row headroom (95k instead of ≤ 75k) for 49 fewer implement-spec runs.

### Pre-split list and watch list (spec §10.6) — dispositions

| spec item | disposition | seam | shared-decision owner |
|---|---|---|---|
| PROXY-CORE (pre-split) | **kept split** R6.10 / R6.11 | server, Json control, gate, surfaces, field policy, receipts (with K1) ‖ release modes, deadline, failure taxonomy, recovery, catalog stop (with BUDGET) | receipt format and deposit ordering → R6.10 |
| RECIPE (pre-split) | **merged** R6.13 (re-partition departure, G20) | B3's seam kept as the in-row stage boundary: quote/provision/verify + declaration + receipts + shim harness ‖ teardown/inventory/compute-open/prepull/harvest/restart/deliver-secret | `hh-compute-receipt/1` + shim battery → R6.13 |
| F2-c (pre-split) | **kept split** R6.20 / R6.21 | leaderboard + frontier ‖ noise, pilot/smoke, quote, refusal views (with F2-d) | component conventions over `hh-comparative-results/1` → R6.20 |
| D1-b (pre-split) | **kept split** R6.19 / R6.27 | estimators + results schema ‖ battery + review record + AR-17/AR-33 dossier (with D1-c) | estimator API and `hh-comparative-results/1` → R6.19 |
| ADMIT-CORE (watch) | (a)+(b) **merged** R6.22 (95k, watched); (c) binary sets in R6.23 with the upload kit | record + validator ‖ two-stage build, ST2 ‖ carried binary sets + export/load/verify (REQ-XC-330, SR-4) | `hh-harness-admission/1` → R6.22; carried artifacts → R6.23 |
| RUNNER-CORE (watch) | (a)+(b) **merged** R6.8; (c) with supervision in R6.16 | crate, engine drive, slot record, outcome map ‖ topology spike, kill-point battery, durable-only resume ‖ scheduling (+ supervision) | `hh-experiment-slot/1` + `withheld{…}` → R6.8; schedule kinds → R6.16 |
| E2-T2 (watch) | **kept split across rows** R6.35 / R6.36 | render path, sections, `system` role (with T3) ‖ `tools[]`, completion surface, `tool_choice` (with T4 (a)) — one renderer, extended, never re-implemented | `ModelSurface` renderer → R6.35 |
| E2-T4 (watch) | **kept split across rows** R6.36 / R6.37 | pure renderers ‖ G18 durable observation plumbing | `ResultRenderSpec` members → R6.36 |
| (beyond the lists) GUARD-X | **merged** R6.1 (the SR-14 split re-merged at 90k) | register, routing, baseline ‖ marker, pattern, HUMAN, skeleton, disposition checks | REQ-X register legs → R6.1 |
| (beyond the lists) EGRESS | **merged** R6.15 | profile admission + static-musl relay ‖ mediator CONNECT tunnel + ordering + allowlist freeze | profile and residual vocabulary → R6.15 |
| (beyond the lists) ARCHIVE | **split across rows** R6.12 / R6.23 | index + condenser (with REPLAY) ‖ upload tool, scans, read-back, remediation (with the binary sets) | `hh-receipt-index/1` → R6.12 |
| (beyond the lists) E2-T6, E2-T8 | **split across rows** R6.38 / R6.39 / R6.41 | AR-25 schema + regeneration (with T5) ‖ exec defaults + helper (with the T8 (a) topology spike + rung record) ‖ lane, slot fields, refusal (with T7) | `open_session.exec` → R6.38; rung record → R6.39 |
| merges | every other merge is in the [merge map](#merge-map) | rows sharing a crate, schema, script or decision | — |

### Shared-decision ownership (the earlier row owns; later rows cite and never redefine)

A decision whose former consumer now sits in the owner's own row is consumed in-row (a later stage); the table lists only cross-row consumers.

| shared decision / seam | owner | consumers |
|---|---|---|
| REQ-X register legs, Round-6 routing, reviewed-baseline amendment, absorbed coverage checker; gate-disposition templates and HUMAN readout tokens (`DEFERRED` + DF id, `DECLINED`, `NOT_NEEDED`, `N/A`) | R6.1 | every gate and HUMAN contract from GATE-G21 to GATE-G32, CAP6.1, CAP6.2 |
| integer µ$ money unit and the one rounding rule (round up per line) | R6.7 | R6.11, R6.14, R6.29 |
| `hh-experiment-slot/1` record and engine drive | R6.8 | R6.9, R6.14 (cost fold), R6.16, R6.25, R6.29, R6.41 |
| closed `withheld{…}` member set (`budget`, `window_deadline`, `admission:AG<n>`, `admission:pp0_infra_hold`, `admission:native_lane_hold`, `l1_not_passed`, `sealed_until_results`) inside `hh-experiment-slot/1` | R6.8 | R6.16, R6.22, R6.29, R6.30, R6.46, R6.48, site rows |
| process topology and the kill-point battery | R6.8 | R6.16, R6.39 |
| schedule kinds (seeded k-round interleave, pair-coupled launch, catalog snapshot per block), packing | R6.16 | R6.29, R6.43, R6.46 |
| `hh-compute-receipt/1`, shim battery; `quote` emits declaration quantities labelled `pre-quote` (REQ-XD-051) | R6.13 | R6.14 |
| `hh.model_io/1` `proxy` member, receipt deposit ordering, rewrite versioning, Json control channel | R6.10 | R6.11, R6.12, R6.14, R6.16, R6.25, R6.27 |
| root cap A_eff × (1 − m), carried root pool | R6.11 | R6.16, R6.29, R6.33, R6.46, FP-S/FP-E readouts |
| `hh-l1-canon/<v>` as a `hharness-wire` module (the only home the proxy may depend on under REQ-XC-101's list; no new crate, no new dependency) | R6.10 | R6.12, R6.28, R6.41 |
| proxy recording format (record/replay) | R6.12 | R6.25, R6.26, R6.28, R6.39 |
| `allowlist_mediated` profile and residual vocabulary | R6.15 | R6.22, R6.24 |
| runner supervision of proxy and mediator (launch, Json control, sole-writer roots), slot-token mint/revoke, cell socket grant, root-pool carry-in | R6.16 | R6.22, R6.25, R6.29, R6.39, R6.41 |
| §3.10 compute ceiling `$_D = r·(D + H_rs) + S·r_disk·h_r + 0.12·E_gib` in integer µ$ — the only compute pricing | R6.14 | R6.18, R6.30, R6.33, R6.46 |
| phase spend ledger (`kind ∈ {model, compute}`) | R6.14 | CAP6.2 |
| `task-selection/1` | R6.18 | R6.30 |
| `comparative_protocol/1`, `comparative-quote/1` (the only quote, C-11; compute lines priced by R6.14) | R6.18 | R6.19, R6.27, R6.30, R6.33, R6.46 |
| `hh-comparative-results/1` (C-33) | R6.19 | R6.20, R6.21, R6.48, R6.49 |
| intended-vs-served counts on arm-bearing packets (REQ-XA-006) | R6.2 | R6.19, R6.48 |
| re-proof packet, discordance register (allowlist freeze record schema: R6.15) | R6.17 | W1 fleshing (R6.32), R6.30, R6.33 |
| `hh-receipt-index/1` (incl. pass-level `artifacts`) | R6.12 | R6.21, R6.23, the window skeletons' upload steps (R6.32 harvest, R6.40, R6.44, R6.45, R6.47) |
| `hh-harness-admission/1` | R6.22 | R6.23, R6.24, R6.25, R6.26, R6.27 |
| carried binary sets (infrastructure set, H set), layer/bundle export, IAP load, image-ID and binary digest verification, slot-identity digests | R6.23 | R6.32, R6.44 and R6.47 fleshing, R6.41 |
| harness renderer interface (admission × slot → argv, env, files) | R6.24 | R6.26, R6.28 |
| PS0 probe driver; `live-verify.sh` legs `ps0`, `pp0`, `corpus`, `w4-main` | R6.26 | R6.40 and R6.47 fleshing |
| projection formulas p̂_h / t̂_h and continuation semantics | R6.29 | R6.46, W4 fleshing (R6.47) |
| HH-native lane-probe driver and the `w4-lane-probe` leg | R6.29 | R6.47 fleshing |
| outcome-seal projections | R6.30 | R6.42, every readout, R6.48 (opens the seal) |
| `hh-l1-diff/1`, `hh-l1-conformance/1`, L1 pass id | R6.28 | R6.41 (runner refusal), R6.42, R6.49, GATE-G29 |
| pre-registration packet (claims, ε, caps, fallback trigger), the first-pass V5 declaration (Q-X56 text, factor id, parameters) and the FP-S readiness record | R6.33 | GATE-G26, R6.43 (seals a level equal to it, with a refusal test), R6.46, GATE-G29, R6.48 |
| compiled `ModelSurface` render path (single renderer; R6.36 extends, never re-implements) | R6.35 | R6.36, R6.37, R6.38, R6.42 |
| `open_session.exec` schema (AR-25; regenerate, CC7) | R6.38 | R6.39 |
| HH-native topology and the C-05 rung record | R6.39 | GATE-G26 (Q-X62), R6.41 |
| `sweep-registry/1` | R6.43 | R6.48 |
| `claims-lexicon.json` (C-25: after the P1-E record leg, byte-identical refactor) | R6.6 | R6.20, R6.21, R6.49 |
| `hh-site-data/1` and the closed derivation list | R6.5 | every later site row (R6.6, R6.20, R6.21, R6.49) |
| `hh-site-publication/1` | R6.6 | R6.31, R6.49, R6.50 |
| later-pass routing register (34 ids) and full-target closure | CAP6.2 | GATE-G32 (signs over it); written and re-read live inside CAP6.2 |

### ADR and amendment placement (spec §10.5; Round-5 dossier precedent)

The spec rules that "Every ADR lands with its implementing ticket, except E2-T0's docs-only bundle and AR-01 in P1-C". This plan follows it; where a merged row holds the docs-only bundle (R6.34) it is the row's first, docs-only stage and commit, ahead of any code. The two dossier stages (E2-T0 in R6.34 and D1-b (b) in R6.27) mirror Round 5's R5.3–R5.5 pattern: each gathers ADRs for an approval gate.

| register | row | note |
|---|---|---|
| AR-01 (ADR), AR-02 (REC), AR-03 (REC) | R6.3 | Q-X25 default: a dated deviation record, not an amendment |
| AR-04 | R6.8 | the Harbor tool admission (pin + `uv.lock`, `uv`) is completed in R6.17, which applies the Q-X41 rule at dispatch (S-4); a C-05 rung-3 outcome in R6.39 raises its amendment to the operator |
| AR-05 | R6.15 | adds the musl target to `rust-toolchain.toml` (REQ-XB-129) |
| AR-06, AR-07 | R6.9 | the CELL-SPEC and DEADLINE stages |
| AR-08 (+ possible AMEND-C, S-9 INTERP), AR-10 | R6.10 | if O2's INTERP becomes an amendment, it is raised from R6.10, never adopted silently |
| AR-09, AR-11 | R6.7 | the PRICE and LANE stages |
| AR-12, AR-13 | R6.18 | AR-13 merges REQ-XD-051 and REQ-XB-221; the round-5 §9.2 envelope is unchanged |
| AR-14 / AR-15 | R6.14 / R6.13 | AR-15 admits `gcloud` and the pinned podman package set as tools |
| AR-16 (SUPP §4.5) | R6.14 | the RECONCILE stage |
| AR-17, AR-33 | R6.27 (method dossier stage) → GATE-G23 | §10.5 lists AR-33 as "first needed by FP-PREREG", but the method-admission gate admits its departures, so it is drafted before GATE-G23 |
| AR-18 / AR-20 | R6.22 / R6.12 | the AR-20 upload-tool admission record is completed in R6.23 |
| AR-21, AR-22, AR-23 (ADR), AR-24 (AMEND-C text) | R6.34 (E2-T0 docs stage) → GATE-G24 | a refusal at GATE-G24 stops E2-T5 and is raised to the operator (contingency above) |
| AR-25 (SCHEMA) | R6.38 | `check-drift.sh`; generated clients regenerated, never hand-edited |
| AR-28 | R6.6 | — |
| AR-31 (decomposition decision) | R6.1 | recorded in GUARD-X's contract and this plan |
| AR-32 (SUPP + `check-position.sh`) | R6.3 | — |
| AR-19, AR-26, AR-27, AR-29, AR-30 | none in Round 6 | later-pass (AR-29 is "not v1") |

The count matches §10.5: the 21 Round-6 ADRs, one AMEND-C, one SCHEMA, two REC, two SUPP, one INTERP and one decomposition decision each have exactly one row above.

### Gate and HUMAN numbering

Numbering continues from GATE-G19 and HUMAN-H11 (§10.3). Gate ids keep their allocation, so the chain order differs from the numeric order in two places (GATE-G23 precedes GATE-G22; GATE-G25 precedes GATE-G24). HUMAN ids are renumbered compactly in chain order (former ids in parentheses).

| id | row | | id | row |
|---|---|---|---|---|
| GATE-G20 | R6 plan review + bootstrap (+ re-partition departures) | | HUMAN-H12 | GitHub Pages enablement (H12) |
| GATE-G21 | site identity mock review | | HUMAN-H13 | GCP billing alert (H13) |
| GATE-G22 | phase-1 publication | | HUMAN-H14 | account setup: Ollama settings + exclusivity, HF dataset + token, optional dedicated key (H15 + H16 + H17) |
| GATE-G23 | method admission | | HUMAN-H15 | Ollama credit purchase (only if needed; `NOT_NEEDED`) (H21) |
| GATE-G24 | AR-24 canonical amendment | | HUMAN-H16 | flagged-pass review (H23) |
| GATE-G25 | PROV-W1 | | HUMAN-H17 | dataset visibility flip (H24) |
| GATE-G26 | FP-S | | HUMAN-H18 | consolidated billing reconciliation W1, W2, W3, Wc, W4 (pre-tail; `N/A` entries for windows that did not run) (H14 + H18 + H19 + H20 + H22) |
| GATE-G27 | PROV-W3 (`run` / `not_run`) | | | |
| GATE-G28 | PROV-Wc (`triggered` / not triggered) | | | |
| GATE-G29 | G-L1 (`full` / `fallback`) | | | |
| GATE-G30 | FP-E | | | |
| GATE-G31 | first-pass publication | | | |
| GATE-G32 | Round-6 acceptance (`accept`) | | | |

Two rows are deliberately absent: the PS0 early gate (Q-X23 default: not used) and the W4 checkpoint, which is automatic. §10.3's "PROV-W3 + G-L1" line becomes two rows (GATE-G27, GATE-G29), because the two decisions fall on either side of the W3 run.

**HUMAN rows and the guard (corrected, SR-15).** A pending HUMAN row at the head of the chain is a guard HOLD; there is no skip path. The plan therefore places each HUMAN row immediately before the first row that genuinely needs it (a gate, the tail, or a row whose `Depends on` names it), and every HUMAN contract names its non-PENDING readout tokens. REQ-XH-026's "code tickets proceed past it only by opening a DEFERRALS row" is realized as a readout `Status: DEFERRED` citing the opened DF id (the guard's landed test accepts any non-PENDING status; R6.1 adds the fixture and requires the DF id). HUMAN-H16 and HUMAN-H17 use it to withhold the freeze claim and the transcript-viewer claim respectively (BM-TICKET-05); HUMAN-H14's optional item uses `DECLINED`, HUMAN-H15 `NOT_NEEDED`, HUMAN-H18's W3/Wc entries `N/A`. Exits added by the review (SR-20): HUMAN-H13 records the billing account and the controller's `gcloud` auth as `provided: yes` (never the billing-account id, which REQ-XB-219 keeps out of every committed record); HUMAN-H14 records `OLLAMA_API_KEY provided: yes` in `HH_OPERATOR_ENV_FILE`.

### Spec ambiguities resolved here (to be confirmed at GATE-G20)

1. **W1's archive upload vs its authorization — an operator-accepted deviation (SR-18).** REQ-XH-102 has each window upload "before the next paid window's gate", authorized once at FP-S, and its AC has "each paid window's gate readout cites the previous window's `archive_verified` index". For W1 the next paid gate *is* FP-S, so W1's upload cannot be both authorized by FP-S and precede it. The plan runs ARCHIVE-UPLOAD-W1 as the **first step of the W2 skeleton's fleshed body** (R6.40, `Depends on: GATE-G26, R6.32, R6.23, HUMAN-H14`), immediately after GATE-G26 and before PROV-W2; W1's harvest stays on the operator's disk meanwhile (REQ-XH-106); the FP-S readout cites W1's *verified harvest manifest*; R6.40 refuses PROV-W2 until W1's index is `archive_verified`. This departs from REQ-XH-102's text, so **GATE-G20 lists it as a deviation needing the operator's verbatim acceptance**, CAP6.1 checks it as a deviation (never as a pass), and R6.1's window-pattern check tolerates W1's upload as the opening step of W2's fleshed body. The alternative, moving the upload authorization to PROV-W1, needs a spec amendment.
2. **Q-X46 linearization** has no default: the operator decides at GATE-G20 (tradeoff 1; B is now B′).
3. **Wc placement** (tradeoff 5) and **reconcile placement** (tradeoff 10, consolidated into HUMAN-H18).
4. **Ownership of later-pass ids.** The spec adopts them (no `deferred` disposition is used, §0.2) but forbids decomposing their bodies now (REQ-XH-001). **Resolution:** CAP6.2 owns the Round-6 obligation for each (write its routing to Round 7 or Round 8, with its trigger gate), with `round_scope: routed(…)` in the register, so the routing register exists before GATE-G32 signs; CAP6.2 re-reads it live in the same row (SR-25, adapted to the operator's tail). No Round-7 or Round-8 skeleton is added (Q-X45 default, hand-off §1).
5. **"SELECT commit" and "FP-PREREG" vs D1-d.** D1-d's single row is split by time. R6.30 commits the split, draw, replacement order and dev list before PROV-W1 (REQ-XH-116), together with the W1 declaration and quote PROV-W1 needs (REQ-XH-118). R6.33 freezes the post-replacement list and hashes the packet after W1.
6. **FP-E's quote input.** FP-E is "presented after G-L1 + W2 settlement". Its D_max uses W3's and Wc's billed figures where a reconcile entry exists and ceilings otherwise. R6.46 therefore runs after GATE-G29 (W2's settlement is inside R6.40), and the credit-purchase HUMAN row follows the quote that may need it.
7. **Re-proof machinery.** §10.6 says "execution tickets … contain no new code", and B2 #8 is execution only. The observe → freeze → prove driver and its packet are an explicit code stage (R6.17) before W1, which also registers the `reproof` leg (Q-X39b default, confirmed at PROV-W1). The same rule puts the PS0 driver and the W2/W4 legs in R6.26 and the lane-probe driver in R6.29 (SR-6).
8. **First-pass runner rules** (REQ-XH-112, 117, 121, 123, 124, 126, 127) have no shaping name in the spec. They become R6.29 (continuation, caps, wall time; fallback, lane probe and hold, PP0 hold) and R6.30 (outcome seal, with the selection commit), all before PROV-W1. REQ-XH-127's PP0 hold applies in W2.
9. **REQ-XA-127's $0 targeted probe** (`grid-pattern-transform`, `swe-bench-langcodes`) lives in R6.9's VERDICT stage. If it cannot run locally, it is owed to W1's fleshing (contributor), never counted as run.
10. **G-L1's two outcomes and the trigger** are GATE-G29's pre-registered dispositions (`full`, `fallback`), both recorded as PASSED with their basis; the trigger record and `hh-l1-conformance/1` carry the substance (SR-1).
11. **REQ-XD-215 vs REQ-XD-220** (SR-25). §10.1.6 lists both under one first-pass adaptation ("baseline shared with the L2 H side and concurrent with it, under one pre-registration"). XD-220's core predicate (phase-4 needs a G-L1 pass for the exact H) applies in the first pass, so R6.43 owns it, citing AR-33's concurrency departure, with R6.41's refusal and R6.33's pre-registration as contributors. XD-215's core predicate (a confirmation window on the census) has no first-pass instance, so it stays routed to Round 7; its routing record cites the same AR-33 shared-baseline departure.
12. **REC rows under the minimal tail.** REQ-XH-029 says "Each round's REC rows SHALL read live state". With the operator's 3-run tail the REC duties are CAP6.2's (aliases REC.1–REC.3 → CAP6.2), so CAP6.2's ACs carry the live-read predicates, and the reads happen before GATE-G32 instead of after it; the acceptance gate signs over them.

### Alias table (spec ticket-shaping names → chain ids)

| shaping name | chain id(s) | | shaping name | chain id(s) |
|---|---|---|---|---|
| B0 bootstrap | GATE-G20 | | E2-T0 + E2-T1 | R6.34 |
| GUARD-X | R6.1 | | E2-T3 + E2-T2 (a) | R6.35 |
| P1-A + P1-B | R6.2 | | E2-T2 (b) + E2-T4 (a) / E2-T4 (b) | R6.36 / R6.37 |
| P1-C + P1-D + P1-E | R6.3 | | E2-T5 + E2-T6 (a) / E2-T6 (b) + E2-T8 (a) | R6.38 / R6.39 |
| P1-F (phase 1b) | — (Q-X24; routed) | | E2-T7 + E2-T8 (b) | R6.41 |
| F2-a / F2-b + F2-e / F2-f + F2-g | R6.4 / R6.5 / R6.6 | | E2-T9 (with FP-RECORD-PARTITION) / E2-T10 (with D3-FP) | R6.42 / R6.43 |
| F2-c (a) / F2-c (b) + F2-d | R6.20 / R6.21 | | E2-T11 (mini L1 gate inputs) | R6.44 (W3 skeleton) |
| F2-h | R6.49 (with the results freeze) | | E2-T12…T16 | — (Round 8) |
| PRICE + LANE | R6.7 | | K1 / K2 + K3 + K4 | R6.10 (with PROXY-CORE-a) / R6.28 |
| RUNNER-CORE (a, b) / (c) | R6.8 / R6.16 | | K5 (L1 gate run) | R6.44 (W3 skeleton) |
| CELL-SPEC + DEADLINE + VERDICT | R6.9 | | K6 (O-only RECORD corpus) | R6.40 (W2 skeleton) |
| SELECT | R6.18 (with D1-a) | | K7 | R6.46 (with the FP-E quote) |
| RECIPE-a + RECIPE-b | R6.13 | | K8 (L2 run) | R6.47 (W4 skeleton, first-pass form) |
| PROXY-CORE-a / -b | R6.10 / R6.11 | | K9 (L1-on-L2 audit + L3) | R6.48 |
| BUDGET / REPLAY | R6.11 / R6.12 | | K10–K14 | — (Round 8) |
| EGRESS (+relay; C2 EGRESS folded, C-02) | R6.15 | | D1-a | R6.18 |
| runner supervision (seam) | R6.16 | | D1-b | R6.19, R6.27 |
| COST-RECORD + RECONCILE | R6.14 | | D1-c | R6.27 |
| HARBOR-O | R6.17 | | D1-d | R6.30 + R6.33 |
| REPROOF (B2 #8) | R6.17 (code) + R6.32 (W1 run) | | D1-e (pilot execution) | R6.40 + R6.47 |
| ARCHIVE (AR-20) | R6.12 (index), R6.23 (upload kit) | | D1-f (quote) | R6.46 (first-pass FP-E form) |
| ADMIT-CORE (a, b) / BINARY-SETS | R6.22 / R6.23 | | D1-g (results) | R6.48 |
| ADAPT (mini, Pi, Claude Code) | R6.24 | | D3-a + D3-c (first-pass scope) = D3-FP | R6.43 |
| TERMINUS-HOST | — (Round 8) | | D3-b, D3-d … D3-i | — (Round 7) |
| CAPTURE + SMOKE-BATTERY / RUNNER-CHECK | R6.25 / R6.26 | | FP-PREREG / FP-RECORD-PARTITION | R6.33 / R6.42 |
| PS0 driver + live legs (seam) | R6.26 | | SETTLE-W2 / SETTLE-W4 | R6.40 / R6.47 (window fleshing) |
| HOST-SMOKE, O-CAPTURE, CENSUS (W1 runs) | R6.32 | | FP-RESULTS | R6.48 (+ R6.49 freeze) |
| ARCHIVE-UPLOAD-Wn | W1 and W2: R6.40; W3: R6.44; Wc: R6.45; W4: R6.47 (window fleshing) | | REC.1–3 | CAP6.2 |
| CAP.1 + CAP.2 / CAP.3 | CAP6.1 / CAP6.2 | | GATE-ACCEPT / DOC.1 + DOC.2 | GATE-G32 / DOC6.1 |

### Tail (minimal, operator Q 2026-11-07) — read decisions

The Round-6 manifest banner declares `tail=minimal` (the coordinator's retained-guard patch): gap-analysis capstone, closure capstone, operator-acceptance gate, one docs row, contiguous at the end of the round, the gate's filename containing `accept`. HUMAN-H18 sits immediately before the block.

| role | id | read decision |
|---|---|---|
| CAP.1 + CAP.2 | CAP6.1 | independent fresh-context verdicts over the REQ-X `COVERAGE_MATRIX.csv` before reading any run ledger; seam hunt across the shared-decision table; then the composed offline verification of the landed round (it authorizes no new window, and unrun legs are xfails whose reason starts with a DF id); every accepted deviation (W1 upload, reconcile placement, the re-partition departures) checked as a deviation, never a pass |
| CAP.3 + REC.1 + REC.2 + REC.3 | CAP6.2 | in order, each `reconcile-build` mode writing its committed artifact before the next starts: **closure** — the two sums, REQ-XH-110's $100 recompute from billed figures (HUMAN-H18), REQ-XH-021's per-kind and per-phase spend, `compute-open.sh` empty (REQ-XH-008), the GCP project's `DELETE_REQUESTED` state; **backlog + readiness, live state** — `compute-open.sh`, the Ollama balance and usage, the HF dataset visibility, operator-disk retention (REQ-XH-029, REQ-XH-106), read at the row because they expire; **spec reconciliation, live state** — landed code, ADRs and REQ-X coverage, the §0.3 register's history (REQ-XH-015), and the later-pass routing register (34 ids) written and then re-read live; **integration plan, live state** — stack CI and branches; recommends, never merges (`mergePolicy: OPERATOR`) |
| GATE-ACCEPT | GATE-G32 | fresh operator signature over CAP6.1's verdicts and CAP6.2's closure, live reads and routing register; accepted deviations listed |
| DOC.1 + DOC.2 | DOC6.1 | `refresh-repo-docs` (reader docs from accepted evidence), then `agent-docs` (agent docs from the landed implementation), as two ordered stages |

**CAP6.2's size (operator-ruled exception).** CAP6.2 is estimated at 150k + 10k: 60 + 50 + 55 + 45 (+ ≈ 10 for the routing register) − 30 (three orientations) − 40 (build memory, spend ledgers and spec §10.6 read once). That is over the 95k bound, and the plan does not hide it: the operator chose the 3-run tail knowing the closure absorbs four former runs. Mitigation, pre-registered for its contract: the four modes are ordered stages, each committing its artifact (closure record, backlog + readiness record, reconciliation, integration plan) before the next starts, so a measured overflow (OL-16) stops at a mode boundary and a scoped `CAP6.2a` insert continues from the committed artifacts without re-reading them. The checker allows `operator-ruled` only on a tail capstone.

## Phase-4 review record (2026-11-07)

**Review:** independent fresh-context split review of the Phase 0–2 draft (findings SR-1…SR-27; verdict "repair: SR-1…SR-18, minors recommended"), dispositioned here under the coordinator's binding rulings on SR-1, SR-2, SR-15/16 and the general instruction to apply the reviewer's change unless it conflicts with the spec, an operator decision or a ruling. **Machine check after revision:** the improved independent checker passes; its output and negative controls are recorded in the [coverage register](2026-11-07_round6-coverage.md#machine-check).

| finding | disposition | what changed / why |
|---|---|---|
| SR-1 fallback unroutable | **applied (adapted to the ruling)** | GATE-G29 is a decision gate with dispositions `full` / `fallback`, either PASSED with its basis verbatim; GATE-G27/G28/G30 carry their own dispositions ([table](#disposition-gates-and-fallback-routing-sr-1)). R6.58 drops R6.57; GATE-G26 depends on HUMAN-H15, H16 and R6.57 and names the Q-X62 waiver. R6.57 is the spike alone and lands its rung record in every outcome; R6.58 writes the FP-S readiness record so the clock can start. Rows after G29 land under either disposition (R6.72/R6.73 branch; R6.75/R6.76 emit typed packets). R6.1a carries the fixtures. The reviewer's three-way G29 (`pass`/`fail`/`trigger_fired`) is folded into the two ruled dispositions with the basis carrying the distinction. The ruling's `withheld{fallback}` is the disposition name; the packets carry the spec's member `withheld{l1_not_passed}` (REQ-XH-123), so no second vocabulary item is minted. Item 5's contingency is adopted with a smaller permuted set, because T7 and T8 (b) now sit after W2 |
| SR-2 R6.1 dispatch | **applied per the ruling** | GATE-G20 authorizes R6.1 under `--no-checker` ROUTE + stock validator at 0 violations (GATE-G8 precedent); the 46 warnings are an OPEN FINDING owned by R6.1 whose AC requires the guard with the stock checker to return PASS (warnings classified into the reviewed baseline with dated provenance). The reviewer's "dispatch under verdict FAIL limited to 46 classes" is replaced by the ruling. **Owed outside this plan:** the invariants file (not owned here) should amend XO-02 to: "Exception: R6.1 dispatches under GATE-G20's scoped bootstrap authorization (`--no-checker` ROUTE plus the stock validator at 0 violations, GATE-G8 precedent); from R6.1a on, `PASS` with the stock checker is required." |
| SR-3 JSON shape | **applied, option (a)** | `source_sha256 = sha256(literal_row)` on every row; `literal_source_sha256` added; the 32 synthesis-allocated ids carry the spec's own row bytes; aliases carry their literal predicate in `requirement` (and their literal or labelled target acceptance); no null or empty field. Anchor scheme defined from Appendix A's hooks (`spec_anchor`, `ac_hook` + `ac_hook_resolution: appendix-a-row` pinned by `appendix_a_row_sha256`, `target_spec_row_sha256`, contract anchors). Schema id kept (`hh-supplement-plan/1`, additive fields); R6.1's scope names the per-source constants it parametrizes (pattern, counts, file), the widened hook pattern `AC-\d+(\.\d+)*-\d+` and Appendix-A resolution |
| SR-4 binary sets | **applied** | new R6.34c owns REQ-XC-330 (R6.34b becomes a contributor, with W1/W3 fleshing); shared-decision row added |
| SR-5 supervision seam | **applied (adapted)** | new R6.24c; R6.20a gains R6.15a; R6.34a depends on R6.24c. R6.24c owns REQ-XB-104 (moved from the runner topology row): its proxy/mediator sole-writer-root predicate cannot be tested before both servers exist; contributor to XC-101, XC-109, XH-103, XH-120 |
| SR-6 probes and legs | **applied** | new R6.38a (PS0 driver, legs `ps0`, `pp0`, `corpus`, `w4-main`); R6.45 gains the lane-probe driver and `w4-lane-probe` leg; contributors on XH-027, XD-041, XH-010 |
| SR-7 canonicalizer | **applied** | R6.41 moves before R6.22 (`Depends on: R6.1a`); home decided: a `hharness-wire` module, the only crate on REQ-XC-101's proxy dependency list that holds `Json::to_canonical_string`; a new leaf crate would add an edge outside that list |
| SR-8 compute quote, unit | **applied** | two shared-decision rows (formula quoted from §3.10, owner R6.25; µ$ unit + rounding, owner R6.13); R6.19a's `quote` emits labelled pre-quote quantities; R6.29 now depends on R6.25 |
| SR-9 `withheld{…}` vocabulary | **applied** | R6.15a owns the closed member set; R6.44 owns only projection formulas and continuation semantics |
| SR-10 E2-T6 | **applied** | R6.55 (XE-118) / R6.55a (XE-119) |
| SR-11 E2-T8 | **applied** | R6.57 (XE-120, rung record) / R6.57a (XE-121, XE-126, XD-102), R6.57a placed after W2 |
| SR-12 E2-T2 | **applied** | R6.52 (XE-103/104, owns the renderer) / R6.52a (XE-105/106, extends it) |
| SR-13 RUNNER-CORE-a | **applied (adapted)** | R6.15a / R6.15b (topology, XB-105) / R6.15c (scheduling); XB-104 goes to R6.24c (SR-5) rather than R6.15b |
| SR-14 GUARD-X size | **applied as a row split** | R6.1 (XH-001/002/003, 60k) / R6.1a (XH-004/008/024/025/026, 55k), both before R6.2 |
| SR-15 HUMAN placement | **adapted per the ruling** | the reconcile reads move to the pre-tail block (tradeoff 10) instead of "H14 before R6.58"; the false plan claims are corrected; the `DEFERRED` + DF fixture is in R6.1a; HUMAN-H17/H21 name `DECLINED` / `NOT_NEEDED`; the placement rule is machine-checked |
| SR-16 Wc unused | **applied** | GATE-G28 template `PASSED — not triggered; Wc unused`; R6.70 fleshed in place to `unused{not_triggered}`; R6.71 `n/a{wc_unused}`; HUMAN-H20 `N/A`; R6.1a fixture |
| SR-17 FP-S→G-L1 padding | **applied** | R6.66/R6.67 after GATE-G29; GATE-G27 depends on R6.65 (plus R6.56 and R6.34c) |
| SR-18 W1 upload | **applied** | listed in GATE-G20 as a deviation needing verbatim acceptance; CAP6.1 checks it as a deviation; R6.59 depends on GATE-G26, HUMAN-H16, R6.31b, R6.48; R6.60's AC refuses before `archive_verified`; R6.1a tolerates R6.59's position |
| SR-19 V5 declaration | **applied** | shared-decision row: R6.58 declares, R6.65 seals an equal level with a refusal test (R6.65 now depends on R6.58) |
| SR-20 project lifecycle, HUMAN exits | **applied in part** | project deletion is in the last window's fleshing scope (R6.73) and CAP6.3 checks `DELETE_REQUESTED`; HUMAN-H15 records `OLLAMA_API_KEY provided: yes`; HUMAN-H13 records `gcloud` auth `provided: yes`. **Rejected:** recording the billing-account id at HUMAN-H13 — REQ-XB-219 states billing-account ids "SHALL never enter a committed record"; H13 records `billing account provided: yes` instead |
| SR-21 operator-disk input | **applied** | R6.2 declares `~/tb-work/scratch-archive-2026-11-06/` and its manifest `~/tb-work/scratch-archive-2026-11-06.sha256` (sha256 `f80859977e5a276603f0cedfb2c04c47193bddf577c8d27e5c9f355c03a863e5`, 71 lines), with a presence check that refuses typed and never reconstructs the archive |
| SR-22 Q-X46 | **applied** | GATE-G20 presents A and B as the operator's decision; under A, HUMAN-H12, GATE-G22 and R6.12 move to just before HUMAN-H13; the B permutation is listed and machine-checked |
| SR-23 checker | **applied** | row-table ↔ JSON owner equality (duplicates and drops caught), graceful missing-id failures, verbatim-in-source literal check, SR-3 shape checks, HUMAN placement and shared-decision owner-first checks. Committing it is R6.1's job (it absorbs the checker under `docs/build/tools/` or as guard legs); this pass writes only the three planning files, so the register cites it by sha256 |
| SR-24 `Depends on` | **applied** | `nothing` for GATE-G20; edges R6.20a → R6.15a, R6.63 → R6.57a (the T8 half that the spec's E2-T8 → T9 edge needs), R6.42 → R6.35 and R6.24b, GATE-G27 → R6.65, R6.59 → HUMAN-H16, R6.31b, R6.48; R6.27 is `live-read` |
| SR-25 routing owner | **applied** | CAP6.1 owns the 34 routing records before GATE-G32; REC6.2 re-reads live; XD-215/XD-220 difference recorded (ambiguity 11) |
| SR-26 over-factoring | **applied** | R6.4 absorbs P1-D (60k + 10k); R6.5 retired |
| SR-27 fleshing mechanics | **applied** | [Skeleton fleshing mechanics](#skeleton-fleshing-mechanics-sr-27) |

**Remaining tradeoffs consciously recorded:** the row count above §10.6's band (tradeoff 11); the reconcile-placement interpretation of REQ-XH-008 and the W1 upload deviation, both for operator confirmation at GATE-G20; the Q-X62 contingency, which needs a scoped insert if T0…T6 stall past the trigger.

**Re-partition note (2026-11-07T04:49:42Z, operator Q).** The record above is kept verbatim; its ids are the pre-re-partition ids (the [merge map](#merge-map) maps each to its new row). The re-partition preserves every disposition's substance, as follows.

| finding | after the re-partition |
|---|---|
| SR-1 | **preserved.** Dispositions, the Q-X62 waiver and the rung record in every outcome are unchanged (rows renumbered: rung record R6.39, readiness record R6.33, branching rows R6.44–R6.48). The post-window record rows now branch inside their skeletons (`n/a{…}` upload records). The contingency's permuted set shrinks to {HUMAN-H14, GATE-G26, R6.40}, because FP-RECORD-PARTITION now rides with E2-T9 after W2 |
| SR-2 | **preserved, simplified.** GATE-G20 still authorizes R6.1 alone under the GATE-G8 precedent; R6.1 is now all of GUARD-X, so its AC requires `PASS` with `DISPATCH — R6.2`. The owed XO-02 wording becomes "from R6.2 on" |
| SR-3 | **preserved.** Register shape unchanged; only owner fields, contributors and routed statuses are regenerated |
| SR-4, SR-5, SR-6 | **preserved in content; changed as rows.** Each seam row is merged whole with its nearest shared-surface neighbour (binary sets → R6.23 with the upload kit; supervision → R6.16 with scheduling; PS0 + legs → R6.26 with the runner check), never split. REQ-XC-330 (R6.23, contributor R6.22) and REQ-XB-104 (R6.16, contributor R6.8) keep their seam-completing owners |
| SR-7 | **preserved.** K1 is a `hharness-wire` module landing before REPLAY (R6.10 before R6.12) |
| SR-8, SR-9 | **preserved.** Formula owner R6.14, µ$ unit owner R6.7; the closed `withheld{…}` set is R6.8's |
| SR-10, SR-11, SR-12 | **preserved.** E2-T6 (a)/(b), E2-T8 (a)/(b) and E2-T2 (a)/(b) stay in different rows (R6.38/R6.39, R6.39/R6.41, R6.35/R6.36) |
| SR-13, SR-14 | **changed (operator bound).** RUNNER-CORE (a)+(b) re-merge in R6.8 (85k) and GUARD-X in R6.1 (90k); both fit the new 95k bound, and their former halves are ordered stages (invariant 10) |
| SR-15, SR-16 | **preserved in rule; consolidated in rows.** The five reconcile rows become HUMAN-H18 immediately before the tail; the Wc-unused path is unchanged |
| SR-17 | **preserved.** K7 and F2-h stay after GATE-G29 (inside R6.46 and R6.49) |
| SR-18 | **preserved as a deviation.** ARCHIVE-UPLOAD-W1 moves from its own row into the W2 skeleton as its first step; GATE-G20 still lists it and CAP6.1 checks it as a deviation; R6.1's tolerance follows the new position |
| SR-19, SR-20, SR-21 | **preserved.** R6.33 declares and R6.43 seals; project deletion is in W4's fleshing and CAP6.2 checks `DELETE_REQUESTED`; H13/H14 exits `provided: yes`; R6.2 declares the operator-disk input |
| SR-22 | **changed.** Linearization B becomes B′ (six rows move; R6.3 and R6.5 stay ahead, because R6.5 carries both F2-b and F2-e); marker machine-checked |
| SR-23 | **preserved, extended.** Checker v3 adds the minimal-tail, 105k, operator-ruled, implement-spec band, record-row fold and B′ checks |
| SR-24 | **preserved.** Every edge remapped and re-checked backward-only |
| SR-25 | **changed (operator tail).** The routing register moves from CAP6.1 to CAP6.2, written and re-read live in one row before GATE-G32 |
| SR-26, SR-27 | **preserved, extended.** P1-C + P1-D now also carry P1-E (R6.3); each fleshing now also carries its window's upload and settlement acts |

### Tail split (operator decision 2026-11-07, coordinator-applied)

The re-partition's 3-run tail left the closure capstone CAP6.2 at an estimated 150k focused context, over the per-run bound (95k + 10k). Asked "The 3-run tail you chose leaves its closure capstone (CAP6.2) at ~150k context, over the ~105k safe ceiling for one SWE-2 run … How should we handle it?", the operator answered "Split into 4-run tail (Recommended)". Applied: CAP6.2 keeps the closure duties and all 39 of its owned ids (34 later-pass routing records + 5 closure ids) at 90k + 10k; a new CAP6.3 (seq 323, reconciliation capstone: spec reconciliation + integration plan, 75k + 10k, owns no ids) sits between CAP6.2 and the acceptance gate; GATE-G32 moves to 324 and depends on CAP6.3; DOC6.1 moves to 325. Tail kinds: capstone, capstone, capstone, gate, docs — the retained guard's declared `tail=minimal` contiguous match (capstone, capstone, gate, docs) holds within it. Totals: 74 rows; 49 implement-spec rows (45 tickets + 4 tail runs). No other row, owner or dependency changed.

### Contract consistency pass (2026-11-07, coordinator-applied)

After the 74 contracts were written, a fresh-context fixer resolved 23 logged cross-row items in the contracts only (no row order, dependency, owner or id changed; each edited contract carries a `> Consistency fix 2026-11-07` note). Plan-side consequences recorded here: (1) the closed `withheld{…}` set owned by R6.8 is the **complete** spec set (Appendix D "Dispositions"; §3.3; §10.1.4), superseding the seven-member shorthand in the shared-decision table above; (2) GATE-G22 and GATE-G31 gain a pre-registered non-stopping disposition — a declined publication is recorded PASSED with Appendix D's `declined` (R6.31 / R6.50 land the declined record; HUMAN-H17 records `N/A`), so declining to publish never stops the chain; (3) GATE-G29 routes "L1 failed on gate-A, no gate-B rerun, no trigger" to PROV-Wc (W3′ gate-B rerun) or, if Wc is declined or used, to `fallback`; (4) REQ-XC-322 contributors R6.22 and R6.46 are registered in the coverage companion.
