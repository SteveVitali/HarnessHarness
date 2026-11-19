<!--
  docs/build/planning/2026-11-19_round7-decision-memo.md — dated next-round
  decision memo (CAP6.3 / reconcile-build mode=integration deliverable).
  Append-only planning record. This memo SEEDS Round 7; it creates no
  manifest row — the round opens only on an operator decision after
  GATE-G32 signs.
-->
# Decision memo — seeding Round 7 from the round-6 routing register

**Date:** 2026-11-19 · **Author:** CAP6.3 (agent-drafted, operator-decides) ·
**Inputs:** `reports/round6/LATER_PASS_ROUTING.md` (34 routed ids) ·
`reports/round6/CAPSTONE_CLOSURE.md` (G32 signature list) ·
`reports/round6/SPEC_RECONCILIATION_PLAN.md` (R6A-1/R6A-2, unticked) ·
`reports/round6/INTEGRATION_PLAN.md` (merge order + red-span item).

## Preconditions (nothing below starts before all of these)

1. **GATE-G32 signed** — the signature list in `INTEGRATION_PLAN.md` §7:
   REQ-XC-325 `unexecuted{no_executor}` waiver · Ws1 estimate-over-ceiling
   `5,250,954 µ$` vs `4,136,280 µ$` · two-fold compute-estimate variance ·
   operator-disk retention (`unrecorded` today) · PR merge ordering · the
   accepted-deviation set incl. AR-08/AR-10 mint-or-waive (R6A-2).
2. **The chain lands** — Phase A green span (407→408→409→410→411→413→412)
   then the repaired red span (414→415→416→417→418→CAP6.3) per
   `INTEGRATION_PLAN.md` §6. The red-span repair decision (item I-3:
   `docs-readers.allow` extension vs ADR-0362-family amendment) is owed
   to the maintainer before Phase B merges.
3. **DOC6.1** (post-G32 documentation closeout) — the ticket that follows
   CAP6.3 in the manifest's own ordering; Round 7 planning must not fork
   ahead of it.
4. **Coordinator applies the proposed ledger rows** — LEDGER.md /
   BUILD_INDEX.md / DEFERRALS.md proposed text in `runs/CAP6.3.md`
   (the CAP6.1 deferral status rows + DF-R6.45l-1's landing note + the
   CAP6.3 build rows). Until those land, `deferrals-open.sh` still emits
   DF-CAP6.1-1…5 and DF-R6.45l-1 — machine state, not new debt.

## The seed — what Round 7 would own

The 34 routed ids, by home section (every row's owner = the later pass,
trigger = its own acceptance predicate; a routed id is never satisfied):

| cluster | ids | what the pass would do |
|---|---|---|
| §8.x sweep/lab deep-pass | XD-204…XD-225 (15: §8.1–§8.7 incl. XD-205's XD-124/XE-127 alias pair) | phase-4 sweep acceptance — requires a real second-pass window decision (budget + subject + gate chain) |
| §7.8 model-port maturity | XE-002, XE-010…XE-013, XE-128…XE-132 (10) | typed-render/L1 surface maturity beyond the first pass |
| §5.5 harness-level | XC-314, XC-316 | full-target harness acceptance legs |
| §6.2/§6.7 comparative | XD-013, XD-052 | selection/continuation full predicates |
| §2.5 provenance | XA-126 | records-in full-target acceptance |
| §3.8 re-proof | XH-033 | re-proof predicate at full target |

Plus the G32-conditioned items: REQ-XC-325's executor (if the waiver is
not granted — the honest outcome if waived), the operator-disk retention
act itself, and R6A-1/R6A-2 if the operator ticks them (canonical
appendix range; the two owed ADRs).

## Options for the operator

- **O-1 — Round 7 as a bounded second pass:** open a small manifest
  targeting the §8.x cluster only (15 ids) with a declared window
  envelope — cheapest meaningful increment; the §7.8 cluster waits for
  a dedicated port-maturity pass.
- **O-2 — Deferred round:** no Round 7 manifest now; the routing
  register + this memo are the dormant record (the register's own
  wording: "next round's target pass"). GATE-G32 decides.
- **O-3 — Scoped-out:** the operator formally accepts the routed set as
  permanently later/not-v1 — requires a G32-style signature, since
  PARTIAL ≠ scoped-out under O4.

**Recommendation:** O-1 conditioned on all preconditions; O-2 is the
honest default if budget/timeline does not admit a second window. O-3
changes requirement dispositions — it is a gate act, not a memo act.
