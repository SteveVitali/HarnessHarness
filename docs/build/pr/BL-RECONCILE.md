BL-50…BL-53 reconcile — honest backlog maintenance (`reconcile-build mode=backlog` class; non-manifest records-in work, USD $0.00).

Reconciles the four stale backlog rows opened at REC5.1 (2026-11-05) against the evidence that landed after them. No code, no verdict fabrication — every status change cites committed artifacts.

- **BL-52 → closed, satisfied** — its named exit met: executed `hh-s1-useful/1` packet `reports/2026-11-05-df-cap5.1-s1-useful.json` (`disposition: executed`, all 7 legs verified, `useful_acceptance: marked`) + independent Decided artifacts `…-s1-decided-artifacts.json` (both subjects `decided` `reward_ppm 1000000`); DF-CAP5.1-1 DONE (operator-authorized re-execution, PR #280).
- **BL-53 → closed, satisfied** — `reports/2026-11-05-df-r5.1-evidence.json` (`hh-guard-boundary-evidence/1`, EV-1…EV-6 verified: all 67 work-row dispatch boundaries carry guard pre+post `PASS DISPATCH`); DF-R5.1-1 DONE.
- **BL-51 → closed, scope-dispositioned — never satisfied** — DF-H9-1's verbatim operator disposition (2026-11-05T16:04:15Z, **permanently unmet-by-decision**: no access to outside humans) is the fresh scope disposition its exit names; participant/outside-reviewer predicates honestly unmet, never waived.
- **BL-50 → stays open, dated note** — R-layer legs discharged by real evidence (parity verified 30/30 tranche-3; S4 executed 300/300 over four epochs; efficacy computed, honest null-equivalent; GATE-G19 PASSED; study `reported`, `unmet{s1_failed}` verbatim); P-layer legs (REQ-G-201/202/207/209 publication + REQ-G-206 value-observation) still voided/carried — no publication or adoption evidence exists and none was authorized.
- **Dedup kept** — the 33 still-PARTIAL matrix ids of the three closed rows re-home to BL-50's `sources` cell (BL-12/BL-21 convention): all 73 round-5 PARTIAL ids sit on the one open carrier exactly once. CAP5.1-era matrix verdicts are not re-verdicted by this records-only reconcile — that pass is owed to the next capstone.

Verify: `deferrals-open.sh` empty (0 open DF) · `--bl` → BL-50 only · `check-bm-tools.sh` OK · `check-backlog.sh` exit 0 both modes (206 + 73 expected) · `check-build-memory.sh` 0 violations (8-warning baseline) · supplement guard PASS, route HOLD (round landed — no dispatch route exists). LEDGER PHASE LOG entry 1,878 B. No deferrals opened or closed. Merge policy: OPERATOR.

*Correction — 2026-11-07 (R6.3 · REQ-XA-114 / AR-03):* "parity verified
30/30" above is the tranche's own exact-agreement verdict (`tb` ↔ a
scratch reference runner, R=1 — `parity_admission` refuses the design:
`MissingKey{pins.rights_ref}` / `AdapterFloor{repeats:1}`); "efficacy
computed, honest null-equivalent" is 0 by construction (oracle
participant — arms cannot change outcomes), not efficacy evidence. The
row dispositions stand.
