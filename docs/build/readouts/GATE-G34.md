# GATE-G34 readout — Round-7 scope composite ruling (O-1 pass + post-pass O-3 scope-out + BL-50)
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> Decision gate. The Round-7 manifest executes the §8.x pass; the permanent scope-out
> disposition mints at the pass's honest terminal state. Agents never sign, tick or infer approval.

## Criterion (verbatim)

Memo `docs/build/planning/2026-11-19_round7-decision-memo.md` (+ post-ADR-0453 addendum):
the operator picks O-1 (bounded second pass, §8.x cluster only — 15 ids XD-204…XD-225, declared
window envelope) / O-2 (deferred round) / O-3 (formally accept the routed set as permanently
later/not-v1 — a G32-style signature act), and dispositions BL-50 (Round-5 GATE-G17-carried
research-layer scope carrier, 40 matrix rows).

## Ruling

Operator verbatim (interactive, Devin CLI session, 2026-11-20):

> "O1 approve, O-2 i'm not sure, O-3 approved, yes fold in BL-50. go"

Recorded as a composite — **O-1 defines the pass; O-3 defines the finality after it:**

1. **O-1 approved** — a small Round-7 manifest targeting the §8.x cluster only (15 ids):
   exploration pre-registration incl. the §8.3 W-rule derivation from non-outcome token
   records, a declared window envelope under its own PROV gate, the §8.5 frozen selection
   rule, and the §8.6 held-out confirmation under §8.7's gate order (G-P4-C before any
   confirmation subject). §7.8 port-maturity is not in this pass.
2. **O-2 not taken** — no deferred/dormant round.
3. **O-3 approved, conditional** — on the O-1 chain's honest terminal state, this ruling is
   the signature accepting the routed-set remainder (§7.8: XE-002, XE-010…XE-013,
   XE-128…XE-132; §5.5: XC-314, XC-316; §6.2/§6.7: XD-013, XD-052; §2.5: XA-126; §3.8:
   XH-033; plus any §8.x member the pass leaves unsatisfied) as permanently later/not-v1.
   The disposition record mints at closure; PARTIAL ≠ scoped-out until it lands. If the
   pass cannot complete, the scope-out does not silently take effect — the ruling
   re-presents.
4. **BL-50 folded in** — its 40 carried matrix rows are dispositioned inside Round-7 scope:
   rows the §8.x pass satisfies close on its evidence; the remainder take the O-3
   disposition verbatim in the same closure record. No Round-5 record is rewritten.

## Scope

- Authorizes the Round-7 manifest's **registration and the offline legs** (pre-registration,
   W-rule derivation, selection-rule freeze, DF-R6.45l-1's derivation ticket, BL-50
   disposition record, O-3 closure record).
- Spend: every window the pass runs declares its own envelope and gate before first metered
   call (same posture as every prior window); this ruling does not pre-authorize any quote.
- Carried: DF-R6.45l-1 rides as its own derivation ticket (acceptance: retroactively emitting
   smoke1's three observed hosts unaided).

Recorded in `docs/adr/ADR-0454-round7-composite-ruling.md`. nextTicket → the Round-7 manifest
rows (registered by decompose-spec `mode=extend`).
