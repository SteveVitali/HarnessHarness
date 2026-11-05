# CAP5.3 — Round-5 closure: close layers and prepare fresh acceptance

> The round-5 CAP.3 record. Every layer verdict below is folded
> **records-in** by `docs/build/tools/cap5_closure.py` from the committed
> CAP5.1 matrix/packet, CAP5.2 composed packet, the seventeen
> disposition-bearing round-5 packets, DEFERRALS.md, GATE DECISIONS and
> readouts — never hand-asserted and never copied from run-ledger
> self-reports. Admitted null/negative/inconclusive research may complete
> honestly; missing or unadmitted study/participant work is **unmet**,
> never waived by engineering evidence. **The build is not DONE.**

- **Ticket:** `docs/tickets/240_CAP5.3__cap3.md` (seq 240 of 246)
- **Base:** `2909edce` (codex/harnessharness-evidence-cap5.2 tip)
- **Packet:** `../2026-11-04-cap5.3-closure.json` — `hh-cap5-closure/1`,
  disposition `reported`, checks 10/10, findings 0.
- **Mode:** records-in, offline-only — no task/provider/container/
  participant execution; `live_authorization: pending{not_claimed}`;
  USD $0.
- **Checker:** 10 records-in checks CL-1…CL-10 (contract dated note);
  a mutated packet, missing row, promoted withheld claim or absent
  owed-work home flips its check — fails-without by construction.

## Layer verdicts (AC-9-3: five layers, reported separately)

| layer | verdict | evidence (committed record) | owed → unique home |
|---|---|---|---|
| engineering | **executed** | `hh-cap5-composed/1` executed — 8/8 stages verified, counts {4 cards · 23 pass · 7 expected_refusal · 0 fail · 20 subjects · 2 receipts}; `hh-ops-live/1` + `hh-activation/1` executed; matrix 0 MISSING / 0 AT-RISK | (none — the scoped engineering record landed) |
| research | **inconclusive** | `hh-study-report/1` reported — `study: unmet{efficacy_unavailable;parity_withheld}`, `s1_unmet`/`s3_unmet`/`s4_unmet` all `true` verbatim; `hh-s1-useful/1` **failed** (machinery repaired at R5.44, never re-admitted); `hh-parity-n0/1` withheld (G13 N=0); `hh-confirm-withheld/1` withheld (G14 quote-only); activation+pilot executed | s1 re-execution or scope disposition → **DF-CAP5.1-1 → GATE-G17**; s3/s4 unmet → **GATE-G17** |
| portability | **withheld** | `hh-outside-withheld/1` withheld{stage_voided} — no outside reviewer ever ran verify/recompute (G15 out-of-scope; HUMAN-H9 inputs never supplied) | participant/outreach predicate set → **DF-H9-1** (operator supply) + **GATE-G17** scope |
| public availability | **withheld** | `hh-publication-withheld/1` withheld{stage_voided}/no_publication — nothing published (G16 voided; sixteen members 0; no public act authorized or performed) | publication stage predicates → **GATE-G17** |
| adoption | **withheld** | `hh-value-withheld/1` withheld{stage_voided}/no_observation — `adoption_absence: observed_not_achieved` (nothing to observe — nothing published); `hh-plugin-unselected/1` + `hh-interchange-unselected/1` unselected{stage_voided}; `hh-formative-withheld/1` withheld | value/publication predicates → **GATE-G17**; participant legs → **DF-H9-1** |

Admitted inconclusive research completes **honestly** — the study
report is a real `reported` record carrying the unmet predicates
verbatim. It is not a positive research claim; no layer verdict borrows
from another (I-07/AC-9-3).

## Recomputed totals (from the committed matrix — CL-1)

- Matrix rows: **180** (167 adopted + 12 merged-with + 1 superseded-by)
- Verdicts: **MET 78 · MET-ENGINEERED 29 · PARTIAL 73** — 0 MISSING,
  0 AT-RISK-INTEGRATION, 0 MET-DIFFERENTLY, 0 WAIVED
- **engineering closed** = MET + MET-DIFFERENTLY + MET-ENGINEERED =
  **107** · **requirement satisfied** = MET + MET-DIFFERENTLY = **78**
- **highest achieved domain** = `engineered` (bounded podman packets are
  executions, not `live-executed` records; no row claims higher)
- Required-domain histogram: E 56 · D/E 35 · D/E/R 16 · D/E/P 12 ·
  D/P 11 · E/R 10 · D/R 10 · D 9 · E/D 9 · E/P 8 · P/D 3 · D/R/P 1
- 102 matrix `owed_legs` cells all route to resolvable homes; 12 pending
  composed residual legs each carry exactly one resolved home (CL-4).

## Primary targets this ticket owns (AC-3)

| target | achieved this ticket | still owed |
|---|---|---|
| REQ-C-005 (D/R) | **D/E** — the closure separates engineering completion from research outcome and from any product default: layer verdicts folded independently (CL-2), research records inconclusive-with-unmet verbatim, no favorable-effect criterion exists anywhere in the fold (CL-6). Evidence: this report + `hh-cap5-closure/1` + `cap5_closure.py` | **R** — research-layer disposition → GATE-G17 fresh acceptance |
| REQ-H-009 (D/R/P) | **D/E** — implementation/research/adoption acceptance reported as distinct layers; unsupported claims recorded withheld, never promoted (CL-6/CL-7). Evidence: this report + packet + checker | **R/P** — outside-reviewer (R5.52 withheld), formative (R5.53 withheld), publication (R5.54 withheld), value (R5.55 withheld) predicates → DF-H9-1 / GATE-G17 |

Contributor records R5.49/52/53/54/55 resolve in the coverage register
(CL-3); they are evidence dependencies — the full-target predicates
stay owed to GATE-G17, and this ticket's closing does not claim the
targets satisfied.

## Findings reconciliation (CL-5)

Old findings re-verified against committed records:

- **CAP5.1 F-1…F-4 — all repairs present.** DF-H9-1 row exists with
  owner/trigger (was a phantom cited by six packets); GATE-G12
  reading-2 PASSED row present in GATE DECISIONS (was PHASE-LOG-only);
  G15 decided_at correction row present (14:32:56Z → verbatim 14:32:29Z);
  DF-CAP5.1-1 row exists for the failed-S1 residual → GATE-G17.
- **CC4 hygiene nit — adjudicated `no-action-accepted`.** The S0.3
  measurement sheet names "cargo" once in prose describing a gate run.
  CC4 binds contracts, data models, criteria and stages — a prose
  mention inside a dated report is none of those, and the report is a
  historical record that is never rewritten. Recorded as an accepted
  deviation candidate in the signature list (below) so GATE-G17 sees it.
- **GATE-G1 carried residuals — carried forward.** DF-S0.3-3 (R2
  cross-camp human signature) and ADR-0226 (trigger-5
  satisfied-by-analysis) remain operator-accepted items; they re-surface
  verbatim in signature-list part 1 for fresh signature.
- **No new findings** — the closure recompute found zero unreconciled
  residuals (findings=0 in the packet).

## Owed-work routing (unique homes — CL-4)

| residual | owner | trigger | home |
|---|---|---|---|
| ten R5.1-owned targets' contributor evidence (REQ-C-104/107/108, REQ-H-001/002/003/005/006/007/011) | GATE-G17 | readout records each dispatch boundary's guard invocation | DF-R5.1-1 |
| withheld participant/outreach predicate set (HUMAN-H9) | operator (Steven Vitali, @SteveVitali) | actual named participants/reviewer + consent/scope/access/resources supplied | DF-H9-1 |
| failed S1 useful-task re-execution or scope disposition | GATE-G17 | executed `hh-s1-useful/1` on repaired machinery or verbatim scope disposition | DF-CAP5.1-1 |
| S3 parity (N=0) + S4 confirmation (quote-only) unmet | GATE-G17 | explicit scope disposition (G13/G14/G16 carried verbatim) | GATE-G17 |
| publication + value-observation stage predicates (voided) | GATE-G17 | explicit scope disposition | GATE-G17 |
| all remaining R/P-layer legs (73 PARTIAL + 29 MET-ENGINEERED targets) | GATE-G17 | fresh acceptance + later contributor evidence | GATE-G17 |
| Q-7 human ratification | operator | ratification after O2 disposition — stays open (AC-9-3) | GATE-G17 |

## Three-part signature list for GATE-G17

### Part 1 — accepted deviations (52: 49 size-trigger rulings + 3 carried)

Each row names its control record (a committed ruling/review record —
`docs/build/planning/*-split-review.md` and/or `docs/build/runs/<id>.md`
ruling section). The 49 recorded round-5 size rulings (46 AMEND, 3
SPLIT — R5.6–R5.55 + R5.14b + R5.42a + CAP5.2; every row's control
verified present by CL-8) are deviations already accepted by scoped
fresh-context review; GATE-G17 re-signs the *pattern*, not each count.

Carried/historical items re-surfacing verbatim:

1. **DF-S0.3-3 residual** — R2 cross-camp human signature; a human
   deliverable the build cannot mint (accepted at GATE-G1).
   Control: `DEFERRALS.md` DF-S0.3-3 + `readouts/GATE-G1.md`.
2. **ADR-0226 trigger-5** — armed revalidation trigger dispositioned
   satisfied-by-analysis (accepted at GATE-G1).
   Control: `docs/adr/ADR-0226-s0.3b-cross-candidate-revalidation-disposition.md`
   + `readouts/GATE-G1.md`.
3. **CC4 hygiene nit** — report prose names the E1 build tool once;
   adjudicated no-action-accepted (above). Control: this section.

### Part 2 — scoped owed legs (7)

The owed-work routing table above is the verbatim list — every leg
carries its owner and trigger; the three DF rows stay OPEN.

### Part 3 — waiver requests (3 — each requires operator-quoting record)

1. **S1 useful-task predicate** — bounded execution failed; machinery
   repaired at R5.44 but never re-admitted. Request: waive round-5
   re-execution **or** name a fresh bounded admission. Requires:
   operator verbatim words in GATE DECISIONS + GATE-G17 signature; a
   durable waiver needs an ADR quoting those words.
2. **Participant/outreach predicate set** — stage voided at GATE-G15
   (operator "accept, continue" 2026-11-04T14:32:29Z). Request: ratify
   the round-5 out-of-scope disposition or supply the HUMAN-H9 inputs.
   Requires: operator verbatim words — voiding ≠ discharge, DF-H9-1
   stays open until then.
3. **Publication stage** — voided at GATE-G16 (operator "confirm and
   proceed" 2026-11-04T18:53:32Z). Request: ratify or name specific
   bounded public acts with expiry. Requires: operator verbatim words +
   ADR quoting them if the disposition becomes durable scope.

## GATE-G17 fresh acceptance prep (CL-10)

- **Criterion (verbatim):** "Closure reports engineering, research,
  portability, public availability and adoption separately with executed
  commands/counts/evidence and true skipped/pending limits."
- **Read items:** `COVERAGE_MATRIX.csv`, `CAPSTONE_GAP_ANALYSIS.md`,
  `cap5.1-gap-hunt.json`, `cap5.2-composed.json`,
  `COMPOSED_E2E_REPORT.md`, this report, `cap5.3-closure.json`,
  `DEFERRALS.md`, `LEDGER.md#gate-decisions`.
- **Artifact digests (sha256, at packet emission):**
  coverage_matrix `982c7bfe…`, gap_hunt_packet `389e5589…`,
  composed_packet `9761b1e7…` (full values in the packet's
  `acceptance_prep.artifact_digests`).
- **Scoped items:** CAP5.1 verdicts + 4 repaired findings; CAP5.2
  composed spine + residual routes; CAP5.3 layer verdicts + this
  signature list; DF-R5.1-1 / DF-H9-1 / DF-CAP5.1-1 OPEN; S3/S4 unmet
  obligations; Q-7 open until human ratification.
- **Fresh required fields:** operator words verbatim, UTC, channel,
  scoped items, expiry/void conditions, agent-draft sha256 confirmation.
- **Old signatures:** `readouts/GATE-G17.md` is absent and stays absent
  until the gate presents; no GATE-ACCEPT/GATE-G1 signature satisfies
  this readout (CL-7 verified none committed).

## Bounds and honest limits

Records-in only: verdicts judge the committed record at base `2909edce`.
Pin byte-equality is trusted per the committed `check_pins` legs (same
posture as CAP5.1/CAP5.2); podman access revalidation was CAP5.2's
permitted probe (2026-11-04T20:49Z record), not re-run here — no
current-service claim is made. The closure recomputes from CAP5.1's
committed matrix; it does not regenerate it, and it flips no matrix
verdict — REQ-C-005/REQ-H-009 stay `PARTIAL` there until GATE-G17.
A pending/withheld predicate is never promoted by this ticket closing,
and **the whole build is not DONE** — LEDGER `projectStatus`
stays `IN_PROGRESS`; next → GATE-G17 (seq 241).
