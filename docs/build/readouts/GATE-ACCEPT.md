# GATE-ACCEPT readout — the operator signs the ACCEPTED-deviations list
Status: SIGNED   <!-- recorded 2026-10-06 (DOC.2): reflects the operator signature of 2026-10-02 already in this file (### Verdict — PASSED + disposition line); the line was absent because the file predates the Status: convention (BM-TAIL-03) -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

## Criterion (verbatim — the gate ticket `docs/tickets/101_GATE-ACCEPT.md`)

> The operator has reviewed the ACCEPTED-deviations list in `docs/build/CAPSTONE_CLOSURE.md`
> (from CAP.3) and signed each row as accepted, or sent a row back to closure. A build of
> 92 tickets against `spec/CANONICAL_SPEC.md` is not DONE while any proposed deviation is
> unsigned.

**Blocks:** `projectStatus: DONE` (BM-TAIL-03) and REC.1–REC.3 that assume signed
deviations.

**Pre-registered thresholds.** *"Every row in the CAP.3 ACCEPTED-deviations list is signed
(accepted) or returned to closure; no proposed deviation is left unsigned."*

**DEFERRALS rule applied (docs/tickets/DEFERRALS.md rule 4):** *"Gates refuse to pass while
any `OPEN` row scoped to that phase remains."* — capstone-phase scoping accounted for in
the sweep (§5–§6).

---

## 2026-10-02 — Reading 1 (the list presented for signature) → PENDING operator disposition

**Evidence sources:** gate ticket `docs/tickets/101_GATE-ACCEPT.md`;
`docs/build/CAPSTONE_CLOSURE.md` (the 10-item ACCEPTED-deviations list — transcribed
verbatim in §3); `docs/build/COVERAGE_MATRIX.csv` (66 requirement rows);
`docs/tickets/DEFERRALS.md` (99 rows — 39 DONE, 60 not-fully-discharged, all classified
in §5); `docs/build/LEDGER.md` (CURRENT STATE + GATE DECISIONS precedents: GATE-G1 PASSED
2026-09-15, GATE-G2 PASSED 2026-09-25, GATE-G3 PASSED 2026-10-02);
`docs/build/COMPOSED_E2E_REPORT.md` (CAP.2 — 12 composed legs green, 6 xfails each
carrying a `DF-*` id, 2 defects the composition itself surfaced);
`docs/build/CAPSTONE_GAP_ANALYSIS.md` (CAP.1 — fresh-context verdicts committed before any
run ledger was read); `docs/build/runs/CAP.3.md` (the completed workspace run).

This readout presents the list and leaves every disposition cell blank for the operator.
*An operator or authorized human record supplies the decision; an agent must not sign or
assume silence is approval.*

### 1 · The suite baseline the acceptance rests on

`cargo test --workspace` at the CAP.3 chain tip (`target/cap_3_ws.log`, recorded in
`docs/build/runs/CAP.3.md`): **280 result blocks / 3028 passed / 0 failed / 2 ignored** —
`cap2_fork_at_turn_boundary_without_explicit_snapshot` (DF-S2.9-3's cadence leg) and the
pre-existing `ac_r_2_2_1_16_latency_1e5_fixture`. All six composed xfails CAP.2 pinned are
green **un-ignored**.

### 2 · Coverage roll-up for context

CAP.1's fresh-context matrix (committed `efd71f5`, before any run ledger was read)
recorded **29 MET / 6 MET-DIFFERENTLY / 29 PARTIAL / 0 MISSING / 2 AT-RISK-INTEGRATION**
across the 66 `R-2.*` requirements. CAP.3 landed the two at-risk compositions and flipped
both to PARTIAL; the matrix now stands **29 MET / 6 MET-DIFFERENTLY / 31 PARTIAL /
0 MISSING / 0 AT-RISK**. No requirement is absent; every residual leg is carried by a
named OPEN `DEFERRALS.md` row (§5).

### 3 · The list presented — CAP.3's ACCEPTED-deviations (10 rows, verbatim)

*Transcribed from `docs/build/CAPSTONE_CLOSURE.md` § "ACCEPTED-deviations — proposed for
operator signature at GATE-ACCEPT". Each row: **id · what deviates · why sound ·
compensating control.** The set is the six MET-DIFFERENTLY verdicts (CAP.1 D3) plus the
gate-accepted deviations; nothing new was accepted at CAP.3.*

1. **R-2.7.2 split (ADR-0146)** — the parent requirement is discharged
   through children R-2.7.2a/b rather than as one row. *Sound:* the
   split is a spec-sanctioned decomposition; both children carry their
   own verdicts and tests. *Control:* children verdicts recorded and
   driven; the parent row references the split explicitly.
2. **R-2.12.3 ecosystem decision (ADR-0050; ADR-0009; ADR-0226)** —
   the contract is discharged by the recorded decision + the E1
   implementation; the E2/E3 foreign legs are deferred, not executed.
   *Sound:* S0.3b proved E1=E2=E3 byte-identity on the golden corpus —
   the foreign legs add no evidence the identity proof doesn't already
   give. *Control:* DF-S0.3-2 carried OPEN to GATE-ACCEPT; the E2
   trigger-5 disposition is recorded (item 8).
3. **R-2.12.4 spec-level deferral (ADR-0210)** — the item is deferred
   by the spec itself. *Sound:* the build honours a spec-anchored
   deferral; no code was ever expected. *Control:* the deferral is
   named in the spec; nothing claims the row.
4. **R-2.12.5 program-level naming thesis (ADR-0003/0006)** — the
   discharge is a program artifact (uniform naming), not a code
   deliverable. *Sound:* the requirement names a program-level
   property. *Control:* naming is applied uniformly and is
   grep-verifiable across the build.
5. **R-2.11.2 E3-ecosystem binding (HUMAN-H1; DF-S4.10-1)** — the
   generated client runs in-ecosystem as the declared honest proxy;
   the real E3 surface-ecosystem binding is unprovisioned. *Sound:* a
   human prerequisite cannot be minted in-build; the proxy exercises
   the same wire surface. *Control:* `provided: no` is recorded;
   discharge at operator provisioning (DF-S4.10-1 stays OPEN).
6. **R-2.12.6 real issue tracker (HUMAN-H2; DF-S5.6-1)** — the fixture
   adapter is the declared honest proxy; the real tracker is
   unprovisioned. *Sound:* as item 5. *Control:* the fixture exercises
   the real contract surface; DF-S5.6-1 stays OPEN.
7. **R2 cross-camp human signature (DF-S0.3-3 residual; accepted at
   GATE-G1)** — the human-signature cell cannot be produced by the
   build. *Sound:* it is a human deliverable by definition. *Control:*
   the machine cells are measured and the byte-identity gate is proven;
   the residual rides to GATE-ACCEPT.
8. **Armed revalidation trigger 5 satisfied-by-analysis (ADR-0226)** —
   E2 steps 5–7 (ADR-0009) were dispositioned by analysis rather than
   re-run, E2's C5/C7 cells falling outside the ±1 band on a
   non-kernel candidate. *Sound:* the analysis is documented and the
   winner (E5a) is unmoved by it. *Control:* the disposition is an ADR;
   the band analysis is reproducible from the recorded data.
9. **Foreign-toolchain verification cells (DF-S0.3-2, DF-S1.2-2,
   DF-S1.5-3, DF-S1.8-1, DF-S1.27-1)** — cross-implementation
   conformance cells sit outside the hermetic scope. *Sound:* the
   environment-bound cells cannot be minted offline; within-E1
   byte-equality against the pinned golden corpus plus S0.3b's proven
   E1=E2=E3 identity bound the same property. *Control:* all five rows
   stay OPEN to GATE-ACCEPT with named verification paths.
10. **GATE-G2 recorded set** — LT-03's live corpus arm, DF-S1.17-1's
    exposure form arm, DF-S1.21-2's OOP conformance battery, DF-S1.15-1,
    DF-S1.24-1 residual, DF-S1.14-4, DF-S1.22-1 (rule-4 reading),
    DF-S5.4-1 — accepted at G2/G3 with recorded reasons. *Sound:* each
    was accepted at a gate with its reason on the record. *Control:*
    the gate readouts name each row; nothing is silently dropped.

### 4 · Per-row disposition — FOR THE OPERATOR

*Each row: `ACCEPTED` or `SEND-BACK` + what would accept it. All cells intentionally
blank — the operator fills them.*

| # | Deviation row | Disposition (ACCEPTED \| SEND-BACK) | What would accept it (if SEND-BACK) |
|---|---|---|---|
| 1 | R-2.7.2 split (ADR-0146) | ACCEPTED | — |
| 2 | R-2.12.3 ecosystem decision (ADR-0050; ADR-0009; ADR-0226) | ACCEPTED | — |
| 3 | R-2.12.4 spec-level deferral (ADR-0210) | ACCEPTED | — |
| 4 | R-2.12.5 program-level naming thesis (ADR-0003/0006) | ACCEPTED | — |
| 5 | R-2.11.2 E3-ecosystem binding (HUMAN-H1; DF-S4.10-1) | ACCEPTED | — |
| 6 | R-2.12.6 real issue tracker (HUMAN-H2; DF-S5.6-1) | ACCEPTED | — |
| 7 | R2 cross-camp human signature (DF-S0.3-3 residual; GATE-G1) | ACCEPTED | — |
| 8 | Armed revalidation trigger 5 satisfied-by-analysis (ADR-0226) | ACCEPTED | — |
| 9 | Foreign-toolchain verification cells (DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1) | ACCEPTED | — |
| 10 | GATE-G2 recorded set (DF-S1.13-3 arm, DF-S1.17-1 arm, DF-S1.21-2, DF-S1.15-1, DF-S1.24-1 residual, DF-S1.14-4, DF-S1.22-1, DF-S5.4-1) | ACCEPTED | — |

### 5 · DEFERRALS sweep — 99 rows audited; 39 DONE; 60 not-fully-discharged, every one classified

No row was closed, amended, or hidden by this gate. Every not-fully-discharged
`DEFERRALS.md` row is either **on the ACCEPTED list above** (presented for signature) or
**carried to the REC.* reconciliation** named below — or a HUMAN row, all three of which
sit on the ACCEPTED list.

**On the ACCEPTED list (15 rows — stay OPEN in the ledger pending this gate's signature):**

| ACCEPTED item | DEFERRALS rows |
|---|---|
| item 5 | DF-S4.10-1 (P — HUMAN-H1) |
| item 6 | DF-S5.6-1 (P — HUMAN-H2) |
| item 7 | DF-S0.3-3 (V — R2 residual; PARTIAL: machine cells DONE) |
| item 9 | DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1 (all V — foreign toolchains) |
| item 10 | DF-S1.13-3, DF-S1.17-1, DF-S1.21-2, DF-S1.15-1, DF-S1.24-1, DF-S1.14-4, DF-S1.22-1 (DF-S5.4-1 is already DONE — landed at S6.1b/S6.3b) |

**Carried forward — 45 OPEN/PARTIAL rows not on the ACCEPTED list.** Every one lands in
REC.1's `BACKLOG.csv` by that ticket's own AC (every OPEN/PARTIAL deferral appears in
exactly one source cell); the classes below name which reconcile scope discharges each.

**Class A — REC.1 backlog scope (37 rows):** machine-achievable residual legs with named
in-build owners — emitter/call-site halves, deferred verification batteries, surface
verbs.
DF-S1.5-1 · DF-S1.9-2 (PARTIAL) · DF-S1.9-4 · DF-S1.11-2 · DF-S1.12-1 · DF-S1.12-2 ·
DF-S1.12-4 · DF-S1.12-6 · DF-S1.13-1 (PARTIAL) · DF-S1.13-4 · DF-S1.14-1 · DF-S1.14-2 ·
DF-S1.14-3 · DF-S1.15-3 · DF-S1.17-2 · DF-S1.17-3 · DF-S1.19-1 · DF-S1.19-2 · DF-S1.20-1 ·
DF-S1.21-1 · DF-S1.21-3 · DF-S1.22-2 · DF-S1.23-1 · DF-S1.23-2 · DF-S1.24-2 · DF-S1.25-1 ·
DF-S2.3-1 · DF-S2.4-1 (member (a) DONE at CAP.3; (b)/(c) open) · DF-S2.4-3 · DF-S2.7-1 ·
DF-S2.8-1 (builder leg DONE at CAP.3; residual set open) · DF-S2.9-1 · DF-S2.9-3 ·
DF-S2.10-1 · DF-S2.11-1 · DF-S4.11-1 · DF-S4.11-2.

**Class B — REC.2 spec-reconciliation scope (4 rows):** the remaining cell is a
spec/ADR/governance ruling, not code.

| Row | Residual | Ruling owed |
|---|---|---|
| DF-S2.5-1 (F) | signer custody + checkpoint-emission halves | ADR-0213 custody ruling |
| DF-S4.13-1 (F) | OQ-388's ratification half (latency bound measured; unratified) | WS-B1/K2 spec-governance act |
| DF-S1.24-3 (H) | `ModelProfile/1` `debt.hypothesis` string vs HIR typed field | `ModelProfile/2` dialect bump or ADR ruling the string canonical |
| DF-S1.26-2 (H) | parked-detach interim `detached:"parked"` member | closed-sum ruling on the detach surface |

**Class C — REC.3 integration scope (4 rows):** the remaining cell needs an
environment/integration surface outside the hermetic build.

| Row | Residual | Integration surface owed |
|---|---|---|
| DF-S1.18-1 (F) | live `Transport`/`CredentialPort` impls (fakes landed) | real provider endpoints/credentials |
| DF-S2.4-2 (F) | TLS-terminating production transport | network-hardening deployment slice |
| DF-S4.2-1 (F) | remote `fetch` transports + trust fields | WS-L6 remote-transport owner |
| DF-S4.2-2 (F) | remaining foreign import/export vocabularies | per-foreign-system owners |

**Class D — HUMAN prerequisite (0 rows off-list):** all three human-bound rows —
DF-S4.10-1 (HUMAN-H1), DF-S5.6-1 (HUMAN-H2), DF-S0.3-3 (R2 signature) — are *on* the
ACCEPTED list (items 5, 6, 7), recorded `provided: no` where applicable; no secrets in
any ledger.

### 6 · Rule-4 accounting — capstone-phase-scoped rows

The gate's deferrals clause names OPEN rows *scoped to the capstone phase*. The four
capstone-created rows are all DONE (DF-CAP.2-1, DF-CAP.2-2, DF-S3.12b-1, DF-S3.12b-2 —
closed with green tests at CAP.3). One edge case is recorded for the operator:
**DF-S1.9-4**'s own `unblocked by` cell names *"the CAP.1 audit / the next gate
readout"* — the audit ran at CAP.1, its finding (DF-S3.12b-2) closed at CAP.3, and this
readout is the named readout; the row remains OPEN only as an unflipped bookkeeping cell
and is carried in Class A. If the operator reads it as literally gate-blocking, a
SEND-BACK on any row returns it to closure. Nothing else is capstone-scoped: the
remaining 44 carried rows name in-build or post-build owners under the G1/G2/G3
carry-forward precedent this gate exists to disposition.

### 7 · Operator reminder — what this signature releases

After signing, the chain still holds: **REC.1** (backlog + operational readiness —
absorbs every carried row above into `BACKLOG.csv`), **REC.2** (spec reconciliation —
folds shipped-vs-spec reality back through ticked amendments), **REC.3** (integration
plan — PR graph, read-only merge dry-run, release notes), and **DOC.1/DOC.2** (repo/agent
docs refresh). **`projectStatus: DONE` is recorded only after this gate records PASSED**
— no proposed deviation may be left unsigned.

### Verdict — PASSED (operator, 2026-10-02)

- [x] PASSED — every ACCEPTED row signed (blanket acceptance — operator: "I accept",
  2026-10-02; no SEND-BACKs).
- [ ] NOT PASSABLE — what would pass it: ____________________
- [ ] SKIPPED-BY-OPERATOR

**Date:** 2026-10-02

### Operator disposition line — FOR THE OPERATOR

- [x] Disposition recorded: operator verbal acceptance in session ("I accept") recorded by orchestrate-build, 2026-10-02 — see LEDGER `GATE DECISIONS` 2026-10-02 GATE-ACCEPT row (operator signature / initials, and the
  ledger `GATE DECISIONS` row reference once `orchestrate-build` records it)
