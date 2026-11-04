# CAP5.2 — Composed execution, methods and recipient verification

**Packet:** `../2026-11-04-cap5.2-composed.json` — `hh-cap5-composed/1`,
`packet_id 727e514f…`, disposition `executed` (the composed
*verification* executed; this is fixture-verified engineering evidence,
never a research/live/study claim). Emitted 2026-11-04T20:49Z by the
battery's `composed_emit` leg (`HH_CAP5_COMPOSED_EMIT`) — never
hand-written.

## Composed path (variants/tiers)

One driver, `hharness::composed::run_composed` (`crates/hharness/src/composed.rs`),
runs the four committed `example-card/1` records — the variants — through
the landed `hharness::examples::run_example` runner on a fresh scratch
`EmbedService` each (scripted `EmbedModel` lane; fixture tier; scratch
stores under a temp dir; no network/provider/credentials; USD 0):

- **ex-01** scripted quickstart → attach by run id → six excerpt checks →
  `kernel.bundle` export → clean-recipient `check_completeness` +
  `reproduce` R0 → `admit_replay` (typed `CardRefused` verbatim).
- **ex-02** pinned `ex-02-fixture/1` → `run_recipe` ×2 tasks (20 subjects:
  register/expand/open/claim/launch/settle/close each) → closed
  `population-receipt/1`s → durable-cell re-pin →
  `finite_manifest_contrast` beta{1,1} → persisted
  `analysis_report_body/1` → `DeclaredReceipt` → clean-recipient
  `recompute_analysis` → `reproduced`.
- **ex-03** pinned executed `hh-s1-useful/1` packet verbatim → sealed
  submission → oracle chain → `model_calls` cap field edit + refusal →
  EX-01 receipt join.
- **ex-04** pinned executed `hh-ops-live/1` packet verbatim → confined
  effect matrix → scripted-lane `model_calls` exhaustion → four typed
  refusal controls.

## Expected vs actual verdicts (AC-1 spine)

Every emitted check maps into the AC-1 stage spine; a stage verifies
only on all-green members (`pass`/`expected_refusal`); drift would land
under `unmapped` (empty — recorded, never dropped).

| stage | expected | actual | member checks |
|---|---|---|---|
| adoption | verified | **verified** | negotiated · run_succeeded · subjects_20 |
| effects | verified | **verified** | excerpt:effect_chain · matrix · exhaustion |
| seal_finish | verified | **verified** | excerpt:run_submission · sealed_submission |
| grade_settle | verified | **verified** | oracle_chain · excerpt:lifecycle_vs_oracle · field_edit |
| populations_methods | verified | **verified** | cells_pinned · informative_contrast · report_identity · exploratory_label |
| receipt | verified | **verified** | receipt |
| clean_recipient | verified | **verified** | excerpt:intended_permission · excerpt:action_model · excerpt:context_replay · export_verify_r0 · replay_admission · recipient_recompute · packet ×2 |
| semantic_controls | verified | **verified** | refusal_controls · refusal_suspend · refusal_resume · refusal_respond · refusal_amend · edit_refusal |

**Counts (from emitted details, never hand-counted):** cards 4 · checks
pass 23 · expected_refusal 7 · fail 0 · subjects_settled 20 ·
receipts_closed 2.

## Commands and native evidence

```sh
cargo test -p hharness --test cap5_2            # 7 passed / 0 failed (~28 s)
cargo test -p hharness                          # all suites 0 failures
cargo fmt --all --check                         # clean
HH_CAP5_COMPOSED_EMIT=docs/build/reports/2026-11-04-cap5.2-composed.json \
  cargo test -p hharness --test cap5_2 composed_emit
```

Native receipts: two closed `population-receipt/1`s (ex-02
`receipts_closed`), the ex-03 receipt join, ex-01 `export_verify_r0`
(bundle export → completeness → R0 reproduce), `replay_admission`
typed-refused verbatim, `recipient_recompute` → `reproduced`, six
pinned artifacts sha256-recomputed before any execution, sixteen
residual packets re-read + sha256 carried, `packet_id` minted last
(`idp_digest` under `cap5_composed` — the landed convention; recomputes
over canonical-minus-id).

## Access validation (permitted, no collection)

`podman --version` → `podman version 6.1.0` at unix 1793825360
(2026-11-04T20:49Z) — the recorded backend-access claim
(`runtime_available:true` in the R5.42a reprobe packet) still holds on
this host. Artifact access: 6 pin members + 16 packet bodies recomputed.
No other current-service claim is made; live authorization is recorded
`pending{not_claimed}` — never borrowed from an earlier stage.

## Negative controls (AC-2)

- `missing_residual_byte_blocks` — the scratch repo minus
  `r5.49-study-report.json`: leg `blocked{inaccessible:read:…}`,
  `sha256:none`, `closure:pending`, packet disposition `failed` (stages
  themselves stayed verified — the missing byte blocks the dependent
  composed claim, never silently skips).
- `tampered_pinned_packet_refuses` — a forged S1 packet (a
  `"verified"`→`"forged"` flip) is caught by the card's own pin
  recompute: `stale_pin` refusal before any composed fold.
- `withheld_residual_never_closes` — every withheld/failed/unselected/
  inconclusive residual folds `pending`; executed/reported legs are
  `recorded` (landed evidence, not a met claim); the S1 packet's
  `withheld{no_independent_correct_artifact}` is byte-verbatim.
- `fixture_labels_retained` — every card's `fixture_label`/`limitations`
  verbatim; a fixture pass closes no S1/S4/R/P predicate.

## Residual routes (verbatim — owed-work homes)

| claim | disposition | closure | home |
|---|---|---|---|
| backend_preflight ×2 | inconclusive | pending | landed:discharged@r5.44 |
| s1_useful | failed | pending | **DF-CAP5.1-1** (resolved) |
| ops_live_first | failed | pending | landed:honest-retention |
| ops_live_second / activation / pilot | executed | recorded | landed:executed |
| s3_parity / s4_confirm | withheld | pending | **GATE-G17** |
| study_report | reported | recorded | **GATE-G17** |
| plugin_exercise / interchange | unselected | pending | **DF-H9-1** (resolved) |
| outside_recipient / formative | withheld | pending | **DF-H9-1** (resolved) |
| publication / value_followup | withheld | pending | **GATE-G17** |

All sixteen homes resolve against committed rows (`DEFERRALS.md` rows or
the `241_GATE-G17__accept.md` contract). No withheld stage was closed;
no deferred full-target predicate became MET by this ticket (I-07);
the participant/publication stage predicates stay withheld (DF-H9-1).

## Guard repair this run (found at pre-dispatch)

`hh-supplement-guard/1` `load_decisions` indexed `correction`-kind GATE
DECISIONS rows as decisions (last-row-wins) — CAP5.1's appended G15
timestamp-correction row hid the PASSED decision → `failed-gate` +
`nextticket` FAIL, blocking CAP5.2's dispatch. Repaired: correction rows
annotate, never decide; fixture `cases/correction-mask` proves a
correction row cannot mask a real non-PASSED decision; the stale
`skeleton-dispatch` fixture (premised on R5.43 still being a skeleton
pre-fleshing) was repointed to flip the manifest kind — battery
14/14 rejected, 0 failures; guard `PASS route: DISPATCH — CAP5.2`.

## Bounds

Records-in residual legs trust the pinned bytes per the committed
`check_pins` legs (pins are not re-hashed beyond sha256 byte equality —
same posture as CAP5.1). The composed run is fixture-verified
engineering evidence only — S1/S4/live/parity/confirmation/participant/
publication claims stay owed to their recorded homes; GATE-G17 owns the
fresh acceptance.
