<!--
  Round-7 post-round contract — publication gate (operator-directed extension, 2026-11-26).
  Authorization verbatim recorded in the manifest's 2026-11-26T09:00Z extension note.
-->
# GATE-G38 — PUB-P4-C: phase-4 second-pass publication authorization

- **Sequence:** 339 of 342 · **Phase:** Round 7 · **Kind:** gate
- **Block:** records · **Alias:** G38 pub-p4c
- **base_branch:** current checkout
- **Depends on:** GATE-G37
- **Gate status:** presented + signed 2026-11-26 (operator ruling at mint)
- **Live stage:** offline-only
- **Production mutations:** none (the publish itself rides pages.yml post-merge)

## Goal

Authorize the publication of the phase-4 second-pass page: the `hh-p4-c-confirmation/1`
packet `docs/build/reports/2026-11-26-fp-p4c-confirmation.json` as a named phase-4 site
input, rendered by the §8.8 two-statement claims form verbatim, with a packet-only
transcript surface (no archive upload; the sanitized p4c transcript archive is deferred by
the operator's verbatim).

## Decision content (verbatim rulings recorded in the readout)

- Scope: "We should publish what we have so far and later we can follow up and  publish
  updated results when they're ready" — `publish`, follow-up publications expected.
- Transcripts: "Perhaps we defer on the sanitized p4c archive in HF and do packet-only
  for now" — packet-only; no upload leg, no new dataset revision, no new scans.
- The dataset visibility/revision pin is untouched — a new upload would be a separate
  gated act.

## Out of scope

A second confirmation attempt (operator verbatim "Defer decision"); the p4c transcript
archive upload; any claim beyond the §8.8 forms.

## Requirement IDs

- Contributing: REQ-XF-128/129 (publication-record pinning), REQ-XH-018 (dataset posture),
  REQ-XD-222/224 (claims forms as published).
