<!--
  Round-6 contract — experiments first pass (decompose-spec mode=extend, 2026-11-07).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Hand-off: docs/experiments-decomposition-prompt.md. Partition + coverage:
  docs/build/planning/2026-11-07_round6-{decomposition,coverage}.md.
  Insertion authorized by the operator (planning conversation, 2026-11-07): "I want you to
  continue on until you're totally done, the new tickets are inserted, there's a new PR with
  the changes". This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# HUMAN-H12 — Enable GitHub Pages (Source: GitHub Actions)

- **Sequence:** 285 of 325 · **Phase:** Round 6 · **Kind:** human
- **Block:** SITE · **Alias:** HUMAN Pages
- **base_branch:** current checkout
- **Depends on:** R6.6
- **Blocks:** GATE-G22, R6.31
- **Owner:** the operator
- **Scheduled:** trigger — after R6.6 (F2-g, `pages.yml`) lands; placed immediately before GATE-G22 (under linearization A it opens the operator batch HUMAN-H12 → GATE-G22 → R6.31 → HUMAN-H13 → GATE-G25); actual date recorded at the trigger
- **Withheld until done:** phase-1 publication — any public site, the `[results-site]` card transition, the README site link (REQ-XF-136)
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H12.md` (create from `_TEMPLATE.md` at presentation)

> **Operator work — NOT an `implement-spec` input.** Only the operator can change repository settings. An operator or authorized human record supplies the decision; an agent must not sign, act in the settings UI, or assume silence is approval (REQ-XF-130: "No agent performs any of them").

## Source (verbatim from the spec)

§10.3 HUMAN table:

> | GitHub Pages enablement | operator | after F2-g | Settings → Pages → Source: GitHub Actions | phase-1 publication |

REQ-XF-130 (a): "HUMAN rows SHALL cover (a) enabling GitHub Pages, …". Design source: `docs/design/39_results-site.md` F10, HUMAN row (a).

## Checklist

- [ ] In the `SteveVitali/HarnessHarness` repository: Settings → Pages → Source: **GitHub Actions**. <!-- tick with `date -u +%F` + readout pointer -->
- [ ] Record in the readout, verbatim: the operator's words that it is done, the UTC time and the channel; the setting is recorded as `provided: yes` (no credential or token is involved or recorded).
- [ ] Record the site address in use: the default project URL (Q-X43 recorded default) or, if the operator chose a custom domain, that choice — a custom domain's DNS is a separate trigger-only HUMAN row created by scoped insert (plan tradeoff 8), not part of this row.

**What enabling does and does not do.** Enabling the source publishes nothing by itself: `pages.yml` (R6.6) deploys only pages pinned by a committed `hh-site-publication/1` record, and with none it exits "nothing published" (REQ-XF-128). No phase is public before GATE-G22 passes and R6.31's record reaches `main` through the operator's merge.

## Exit criterion

The readout records the operator's own words that the Pages source is GitHub Actions, with UTC and channel, and its `Status:` leaves PENDING. Attempted but unconfirmed action is not completion. The landing tokens are R6.1's HUMAN readout vocabulary; a `DEFERRED` status must cite its opened DF id and withholds phase-1 publication.

## DEFERRALS rule

A row that depends on this one but runs before it is done opens a `DF-<TICKET>-<n>` row in `DEFERRALS.md` — why deferred = "HUMAN-H12 pending · withholds: phase-1 publication", unblocked by = "HUMAN-H12 · owner: the operator · trigger: R6.6 landed" — and reports "gate pending": never a failure, never a fabricated pass. GATE-G22 and R6.31 never proceed on a deferral alone. A second deferral needs the operator's explicit scope/date decision. Follow the [manifest operating rules](00_MANIFEST.md#operating-rules) and [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) X-03.
