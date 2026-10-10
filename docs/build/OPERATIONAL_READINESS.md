# OPERATIONAL READINESS — capability picture at end of Round 2 (R2.26, 2026-10-10)

> R2.26's honest account of how far each capability climbed by the close of
> Round 2 (R2.1–R2.26 landed; **GATE-G4 PASSED** 2026-10-10 — operator
> blanket acceptance, the 28-row carried set dispositioned, the named-set
> human/env rows recorded gate-pending; R2.27's docs refresh states that
> outcome). Status
> vocabulary: `engineered` → `fixture-verified` → `staging-verified` →
> `live-executed` → `public`; `human-completed` marks rows whose only
> remaining step is a person. No production claim is made without a
> citation; where nothing above `fixture-verified` can be cited, the row
> says so. This document supersedes the REC.1 (2026-10-06) account — the
> delta is stated row-by-row below and summarized in the Round-2 accounting
> appended to `planning/2026-10-06_round2-decomposition.md`.

## Live read of record

- **ci-boundary/1** · `docs/build/logs/ci-R2.25.json` · read_at
  `2026-10-10T01:52:45Z` · result **pass**: PR #132 @ `c7be5d0`
  (build · test · fmt · drift — runs 38013901685, 38013904205).
  R2.26's and R2.27's own boundary reads are recorded at their
  closeouts (`logs/ci-R2.26.json`, `logs/ci-R2.27.json`); this row names
  the latest landed-chain read of the code chain.
- Round-2 head of chain: branch `svitali/harnessharness-r2.27` stacked on
  `svitali/harnessharness-r2.26` (R2.27 is docs-only); the verified ceiling
  across the stack is `fixture-verified` — no live read above it exists.

## Capability table

| Capability (R-2.x family) | Code state at end of R2 | Infrastructure needed | Human owner + date | Highest layer | Proof |
|---|---|---|---|---|---|
| Harness assembly (R-2.1.x) | Compiler + sealed packages landed (CAP.3); R-2.1.4's remaining leg — emitter-vs-exemption adjudication of the 11 no-document-path codes — rides DF-S1.9-2 → BL-10 (+ ADR-0350) | none beyond toolchain | operator disposition at GATE-G4 | fixture-verified | `hh-assembly` suites; R-2.1.x verdicts in COVERAGE_MATRIX.csv |
| Ledger & recovery (R-2.2.x) | Round-2 delta: HHZ1-tiered retention + GC bound set + replay driver landed (R2.2, ADR-0333); durable-exec producer legs incl. suspend/compensate/heal + `healing_policy_ref` landed (R2.3, ADR-0334). Carried: R-2.2.1's verdict is a spec-ruling cell (BL-30/ADR-0274); DF-S2.3-1's residual producers (resume_set, defer, child-lease, non-fleet ingress, heal surface) → BL-44 | none beyond toolchain | — | fixture-verified | `hh-ledger`/`hh-env` suites `r2_2.rs`/`r2_3.rs`; R-2.2.4/2.2.5 flipped MET at R2.26 |
| Model plane (R-2.3.x) | Model-plane producer legs landed (R2.7, ADR-0338); live provider transports owed → BL-31 | provider endpoints + credentials | provisioner needed before first live call | fixture-verified | `hh-gateway` fake-transport suites; R-2.3.x verdicts |
| Context & memory (R-2.4.x) | Round-2 delta: memory-store producer legs + context surfaces landed (R2.5, ADR-0336); DF-S2.8-1 members discharged → R-2.4.1/2.4.2 flipped MET at R2.26. Carried: §5c battery residuals DF-S1.19-1/-2 → BL-15 (+ ADR-0254/0255) | none beyond toolchain | — | fixture-verified | `hh-context` suites; `hh-embed/tests/cap_2_composed.rs` |
| Tools & exposure (R-2.5.x) | Round-2 delta: tool-exposure residual landed — PlanMap/composite bindings, code_mode, surface_rejected emitters (R2.8, ADR-0252/0286/0339); R-2.5.2 flipped MET in-round. Carried: DF-S1.17-1 members + sync_source/retrieval_eval fixture legs → BL-16 (+ ADR-0272) | sandbox runtime for hosted exec | — | fixture-verified | `hh-registry`/`hh-control` suites `r2_6.rs` |
| Control plane (R-2.6.x) | Round-2 delta: durable steer transport + queue_next_turn delivery + OOP conformance landed (R2.6, ADR-0337/0273). Carried: DF-S1.20-1 legs (delegation_reason on real runs, workflow/program interpreter legs) → BL-14 | none beyond toolchain | — | fixture-verified | `hh-control` suites |
| Verification (R-2.7.x) | Round-2 delta: diff_sanity conditioned rule + belief-probe emitters landed (R2.15, ADR-0257/0316). Carried: judge-binding legs DF-S1.21-1/-3 → BL-17; live-judge admissibility is environment-bound (ADR-0347/0348) | none beyond toolchain | — | fixture-verified | `hh-verification`/`hh-eval` suites `r2_16.rs` |
| Security & governance (R-2.8.x) | Round-2 delta: IFC labels_leaves walk + remedy-consumed machinery (R2.9a/b, R2.12 — R-2.8.2 flipped MET at R2.26); egress-mediator + decide_egress order (R2.9a/b); approvals modify + mid-dispatch ask (R2.11 — R-2.8.7 flipped MET in-round); extension-trust producers (R2.20). Carried: broker dpop/wrapped_long_lived → BL-19 (AEAD absent); audit per-event blob encryption + R-2.8.6 legs → BL-45 (AEAD absent, DF-S1.15-3); TLS-bound triggers → BL-31 | TLS-terminating transport → BL-31 | — | fixture-verified | `hh-containment`; `hh-env/tests/acceptance.rs::cap2_dispatch_net_egress_mediated` |
| Telemetry & audit (R-2.9.1, R-2.8.6) | Round-2 delta: §5h.1 deferral rows discharged — emitter classes, audit_view, ProcessMetric (R2.14 — R-2.9.1 flipped MET at R2.26); audit attestation + witnessed checkpoints + receiver receipts (R2.13, ADR-0248 answered). Carried: per-event encryption leg → BL-45; live observability-sink trigger ADR-0346 → BL-31 | exporter endpoint when seam lands | — | fixture-verified | `hh-telemetry` suites `r2_14.rs` |
| Lab: bench/eval/results (R-2.9.2–9.8, R-2.10.x) | Round-2 delta: eval residual landed — combined-failure, search_budget, LeakedSplit, FaultType (R2.16 — R-2.9.2 flipped MET at R2.26); task_id round-trip + resolver checks (R2.17/R2.18); DefinitionInput::Ref publishing + SSE (R2.19 — R-2.11.4 flipped MET at R2.26). Carried: live foreign-manifest import → BL-26 (+ ADR-0349); remote fetch + foreign vocabularies → BL-32 | remote endpoints + foreign vocab corpora | — | fixture-verified | `hh-bench`/`hh-eval`/`hh-results`/`hh-embed` suites |
| Surfaces (R-2.11.x) | Round-2 delta: env verbs + automatic snapshot cadence landed (R2.4, ADR-0335); parked-detach closing pass (R2.1) → R-2.11.1 flipped MET at R2.26; MCP SSE + definition publishing (R2.19 → R-2.11.4 MET). Carried: oauth/mtls mediator legs → BL-27 (+ ADR-0303/0351) | none beyond toolchain | — | fixture-verified | `hh-cli`, `hh-embed`, `hh-mcp-lab` suites `r2_19.rs` |
| Conformance (R-2.12.1–12.3) | Round-2 delta: foreign-toolchain bundle + self-check landed (R2.21, ADR-0353) — the environment prerequisite is now fully specified; E2/E3 cross-cells still environment-pending (gate-accepted item 9) → BL-01 | foreign toolchains (E2/E3) | human reviewer for R2 signature → BL-02 | fixture-verified | golden corpora + C12 CI measurement; `hh-embed/tests/conformance.rs` |
| Packaging/licensing (R-2.12.4) | Spec-deferred package (ADR-0210) → BL-33; unchanged in-round | legal counsel | counsel at WS-L6 | engineered | ADR-0210 + scope row |
| Naming thesis (R-2.12.5) | Program-level thesis landed; market check deferred — unchanged; CF-489 records the thesis-conformance divergence found at R2.24 | naming/legal search at WS-L6 | operator at WS-L6 | engineered | ADR-0003/0006 |
| Fleet/organizational (R-2.12.6) | Adapter + fixture legs landed; real tracker + signed webhook → BL-04 (HUMAN-H2, unprovisioned at R2.26) | tracker instance + webhook | operator (HUMAN-H2) | fixture-verified | S5.5/S5.6 signed-run records |
| Registers & spec-debt (ADR-0212–0216) | Round-2 delta: first revalidation sweeps landed — ADR-0212 pkg (R2.22), ADR-0213 pkg (R2.23), ADR-0214 pkg (R2.24), spec-debt first sweep (R2.25: 216 graded). Carried: all stay open at the named homes BL-35/36/37/39 (interim rules in force; 3 answered, 49 narrowed, rest open) | none beyond toolchain | — | fixture-verified | `registers/open-questions.md`, `spec-debt.md` dated sweep sections |

## Ordered critical path to a production claim

Every step names its owning item (`ticket:`) and the evidence that would
close it (`proof:`). Nothing below asserts a claim that lacks a citation.
The ordering is the dependency chain, not a schedule.

1. `ticket:` BL-02 — R2 cross-camp human reviewer signature. `proof:` signed
   reviewer record appended to the S0.3 measurement sheet.
2. `ticket:` BL-01 + BL-26 — E2/E3 byte-equality cells + live foreign-manifest
   import (HUMAN-H3 environment). `proof:` cross-ecosystem conformance run
   over the R2.21 bundle with `EXIT=0`; foreign-manifest ingest run record.
3. `ticket:` BL-31 + BL-27 — live provider transports, TLS-terminating
   transport, oauth/mtls mediator legs. `proof:` live adapter impls + a
   probe-run record {id, `date -u`, result, sha256} against a real endpoint.
4. `ticket:` BL-03 — E3 surface-ecosystem binding (HUMAN-H1). `proof:` real
   E3 client bound to the generated surface + drift-check run record.
5. `ticket:` BL-04 — real tracker + signed webhook (HUMAN-H2). `proof:` AC-9
   replayed against the live source with a signed-event record.
6. `ticket:` BL-17 — live-judge admissibility + judge-binding legs.
   `proof:` a live-judge run record admitted under ADR-0347/0348's conditions.
7. `ticket:` BL-19 + BL-45 — dpop / wrapped_long_lived + audit blob
   encryption. `proof:` a conforming pure-std AEAD (or an accepted departure
   ADR) + the named emitter/battery legs green.
8. `ticket:` BL-10 + BL-14 + BL-15 + BL-16 + BL-44 — in-build carried
   residuals (assembly adjudication; delegation/interpreter legs; §5c
   battery members; sync_source legs; durable-exec producers).
   `proof:` each matrix PARTIAL row re-verdicted `MET` with its test named —
   these are Round-3-shape work, dispositioned at GATE-G4.
9. `ticket:` BL-05 — gate-accepted executable-battery arms (live corpus/OOP
   arms signed at GATE-ACCEPT item 10). `proof:` the arms executed against
   live fixtures when the environments in steps 1–7 exist.
10. `ticket:` BL-30 — spec/governance rulings (REC.2). `proof:` the named
    spec-ruling cells written into the registers with ADR cross-refs
    (OQ-170 custody, OQ-388 ratification, CF-476/487/488/489/490/491).
11. `ticket:` BL-32 — remote fetch + foreign vocabularies (REC.3). `proof:`
    foreign-manifest ingest run + fetch-transport probe record.
12. `ticket:` BL-33/34/38 + BL-35/36/37/39 — WS-L6 window + register
    packages' remaining open questions. `proof:` program-resume decision
    record + per-OQ answered rows.
13. `ticket:` BL-41/42/43 — standing ledgers (re-issued, never closed).
    `proof:` the standing rules honoured — no proof owed.

## What is *not* claimed

- No `staging-verified`, `live-executed`, or `public` row exists: the build
  ran offline/hermetic by design and the only live read of record is the
  CI-boundary record above, which attests to the stacked PRs' checks — not
  to production execution. Round 2 landed no claim above `fixture-verified`.
- No secret material is recorded; every `provided:` cell stays `yes/no`.
- The environment-bound rows (BL-01, BL-02, BL-03, BL-04, BL-26, BL-27,
  BL-31) are honest carried debt, not defects: each names its provisioning
  precondition. The in-build residuals (BL-10, BL-14, BL-15, BL-16, BL-19,
  BL-44, BL-45) are enumerated for GATE-G4 with reason codes in the round-2
  accounting (`planning/2026-10-06_round2-decomposition.md`).
