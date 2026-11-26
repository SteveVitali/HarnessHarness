# Results — dated phase records

One row per phase record. Each record is an internal, dated artifact;
corrections land as dated addenda, never in-place rewrites. Nothing
here is outside-verified or `live-executed` (ADR-0363 owns that class).

| Date | Phase | Record | Class |
|---|---|---|---|
| 2026-11-07 | 1 | [Execution-surface replay record — phase 1 (TB-core 0.1.1, $0 oracle)](phase1-execution-surface-replay.md) | recorded execution — internal record; not outside-verified |
| 2026-11-26 | 4 (second pass) | [P4-C held-out confirmation packet](../build/reports/2026-11-26-fp-p4c-confirmation.json) — `hh-p4-c-confirmation/1`, outcome `inconclusive{unresolved_cell}` verbatim | recorded execution — internal record; not outside-verified |

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

| 2026-11-19 | 2 | [phase-2-2026-11-19.json](publication/phase-2-2026-11-19.json) — the pinned phase-2 input set (`inputs_digest` `sha256:47b94e794783d917…`); superseded by the 2026-11-20 record below | GATE-G31 per [ADR-0453](../adr/ADR-0453-w4x-extension-unblock-rulings.md) ruling 3 — "Publish — same terms as G31" |
| 2026-11-20 | 2 | [phase-2-2026-11-20.json](publication/phase-2-2026-11-20.json) — the W4x re-pinned phase-2 input set (`inputs_digest` `sha256:47b94e794783d917…`); superseded by the flip record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-20 | 3 | [phase-3-2026-11-20.json](publication/phase-3-2026-11-20.json) — the W4x re-pinned phase-3 input set (`inputs_digest` `sha256:e0d68dc40a068cde…`); superseded by the flip record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-20 | 4 | [phase-4-2026-11-20.json](publication/phase-4-2026-11-20.json) — the W4x re-pinned phase-4 input set (`inputs_digest` `sha256:c62475cb8a1c18a4…`); superseded by the flip record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-20 | 2 | [phase-2-2026-11-20-flip.json](publication/phase-2-2026-11-20-flip.json) — supersedes the phase-2 base record; dataset `visibility: public` restored at pinned revision `a11591e9…` (HUMAN-H20 restore 18:26Z) | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-20 | 3 | [phase-3-2026-11-20-flip.json](publication/phase-3-2026-11-20-flip.json) — supersedes the phase-3 base record; dataset `visibility: public` restored at pinned revision `a11591e9…` (HUMAN-H20 restore 18:26Z) | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-20 | 4 | [phase-4-2026-11-20-flip.json](publication/phase-4-2026-11-20-flip.json) — supersedes the phase-4 base record; dataset `visibility: public` restored at pinned revision `a11591e9…` (HUMAN-H20 restore 18:26Z); superseded by the second-pass record below | [GATE-G31](../build/readouts/GATE-G31.md) PASSED — publish |
| 2026-11-26 | 4 (second pass) | [phase-4-2026-11-26.json](publication/phase-4-2026-11-26.json) — supersedes the phase-4 flip record; pins `{p4-fp, p4c-confirm}` (`inputs_digest` `sha256:966bf3c4ab5a3575…`) — the `hh-p4-c-confirmation/1` fold `ebaf0b17…` joins the phase-4 page | [GATE-G38](../build/readouts/GATE-G38.md) PASSED — publish |

The [results site](https://stevevitali.github.io/HarnessHarness/)
renders the pinned phase-1, phase-2 (pinned under GATE-G31) and
phase-3/4 pages — including the phase-4 second pass — an
internal record; not outside-verified. The dataset
`SteveVitali/harnessharness-archive` is public at pinned revision
`a11591e931442908c34a5bc7a65e8e664f4752d7` (HUMAN-H17,
2026-11-19T04:23Z) — sanitized derivatives only; transcripts link
to it under GATE-G31's "(b) links only" option. Deployment follows
only the operator's merge of the record to `main` through
`pages.yml`.

### Phase-4 second pass — published under GATE-G38

GATE-G38 PASSED `publish` (operator verbatims
`e2773490…` authorizing the publish and `aa273de7…` the packet-only
transcript surface): the record
[phase-4-2026-11-26.json](publication/phase-4-2026-11-26.json) pins
`{p4-fp, p4c-confirm}` and the `/phase-4/` page renders the
[confirmation packet](../build/reports/2026-11-26-fp-p4c-confirmation.json)'s
§8.8 two-statement claims verbatim:

> "Not confirmed on 37 held-out tasks: no selected member was
> confirmed different from the baseline at Holm α = .05 (family
> P4-C1). V1 — inconclusive{unresolved_cell}"
>
> "In exploration on 11 development tasks, the level(s) with the
> highest estimated contrast vs the baseline were V1 (+0.0455,
> reasoning effort · declared lane parameter, proxy pin). These
> estimates are descriptive, selected from the same data, and
> expected to be optimistic (expected optimism ≈ +.03 to +.10 by
> design simulation)."

Outcome verbatim: `inconclusive{unresolved_cell}` — 367/370 slots
resolved (2 `verifier_unavailable{verifier_timeout}` +
1 `recorded{reward_absent}`, never imputed), V1 Δ̂
`refused{UnresolvedCell}`, descriptive cost fields baseline
10,398,498 / V1 6,239,536 µ$. The transcript surface stays
packet-only — the sanitized archive upload, fresh scans and a
dataset revision re-pin are deferred verbatim by the operator; the
`a11591e9…` dataset pin is carried untouched.
