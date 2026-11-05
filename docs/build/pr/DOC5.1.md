# DOC5.1 — Refresh reader documentation from accepted evidence

**Spec:** `docs/tickets/245_DOC5.1__doc1.md` (seq 245 of 246 · Kind docs · Role DOC.1) · **Base:** `codex/harnessharness-evidence-rec5.3` @ `42e169d1` · **Requirement IDs stamped:** none — contributing slice only.

## Summary

Refresh the reader-facing surfaces (README + the living docs) so every
claim matches the **accepted round-5 record** — GATE-G17 PASSED
2026-11-05T00:13:07Z (engineering `executed` · research `inconclusive` ·
portability/public/adoption `withheld` · `build_status: not_done`).
Executed things are documented as executed, withheld/unmet things stay
withheld/unmet; no claim goes beyond the committed packets.

## What changed

| file | refresh |
|---|---|
| `README.md` | spine: "two example cards" → four (EX-01–EX-04, labelled fixtures); useful-task bullet names the recorded failed bounded attempt |
| `docs/evidence.md` | useful-task card: `hh-s1-useful/1` ran and `failed` (machinery repaired, no re-execution admitted; DF-CAP5.1-1 open); study card: `hh-study-report/1` `reported` with `unmet{efficacy_unavailable;parity_withheld}` + s1/s3/s4 unmet verbatim; operations card: `hh-ops-live/1` `executed` 24/24, 0 violations, confirmation `withheld{tranche_absent}`; component/interchange cards: recorded `unselected{stage_voided}`; evidence-access card: `withheld{stage_voided}` + DF-H9-1; scenarios table + partial-release note carry the GATE-G17 verdicts and the three open deferrals |
| `docs/status.md` | new dated 2026-11-05 "Round-5 evidence program" section |
| `docs/getting-started.md` | "Two" → "Four" committed `example-card/1` records |
| `docs/experiments.md` | EX-03 pins the retained packet with `disposition: failed` verbatim |
| `CONTRIBUTING.md` | component route: gated + recorded unselected (stage voided) |
| `docs/build/reports/round5/READER_DOCS_CHECK.md` | the contract artifact: claim-level audit, executed checks + counts, committed-HEAD scope |
| `docs/build/runs/DOC5.1.md` | run ledger |

No card **state** was promoted — useful-task/study/operations stay
`Not yet available`, component/interchange/evidence-access stay
`Restricted`, matching the accepted `not_done` fold. Frozen/historical
surfaces untouched.

## Design decisions

- Dispositions are quoted verbatim from the committed packets
  (`failed` / `executed` / `reported` / `withheld` / `unselected`),
  verified byte-wise before writing — never paraphrased upward.
- The failed S1 packet and the withheld/unselected stage records are
  retained and linked, not hidden — the refresh records honesty rather
  than removing it.
- Committed-HEAD checks are explicitly scoped (they test the committed
  tree in a clean clone, not the working tree).

## Verification (actual commands + counts)

- `bash scripts/check-living-docs.sh` → OK (456 files)
- `bash scripts/check-position.sh` → OK, 13 legs (working tree) **and** `--head` @ e9148ce2, 13 legs
- `bash scripts/check-examples.sh` → OK (4 cards, 6 pins, 29 ids) **and** `--head` @ e9148ce2
- `bash scripts/check-trace.sh` → OK **and** `--head` @ e9148ce2
- `bash scripts/check-contributing.sh` → OK, 12 ids **and** `--head` @ e9148ce2
- `bash scripts/check-quickstart.sh` → **green** — verbatim block on committed HEAD e9148ce2 (clone → build → doctor `connectivity:ok` → run `succeeded`/`completed`); committed-HEAD scope recorded
- `bash scripts/check-bm-tools.sh` → OK · `cargo fmt --all --check` → clean (docs-only)
- Documented commands executed on scratch stores: `cargo test -p hharness --test r5_29 ex01` 1/1 · `ex02` 5/5 · `--test r5_45 ex03` 3/3 · `ex04` 2/2
- `bash docs/build/tools/check-supplement-guard.sh` → PASS — route DISPATCH DOC5.1 (82 rows / 180 cand / 0 viol / 8 warn), pre- and post-artifacts

## Acceptance criteria → evidence

| AC | verdict | evidence |
|---|---|---|
| AC-1 refresh guides w/ actual commands + ready evidence/limits; offline checks; links/freshness; preserve unmet + contributor path | met (D/E) | diff of the six living surfaces; all green checks above; unmet/withheld verbatim retained; `no_ritual` leg green |
| AC-2 no unexecuted quickstart claimed verified / unknown access public / impl detail imposed; committed-HEAD check scoped | met (D/E) | quickstart executed at committed HEAD; four `--head` checks at e9148ce2; no public/access claim added; `withheld`/`Restricted` states kept |
| AC-3 primary targets → checks/evidence; achieved D/E vs owed R/P | met (D/E) | none — contributing slice; recorded in run ledger; no full-target predicate claimed |
| AC-4 fails-without; docs-only living-docs/row/link checks; commands + counts | met (E/D) | the checker suite is the fails-without surface (state vocabulary, unmet clause, card/links, lane drift all fail closed); counts above |
| AC-5 universal phase gate | met (D/E) | run/PR/BUILD_INDEX/LEDGER agree at closeout; ≤2KiB phase log; three OPEN deferrals carried verbatim, none closed |
