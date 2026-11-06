# GATE-G19 — Rule the efficacy-inference admission over the completed cohort · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim)

> Fixed sample/final analysis; integrity/treatment/cap/isolation/leakage/drift/ceiling failure stops and retains partial results/costs. No favorable peeking/top-ups or retrospective smaller manifest. Feasibility/precision failure leaves objective unmet with owner/trigger, not waived or null.

## Pre-registered thresholds (verbatim)

The ruling admits exactly one efficacy-inference basis for R5.59, on the **real post-completion state** (the R5.58 `hh-s4-completion/1` record: executed/inconclusive verbatim per slot — the ~290/300 executed ceiling given `fix-git`'s permanent `upstream_deleted`):

- **complete-cohort** — the registered finite-manifest efficacy contrast (REQ-D-107/109, §4.3 machinery admitted at GATE-G9) computes only when every one of the 300 cohort slots carries a real receipt; any permanent inconclusive residue leaves the efficacy inference `unavailable{incomplete_cohort}` and S4 honestly partial — no efficacy verdict is produced and nothing is inferred from the executed subset.
- **bounded-inference** — the registered contrast computes over the executed base with missingness declared verbatim per slot (which slots, which reason codes, which tasks/arms/rounds they sit in); the residual is declared, never dropped or silently shrunk — never a retrospective smaller manifest; unavailable members stay inside the frozen multiplicity family per §4.3/§3.2 and the verdict's label names its basis.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Frozen-run confirmation cohort — `docs/build/reports/2026-11-05-s4-confirmation.json` (`hh-s4-confirmation/1`, `partial` — 160/300 executed, 140 inconclusive verbatim) — read 2026-11-06T22:04Z — layer: live-executed
- Egress supplement — `docs/build/reports/2026-11-05-s4-egress-cohort.json` (`hh-s4-egress-supplement/1`, `partial` — 90/140 executed, 50 inconclusive verbatim) — read 2026-11-06T22:04Z — layer: live-executed
- GCE completion epoch — `docs/build/reports/2026-11-06-s4-completion.json` (`hh-s4-completion/1`, `partial` — 20/50 executed, 30 inconclusive verbatim) — read 2026-11-06T22:04Z — layer: live-executed
- 30-slot repairs epoch — `docs/build/reports/2026-11-06-s4-30slot-repairs.json` (`hh-s4-30slot-repairs/1`, `executed` — 30/30 executed, 0 inconclusive) — read 2026-11-06T22:04Z — layer: live-executed
- Parity tranche — `docs/build/reports/2026-11-05-parity-tranche-3.json` (`hh-parity-tranche-report/1`, `verified` 30/30, zero discordance) — read 2026-11-06T22:04Z — layer: live-executed
- Combined cohort state — `docs/build/runs/S4-30SLOT-REPAIRS.md`: **300/300 slots carry a real receipt** (220 resolved / 80 unresolved), 60/60 first subjects, one `protocol_id` `9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f` across all four epochs — read 2026-11-06T22:04Z — layer: live-executed
- The executed ledger — `docs/build/runs/S4-CONFIRMATION.md` / `S4-EGRESS.md` / `S4-COMPLETION.md` — read 2026-11-06T22:04Z — layer: live-executed

<!-- agent-drafted:begin sha256=13471dbdc0e4fe1f864cc83c51137dee115278f3f63262bdd81c93c702cfaf46 -->
## The presented state (executed record verbatim — not a projection)

The cohort is **complete**: all 300/300 planned slots carry a real
receipt across four named epochs under the frozen protocol
(`protocol_id 9edc5327…`, seed `parity-tranche-2026-11-05:seed:v3`,
never re-picked):

| epoch | slots | executed | inconclusive | first subjects |
|---|---|---|---|---|
| frozen run (`hh-s4-confirmation/1`) | 300 | 160 | 140 | 32/60 |
| egress supplement (`hh-s4-egress-supplement/1`) | 140 | 90 | 50 | 18/28 |
| s4-completion-gce (`hh-s4-completion/1`) | 50 | 20 | 30 | 4/10 |
| s4-30slot-repairs-gce (`hh-s4-30slot-repairs/1`) | 30 | 30 | 0 | 6/6 |
| **combined** | **300** | **300** | **0** | **60/60** |

Receipt verdicts: 220 resolved / 80 unresolved (verifier-decided, never
manufactured); no residue — the `fix-git` `upstream_deleted` ceiling the
pre-registered note assumed was lifted by the vendored-mirror repair.
Every prior packet stands byte-untouched.

Under the ruling's own terms the **complete-cohort** basis is now
satisfiable: every one of the 300 cohort slots carries a real receipt,
so the registered finite-manifest efficacy contrast (REQ-D-107/109,
§4.3 machinery) computes on the full cohort with no declared residue.
The bounded-inference basis is unnecessary — there is no missingness
left to declare.

### Disposition requested

Your verbatim decision, UTC, channel: PASSED (admits complete-cohort —
R5.59 computes the contrast over the full 300/300 cohort) /
SKIPPED-BY-OPERATOR / NOT-PASSABLE (+what would pass). Name any carried
unmet predicates and compensating controls. Confirm the agent-drafted
block by its sha256 prefix. A pending or failed gate stops dependent
dispatch (R5.59 withholds until G19 passes).
<!-- agent-drafted:end -->

## Signature

**PASSED** — operator words (verbatim, interactive, 2026-11-06T22:04Z): "confirmed and proceed" — recorded verbatim in GATE DECISIONS and this signature; the admitted basis is **complete-cohort** — compute the efficacy verdict on the full 300/300 executed cohort (the pre-registered `fix-git` residue was lifted by the recorded repairs, so no missingness declaration remains). Agent-drafted block sha256 `13471dbdc0e4` confirmed by the approval.
