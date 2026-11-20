<!--
  Round-7 contract — experiments §8.x second pass (decompose-spec mode=extend, 2026-11-20).
  Scope ruling: docs/build/readouts/GATE-G34.md + docs/adr/ADR-0454-round7-composite-ruling.md.
  GATE-G36 is a normal gate, presented to the operator — NOT pre-answered. This readout is
  where the confirmation spend authorization is requested (hash-before-subjects).
  This contract CITES the spec; it does not copy a design. Executed contracts are frozen.
-->
# GATE-G36 — G-P4-C: authorize the held-out confirmation window (baseline re-run + selected members)

- **Sequence:** 331 of 338 · **Phase:** Round 7 · **Kind:** gate
- **Block:** P4-C · **Alias:** GATE G-P4-C
- **base_branch:** current checkout
- **Depends on:** R7.4
- **Readout:** `docs/build/readouts/GATE-G36.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

REQ-XD-221 (verbatim):

> | **REQ-XD-221** | Phase-4 spend SHALL be authorized at G-P4-X (X0 + X1) and G-P4-C (confirmation), recorded verbatim, outside the $60 pilot authorization. | Gate entries precede subjects. · `AC-8.7-2` · D |

REQ-XD-217 (verbatim):

> | **REQ-XD-217** | The confirmation pre-registration SHALL list the §8.6 members and SHALL be frozen and gated (G-P4-C) before any confirmation subject. | Hash precedes subjects. · `AC-8.6-3` · D |

REQ-XH-004 (verbatim):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

## What the readout must contain (pre-registered — every element present before presentation)

1. **The frozen confirmation pre-registration** — hash + member list verbatim (baseline
   re-run + selected members, ≤ m_max, plus a declared C₁₂ only if `combine` fired),
   minted by R7.4 **before any confirmation subject**.
2. **The selection-packet hash** with its recorded predating proof vs the prereg mint —
   the §8.7 ordering evidence.
3. **The declared envelope** — the band-A held-out census T = 37 (or the pre-declared
   A ∪ B census of 54), k = 5, seeded schedule, ~555 slots at m = 2 (§8.6/§10.4).
4. **The confirmation quote** — R7.4's output verbatim (selection-conditioned precision;
   X0/X1-measured $/trial; planning ceiling ≈ $340 + compute share).
5. **The model authorization** and **the compute authorization**, each verbatim and
   separately (REQ-XH-004), outside the pilot authorization (REQ-XD-221).
6. **No-baseline-reuse attestation** — no L2 or phase-2 baseline data is reused; the
   baseline re-runs inside this window (REQ-XD-215; the AR-33 shared-baseline departure
   does not extend here).
7. **The `no_candidate` path** — if the frozen rule selected no member, the readout
   presents that state; no subject envelope is authorized.
8. **The disposition ladder** — `run | not_run{…} | re-scope{…}` with the §6 re-scope forms.

## The window this gate would authorize

R7.5's held-out confirmation under the declared envelope — skeleton until PASSED; the
fleshed body carries verbatim: member list, census, k, seeds, ceilings, receipts paths,
teardown/settle/archive legs.

## Dispositions (pre-registered)

- `PASSED — run` — the operator authorizes the declared envelope verbatim.
- `PASSED — re-scope{…}` — a narrowed envelope recorded verbatim in the readout.
- `NOT PASSABLE` / `FAILED` — recorded with basis; the skeleton stays a skeleton.

## Operating clauses

Follow the [manifest operating rules](00_MANIFEST.md#operating-rules). No ticket authorizes
spend; the readout is the authorization record. Hash-before-subjects is a hard order — a
readout presented after any confirmation subject record exists fails the ordering check.

## DEFERRALS rule

A failed prerequisite is `blockedOn` with a dated note, never a fabricated pass.
