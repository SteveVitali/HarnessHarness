# CAP5.1 — Independent whole-scope gap hunt (round 5, records-in)

- **Verdicts computed records-in, independently of run-ledger narratives.** The
  coverage matrix was generated mechanically by `docs/build/tools/cap5_gap_hunt.py`
  (`--matrix`) from the committed coverage register, packet JSONs, GATE DECISIONS,
  readouts, DEFERRALS and BUILD_INDEX — never copied from per-ticket self-reports.
  Bounded run-ledger reads (R5.42/R5.44/R5.50/R5.55) were used during orientation
  and for specific cross-checks only; they were not verdict inputs. The
  matrix/packet were committed after the checker's own records passed 12/12.
- **Matrix:** `COVERAGE_MATRIX.csv` — one row per literal REQ id (180 rows =
  167 adopted + 12 merged-with + 1 superseded-by; aliases inherit their target's
  verdict). Columns are the BM matrix set plus `required_domain`,
  `achieved_domain`, `owed_legs`, `accepted_scope`. Verdicts are BM-VERDICT-01
  words: `MET` (all clauses at required layer, no open leg), `MET-ENGINEERED`
  (engineering verified; only live/operator/human acts remain, each in an
  OPEN D-row), `PARTIAL` (owed legs routed to a live home), `MISSING`,
  `AT-RISK-INTEGRATION`, `MET-DIFFERENTLY`, `WAIVED`, `N/A-RATIONALE`.
- **Packet:** `../2026-11-04-cap5.1-gap-hunt.json` — machine-readable checks,
  findings and repairs.
- **Conventions used:** `level`=MUST (the supplement states no weaker levels);
  `class`=register disposition; `achieved_domain`=BM-STATUS-01 word
  (`engineered` — podman packets are bounded executions, not `live-executed`
  records; no packet claims it); `owed_legs`=`src:state->home`.

## Method and commands

1. `python3 docs/build/tools/cap5_gap_hunt.py --matrix` — parses committed
   records only (records-in; writes only its own artifacts), emits the JSON
   packet and the CSV matrix.
2. `bash scripts/deferrals-open.sh` → `DF-R5.1-1 | OPEN` (post-repair state adds
   DF-H9-1 and DF-CAP5.1-1, both minted this run).
3. Twelve records-in checks (NC-1…NC-12 per the contract's dated note): packet
   inventory, disposition re-fold, gate↔record verbatim, deferral-reference
   integrity, register ownership/anchor/contributor integrity, register status
   honesty, BUILD_INDEX consistency, ledger machine-state + PHASE-LOG coverage,
   withheld over-claim scan, coverage⇄contributor binding, owed-work homes,
   readout completeness.
4. Findings were repaired append-only (minted rows / appended decision rows /
   a dated correction row), then the checker re-ran to 12/12 pass.

## Roll-up by verdict

| verdict | count | reading |
|---|---|---|
| MET | 78 | pure D/E targets whose owners landed verified engineering evidence (incl. the two dated-note discharged rows REQ-E-202/E-008) |
| MET-ENGINEERED | 29 | engineering verified; remaining acts live in OPEN D-rows — DF-R5.1-1 (ten ids), DF-H9-1 (participant/outreach legs), DF-CAP5.1-1 (S1 residual) |
| PARTIAL | 73 | engineering landed but owed legs route to GATE-G17 scope dispositions or pending tail rows — mostly R/P-layer targets and withheld-stage predicates |
| MISSING / MET-DIFFERENTLY / AT-RISK / WAIVED / N/A | 0 | — |

*engineering closed* = MET + MET-DIFFERENTLY + MET-ENGINEERED = **107**;
*requirement satisfied* = MET + MET-DIFFERENTLY = **78**. No deferred
full-target predicate became MET by this ticket's closing (I-07).

## Findings (real — all repaired this run, append-only)

| # | finding | severity | repair |
|---|---|---|---|
| F-1 | **DF-H9-1 was a phantom row** — cited `open` by six withheld/unselected packets (`df_h9_1:open`), GATE-G15/G16 decisions, H9/H10/H11 readouts and manifest notes; `DEFERRALS.md` had no row — a deferral not in that file did not happen, and `deferrals-open.sh`/`bm-row.sh` were blind to it | high | minted `DF-H9-1 (P)` verbatim from readouts/HUMAN-H9.md (owner=operator; trigger=participants/consent/scope/access/resources supplied; control=claims withheld, gates blocked on the row) |
| F-2 | **GATE DECISIONS G12 row stale** — still read "Reading 2 PENDING" though Reading 2 PASSED 05:44:42Z (readout + pilot packet `reading_2:passed`) | medium | appended the verbatim reading-2 decision row (PHASE-LOG-only decision now in the machine table; old text is history) |
| F-3 | **G15 decided_at drift** — GATE DECISIONS reads `14:32:56Z`; readouts/GATE-G15.md + four packets + PHASE LOG + BUILD_INDEX carry `14:32:29Z` | medium | appended a dated correction row naming the verbatim value (timestamp corrected, decision untouched) |
| F-4 | **Failed S1 residual was orphaned** — `hh-s1-useful/1` `disposition:failed` (REQ-A-003/A-104 + contributor legs on REQ-A-004/A-102); machinery repaired at R5.44 but no re-execution admitted; 'primary only' contributors, no DF row | high | minted `DF-CAP5.1-1 (V)` — routed to GATE-G17 scope disposition (or an earlier named re-execution admission); the failure was always honestly recorded, never inflated |

## Seam hunt

Checked beyond per-row state (the per-ticket lens cannot see these):

- **Leg-fold recomputation** over all 16 disposition-bearing packets — every
  declared disposition equals its leg fold (refused→failed, inconclusive→
  inconclusive, all-verified→positive word). No over-claim.
- **Two-packet emissions** (r5.42a preflight ×2, r5.44 ops-live first/second)
  are retained honestly — the failed/inconclusive emissions are not suppressed.
- **S1→S4 machinery seam**: the `exec_backend_hello_clobber` defect that failed
  S1 was repaired under R5.44's admission and discharged verbatim
  (`discharged_predicates` in the second ops-live packet) — but S1 itself was
  never re-executed (fixed completion). The seam now has a home (F-4).
- **Probe-verb chain**: G11's carried predicate (probe must be live at bind)
  → ENOENT in r5.43 (subjects honestly inconclusive) → discharged at R5.44
  (`probe: applied`/`undeterminable`) → `ok:undeterminable` verbatim in R5.46.
  Carried state consistent end-to-end.
- **`confirmation_blocked` chain**: parity `n0_withheld` → confirm
  `quote_only_withheld` citing `prerequisite:parity` → study report
  `s3_unmet`/`s4_unmet` verbatim → publication/value carry `unmet{
  efficacy_unavailable;parity_withheld}` — no silent upgrade anywhere.
- **Register vs records**: 180 literal rows, 167/12/1 dispositions, every
  adopted target's owner contract + `#ac-` anchor resolves, every contributor
  resolves to a chain row with landed evidence or a pending gate/human — no
  duplicate/missing owner, no anchor without a contributing record.
- **Deferral-reference integrity**: all 104 cited DF-ids resolve (post-repair);
  the only phantom was DF-H9-1 (F-1).
- **Honest withheld posture**: no withheld/unselected packet asserts
  `claim_awarded`; no BUILD_INDEX/PHASE-LOG row inflates a withheld stage;
  register statuses are `planned; unmet` except the two dated-note discharges.

## Residual routing (owed-work homes)

| residual | home |
|---|---|
| ten R5.1 targets' contributor evidence | DF-R5.1-1 → GATE-G17 |
| withheld participant/outreach predicate set | DF-H9-1 → operator supply or GATE-G17 |
| failed S1 re-execution/scope disposition | DF-CAP5.1-1 → GATE-G17 |
| S3 parity (N=0) / S4 confirmation unmet | GATE-G17 (scoped dispositions recorded at G13/G14/G16) |
| publication/value stage predicates (voided) | GATE-G17 |
| all remaining R/P-layer legs | GATE-G17 fresh acceptance |

## Bounds

Records-in only: no runtime probe, no re-execution, no CI inference beyond
committed `logs/ci-*.json` records; pin byte-equality is trusted per the
committed `check_pins` legs (pins are not re-hashed here — noted honestly);
the verdicts judge the committed record at base `27699a43`.
