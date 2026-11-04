# Run ledger — CAP5.2

- Spec: `docs/tickets/239_CAP5.2__cap2.md`
- Base: `a5bf31764ae6dcb75919804ab85d8e30b5d88b47` (codex/harnessharness-evidence-cap5.1 tip; PR #271 CI pending at boundary — honest)
- Branch: `codex/harnessharness-evidence-cap5.2`
- Config: `implement-spec spec=docs/tickets/239_CAP5.2__cap2.md` (autonomous, ledger on; contract already fleshed — concrete AC-1 checks recorded here per dispatch note)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-04T20:40Z (`date -u` at SETUP)
- Closed: (pending)
- Context at SETUP: ticket estimate 28k + 10k reserve; loaded = contract 239 + invariants I-01–I-08/O-01–O-08 + coverage register (no CAP5.2 primary rows — contributing slice only) + decomposition rows 238–246 + CAP5.1 run ledger + gap-hunt packet + CAPSTONE_GAP_ANALYSIS/COVERAGE_MATRIX + runs R5.24/R5.28/R5.29/R5.42–R5.55 (bounded heads) + 17 disposition-bearing + capstone packets + baseline symbols (hharness::examples run_example/ExampleCard/ExampleReport/verify_pins/CheckRecord; hharness::recipe run_recipe; hh-env packet idp_digest mint pattern; supplement_guard load_decisions/failed-gate; ex-01…ex-04 cards + pins) ≈ at estimate; ≤700 changed source/test lines split trigger watched.

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-04T20:39Z —
`DF-R5.1-1 | OPEN` (intentional: GATE-G17 contributor evidence),
`DF-H9-1 | OPEN` (minted CAP5.1 — withheld participant/outreach set),
`DF-CAP5.1-1 | OPEN` (minted CAP5.1 — failed S1 residual → GATE-G17).
All three are carried OPEN; none discharges here.

## Requirements

Composed execution/methods/recipient verification over the landed round:
run the landed offline procedures end-to-end (adoption→effects→
seal/finish→grade/settle→populations/methods→receipt→clean-recipient)
with genuine semantic/negative controls; retain counts, native receipts,
failed/skipped/pending limits; validate current claimed access without
extra collection; never close an absent later research/participant
predicate (withheld stays withheld). Offline-only; no task/provider/
container/participant execution; USD 0.

## Acceptance criteria

- AC-1 (D/E; positive): composed spine runs with semantic/negative
  controls; counts + native receipts + all failed/skipped/pending
  limits retained; claimed access revalidated without new collection.
- AC-2 (D/E; negative): fixture-only cannot close actual S1/S4/R/P;
  inaccessible required byte or pending oracle blocks the dependent
  composed claim; missing live authorization stays pending — never
  borrows earlier stage budget.
- AC-3 (D/E; ownership+evidence): no primary targets — predicates
  resolve to numbered checks + actual evidence in the coverage
  register; achieved D/E recorded separately from owed R/P.
- AC-4 (E/D; scoped verification): regression fails without new
  behavior; fmt + owning-package tests; record commands + counts.
- AC-5 (D/E; phase gate): verification green; owed items are DEFERRALS
  rows; run/PR/BUILD_INDEX/LEDGER agree; no deferred full-target
  predicate becomes MET by this close.

## Concrete checks (AC-1/AC-2 binding — recorded at SETUP)

- CC-1 composed run: all four committed `example-card/1` records execute
  via `run_example` on scratch stores through the landed owners only;
  every emitted check recorded verbatim.
- CC-2 stage fold: the AC-1 spine (adoption, effects, seal_finish,
  grade_settle, populations_methods, receipt, clean_recipient) +
  semantic_controls fold; every emitted check maps to ≥1 stage; a stage
  verifies only on all-green member checks.
- CC-3 counts: cards, subjects, receipts_closed, check verdicts tallied
  from emitted details — never hand-counted.
- CC-4 native evidence: card `records` payloads, pinned-artifact sha256
  recompute, residual-packet sha256s; `packet_id` minted last over the
  canonical record (`idp_digest` — the landed convention).
- CC-5 residual legs (records-in): the 16 disposition-bearing
  `2026-11-0[34]-r5.*` packets re-read; `{file, sha256, version,
  disposition, admission}` verbatim + home resolution (DF row present /
  ticket file present); an absent/corrupt expected packet =
  `blocked{inaccessible}` → composed disposition failed, never pass.
- CC-6 pending carry: failed/withheld/unselected/inconclusive
  dispositions fold to `pending` — no residual becomes `closed`; live
  authorization recorded `pending{not_claimed}`.
- CC-7 access validation: permitted `podman --version` probe
  {probe, unix, result|absent} + artifact-access counts; no claim beyond
  the probe record.
- CC-8 negative controls: fixture labels retained verbatim; stale pin /
  inaccessible byte / refusal propagation exercised; withheld-stage
  promotion guard.

## Plan

1. Repair the found guard defect (`load_decisions` last-row-wins indexes
   `correction` rows as gate decisions — the CAP5.1 G15 timestamp row
   overwrote G15 PASSED → `failed-gate` stops DISPATCH). Skip
   non-decision `correction` rows; fixture proving a correction row
   never masks a real non-PASSED decision.
2. `crates/hharness/src/composed.rs` — `hh-cap5-composed/1` driver:
   `run_composed` drives ex-01…ex-04 through `run_example` (fresh
   `EmbedService` scratch stores each), folds checks into the CC-2
   stages, folds the 16 residual packets verbatim + homes, records the
   access probe, mints `packet_id` last.
3. `crates/hharness/tests/cap5_2.rs` — battery: positive composed run +
   packet round-trip; negatives (missing residual byte blocks;
   tampered pinned packet refuses; pending/failed residual never closes;
   fixture labels retained; no unmapped check). Env-gated emit
   (`HH_CAP5_COMPOSED_EMIT`).
4. Emit `docs/build/reports/2026-11-04-cap5.2-composed.json`; write
   `docs/build/reports/round5/COMPOSED_E2E_REPORT.md`.
5. Verify (fmt, `cargo test -p hharness`, guard PASS, fixture battery,
   docs checks, bm `.`/`--staged`, bm-tools); measure ≤700; PR; ci-boundary;
   §6.5 close.

## Test matrix

| AC | test |
|---|---|
| AC-1 positive | `cap5_2.rs::composed_path_verifies` — all stages verified, counts exact, residual legs recorded verbatim |
| AC-1 round-trip | `composed_packet_json_is_honest` — schema, packet_id recompute, verbatim carries |
| AC-2 missing byte | `missing_residual_byte_blocks` — renamed packet → blocked leg → disposition failed |
| AC-2 stale pin | `tampered_pinned_packet_refuses` — forged S1 packet bytes → stale_pin |
| AC-2 no closure | `withheld_residual_never_closes` — pending fold + verbatim claims |
| AC-2 labels | `fixture_labels_retained` — fixture_label/limitations verbatim |
| AC-4 regression | battery red at base (module absent → compile/missing-file red); guard fixture `correction-mask` |

## Progress

- 2026-11-04T20:40Z — SETUP: branch forked at `a5bf3176`; deferrals read
  (3 OPEN — all carried); contract/invariants/coverage/decomposition/
  CAP5.1 records/R5.42–R5.55 ledgers loaded (bounded).
- 2026-11-04T20:41Z — guard pre-dispatch: **FAIL** — real finding F-1:
  `failed-gate GATE-G15` + `nextticket` — `load_decisions` indexes the
  appended `correction`-kind row as G15's decision (last-row-wins),
  hiding the PASSED decision row. In-scope repair under this run.
- podman probe at SETUP: `podman version 6.1.0` present (permitted
  offline probe — validates the preflight's recorded `runtime_available:
  true` claim without collection).
- 2026-11-04T20:44Z — F-1 repaired: `load_decisions` skips `correction`
  rows (annotators, never decision records); new fixture
  `cases/correction-mask` proves a correction row cannot mask a real
  non-PASSED decision. F-2 found: stale `skeleton-dispatch` fixture
  (premised on R5.43 still a skeleton pre-fleshing) repointed — flips
  the manifest kind instead. Battery 14/14 rejected, 0 failures; guard
  `PASS route: DISPATCH — CAP5.2` post-repair.
- 2026-11-04T20:50Z — composed driver landed (`hh-cap5-composed/1`):
  four cards through `run_example` (fresh scratch svc each) → eight-stage
  AC-1 fold → sixteen residual legs verbatim + homes + sha256 → access
  probe → `packet_id` minted last. Battery `cap5_2.rs` 7/7 green;
  `cargo test -p hharness` all suites 0 failures; fmt clean. Packet
  emitted `reports/2026-11-04-cap5.2-composed.json` disposition
  `executed` (counts 4 cards · 23 pass · 7 expected_refusal · 0 fail ·
  20 subjects · 2 receipts); `COMPOSED_E2E_REPORT.md` written.
- 2026-11-04T20:52Z — **SIZE TRIGGER CROSSED — STOP for scoped ruling.**
  Measured: 841 changed source/test lines under `crates/`
  (`composed.rs` 534 + `cap5_2.rs` 300 + `lib.rs` 7); +43
  docs/build tooling+fixtures (`supplement_guard.py` 5,
  `correction-mask` 22, `skeleton-dispatch` 15+0 net) → **884 total
  vs ≤700 (~1.26×)**. Checkpoint commit; PR/close held.
  **Proposed seam (for the scoped review):**
  - AMEND (proposed): the `hh-cap5-composed/1` record is conjunctive —
    `packet_id` is minted over the whole canonical record (stages +
    cards + 16 residual legs + access + counts); a partial packet is a
    different record. The AC-1 spine, the four-card drive, the residual
    verbatim carry and the negative controls are one verification unit.
  - impl-vs-battery: arithmetically curative (541 vs 337) but
    semantically rejected — landing the driver without its acceptance
    battery leaves unverified machinery at main; the invariant (AC-4 /
    O-05) binds regression to the behavior's own PR.
  - residual-fold split (records-in legs → separate owner): rejected —
    splits the conjunctive packet schema; `packet_id` coverage and the
    staged verdict lose atomicity.
  - guard repair split (42 lines): does not cure the crates/ measure.

## Gap table

(filled during Phase 4)

| item | required | achieved | status | evidence |
|---|---|---|---|---|

## Evidence log

(filled during Phase 5)

## Evidence report

(filled at close)
