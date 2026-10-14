# Round-3 decomposition record — decompose-spec mode=extend

- **Date:** 2026-10-14 · **Mode:** extend · **Seeds:** `docs/tickets/00_MANIFEST.md` rows 137–149
- **Inputs:** the GATE-G4 closeout (`docs/build/readouts/GATE-G4.md` §1 chartered-vs-landed,
  §2 carried set with reason codes, §8 honest flags G4-1…G4-7, §9 dispositions),
  `docs/build/BACKLOG.csv` (BL-01…BL-46), `docs/tickets/DEFERRALS.md` owed set (28 rows),
  `docs/build/LEDGER.md` CURRENT STATE + OPEN FINDINGS, `docs/build/COVERAGE_MATRIX.csv`,
  `research/registers/conflicts.md` (CF-488…CF-491).
- **Authorized scope:** the GATE-G4 `in-build`/`dep`-coded carried rows only.
- **Non-goals:** the `human` rows (BL-02/BL-03/BL-04 — H1/H2/H3/H4 operator provisioning), the
  `env` rows (BL-01/BL-17/BL-26/BL-27/BL-31/BL-32), the `ruling`/`window`/`standing`/`accepted`
  rows (BL-30, BL-33–BL-39, BL-41/BL-42/BL-43, BL-05), anything needing credentials or live
  services, and any implementation — this run authors the plan, never code.

## Phase 0 — state read

Round 2 ended `projectStatus: DONE` at row 136 (R2.27), five gates PASSED (G1/G2/G3/GATE-ACCEPT/
GATE-G4), coverage 47/66 on both sums. Owed work at the GATE-G4 disposition: the 28-row carried
set, of which the machine-achievable residual is the five `in-build` rows (BL-10, BL-14, BL-15,
BL-16, BL-44) and the two `dep` rows (BL-19, BL-45). Open findings owed to a code ticket: the
`hh-mcp-lab r2_19 credential_mediator_oauth` timing flake (G4-1 — six recorded firings, rising
cadence, a real defect), the `ac5` `derived_from.view_hash` parity flake (G4-2 / LEDGER OPEN
FINDINGS), the `hh-embed r2_14` loopback conn-reset (G4-7), and the bookkeeping flags G4-4 (stale
`OPEN` prefixes on ~17 discharged DEFERRALS cells), G4-5 (DF-S2.4-2 bare `OPEN`), G4-6 (stale
BACKLOG.md counts — partially corrected at R2.27, verify at run time). CF-489 (BL-30) sits on a
WS-I6/OQ-441 ruling — code-side conform is implementable only once the ruling names a side.

## Phase 1 — dependency/coupling map

Clusters formed by shared context (one fresh-context subagent must hold the whole working set):

| cluster | sources | shared context | → row |
|---|---|---|---|
| hygiene + flakes | LEDGER OPEN FINDINGS (oauth timing, ac5 view_hash, r2_14 loopback), G4-4/G4-5/G4-6 | three disjoint test loci + a mechanical register sweep; one verification pass | R3.1 |
| durable-exec producers | DF-S2.3-1 residual, BL-44, ADR-0265 | hh-env `deliver_wakeup` producer pass + hh-embed durability ops + the R2.3 machinery | R3.2 |
| control delegation | DF-S1.20-1 residual, BL-14 | hh-control variant interpreters + the R-2.6.3 child-lease substrate R3.2 lands | R3.3 |
| §5c context/memory | DF-S1.19-1/-2 residuals, BL-15 | hh-context + hh-bench corpus + packaged `hh-memory-store` variant | R3.4 |
| tool exposure | DF-S1.17-1 residual, BL-16, ADR-0272 (a)/(b) | hh-registry/hh-compiler exposure + the R2.8 batteries | R3.5 |
| assembly emitters | DF-S1.9-2 residual, BL-10, ADR-0350 | the AC-CC-11 INDEX + hh-assembly/hh-compiler error kinds | R3.6 |
| crypto-dep legs | BL-19 (dpop/wrapped_long_lived), BL-45 (AEAD) | one missing-primitive decision; two subsystems | HUMAN-H5 → skeletons R3.7/R3.8 |
| DebtPolicy conform | CF-489 | gated on WS-I6/OQ-441 (BL-30 ruling) | skeleton R3.9 |
| closure | the round's accounting | reconcile-build mode=backlog | R3.10 → GATE-G5 → R3.11 |

Decisions the map forced:

1. **`dep` rows become skeletons + one HUMAN marker.** BL-19 and BL-45 need primitives the
   hermetic workspace lacks (an asymmetric verifier; a conforming AEAD). Implementation against a
   forbidden dep is not schedulable; a fabricated pass is worse. One operator ruling (HUMAN-H5 —
   in-workspace construction / ADR'd deps-contract amendment / permanently typed-refused) gates
   two skeleton rows, one per BL row/subsystem, each fleshed by a later mode=extend insert.
2. **CF-489 is gated, not small.** The conform direction is not determinable in-build (CF-489's
   own record: both sides provisional pending WS-I6/OQ-441, joint with OQ-304). A skeleton names
   the gate; BL-30 remains the ruling's home.
3. **R-2.6.3 single-owner.** BL-44's child-lease producers and BL-14's `delegation_reason` leg
   share the delegation substrate → R3.2 owns it, R3.3 consumes it (the ordering edge).
4. **`resume_set` producer/consumer seam.** The consumers landed at R2.5; the producer is
   DF-S2.3-1's (R3.2). DF-S1.19-1's residual keeps only "by_run producer paths beyond
   resume_set" — recorded on both tickets so no cell is claimed twice.
5. **Flakes merged into one hygiene ticket** (the R2.1 pattern): three disjoint test loci +
   a mechanical bookkeeping sweep share one verification pass; per-subsystem placement would
   multiply fixed cost and let the oauth defect age another round.
6. **Everything else stays carried** on its GATE-G4 disposition — the `env`/`human`/`ruling`/
   `window`/`standing`/`accepted` rows are recorded in the manifest's not-scheduled list; DF-S2.4-2
   gets a dated residual note under R3.1, not a ticket (its work is BL-31's TLS transport).

## Phase 2 — the partition

13 new rows: 137–149. Six implement-spec tickets (R3.1–R3.6), one HUMAN marker (H5), three
skeletons (R3.7 BL-19 / R3.8 BL-45 / R3.9 CF-489), one closure reconcile (R3.10), one gate
(GATE-G5), one docs row (R3.11). Ordering is the dependency spine; the specific backward edges
are recorded in the manifest's Round-3 decomposition block. Tail: minimal (closure + gate +
docs), matching the Round-2 shape.

## Phase 4 — adversarial review (reread-and-argue; no fresh subagent available)

Recorded verbatim in the manifest's `## Decomposition decisions → Round 3` block:
fragmented-decision hunt (R-2.6.3 substrate single-owner; `resume_set` producer/consumer seam),
overflow check (R3.4 is the largest — pre-registered R3.4a/R3.4b seam in its Notes; all other
rows ≤1400 changed lines), orphan-seam audit (the gated rows name their gates and fleshing
paths — nothing silently dropped), coverage map (every `in-build`/`dep` carried row + every
ledger finding + every G4 flag → exactly one R3 row), double-ownership check (producer vs
consumer named per side), ordering acyclicity (all Depends-on backward; skeletons carry no Run
line; the gate is never pre-answered), over-factoring check (flakes merged; the two `dep`
skeletons kept separate because a per-primitive ruling may split them). **Verdict:**
dependency-clean and honesty-preserving; the only residual risk is R3.4's size, mitigated by the
pre-registered split.

## Phase 5 — seeded state

Ledger resumed: `projectStatus: IN_PROGRESS`, `nextTicket: R3.1`, `round: 3`,
`chainTip: svitali/harnessharness-r2.27` (round-3 rows fork from the round-2 tip),
`lastCompleted: R2.27` preserved, PHASE LOG gained the `### Round 3` heading + seed entry.
`BUILD_INDEX.md` untouched — Round-3 rows index as they land.

## Round-3 accounting — appended at closure (R3.10)

*(to be appended by R3.10 — landed vs carried, with the carried set enumerated for GATE-G5.)*

## Amendment — operator rulings applied (2026-10-14, same-day, post-seed commit `89584c8`)

Three rulings arrived after the partition was seeded; the records below are the conform edits.
Row numbering is stable — no row was renumbered, moved or deleted; the skeleton texts stand as
history in the seed commit.

**1. HUMAN-H5 answered — disposition (b): external crypto crate admitted.** The operator permits
a vetted crate from the ring/aws-lc/rustls family. Consequences: `144_R3.7` and `145_R3.8` were
rewritten from skeletons into full contracts (Run lines, ACs, size budgets 1200 lines each). The
crate+version choice is owned by the **dep-choice ADR inside R3.7** — it names the crate, pins a
version **published ≥ 7 days before selection** (the operator's supply-chain rule), states the
admitted scope, and amends the workspace's hermetic-deps note (Cargo.toml header + AGENTS.md
clauses land behind the ADR in the same PR; vetting posture per ADR-0050 C11 — lockfile-pinned,
audit/vet, offline-reproducible build preserved). R3.8 consumes the pinned crate for its AEAD
surface where the ADR scopes it, else authors a sibling dep ADR under the same rules — so the
R3.7 → R3.8 Depends-on is now a *real* dependency edge, not only chain position.

**2. CF-489 direction ruled — conform code to spec.** `DebtPolicy::default()` moves to §5h.6's
proposed 60 d `min_runway` / 30 d `warn_within` (the other two placeholders already matched).
`146_R3.9` was rewritten into a small executable ticket (≤300 lines: constants + fidelity comment
+ caller/test sweep across hh-ontology/hh-debt/hh-embed/hh-lab/hh-hir + dated CF-489 resolution
note). The `[[DEFERRED ADR-0214: OQ-441]]` spec markers are NOT retired — WS-I6 (with
WS-C3/WS-I2) still owns the final Stage-5 `DebtIndex` re-pin; the conform removes the recorded
divergence, it does not answer the question. BL-30/BL-37 receive dated progress notes and stay
open on their other named-workstream rulings.

**3. Environment path is live.** The operator states HUMAN-H1/H2/H3 are provisionable. The
env-gated rows (BL-01/17/26/27/31/32 and their dependents) stay *excluded* this round — nothing
is provisioned yet — but they are no longer a dead partition: they unblock when the H-rows land,
and a future `mode=extend` round schedules them against the provisioned environment. Recorded
here and in the manifest's Round-3 block so the exclusion is a reasoned carry, not a silent drop.

**Ordering re-check (recorded call):** positions 144–146 retained — renumbering would break the
filename convention and no edge requires moving them. New ordering edge: R3.7 → R3.8 (the dep
pin). R3.9 has no code coupling; it stays a small ticket ahead of closure. R3.1 hygiene-first and
R3.2 → R3.3 (the R-2.6.3 substrate) are unchanged. The round is now **9 implementable tickets
(R3.1–R3.9) + H5's ruling record + R3.10 closure + GATE-G5 + R3.11 docs**.
