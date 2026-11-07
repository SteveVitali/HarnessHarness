# Round 6 — experiments first pass decomposition (partition plan)

**Status:** decomposition draft, revised after its independent Phase-4 split review (SR-1…SR-27) under the coordinator's binding rulings; written 2026-11-07T03:42:36Z, revised 2026-11-07T04:19:14Z. It carries Phases 0–2, the [Phase-4 review record](#phase-4-review-record-2026-11-07) and the Phase-5 coverage register. No ticket files, manifest rows or ledger edits are made here. Contract writers (Phase 3) follow. Nothing in this plan authorizes spend, provisioning, publication, external writes or merges, and it answers no gate.

**Invocation (frozen hand-off).** `decompose-spec spec=docs/experiments-spec.md build_name=experiments mode=extend tickets_dir=docs/tickets dispatch_target=subagent granularity=balanced tail=full`, per [the hand-off](../../experiments-decomposition-prompt.md) (sha256 `fd7fe273d559ec1e…`).

## Boundary and source identity

- **Spec:** [experiments supplement](../../experiments-spec.md), canonical v0.3, sha256 `0bcfb3572dcc0bc921014f265ee34876f2f74bad7b6cd685abe214ebcf78362e`. This equals the hand-off's digest. `req_id_pattern: REQ-X[A-H]-[0-9]+`; Appendix A lists 456 candidates (424 source + 32 synthesis-allocated), 370 adopted.
- **Base:** worktree branch `claude/experiments-spec` at `6bd6d99d`; `main` `185c16c7` is an ancestor. The manifest tail is still row 251 (`251_R5.59__efficacy-closure.md`), so Round 6 opens at **sequence 252**. Re-check at integration, because `main` moves.
- **State reads (mode=extend).** LEDGER CURRENT STATE: `round: 5`, `nextTicket: DONE`, `lastCompleted: R5.59`, `chainTip: codex/harnessharness-bl-reconcile`, `harness: devin-desktop/swe-2-high/subagent`. `bash scripts/deferrals-open.sh` printed no rows, which is a valid zero-open-rows result. Hand-off §4 gives the ledger values for the extension (`round: 6`, `nextTicket: GATE-G20`, `chainTip: main`, with `buildBranchBase` and `pinnedBaseSha` unchanged). The coordinator writes them, not this plan.
- **Guard mechanics honoured** (`docs/build/tools/supplement_guard.py` at `185c16c7`, sha256 `18ef79a8…`). Chain-id grammar; three-digit sequence filenames; kinds from the closed set; backward `Depends on` (written `nothing` for the bootstrap gate, never `—`); the contiguous `tail=full` block whose gate filename contains `accept`; landed = BUILD_INDEX id/seq (tickets, skeletons), a GATE DECISIONS row containing `PASSED` and neither `FAILED` nor `NOT PASSABLE` (gates), or a readout `Status:` other than `PENDING` (HUMAN). A pending gate or HUMAN row at the head of the chain is a HOLD; a recorded non-PASSED gate is `failed-gate` (FAIL); an unlanded skeleton at the head is `skeleton-dispatch` (REFUSED).

## Phase 0 — extracts

### Requirements by block (370 adopted; one primary owner each, see the [coverage register](2026-11-07_round6-coverage.md))

| block | adopted ids | rows | content |
|---|---|---|---|
| GUARD | 8 | R6.1, R6.1a | guard extension: register, routing and baseline (XH-001…003) ‖ gate/HUMAN/skeleton/window-pattern rules (XH-004, 008, 024…026) |
| P1 | 31 | R6.2–R6.4, R6.6 | phase-1 provenance, hardening run, corrections + recompute + AR-01/02/03, record + cards + checker (XA-006…125 less XA-126/127; XB-222; XH-019, 028, 032) |
| SITE | 53 | R6.7–R6.11, R6.32a/b, R6.33, R6.67 | identity, generator, pages, claims checker, Pages workflow, comparative/grid/phase-3/4 components (XF-*, plus XC-014, XC-120, XD-003, 092, 126, 222, XH-018, 020, 022, 119, 122) |
| PUB | 2 | R6.12, R6.78 | publication records after each publication gate (XF-136, XH-105) |
| INFRA | 104 | R6.13–R6.28, R6.31a/b | runner (core, topology, schedule, supervision), cells, deadlines, selection, compute recipe, proxy, budget, replay, verdict, egress, cost, reconcile, Harbor-O, re-proof machinery, archive |
| METHODS | 18 | R6.29, R6.30a/b, R6.40 | comparative protocol and quote, estimators, battery + method dossier, integrity scan |
| HOSTING | 32 | R6.34a–R6.39 | admission, two-stage build, carried binary sets, three first-pass harness renderers, capture, smoke battery, PS0 + live legs, runner check |
| L1 | 13 | R6.41–R6.43, R6.66 | canonicalizer (a `hharness-wire` module, before REPLAY), observation replay + corpus + census, L1 runner, L2 wiring |
| HIR | 31 | R6.49–R6.57a, R6.63–R6.65 | E2-T0…T10 gap closure (T2, T6, T8 split), the mini-swe-agent document, sweep glue, first-pass phase-4 level |
| FP | 22 | R6.44–R6.47, R6.58, R6.62, R6.72 | first-pass runner rules (continuation, holds, lane probe, seal), selection commit, pre-registration, partition, FP-E quote |
| W1 / W2 / W3 windows | 2 / 4 / 5 | R6.48, R6.60, R6.68, GATE-G26 | R-layer evidence that is a window's own record (re-proof, W2 stages, L1 gate run) plus FP-S readout fields |
| RESULTS | 6 | R6.75, R6.76, HUMAN-H23 | first-pass results packets, audit, L3, flagged-pass review |
| TAIL | 5 | CAP6.3, REC6.1, REC6.2 | $100 closure, per-kind spend, live reads, decision-register history |
| later pass (routing records) | 34 | CAP6.1 | Round 7: phase-4 explore → confirm, tranches, full re-proof, phase 1b (22). Round 8: Terminus-2 (12). Written before GATE-ACCEPT; REC6.2 re-reads it live (SR-25) |

Layers among the 370: E 257, D 80, R 32, P 1. The ids per row are in the row table below and, literally, in the coverage register.

### Acceptance and the cross-cutting invariants (every Round-6 row re-checks these)

1. **Spend ceiling** ⚑: the first pass costs ≤ $100 in all (Q-X47, Q-X50). Model ≤ $70, compute ≤ $20, and a $10 reserve that is never authorized (Q-X54). Purchased credit counts in full (Q-X63). No ticket authorizes spend; only the recorded gates (PROV-W1, FP-S, PROV-W3, PROV-Wc, FP-E) do, verbatim (REQ-XH-004, REQ-XH-110).
2. **Skeleton rule** ⚑: every W1–W4 and Wc run body is `Kind: skeleton`, with no Run line until its gate passes. A scoped `decompose-spec` insert then fleshes it in place (Q-X58, REQ-XH-001, REQ-XH-024; mechanics in [Decomposition decisions](#skeleton-fleshing-mechanics-sr-27)). A pending gate is RETURN PASS and is never pre-answered.
3. **Guard first** ⚑: GATE-G20, then GUARD-X (R6.1, R6.1a; REQ-XH-002, AR-31). No product ticket dispatches before both land.
4. **Firewall**: no gate or continuation rule reads an outcome field. First-pass agent-slot outcomes stay sealed until FP-RESULTS (REQ-XH-112, REQ-XH-121). Every cost-triggered drop is labelled "cost-triggered; cost correlates with failure length".
5. **Repository invariants** (root `AGENTS.md`): K-2 (no kernel semantics in `hh`), CC1 (one implementation per codec, store, class table and renderer), CC7 (schema → regenerate; never hand-edit generated files), CC10 (one home per kind), durable-before-visible, honest typed refusal (`Unsupported`/`Refused`/`n/a{reason}`/`withheld{…}`), no new dependency without an ADR, `provided: yes/no` only for secrets, append-only history, dispositions never relabelled, `mergePolicy: OPERATOR`, and build-memory writes by shell/Python with `git diff` verification.
6. **Evidence labels**: `live-executed` only through a registered `scripts/live-verify.sh` leg (REQ-XH-027, ADR-0363); the legs are built by R6.28 (`reproof`), R6.38a (`ps0`, `pp0`, `corpus`, `w4-main`) and R6.45 (`w4-lane-probe`). Fixture-verified and recorded-execution labels are kept distinct. No favourable effect is an acceptance criterion (§0.2).
7. **Namespace**: the Appendix D spellings (W1–W4/Wc, AG1–AG9, SM-*, PS0/PP0, V1–V9, ST0–ST7). Round-5 S1–S4 are never reused for program stages (REQ-XH-022, S-12).
8. **Requirement ids**: kept verbatim and append-only. Each adopted id has exactly one primary owner, and aliases keep their literal predicate bytes at the target (REQ-XH-003). The round-5 `REQ-[A-H]-[0-9]+` register is untouched.
9. **Universal phase-gate AC** (BM-TICKET-02) on every implement-spec row, plus the Round-5 operating clauses (dates from the clock, PR checks read and recorded, AC layers stated, protected records only appended, gate words verbatim, harness in the run-ledger header).

### Out of scope for Round 6 (every later-round item named by spec §10.6, §1.2 and §10.1.6)

- Later-pass phase 2: tranches TR1 → TR2 → TR3, tranche quotes and gates, the band census, the full shared pilot and pilot set P, R-live, and up to 8 harnesses (Hermes, OpenHands, Kimi Code and OpenCode adapters; Codex and a Responses surface, C-32).
- Later-pass phase 3: the full L2 at N ≈ 150–300 and G-L2. The **Terminus-2 stage** goes to Round 8: `lab_host` hosting (AR-19, REQ-XC-314, REQ-XC-316), E2-T12…T16, K10–K14 and AR-30.
- Later-pass phase 4: exploration X0/X1, frozen selection, held-out confirmation, G-P4-X/G-P4-C, D3-b/d…i, effort and compaction as factors (AR-26, AR-27), and `edit_file`.
- The full 87-task re-proof (it precedes any later-pass draw; REQ-XH-033). Phase 1b is out too (Q-X24 default: unscheduled; REQ-XA-126).
- The optional PS0 early gate (Q-X23 default: PS0 runs in W2 on the VM), vendored fonts (AR-29) and a custom domain (Q-X43 default: project URL).
- Discharging BL-50's P-layer legs. REQ-XA-114 corrects only the phrasing of its dated note.
- Any guard-enforced first-pass exception to the skeleton rule (Q-X58 option (b), an operator decision).

These 34 adopted ids keep one owner (CAP6.1), which writes their routing record before GATE-ACCEPT, with `round_scope: routed(round-7|round-8)`. No implementing row for them exists in Round 6, as REQ-XH-001 requires.

## Phase 1 — dependency DAG and coupling seams

### Window and gate sequence (FP-10 staged spending, as linearized; linearization A)

```text
GATE-G20 bootstrap ─► R6.1 GUARD-X (a) ─► R6.1a GUARD-X (b) ─► P1 (R6.2, R6.3, R6.4, R6.6) ─► F2-a ─► GATE-G21 mock ─► F2-b ─► F2-e ─► F2-f ─► F2-g
─► P2-infra (R6.13…R6.43; K1 before REPLAY; GATE-G23 after the D1-b dossier) ─► FP rules (R6.44…R6.46) ─► R6.47 SELECT commit + W1 quote
─► operator batch: HUMAN-H12 Pages ─► GATE-G22 pub-P1 ─► R6.12 PUB-P1 ─► HUMAN-H13 billing alert ─► GATE-G25 PROV-W1
─► [W1 skeleton] ─► R6.58 FP-PREREG (+ FP-S readiness record: the Q-X62 clock can start from here)
─► E2-T0…T6 (GATE-G24 AR-24 amendment between T4 and T5) ─► R6.57 E2-T8 (a): the C-05 rung record, landed in every outcome
─► operator batch: HUMAN-H15 Ollama settings, HUMAN-H16 HF dataset, HUMAN-H17 key (optional) ─► GATE-G26 FP-S
─► R6.59 ARCHIVE-UPLOAD-W1 (operator-accepted deviation) ─► [W2 skeleton] ─► R6.61 ARCHIVE-UPLOAD-W2 + SETTLE-W2 ─► R6.62 FP-RECORD-PARTITION
─► E2-T7 ─► E2-T8 (b) ─► E2-T9 ─► E2-T10 ─► D3-FP ─► GATE-G27 PROV-W3 {run | not_run} ─► [W3 skeleton] ─► R6.69 ARCHIVE-UPLOAD-W3
─► GATE-G28 PROV-Wc {triggered | not triggered} ─► [Wc skeleton] ─► R6.71 ARCHIVE-UPLOAD-Wc (or n/a{wc_unused})
─► GATE-G29 G-L1 {full | fallback} ─► K7 ─► F2-h ─► R6.72 FP-E quote (per disposition) ─► HUMAN-H21 credit purchase (only if needed) ─► GATE-G30 FP-E
─► [W4 skeleton: lane probe, rounds 1–2, automatic checkpoint, rounds 3–5, project deletion] ─► R6.74 ARCHIVE-UPLOAD-W4 + SETTLE-W4
─► R6.75 FP-RESULTS (P2-FP, P4-FP) ─► R6.76 FP-RESULTS (P3-L2, audit, L3) ─► HUMAN-H23 flagged review ─► R6.77 freeze
─► GATE-G31 pub-FP ─► HUMAN-H24 dataset flip ─► R6.78 PUB-FP
─► reconcile reads HUMAN-H14 (W1), H18 (W2), H19 (W3), H20 (Wc), H22 (W4) ─► tail (CAP6.1…3, GATE-G32 accept, REC6.1…3, DOC6.1…2)
```

The W4 checkpoint is automatic and pre-registered, so it gets no row (§10.3). The fallback (REQ-XH-123) is not a row: it is GATE-G29's pre-registered `fallback` disposition, and every row after G29 is written so that it lands honestly under either disposition ([Disposition gates and fallback routing](#disposition-gates-and-fallback-routing-sr-1)).

### P2-infra DAG (spec §10.2, with the splits applied; edges = "must land before")

```text
PRICE(R6.13) ∥ LANE(R6.14) ∥ RUNNER-CORE-a(R6.15a) ──► PROXY-CORE-a(R6.20a) ──► PROXY-CORE-b(R6.20b) ──► BUDGET(R6.21)
K1(R6.41) ∥ BUDGET ──► REPLAY(R6.22)
R6.15a ──► RUNNER-TOPOLOGY(R6.15b) ──► RUNNER-SCHEDULE(R6.15c);  R6.15a ──► CELL-SPEC(R6.16), DEADLINE(R6.17)
{R6.15a, R6.16} ──► VERDICT(R6.23) ──► EGRESS-a(R6.24a, also after R6.20a) ──► EGRESS-b(R6.24b) ──► HARBOR-O(R6.27) ──► REPROOF-KIT(R6.28)
{R6.15c, BUDGET, EGRESS-b} ──► RUNNER-SUPERVISION(R6.24c) ──► ADMIT-a(R6.34a, also after REPLAY)
RECIPE-a(R6.19a) ──► RECIPE-b(R6.19b) ──► COST-RECORD(R6.25, also after BUDGET) ──► RECONCILE(R6.26, also after REPLAY)
{SELECT(R6.18), COST-RECORD} ──► D1-a(R6.29) ──► D1-b-a(R6.30a) ──► D1-b-b(R6.30b) ──► GATE-G23 method admission
REPLAY ──► ARCHIVE-a(R6.31a) ──► ARCHIVE-b(R6.31b);   {F2-b, D1-b-a} ──► F2-c-a ──► F2-c-b;   {F2-b, ARCHIVE-a} ──► F2-d
ADMIT-a ──► ADMIT-b(R6.34b) ──► BINARY-SETS(R6.34c, also after ARCHIVE-a, RECIPE-b);   ADMIT-b ──► ADAPT-CHAT(R6.35) ──► ADAPT-MESSAGES(R6.36)
     ──► CAPTURE(R6.37) ──► SMOKE-BATTERY(R6.38) ──► {PS0+LEGS(R6.38a, also after REPROOF-KIT), RUNNER-CHECK(R6.39, also after HARBOR-O)}
{ADMIT-a, REPLAY} ──► D1-c answer-scan(R6.40);   {K1, REPLAY, ADAPT-CHAT, EGRESS-b} ──► K2+K3(R6.42) ──► K4(R6.43) ──► K7(R6.66, after G-L1)
{RUNNER-SCHEDULE, BUDGET} ──► FP-CONT(R6.44) ──► FP-HOLDS(R6.45) ──► FP-SEAL(R6.46, also after F2-b)
{SELECT, D1-a, GATE-G23, REPROOF-KIT, COST-RECORD} ──► FP-SELECT(R6.47) ──► … W1 ──► FP-PREREG(R6.58, also after GATE-G23)
E2-T0 ──► (T1 ∥ T3) ──► T2a ──► T2b ──► T4a ──► T4b ──► GATE-G24 ──► T5 ──► T6a (also after CELL-SPEC) ──► T6b
     ──► T8a rung record (also after RUNNER-SCHEDULE, REPLAY, RUNNER-SUPERVISION) ──► GATE-G26 FP-S (edge waived by a recorded Q-X62 trigger)
T6b ──► T7;   {T8a, K4, RUNNER-SCHEDULE} ──► T8b;   {W2 harvest, FP-SEAL} ──► FP-RECORD-PARTITION;   {FP-RECORD-PARTITION, T8b} ──► T9 ──► T10 ──► D3-FP (also after FP-PREREG)
```

### Coupling seams (where the cuts fall, and where they must not)

- **Cut on:** crate and binary boundaries: `hharness-runner`, `hh-model-proxy`, `hh-egress-mediator` / `hh-cell-relay`, `scripts/gce-backend.sh`, `scripts/site/`, `hh-eval` modules (`task-selection/1`, `comparative_protocol/1`), and the `hharness-wire` module `hh-l1-canon`. Also on schema owners (one schema, one row) and on the window boundaries, which are strict.
- **Named seam rows (SR-4…SR-6).** Three integrations that no single owner row covered now have their own row: R6.24c (runner ↔ proxy/mediator supervision, slot-token lifecycle, socket grant, carry-in), R6.34c (the two carried HH binary sets and the layer export/load/verify path) and R6.38a (the PS0 driver and the W2/W4 `live-verify.sh` legs). R6.45 builds the W4 lane-probe driver and its leg.
- **Do not cut:** the single compiled-surface renderer (R6.52 owns it; R6.52a extends it and never re-implements it, CC1), the receipt deposit ordering (it stays with PROXY-CORE-a, and release modes build on it), the slot record schema with its closed `withheld{…}` vocabulary (RUNNER-CORE-a), and the projection formulas p̂_h/t̂_h (FP-CONT, which FP-E's quote and the W4 checkpoint consume).
- **Out-of-chain:** none. Every row is in this repository. The Hugging Face dataset, GCP project and Ollama account are external accounts reached through HUMAN rows and gated execution acts, never through a stacked branch.

## Phase 2 — the Round-6 row table

Chain order, starting at 252. Each size budget is the Round-5 contract header form, `<n>k estimated focused context + 10k verification reserve; ≤<lines> changed lines (split trigger, not target)`, with *n* calibrated against Round-5's measured ×3 under-estimates. Every row stays at or below 85k including the reserve, inside the hand-off's 100–120k working-set bound (SWE-2 High, 256k, ≤ 40 %). The gate cell is `STOP` on every gate. The coordinator later writes the outcome tokens there (never this plan). HUMAN rows carry `—`, because they are skipped by kind. Ids keep their allocation from the reviewed draft; `R6.5` is retired (merged into R6.4, SR-26) and is never re-bound, and gate/HUMAN numbers are allocation order, not chain order.

| seq | file | chain id | kind | block | scope | spec §§ | primary REQ IDs owned | depends on | size budget | live stage | gate | alias |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 252 | `252_GATE-G20__bootstrap.md` | GATE-G20 | gate | B0 | Review the partition and coverage register; operator decides Q-X46 (A recommended, B offered), the W1 upload deviation and reconcile placement; authorizes only R6.1, routed by the retained guard in `--no-checker` mode with the stock validator run separately at 0 violations (GATE-G8 precedent) | §10.3, §10.6 | — | nothing | — (marker) | n/a | STOP | R6 plan review + bootstrap (B0) |
| 253 | `253_R6.1__guard-x.md` | R6.1 | ticket | GUARD | Extend `hh-supplement-guard/1`: second frozen source, REQ-X register legs over this JSON (Appendix A id source; § anchors + Appendix-A hook resolution), Round-6 routing, the 46 legacy `reqcov` warnings classified into the reviewed baseline (OPEN FINDING), coverage checker absorbed (AR-31) | §10.6, §0.2 | 3: XH-001, XH-002, XH-003 | GATE-G20 | 60k + 10k; ≤900 lines | offline-only | — | GUARD-X (a) |
| 254 | `254_R6.1a__guard-rules.md` | R6.1a | ticket | GUARD | Guard rules: verbatim gate markers and disposition-gate routing (GATE-G27…G30 tokens), HUMAN readout tokens, skeleton + fleshing-in-place, window pattern (REQ-XH-008), paid-row gate refusal | §10.3, §3.9 | 5: XH-004, XH-008, XH-024, XH-025, XH-026 | R6.1 | 55k + 10k; ≤800 lines | offline-only | — | GUARD-X (b) |
| 255 | `255_R6.2__p1-provenance.md` | R6.2 | ticket | P1 | Commit the scratch archive (operator-disk input, manifest sha256 pinned; typed refusal if absent), raw records, content digests and `p1_records.py` with its battery | §2.2 | 6: XA-101, XA-102, XA-103, XA-104, XA-105, XA-106 | R6.1a | 40k + 10k; ≤700 lines | offline-only | — | P1-A |
| 256 | `256_R6.3__p1-hardening.md` | R6.3 | ticket | P1 | Records-in cross-check (16/14/2; 26/30) and estimator ladders as committed packets | §2.3 | 9: XA-006, XA-107, XA-108, XA-109, XA-110, XA-111, XA-112, XA-113, XH-032 | R6.2 | 55k + 10k; ≤800 lines | offline-only | — | P1-B |
| 257 | `257_R6.4__p1-corrections.md` | R6.4 | ticket | P1 | Dated corrections, S4 deviation record, AR-01 (+AR-02/AR-03 records; Q-X25 default) and the dated study recompute folding DF-CAP5.1-1; R5.59 bytes unchanged | §2.4, §10.5 | 6: XA-114, XA-115, XA-116, XA-117, XB-222, XH-028 | R6.2 | 60k + 10k; ≤800 lines | offline-only | — | P1-C + P1-D |
| 258 | `258_R6.6__p1-record.md` | R6.6 | ticket | P1 | Phase-1 record, bounds block, `[replay-record]` card, README spine, `record` leg (AR-32) | §2.5–§2.7, §9.6 | 10: XA-008, XA-118, XA-119, XA-120, XA-121, XA-122, XA-123, XA-124, XA-125, XH-019 | R6.3, R6.4 | 60k + 10k; ≤900 lines | offline-only | — | P1-E |
| 259 | `259_R6.7__site-shell.md` | R6.7 | ticket | SITE | Instrument design system, tokens test, glyph rasterizer, shell and all-absent site | §9.1, §9.2 | 8: XF-101, XF-102, XF-103, XF-104, XF-105, XF-106, XF-107, XF-114 | R6.1a | 60k + 10k; ≤1000 lines | offline-only | — | F2-a |
| 260 | `260_GATE-G21__mock-review.md` | GATE-G21 | gate | SITE | Operator views the rendered mock (both themes, four viewports); approve "Instrument" or redirect (Q-X43) | §9.1, §10.3 | — | R6.7 | — (marker) | n/a | STOP | site identity mock review |
| 261 | `261_R6.8__site-generator.md` | R6.8 | ticket | SITE | Stdlib generator core, `hh-site-data/1`, named inputs, derivations D-1…D-8, drift | §9.3 | 9: XF-021, XF-117, XF-118, XF-119, XF-120, XF-121, XF-122, XF-125, XF-139 | GATE-G21 | 65k + 10k; ≤1000 lines | offline-only | — | F2-b |
| 262 | `262_R6.9__site-phase1.md` | R6.9 | ticket | SITE | Methodology, honesty, spend, limits, changelog, data pages and phase-1 content | §9.2, §9.3 | 5: XF-108, XF-109, XF-116, XF-141, XH-020 | R6.8, R6.6 | 55k + 10k; ≤900 lines | offline-only | — | F2-e |
| 263 | `263_R6.10__claims-checker.md` | R6.10 | ticket | SITE | `claims-lexicon.json` (C-25 byte-identical refactor), `check-site.sh` legs, mutation battery, ci step | §9.4, §9.5 | 11: XF-009, XF-126, XF-127, XF-134, XF-135, XF-137, XF-138, XH-022, XD-003, XD-092, XC-014 | R6.9 | 70k + 10k; ≤1100 lines | offline-only | — | F2-f |
| 264 | `264_R6.11__pages-workflow.md` | R6.11 | ticket | SITE | `pages.yml` (AR-28), `hh-site-publication/1` validator, public-profile refusals | §9.6 | 7: XF-128, XF-129, XF-130, XF-140, XF-142, XH-018, XC-120 | R6.10 | 45k + 10k; ≤600 lines | offline-only | — | F2-g |
| 265 | `265_R6.13__price.md` | R6.13 | ticket | INFRA | `PricingTable.rate_unit`; the integer-µ$ money unit and the one rounding rule (shared owner); `NoPrice{unrepresentable_rate}` (AR-09) | §4.5 | 1: XC-116 | R6.1a | 30k + 10k; ≤400 lines | offline-only | — | PRICE |
| 266 | `266_R6.14__lane.md` | R6.14 | ticket | INFRA | Ollama lane dialects, raw forward leg, auth from `auth_kinds` (AR-11) | §4.1, §4.2 | 1: XC-104 | R6.1a | 50k + 10k; ≤800 lines | offline-only | — | LANE |
| 267 | `267_R6.15a__runner-core.md` | R6.15a | ticket | INFRA | `hharness-runner` crate, engine drive, K-2 edge check, `hh-experiment-slot/1` with the closed `withheld{…}` member set, outcome map (AR-04) | §3.2, §3.3 | 5: XB-101, XB-102, XB-103, XB-108, XB-120 | R6.1a | 55k + 10k; ≤900 lines | offline-only | — | RUNNER-CORE (a) |
| 268 | `268_R6.15b__runner-topology.md` | R6.15b | ticket | INFRA | Process-topology spike, process-level concurrency, two-process kill-point battery, durable-only resume | §3.2 | 1: XB-105 | R6.15a | 50k + 10k; ≤800 lines | offline-only | — | RUNNER-CORE (b) |
| 269 | `269_R6.15c__runner-schedule.md` | R6.15c | ticket | INFRA | Packing, schedule kinds (k-round, pair-coupled), reattempt policy, admission cut-off, C rule | §3.3, §3.9, §3.10, §7.5 | 9: XB-121, XB-127, XB-128, XB-214, XD-060, XD-111, XH-016, XH-017, XH-125 | R6.15b | 60k + 10k; ≤900 lines | offline-only | — | RUNNER-CORE (c) |
| 270 | `270_R6.16__cell-spec.md` | R6.16 | ticket | INFRA | Helper WORKDIR, cpu/memory limits, kernel-mediated copy-in/out, digest-only images (AR-06) | §3.3, §3.1 | 3: XB-106, XB-107, XB-117 | R6.15a | 45k + 10k; ≤600 lines | offline-only | — | CELL-SPEC |
| 271 | `271_R6.17__deadline.md` | R6.17 | ticket | INFRA | Task-declared deadlines above 900 s; legacy path byte-identical (AR-07) | §3.6, §3.3 | 2: XB-118, XB-119 | R6.15a | 40k + 10k; ≤500 lines | offline-only | — | DEADLINE |
| 272 | `272_R6.18__select.md` | R6.18 | ticket | INFRA | `task-selection/1` materializer in hh-eval; seeded split; outcome-field refusals | §3.1, §6.2, §6.12 | 3: XB-122, XD-011, XH-012 | R6.1a | 45k + 10k; ≤700 lines | offline-only | — | SELECT |
| 273 | `273_R6.19a__recipe-provision.md` | R6.19a | ticket | INFRA | `gce-backend.sh` quote (declaration quantities only, labelled `pre-quote`) / provision / verify, declaration schema, receipts, fake-shim battery (AR-15) | §3.9, §3.11 | 10: XB-201, XB-202, XB-203, XB-204, XB-205, XB-206, XB-208, XB-209, XB-212, XB-223 | R6.1a | 60k + 10k; ≤1000 lines | offline-only | — | RECIPE-a |
| 274 | `274_R6.19b__recipe-teardown.md` | R6.19b | ticket | INFRA | Teardown nine-step, inventory, `compute-open.sh`, prepull, harvest, restart-only, deliver-secret | §3.9, §3.11 | 8: XB-207, XB-210, XB-211, XB-219, XB-220, XB-224, XH-009, XH-023 | R6.19a | 55k + 10k; ≤900 lines | offline-only | — | RECIPE-b |
| 275 | `275_R6.20a__proxy-core.md` | R6.20a | ticket | INFRA | `hh-model-proxy` server, Json control channel, token gate, surfaces, field policy, `GenParamPolicy`, durable receipts (AR-08, AR-10) | §4.1–§4.3 | 9: XC-007, XC-101, XC-103, XC-105, XC-106, XC-107, XC-108, XC-109, XC-208 | R6.13, R6.14, R6.15a | 70k + 10k; ≤1100 lines | offline-only | — | PROXY-CORE-a |
| 276 | `276_R6.20b__proxy-release.md` | R6.20b | ticket | INFRA | Release modes, non-stream deadline, failure taxonomy, crash recovery, catalog identity stop | §4.2, §4.6, §4.8, §5.3 | 6: XC-110, XC-111, XC-119, XC-123, XC-124, XC-322 | R6.20a | 60k + 10k; ≤900 lines | offline-only | — | PROXY-CORE-b |
| 277 | `277_R6.21__budget.md` | R6.21 | ticket | INFRA | Root cap A_eff×(1−m), slices, worst-case reservation, ≤3 concurrency, carried root pool | §4.4, §10.1.1 | 9: XC-004, XC-112, XC-113, XC-114, XC-115, XC-126, XC-210, XH-005, XH-120 | R6.20b | 55k + 10k; ≤800 lines | offline-only | — | BUDGET |
| 278 | `278_R6.41__l1-canon.md` | R6.41 | ticket | L1 | `hh-l1-canon/1` rules C01–C07 as a `hharness-wire` module over `Json::to_canonical_string` (the proxy-admissible home), golden + mutation tests | §7.4 | 1: XD-105 | R6.1a | 40k + 10k; ≤600 lines | offline-only | — | K1 |
| 279 | `279_R6.22__replay.md` | R6.22 | ticket | INFRA | Proxy record/replay modes, `RequestCanonicalizer` (identity, json_canonical, `hh-l1-canon`), `RecordedTransport` | §4.9 | 2: XC-122, XC-125 | R6.21, R6.41 | 50k + 10k; ≤800 lines | offline-only | — | REPLAY |
| 280 | `280_R6.23__verdict.md` | R6.23 | ticket | INFRA | `harbor_reward_file/1`, `tb10_pytest_log/1`, bootstrap classifier, regrade bound, ANSI/workdir | §3.4, §2.3 | 5: XB-109, XB-110, XB-111, XB-112, XA-127 | R6.15a, R6.16 | 45k + 10k; ≤700 lines | offline-only | — | VERDICT |
| 281 | `281_R6.24a__egress-profile.md` | R6.24a | ticket | INFRA | `allowlist_mediated` admission, model socket, static-musl `hh-cell-relay`, residuals (AR-05) | §3.5, §3.3 | 3: XB-113, XB-115, XB-129 | R6.23, R6.20a | 60k + 10k; ≤900 lines | offline-only | — | EGRESS (a; absorbs C2 EGRESS) |
| 282 | `282_R6.24b__egress-mediator.md` | R6.24b | ticket | INFRA | `hh-egress-mediator` CONNECT tunnel, decide-before-byte ordering, phase swap, allowlist freeze record | §3.5 | 2: XB-114, XB-116 | R6.24a | 60k + 10k; ≤900 lines | offline-only | — | EGRESS (b) |
| 283 | `283_R6.24c__runner-supervision.md` | R6.24c | ticket | INFRA | Runner supervision: launch proxy + mediator over Json control, sole-writer store roots, per-slot token mint/revoke, cell socket grant, root-pool carry-in from harvested ledgers | §3.2, §4.1, §4.3, §10.1.1 | 1: XB-104 | R6.15c, R6.21, R6.24b | 55k + 10k; ≤800 lines | offline-only | — | RUNNER-SUPERVISION (seam row) |
| 284 | `284_R6.25__cost-record.md` | R6.25 | ticket | INFRA | `hh-compute-cost/1`, `hh-live-pricing/2` compute rows, phase spend ledger, the §3.10 ceiling formula in µ$ (sole compute pricing; AR-14) | §3.10, §10.1.5 | 6: XB-213, XB-215, XB-216, XB-217, XH-006, XH-118 | R6.19b, R6.21 | 50k + 10k; ≤800 lines | offline-only | — | COST-RECORD |
| 285 | `285_R6.26__reconcile.md` | R6.26 | ticket | INFRA | Telemetry poller, settlement, `evaluate_admitted_costs`, `hh-slot-costs/1` fold (AR-16) | §4.5, §4.7 | 5: XC-006, XC-117, XC-118, XC-121, XC-127 | R6.25, R6.22 | 60k + 10k; ≤900 lines | offline-only | — | RECONCILE |
| 286 | `286_R6.27__harbor-o.md` | R6.27 | ticket | INFRA | Harbor pin (Q-X41 rule at dispatch, S-4), job-config writer, read-only ingest, image-drift, host venv | §3.7, §5.9, §3.1, §5.5 | 5: XB-123, XB-124, XC-306, XC-315, XH-014 | R6.24b | 55k + 10k; ≤800 lines | live-read | — | HARBOR-O |
| 287 | `287_R6.28__reproof-kit.md` | R6.28 | ticket | INFRA | Re-proof protocol (observe → freeze → prove; O/H-pub/H-allow), discordance register, packet label, `reproof` live-verify leg | §3.8, §10.3 | 2: XB-126, XH-027 | R6.27 | 55k + 10k; ≤800 lines | offline-only | — | REPROOF machinery (B2 #8 code half) |
| 288 | `288_R6.29__comparative-protocol.md` | R6.29 | ticket | METHODS | `comparative_protocol/1`, `comparative-quote/1` (compute lines priced by R6.25), support gate and envelopes (AR-12, AR-13) | §6.3, §6.7, §6.1, §4.6 | 8: XC-010, XD-002, XD-004, XD-020, XD-021, XD-050, XD-051, XH-030 | R6.18, R6.25 | 65k + 10k; ≤1000 lines | offline-only | — | D1-a |
| 289 | `289_R6.30a__estimators.md` | R6.30a | ticket | METHODS | Single-arm FM rate, flips, $/solve, tied-with-leader route, Holm family, `hh-comparative-results/1` (C-33) | §6.4, §6.5, §6.8, §6.11, §9.3 | 6: XD-001, XD-022, XD-030, XD-031, XD-061, XF-123 | R6.29 | 65k + 10k; ≤1000 lines | offline-only | — | D1-b (a) |
| 290 | `290_R6.30b__method-dossier.md` | R6.30b | ticket | METHODS | Comparative battery incl. first-pass T = 10 / H = 3 / H = 2 grid; review record; AR-17, AR-33 dossier | §6.4, §10.1.3, §10.5 | 2: XD-023, XD-024 | R6.30a | 60k + 10k; ≤800 lines | offline-only | — | D1-b (b) |
| 291 | `291_GATE-G23__method-admission.md` | GATE-G23 | gate | METHODS | Admit or refuse the protocol, quote, estimators, schedule randomization and AR-33 departures | §6.4, §10.3 | — | R6.30b | — (marker) | n/a | STOP | GATE method-admission |
| 292 | `292_R6.31a__archive-index.md` | R6.31a | ticket | INFRA | `hh-receipt-index/1`, condenser, pass-level artifacts section, fold recompute (AR-20) | §4.10 | 2: XF-124, XH-101 | R6.22 | 55k + 10k; ≤800 lines | offline-only | — | ARCHIVE (a) |
| 293 | `293_R6.31b__archive-upload-kit.md` | R6.31b | ticket | INFRA | Upload-tool admission, secret + live-credential scan, read-back verify, remediation procedure | §4.10, §3.11 | 4: XH-102, XH-103, XH-104, XH-107 | R6.31a | 55k + 10k; ≤800 lines | offline-only | — | ARCHIVE (b) |
| 294 | `294_R6.32a__site-leaderboard.md` | R6.32a | ticket | SITE | Leaderboard and frontier components over `hh-comparative-results/1` fixtures | §9.2 | 3: XF-110, XF-111, XF-115 | R6.8, R6.30a | 55k + 10k; ≤900 lines | offline-only | — | F2-c (a) |
| 295 | `295_R6.32b__site-noise-quote.md` | R6.32b | ticket | SITE | Noise, pilot/smoke, quote and refusal views over fixtures | §9.2 | 2: XF-112, XF-113 | R6.32a | 45k + 10k; ≤700 lines | offline-only | — | F2-c (b) |
| 296 | `296_R6.33__site-grid-viewer.md` | R6.33 | ticket | SITE | Per-task grid, static trial pages, gated transcript viewer (XSS, sha-mismatch) | §9.2 | 3: XF-131, XF-132, XF-133 | R6.8, R6.31a | 60k + 10k; ≤900 lines | offline-only | — | F2-d |
| 297 | `297_R6.34a__admit-record.md` | R6.34a | ticket | HOSTING | `hh-harness-admission/1` record + validator (AG1–AG9), terms, versions, declarations (AR-18) | §5.1, §5.3 | 5: XC-202, XC-214, XC-301, XC-302, XD-081 | R6.24c, R6.22 | 60k + 10k; ≤900 lines | offline-only | — | ADMIT-CORE (a) |
| 298 | `298_R6.34b__admit-build.md` | R6.34b | ticket | HOSTING | Two-stage bundle + per-task layer build, ST2 verify-only | §5.2 | 3: XC-303, XC-304, XC-305 | R6.34a | 55k + 10k; ≤900 lines | offline-only | — | ADMIT-CORE (b) |
| 299 | `299_R6.34c__binary-sets.md` | R6.34c | ticket | HOSTING | Carried artifacts: infrastructure and H binary sets plus layers and bundles — pinned-container build recipe, export with sha256 into `artifacts`, IAP load path, image-ID/binary digest verify, slot-identity digests | §5.2, §4.10, §10.1.4 | 1: XC-330 | R6.34b, R6.31a, R6.19b | 55k + 10k; ≤800 lines | offline-only | — | BINARY-SETS (seam row) |
| 300 | `300_R6.35__adapt-chat.md` | R6.35 | ticket | HOSTING | Golden renderers for original mini-swe-agent and Pi; closed env; effort; instruction pre-check | §5.3, §7.1, §6.10 | 8: XC-307, XC-308, XC-309, XC-310, XC-320, XE-001, XD-082, XD-121 | R6.34b | 55k + 10k; ≤800 lines | offline-only | — | ADAPT-CHAT-PY + ADAPT-CHAT-NODE (first-pass harnesses) |
| 301 | `301_R6.36__adapt-messages.md` | R6.36 | ticket | HOSTING | Claude Code renderer and Q-X26 admission terms (binary sha256, no OAuth/key) | §5.4, §5.3 | 1: XC-327 | R6.35 | 45k + 10k; ≤600 lines | offline-only | — | ADAPT-MESSAGES |
| 302 | `302_R6.37__capture.md` | R6.37 | ticket | HOSTING | `/logs/agent` copy-out, post-hoc ATIF, integrity bundle, receipt reconciliation, self-modification flag | §5.6, §4.3 | 4: XC-207, XC-317, XC-318, XC-328 | R6.36 | 50k + 10k; ≤700 lines | offline-only | — | CAPTURE |
| 303 | `303_R6.38__smoke-battery.md` | R6.38 | ticket | HOSTING | SM-pos/429/402/slow/hang scripted recordings, offline checker, hosting-failure classifier | §5.7, §5.8, §5.3, §5.1 | 7: XC-203, XC-311, XC-312, XC-321, XC-323, XC-324, XC-329 | R6.37 | 60k + 10k; ≤900 lines | offline-only | — | SMOKE-BATTERY |
| 304 | `304_R6.38a__ps0-and-legs.md` | R6.38a | ticket | HOSTING | PS0 probe driver (≤ $0.05 plan: one call per dialect + seed, `tool_choice`, `cache_control`) and `live-verify.sh` legs `ps0`, `pp0`, `corpus`, `w4-main` | §6.6, §10.1.4, §10.3 | — | R6.38, R6.28 | 45k + 10k; ≤700 lines | offline-only | — | PS0 + LEGS (seam row) |
| 305 | `305_R6.39__runner-check.md` | R6.39 | ticket | HOSTING | O-surface routing via host relay, R-replay comparator, `hh-runner-check/1` (R-live dropped) | §5.7, §7.5 | 3: XC-325, XC-326, XH-031 | R6.38, R6.27 | 50k + 10k; ≤700 lines | offline-only | — | RUNNER-CHECK |
| 306 | `306_R6.40__answer-scan.md` | R6.40 | ticket | METHODS | `answer-scan/1` integrity scanner, request-fingerprint check, review-record schema | §6.9, §5.3 | 2: XD-070, XC-313 | R6.34a, R6.22 | 45k + 10k; ≤700 lines | offline-only | — | D1-c |
| 307 | `307_R6.42__l1-replay-corpus.md` | R6.42 | ticket | L1 | Observation-replay executor (original run in a cell), synthetic fixtures M1–M19, turn-1 census generator, recorded-response driver | §7.4, §7.1 | 1: XE-004 | R6.41, R6.22, R6.35, R6.24b | 55k + 10k; ≤900 lines | offline-only | — | K2 + K3 |
| 308 | `308_R6.43__l1-runner.md` | R6.43 | ticket | L1 | L1 runner: L1.0/L1.1/L1.2 legs, stable prefix, `hh-l1-diff/1`, `hh-l1-conformance/1` | §7.4 | 3: XD-103, XD-104, XD-109 | R6.42 | 65k + 10k; ≤1000 lines | offline-only | — | K4 |
| 309 | `309_R6.44__fp-continuation.md` | R6.44 | ticket | FP | Cost-only continuation, projection formulas, per-arm caps (expected-cost admission), wall-time rule, stop rules | §10.1.4, §6.6 | 5: XH-112, XH-117, XH-126, XD-040, XD-042 | R6.15c, R6.21 | 60k + 10k; ≤900 lines | offline-only | — | FP runner rules (REQ-XH-112/117/126) |
| 310 | `310_R6.45__fp-holds.md` | R6.45 | ticket | FP | Hosted-only fallback schedule; HH-native lane-probe driver, `w4-lane-probe` leg and hold; PP0 hold dispositions | §10.1.4, §10.1.2 | 3: XH-123, XH-124, XH-127 | R6.44 | 55k + 10k; ≤800 lines | offline-only | — | FP holds (REQ-XH-123/124/127) |
| 311 | `311_R6.46__fp-seal.md` | R6.46 | ticket | FP | Outcome seal: sealed projections for readouts, checkpoint, dev partition and internal site profile | §10.1.4 | 1: XH-121 | R6.45, R6.8 | 50k + 10k; ≤700 lines | offline-only | — | FP seal (REQ-XH-121) |
| 312 | `312_R6.47__fp-select.md` | R6.47 | ticket | FP | Commit first-pass split, draw, within-cell replacement order and dev list; W1 declaration + quote | §10.1.2, §6.2, §3.10 | 3: XD-010, XD-012, XD-014 | R6.18, R6.29, GATE-G23, R6.28, R6.25 | 40k + 10k; ≤400 lines | offline-only | — | SELECT commit (D1-d part 1) |
| 313 | `313_HUMAN-H12__pages-enable.md` | HUMAN-H12 | human | SITE | Enable GitHub Pages (Source: GitHub Actions) | §10.3 | — | R6.11 | — (marker) | n/a | — | HUMAN Pages |
| 314 | `314_GATE-G22__pub-p1.md` | GATE-G22 | gate | PUB | Phase-1 publication: `hh-site-publication/1` pinning the phase-1 input set; labels; card transition | §9.6, §10.3 | — | HUMAN-H12 | — (marker) | n/a | STOP | GATE pub-P1 |
| 315 | `315_R6.12__pub-p1-record.md` | R6.12 | ticket | PUB | Commit the phase-1 publication record; README site link; `[results-site]` transition; deploy read | §9.6 | 1: XF-136 | GATE-G22 | 25k + 10k; ≤200 lines | operator-gated:GATE-G22 | — | PUB-P1 |
| 316 | `316_HUMAN-H13__billing-alert.md` | HUMAN-H13 | human | W1 | GCP billing alert (default $20; 50/90/100 %); billing account and controller `gcloud` auth `provided: yes`; recipients `provided: yes` | §10.3 | — | R6.47 | — (marker) | n/a | — | HUMAN billing-alert |
| 317 | `317_GATE-G25__prov-w1.md` | GATE-G25 | gate | W1 | PROV-W1: compute quote, dead-man + restart, network posture (Q-X55), re-proof scope, export estimate, Q-X39b | §10.3, §10.1.4 | — | HUMAN-H13 | — (marker) | n/a | STOP | GATE PROV-W1 |
| 318 | `318_R6.48__w1-run.md` | R6.48 | skeleton | W1 | W1 window body: infrastructure-set + bundle + layer builds and export, observe/freeze/prove re-proof, census, smokes, R-replay, teardown | §10.1.4, §3.8, §3.9 | 2: XB-125, XH-116 | GATE-G25 | — (fleshed after gate) | operator-gated:W1 | — | W1{PROV … TEARDOWN} (REPROOF, HOST-SMOKE, CENSUS) |
| 319 | `319_R6.58__fp-prereg.md` | R6.58 | ticket | FP | Post-replacement list frozen; pre-registration packet hashed (V5 declaration, fallback trigger); W2 quote; FP-S readiness record (Q-X62 clock) | §10.1.2, §10.1.3, §7.5 | 4: XH-111, XH-113, XH-115, XD-115 | R6.47, R6.48, GATE-G23 | 55k + 10k; ≤700 lines | offline-only | — | FP-PREREG (D1-d part 2) |
| 320 | `320_R6.49__e2-adr-bundle.md` | R6.49 | ticket | HIR | Docs-only ADR bundle AR-21, AR-22, AR-23 and the AR-24 §5e.1 amendment text | §7.3, §10.5 | — | R6.1a | 45k + 10k; ≤600 lines | offline-only | — | E2-T0 |
| 321 | `321_R6.50__wire-fidelity.md` | R6.50 | ticket | HIR | Model-port wire fidelity: non-stream transport, `tool_choice` omit, provider ids, verbatim args, replay | §7.3 | 2: XE-107, XE-108 | R6.49 | 55k + 10k; ≤800 lines | offline-only | — | E2-T1 |
| 322 | `322_R6.51__hh-tmpl.md` | R6.51 | ticket | HIR | `hh-tmpl/1` engine, strict undefined, whitespace control, ledgered uname probe | §7.3 | 2: XE-112, XE-113 | R6.49 | 50k + 10k; ≤800 lines | offline-only | — | E2-T3 |
| 323 | `323_R6.52__compiled-surface.md` | R6.52 | ticket | HIR | Model port renders the compiled `ModelSurface` (the single renderer): render path, sections, `system` role, byte-identical legacy | §7.3 | 2: XE-103, XE-104 | R6.50, R6.51 | 60k + 10k; ≤800 lines | offline-only | — | E2-T2 (a) |
| 324 | `324_R6.52a__surface-tools.md` | R6.52a | ticket | HIR | Extends R6.52's renderer: `tools[]` verbatim, completion surface, `tool_choice` only when declared | §7.3 | 2: XE-105, XE-106 | R6.52 | 50k + 10k; ≤700 lines | offline-only | — | E2-T2 (b) |
| 325 | `325_R6.53a__typed-renderers.md` | R6.53a | ticket | HIR | `json_head_tail@1`, `text_head_tail@1`, `error_format` classes | §7.3 | 2: XE-110, XE-111 | R6.52a | 50k + 10k; ≤800 lines | offline-only | — | E2-T4 (a) |
| 326 | `326_R6.53b__observation-plumbing.md` | R6.53b | ticket | HIR | G18: durable model-view rendering of exec observations, replay re-serves it | §7.3 | 1: XE-109 | R6.53a | 60k + 10k; ≤800 lines | offline-only | — | E2-T4 (b) |
| 327 | `327_GATE-G24__ar24-amendment.md` | GATE-G24 | gate | HIR | AR-24 canonical amendment `stop_rule.submit{marker, line, require_returncode}`: approve or refuse | §7.3, §10.3, §10.5 | — | R6.53b | — (marker) | n/a | STOP | GATE AR-24 amendment |
| 328 | `328_R6.54__strategy-submit.md` | R6.54 | ticket | HIR | Strategy params from slots, submit marker detection, format-error semantics, retry policy | §7.3 | 3: XE-115, XE-116, XE-117 | GATE-G24 | 60k + 10k; ≤900 lines | offline-only | — | E2-T5 |
| 329 | `329_R6.55__exec-embed.md` | R6.55 | ticket | HIR | `open_session.exec` member (AR-25), regenerate (CC7), `bind`-checked arming, refusal test | §7.3 | 1: XE-118 | R6.54, R6.16 | 50k + 10k; ≤700 lines | offline-only | — | E2-T6 (a) |
| 330 | `330_R6.55a__exec-defaults.md` | R6.55a | ticket | HIR | `exec_defaults`, helper merged stdio, in-cell process-tree kill, `returncode = -1` rendering | §7.3 | 1: XE-119 | R6.55 | 50k + 10k; ≤700 lines | offline-only | — | E2-T6 (b) |
| 331 | `331_R6.57__native-topology.md` | R6.57 | ticket | HIR | HH-native topology spike: two-process ownership, `env.close{detach}` + re-attach; the C-05 rung record lands at the rung reached, in every outcome | §7.3, §7.2 | 1: XE-120 | R6.55a, R6.15c, R6.22, R6.24c | 55k + 10k; ≤800 lines | offline-only | — | E2-T8 (a; rung record) |
| 332 | `332_HUMAN-H15__ollama-settings.md` | HUMAN-H15 | human | W2 | Ollama: automatic usage billing off (or meter-only recorded), exclusivity per window; `OLLAMA_API_KEY provided: yes` in `HH_OPERATOR_ENV_FILE` | §10.3 | — | R6.58 | — (marker) | n/a | — | HUMAN Ollama-settings |
| 333 | `333_HUMAN-H16__hf-dataset.md` | HUMAN-H16 | human | W2 | HF dataset repo (private) + fine-grained write token, `provided: yes` | §10.3, §4.10 | — | R6.58 | — (marker) | n/a | — | HUMAN HF-dataset |
| 334 | `334_HUMAN-H17__ollama-key.md` | HUMAN-H17 | human | W2 | Optional dedicated, revocable Ollama key (Q-H1-5); readout `Status: DECLINED` when not taken | §10.3 | — | R6.58 | — (marker) | n/a | — | HUMAN dedicated key (optional) |
| 335 | `335_GATE-G26__fp-s.md` | GATE-G26 | gate | W2 | FP-S first-pass spend: verbatim A ≤ $70, m, A_eff, caps, wall-time rule, fallback trigger, W2 quote, prereg hash, upload authorization; waits for R6.57 unless a Q-X62 trigger is recorded | §10.3, §10.1 | 2: XH-007, XH-013 | HUMAN-H15, HUMAN-H16, R6.57 | — (marker) | n/a | STOP | GATE FP-S |
| 336 | `336_R6.59__post-w1.md` | R6.59 | ticket | W1 | ARCHIVE-UPLOAD-W1 under the FP-S upload authorization (operator-accepted deviation); read-back verify; index commit | §4.10 | — | GATE-G26, HUMAN-H16, R6.31b, R6.48 | 30k + 10k; ≤150 lines | operator-gated:GATE-G26 (upload) | — | ARCHIVE-UPLOAD-W1 |
| 337 | `337_R6.60__w2-run.md` | R6.60 | skeleton | W2 | W2 window body: refuses to start until W1's index is `archive_verified`; load, PS0, PP0 (3 × 1 × 3 hosted arms), L1 RECORD corpus (28), teardown | §10.1.4 | 2: XD-041, XH-010 | GATE-G26, R6.59 | — (fleshed after gate) | operator-gated:FP-S | — | W2{PROV, LOAD, PS0, PP0, CORPUS, TEARDOWN} |
| 338 | `338_R6.61__post-w2.md` | R6.61 | ticket | W2 | ARCHIVE-UPLOAD-W2 + SETTLE-W2 (controller-side settlement) | §4.10, §4.7 | — | R6.60 | 30k + 10k; ≤150 lines | operator-gated:GATE-G26 (upload, settlement read) | — | ARCHIVE-UPLOAD-W2 + SETTLE-W2 |
| 339 | `339_R6.62__fp-record-partition.md` | R6.62 | ticket | FP | Partition the W2 corpus by task (dev/gate-A/gate-B); commit digests; sealed dev projection | §10.1.2, §7.4, §6.6 | 3: XH-011, XH-114, XD-106 | R6.61, R6.46 | 35k + 10k; ≤300 lines | offline-only | — | FP-RECORD-PARTITION |
| 340 | `340_R6.56__compaction-slot.md` | R6.56 | ticket | HIR | `compaction_strategy` slot wiring; absent slot byte-identical | §7.3 | 1: XE-122 | R6.55a | 40k + 10k; ≤500 lines | offline-only | — | E2-T7 |
| 341 | `341_R6.57a__native-participant.md` | R6.57a | ticket | HIR | HH-native participant: loopback lane through the proxy, `participant.hir` slot fields, L2-without-L1-pass refusal | §7.3, §7.2, §7.4 | 3: XE-121, XE-126, XD-102 | R6.57, R6.43, R6.15c | 55k + 10k; ≤800 lines | offline-only | — | E2-T8 (b) |
| 342 | `342_R6.63__mini-hir.md` | R6.63 | ticket | HIR | mini-swe-agent 2.4.6 HIR document, representation ledger, O goldens from census and corpus | §7.2, §7.3 | 5: XE-101, XE-102, XE-114, XE-123, XD-123 | R6.62, R6.57a | 70k + 10k; ≤1000 lines | offline-only | — | E2-T9 |
| 343 | `343_R6.64__sweep-glue.md` | R6.64 | ticket | HIR | Enumerate → sealed levels → `LevelSpec.ref_`; declared-delta and liveness refusal | §8.1, §8.2 | 2: XE-124, XE-125 | R6.63 | 55k + 10k; ≤800 lines | offline-only | — | E2-T10 |
| 344 | `344_R6.65__d3-fp.md` | R6.65 | ticket | HIR | `sweep-registry/1` (V1–V9 recorded), V5 level sealed equal to R6.58's declaration, V5 arm in the W4 schedule | §8.1, §8.7, §10.1.2 | 3: XD-201, XD-202, XD-220 | R6.64, R6.58 | 50k + 10k; ≤700 lines | offline-only | — | D3-FP (D3-a + D3-c, first-pass scope) |
| 345 | `345_GATE-G27__prov-w3.md` | GATE-G27 | gate | W3 | PROV-W3: compute quote incl. H-set build/export and load; frozen H record named; disposition `run` \| `not_run{fallback\|pp0_hold}` | §10.3 | — | R6.64, R6.65, R6.43, R6.62, R6.56, R6.34c | — (marker) | n/a | STOP | GATE PROV-W3 |
| 346 | `346_R6.68__w3-run.md` | R6.68 | skeleton | W3 | W3 window body: load, H-set build + export, L1.0/L1.1/L1.2 on gate-A, teardown; or fleshed in place to `unused{…}` under `not_run` | §10.1.4, §7.4 | 5: XE-006, XE-007, XE-008, XE-009, XD-107 | GATE-G27 | — (fleshed after gate) | operator-gated:W3 | — | W3{PROV, LOAD, H-SET BUILD+EXPORT, L1 run, TEARDOWN} (K5, E2-T11) |
| 347 | `347_R6.69__post-w3.md` | R6.69 | ticket | W3 | ARCHIVE-UPLOAD-W3; read-back verify; index commit (or `n/a{w3_not_run}`) | §4.10 | — | R6.68 | 30k + 10k; ≤150 lines | operator-gated:GATE-G26 (upload) | — | ARCHIVE-UPLOAD-W3 |
| 348 | `348_GATE-G28__prov-wc.md` | GATE-G28 | gate | Wc | PROV-Wc (at most once): `triggered{W1′\|W3′\|restart}` within Wc’s reservation, Σ ≤ $20 \| `PASSED — not triggered; Wc unused` | §10.3, §10.1.4 | — | R6.69 | — (marker) | n/a | STOP | GATE PROV-Wc |
| 349 | `349_R6.70__wc-run.md` | R6.70 | skeleton | Wc | Wc contingency body (W1′ repair re-proof, W3′ L1 rerun on gate-B, or restart overflow); not triggered → fleshed in place to a ticket emitting `unused{not_triggered}` | §10.1.4 | — | GATE-G28 | — (fleshed after gate) | operator-gated:Wc | — | Wc{…} |
| 350 | `350_R6.71__post-wc.md` | R6.71 | ticket | Wc | ARCHIVE-UPLOAD-Wc; read-back verify; index commit — or the `n/a{wc_unused}` record | §4.10 | — | R6.70 | 30k + 10k; ≤150 lines | operator-gated:GATE-G26 (upload) | — | ARCHIVE-UPLOAD-Wc |
| 351 | `351_GATE-G29__g-l1.md` | GATE-G29 | gate | W3 | G-L1 decision gate: `full` (pass for the exact H, frozen by digest) \| `fallback` (Q-X62 trigger fired, or both partitions failed); either PASSED with its basis verbatim | §10.3, §7.4, §10.1.4 | — | R6.69, R6.71 | — (marker) | n/a | STOP | GATE G-L1 |
| 352 | `352_R6.66__l2-wiring.md` | R6.66 | ticket | L1 | L2 packet and analysis wiring: `ParityAdmissionDecl`, KL filters, veto layer, claim fold | §7.5 | 8: XD-101, XD-110, XD-112, XD-113, XD-114, XD-116, XD-117, XD-118 | R6.43, R6.30a | 60k + 10k; ≤900 lines | offline-only | — | K7 |
| 353 | `353_R6.67__site-phase34.md` | R6.67 | ticket | SITE | `/phase-3/` and `/phase-4/` pages on fixtures; first-pass labels; §8.8 phrases; typed `withheld` renderings | §9.3, §7.7, §8.8, §10.1 | 5: XF-143, XD-126, XD-222, XH-119, XH-122 | R6.32b, R6.66 | 55k + 10k; ≤800 lines | offline-only | — | F2-h |
| 354 | `354_R6.72__fp-quote-e.md` | R6.72 | ticket | FP | FP-E quote for the recorded G-L1 disposition (`full`: five arms; `fallback`: three hosted arms + `withheld{l1_not_passed}`); W2 measurement packet; D_max, caps, A_eff, carried root | §10.1.4, §10.1.5, §6.6, §6.8 | 3: XD-043, XD-044, XD-062 | GATE-G29, R6.61, R6.65 | 45k + 10k; ≤500 lines | offline-only | — | FP-E quote (D1-f first-pass form) |
| 355 | `355_HUMAN-H21__credit-purchase.md` | HUMAN-H21 | human | W4 | Ollama credit purchase (≤ $10) only if the FP-E quote needs it; A_eff re-declared; readout `Status: NOT_NEEDED` otherwise | §10.3 | — | R6.72 | — (marker) | n/a | — | HUMAN Ollama credit purchase (only if needed) |
| 356 | `356_GATE-G30__fp-e.md` | GATE-G30 | gate | W4 | FP-E main window: arm inclusion by priority, D_max, caps, m, A_eff, W4 quote, holds; `run{full\|hosted-only}` \| `end_after_w2` (REQ-XH-127(b)) | §10.3, §10.1.4 | — | R6.72 | — (marker) | n/a | STOP | GATE FP-E |
| 357 | `357_R6.73__w4-run.md` | R6.73 | skeleton | W4 | W4 window body: load, lane probe, rounds 1–2, checkpoint, rounds 3–5, teardown, GCP project deletion (last window); fleshed per step; hosted-only under `fallback` | §10.1.4, §3.9 | — | GATE-G30 | — (fleshed after gate) | operator-gated:FP-E | — | W4{…} (K8 first-pass form, D1-e) |
| 358 | `358_R6.74__post-w4.md` | R6.74 | ticket | W4 | ARCHIVE-UPLOAD-W4 + SETTLE-W4 | §4.10, §4.7 | — | R6.73 | 30k + 10k; ≤150 lines | operator-gated:GATE-G26 (upload, settlement read) | — | ARCHIVE-UPLOAD-W4 + SETTLE-W4 |
| 359 | `359_R6.75__fp-results-comparative.md` | R6.75 | ticket | RESULTS | Seal opens; answer-scan; P2-FP packet; P4-FP packet, or typed `withheld{l1_not_passed}` under `fallback` | §6.11, §6.5, §8.8, §10.1.3 | 3: XD-005, XD-090, XD-224 | R6.74, R6.40, R6.30a | 55k + 10k; ≤500 lines | offline-only | — | FP-RESULTS (a; D1-g first-pass form) |
| 360 | `360_R6.76__fp-results-parity.md` | R6.76 | ticket | RESULTS | P3-L2 packet, L1-on-L2 audit, L3 report — or typed `withheld{l1_not_passed}` packets under `fallback` | §7.5, §7.6, §10.1.3 | 2: XD-119, XD-120 | R6.75, R6.66 | 55k + 10k; ≤500 lines | offline-only | — | FP-RESULTS (b; K9) |
| 361 | `361_HUMAN-H23__flagged-review.md` | HUMAN-H23 | human | RESULTS | Flagged-pass review: dispositions with excerpt hashes before the results freeze; `Status: DEFERRED` + DF id withholds the freeze claim | §6.9, §10.3 | 1: XD-071 | R6.76 | — (marker) | n/a | — | HUMAN flagged-pass review |
| 362 | `362_R6.77__fp-freeze.md` | R6.77 | ticket | RESULTS | Apply review dispositions; freeze results; first-pass site data (internal profile); publication draft | §9.3, §9.6, §6.9 | — | R6.76, R6.67, HUMAN-H23 | 45k + 10k; ≤400 lines | offline-only | — | results freeze |
| 363 | `363_GATE-G31__pub-fp.md` | GATE-G31 | gate | PUB | First-pass publication: `hh-site-publication/1`; flip as separate HUMAN act; terms read (Q-H1-6) | §9.6, §10.3 | — | R6.77 | — (marker) | n/a | STOP | GATE pub-FP |
| 364 | `364_HUMAN-H24__dataset-flip.md` | HUMAN-H24 | human | PUB | HF dataset visibility flip after both scans; revision pinned; `Status: DEFERRED` + DF id withholds the viewer claim | §10.3, §4.10 | — | GATE-G31 | — (marker) | n/a | — | HUMAN dataset flip |
| 365 | `365_R6.78__pub-fp-record.md` | R6.78 | ticket | PUB | Commit the first-pass publication record; pinned dataset revision; deploy read | §9.6, §4.10 | 1: XH-105 | GATE-G31, HUMAN-H24 | 30k + 10k; ≤200 lines | operator-gated:GATE-G31 | — | PUB-FP |
| 366 | `366_HUMAN-H14__reconcile-w1.md` | HUMAN-H14 | human | W1 | Billing read ≥ 24 h after W1 teardown (expected HOLD only if not yet read) | §10.3 | — | R6.48 | — (marker) | n/a | — | HUMAN reconcile-W1 |
| 367 | `367_HUMAN-H18__reconcile-w2.md` | HUMAN-H18 | human | W2 | Billing read ≥ 24 h after W2 teardown (expected HOLD only if not yet read) | §10.3 | — | R6.61 | — (marker) | n/a | — | HUMAN reconcile-W2 |
| 368 | `368_HUMAN-H19__reconcile-w3.md` | HUMAN-H19 | human | W3 | Billing read ≥ 24 h after W3 teardown; `Status: N/A` when W3 did not run | §10.3 | — | R6.69 | — (marker) | n/a | — | HUMAN reconcile-W3 |
| 369 | `369_HUMAN-H20__reconcile-wc.md` | HUMAN-H20 | human | Wc | Billing read ≥ 24 h after Wc teardown; `Status: N/A` when Wc unused | §10.3 | — | R6.71 | — (marker) | n/a | — | HUMAN reconcile-Wc |
| 370 | `370_HUMAN-H22__reconcile-w4.md` | HUMAN-H22 | human | W4 | Billing read ≥ 24 h after W4 teardown (expected HOLD) | §10.3 | — | R6.74 | — (marker) | n/a | — | HUMAN reconcile-W4 |
| 371 | `371_CAP6.1__cap1.md` | CAP6.1 | capstone | TAIL | Independent whole-round gap analysis; COVERAGE_MATRIX over REQ-X; writes the later-pass routing register (34 ids); accepted deviations checked as deviations | tail | 34 (routing records): XA-126, XC-314, XC-316, XD-013, XD-052, XD-204, XD-205, XD-206, XD-207, XD-208, XD-209, XD-210, XD-211, XD-212, XD-213, XD-214, XD-215, XD-216, XD-217, XD-218, XD-219, XD-221, XD-225, XE-002, XE-010, XE-011, XE-012, XE-013, XE-128, XE-129, XE-130, XE-131, XE-132, XH-033 | R6.78 | 60k + 10k; tail template | offline-only | — | CAP.1 |
| 372 | `372_CAP6.2__cap2.md` | CAP6.2 | capstone | TAIL | Composed verification of Round 6 as one unit | tail | — | CAP6.1 | 60k + 10k; tail template | offline-only | — | CAP.2 |
| 373 | `373_CAP6.3__cap3.md` | CAP6.3 | capstone | TAIL | Closure: two sums, $100 recompute from billed figures, spend per kind, `DELETE_REQUESTED` project check | tail, §10.1.1 | 2: XH-021, XH-110 | CAP6.2, HUMAN-H14, HUMAN-H18, HUMAN-H19, HUMAN-H20, HUMAN-H22 | 60k + 10k; tail template | offline-only | — | CAP.3 |
| 374 | `374_GATE-G32__accept.md` | GATE-G32 | gate | TAIL | Round-6 acceptance (GATE-ACCEPT role) | tail, §10.3 | — | CAP6.3 | — (marker) | n/a | STOP | GATE-ACCEPT |
| 375 | `375_REC6.1__rec1.md` | REC6.1 | reconcile | TAIL | Backlog + readiness; live reads: compute-open, Ollama balance/usage, dataset visibility | tail, §10.6 | 2: XH-029, XH-106 | GATE-G32 | 50k + 10k; tail template | live-read | — | REC.1 |
| 376 | `376_REC6.2__rec2.md` | REC6.2 | reconcile | TAIL | Spec reconciliation: landed code, ADRs, REQ-X coverage; re-reads CAP6.1's routing register live; §0.3 history | tail, §10.6 | 1: XH-015 | REC6.1 | 55k + 10k; tail template | live-read | — | REC.2 |
| 377 | `377_REC6.3__rec3.md` | REC6.3 | reconcile | TAIL | Integration plan from actual stack CI and branches; recommend, never merge | tail | — | REC6.2 | 45k + 10k; tail template | live-read | — | REC.3 |
| 378 | `378_DOC6.1__doc1.md` | DOC6.1 | docs | TAIL | Repo-docs refresh (refresh-repo-docs) | tail | — | REC6.3 | 50k + 10k; tail template | offline-only | — | DOC.1 |
| 379 | `379_DOC6.2__doc2.md` | DOC6.2 | docs | TAIL | Agent-docs refresh (agent-docs) | tail | — | DOC6.1 | 45k + 10k; tail template | offline-only | — | DOC.2 |

### Totals

| kind | rows |
|---|---|
| ticket | 89 (of which 7 are no-new-code execution or record acts: R6.12, R6.59, R6.61, R6.69, R6.71, R6.74, R6.78) |
| skeleton | 5 (W1 R6.48, W2 R6.60, W3 R6.68, Wc R6.70, W4 R6.73) |
| gate | 13 (GATE-G20…G31 + GATE-G32 accept) |
| human | 13 (HUMAN-H12…H24) |
| capstone / reconcile / docs | 3 / 3 / 2 |
| **total rows** | **128** (sequences 252–379) |
| **implement-spec rows** | **97** (89 tickets + 8 tail rows) |

Spec §10.6's first-cut estimate was 100–110 rows with 85–90 implement-spec rows. Both counts land above that band, consciously: the per-window ARCHIVE-UPLOAD rows (REQ-XH-102 names them as "that window's ARCHIVE-UPLOAD row"), the full §10.3 HUMAN table, and the Phase-4 review's eight added rows (three orphan-seam rows R6.24c, R6.34c, R6.38a; five overflow splits R6.1a, R6.15b, R6.52a, R6.55a, R6.57a), less the R6.4/R6.5 merge. Each split row's surface is measured in [the review record](#phase-4-review-record-2026-11-07).

## Decomposition decisions

The plan can be revised at run time. `orchestrate-build` may split an overflowing row (OL-16 size trigger, stopping at the next boundary), merge trivial neighbours, or move the Wc block. Each such change goes through a scoped `decompose-spec mode=extend` insert with a suffix-letter file and a `## Plan extensions` line. Ids are never re-bound to another slug.

### Skeleton fleshing mechanics (SR-27)

The guard routes on the skeleton row itself, and an unlanded skeleton at the head of the chain is `skeleton-dispatch` REFUSED. Fleshing therefore **rewrites the skeleton row in place** (kind `skeleton` → `ticket`, the same id, sequence and filename slug, with a dated manifest note row, as in Round-5 rows 215/217/223) and adds suffix-letter rows only after it (e.g. `R6.73a`, `R6.73b`). The in-place row inherits the skeleton's owned ids (rule O2). W4's body (D = 22.5 h, five steps) is expected to flesh into one row per step: load + lane probe, rounds 1–2, checkpoint readout, rounds 3–5, teardown + project deletion. A window that does not run (Wc not triggered, W3 `not_run`) is fleshed in place into a ticket that emits a typed `unused{…}` record and nothing else (Round-5 precedent: manifest notes 223 and 229).

### Bootstrap and the legacy guard warnings (SR-2, coordinator ruling)

With the stock checker the retained guard returns `verdict FAIL` (46 legacy `reqcov` `unexplained-diagnostic` warnings on rows 083–164f; they predate this program). GATE-G20's pre-registered content therefore authorizes **R6.1's dispatch only**, with the retained guard's ROUTE computed in `--no-checker` mode and the stock build-memory validator run separately with 0 violations — the round-5 GATE-G8 precedent (its scoped bootstrap authorization of R5.1/R5.2). This is never a silent substitution: G20 quotes it. The 46 warnings are an **OPEN FINDING owned by R6.1**. R6.1's AC requires that after it lands, `check-supplement-guard.sh` **with** the stock checker returns `PASS` with `DISPATCH — R6.1a`, the 46 warnings classified into the reviewed baseline with dated provenance (an append-only amendment citing the list's sha256), never silently. R6.1a then dispatches under the normal guard and its AC requires `PASS` with `DISPATCH — R6.2` after it lands (REQ-XH-002's AC is phrased against the first product row, R6.2).

### Disposition gates and fallback routing (SR-1)

The guard has no branch: a gate either lands (`PASSED`) or stops the chain. Every pre-declared alternative path is therefore a **pre-registered disposition** of a gate, recorded as `PASSED — disposition <token>; basis: <verbatim>` (precedent: round-5 GATE-G15/G16 "PASSED — out-of-scope disposition"). Only an operator refusal stops the chain. Disposition basis text never contains the tokens `FAILED` or `NOT PASSABLE` (the guard reads them as non-PASSED); R6.1a carries a fixture for every disposition below.

| gate | pre-registered dispositions | rows that branch |
|---|---|---|
| GATE-G26 FP-S | waits for R6.57's rung record, **unless** a Q-X62 trigger record exists (third specific of §10.1.4: "once the trigger fires, FP-S no longer waits for the rung record") | the marker names the waiver |
| GATE-G27 PROV-W3 | `run` ‖ `not_run{fallback}` (a Q-X62 trigger is recorded before PROV-W3 is presented) ‖ `not_run{pp0_hold}` (REQ-XH-127(b): a held original mini-swe-agent withholds the corpus) | R6.68 fleshed in place to `unused{…}`; R6.69 emits `n/a{w3_not_run}`; HUMAN-H19 `Status: N/A` |
| GATE-G28 PROV-Wc | `triggered{W1′\|W3′\|restart}` ‖ `PASSED — not triggered; Wc unused` | R6.70 fleshed in place to `unused{not_triggered}`; R6.71 emits `n/a{wc_unused}`; HUMAN-H20 `Status: N/A` |
| GATE-G29 G-L1 | `full` (L1 passed for the exact H, frozen by digest) ‖ `fallback` (the Q-X62 trigger fired, or L1 did not pass on either gate partition) | R6.72 quotes five arms or the three hosted arms; R6.73's fleshing builds the matching schedule; R6.75/R6.76 emit P4-FP and P3-L2 packets or typed `withheld{l1_not_passed}` packets (precedent rows 229–237) |
| GATE-G30 FP-E | `run{full}` ‖ `run{hosted-only}` ‖ `end_after_w2` (REQ-XH-127(b)'s scope decision default) | R6.73 fleshed in place to `unused{scope_end}` under `end_after_w2`; R6.74 emits `n/a`; R6.75/R6.76 withhold by the hold's typed reason |

`fallback` is the disposition name; the typed reason every withheld packet carries is the spec's own member `withheld{l1_not_passed}` (REQ-XH-123), never a new vocabulary item. K7 (R6.66) and F2-h (R6.67) are $0 code that lands under either disposition (their pages render the typed withholdings).

**Q-X62 sequencing kept (coordinator ruling).** "FP-S waits for the E2-T8 rung record" stays: GATE-G26 depends on R6.57. R6.58 FP-PREREG no longer depends on any E2 row — it needs only R6.47, W1 and GATE-G23 (its H is fixed by rule, "the version that passes G-L1"). R6.57 is the E2-T8 spike alone, and its **rung record is a landed output in every outcome**: rung 1, 2 or 3 is recorded honestly at the rung reached (rung 3 raises REQ-XE-120's AR-04 amendment ADR to the operator and is not itself a trigger). A stalled or failed spike therefore never holds FP-S. R6.58 also commits the **FP-S readiness record** computation: an offline, $0 recompute over landed records (R6.58 itself, the HUMAN-H15/H16 readout dates, W1's verified harvest manifest) that yields the instant at which every FP-S precondition except the rung record was met and, from it, the Q-X62 14-day trigger instant. The GATE-G26, GATE-G29 and R6.72 records re-run and cite it; nothing is pre-dated.

**Contingency (pre-registered scoped insert).** If the Q-X62 trigger fires while an E2 row ahead of GATE-G26 (R6.49…R6.55a, GATE-G24, R6.57) is unlanded, or GATE-G24 is refused (an operator refusal: the spec's "a refusal stops E2-T5 and is raised to the operator"), the operator's recorded decision authorizes a scoped insert that permutes {HUMAN-H15, HUMAN-H16, HUMAN-H17, GATE-G26, R6.59, R6.60, R6.61, R6.62} ahead of the first unlanded E2 row. It is a pure permutation: none of those rows depends on an E2 row except GATE-G26's R6.57 edge, which the recorded trigger waives. The HH-native arms then follow G-L1's disposition. T7 and T8 (b) already sit after W2, so the exposed E2 stretch is T0…T6 plus the rung spike.

### Conscious tradeoffs

1. **Linearization A recommended; Q-X46 is the operator's decision at GATE-G20 (SR-22).** Q-X46 has no recorded default, so G20 presents both. The plan's order is A (phase 1 and site shell first; H1 recommends it). 71 implement-spec rows precede FP-S (59 precede PROV-W1) at unmeasured throughput, so racing the credit reset at 2026-11-22T16:30:34Z is not attempted. **B is a checked pure permutation:** R6.4, R6.6, R6.9, R6.10, R6.11, R6.32a, R6.32b, R6.33, HUMAN-H12, GATE-G22 and R6.12 move, in that order, to just after GATE-G29, so neither FP-S nor G-L1 (which the Q-X62 trigger times) waits on them; R6.2, R6.3 (owner of the intended-vs-served counts R6.30a consumes), R6.7, GATE-G21 and R6.8 (R6.46 depends on it) stay ahead. The generator checks that order backward-only.
2. **Operator batches (SR-22, SR-15/16 ruling).** Under A, phase-1 publication (HUMAN-H12, GATE-G22, R6.12) moves from right after F2-g to just before HUMAN-H13, so the Pages, pub-P1, billing-alert and PROV-W1 acts form one operator batch; the FP-S acts (HUMAN-H15/H16/H17, GATE-G26) form another. No HUMAN row sits in front of a code row that does not depend on it (machine-checked).
3. **E2 to the rung record sits between W1 and FP-S; T7 and T8 (b) move after W2.** W1 ($0 model, ≈ $5 compute ceiling) runs as soon as P2-infra is complete, and re-proof exclusions are known before FP-PREREG. T7 (compaction slot) and T8 (b) (loopback lane, slot fields, L1-pass refusal) are needed only before W3/W4, so they no longer pad the path to FP-S; the number of rows before G-L1 is unchanged.
4. **$0 rows needed only before W3 or W4 run after W2** (T7, T8 (b), T9, T10, D3-FP); **K7 and F2-h run after G-L1** (SR-17): their only consumers are R6.76 and R6.77, so they no longer pad the FP-S → G-L1 path that Q-X62's 14-day trigger times. GATE-G27 now depends on D3-FP (the H set includes the phase-4 level, REQ-XC-330), on T7 and on BINARY-SETS.
5. **Wc block between W3 and G-L1.** Its most likely trigger is W3′, an L1 rerun on gate-B after a gate-A failure, so G-L1 is presented after any W3′. A W1′ repair or a post-W4 restart overflow moves the block through a scoped insert. If no trigger exists, GATE-G28 records `PASSED — not triggered; Wc unused` and the Wc rows land as typed `unused`/`n/a` records (SR-16). A gate-A L1 failure also needs an allowlist version bump (REQ-XD-106), a scoped-insert code row before PROV-Wc.
6. **Settlement folded into POST-W2/POST-W4.** SETTLE-W2 and SETTLE-W4 (REQ-XC-127, controller-side, $0) share the post-harvest controller session with that window's archive upload: one row per paid window, not two.
7. **Publication records as their own rows** (R6.12, R6.78). The gate records the operator's decision; a ticket commits the `hh-site-publication/1` record, adds the README link (REQ-XF-136, only after the first record) and reads back the deploy. No gate cell writes a file.
8. **Trigger-only HUMAN rows are not chain rows.** These are the credential rotation after a post-upload secret finding (REQ-XH-107, Q-X59), key rotation and custom-domain DNS (only if chosen). Each is created by a scoped insert when its trigger fires and is listed under the manifest's `## Human prerequisites`. The optional dedicated Ollama key (Q-H1-5) *is* a row (HUMAN-H17); its decline token is `Status: DECLINED`.
9. **Calibrated size estimates.** Round-5 contracts estimated 18–28k and drew repeated size-deviation rulings at ≈ 3×. Round-6 estimates are stated on the measured basis (25–75k focused); every row is at or below 75k + 10k, and every row the review flagged on a measured code surface is split.
10. **Reconcile billing reads in a pre-tail block (coordinator ruling on SR-15/16).** HUMAN-H14, H18, H19, H20 and H22 sit together immediately before CAP6.1, the start of the tail whose CAP6.3 consumes the billed figures (§10.3: reconcile rows withhold "round closure totals"). None sits in front of an unrelated code row. Each is an **expected HOLD** there only if its read has not yet been recorded; the operator may land any reconcile readout as soon as its ≥ 24 h have elapsed, and R6.72's D_max then uses that billed figure (REQ-XH-126(b): "billed figure where a reconcile read exists and at their ceiling otherwise"). Window execution stays strictly sequential (each paid window waits for the previous window's verified harvest, REQ-XH-120). REQ-XH-008's "GATE-PROV → PROV → runs → TEARDOWN → HUMAN-reconcile" is enforced by R6.1a as an ordered per-window subsequence (reconcile after its window's teardown and archive row), not as a contiguous block; G20 presents this interpretation for operator confirmation.
11. **Row count above the §10.6 band** (Totals). The splits buy run-sized rows on measured surfaces; the alternative is in-run overflow re-plans (OL-16).

### Pre-split list and watch list (spec §10.6) — dispositions

| spec item | disposition | seam | shared-decision owner |
|---|---|---|---|
| PROXY-CORE (pre-split) | R6.20a / R6.20b | server, Json control, gate, surfaces, field policy, receipts ‖ release modes, deadline, failure taxonomy, recovery, catalog stop (C2's own 3a/3b seam) | receipt format and deposit ordering → R6.20a |
| RECIPE (pre-split) | R6.19a / R6.19b | quote (pre-quote quantities)/provision/verify + declaration + receipts + shim harness ‖ teardown/inventory/compute-open/prepull/harvest/restart/deliver-secret (B3's seam) | `hh-compute-receipt/1` + shim battery → R6.19a |
| F2-c (pre-split) | R6.32a / R6.32b | leaderboard + frontier ‖ noise, pilot/smoke, quote, refusal views (F2 F14's seam) | component conventions over `hh-comparative-results/1` → R6.32a |
| D1-b (pre-split) | R6.30a / R6.30b | estimators + results schema ‖ battery + review record + AR-17/AR-33 dossier | estimator API and `hh-comparative-results/1` → R6.30a |
| ADMIT-CORE (watch) | **split** R6.34a / R6.34b, plus seam row R6.34c | record + validator ‖ two-stage build, ST2 ‖ carried binary sets + export/load/verify (REQ-XC-330, SR-4) | `hh-harness-admission/1` → R6.34a; carried artifacts → R6.34c |
| RUNNER-CORE (watch) | **split** R6.15a / R6.15b / R6.15c (SR-13) | crate, engine drive, slot record, outcome map ‖ topology spike, kill-point battery, durable-only resume ‖ scheduling | `hh-experiment-slot/1` + `withheld{…}` → R6.15a; schedule kinds → R6.15c |
| E2-T2 (watch) | **split** R6.52 / R6.52a (SR-12) | render path, sections, `system` role ‖ `tools[]`, completion surface, `tool_choice` — one renderer, extended, never re-implemented | `ModelSurface` renderer → R6.52 |
| E2-T4 (watch) | **split** R6.53a / R6.53b | pure renderers ‖ G18 durable observation plumbing | `ResultRenderSpec` members → R6.53a |
| (beyond the lists) GUARD-X | **split** R6.1 / R6.1a (SR-14) | register, routing, baseline ‖ marker, pattern, HUMAN, skeleton, disposition checks | REQ-X register legs → R6.1 |
| (beyond the lists) EGRESS | **split** R6.24a / R6.24b | profile admission + static-musl relay ‖ mediator CONNECT tunnel + ordering + allowlist freeze | profile and residual vocabulary → R6.24a |
| (beyond the lists) ARCHIVE | **split** R6.31a / R6.31b | index + condenser ‖ upload tool, scans, read-back, remediation | `hh-receipt-index/1` → R6.31a |
| (beyond the lists) E2-T6, E2-T8 | **split** R6.55 / R6.55a (SR-10); R6.57 / R6.57a (SR-11) | AR-25 schema + regeneration ‖ exec defaults + helper; topology spike + rung record ‖ lane, slot fields, refusal | `open_session.exec` → R6.55; rung record → R6.57 |
| merges | K2 + K3 → R6.42; ADAPT-CHAT-PY + ADAPT-CHAT-NODE → R6.35 (only mini-swe-agent and Pi in the first pass); D3-a + D3-c (first-pass scope) → R6.65; P1-C + P1-D → R6.4 (SR-26) | small adjacent rows sharing fixtures or the renderer interface | — |

### Shared-decision ownership (the earlier row owns; later rows cite and never redefine)

| shared decision / seam | owner | consumers |
|---|---|---|
| REQ-X register legs, Round-6 routing, reviewed-baseline amendment, absorbed coverage checker | R6.1 | R6.1a, CAP6.1, REC6.2 |
| gate-disposition templates and HUMAN readout tokens (`DEFERRED` + DF id, `DECLINED`, `NOT_NEEDED`, `N/A`) | R6.1a | every gate and HUMAN contract from GATE-G21 to GATE-G32 |
| integer µ$ money unit and the one rounding rule (round up per line) | R6.13 | R6.21, R6.25, R6.44 |
| `hh-experiment-slot/1` record and engine drive | R6.15a | R6.15b, R6.15c, R6.23, R6.26 (cost fold), R6.37, R6.44, R6.57a |
| closed `withheld{…}` member set (`budget`, `window_deadline`, `admission:AG<n>`, `admission:pp0_infra_hold`, `admission:native_lane_hold`, `l1_not_passed`, `sealed_until_results`) inside `hh-experiment-slot/1` | R6.15a | R6.15c, R6.34a, R6.44, R6.45, R6.46, R6.72, R6.75, R6.76, site rows |
| process topology and the kill-point battery | R6.15b | R6.24c, R6.57 |
| schedule kinds (seeded k-round interleave, pair-coupled launch, catalog snapshot per block), packing | R6.15c | R6.44, R6.45, R6.65, R6.66 |
| `hh-compute-receipt/1`, shim battery; `quote` emits declaration quantities labelled `pre-quote` (REQ-XD-051) | R6.19a | R6.19b, R6.25 |
| `hh.model_io/1` `proxy` member, receipt deposit ordering, rewrite versioning, Json control channel | R6.20a | R6.20b, R6.21, R6.22, R6.24c, R6.26, R6.31a, R6.37, R6.40 |
| root cap A_eff × (1 − m), carried root pool | R6.21 | R6.24c, R6.44, R6.58, R6.72, FP-S/FP-E readouts |
| `hh-l1-canon/<v>` as a `hharness-wire` module (the only home the proxy may depend on under REQ-XC-101's list; no new crate, no new dependency) | R6.41 | R6.22, R6.43, R6.57a |
| proxy recording format (record/replay) | R6.22 | R6.38, R6.39, R6.42, R6.43, R6.57 |
| `allowlist_mediated` profile and residual vocabulary | R6.24a | R6.24b, R6.34a, R6.35, R6.36 |
| runner supervision of proxy and mediator (launch, Json control, sole-writer roots), slot-token mint/revoke, cell socket grant, root-pool carry-in | R6.24c | R6.34a, R6.37, R6.45, R6.57, R6.57a |
| §3.10 compute ceiling `$_D = r·(D + H_rs) + S·r_disk·h_r + 0.12·E_gib` in integer µ$ — the only compute pricing | R6.25 | R6.29, R6.47, R6.58, R6.72 |
| phase spend ledger (`kind ∈ {model, compute}`) | R6.25 | R6.26, CAP6.3 |
| `task-selection/1` | R6.18 | R6.29, R6.47 |
| `comparative_protocol/1`, `comparative-quote/1` (the only quote, C-11; compute lines priced by R6.25) | R6.29 | R6.30a, R6.30b, R6.47, R6.58, R6.72 |
| `hh-comparative-results/1` (C-33) | R6.30a | R6.32a, R6.32b, R6.67, R6.75 |
| intended-vs-served counts on arm-bearing packets (REQ-XA-006) | R6.3 | R6.30a, R6.75 |
| re-proof packet, discordance register (allowlist freeze record schema: R6.24b) | R6.28 | W1 fleshing, R6.47, R6.58 |
| `hh-receipt-index/1` (incl. pass-level `artifacts`) | R6.31a | R6.31b, R6.33, R6.34c, POST rows |
| `hh-harness-admission/1` | R6.34a | R6.34b, R6.34c, R6.35, R6.36, R6.37, R6.38, R6.39, R6.40 |
| carried binary sets (infrastructure set, H set), layer/bundle export, IAP load, image-ID and binary digest verification, slot-identity digests | R6.34c | R6.48 and R6.68 and R6.73 fleshing, R6.57a |
| harness renderer interface (admission × slot → argv, env, files) | R6.35 | R6.36, R6.39, R6.42 |
| PS0 probe driver; `live-verify.sh` legs `ps0`, `pp0`, `corpus`, `w4-main` | R6.38a | R6.60 and R6.73 fleshing |
| projection formulas p̂_h / t̂_h and continuation semantics | R6.44 | R6.45, R6.72, W4 fleshing |
| HH-native lane-probe driver and the `w4-lane-probe` leg | R6.45 | R6.73 fleshing |
| outcome-seal projections | R6.46 | R6.62, every readout, R6.75 (opens the seal) |
| `hh-l1-diff/1`, `hh-l1-conformance/1`, L1 pass id | R6.43 | R6.57a (runner refusal), R6.63, R6.67, GATE-G29 |
| pre-registration packet (claims, ε, caps, fallback trigger), the first-pass V5 declaration (Q-X56 text, factor id, parameters) and the FP-S readiness record | R6.58 | GATE-G26, R6.65 (seals a level equal to it, with a refusal test), R6.72, GATE-G29, R6.75, R6.76 |
| compiled `ModelSurface` render path (single renderer; R6.52a extends, never re-implements) | R6.52 | R6.52a, R6.53a, R6.53b, R6.54, R6.63 |
| `open_session.exec` schema (AR-25; regenerate, CC7) | R6.55 | R6.55a, R6.57 |
| HH-native topology and the C-05 rung record | R6.57 | GATE-G26 (Q-X62), R6.57a |
| `sweep-registry/1` | R6.65 | R6.75 |
| `claims-lexicon.json` (C-25: after the P1-E record leg, byte-identical refactor) | R6.10 | R6.32a, R6.32b, R6.67 |
| `hh-site-data/1` and the closed derivation list | R6.8 | every site row |
| `hh-site-publication/1` | R6.11 | R6.12, R6.77, R6.78 |
| later-pass routing register (34 ids) and full-target closure | CAP6.1 | REC6.2 (re-reads live), CAP6.3 |

### ADR and amendment placement (spec §10.5; Round-5 dossier precedent)

The spec rules that "Every ADR lands with its implementing ticket, except E2-T0's docs-only bundle and AR-01 in P1-C". This plan follows it. The two dossier rows (E2-T0 and D1-b-b) mirror Round 5's R5.3–R5.5 pattern: each gathers ADRs for an approval gate.

| register | row | note |
|---|---|---|
| AR-01 (ADR), AR-02 (REC), AR-03 (REC) | R6.4 | Q-X25 default: a dated deviation record, not an amendment |
| AR-04 | R6.15a | the Harbor tool admission (pin + `uv.lock`, `uv`) is completed in R6.27, which applies the Q-X41 rule at dispatch (S-4); a C-05 rung-3 outcome in R6.57 raises its amendment to the operator |
| AR-05 | R6.24a | adds the musl target to `rust-toolchain.toml` (REQ-XB-129) |
| AR-06 / AR-07 | R6.16 / R6.17 | — |
| AR-08 (+ possible AMEND-C, S-9 INTERP), AR-10 | R6.20a | if O2's INTERP becomes an amendment, it is raised from R6.20a, never adopted silently |
| AR-09 / AR-11 | R6.13 / R6.14 | — |
| AR-12, AR-13 | R6.29 | AR-13 merges REQ-XD-051 and REQ-XB-221; the round-5 §9.2 envelope is unchanged |
| AR-14 / AR-15 | R6.25 / R6.19a | AR-15 admits `gcloud` and the pinned podman package set as tools |
| AR-16 (SUPP §4.5) | R6.26 | — |
| AR-17, AR-33 | R6.30b (method dossier) → GATE-G23 | §10.5 lists AR-33 as "first needed by FP-PREREG", but the method-admission gate admits its departures, so it is drafted before GATE-G23 |
| AR-18 / AR-20 | R6.34a / R6.31a | the AR-20 upload-tool admission record is completed in R6.31b |
| AR-21, AR-22, AR-23 (ADR), AR-24 (AMEND-C text) | R6.49 (E2-T0 docs bundle) → GATE-G24 | a refusal at GATE-G24 stops E2-T5 and is raised to the operator (contingency above) |
| AR-25 (SCHEMA) | R6.55 | `check-drift.sh`; generated clients regenerated, never hand-edited |
| AR-28 | R6.11 | — |
| AR-31 (decomposition decision) | R6.1 | recorded in GUARD-X's contracts (R6.1, R6.1a) and this plan |
| AR-32 (SUPP + `check-position.sh`) | R6.6 | — |
| AR-19, AR-26, AR-27, AR-29, AR-30 | none in Round 6 | later-pass (AR-29 is "not v1") |

The count matches §10.5: the 21 Round-6 ADRs, one AMEND-C, one SCHEMA, two REC, two SUPP, one INTERP and one decomposition decision each have exactly one row above.

### Gate and HUMAN numbering

Numbering continues from GATE-G19 and HUMAN-H11 (§10.3). Ids keep their allocation from the reviewed draft, so the chain order differs from the numeric order in two places (GATE-G22 follows GATE-G23; GATE-G24 follows GATE-G25) and the reconcile rows sit in the pre-tail block.

| id | row | | id | row |
|---|---|---|---|---|
| GATE-G20 | R6 plan review + bootstrap | | HUMAN-H12 | GitHub Pages enablement |
| GATE-G21 | site identity mock review | | HUMAN-H13 | GCP billing alert |
| GATE-G22 | phase-1 publication | | HUMAN-H14 | reconcile W1 (pre-tail) |
| GATE-G23 | method admission | | HUMAN-H15 | Ollama account settings + exclusivity |
| GATE-G24 | AR-24 canonical amendment | | HUMAN-H16 | HF dataset repo + write token |
| GATE-G25 | PROV-W1 | | HUMAN-H17 | dedicated Ollama key (optional; `DECLINED`) |
| GATE-G26 | FP-S | | HUMAN-H18 | reconcile W2 (pre-tail) |
| GATE-G27 | PROV-W3 (`run` / `not_run`) | | HUMAN-H19 | reconcile W3 (pre-tail; `N/A` if W3 did not run) |
| GATE-G28 | PROV-Wc (`triggered` / not triggered) | | HUMAN-H20 | reconcile Wc (pre-tail; `N/A` if unused) |
| GATE-G29 | G-L1 (`full` / `fallback`) | | HUMAN-H21 | Ollama credit purchase (only if needed; `NOT_NEEDED`) |
| GATE-G30 | FP-E | | HUMAN-H22 | reconcile W4 (pre-tail) |
| GATE-G31 | first-pass publication | | HUMAN-H23 | flagged-pass review |
| GATE-G32 | Round-6 acceptance (`accept`) | | HUMAN-H24 | dataset visibility flip |

Two rows are deliberately absent: the PS0 early gate (Q-X23 default: not used) and the W4 checkpoint, which is automatic. §10.3's "PROV-W3 + G-L1" line becomes two rows (GATE-G27, GATE-G29), because the two decisions fall on either side of the W3 run.

**HUMAN rows and the guard (corrected, SR-15).** A pending HUMAN row at the head of the chain is a guard HOLD; there is no skip path. The plan therefore places each HUMAN row immediately before the first row that genuinely needs it (a gate, the tail, or a row whose `Depends on` names it), and every HUMAN contract names its non-PENDING readout tokens. REQ-XH-026's "code tickets proceed past it only by opening a DEFERRALS row" is realized as a readout `Status: DEFERRED` citing the opened DF id (the guard's landed test accepts any non-PENDING status; R6.1a adds the fixture and requires the DF id). HUMAN-H23 and HUMAN-H24 use it to withhold the freeze claim and the transcript-viewer claim respectively (BM-TICKET-05); HUMAN-H17 uses `DECLINED`, HUMAN-H21 `NOT_NEEDED`, HUMAN-H19/H20 `N/A`. Exits added by the review (SR-20): HUMAN-H13 records the billing account and the controller's `gcloud` auth as `provided: yes` (never the billing-account id, which REQ-XB-219 keeps out of every committed record); HUMAN-H15 records `OLLAMA_API_KEY provided: yes` in `HH_OPERATOR_ENV_FILE`.

### Spec ambiguities resolved here (to be confirmed at GATE-G20)

1. **W1's archive upload vs its authorization — an operator-accepted deviation (SR-18).** REQ-XH-102 has each window upload "before the next paid window's gate", authorized once at FP-S, and its AC has "each paid window's gate readout cites the previous window's `archive_verified` index". For W1 the next paid gate *is* FP-S, so W1's upload cannot be both authorized by FP-S and precede it. The plan runs R6.59 (ARCHIVE-UPLOAD-W1) immediately after GATE-G26 and before the W2 skeleton (`Depends on: GATE-G26, HUMAN-H16, R6.31b, R6.48`); W1's harvest stays on the operator's disk meanwhile (REQ-XH-106); the FP-S readout cites W1's *verified harvest manifest*; R6.60's skeleton AC refuses to start until W1's index is `archive_verified`. This departs from REQ-XH-102's text, so **GATE-G20 lists it as a deviation needing the operator's verbatim acceptance**, CAP6.1 checks it as a deviation (never as a pass), and R6.1a's window-pattern check tolerates R6.59 between GATE-G26 and R6.60. The alternative, moving the upload authorization to PROV-W1, needs a spec amendment.
2. **Q-X46 linearization** has no default: the operator decides at GATE-G20 (tradeoff 1).
3. **Wc placement** (tradeoff 5) and **reconcile placement** (tradeoff 10).
4. **Ownership of later-pass ids.** The spec adopts them (no `deferred` disposition is used, §0.2) but forbids decomposing their bodies now (REQ-XH-001). **Resolution:** CAP6.1 owns the Round-6 obligation for each (write its routing to Round 7 or Round 8, with its trigger gate), with `round_scope: routed(…)` in the register, so the routing register exists before GATE-G32 signs; REC6.2 re-reads it live (SR-25). No Round-7 or Round-8 skeleton is added (Q-X45 default, hand-off §1).
5. **"SELECT commit" and "FP-PREREG" vs D1-d.** D1-d's single row is split by time. R6.47 commits the split, draw, replacement order and dev list before PROV-W1 (REQ-XH-116), together with the W1 declaration and quote PROV-W1 needs (REQ-XH-118). R6.58 freezes the post-replacement list and hashes the packet after W1.
6. **FP-E's quote input.** FP-E is "presented after G-L1 + W2 settlement". Its D_max uses W3's and Wc's billed figures where a reconcile read exists and ceilings otherwise. R6.72 therefore runs after GATE-G29, and the credit-purchase HUMAN row follows the quote that may need it.
7. **Re-proof machinery.** §10.6 says "execution tickets … contain no new code", and B2 #8 is execution only. The observe → freeze → prove driver and its packet are an explicit code row (R6.28) before W1, which also registers the `reproof` leg (Q-X39b default, confirmed at PROV-W1). The same rule puts the PS0 driver and the W2/W4 legs in R6.38a and the lane-probe driver in R6.45 (SR-6).
8. **First-pass runner rules** (REQ-XH-112, 117, 121, 123, 124, 126, 127) have no shaping name in the spec. They become R6.44 (continuation, caps, wall time), R6.45 (fallback, lane probe and hold, PP0 hold) and R6.46 (outcome seal), all before PROV-W1. REQ-XH-127's PP0 hold applies in W2.
9. **REQ-XA-127's $0 targeted probe** (`grid-pattern-transform`, `swe-bench-langcodes`) lives in R6.23. If it cannot run locally, it is owed to W1's fleshing (contributor), never counted as run.
10. **G-L1's two outcomes and the trigger** are GATE-G29's pre-registered dispositions (`full`, `fallback`), both recorded as PASSED with their basis; the trigger record and `hh-l1-conformance/1` carry the substance (SR-1).
11. **REQ-XD-215 vs REQ-XD-220** (SR-25). §10.1.6 lists both under one first-pass adaptation ("baseline shared with the L2 H side and concurrent with it, under one pre-registration"). XD-220's core predicate (phase-4 needs a G-L1 pass for the exact H) applies in the first pass, so R6.65 owns it, citing AR-33's concurrency departure, with R6.57a's refusal and R6.58's pre-registration as contributors. XD-215's core predicate (a confirmation window on the census) has no first-pass instance, so it stays routed to Round 7; its routing record cites the same AR-33 shared-baseline departure.

### Alias table (spec ticket-shaping names → chain ids)

| shaping name | chain id(s) | | shaping name | chain id(s) |
|---|---|---|---|---|
| B0 bootstrap | GATE-G20 | | E2-T0 | R6.49 |
| GUARD-X | R6.1, R6.1a | | E2-T1 / T3 | R6.50 / R6.51 |
| P1-A, P1-B | R6.2, R6.3 | | E2-T2 | R6.52, R6.52a |
| P1-C + P1-D / P1-E | R6.4 / R6.6 | | E2-T4 | R6.53a, R6.53b |
| P1-F (phase 1b) | — (Q-X24; routed) | | E2-T5 / T6 / T7 | R6.54 / R6.55, R6.55a / R6.56 |
| F2-a, F2-b | R6.7, R6.8 | | E2-T8 | R6.57 (rung record), R6.57a |
| F2-e, F2-f, F2-g | R6.9, R6.10, R6.11 | | E2-T9 / T10 | R6.63 / R6.64 |
| F2-c | R6.32a, R6.32b | | E2-T11 (mini L1 gate inputs) | R6.68 (W3 skeleton) |
| F2-d / F2-h | R6.33 / R6.67 | | E2-T12…T16 | — (Round 8) |
| PRICE / LANE | R6.13 / R6.14 | | K1 / K2+K3 / K4 | R6.41 / R6.42 / R6.43 |
| RUNNER-CORE | R6.15a, R6.15b, R6.15c | | K5 (L1 gate run) | R6.68 (W3 skeleton) |
| CELL-SPEC / DEADLINE / SELECT | R6.16 / R6.17 / R6.18 | | K6 (O-only RECORD corpus) | R6.60 (W2 skeleton) |
| RECIPE-a / RECIPE-b | R6.19a / R6.19b | | K7 | R6.66 |
| PROXY-CORE-a / -b | R6.20a / R6.20b | | K8 (L2 run) | R6.73 (W4 skeleton, first-pass form) |
| BUDGET / REPLAY / VERDICT | R6.21 / R6.22 / R6.23 | | K9 (L1-on-L2 audit + L3) | R6.76 |
| EGRESS (+relay; C2 EGRESS folded, C-02) | R6.24a, R6.24b | | K10–K14 | — (Round 8) |
| runner supervision (seam) | R6.24c | | D1-a | R6.29 |
| COST-RECORD / RECONCILE | R6.25 / R6.26 | | D1-b | R6.30a, R6.30b |
| HARBOR-O | R6.27 | | D1-c | R6.40 |
| REPROOF (B2 #8) | R6.28 (code) + R6.48 (W1 run) | | D1-d | R6.47 + R6.58 |
| ARCHIVE (AR-20) | R6.31a, R6.31b | | D1-e (pilot execution) | R6.60 + R6.73 |
| ADMIT-CORE | R6.34a, R6.34b (+ R6.34c binary sets) | | D1-f (quote) | R6.72 (first-pass FP-E form) |
| ADAPT (mini, Pi, Claude Code) | R6.35, R6.36 | | D1-g (results) | R6.75 |
| TERMINUS-HOST | — (Round 8) | | D3-a + D3-c (first-pass scope) = D3-FP | R6.65 |
| CAPTURE / SMOKE-BATTERY / RUNNER-CHECK | R6.37 / R6.38 / R6.39 | | D3-b, D3-d … D3-i | — (Round 7) |
| PS0 driver + live legs (seam) | R6.38a | | FP-PREREG / FP-RECORD-PARTITION | R6.58 / R6.62 |
| HOST-SMOKE, O-CAPTURE, CENSUS (W1 runs) | R6.48 | | SETTLE-W2 / SETTLE-W4 | R6.61 / R6.74 |
| ARCHIVE-UPLOAD-Wn | R6.59, R6.61, R6.69, R6.71, R6.74 | | FP-RESULTS | R6.75, R6.76 (+ R6.77 freeze) |
| CAP.1–3, GATE-ACCEPT | CAP6.1–3, GATE-G32 | | REC.1–3, DOC.1–2 | REC6.1–3, DOC6.1–2 |

### Tail (tail=full) — read decisions

| role | id | read decision |
|---|---|---|
| CAP.1 | CAP6.1 | independent fresh-context verdicts over the REQ-X `COVERAGE_MATRIX.csv` before reading any run ledger; seam hunt across the shared-decision table above; writes the later-pass routing register (34 ids); checks every accepted deviation (W1 upload, reconcile placement) as a deviation, never a pass |
| CAP.2 | CAP6.2 | composed verification of the landed round (offline). It authorizes no new window, and unrun legs are xfails whose reason starts with a DF id |
| CAP.3 | CAP6.3 | the two sums; REQ-XH-110's $100 recompute from billed figures wherever a bill exists (the pre-tail reconcile reads), plus REQ-XH-021's per-kind and per-phase spend; `compute-open.sh` empty (REQ-XH-008); the GCP project's `DELETE_REQUESTED` state (teardown step 7, done in the last window's fleshing) |
| GATE-ACCEPT | GATE-G32 | fresh operator signature; accepted deviations listed |
| REC.1 | REC6.1 | **live state**: `compute-open.sh`, the Ollama balance and usage, the HF dataset visibility, operator-disk retention (REQ-XH-029, REQ-XH-106). These expire, so they are read at the row |
| REC.2 | REC6.2 | **live state**: landed code, ADRs and REQ-X coverage; re-reads CAP6.1's routing register live and checks the §0.3 register's history (REQ-XH-015) |
| REC.3 | REC6.3 | **live state**: stack CI and branches; recommends, never merges (`mergePolicy: OPERATOR`) |
| DOC.1 / DOC.2 | DOC6.1 / DOC6.2 | reader docs from accepted evidence / agent docs from the landed implementation |

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
