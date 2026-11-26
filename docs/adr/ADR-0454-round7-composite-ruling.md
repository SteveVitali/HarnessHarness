# ADR-0454: Round-7 composite ruling — O-1 bounded §8.x second pass, post-pass O-3 permanent scope-out, BL-50 folded in

- **Status:** Accepted — recorded verbatim from the operator, 2026-11-20 (channel: operator CLI session)
- **Date:** 2026-11-20
- **Ticket:** Round-7 seed (manifest rows per `docs/tickets/00_MANIFEST.md` `## Plan extensions`); decided against `docs/build/planning/2026-11-19_round7-decision-memo.md` (+ its post-ADR-0453 addendum)
- **Requirement ids:** the 34-id later-pass routing register (CAP6.2); O-1 scope = XD-204…XD-225 (15, §8.1–§8.7 incl. XD-205's XD-124/XE-127 alias pair); O-3 scope = the remainder; BL-50's 40 carried matrix rows
- **Rung (ADR-0024 obligation):** an explicit non-contract note — this ADR governs experiments-program scope/disposition, not the canonical kernel contract ladder.

## Context

Round 6 closed with all 325 manifest rows landed, GATE-G32's recorded accept, and the W4x
extension (ADR-0453) folded under HUMAN-H20's *"all 69 cleared"* — five real arms frozen
(pi θ .62, claude-code θ .60, mini-swe-agent θ .58 ref, v5-corrected θ .40). CAP6.2's routing
register carried 34 requirement ids to a later pass; the decision memo offered O-1/O-2/O-3 and
named BL-50 as an operator decision. The operator ruled interactively on 2026-11-20.

## Decision (operator verbatim)

Verbatim: *"O1 approve, O-2 i'm not sure, O-3 approved, yes fold in BL-50. go"*

Recorded as a composite — O-1 defines the pass; O-3 defines the finality after it:

1. **O-1 approved — bounded §8.x second pass now.** A small Round-7 manifest targeting the §8.x
   cluster only (15 ids, XD-204…XD-225): exploration pre-registration (incl. the §8.3 W-rule
   derivation from non-outcome token records), a declared window envelope with its own gate,
   the frozen selection rule, and the §8.6 held-out confirmation under its §8.7 gate order.
   The §7.8 port-maturity cluster is not in this pass (memo: it waits for a dedicated pass —
   here, the O-3 disposition).
2. **O-2 not taken.** No deferred/dormant round — the pass happens now.
3. **O-3 approved — terminal scope-out after the pass.** On the O-1 chain's completion the
   operator's approval stands as the G32-style signature accepting the routed-set remainder —
   §7.8 (XE-002, XE-010…XE-013, XE-128…XE-132), §5.5 (XC-314, XC-316), §6.2/§6.7 (XD-013,
   XD-052), §2.5 (XA-126), §3.8 (XH-033), and any §8.x member the pass leaves unsatisfied —
   as permanently later/not-v1. The disposition record mints at closure with the per-id
   dispositions verbatim; PARTIAL ≠ scoped-out until that record lands.
4. **BL-50 folded in.** The Round-5 GATE-G17-carried research-layer scope carrier (40 matrix
   rows — R unmet predicates and voided P legs) is dispositioned inside Round-7's scope: rows
   the §8.x pass satisfies are closed by its evidence; the remainder take the O-3 disposition
   verbatim in the same closure record.

## Consequences

- New Round-7 manifest rows carry the work (decompose-spec `mode=extend`); no existing row is
  renamed or superseded; the §8.x pass's windows carry their own declared envelopes and gates —
  this ADR authorizes scope, not spend.
- The O-3 signature is **conditional on the pass landing**: if the O-1 chain cannot complete,
  the scope-out does not silently take effect — the chain reports its honest terminal state and
  the disposition re-presents.
- BL-50's fold-in does not re-open Round-5 records; dispositions are recorded in Round-7's
  closure record, with BL-50's `status`/`landing` cells updated by dated disposition.
- DF-R6.45l-1's named unblocker (the runtime-observed transitive host derivation — acceptance:
  retroactively emitting smoke1's three observed hosts unaided) rides the Round-7 manifest as
  its own derivation ticket per the memo's precondition note.
- Spend: any window the pass runs declares its envelope before first metered call under the
  standing ≤$50/day sign-off; withholds and typed absences are verbatim, never fabricated.

## Revisit trigger

Any proposal to extend the pass beyond the §8.x cluster (e.g., pulling §7.8 or §5.5 ids back into
scope), to run further rounds after the O-3 closure record lands, or to treat the O-3 signature
as effective before the pass's honest terminal state — then this ADR's scope must be re-ruled
explicitly.
