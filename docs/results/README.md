# Results — dated phase records

One row per phase record. Each record is an internal, dated artifact;
corrections land as dated addenda, never in-place rewrites. Nothing
here is outside-verified or `live-executed` (ADR-0363 owns that class).

| Date | Phase | Record | Class |
|---|---|---|---|
| 2026-11-07 | 1 | [Execution-surface replay record — phase 1 (TB-core 0.1.1, $0 oracle)](phase1-execution-surface-replay.md) | recorded execution — internal record; not outside-verified |

## Publication

`site-data/` is the generated `hh-site-data/1` layer (rebuild:
`python3 scripts/site/build.py --emit`; drift check `--check`).
`publication/` holds `hh-site-publication/1` records — the only path a
phase takes to the public profile (REQ-XF-128/129; schema and
operator checklist in [publication/README.md](publication/README.md)).

| Date | Phase | Publication record | Gate |
|---|---|---|---|
| 2026-11-09 | 1 | [phase-1-2026-11-09.json](publication/phase-1-2026-11-09.json) — the pinned phase-1 input set (`inputs_digest` `sha256:15f819883d372ebe…`) | [GATE-G22](../build/readouts/GATE-G22.md) PASSED |
| 2026-11-19 | 3 | [phase-3-2026-11-19.json](publication/phase-3-2026-11-19.json) — the pinned phase-3 input set (`inputs_digest` `sha256:1ecaf168ab5ca9ac…`) | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-19 | 4 | [phase-4-2026-11-19.json](publication/phase-4-2026-11-19.json) — the pinned phase-4 input set (`inputs_digest` `sha256:937aa94c7b0012aa…`) | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |

The [results site](https://stevevitali.github.io/HarnessHarness/)
renders the pinned phase-1 and first-pass phase-3/4 pages — an
internal record; not outside-verified. Phase 2 stays
publication-gate pending and the dataset stays private
(HUMAN-H17 pending). Deployment follows only the operator's merge
of the record to `main` through `pages.yml`.
