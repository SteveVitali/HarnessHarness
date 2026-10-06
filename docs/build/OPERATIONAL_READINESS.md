# OPERATIONAL READINESS — capability picture at GATE-ACCEPT + REC.1 (2026-10-06)

> REC.1's honest account of how far each capability climbed. Status vocabulary:
> `engineered` → `fixture-verified` → `staging-verified` → `live-executed` →
> `public`; `human-completed` marks rows whose only remaining step is a person.
> No production claim is made without a citation; where nothing above
> `fixture-verified` can be cited, the row says so.

## Live read of record

- **ci-boundary/1** · `target/rec1-ci-boundary-chainTip.json` · read_at
  `2026-10-06T13:39:34Z` · sha256
  `c09d50e1b06defb7063b1bef8c20e08ec1d5afff12a02631b1bc9820cfa0fe28` ·
  result **pass**: PR #99 @ `82ef6b0` (build · test · fmt · drift, both runs)
  plus the open stack #98…#87 — all green within 24h of this commit.

## Capability table

| Capability (R-2.x family) | Code state | Infrastructure needed | Human owner + date | Highest layer | Proof |
|---|---|---|---|---|---|
| Harness assembly (R-2.1.x) | Compiler + sealed packages landed; R-2.1.4 residual legs on BL-10 | none beyond toolchain | operator review when round-2 lands | fixture-verified | `hh-assembly` suites; 29 MET + 6 MET-DIFFERENTLY rows in COVERAGE_MATRIX.csv |
| Ledger & recovery (R-2.2.x) | Durable event spine landed; retention/replay-driver legs on BL-11/BL-12/BL-13 | none beyond toolchain | — | fixture-verified | `hh-ledger`/`hh-env` suites; crash-recovery fixtures |
| Model plane (R-2.3.x) | Gateway + policy landed; live provider transports owed → BL-31 | provider endpoints + credentials | provisioner needed before first live call | fixture-verified | `hh-gateway` fake-transport suites; R-2.3.x verdicts |
| Context & memory (R-2.4.x) | Assembler wired (CAP.3); §5c battery + emitter legs on BL-15 | none beyond toolchain | — | fixture-verified | `hh-context`, `hh-embed/tests/cap_2_composed.rs` |
| Tools & exposure (R-2.5.x) | Registry + exposure lattice landed; residual bindings on BL-16 | sandbox runtime for hosted exec | — | fixture-verified | `hh-registry`/`hh-tools` suites |
| Control plane (R-2.6.x) | Policy/interpreter landed; steer + OOP-conformance legs on BL-14 | none beyond toolchain | — | fixture-verified | `hh-control` suites |
| Verification (R-2.7.x) | Gates/probes landed; conditioned-rule + judge legs on BL-17 | none beyond toolchain | — | fixture-verified | `hh-verification` suites |
| Security & governance (R-2.8.x) | Monitor/containment landed; egress-mediator + broker + IFC legs on BL-18/19/20/21/22/28 | TLS-terminating transport → BL-31 | — | fixture-verified | `hh-containment`, `hh-env/tests/acceptance.rs::cap2_dispatch_net_egress_mediated` |
| Telemetry & audit (R-2.9.1, R-2.8.6) | Typed streams landed; §5h.1 halves on BL-24, attestation on BL-21 | exporter endpoint when seam lands | — | fixture-verified | `hh-telemetry` suites |
| Lab: bench/eval/results (R-2.9.2–9.8, R-2.10.x) | Instruments landed; foreign lifts + fetch transports → BL-32 | remote endpoints + foreign vocab corpora | — | fixture-verified | `hh-bench`/`hh-eval`/`hh-results` suites |
| Surfaces (R-2.11.x) | CLI + embed + MCP labs landed; env verbs on BL-13, embed/MCP legs on BL-27 | none beyond toolchain | — | fixture-verified | `hh-cli`, `hh-embed`, `hh-mcp-lab` suites |
| Conformance (R-2.12.1–12.3) | In-ecosystem cells green; E2/E3 cross-cells gate-accepted → BL-01 | foreign toolchains (E2/E3) | human reviewer for R2 signature → BL-02 | fixture-verified | golden corpora + C12 CI measurement |
| Packaging/licensing (R-2.12.4) | Spec-deferred package (ADR-0210) → BL-33 | legal counsel | counsel at WS-L6 | engineered | ADR-0210 + scope row |
| Naming thesis (R-2.12.5) | Program-level thesis landed; market check deferred | naming/legal search at WS-L6 | operator at WS-L6 | engineered | ADR-0003/0006 |
| Fleet/organizational (R-2.12.6) | Adapter + fixture legs landed; real tracker + signed webhook → BL-04 | tracker instance + webhook | operator (HUMAN-H2) | fixture-verified | S5.5/S5.6 signed-run records |

## Ordered critical path to a production claim

Every step names its owning item (`ticket:`) and the evidence that would close
it (`proof:`). Nothing below asserts a claim that lacks a citation.

1. `ticket:` BL-02 — R2 cross-camp human reviewer signature. `proof:` signed
   reviewer record appended to the S0.3 measurement sheet.
2. `ticket:` BL-01 — E2/E3 byte-equality cells. `proof:` cross-ecosystem
   conformance run over the golden corpora with `EXIT=0`.
3. `ticket:` BL-31 — live provider transports + TLS-terminating transport.
   `proof:` live adapter impls + a probe-run record {id, `date -u`, result,
   sha256} against a real endpoint.
4. `ticket:` BL-03 — E3 surface-ecosystem binding (HUMAN-H1). `proof:` real
   E3 client bound to the generated surface + drift-check run record.
5. `ticket:` BL-04 — real tracker + signed webhook (HUMAN-H2). `proof:` AC-9
   replayed against the live source with a signed-event record.
6. `ticket:` BL-18..BL-22 — egress/broker/IFC/audit/approval residual legs.
   `proof:` the named executable batteries (AC-H4, LT-12, AT-H1) green.
7. `ticket:` BL-11..BL-17, BL-23..BL-28 — emitter/producer halves.
   `proof:` each matrix PARTIAL row re-verdicted `MET` with its test named.
8. `ticket:` BL-30 — spec/governance rulings (REC.2). `proof:` four spec-ruling
   cells written into the registers with ADR cross-refs.
9. `ticket:` BL-32 — remote fetch + foreign vocabularies (REC.3). `proof:`
   foreign-manifest ingest run + fetch-transport probe record.
10. `ticket:` BL-33/34/38 — WS-L6 window (packaging/naming/program candidates).
    `proof:` program-resume decision record.

## What is *not* claimed

- No `staging-verified`, `live-executed`, or `public` row exists: the build ran
  offline/hermetic by design and the only live read of record is the CI-boundary
  record above, which attests to the stacked PRs' checks — not to production
  execution.
- No secret material is recorded; every `provided:` cell stays `yes/no`.
