# REC.1 — backlog and operational readiness

**Stacked on:** `svitali/harnessharness-capstone` (PR #99) · **Do not merge — operator-controlled**

## Summary

`reconcile-build mode=backlog`: everything the build left owed at
GATE-ACCEPT is deduplicated into `docs/build/BACKLOG.csv` (39 rows;
every owed source identifier occurs in exactly one `sources` cell) and
read at `docs/build/BACKLOG.md`; the production path is explicit in
`docs/build/OPERATIONAL_READINESS.md` (capability-by-capability, layers
honest, no TBD, live read of record cited). Catalogue only — no
deviations signed, no deferral cells flipped, no verdicts changed, no
code touched.

Requirement ids touched (backlog ownership, not verdict changes):
`R-2.1.4` `R-2.2.1` `R-2.2.3` `R-2.2.4` `R-2.2.5` `R-2.4.1` `R-2.4.2`
`R-2.4.3` `R-2.4.4` `R-2.5.2` `R-2.5.3` `R-2.6.1` `R-2.7.1` `R-2.7.2b`
`R-2.8.2` `R-2.8.3` `R-2.8.4` `R-2.8.5` `R-2.8.6` `R-2.8.7` `R-2.9.1`
`R-2.9.2` `R-2.9.3` `R-2.9.4` `R-2.9.6` `R-2.10.3` `R-2.10.5` `R-2.11.1`
`R-2.11.3` `R-2.11.4` `R-2.12.1` `R-2.12.4` `R-2.12.6` `R-2.8.1`
`R-2.3.3` `R-2.1.5` `R-2.1.1` `R-2.11.2`.

## Deliverables

- `docs/build/BACKLOG.csv` / `docs/build/BACKLOG.md` — deduplicated
  owed-work register + dated revisit-trigger sweep (114 ADRs).
- `docs/build/OPERATIONAL_READINESS.md` — capability picture + ordered
  critical path with `ticket:`/`proof:` on every step.
- `research/registers/risks.md` — appended `## Round 1 review`
  (append-only).
- `docs/build/runs/REC.1.md`, `docs/build/pr/REC.1.md`, BUILD_INDEX row
  99, LEDGER current-state + phase-log tail.

## Verification

- `check-backlog.sh` → exit 0 (145 stable sources placed once, no
  duplicates, verdict-consistent).
- `check-build-memory.sh` → 0 violations (7 pre-existing warnings only).
- DEFERRALS single-pipe hygiene clean; no `TBD` in the readiness doc.
- `cargo test --workspace` not required — no code changed.
