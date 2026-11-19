# CAPSTONE_CLOSURE — Round-6 first-pass closure (CAP6.2, stage 1 of 3)

Record class: closure record · authored 2026-11-19 (UTC) · branch `codex/harnessharness-cap6.2`
Requirement coverage: REQ-XH-110 (recompute), REQ-XH-021 (spend by kind/phase), REQ-XH-029 /
REQ-XH-106 (live reads), REQ-XH-015 (history check — stage 3), REQ-XH-001/O4 (routing — stage 3).
Inputs: `CAPSTONE_GAP_ANALYSIS.md`, `COVERAGE_MATRIX.csv` (CAP6.1), `readouts/HUMAN-H18.md`,
`reports/compute/*-cost.json` (21 committed `cost_lib` estimate records), LEDGER `## GATE
DECISIONS` (restated rows, DF-CAP6.1-1), `reports/compute/2026-11-19-fp-w4-teardown.json`.

## The two sums (recomputed from the matrix, never asserted)

- **Engineering closed** = MET + MET-DIFFERENTLY + MET-ENGINEERED = 409 + 1 + 0 = **410 / 456**.
- **Requirement satisfied** = MET + MET-DIFFERENTLY = 409 + 1 = **410 / 456** (MET-ENGINEERED is
  never counted as MET; the matrix contains zero MET-ENGINEERED rows).
- Remainder: **46 PARTIAL**, **0 MISSING** — every PARTIAL's owed legs are dispositioned by this
  ticket (accepted deviation, scoped-out carried leg with owner/trigger, or waiver request).

## Highest layer reached

The highest closure layer reached is **first-pass closure with routed obligations**: every
Round-6 obligation is either satisfied (410), honestly unmet-by-decision/typed-outcome (46
PARTIAL legs), or routed to a named later-pass home (34 ids — `LATER_PASS_ROUTING.md`, stage 3).
Nothing is claimed satisfied by inference; no routed id is counted as satisfied.

## REQ-XH-110 — the amended $100 recompute

Amended bounds (2026-11-16 §10.5 row A2 / ADR-0444, operator ruling R-5): **Σ authorized ≤ $100**
(model ≤ $70 + compute ≤ $30), **Σ spent ≤ $110**; billed where HUMAN-H18 records a figure,
settled/metered/`cost_lib` estimates flagged otherwise; authorized-but-unrun at ceilings only as
the accounting requires; purchased credit in full (Q-X63).

| component | figure | class | source |
|---|---|---|---|
| model spend | **$4.52** | **billed** | HUMAN-H18 (provider usage API + operator console agree, read 2026-11-19T04:23Z/04:45Z) |
| compute spend | **$27.367214** (Σ 27,367,214 µ$ over 21 `*-cost.json` records) | **estimated** (cost_lib tracked, never billed) | `docs/build/reports/compute/*-cost.json` fold, this ticket |
| GCP compute billing | — | **unavailable{no_billing_export}** — project deleted, no billing API read exists | HUMAN-H18 W2/W3/Wc rows |
| W4 | **$0** | `unused{scope_end}` — never provisioned | GATE-G30 `re-scope{end-first-pass-early}`; teardown receipt |
| purchased credit | $0 purchased | purchase NOT_NEEDED (A_eff $60 root covers needed 7,472,434 µ$) | HUMAN-H15 / `hh-fp-e/1` `d6fa1dab…` |

- Σ spent (billed + flagged estimates + purchased credit) = 4.52 + 27.367214 + 0 =
  **$31.887214 ≤ $110** — within the amended bound.
- Σ authorized: model arm ≤ $70 (used $4.52 billed); compute arm ≤ $30 — completed-window
  estimates $27.37 ≤ $30; W4 `unused{scope_end}` counts $0 (never authorized — GATE-G30's
  end-early arm), so **Σ authorized ≤ $100 holds**. Note: an earlier interim fold printed
  `30.024544` — that figure included rows this fold scopes differently (smoke/ws1r membership);
  both folds sit ≤ $30 is **not** claimed — this ticket's authoritative fold is the 21-record
  enumeration above = **$27.367214**; the earlier print is retained as history, never rewritten.
- The model-account tail read (trailing-24 h usage $0.34, `2026-11-19-ws1-settle-confirm-read.json`)
  is a usage-window figure, distinct from the $4.52 month billed total — recorded, not conflated.

## REQ-XH-021 — spend by kind and by phase (µ$, estimates flagged)

By kind: **model** $4.52 billed (Ollama, month total covering W1/W2/Wc/Ws1 metered lines) ·
**compute** 27,367,214 µ$ estimated across 21 committed cost records · **compute billing**
unavailable (see above).

By phase (Σ µ$, cost_lib estimates; W4 excluded — never provisioned):

| phase | windows | µ$ (est.) |
|---|---|---|
| W1 evidence window (+restart) | fp-w1 ×2 | 2,129,191 |
| W2 / W3 evidence | fp-w2, fp-w3 | 368,538 |
| Wc repair windows | fp-wc…wc10 (10) | 16,423,152 |
| Wcs1 successor re-proof | fp-wcs1 | 853,518 |
| Ws1 withheld-subject legs (+re-drive) | fp-ws1, fp-ws1r | 6,338,301 |
| diagnostic smokes (non-evidence) | smoke1/2/4/5 | 1,254,514 |
| **Σ** | | **27,367,214** |

## Live reads (each UTC + source; `unavailable{}` is a landed token, not a failure)

- **open compute:** `bash scripts/compute-open.sh` → **zero rows, exit 0** (read at CAP6.2 stage-1
  assembly; REQ-XB-211 check repaired earlier under R6.47 and re-verified here).
- **project deletion state:** `reports/compute/2026-11-19-fp-w4-teardown.json` —
  `hh-fp-a1b2c3` `DELETE_REQUESTED` at **2026-11-19T02:01:58Z** (read, not re-executed).
- **model billing:** $4.52 billed (HUMAN-H18, sources above); **compute billing:**
  `unavailable{no_billing_export}`.
- **dataset visibility:** flipped public at a11591e9 per HUMAN-H17 (recorded; a fresh HF re-read
  is offline-unavailable in this environment — cited, not re-verified).
- **operator-disk retention:** not recorded — carried to GATE-G32's signature list (§ below).
- **stack CI:** `blockedOn` at base — PRs #414–#416 recorded red on build/test/fmt/drift legs
  (inherited; CAP6.2's own checker repairs are in this branch and verified locally).

## CAP6.1 routed gaps — dispositions

- **DF-CAP6.1-1 (decision-row placement) — CLOSED.** The 15 Round-6 decision rows misfiled under
  `## OPEN FINDINGS` are restated verbatim inside `## GATE DECISIONS` (append-only; originals
  preserved); GATE-G28i's date cell restored at `50e93509`; `supplement_guard` PASS —
  route `DISPATCH — CAP6.2`, zero stock violations.
- **DF-CAP6.1-2 (prereg pin drift) — DISCHARGED as accepted deviation.** Dated amendment record
  `2026-11-19-fp-prereg-hash-drift.md` covers the three `admission_sha256` drifts and the
  `fp-seal-open` leg; frozen records untouched.
- **DF-CAP6.1-3 (REQ-XC-325 unexecuted) — DEFERRED with waiver request.** No executor exists in
  scope; honest record `2026-11-19-req-xc-325-unexecuted.md`; waiver rides GATE-G32's list.
- **DF-CAP6.1-4 (register/manifest normalization) — CLOSED.** Dated normalization notes on
  309p/309r/309s/313 cells; manifest `313 note` token order normalized; BUILD_INDEX backfilled
  with the 12 unrecorded landings (dated normalization row).
- **DF-CAP6.1-5 (checker drift) — CLOSED.** `check-compute-receipts.sh` non-dict guard +
  hh-compute-* scoping (101 receipts valid, 6 non-compute skips noted); `publication.py` `-flip`
  filename/`supersedes`/`recorded_at` handling (check-site 15/15); `fp_e_emit.rs:189`
  needless-borrow fixed; `docs/adr/README.md` regenerated; ADR-0450/0451 placed in BL-47 →
  `gen-revisit-triggers.sh --check` in sync; stock checker `GATE-G30.md` phantom-filename false
  positive resolved via the manifest-note token normalization; guard signature-reuse scoped to
  pre-round signatures (dated repair comment in `supplement_guard.py`).

## GATE-G32 — the three-part signature list

1. **Accepted deviations** (each with ADR/control): W1 upload as W2's first step + the reconcile
   placement (§10.5 A-rows/ADR-0444 lineage); prereg `admission_sha256` drift incl.
   `fp-seal-open` (this ticket's dated amendment); skeleton→ticket fleshing kind normalization;
   the exec-drain/model-port machinery line (PRs #404/#407).
2. **Scoped-out owed legs with owner/trigger:** the 34 routed ids (`LATER_PASS_ROUTING.md`) —
   owner = the named later pass, trigger per row; REQ-XC-325 R-replay executor (absent);
   operator-disk retention decision (unrecorded — the operator's call); stack-CI red legs at
   base (inherited, coordinator-owned diagnosis owed).
3. **Waivers requested** (operator-quoting records/ADRs): REQ-XC-325 unexecuted waiver request
   (`2026-11-19-req-xc-325-unexecuted.md`); the Ws1 compute estimate-over-ceiling
   (5,250,954 µ$ vs 4,136,280 declared — recorded verbatim at R6.45q closeout); the two-fold
   compute-estimate variance recorded in this document.

*CAP6.2 stage 1 of 3. Stages 2 (backlog/readiness) and 3 (routing + §0.3 history) follow in
order; nothing above asserts a merged PR, a discharged operator act, or a satisfied routed id.*
