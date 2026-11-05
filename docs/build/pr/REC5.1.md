# REC5.1 — Reconcile backlog and current readiness

**Spec:** `docs/tickets/242_REC5.1__rec1.md` (seq 242 of 246, role REC.1)
**Base:** `codex/harnessharness-evidence-cap5.3` @ `72f080bf` (tip incl. the two GATE-G17 marker commits — markers inherit tip)

## Summary

Round-5 backlog + readiness reconcile, records-in and offline-only
(USD $0, no task/provider/container/participant execution). Invoked
`reconcile-build mode=backlog`: gathered every owed source per
BM-VERDICT-01 and homed it exactly once — **4 new open rows
(BL-50…BL-53)** absorb the 73 PARTIAL round-5 matrix ids and the 3
OPEN deferrals GATE-G17 carried forward verbatim (no waiver named);
the 204 ADR revisit triggers were re-swept (150 quiet · 49
fired-answered · 3 fired-unanswered · 2 dormant · 0 superseded — no
re-home needed); the two sums recomputed **107/78**, matching CAP5.3's
headline; LEDGER OPEN FINDINGS (9) stay homed with the CC4 nit's
`no-action-accepted` adjudication recorded. Registers refreshed
append-only: `BACKLOG.csv` (45→49 rows), `BACKLOG.md` (new landing
group + dated sweep/sums/register-review/findings sections),
`OPERATIONAL_READINESS.md` (dated round-5 section, `engineered`
ceiling, critical path with ticket:+proof: per step, no TBD),
`research/registers/risks.md` (`## Round 5 review`). Named artifacts:
`docs/build/reports/round5/{BACKLOG_READINESS.md,READINESS_REPORT.md}`.

## Reconcile findings (real)

- The round-5 record is honest and fully homed: every PARTIAL id got a
  unique live row; every MET-ENGINEERED leg resolves to an OPEN D-row;
  the three OPEN deferrals keep owner/trigger — **withheld stays
  withheld, the build is not DONE**.
- Orient finding: an uncommitted `cap5.3-closure.json` regen
  (base→`27decf54`, `packet_id b82a02e8…`, identical folded record) was
  reverted — the committed packet `9d756c68…` is the BUILD_INDEX-cited
  artifact.
- Checker blind spots recorded: stock `check-backlog.sh` gathers `D-`
  deferral ids and `D-*` leg tokens only — the `DF-*` rows and
  `src:state->DF-id` legs were homed and verified by hand/script
  (documented in `runs/REC5.1.md`).
- Probes (read-only, ≤24 h): `podman --version` → 6.1.0 ·
  `gh release view v0.1.0` → not-draft, 5 assets ·
  `crates.io/api/v1/crates/hharness-cli` → 200 (UA-required; bare 403
  recorded as an access limitation, not a defect).

## AC → evidence

| AC | layer req/ach | verdict | evidence |
|---|---|---|---|
| AC-1 current access/service state read; residuals classified by cause + unique home; readiness updated w/ truthful ceilings | D/E / engineered | met | probes above; `BACKLOG_READINESS.md` cause table; `OPERATIONAL_READINESS.md` §Round-5 |
| AC-2 no stale link/live claim; no duplicate home; no blanket waiver; access failure stays limitation | D/E / engineered | met | `check-backlog.sh` exactly-once; crates.io 403 recorded verbatim; no waiver anywhere in the diff |
| AC-3 primary targets resolve to checks+evidence; achieved D/E separate from owed R/P | D/E / engineered | met | no primary ownership (contributing slice); matrix `owed_legs`→row mapping in `BACKLOG_READINESS.md` |
| AC-4 scoped verification; fails-without; exact commands+counts | E/D / engineered | met | see Verification |
| AC-5 phase gate; DEFERRALS rows for owed; run/PR/BUILD_INDEX/LEDGER agree | D/E / engineered | met | this PR + closeout; REQ ids: none (contributing slice) |

## Verification

- `check-backlog.sh docs/build/BACKLOG.csv docs/build docs/tickets` → exit 0 (204 expected)
- `check-backlog.sh docs/build/BACKLOG.csv docs/build/reports/round5 docs/tickets` → exit 0 (73 expected; sums 107/78)
- `check-living-docs.sh` → OK (456 files) · `check-bm-tools.sh` → OK
- `check-build-memory.sh .` → 0 violations (8 legacy warnings)
- supplement guard → PASS DISPATCH REC5.1 (pre+post)
- fmt/tests: docs-only change — `cargo fmt` n/a (no Rust touched);
  fails-without by construction: removing any new row's sources makes
  `check-backlog.sh` exit 1 on the unhomed PARTIAL ids
