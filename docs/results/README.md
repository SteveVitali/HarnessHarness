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
| 2026-11-19 | 3 | [phase-3-2026-11-19.json](publication/phase-3-2026-11-19.json) — the pinned phase-3 input set (`inputs_digest` `sha256:1ecaf168ab5ca9ac…`); superseded by the flip record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-19 | 4 | [phase-4-2026-11-19.json](publication/phase-4-2026-11-19.json) — the pinned phase-4 input set (`inputs_digest` `sha256:937aa94c7b0012aa…`); superseded by the flip record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-19 | 3 | [phase-3-2026-11-19-flip.json](publication/phase-3-2026-11-19-flip.json) — supersedes the phase-3 base record; dataset `visibility: public` at pinned revision `a11591e9…` (`dataset_flip: done{human_h17:2026-11-19T04:23Z}`) | [HUMAN-H17](../build/readouts/HUMAN-H17.md) PASSED — flip executed |
| 2026-11-19 | 4 | [phase-4-2026-11-19-flip.json](publication/phase-4-2026-11-19-flip.json) — supersedes the phase-4 base record; dataset `visibility: public` at pinned revision `a11591e9…` (`dataset_flip: done{human_h17:2026-11-19T04:23Z}`) | [HUMAN-H17](../build/readouts/HUMAN-H17.md) PASSED — flip executed |

The [results site](https://stevevitali.github.io/HarnessHarness/)
renders the pinned phase-1 and first-pass phase-3/4 pages — an
internal record; not outside-verified. Phase 2 stays
publication-gate pending. The dataset
`SteveVitali/harnessharness-archive` is public at pinned revision
`a11591e931442908c34a5bc7a65e8e664f4752d7` (HUMAN-H17,
2026-11-19T04:23Z) — sanitized derivatives only; transcripts link
to it under GATE-G31's "(b) links only" option. Deployment follows
only the operator's merge of the record to `main` through
`pages.yml`.
