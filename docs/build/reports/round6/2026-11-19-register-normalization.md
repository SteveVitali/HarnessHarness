# Dated normalization record — manifest/plan/register drift (DF-CAP6.1-4 / GAP-04)

2026-11-19 · CAP6.2 · append-only normalization index; originals preserved verbatim inside
each dated note.

| surface | drift | normalization |
|---|---|---|
| `309r_GATE-G28l` Depends-on | `R6.45n, GATE-G29 (recorded ruling)` — unresolvable annotation + forward dep | cell → `R6.45n`; original text preserved in the dated note |
| `309s_R6.45o` Depends-on | `GATE-G29` listed forward of the insert | `GATE-G29` removed from the parser cell; verbatim list preserved in the dated note |
| `309p_GATE-G28k` criterion heading | `## Criterion (the scope-decision…)` variant | → `## Criterion (verbatim — the scope-decision…)`; heading only, content untouched |
| `313_R6.47` Kind cell | `skeleton` after in-place fleshing + execution | → `ticket` (dated flesh note stands) |
| `00_MANIFEST.md` `313 note` row | note-cell text ordered so the stock checker's first-`.md` extraction grabbed `GATE-G30.md` (phantom filename → false manifest violation) | note reordered — `313_R6.47__w4-run.md` is now the row's first `.md` token; text otherwise verbatim |
| `BUILD_INDEX.md` | 12 landed rows never indexed (insert + closure-phase rows) | dated normalization block appends rows 309/309p/309q/309r/309v/311/312/315/317/318/320/321a citing committed evidence |
| `check_round6_coverage.py` | plan `skeleton` vs register `ticket` kind cells on fleshed rows | `kind_ok` accepts skeleton→ticket only where the plan says "fleshed" |
| deferral-token drift | DF-R6.50-1 dated completion token + phantom-deferral mentions | covered by the DF discharge records and this round's dated rows; no row deleted |
