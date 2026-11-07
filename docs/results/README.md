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
Until GATE-G22, no records exist and nothing is public.
