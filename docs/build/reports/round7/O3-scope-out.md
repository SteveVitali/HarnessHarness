# O-3 scope-out record — Round-7 closure (CAP7.2, manifest 336)

2026-11-26 · branch `codex/harnessharness-r6.53` · offline, USD $0.00 · this file is
the permanent per-id disposition record the GATE-G34 ruling made conditional on the
pass's honest terminal state. It is a closure record — it signs nothing new; it cites
the operator's recorded signature.

## The ruling (cited, not re-signed)

GATE-G34 readout `docs/build/readouts/GATE-G34.md`, Status **PASSED** —
operator verbatim (interactive, Devin CLI session, 2026-11-20):

> "O1 approve, O-2 i'm not sure, O-3 approved, yes fold in BL-50. go"

Recorded in `docs/adr/ADR-0454-round7-composite-ruling.md` and the LEDGER GATE
DECISIONS row `2026-11-20T19:15Z | GATE-G34`. The O-3 clause verbatim (ADR-0454
decision 3):

> **O-3 approved — terminal scope-out after the pass.** On the O-1 chain's completion
> the operator's approval stands as the G32-style signature accepting the routed-set
> remainder — §7.8 (XE-002, XE-010…XE-013, XE-128…XE-132), §5.5 (XC-314, XC-316),
> §6.2/§6.7 (XD-013, XD-052), §2.5 (XA-126), §3.8 (XH-033), and any §8.x member the
> pass leaves unsatisfied — as permanently later/not-v1. The disposition record mints
> at closure with the per-id dispositions verbatim; PARTIAL ≠ scoped-out until that
> record lands.

And the conditional (ADR-0454 consequences, verbatim):

> The O-3 signature is **conditional on the pass landing**: if the O-1 chain cannot
> complete, the scope-out does not silently take effect — the chain reports its honest
> terminal state and the disposition re-presents.

## The conditional — verified satisfied

CAP7.1's canonical terminal-state record
`docs/build/reports/2026-11-26-round7-gap-analysis.json`
(`hh-round7-gap-analysis/1`) sha256
`764ec84ff7bfa6bf0696b4a8af12fec9b75a9d760adbb80ea6ca7d844ea37557` — sidecar
re-verified byte-identical this pass (`shasum -a 256 -c` OK on the `.json` and the
`.md` companion `a80d7732…`). Its recorded fields, re-read verbatim at this closure:

- `counts`: **16 satisfied · 2 typed_terminal · 0 unsatisfied · 0 pending · 0
  silently_absent** over the 18 routed §8.x ids.
- `terminal_state.determination`: **`complete`** — "every one of the 18 routed §8.x
  ids carries either a satisfied verdict on committed evidence (16) or a
  spec-sanctioned typed terminal disposition (2: REQ-XD-205
  `withheld{inert_at_admissible_window}`; REQ-XD-214 `not_admitted{m_eq_1}`); zero
  ids are unsatisfied, pending, or silently absent. No routed id's verdict depends on
  a billed figure — the P4-C ≥24 h provider confirming read stays `pending{clock}`
  verbatim as a settlement hygiene item (HUMAN-H21), not a routed-id dependency."
- `terminal_state.o3_implication`: "no §8.x routed member is left unsatisfied — the
  O-3 remainder set (CAP7.2's conditional mint) carries no §8.x additions on this
  record's evidence".

**Determination: `complete` → the O-3 conditional IS satisfied → this record mints.**
The §8.x addition set is **empty** — zero §8.x routed members join the remainder.

## Part 1 — the routed-set remainder: 16 non-§8.x ids, verbatim dispositions

Each row verbatim from `docs/build/reports/round6/LATER_PASS_ROUTING.md` (CAP6.2 —
"a routed id is routed, never satisfied" in Round 6). Disposition under the recorded
O-3 signature: **permanently later/not-v1**.

| id | spec § | Round-6 verdict | Round-6 obligation state (verbatim) | O-3 disposition |
|---|---|---|---|---|
| REQ-XA-126 | §2.5 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XC-314 | §5.5 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XC-316 | §5.5 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XD-013 | §6.2 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XD-052 | §6.7 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-002 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-010 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-011 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-012 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-013 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-128 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-129 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-130 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-131 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XE-132 | §7.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |
| REQ-XH-033 | §3.8 | PARTIAL | "routed, never satisfied this round; later-pass predicate; the routing register is CAP6.2's deliverable" | **permanently later/not-v1** — O-3 (GATE-G34 PASSED / ADR-0454) |

**Count, verbatim:** the ruling's enumerated remainder set is **16 ids** — §7.8
XE-002 + XE-010…XE-013 + XE-128…XE-132 = 10; §5.5 XC-314 + XC-316 = 2;
§6.2/§6.7 XD-013 + XD-052 = 2; §2.5 XA-126 = 1; §3.8 XH-033 = 1. The routing
register's 34 routed ids decompose as 18 §8.x (CAP7.1's verdict table) + these 16 —
the register governs; earlier prose counts of "15" (the memo's §8.3–§8.7 phrasing
carried into some summaries) are superseded by the verbatim enumeration. **§8.x
additions: 0.** Alias rows REQ-XD-124→REQ-XE-013 and REQ-XE-127→REQ-XD-205 stay
folded at their targets verbatim — no separate disposition mints for an alias.

**Effect:** the 16 ids are dispositioned **permanently later/not-v1** — routed,
recorded, and out of scope for v1 by the operator's recorded signature. This record
does not amend the spec (frozen; amendment protocol only), does not re-verdict the
routing register or any coverage matrix row (append-only history), and is not a new
gate act — it is the conditional signature's minted record. ADR-0454's revisit
trigger stands verbatim: any proposal to extend a pass back over these ids, to run
further rounds after this record, or to treat the signature as effective before the
terminal state, must be re-ruled explicitly.

## Part 2 — BL-50's 40 GATE-G17-carried Round-5 matrix rows

ADR-0454 clause 4 verbatim: "BL-50 folded in — the Round-5 GATE-G17-carried
research-layer scope carrier (40 matrix rows — R unmet predicates and voided P legs)
is dispositioned inside Round-7's scope: rows the §8.x pass satisfies are closed by
its evidence; the remainder take the O-3 disposition verbatim in the same closure
record."

The carried set, verbatim from BL-50's minted `sources` cell (REC5.1,
2026-11-05): REQ-B-101; REQ-B-103; REQ-B-105; REQ-B-106; REQ-C-004; REQ-C-005;
REQ-C-105; REQ-C-106; REQ-D-001; REQ-D-002; REQ-D-003; REQ-D-004; REQ-D-007;
REQ-D-009; REQ-D-101; REQ-D-102; REQ-D-103; REQ-D-104; REQ-D-105; REQ-D-106;
REQ-D-107; REQ-D-108; REQ-D-109; REQ-D-110; REQ-D-301; REQ-D-302; REQ-D-303;
REQ-D-304; REQ-D-305; REQ-D-306; REQ-D-307; REQ-D-308; REQ-D-309; REQ-D-310;
REQ-E-011; REQ-G-201; REQ-G-202; REQ-G-206; REQ-G-207; REQ-G-209 — **40 rows**.

**Honest attribution, recorded up front: zero of the 40 are satisfied by the §8.x
pass's own evidence** — the pass's evidence addresses the Round-6-routed §8.x
REQ-XD ids, not Round-5 R/P-layer predicates. The closures below cite the
discharges already committed on BL-50's dated landing note (BL-RECONCILE,
2026-11-06 — `docs/build/runs/BL-RECONCILE.md`); the withheld/voided legs the
reconcile recorded as still carried take the O-3 disposition verbatim.

### The cited 2026-11-06 discharge evidence (BL-RECONCILE dated note, verbatim)

"the R-layer legs discharged by real evidence — parity verified 30/30 EXACT with
zero discordance (`reports/2026-11-05-parity-tranche-3.json`,
hh-parity-tranche-report/1); S4 confirmation executed 300/300 across four epochs
(through `reports/2026-11-06-s4-30slot-repairs.json`, hh-s4-30slot-repairs/1; first
subjects 60/60); efficacy computed (`reports/2026-11-06-r5.59-efficacy.json`,
hh-s4-efficacy/1 — honest null-equivalent, never inflated); GATE-G19 PASSED
2026-11-06 (complete-cohort admission); study recompute
`reports/2026-11-06-r5.59-study-report.json` reported with `unmet{s1_failed}`
carried verbatim."

With the 2026-11-07 dated correction carried verbatim (R6.3, REQ-XA-114/AR-03):
"'parity verified 30/30' is the tranche's own exact-agreement verdict (tb ↔ scratch
reference runner, R=1 — parity_admission refuses the design:
MissingKey{pins.rights_ref} / AdapterFloor{repeats:1}); 'honest null-equivalent' is
0 by construction (oracle participant — arms cannot change outcomes), not efficacy
evidence." These discharges stand as recorded — evidence, not inflation.

### Leg vocabulary (verbatim from the matrix's `owed_legs` cells)

- `R-layer->GATE-G17` — the research-layer (measurement/methods/parity/
  confirmation) unmet predicates carried by GATE-G17's fresh acceptance.
  Discharged on the cited 2026-11-06 evidence → **closed on cited evidence**.
- `R5.47:withheld->GATE-G17` — the funded authentic original-runner parity tranche
  (R5.47, N=0 declared at GATE-G13, quote-only — never funded, never executed).
  Still carried → **O-3: permanently later/not-v1**.
- `R5.54:withheld->GATE-G17` — the publication stage (R5.54; voided at GATE-G16 —
  no publication evidence, none authorized). Still carried → **O-3: permanently
  later/not-v1**.
- `R5.55:withheld->GATE-G17` — the bounded adoption/value-observation follow-up
  (R5.55; voided at GATE-G16 — no adoption evidence, none authorized). Still
  carried → **O-3: permanently later/not-v1**.
- `P-layer->GATE-G17` — the voided publication/value-stage legs (GATE-G16/G17;
  BL-50's dated note: "carry no publication or adoption evidence and none was
  authorized"). Still carried → **O-3: permanently later/not-v1**.

### Per-row dispositions (40 rows — verbatim `owed_legs` cited per row)

**Closed on cited evidence — every carried leg is `R-layer->GATE-G17`, discharged
2026-11-06 (23 rows):**

| id | carried legs (verbatim) | disposition |
|---|---|---|
| REQ-B-101 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-B-103 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-B-105 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-B-106 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-C-004 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-C-005 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-C-105 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-C-106 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-001 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-002 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-003 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-007 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-009 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-101 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-102 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-103 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-104 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-105 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-106 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-107 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-109 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-D-110 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |
| REQ-E-011 | `R-layer->GATE-G17` | **closed — evidence-cited** (2026-11-06 set above) |

**Mixed legs — `R-layer` leg discharged on the cited evidence; the withheld leg
takes O-3 verbatim (12 rows):**

| id | carried legs (verbatim) | disposition |
|---|---|---|
| REQ-D-004 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-108 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** (matrix class `superseded-by` — alias of REQ-D-304, verdict inherited verbatim at the target row) |
| REQ-D-301 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-302 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-303 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-304 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-305 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-306 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-307 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-308 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-309 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |
| REQ-D-310 | `R5.47:withheld->GATE-G17;R-layer->GATE-G17` | R-layer leg **closed — evidence-cited**; `R5.47:withheld` leg **permanently later/not-v1 — O-3** |

**Voided-stage legs — all carried legs take O-3 verbatim (5 rows):**

| id | carried legs (verbatim) | disposition |
|---|---|---|
| REQ-G-201 | `R5.54:withheld->GATE-G17;P-layer->GATE-G17` | both legs **permanently later/not-v1 — O-3** |
| REQ-G-202 | `R5.54:withheld->GATE-G17;P-layer->GATE-G17` | both legs **permanently later/not-v1 — O-3** |
| REQ-G-206 | `R5.55:withheld->GATE-G17;P-layer->GATE-G17` | both legs **permanently later/not-v1 — O-3** |
| REQ-G-207 | `R5.54:withheld->GATE-G17;P-layer->GATE-G17` | both legs **permanently later/not-v1 — O-3** |
| REQ-G-209 | `R5.54:withheld->GATE-G17;P-layer->GATE-G17` | both legs **permanently later/not-v1 — O-3** |

### BL-50 closure counts (verbatim)

- **40 rows dispositioned — zero silently dropped.** 23 closed on cited evidence;
  17 rows carry remaining legs scoped out under O-3 verbatim (12 with a
  `R5.47:withheld` remainder, 5 fully voided-stage). 0 rows satisfied by the §8.x
  pass's own evidence (honest attribution — the closures cite the committed
  2026-11-06 discharges, not this pass).
- BL-50's backlog row (`docs/build/BACKLOG.csv`) carries the dated disposition note
  appended append-only; `status` `open → closed` under the dated disposition
  (ADR-0454: "BL-50's `status`/`landing` cells updated by dated disposition"). The
  33 re-homed ids (the 2026-11-06 reconcile's donors BL-51/BL-52/BL-53) stay on the
  row's `sources` cell verbatim — their routed homes are all discharged (DF-H9-1
  permanently unmet-by-decision, DONE 2026-11-06; DF-CAP5.1-1 DONE 2026-11-05;
  DF-R5.1-1 DONE 2026-11-05). **No Round-5 record is rewritten** — the matrix's
  PARTIAL verdicts stand verbatim; this record is the disposition, not a
  re-verdict.
- Machine-readable consequence, recorded verbatim (never silently repaired):
  `check-backlog.sh` over `docs/build/reports/round5/` gathers by the matrix's raw
  PARTIAL verdicts and cannot see this record's dispositions — post-closure it
  reports the 73 homed ids as `home-closed`. That posture is the expected O-3
  aftermath; a `resolved`-class amendment to the checker's invariant is
  guard-owner scope, surfaced at GATE-G37.

## Carried debt — verbatim (typed and rowed, never scoped out by this record)

The O-3 mint dispositions the routed-set remainder and BL-50's carried set only.
The round's live deferrals carry forward OPEN, verbatim (deferrals-open.sh post-
closure re-read): **DF-R7.1-2** (V3/V4/V5 activation evidence
`unmeasured{evidence_absent}` — needs a metric-bearing activation corpus or an
operator-directed probe window), **DF-R7.3-1** (refusal-bearing H-drive fold
semantics — an ADR is owed), **DF-R7.3-2** (attempt-3 settlement residuals —
H21 landed with compute `unavailable`; recorded estimates stand), **DF-R7.3-4**
(harvest `rroot` derivation leg (a) outstanding; leg (b) fails-closed repair
landed), **DF-R7.5-1** (driver/session artifacts not resume-safe), **DF-R7.5-2**
(no per-container memory bound — host-global OOM collateral), **DF-CAP7.1-1**
(`slot_result.cost_micros` undercounts late `cost.attributed`), **DF-CAP7.1-2**
(verify-cell podman name collision across resumes). Non-rowed carries:
**`pending{clock}`** — the P4-C ≥24 h post-teardown provider confirming read due
≥2026-11-27 ~07:26Z (HUMAN-H21 verbatim; a confirming check, teardown complete);
**the site-publication pin** — the §8.8 claims packet is not in
`docs/results/site-data` (a `hh-site-publication/1` mint is a publication act
outside this chain's authorization — R7.6 verbatim); **`compute-open.sh`
posture** — `p4 | p4c` lists the provision receipt without a `hh-compute-receipt/1`
teardown receipt (the VM was deleted operator-side under H21 ruling B ~07:26Z —
zero billable resources verified; the index's empty requirement is recorded, the
receipt never fabricated).
