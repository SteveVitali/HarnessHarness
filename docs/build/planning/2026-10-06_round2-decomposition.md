# Round-2 decomposition record — decompose-spec mode=extend

- **Date:** 2026-10-06 · **Mode:** extend · **Seeds:** `docs/tickets/00_MANIFEST.md` rows 107–136
- **Inputs:** the Round-1 closeout (`CAPSTONE_CLOSURE.md`, `BACKLOG.csv` BL-01…BL-43,
  `DEFERRALS.md` open/residual set), `2026-10-06_decision-memo.md`, the spec-amend fold (A-1..A-3),
  and the open-question/risk/conflict registers.
- **Non-goals:** the 10 GATE-ACCEPT signed deviations (accepted, not owed); anything DONE; new
  speculative scope; no implementation in this act.

## Phase 0 — state read

Round 1 ended `projectStatus: DONE` at row 106 (DOC.2), four gates PASSED, 102 landed non-gate
rows. Outstanding owed work = 40-ish OPEN/PARTIAL DEFERRALS rows + the backlog register +
register-revalidation packages + two human rows that persist (H1/H2) + one new defect opened at
DOC.1 (DF-DOC.1-1) + one CI flake observed on PR #104 (run 37482844398 — the `assembly_ms`
wall-time parity false-positive; sibling run on the same SHA green).

## Phase 1 — dependency/coupling map

Clusters formed by shared context (one fresh-context subagent must hold the whole working set):

| cluster | sources | shared context | → row |
|---|---|---|---|
| hygiene | DF-DOC.1-1, DF-S1.3-2/-9-4/-26-2 flips, BL-29, CI flake | small, disjoint, verification-flavored | R2.1 |
| ledger retention | DF-S1.5-1 residual, DF-S2.9-1 | hh-ledger store internals | R2.2 |
| durable exec | DF-S2.3-1 | wakeup producers + protocol entry points | R2.3 |
| env ops | DF-S2.9-3, DF-S2.10-1 | env drivers + boundary ops + cadence | R2.4 |
| context/memory | DF-S2.8-1 (sans steer), DF-S1.19-1/-2 | hh-context + driver call sites + §5c corpora | R2.5 |
| control steer | DF-S2.11-1, DF-S1.20-1, DF-S2.8-1a steer leg | ONE durable steer-cue seam | R2.6 |
| model plane | DF-S1.18-1 machine cells | hh-gateway + driver emitters + dialect corpus | R2.7 |
| tool exposure | DF-S1.17-1/-2/-3 | registry/catalogue + exposure loop | R2.8 |
| egress/containment | DF-S1.12-1/-2/-4/-6, DF-S2.4-1, DF-S2.4-3 | hh-containment + mediator + leak-scan | R2.9 |
| credential broker | DF-S1.13-1/-3 | hh-secrets + env image manifests | R2.10 |
| approvals + Π | DF-S1.23-1, DF-S1.11-2 | same hh-monitor decide-path working set (merged BL-22+BL-23) | R2.11 |
| IFC | DF-S2.7-1 | provenance walk + remedy consume (consumes R2.11's ingress) | R2.12 |
| audit | DF-S1.15-3, DF-S1.15-1 non-custody | hh-ledger signer/checkpoint machinery | R2.13 |
| telemetry | DF-S1.14-1/-2/-4 | telemetry emitters + subscriber runtime + sinks | R2.14 |
| verification | DF-S1.21-1/-2/-3 | hh-verification + critics + validator suite | R2.15 |
| eval | DF-S1.22-1/-2 | hh-eval engine ops + fault battery | R2.16 |
| bench/measurement | DF-S1.24-1/-2 machine cells | hh-bench + hh-debt resolvers | R2.17 |
| assembly | DF-S1.9-2 | hh-assembly grammar + C2 layers | R2.18 |
| embed/MCP surface | DF-S1.25-1, DF-S4.11-1/-2 | hh-embed + hh-mcp-lab + mediator seam | R2.19 |
| extension trust | DF-S1.23-2 | registry lifecycle + monitor checks | R2.20 |
| foreign verification | DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1, live cells | one environment (H3) + one replay harness | HUMAN-H3 → R2.21 |
| human signature | DF-S0.3-3 residual / BL-02 | operator act | HUMAN-H4 |
| register revalidation | BL-35+39 / BL-36 / BL-37 / BL-40 | append-only register passes | R2.22…R2.25 |
| closure | backlog refresh + coverage + readiness | reconcile-build mode=backlog | R2.26 → GATE-G4 → R2.27 |

Decisions the map forced:

1. **BL-05 dissolved.** Its members are subsystem-owned cells bundled for gate acceptance; each went
   to its home-subsystem ticket rather than a grab-bag ticket (would share no implicit decision).
2. **Steer-cue seam single-owner.** DF-S2.8-1(a)'s steer arm + DF-S2.11-1's `queue_next_turn` are one
   durable transport seam → R2.6 owns both; R2.5 keeps the sequencing/emitter legs.
3. **DF-S1.18-1 split along its own seam.** Offline-decidable cells (emitters, routing arms under
   declared guards, WireDialect corpus, K4/K5) → R2.7; live transports stay BL-31.
4. **Foreign verification = one environment ticket + one HUMAN marker.** The cluster is five DF rows
   gated on the same unprovisioned environment; one gated ticket (R2.21) lands the hermetic
   packaging + E1 self-check and runs the foreign cells only where H3 exists. Multiple separate
   foreign tickets would multiply the same missing prerequisite.
5. **DF-S1.13-4 already DONE (S4.14b)** — excluded from R2.10's scope; the ticket verifies, doesn't
   redo. Same check across the whole set caught DF-S1.26-1, DF-S3.5-1, DF-S4.11-3, DF-S5.4-1,
   DF-S1.24-3, DF-S3.9-1, DF-S2.9-2, DF-S2.10-2, DF-S3.12b-1/-2 as DONE → not scheduled.
6. **BL-41/BL-42/BL-43 are perpetual/trigger rows** — re-issued at closure (R2.26), never ticketed.

## Phase 2 — the partition

30 new rows: 107–136. 20 implement-spec tickets (R2.1–R2.21), 4 register-revalidation reconciles
(R2.22–R2.25), 1 closure reconcile (R2.26), 2 HUMAN markers (H3, H4), 1 gate (GATE-G4), 1 docs row
(R2.27). Ordering is the dependency spine; the specific backward edges are recorded in the
manifest's Round-2 decomposition block.

## Phase 4 — adversarial review (reread-and-argue; no fresh subagent available)

Recorded verbatim in the manifest's `## Decomposition decisions → Round 2` block: fragmented-
decision hunt (steer seam consolidated; remedy ingress single-owner; postconditions binding vs
consumer split named), overflow pre-registered splits (R2.9a/b, R2.15a/b), orphan-seam audit
(custody/TLS/WS-L6 cells deferral-backed, not dropped), coverage map (every OPEN/PARTIAL DF row +
machine BL row → exactly one owner), double-ownership check (the two split rows name members per
side), ordering acyclicity, over-factoring merges (BL-22+23, BL-35+39) and the honest-gate posture
(H3/H4 markers; R2.21 withholds rather than fabricates). Verdict: dependency-clean; residual risk is
ticket-level overflow on R2.9/R2.15.

## Phase 5 — seeded state

Ledger resumed: `projectStatus: IN_PROGRESS`, `nextTicket: R2.1`, `round: 2`,
`chainTip: svitali/spec-amend-a1-a3` (the amended-spec tip — A-1..A-3 ride on doc.2 and Round-2
tickets verify against them), `lastCompleted: DOC.2` preserved, OPEN FINDINGS gained the CI-flake
row, PHASE LOG gained the `### Round 2` heading + seed entry. `BUILD_INDEX.md` untouched — Round-2
rows index as they land.
