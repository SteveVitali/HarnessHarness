# ADR-0330: CAP.3 closure rulings and the accepted-deviation set

- **Status:** Accepted
- **Date:** 2026-10-02
- **Ticket:** CAP.3
- **Requirement ids:** R-2.4.1 ·
  R-2.8.4 · R-2.10.3 · R-2.11.3 ·
  R-2.1.4 (the rung sweep) · the six
  MET-DIFFERENTLY rows (R-2.7.2,
  R-2.11.2, R-2.12.3, R-2.12.4,
  R-2.12.5, R-2.12.6) · DF-S2.4-1 ·
  DF-S2.8-1 · DF-CAP.2-2 ·
  DF-S3.12b-2.
- **Spec:** §5g.4 (egress
  mediation) · §5c.1 (context
  assembly) · §6.4 (matched-budget
  admissibility) · §3.3.3 +
  AC-CC-09 (the rung obligation) ·
  ADR-0061 D5 · ADR-0165 D3 ·
  ADR-0146/0050/0210/0003/0006
  (the deviation anchors).
- **Rung (ADR-0024 obligation):**
  **— (capstone record)** — the
  closure rulings land at the rung
  each cited row names; the record
  itself migrates no contract.

## Context

CAP.3 is the closure ticket: every gap CAP.1/CAP.2 routed here must be
fixed, consciously accepted, or re-anchored to a named deferral — and
the MET-DIFFERENTLY / SHOULD-level deviation set must be recorded for
the operator's signature at GATE-ACCEPT. ADR-0327/0328/0329 carry the
three contract-shape rulings (close-row partition, lift shaping, the
surface answer verb). This record carries the remaining rulings: the
two wiring dispositions, the enforcement-evidence admissibility ruling,
the rung-sweep disposition, and the deviation set itself.

## Decision

D1. **The egress seam is wired, not ruled away (DF-S2.4-1, member a —
landed).** Dispatch routes a `net_egress` effect on a `mediated`
environment through `EgressMediator`: `gate` runs
`requested → attributed → decide_egress` while the effect is still
`intended` (deny → `action.effect.refused`; ask → durable
`pending`/`requested`/`decided{ask}` + `wakeup_subscribe` +
`suspend`, the same lifecycle point as the monitor's own ask);
`forward` runs `recheck → sentinels → decided{allow} → wire → charge`
at the wire point — post-`committed`, pre-executor — so the effect's
write-ahead still precedes the wire and the consume-once re-resolution
still guards the SSRF pivot. A post-commit denial can no longer mint
`refused`; the wire provably never ran, so the honest terminal is
`observed{not_applied, containment_denied}` + `completed{error}`.
The broker seam (`set_egress_broker`) defaults to `DenyAllResolver` —
a sentinel-bearing request denies until a composed host installs a live
broker (SV-8 PDP/CDP separation). **Still open on the row (members
b/c):** nothing feeds the operator verdict into `endorse_asked`'s
`ApprovalCache` on resume (the dispatch `gate`/`forward` uses a fresh
mediator per call — the re-gate re-runs the pure decision; the
endorsement fold is the residual), and `fork`'s
`rebind_credentials_for_fork` call site stays unwired pending the
broker custody seam on the session.

D2. **The assembler seam is wired (DF-S2.8-1, member a's builder leg —
landed).** `AssemblerPort::assemble` gains `AssembleInputs{model_call_id,
prefix, window_cap_tokens}`; `KernelAssembler` now runs the real
`hh_context::assemble` over the run's durable prefix — observation
events project to `Observation` candidates with honest provenance, and
the appended `context.assembled` is the builder's canonical plan record
(`plan_id`/`layout_ref`/`policy_ref`/`derived_from`/`occupancy_estimate`/
`assembly_ms`/`compaction_state`), never the pass-through stamp. Builder
side-band emissions append durable under the same call scope — nothing
is dropped silently. The port is infallible: an assembly error is
recorded on the row, never hidden. **Still open on the row:**
`ContextWindowExceeded → CompactionRequired` routing, `deliver_wakeup`'s
steer arm (OQ-316), `mark_scope_ended` sequencing, `resume_set`
consumers, `judged`/`human` detectors, and the OOP differential
conformance corpus run — all carried by DF-S2.8-1's residual set.

D3. **Enforcement evidence is the adapter's stamp, never the claim
(DF-CAP.2-2 — closed).** `analysis_ops::resolve_arms` reads the
`budget_enforcement` map stamped on the bound subject run's manifest
(`extra["budget_enforcement"]`, the Hosting ABI's adapter-derived
`{dimension → enforced|advisory|unenforceable}` — ADR-0165 D3). A
stamped map admits exactly the dims the adapter proved; an unstamped
hosted arm keeps the empty map the match check refuses on. The
`limits_enforced` record field is a claim — it is never read back as
proof (CC2/CC9); `full` still maps to `native()` for unhosted arms.
A hosted arm stamped `model_calls: enforced` now satisfies
`matched_cap`'s enforceability bar — the composed compare lands a real
verdict instead of `IncommensurableMatch`.

D4. **The rung-statement sweep is done, not scoped away (DF-S3.12b-2 —
closed).** All 110 `docs/adr/ADR-*.md` records now carry the
`**Rung (ADR-0024 obligation):**` line — the 30 records that already
carried it (the ADR-0240–0268 convention band plus later records) and
the 80 swept at CAP.3 (the audit's 43 plus the Stage-4/5/6 records that
had since landed). Each line names the rung the ticket's rulings land
at (or an explicit non-contract note for spike/gate/program records).
`_TEMPLATE.md` now requires the line, so the obligation stays fixed —
the retro-added line is the uniformity retcon the deferral authorizes;
content of landed ADRs is untouched (append-only).

D5. **The MET-DIFFERENTLY set is ratified for signature.** Each row's
verdict stands, anchored to its owning record, and is proposed to the
operator as an accepted deviation (the full list with soundness +
compensating controls is `docs/build/CAPSTONE_CLOSURE.md`):

- **R-2.7.2** — split into R-2.7.2a/b (ADR-0146); discharged through
  the children.
- **R-2.12.3** — the ecosystem decision + E1 implementation discharges
  the contract (ADR-0050; ADR-0009 revalidation; ADR-0226 disposition);
  foreign legs deferred (DF-S0.3-2 carried to GATE-ACCEPT).
- **R-2.12.4** — spec-level deferral (ADR-0210); the build honours the
  deferral; no code expected.
- **R-2.12.5** — program-level naming thesis (ADR-0003/0006); discharge
  is a program artifact — naming applied uniformly.
- **R-2.11.2** — HUMAN-H1 unprovisioned; the in-ecosystem generated
  client is the declared honest proxy (DF-S4.10-1).
- **R-2.12.6** — HUMAN-H2 unprovisioned; the fixture adapter is the
  declared honest proxy (DF-S5.6-1).

D6. **The carried SHOULD-level deviations stand for signature.** The
gate-accepted deviations are proposed unchanged: the R2 cross-camp
human-signature residual (DF-S0.3-3 — a human deliverable no build can
mint), the armed revalidation trigger-5 disposition
satisfied-by-analysis (ADR-0226; ADR-0009 steps 5–7), the
foreign-toolchain verification cells (DF-S0.3-2, DF-S1.2-2, DF-S1.5-3,
DF-S1.8-1, DF-S1.27-1 — outside the hermetic scope; within-E1
byte-equality against pinned golden corpora + S0.3b's proven
E1=E2=E3 identity are the compensating controls), and GATE-G2's
recorded set (LT-03 live corpus arm, DF-S1.17-1 form arm, DF-S1.21-2
OOP battery). Every other residual stays an OPEN `DEFERRALS.md` row —
deferred, not accepted, each with a named owner and verification path.

## Consequences

- The two AT-RISK-INTEGRATION verdicts (R-2.8.4, R-2.4.1) move to
  PARTIAL — the composed call sites are now wired and driven green on
  live paths; the residual legs stay on their open rows.
- The ACCEPTED-deviations list exists for GATE-ACCEPT: the six
  MET-DIFFERENTLY rows + the gate-accepted deviation set; the operator
  signs or returns each.
- The `#[ignore]`d composed xfails dropped from 6 to 1 (only
  DF-S2.9-3's snapshot-cadence leg remains — a staged env-driver row,
  not CAP.3's).

## Alternatives considered

- **ADR the unwired seams as intended (fail-closed egress / pass-through
  assembler):** rejected — CAP.1's routing offered wiring *or* a ruling;
  the wirings were tractable and are what the spec's composed contracts
  name. Fail-closed refusal remains the fallback behaviour inside the
  wired paths (unattributed/underivable/error all refuse typed).
- **Defer the rung sweep again:** rejected — it is mechanical, the
  deferral names this ticket as an acceptable owner, and every OPEN
  audit-row blocks a later gate; `_TEMPLATE.md` now keeps it fixed.
- **New deferral rows for the endorsement/cadence residuals:** rejected —
  the members are already carried by DF-S2.4-1/DF-S2.8-1/DF-S2.9-3;
  duplicate rows would fork the residual ledger.

## Revisit trigger

A resume-after-answer path lands that must feed `endorse_asked` from
the durable decided fold (member b's owner), a host installs a live
egress broker, or a routed residual's owner ticket lands — each flips
its row's remaining members; GATE-ACCEPT signs or returns the D5/D6
set.
