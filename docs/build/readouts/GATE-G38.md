<!--
  Readout for docs/tickets/339_GATE-G38__pub-p4c.md (manifest 339, Round 7 post-round,
  gate — phase-4 second-pass publication authorization). APPEND-ONLY: the `Status:` line
  is the only line that ever changes; signing APPENDS the Signature block and never
  edits or deletes anything above it.
-->
# GATE-G38 readout — PUB-P4-C: publish the phase-4 second-pass page
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> Publication gate for the Round-7 P4-C confirmation result. REQ-XF-128/129 + REQ-XH-018:
> a phase's pages appear in the public profile only when a committed, append-only
> `hh-site-publication/1` record pins that phase's exact input set, referencing the
> operator's verbatim decision in this readout. The dataset visibility/revision pin is
> untouched by this gate — a transcript upload would be a separate gated act.

## Criterion (verbatim, from the contract)

"Authorize the publication of the phase-4 second-pass page: the `hh-p4-c-confirmation/1`
packet `docs/build/reports/2026-11-26-fp-p4c-confirmation.json` as a named phase-4 site
input, rendered by the §8.8 two-statement claims form verbatim, with a packet-only
transcript surface (no archive upload; the sanitized p4c transcript archive is deferred
by the operator's verbatim)."

## Evidence (per item: source, date read, layer reached)

- `hh-p4-c-confirmation/1` `docs/build/reports/2026-11-26-fp-p4c-confirmation.json`
  sha256 `ebaf0b17…` — disposition `inconclusive{unresolved_cell}` verbatim (367/370
  resolved; V1 primary Δ̂ `refused{UnresolvedCell…}`; Holm α=.05 not-rejected) — read
  2026-11-26 — layer: `engineered`.
- §8.8 two-statement claims verbatim from the packet (the only claims forms the page
  may render):
  > "Not confirmed on 37 held-out tasks: no selected member was confirmed different
  > from the baseline at Holm α = .05 (family P4-C1). V1 — inconclusive{unresolved_cell}"
  >
  > "In exploration on 11 development tasks, the level(s) with the highest estimated
  > contrast vs the baseline were V1 (+0.0455, reasoning effort · declared lane
  > parameter, proxy pin). These estimates are descriptive, selected from the same
  > data, and expected to be optimistic (expected optimism ≈ +.03 to +.10 by design
  > simulation)."
- Existing phase-4 surface: `p4-fp` frozen packet `ef6d1be9…` stays pinned; the
  publication record's inputs become `{p4-fp, p4c-confirm}` and the record
  `supersedes` `phase-4-2026-11-20-flip.json` — `engineered`.
- GATE-G37 PASSED `accept` (operator verbatim "Accept and continue" `ea9ac3e0…`) — the
  round's terminal state accepted; this gate authorizes only the publication act.
- check-site internal 15/15 + public 15/15 green on the current surfaces (pre-input;
  re-verified after the R7.7 land) — `engineered`.

## Decision content for the operator — verbatim on items 1–3

1. **Scope** — `publish` the phase-4 second-pass page pinned to the input set above;
   `withhold` keeps the typed absence; narrower verbatim narrows it.
2. **Transcript surface** — packet-only (no transcript links) or links-only (requires
   the sanitized p4c archive upload + fresh secret/contamination scans + a dataset
   revision re-pin — a separate gated leg).
3. **Second confirmation attempt** — deferred or drafted (a fresh prereg + envelope;
   no spend without your signature).

<!-- agent-drafted:begin sha256=36cda0ef9ba3a4e9656e5a94e40516956b8e96d14cf209dd89db2f76c1047db7 -->
(decision content above)
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Operator:** Steve Vitali · **UTC:** 2026-11-26T08:5xZ · **Channel:** operator CLI
  session (structured rulings).
- **Decision:** `PASSED — publish` — verbatim (item 1): "We should publish what we have
  so far and later we can follow up and  publish updated results when they're ready"
  — verbatim_sha256 `e27734902ec9b4c21bcaccff7bbe07c57c25848d744dbf767203e1755b5be4dd`.
  The follow-up sentence is recorded: later publications supersede by new dated record.
- **Transcript surface (item 2):** `packet-only` — verbatim: "Perhaps we defer on the
  sanitized p4c archive in HF and do packet-only for now" — verbatim_sha256
  `aa273de7eab1bc181eaa51d8eaa1c38ed8f8ece917755078a684f3cbff32328a`. No upload leg,
  no new dataset revision, no new scans; transcript links stay a typed absence.
- **Second confirmation attempt (item 3):** `deferred` — verbatim button label "Defer
  decision". The inconclusive verdict stands; no new envelope drafted.
- Separate verbatim on the same session, recorded for row 342 (not this gate's scope):
  "I authorize up to $25 of spend in advance. If it's estimated to be more than that,
  you can raise to me for re-authorization" — verbatim_sha256 `a312b4a9…` — a probe-
  window spend bound for DF-R7.1-2's unblock leg.
- Layer: `signed` — operator act recorded; expires/voids only by a later recorded
  operator decision.
