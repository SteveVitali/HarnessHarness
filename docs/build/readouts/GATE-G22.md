# GATE-G22 — Phase-1 publication · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)
§10.3 "Gates and HUMAN rows": "Phase-1 publication | first | F2-g + HUMAN Pages | `hh-site-publication/1`
pinning the phase-1 input set; labels; `[results-site]` transition" · §9.6: "The publication gate governs the
site as a claims surface." · REQ-XF-129: a phase's pages appear in the public profile only when a committed,
append-only `hh-site-publication/1` record pins that phase's exact input set and references the operator's
verbatim decision in a gate readout. · REQ-XH-018: publication GATE + publication record + (where transcripts
exist) a separate dataset-visibility flip. · §9.4: word-bounded matching cannot catch paraphrase — a human
publication review remains necessary. · Card register §9.6: `[results-site]` reads "Not yet available —
publication gate pending" until the first publication record, then "Available — internal record; not
outside-verified" (Q-X43 recorded default; cap "same").

## Decision content
Publish phase 1 with exactly the presented input set and labels, or not. Authorizes only R6.31 (the record
commit, the README site link, the `[results-site]` transition, and the deploy after the operator's merge to
main). No spend, no dataset visibility change, no transcript publication, no other phase's pages.

## Pre-registered dispositions
- **approve** → PASSED with the operator's words verbatim, naming the approved `inputs_digest`.
- **PASSED — publication withheld** → no public act; R6.31 lands the no-publication state. Declining to
  publish never stops the engineering chain (GATE-G16 precedent).
- **refuse** → NOT-PASSABLE with what would pass it (a named technical or content defect).

## Technical prerequisites (blockedOn if unmet — verified at presentation)
- HUMAN-H12 readout COMPLETE — Pages source GitHub Actions (API-verified `build_type: workflow`,
  readouts/HUMAN-H12.md) — read 2026-11-09T06:27:04Z — layer: gate-passed
- R6.6 landed — `pages.yml`, `publication.py`, `check-publication.sh` (PR #300) — read 2026-11-09T06:27:04Z — layer: engineered
- `check-site.sh --profile public` — <PUBSITE> — read 2026-11-09T06:27:04Z — layer: engineered
- `check-site.sh --head` — <HEADSITE> — read 2026-11-09T06:27:04Z — layer: engineered
- site-data drift — <DRIFT> — read 2026-11-09T06:27:04Z — layer: engineered
- `check-publication.sh` (publication-gate checklist) — <PUBCHK> — read 2026-11-09T06:27:04Z — layer: engineered

## What is presented
1. **The draft `hh-site-publication/1` content** (uncommitted; R6.31 commits exactly this):
   `docs/results/publication/phase-1-2026-11-09.json` — `phase: "phase-1"`,
   `pages: ["/phase-1/", "/phase-1/tasks/"]`,
   `inputs_digest: sha256:15f819883d372ebe131d63f282d3ba9f89d952b2b6fb969db10ab2a3d0386874`,
   `labels: {evidence_class: "recorded execution", outside_verified: false}`,
   `dataset: {visibility: "private", revision: "", secret_scan_ref: "", contamination_scan_ref: ""}`,
   `gate_row: "286 GATE-G22 (286_GATE-G22__pub-p1.md)"`, `readout: this file`,
   `recorded_at: "2026-11-09"`. The 8 pinned inputs (path + sha256 at the presented commit):
   - `parity-t3` docs/build/reports/2026-11-05-parity-tranche-3.json — `19789c799ab4365f…`
   - `s4-repairs` docs/build/reports/2026-11-06-s4-30slot-repairs.json — `4942496622b54d6c…`
   - `study-r559` docs/build/reports/2026-11-06-r5.59-study-report.json — `59565bdd4b758999…`
   - `study-r63` docs/build/reports/2026-11-07-r6.3-study-report.json — `65994d04ac09624e…`
   - `xcheck` docs/build/reports/2026-11-07-p1-surface-xcheck.json — `fa68ed4b427d92cd…`
   - `estimators` docs/build/reports/2026-11-07-p1-estimators.json — `f91ebd5bbbe07ed4…`
   - `deviations` docs/build/reports/2026-11-07-s4-protocol-deviations.json — `10bf9fe051d91d8a…`
   - `record` docs/results/phase1-execution-surface-replay.md — `77c46f90bfca11e7…`
2. **The public-profile build at this commit (tree sha256)** — `python3 scripts/site/build.py --profile
   public --out /tmp/hh-pub-g22` with the draft record in place — <TREESHA> — phases 2–4 render absent with
   no numbers — read 2026-11-09T06:27:04Z — layer: engineered
3. **The labels** — every page header's REQ-XD-092 statements: "The phase-1 record is an internal record; it
   is not outside-verified." (pages.py `phase-1` evidence statement); card register `[results-site]`:
   "Available — internal record; not outside-verified" on publish (cap "same" — no card exceeds it);
   `labels.outside_verified: false` in the record itself.
4. **The `[results-site]` transition** — "Not yet available — publication gate pending" → "Available —
   internal record; not outside-verified" (Q-X43 recorded default) with the README site link added by R6.31.
5. **The publication-gate checklist** — REQ-XC-014 (HF figures labelled a different serving setup, never a
   replication target or HH result), REQ-XC-120 (`catalog_declared` only, stated limits), REQ-XD-003 (no
   extrapolation beyond executed bands), REQ-XD-092 labels, REQ-XF-142 public-profile refusals — plus the
   operator's own paraphrase reading (§9.4).
6. **Transcripts** — phase 1 has no model transcripts (design/39 F11); no dataset flip is part of this phase;
   REQ-XH-105's transcript-option and terms-read records belong to GATE-G31.

## DEFERRALS rule
Satisfied — no OPEN row scoped to site or phase-1 publication (R6.4–R6.6, R6.20, R6.21, HUMAN-H12). The OPEN
rows are `DF-R6.23-1` (HOSTING/paid-window readouts — owners R6.40/44/45/47) and `DF-R6.28-1` (real-O leg →
W1/R6.32); neither is site- or publication-scoped.

<!-- agent-drafted:begin sha256=8bb412bc9be340603fd5b5b695693f27966adffa30f262192810acc88c29710b -->
The honest reading of this gate: it authorizes publishing exactly this phase-1 input set — the execution-
surface replay record (parity tranche + 30-slot repairs + the two study reports + the cross-check packet +
the estimators record + the deviations ledger + the protocol record) — under the recorded-execution label
with `outside_verified: false`. It changes one settings-visible thing plus three records: the committed
publication record, the `[results-site]` card transition, and the README site link. It authorizes no spend,
no transcript visibility, no phase-2+ pages, and no claim beyond "internal record; not outside-verified".
Declining to publish (PASSED — publication withheld) keeps the site internal-only and never stops the
engineering chain.
<!-- agent-drafted:end -->

## Disposition

**PASSED — 2026-11-09T15:52:54Z · interactive operator chat · verbatim: "approved, proceed"**

- Phase-1 publication **approved** with exactly the presented input set — the approved `inputs_digest` is
  `sha256:15f819883d372ebe131d63f282d3ba9f89d952b2b6fb969db10ab2a3d0386874` (the one input set this readout
  presented; the verbatim references "approved" on this presentation, no other set exists).
- Authorizes only R6.31: committing the record exactly as presented, the README site link, the
  `[results-site]` transition to "Available — internal record; not outside-verified", and the deploy that
  follows the operator's merge to main. No spend, no dataset visibility change, no transcript publication,
  no other phase's pages.

## Signature
Operator decision (verbatim, received 2026-11-09T15:52:54Z via interactive chat): "approved, proceed"
GATE DECISIONS row: 2026-11-09T15:52:54Z | GATE-G22
Operator confirmation of agent-drafted text (verbatim, 2026-11-09T15:52:54Z): "approved, proceed" — covers
sha256:8bb412bc9be3
Signed by: the operator. Recorded by Devin (SWE-2 High), which does not sign.
