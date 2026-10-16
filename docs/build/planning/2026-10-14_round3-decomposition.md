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

*Dated 2026-10-16 · branch `svitali/harnessharness-r3.10` (stacked on
`svitali/harnessharness-r3.9` @ `9af8ae4`) · reconcile-build mode=backlog.*

### Landed vs carried — sources

Round 3 discharged seven backlog rows, each exactly once, with dated evidence:

| BL | Discharged at | What landed |
|---|---|---|
| BL-44 | R3.2 | DF-S2.3-1's five residual producer legs (resume_set / defer / child-lease / non-fleet ingress / heal(env_handle)) — landed or verified-landed; R-2.2.3's first flip leg |
| BL-14 | R3.3 | DF-S1.20-1's interpreter/delegation legs — R-2.6.1 + R-2.6.3 flipped MET |
| BL-15 | R3.4 | DF-S1.19-1/-2 — the six + five §5c residual members over the real port; R-2.4.1/2.4.2/2.4.3/2.4.4 flipped MET (ADR-0356) |
| BL-16 | R3.5 | DF-S1.17-1 — provider deferral flags + `surface_rejection_rate` fold (ADR-0357); R-2.5.3 flipped MET |
| BL-10 | R3.6 | DF-S1.9-2 — emitter-vs-exemption adjudication over the 11 no-producer codes (ADR-0358; CF-492 registered); R-2.1.4 flipped MET |
| BL-19 | R3.7 | DF-S1.13-1/-4 — `dpop` verification over `ring = 0.17.14` + `wrapped_long_lived` delivery (ADR-0359/0360, HUMAN-H5 disposition (b)); R-2.8.3 → MET-ENGINEERED(DF-S2.4-2) |
| BL-45 | R3.8 | DF-S1.15-3 — per-event blob encryption (`HHBE` frame + `BlobCipher`/`RingBlobCipher`, ADR-0361) |

Bookkeeping cells accounted at this closure (R3.10): five stale status leads
swept to `DONE` — DF-S1.13-1, DF-S1.13-4, DF-S1.19-1, DF-S1.19-2 (the ticket's
named four) **+ DF-S1.17-1** (discharged at R3.5; the inline `|` characters in
its cell hid it from the naive lead scan — LEDGER-OF-6 records the class).
DF-S2.4-2 gained a dated residual re-confirmation (still `OPEN`, env-gated on
BL-31). No cell rewritten; all dated DONE markers preserved.

### Coverage movement

`docs/build/COVERAGE_MATRIX.csv` recomputed (66 scoped requirement rows):

- **engineering closed = MET + MET-DIFFERENTLY + MET-ENGINEERED:**
  47 + 6 + 0 = 47/66 (R2.26) → **47 + 6 + 2 = 55/66**
- **requirement satisfied = MET + MET-DIFFERENTLY:**
  41 + 6 = 47/66 (R2.26) → **47 + 6 = 53/66**

Round-3 PARTIAL→MET flips, stated exactly: **R-2.2.3** (R3.2+R3.3),
**R-2.4.1 / R-2.4.2 / R-2.4.3 / R-2.4.4** (R3.4), **R-2.5.3** (R3.5),
**R-2.6.1 / R-2.6.3** (R3.3), **R-2.1.4** (R3.6). Re-verdicted at this
closure: **R-2.8.4 → MET-ENGINEERED(DF-S2.4-2)** — every §5g.4 engineering
leg landed (mediated dispatch CAP.3; ask-endorsement ingress + fork-rebind
R2.9a; the dpop residual closed by R3.7's preserved typed-refusal), the sole
owed leg is the env-gated TLS transport on BL-31 — the same residual its
sibling R-2.8.3 already carries. **11 PARTIAL remain** (R-2.2.1, R-2.7.1,
R-2.7.2b, R-2.8.6, R-2.9.3, R-2.9.4, R-2.9.6, R-2.10.3, R-2.10.5, R-2.11.3,
R-2.12.1) — each re-checked 2026-10-16 with a dated note; none owes an
in-build leg.

### Backlog movement

- Closed at R3.2–R3.8: BL-44, BL-14, BL-15, BL-16, BL-10, BL-19, BL-45.
- BL-30 (`ruling`, open): += CF-492 (the §3.3.8 `seal`-stage taxonomy gap)
  + CF-493 (the `hh-xcheck-bundle/1` `pending_cells[]`/`arms[].status`
  format gap — no spelling for a partially-foreign-discharged cell);
  CF-489 resolved at R3.9 (code→spec conform); the custody/OQ-388/
  CF-476/487/488/490/491 rulings stand.
- BL-41 (`standing`, accepted): += LEDGER-OF-6 — status-marker drift is a
  recurring class; the dated lead/format sweep is a standing closure rule.
- BL-42: 56 quiet/dormant standing conditions (+= ADR-0240 re-home +
  ADR-0255/0342 partially-fired + the nine new round-3 quiet conditionals
  ADR-0354…ADR-0361 + ADR-0265 re-home). BL-43: 49 fired-answered
  (+= ADR-0254, ADR-0272, ADR-0350, ADR-0244).
- BL-01 stays `accepted`/`env` — re-checked 2026-10-16: the E2 legs now
  carry foreign-run-verified evidence (PR #136, 49/49 over six arms);
  the record cannot express partial discharge in-bundle (CF-493).
- `check-backlog.sh` exit 0 — 156 expected sources (145 ADR + 11 owed
  matrix rows), complete, non-duplicating, verdict-consistent.

### New findings this round

- **CF-492** (R3.6) — §3.3.8 closed `stage` set has no `seal` spelling for
  `C-SEAL-1`'s producer; interim rule recorded, owner ruling owed → BL-30.
- **CF-493** (R3.10) — the `hh-xcheck-bundle/1` status vocabulary cannot
  express E2-verified/E3-pending; a per-candidate discharge member is owed
  before the next bundle mint → BL-30.
- **LEDGER-OF-6** (R3.10) — status-marker drift recurs each round (16 cells
  swept at R3.1, five at R3.10); standing sweep rule → BL-41.
- R3.7's loaded-host helper-spawn repair is already a recorded OPEN
  FINDINGS resolution (env overrides `HH_HELPER_SPAWN_DEADLINE_MS`/
  `HH_HELPER_CONNECT_DEADLINE_MS`; defaults unchanged) — no new row.

### The carried set for GATE-G5 — per-row reason codes

Reason codes: `human` — a person's act is the only remaining leg · `env` —
needs a provisioned environment (foreign toolchain, live provider, TLS
endpoint, live judge) · `dep` — blocked on a primitive/dependency the
hermetic build lacks (or on a dependency-side ruling such as WS-L6 trust
fields) · `in-build` — machine-achievable residual carried on capacity ·
`accepted` — gate-accepted at GATE-ACCEPT; arms execute when their
environments exist · `ruling` — spec/governance ruling owed, not code ·
`window` — scheduled to the WS-L6 program window · `standing` — perpetual
ledger row, re-issued never closed.

The GATE-G4 28-row set minus the seven round-3 discharges
(BL-10/14/15/16/19/44/45) = **21 rows carried to GATE-G5**:

| BL | Carried work | Reason |
|---|---|---|
| BL-01 | E3 replay, the HIR/ledger-corpus extension, the two-toolchain C12 cells (E2 legs foreign-run-verified per PR #136 — a partial discharge the bundle cannot yet spell, CF-493) | `env` + `accepted` |
| BL-02 | R2 cross-camp human reviewer signature (DF-S0.3-3's scoped residual) | `human` |
| BL-03 | HUMAN-H1 — E3 surface-ecosystem binding (DF-S4.10-1; provisionable per the operator's 2026-10-14 statement, unprovisioned) | `human` |
| BL-04 | HUMAN-H2 — real tracker + signed webhook (DF-S5.6-1; unprovisioned) | `human` |
| BL-05 | Gate-accepted battery arms (item 10 — incl. DF-S1.15-1's custody-riding halves and DF-S1.21-2's arms) | `accepted` |
| BL-17 | Judge binding + live-judge admissibility (DF-S1.21-3; R-2.7.1/R-2.7.2b's residual) | `env` |
| BL-26 | Live foreign-manifest import (DF-S1.24-2; R-2.9.4/R-2.9.6's residual; HUMAN-H3) | `env` |
| BL-27 | oauth/mtls CallerCredential mediator legs (DF-S4.11-2; R-2.11.3's residual; WS-H3) | `env` |
| BL-30 | Spec/governance rulings — signer custody (OQ-170/DF-S2.5-1), OQ-388 ratification (DF-S4.13-1), CF-476/487/488/490/491 + CF-492/CF-493, R-2.2.1/R-2.8.6 verdict residuals, ADR-0345 trigger (b), the OQ-441/WS-I6 re-pin marker | `ruling` |
| BL-31 | Live provider transports + the TLS-terminating transport (DF-S2.4-2 — the residual R-2.8.3/R-2.8.4 both carry as MET-ENGINEERED legs; DF-S1.18-1 live variants; TLS/live-sink-bound triggers ADR-0266/0340/0341/0346) | `env` |
| BL-32 | Remote fetch transports + foreign import/export vocabularies (DF-S4.2-1/-2 — dep-gated on WS-L6 trust fields; R-2.9.3/R-2.10.3/R-2.10.5 residuals) | `dep` + `env` |
| BL-33 | WS-L6 package (R-2.12.4 + 13 OQs) | `window` |
| BL-34 | ADR-0211 ratified deferrals D-1..D-6 + OQ-461/462/463 | `window` |
| BL-35 | ADR-0212 register package — remaining open OQs | `window` |
| BL-36 | ADR-0213 register package — remaining open OQs | `window` |
| BL-37 | ADR-0214 register package — remaining open OQs (incl. the OQ-441 Stage-5 re-pin) | `window` |
| BL-38 | ADR-0215 program candidates + OQ-440 | `window` |
| BL-39 | ADR-0216 fixer deferrals | `window` |
| BL-41 | Ledger OPEN-FINDINGS standing rules (incl. LEDGER-OF-6's status-marker sweep) | `standing` |
| BL-42 | 56 quiet/dormant ADR triggers | `standing` |
| BL-43 | 49 fired-and-answered ADR triggers | `standing` |

And the three pending cells the bundle still lists but no foreign run has
discharged — `polyglot-ci.two-toolchain` (DF-S0.3-2 → BL-01),
`bench-manifest.live-import` (DF-S1.24-2 → BL-26), `lt-03.live-corpus-arm`
(DF-S1.13-3 → BL-05) — plus the format-gap decision (CF-493 → BL-30) the
partially-discharged record now needs. HUMAN-H5's record is discharged
(ruled 2026-10-14, disposition (b); ADR-0359 owns the admitted scope) and
HUMAN-H1/H2/H3/H4 stay `gate pending` rows on their BL homes — provisionable
per the operator's 2026-10-14 statement, none provisioned in-build.

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
