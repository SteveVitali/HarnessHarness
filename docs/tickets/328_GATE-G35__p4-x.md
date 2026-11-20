<!--
  Round-7 contract — experiments §8.x second pass (decompose-spec mode=extend, 2026-11-20).
  Scope ruling: docs/build/readouts/GATE-G34.md + docs/adr/ADR-0454-round7-composite-ruling.md.
  GATE-G35 is a normal gate, presented to the operator — NOT pre-answered. Registration
  authorizes no spend; this readout is where the X0+X1 spend authorization is requested.
  This contract CITES the spec; it does not copy a design. Executed contracts are frozen.
-->
# GATE-G35 — G-P4-X: authorize the phase-4 exploration window (X0 smoke + X1 OFAT sweep)

- **Sequence:** 328 of 338 · **Phase:** Round 7 · **Kind:** gate
- **Block:** P4-X · **Alias:** GATE G-P4-X
- **base_branch:** current checkout
- **Depends on:** R7.2
- **Readout:** `docs/build/readouts/GATE-G35.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

REQ-XD-221 (verbatim):

> | **REQ-XD-221** | Phase-4 spend SHALL be authorized at G-P4-X (X0 + X1) and G-P4-C (confirmation), recorded verbatim, outside the $60 pilot authorization. | Gate entries precede subjects. · `AC-8.7-2` · D |

REQ-XH-004 (verbatim):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

This authorization is **outside the $60 pilot authorization** and outside the first-pass
compute accounting — a separately recorded model + compute authorization.

## What the readout must contain (pre-registered — every element present before presentation)

1. **The frozen exploration pre-registration** — R7.2's packet hash, the registry hash and W
   verbatim, the withheld-level set (`withheld{inert_predicted}` members), and the frozen
   selection rule's hash.
2. **The declared envelope** — X0 = 3 dev tasks × 1 × all arms; X1 = OFAT (base + single
   perturbations) on the 11 band-A development tasks at k = 5, seeded schedule in 5 rounds
   (§8.4: 550 slots); the arm list = the HH-native baseline plus every non-withheld registry
   level.
3. **The quote** — minted by the one quote path; the §10.4 phase-4 planning bounds recorded
   (X0+X1 model ≈ $28–$187, planning ceiling ≈ $337; compute share of the phase-4 row);
   the hard-cap formula verbatim.
4. **The model authorization** and **the compute authorization**, each recorded verbatim and
   separately (REQ-XH-004), with the standing ≤$50/day sign-off cited where it applies.
5. **G-L1 evidence** — a G-L1 pass id for the exact H swept (`l1-pass:…` bound to the frozen
   `h_identity.digest`); exploration follows the recorded L2 verdict and never runs
   concurrently with L2 (REQ-XD-220's later-pass leg); any H change starts a new cohort.
6. **Stop rules** — the §8.4 declared set verbatim (budget, infrastructure, integrity,
   catalog identity); exclusivity + kill discipline per the established window posture.
7. **The disposition ladder** — `run | not_run{…} | re-scope{…}` with the §6 re-scope forms.

## The window this gate would authorize

R7.3's X0+X1 execution under the declared envelope — bodies stay skeleton until this gate
records PASSED; the fleshed body carries verbatim: slots, arms, seeds, ceilings, receipts
paths, teardown + settle + archive legs (the R6.45x skeleton→flesh pattern).

## Dispositions (pre-registered)

- `PASSED — run` — the operator authorizes the declared envelope verbatim.
- `PASSED — re-scope{…}` — a narrowed envelope recorded verbatim in the readout.
- `NOT PASSABLE` / `FAILED` — recorded with basis; the skeleton stays a skeleton.

## Operating clauses

Follow the [manifest operating rules](00_MANIFEST.md#operating-rules). No ticket authorizes
spend; the readout is the authorization record. Secrets never — `provided: yes/no` only.

## DEFERRALS rule

A failed prerequisite is `blockedOn` with a dated note, never a fabricated pass.
