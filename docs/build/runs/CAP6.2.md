# CAP6.2 run ledger — Round-6 closure (billed sums, live reads, later-pass routing)

Branch `codex/harnessharness-cap6.2` · base `codex/harnessharness-cap6.1` @ `50e93509` ·
offline, USD $0.00, no merges, no production mutations (read-only reads only).

## Stages (contract order — each committed before the next)

1. `e4bf06f2` — `CAPSTONE_CLOSURE.md` + DF-CAP6.1-1…5 dispositions.
2. `029a0952` — `reports/round6/BACKLOG.csv|.md` + `OPERATIONAL_READINESS.md` (live reads).
3. `6fa07411` — `LATER_PASS_ROUTING.md` (34 ids, live re-read) + §0.3 history check.

## Deferral dispositions

| DF | disposition | verification |
|---|---|---|
| DF-CAP6.1-1 | closed — 15 decision rows restated verbatim in GATE DECISIONS; G28i date restored at base | `load_decisions` reads all 15; guard PASS DISPATCH-CAP6.2, 0 stock violations |
| DF-CAP6.1-2 | discharged — dated amendment `…-fp-prereg-hash-drift.md`; frozen records untouched | check-fp-prereg FAIL legs recorded as amended-input drift; all other legs OK |
| DF-CAP6.1-3 | deferred — `unexecuted{no_executor}` record + waiver request on G32 list item 3 | record emitted; nothing claimed executed |
| DF-CAP6.1-4 | closed — dated normalization record + per-file dated notes; 12 BUILD_INDEX landings backfilled | guard nextticket/dependency/forward-dep/criterion checks green |
| DF-CAP6.1-5 | closed — checker repairs verified | compute-receipts 101 receipts + 6 skips; check-site 15/15 ×2 profiles; clippy clean; ADR index + triggers regenerated; stock phantom-filename violation cleared |

## Verification battery (executed, this worktree)

- `check-supplement-guard.sh` → PASS, route `DISPATCH — CAP6.2` (was FAIL/HOLD at base).
- `check-backlog.sh` → PASS (inherited stray-semicolon in BL-47 status repaired).
- `check-compute-receipts.sh` → PASS (101 receipts, 21 cost, 24 decls, 6 non-compute skips).
- `check-site.sh` → PASS 15/15 legs internal + public profiles (committed-HEAD `--head`
  legs pending on the fix commit — recorded as such).
- `check_round6_coverage.py` → PASS 35/35. `check-publication` → 5/5 records.
- `cargo clippy -p hharness-runner --bin fp_e_emit` → clean (189 needless-borrow fixed);
  `cargo fmt -p` → clean.
- `compute-open.sh` → empty output, exit 0. W4 `DELETE_REQUESTED` receipt verified.
- `gen-revisit-triggers.sh --check` → in sync (ADR-0450/0451 placed BL-47); `adr-index.sh` regenerated.
- `check-fp-prereg.sh` → 2 emit-identical FAILs stand as the DF-CAP6.1-2 amended drift;
  `check-fp-results.sh` fp-seal-open leg same family — both bound by the dated amendment,
  never rewritten.

## Inherited vs introduced

Inherited at base (recorded, not caused by CAP6.2): stack PRs #414–#416 CI red on
build/test/fmt/drift (coordinator diagnosis owed — operator blocker); check-fp-prereg/
check-fp-results emit-identical drift (now bound by the dated amendment); BL-47 status
stray `;` (repaired); guard HOLD on G28i (repaired). Introduced by CAP6.2: none known —
the restated decision rows' signature-reuse surfaced and was resolved by the dated
pre-round scoping repair in `supplement_guard.py`.

## Live reads

See `reports/round6/OPERATIONAL_READINESS.md` — every row cites UTC + source;
`unavailable{no_billing_export}` and `unavailable{offline}` are landed tokens.
