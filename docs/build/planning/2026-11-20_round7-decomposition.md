# Round-7 decomposition — §8.x exploration/confirmation second pass (decompose-spec mode=extend)

2026-11-20 · worktree `/tmp/hh-r6.53-wt` on `codex/harnessharness-r6.53` · coordinator reviews and commits.

## Authorization

Operator composite ruling at [GATE-G34](../readouts/GATE-G34.md) (PASSED, 2026-11-20),
recorded as [ADR-0454](../../adr/ADR-0454-round7-composite-ruling.md) (Accepted). Verbatim:
`"O1 approve, O-2 i'm not sure, O-3 approved, yes fold in BL-50. go"`. The ruling authorizes
the Round-7 manifest's registration and its offline legs; **no spend, provisioning,
publication, or merge is authorized by registration** — each execution window declares its
envelope up front and stops at its own gate before any metered call.

## Scope (what Round 7 is and is not)

- **O-1 (approved):** a bounded second pass over the §8.x exploration/confirmation cluster —
  the 18 §8.x ids CAP6.2's routing register (`docs/build/reports/round6/LATER_PASS_ROUTING.md`)
  routed to a later pass: REQ-XD-204, REQ-XD-205 (carrying the REQ-XE-127 merged predicate),
  REQ-XD-206…REQ-XD-219 (minus REQ-XD-220, whose first-pass leg R6.43 owns and whose later-pass
  leg the windows re-run), REQ-XD-221, REQ-XD-225, REQ-XD-208, REQ-XD-209.
  *Count reconciliation:* ADR-0454 writes "15, §8.1–§8.7". The register's routed §8.x set is 18
  ids (the contiguous XD-204…XD-225 range minus first-pass-owned REQ-XD-220/222/223/224). The
  ruling's 15 is the §8.3–§8.7 subset; the pass also carries §8.1's REQ-XD-225 (tier-2
  admission) and §8.2's REQ-XD-208/REQ-XD-209 (activation fold + inert forecast) because the
  sweep cannot run without them — all 18 are the register's routed members of the same cluster.
  Any §8.x member the pass leaves unsatisfied joins the O-3 remainder set verbatim.
- **O-2 (not taken):** no dormant/deferred round — no manifest rows minted for it.
- **O-3 (approved, conditional):** the permanent scope-out record mints **at the pass's honest
  terminal state** (CAP7.2), verbatim per id: §7.8 REQ-XE-002, REQ-XE-010…XE-013,
  REQ-XE-128…XE-132; §5.5 REQ-XC-314, REQ-XC-316; §6.2/§6.7 REQ-XD-013, REQ-XD-052; §2.5
  REQ-XA-126; §3.8 REQ-XH-033; plus any §8.x member left unsatisfied. If the pass cannot reach
  an honest terminal state, the scope-out does **not** take effect — CAP7.2 records the state
  verbatim and the ruling re-presents at GATE-G37 (ADR-0454 consequence).
- **BL-50 (folded in):** the 40 GATE-G17-carried Round-5 matrix rows are dispositioned at
  closure — rows the §8.x pass satisfies close on its evidence; the remainder take the O-3
  disposition verbatim in the same closure record. No Round-5 records rewritten.
- **Register/lead-token reconciliation:** MAINTAINING.md's round-closure rule — re-sweep
  DEFERRALS lead tokens (DF-CAP6.1-1…5, DF-R6.45l-1, DF-R6.50-1, DF-R6.53-1 show stale `OPEN`
  leads whose dated dispositions are DONE/discharged/waived), machine-readable pending sets,
  backlog state, revisit triggers. Append dated corrections; never rewrite history.
- **DF-R6.45l-1:** GATE-G34/ADR-0454 assumed its derivation ticket rides Round 7. The dated
  record shows the work already landed — `321a_R6.51__verify-host-derivation.md` (closure
  insert, BUILD_INDEX row 321a, PR #413, `transitive verify-host derivation from captured
  runtime evidence emits exactly the three smoke1-observed hosts unaided; DF-R6.45l-1 DONE`;
  DEFERRALS dated marker DONE 2026-11-19 R6.51). PR #413 is **not** an ancestor of this chain,
  so the machinery rides main, not this branch — an integration fact, not owed work. **No
  derivation ticket is minted;** CAP7.1 verifies the dated discharge stands and CAP7.2 records
  it + reconciles the stale lead token. *Deviation from the suggested partition, item 7 —
  the historical landing makes a new derivation ticket a fabrication.*

## Partition (13 rows, 326–338)

| # | row | kind | scope |
|---|---|---|---|
| 326 | R7.1 | ticket | Level-admissibility machinery (offline): the pass's frozen `sweep-registry` record (new exploration identity; reuses R6.43's shape/validator — REQ-XD-201/202 machinery consumed, never re-owned), seal the remaining Tier-1 levels + declared lane/compaction composite levels through R6.43's enumerate→seal path, tier-2 admission check (REQ-XD-225), effort lane parameter + proxy pin + applied-vs-declared ineligibility (REQ-XD-204), the W-rule derivation from non-outcome token records incl. task-conditioned-cap/padding refusals (REQ-XD-205 + REQ-XE-127), §4.2-generic activation fold (REQ-XD-208), inert forecast `withheld{inert_predicted}` (REQ-XD-209) |
| 327 | R7.2 | ticket | Exploration pre-registration + frozen selection machinery (offline): declared X0/X1 design (REQ-XD-206), declared stop rules (REQ-XD-210), exploration-packet label machinery (REQ-XD-207), frozen selection rule (REQ-XD-211), deterministic selection packet with input-manifest exclusion and hash-predates rule (REQ-XD-212), optimism labels (REQ-XD-213), combination admissibility (REQ-XD-214); emits the frozen pre-registration (registry hash + W + frozen rule) |
| 328 | GATE-G35 | gate | **G-P4-X** — authorize the X0+X1 exploration envelope; REQ-XD-221 leg 1; STOP |
| 329 | R7.3 | skeleton | X0+X1 exploration window body — fleshes only after GATE-G35 PASSED |
| 330 | R7.4 | ticket | Post-X1 fold (offline): exploration packet emit, selection packet minted (hash predates the confirmation pre-registration), confirmation pre-registration minted + frozen (REQ-XD-217), confirmation quote (REQ-XD-218) |
| 331 | GATE-G36 | gate | **G-P4-C** — authorize the confirmation window; hash-before-subjects; REQ-XD-221 leg 2; STOP |
| 332 | R7.5 | skeleton | Held-out confirmation window body — fleshes only after GATE-G36 PASSED |
| 333 | R7.6 | ticket | Confirmation fold + claims packet (offline): Holm estimation (REQ-XD-216), null/inconclusive dispositions (REQ-XD-219), §8.8 two-statement claims under the first-pass machinery's checks (REQ-XD-222/REQ-XD-224 contributing) |
| 334 | HUMAN-H21 | human | Round-7 billing/settlement reconciliation — one dated entry per window ≥ 24 h post-teardown (H18 pattern) |
| 335 | CAP7.1 | capstone | Pass terminal-state gap analysis: per-id verdicts over the 18 §8.x ids; the pass's honest terminal state; DF-R6.45l-1 discharge verification |
| 336 | CAP7.2 | capstone | Closure: O-3 disposition record mint (per-id verbatim, conditional), BL-50's 40 rows dispositioned, lead-token/register reconciliation, DF-R6.45l-1 recorded discharge |
| 337 | GATE-G37 | gate | Round-7 acceptance — terminal state + minted O-3 record + BL-50 dispositions + reconciliation verified; STOP |
| 338 | DOC7.1 | docs | Docs refresh over landed Round-7 evidence (§8.x site/status surfaces, ledger/index bookkeeping); the §8.8 claims checks hold |

## Decisions and justifications

1. **Two offline machinery legs, not one.** R7.1 (admissibility: registry + sealing + W-rule +
   activation/forecast) and R7.2 (pre-registration packet + selection machinery) share the
   registry-hash/W/level seam but split at the spec's own boundary (§8.1–§8.3 admissibility vs
   §8.4–§8.5 design+selection). A single ticket would exceed the ≲100k subagent working-set
   ceiling (the W-rule derivation alone reads the metered-record corpus).
2. **Two windows, two gates.** §8.7's order is load-bearing: G-P4-X authorizes X0+X1;
   selection runs on committed X1 records; the confirmation pre-registration + G-P4-C
   precede any confirmation subject (REQ-XD-217 hash-before-subjects, REQ-XD-212's
   hash-predates rule). Merging the gates would collapse the selection→confirmation seam the
   spec gates on. This matches the suggested partition's G35/G36 split.
3. **Skeletons declare envelopes up front** (slots, arms, ceilings — cited from §8.4/§8.6/§10.4)
   and flesh only after their gate — the R6.45x pattern; gates carry GATE-G25-style PROV
   posture (declared envelope, verbatim authorizations, recorded dispositions).
4. **No bootstrap gate.** GATE-G34 is Round 7's bootstrap (registration + offline legs
   authorized); the first chain row is the offline machinery ticket.
5. **Minimal tail declared in the banner** (`tail=minimal`), matching the operator's Round-6
   tail decision — contiguous CAP7.1, CAP7.2, GATE-G37 (accept), DOC7.1. The acceptance gate
   is the natural re-presentation point for the O-3 ruling when the pass cannot complete
   (ADR-0454's consequence clause).
6. **No separate DF-R6.45l-1 derivation ticket** — see the scope section; the dated DONE
   discharge (R6.51/PR #413) is recorded at CAP7.1/CAP7.2 instead of fabricating new work.
7. **No register rewrite.** `LATER_PASS_ROUTING.md` and the coverage JSON are append-only
   Round-6 records; the Round-7 rows cite them and the manifest's ownership subsection maps
   the routed ids to implementation rows without touching the register.
8. **HUMAN-H21** consolidates the per-window billing/settlement reads (H18 pattern) — the only
   operator act the windows add; account setup, billing alert and credentials are already
   provisioned under H12–H14's landed state.

## Phase-4 adversarial review (self-review record — no fresh-context subagent available)

Re-read discipline over spec §8.1–§8.8, §10.1.6, §10.4, §10.6, the routing register, and the
contracts as written; findings and dispositions:

- **F1 (fixed):** early draft dropped REQ-XD-208/REQ-XD-209/REQ-XD-225 trusting the ruling's
  "15". The register's routed §8.x set is 18; all three are cluster prerequisites (X1 cannot
  run the sweep without activation/forecast; the registry record needs the tier-2 rule).
  All 18 are now covered; the count reconciliation is recorded above.
- **F2 (fixed):** draft had the confirmation pre-registration mint inside the fold ticket but
  no row emitting it *before* the gate — REQ-XD-217 requires frozen-and-gated before subjects,
  and REQ-XD-212 requires the selection-packet hash to predate it. R7.4 now mints both in
  order; GATE-G36 verifies the hashes precede subjects.
- **F3 (fixed):** draft placed HUMAN-H21 between the window and the fold; the fold doesn't
  depend on billing reads and a held human row would stall honest offline work. Moved after
  the fold (row 334); CAP7.2 cites its entries or records `pending{clock}` honestly.
- **F4 (fixed):** draft omitted the AR-33 departure citation for REQ-XD-215's baseline re-run
  semantics and the "no L2/phase-2 baseline reuse" leg — now explicit in R7.5/GATE-G36.
- **F5 (noted, accepted):** REQ-XD-220's later-pass leg (G-L1 pass id for the exact H swept,
  no concurrency with L2, new-cohort-on-H-change) is enforced in the window skeletons and
  gates, though the id's primary owner of record stays R6.43 (first pass). The contracts say
  "contributing" — the register isn't rewritten.
- **F6 (noted, accepted):** the Round-6 guard's `check_markers6`/`check_window6` legs are
  round-6-keyed and won't fire under `round: 7`; the stock build-memory validator's
  skeleton/no-Run and chain checks still hold. Recorded as a guard-coverage limitation for
  the coordinator — a Round-7 guard extension is NOT in this registration's scope.

## Verification at planning time

- `git diff` inspected before report (BL-41 policy).
- Stock build-memory validator run; result recorded in the closeout report (any
  round-6-keyed guard limitations are reported, never passed off as green).
- No BUILD_INDEX rows added (registration only); LEDGER `nextTicket` → `R7.1`, `round` → 7.
