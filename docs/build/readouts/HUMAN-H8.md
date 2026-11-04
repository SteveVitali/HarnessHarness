# HUMAN-H8 — Bind actual backend, inputs, rights and stage resources · readout

- **Date:** 2026-11-03 · **Status:** COMPLETE — operator supply recorded verbatim; exit criterion met
- **Contract:** `docs/tickets/213_HUMAN-H8__inputs.md` · **Blocks:** GATE-G18, GATE-G11, GATE-G12, GATE-G13, GATE-G14
- **Precondition met:** R5.42 landed (PR #256, `ce45df58`) — the quote/readiness engine exists; it validates supplied evidence and never manufactures authentic runs.
- **Operator words (verbatim, interactive answers 2026-11-03T23:17:50Z):**
  - Named owner: "Steven Vitali (@SteveVitali)" — verified owner of the study/artifact inputs.
  - Backend/access: "Verified yes" — backend confinement/observer/probe feasibility and original-runner access verified yes, attested by the named owner as provenance.
  - Inputs source: "In-tree pinned artifacts" — upstream task/oracle/runner declarations are the pinned artifacts in the repo; hash-verified below.
  - Rights: "Landed defaults" — public_review for releasable evidence members; restricted for held-out roles (solution/oracle/model_io); controlled for recipient-procedure items.
  - Cost posture: "Minimum admission — confirm floor values" — the declared §9.2 worksheet below.
  - Stage scope: "All five stages" — S1/S4, activation, pilot, parity, confirmation each admitted.
  - Credentials: "Yes" — provided, values never recorded.
  - Gap handling: "Full pause" — no gaps remained; all items supplied.
- **Agent-drafted block sha256:** `cea0bff61caafb8cff38e63e22f1740d813bf501b0061eede2d9aec0d12b635e`

## Checklist (verbatim, recorded)

- [x] **Study/operator and environment/artifact owners supply** unchanged upstream task/oracle/runner/image/model/roles — **in-tree pinned artifacts**, hash-verified `2026-11-03T23:17:50Z`:
  - `crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/` (5 task declarations) — aggregate tail `…1fc87a4` (a-upper sha256 `3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4`)
  - `scripts/examples/ex-01.json` — sha256 `0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3`
  - `scripts/examples/ex-02.json` — sha256 `8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285`
  - `scripts/examples/ex-02-fixture.json` — sha256 `f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8`
  - `clients/ts/fixture-definition.json` — sha256 `732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8`
  - backend confinement/observer/probe feasibility: **verified yes** (operator attestation); original-runner access: **verified yes**; exact rights: **landed defaults** (public_review / restricted-held-out / controlled recipient-procedure); full §9.2 worksheet: **minimum admission** (below); separate choices recorded for **all five stages**; credentials: **yes** (values never recorded). No task/container/provider execution follows from this supply.
- [x] **Record** named owner (Steven Vitali, @SteveVitali), exact UTC (`2026-11-03T23:17:50Z`), verification evidence (in-tree hashes above + operator attestations), bounded scope (all five stages, quote-only for parity/confirmation) and withheld claims (execution/participant/publication acts stay withheld until the named gates decide) — recorded here and in the ledger.

## Declared §9.2 worksheet — minimum admission (operator-confirmed floor values)

| stage | subjects | R | retries | regrades | instrument lines | archive/version/backup/retention | free components | provider_unknown_exposure |
|---|---|---|---|---|---|---|---|---|
| pilot | N=10T floor (2·T·R≥10T → T=1,R=5) | 5 | 1 | 1 | 1× required copy each | 1× required copy each | USD $0.00 (scratch stores, in-tree fixtures, local compute) | 0 (declared) |
| activation | per floor | — | — | — | — | — | USD $0.00 | 0 |
| S1/S4 | per floor | — | — | — | — | — | USD $0.00 | 0 |
| parity | — | — | — | — | — | — | USD $0.00 | 0 |
| confirmation | — | — | — | — | — | — | USD $0.00 | 0 |

Parity + confirmation stay **quote-only** (no allocated tranche — spec default). USD currency. The quote engine computes exposure from these declared values verbatim; any unbound pin/rights/exposure line blocks that stage's action.

## Signature

**COMPLETE** — supplied by Steven Vitali (@SteveVitali) via interactive answers at `2026-11-03T23:17:50Z`; recorded verbatim. Named owner attestation is the provenance for backend/access; in-tree hashes are the verification for the task/oracle/runner identities.
