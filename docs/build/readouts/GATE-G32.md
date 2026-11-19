# GATE-G32 readout — Round-6 acceptance
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> Milestone gate. The chain STOPS until the operator dispositions the list for signature. Agents
> never sign, tick or infer approval; old acceptance (GATE-G17) never substitutes.

## Criterion (verbatim)

§10.3 "GATE-ACCEPT (tail)": Round-6 sums from billed figures where they exist, including the $100
check (REQ-XH-110); accepted deviations. REQ-XH-110: Σ authorized ≤ $90; Σ(model billed else
settled else metered-orphans-bounded.hi; compute billed else estimated; other) ≤ $100, billed
figure wherever a bill exists; readouts cite Q-X47 and Q-X54 verbatim.

## The packet (sha256-pinned)

- CAP6.1 `COVERAGE_MATRIX.csv` `3e396617…` · `CAPSTONE_GAP_ANALYSIS.md` `62611499…` ·
  `COMPOSED_E2E_REPORT.md` `8c95413a…` · `hh-cap6-gap-hunt/1` `2026-11-19-cap6.1-gap-hunt.json`
- CAP6.2 `CAPSTONE_CLOSURE.md` `3861bffd…` (+ prereg-hash-drift / req-xc-325-unexecuted /
  register-normalization records, each sidecar-pinned)
- CAP6.3 spec reconciliation + integration plan (`docs/build/runs/CAP6.3.md` + committed JSON
  reads; merge dry-run 13/13 clean)

**Two sums:** engineering closed **410/456** (requirement satisfied: 0 MISSING; 46 PARTIAL — 34
routed, 5 discharged, 4 scope-ended GATE-G30, 1 unexecuted XC-325, 2 aliases). Spend: **$4.52
model billed** (H18, console+API agree) + **$27,367,214 µ$ compute estimated** (21 committed cost
records; GCP billed `unavailable{no_billing_export}`) = **$31.887214 ≤ $100**; Σ authorized ≤ $90
holds (the $30 compute cap + model envelope). Highest layer: `signed` (operator acts) /
`engineered` (records); live claims gated.

**Live reads at presentation:** stack CI read — #407–#413 pass, #414–#418 FAIL on the
check-ci-gating docs/build-literal leg (diagnosed; repair = maintainer decision I-3); Ollama
$4.52 (API, 04:2xZ); dataset `public` @ a11591e9 (04:23Z); project `DELETE_REQUESTED` (R6.47,
02:01:58Z); compute-open empty exit 0.

## The list for signature — your per-row disposition (ACCEPTED / SEND-BACK + what would accept)

**(a) Accepted deviations** — CAP6.1 §6 + CAP6.2's register: W1-archive-as-W2-first-step (SR-18);
consolidated reconcile placement (REQ-XH-008 interpretation); re-partition + tail-split
departures from §10.6 (minimal / 4-run tail vs `tail=full`; RECIPE a/b merged; row count below
first-cut band); prereg `admission_sha256` drift (dated deviation `fp-prereg-hash-drift.md`,
frozen records untouched); `fp-seal-open` byte-identical FAIL (same binding).

**(b) Scoped-out owed legs (owner + trigger)** — every `withheld{…}` claim verbatim: arms
`withheld{window_deadline}` (mini-swe-agent, hh_native), `withheld{budget}` (v5),
`withheld{admission:AG3}` (pi, claude-code), claims `protected_pair_truncated{k<5,R<5}`; BL-50
(Round-5 publication/value-observation carried scope — owner operator, trigger: a publication
decision for that round); operator-disk retention decision (REQ-XH-106) — **unrecorded; needs
your words now or it stays open**.

**(c) Waivers requested — each names an ADR quoting you:**
1. REQ-XC-325 R-replay/SM-* `unexecuted{no_executor}` — waive or schedule
2. Ws1 estimate-over-ceiling `5,250,954 µ$` vs declared `4,136,280 µ$` (~127%;
   crash/recovery + ~24h) — accept the overage or reject to closure
3. Two-fold compute-estimate variance (`30.024544` interim vs `27.367214` authoritative —
   "both ≤$30" is NOT claimed) — accept the fold record as-is
4. AR-08 + AR-10 never minted (R6.10 item 12) → **R6A-2** mint-or-waive
5. Stack CI red span #414–#418 — repair decision I-3 (check-ci-gating allowlist vs records
   relocate) before Phase B merges

**Merge ordering (recommends, never merges):** Phase A `407→408→409→410→411→413→412`; Phase B
`414→415→416→417→418→419` after I-3.

An `inconclusive`/`withheld` first-pass outcome is acceptable as recorded; no threshold moved.

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Operator:** Steve Vitali · **UTC:** 2026-11-19T06:15Z · **Channel:** operator CLI session.
- **Verbatim rulings** (in the operator's own words, applied per the readout's stated semantics —
  a waiver = operator-accepted deviation, recorded verbatim, never marked satisfied):
  - **(a) accepted deviations + (c) waivers 1–4:** "I'm okay with these deviations and want to
    proceed with them" — ACCEPTED: SR-18, reconcile placement, re-partition/tail-split, prereg
    pin drift, seal-open byte-identical; waivers: REQ-XC-325 `unexecuted{no_executor}` WAIVED,
    Ws1 estimate-over-ceiling ACCEPTED (5,250,954 vs 4,136,280 µ$ recorded verbatim), two-fold
    compute-estimate variance ACCEPTED as folded, AR-08/AR-10 unminted WAIVED (R6A-2 records the
    operator quote).
  - **(c)5 CI repair (I-3):** "Fix with allowlist" — the check-ci-gating docs/build-literal span
    repairs via allowlist entries (same class as the 25 existing fp_select.rs entries); repair
    rides its own PR, gates Phase-B merges.
  - **Operator-disk retention (REQ-XH-106):** "don't delete evidence if it's our only copy of it
    and we might need it later" — RETAIN the retained harvest trees.
  - **(b) owed legs:** all `withheld{…}` tokens + BL-50 carried verbatim as recorded.
- **Decision:** **PASSED — Round-6 accepted as recorded.** Every row dispositioned; none left
  unsigned. `accepted_scope` set ⇒ verdict ≤ MET-ENGINEERED holds; routed ids accepted as
  routing. An `inconclusive`/`withheld` first-pass outcome stands as recorded.
- Layer: `signed` — operator act; expires/voids only by a later recorded operator decision.
