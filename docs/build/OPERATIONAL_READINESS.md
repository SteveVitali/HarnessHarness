# OPERATIONAL READINESS — capability picture at end of Round 3 (R3.10, 2026-10-16)

> R3.10's honest account of how far each capability climbed by the close of
> Round 3 (R3.1–R3.10 landed on the stacked chain; **GATE-G5 dispositioned
> PASSED 2026-10-16** — operator verbatim "passed, proceed"; the dated
> Signature block of `docs/build/readouts/GATE-G5.md` records 16 carried
> rows CARRY to Round 4, 5 ACCEPT, 0 WAIVE over the 21-row set; the 19 owed
> DEFERRALS rows inherit their BL home's disposition). Status vocabulary:
> `engineered` → `fixture-verified` → `staging-verified` → `live-executed` →
> `public`; `human-completed` marks rows whose only remaining step is a
> person. **One evidence class exists between nothing and the in-tree runs:
> `foreign-run-verified`** — produced by a foreign toolchain's recorded run
> over the committed replay bundle (the PR #136 E2 legs), distinct from and
> never conflated with `fixture-verified`. **(2026-10-21, R4.5a /
> ADR-0363): `live-executed` is now claimable — only with a cited
> `hh-live-evidence/1` record under `docs/build/reports/live/` written by
> `scripts/live-verify.sh`; the first is `2026-10-21-local-reachability.json`
> (local Ollama, USD 0). `staging-verified`/`public` remain unearned until
> L4.6/R4.18.** No production claim is made
> without a citation; where nothing above `fixture-verified` can be cited,
> the row says so. This document supersedes the R2.26 (2026-10-10) account —
> the delta is stated row-by-row below and summarized in the Round-3
> accounting appended to
> `planning/2026-10-14_round3-decomposition.md`.

## Live read of record

- **ci-boundary/1** · `docs/build/logs/ci-R3.10.json` · read_at
  `2026-10-16T20:09:09Z` · result **pass**: PR #146 @ `bdc0a9b`
  (build · test · fmt · drift; the stack read at the GATE-G5
  disposition head — #137–#146 + #135 all OPEN, all `pass`). This row
  names the latest landed-chain read of the code chain.
- Round-3 head of chain: branch `svitali/harnessharness-r3.10` stacked on
  `svitali/harnessharness-r3.9` (R3.10 is docs/records-only); the verified
  ceiling across the stack stays `fixture-verified` — no live read above it
  exists. The workspace's dependency posture changed in-round: `ring =
  0.17.14` is admitted to `hh-secrets` only, under ADR-0359's closed scope
  (DPoP signature verification + test-time fixture signing + the
  `aead`/`rand` surface R3.8 consumed for blob-pool AEAD under ADR-0361) —
  the single named, pinned exception to the hermetic-deps contract, ruled
  by the operator (HUMAN-H5 disposition (b), 2026-10-14). Every other crate
  and every other `ring` surface stays hermetic.

## Capability table

| Capability (R-2.x family) | Code state at end of R3 | Infrastructure needed | Human owner + date | Highest layer | Proof |
|---|---|---|---|---|---|
| Harness assembly (R-2.1.x) | R-2.1.4's emitter-vs-exemption adjudication landed at R3.6 (ADR-0358 — DF-S1.9-2 DONE; C-LINK-8/-9 gained emitters; nine owner rulings; the §3.3.8 `seal`-stage taxonomy gap registered CF-492 → BL-30); R-2.1.4 flipped MET | none beyond toolchain | — | fixture-verified | `hh-assembly` suites; R-2.1.x verdicts in COVERAGE_MATRIX.csv |
| Ledger & recovery (R-2.2.x) | Round-3 delta: DF-S2.3-1's five residual producers landed at R3.2 (BL-44 closed); DF-S1.20-1's control legs at R3.3; R-2.2.3 flipped MET; per-event blob encryption landed at R3.8 (ADR-0361 `HHBE`/`BlobCipher`/`RingBlobCipher`; DF-S1.15-3 DONE; BL-45 closed). Carried: R-2.2.1 stays PARTIAL — OQ-388 ratification is a ruling leg (BL-30), the cross-impl cells ride BL-01/BL-02 | none beyond toolchain | — | fixture-verified | `hh-ledger`/`hh-env` suites `r3_2.rs`; R-2.2.x verdicts |
| Model plane (R-2.3.x) | Unchanged in-round; live provider transports owed → BL-31 | provider endpoints + credentials | provisioner needed before first live call | fixture-verified | `hh-gateway` fake-transport suites; R-2.3.x verdicts |
| Context & memory (R-2.4.x) | Round-3 delta: the §5c residual members landed at R3.4 — DF-S1.19-1/-2 DONE, R-2.4.1/2.4.2/2.4.3/2.4.4 flipped MET (BL-15 closed; ADR-0356) | none beyond toolchain | — | fixture-verified | `hh-embed/tests/r3_4.rs`; `hh-context` suites |
| Tools & exposure (R-2.5.x) | Round-3 delta: DF-S1.17-1's residual members landed at R3.5 — `deferred_tools`/`tool_search` flags on the minimal profiles, `lower_provider` deferral block, `surface_rejection_rate` telemetry fold (ADR-0357; BL-16 closed); R-2.5.3 flipped MET | sandbox runtime for hosted exec | — | fixture-verified | `hh-compiler`/`hh-telemetry`/`hh-bench` r3_5 batteries |
| Control plane (R-2.6.x) | Round-3 delta: DF-S1.20-1's interpreter legs landed at R3.3 — delegation_reason on real runs, workflow/program interpreters (BL-14 closed); R-2.6.1/R-2.6.3 flipped MET | none beyond toolchain | — | fixture-verified | `hh-control`/`hh-orchestrator` r3_3 batteries |
| Verification (R-2.7.x) | Unchanged in-round. Carried: judge-binding leg DF-S1.21-3 → BL-17 (env — no live judge admissible); DF-S1.21-2's residual arms are gate-accepted → BL-05 | none beyond toolchain | — | fixture-verified | `hh-verification`/`hh-eval` suites |
| Security & governance (R-2.8.x) | Round-3 delta: the credential-broker residual landed at R3.7 — `dpop` verifies over `ring = 0.17.14` (Ed25519 + ES256 fixture-signed proofs on bind/mediate/mint/verify_minted/deliver_wrapped, fail-closed `SenderConstraintUnmet` kept) and `wrapped_long_lived` delivers with durable-before-visible revocation (ADR-0359/0360; DF-S1.13-1/-4 DONE; BL-19 closed); R-2.8.3 + R-2.8.4 read `MET-ENGINEERED(DF-S2.4-2)` — every engineering leg landed, the TLS-terminating transport leg stays env-gated → BL-31. Carried: signer custody ruling (DF-S2.5-1) → BL-30; battery halves → BL-05 | TLS-terminating transport → BL-31 | — | fixture-verified | `hh-secrets/tests/r3_7.rs`; `cap2_dispatch_net_egress_mediated` |
| Telemetry & audit (R-2.9.1, R-2.8.6) | Round-3 delta: per-event blob encryption landed at R3.8 — `ring::aead` AES-256-GCM, per-blob derived keys, fresh nonces; `key_unavailable`/`BlobCorrupt`/`CipherFault` typing, plaintext-address preserved, `audit_view` encryption member (ADR-0361; DF-S1.15-3 DONE; BL-45 closed); R-2.8.6 stays PARTIAL on the custody legs (BL-30/BL-05). Carried: live observability-sink trigger ADR-0346 → BL-31 | exporter endpoint when seam lands | — | fixture-verified | `hh-ledger`/`hh-secrets` r3_8 batteries |
| Lab: bench/eval/results (R-2.9.2–9.8, R-2.10.x) | Unchanged in-round. Carried: live foreign-manifest import → BL-26 (+ ADR-0349); remote fetch + foreign vocabularies → BL-32; `DebtPolicy::default()` conformed to spec at R3.9 (CF-489 resolved — 60 d/30 d) | remote endpoints + foreign vocab corpora | — | fixture-verified | `hh-bench`/`hh-eval`/`hh-results`/`hh-embed` suites |
| Surfaces (R-2.11.x) | Unchanged in-round. Carried: oauth/mtls mediator legs → BL-27 (+ ADR-0303/0351); R-2.11.3 stays PARTIAL on DF-S4.11-2 | none beyond toolchain | — | fixture-verified | `hh-cli`, `hh-embed`, `hh-mcp-lab` suites |
| Conformance (R-2.12.1–12.3) | Round-3 delta: the E2 candidate (`foreign/e2-python`) replayed all six `hh-xcheck-bundle/1` arms byte-for-byte via `hh-xcheck verify` — 49/49 (PR #136, 2026-10-14; HUMAN-H3 material provided): identity-trees 22/22, identity-records 4/4, ledger-transcript 3/3, registry-snapshot 1/1, plugin-manifests 14/14, canonical-parse 6/6 incl. timing members — **`foreign-run-verified` evidence on the five covered DEFERRALS rows, never a fixture/native claim**. Still env-pending: E3 replay, the HIR/ledger-corpus extension, the two-toolchain C12 cells → BL-01 (gate-accepted item 9); R-2.12.1 stays PARTIAL. The bundle's status vocabulary cannot yet express a partially-foreign-discharged cell — CF-493 registered → BL-30 | foreign toolchains (E3; E2's remaining legs) | human reviewer for R2 signature → BL-02 | fixture-verified (in-tree) + foreign-run-verified (E2 legs) | `hh-xcheck` bundle + dated DEFERRALS notes (PR #136); `hh-embed/tests/conformance.rs` |
| Packaging/licensing (R-2.12.4) | Spec-deferred package (ADR-0210) → BL-33; unchanged in-round | legal counsel | counsel at WS-L6 | engineered | ADR-0210 + scope row |
| Naming thesis (R-2.12.5) | Program-level thesis landed; market check deferred — unchanged; CF-489's conformance divergence **resolved at R3.9** (code conformed to spec) | naming/legal search at WS-L6 | operator at WS-L6 | engineered | ADR-0003/0006 |
| Fleet/organizational (R-2.12.6) | Adapter + fixture legs landed; real tracker + signed webhook → BL-04 (HUMAN-H2, still unprovisioned) | tracker instance + webhook | operator (HUMAN-H2) | fixture-verified | S5.5/S5.6 signed-run records |
| Registers & spec-debt (ADR-0212–0216) | Unchanged in-round — all stay open at the named homes BL-35/36/37/39 (interim rules in force); OQ-441's Stage-5 `DebtIndex` re-pin stays with WS-I6 | none beyond toolchain | — | fixture-verified | `registers/open-questions.md`, `spec-debt.md` dated sweep sections |

## Ordered critical path to a production claim

Every step names its owning item (`ticket:`) and the evidence that would
close it (`proof:`). Nothing below asserts a claim that lacks a citation.
The ordering is the dependency chain, not a schedule.

1. `ticket:` BL-02 — R2 cross-camp human reviewer signature. `proof:` signed
   reviewer record appended to the S0.3 measurement sheet.
2. `ticket:` BL-01 + BL-26 — E3 byte-equality cells + the HIR/ledger-corpus
   extension + the C12 two-toolchain cells, and the live foreign-manifest
   import (HUMAN-H3 environment; the E2 legs already carry
   foreign-run-verified evidence per PR #136). `proof:` cross-ecosystem
   conformance run over the R2.21 bundle for E3 with `EXIT=0`;
   foreign-manifest ingest run record.
3. `ticket:` BL-31 + BL-27 — live provider transports, the TLS-terminating
   transport (DF-S2.4-2 — the residual R-2.8.3/R-2.8.4 both carry),
   oauth/mtls mediator legs. `proof:` live adapter impls + a probe-run
   record {id, `date -u`, result, sha256} against a real endpoint.
4. `ticket:` BL-03 — E3 surface-ecosystem binding (HUMAN-H1). `proof:` real
   E3 client bound to the generated surface + drift-check run record.
5. `ticket:` BL-04 — real tracker + signed webhook (HUMAN-H2). `proof:` AC-9
   replayed against the live source with a signed-event record.
6. `ticket:` BL-17 — live-judge admissibility + judge-binding legs.
   `proof:` a live-judge run record admitted under ADR-0347/0348's
   conditions.
7. `ticket:` BL-05 — gate-accepted executable-battery arms (live corpus/OOP
   arms signed at GATE-ACCEPT item 10). `proof:` the arms executed against
   live fixtures when the environments in steps 1–6 exist.
8. `ticket:` BL-30 — spec/governance rulings (REC.2). `proof:` the named
   spec-ruling cells written into the registers with ADR cross-refs
   (OQ-170 custody, OQ-388 ratification, CF-476/487/488/490/491,
   CF-492 `seal`-stage taxonomy, CF-493 `pending_cells`/`arms[].status`
   vocabulary).
9. `ticket:` BL-32 — remote fetch + foreign vocabularies (REC.3). `proof:`
   foreign-manifest ingest run + fetch-transport probe record.
10. `ticket:` BL-33/34/38 + BL-35/36/37/39 — WS-L6 window + register
    packages' remaining open questions. `proof:` program-resume decision
    record + per-OQ answered rows.
11. `ticket:` BL-41/42/43 — standing ledgers (re-issued, never closed).
    `proof:` the standing rules honoured — no proof owed.

## What is *not* claimed

- No `staging-verified`, `live-executed`, or `public` row exists: the build
  ran offline/hermetic by design and the only live read of record is the
  CI-boundary record above, which attests to the stacked PRs' checks — not
  to production execution. Round 3 landed no claim above `fixture-verified`.
- The `foreign-run-verified` E2 evidence (PR #136) is a foreign toolchain's
  recorded replay of the committed bundle — it is not an in-tree
  verification, not a live-service claim, and does not lift the
  `fixture-verified` ceiling; the E3/C12/corpus-extension legs remain
  `environment-pending`.
- No live provider call, live TLS termination, live judge, real tracker or
  signed webhook, real OAuth/mTLS exchange, E3 binding, or live
  foreign-manifest import is claimed — each names its provisioning
  precondition on BL-01…BL-04, BL-17, BL-26, BL-27, BL-31.
- No secret material is recorded; every `provided:` cell stays `yes/no`
  (HUMAN-H3 material was *provided* as the PR #136 run record; no
  credential entered a ledger).
- The environment-bound rows (BL-01, BL-02, BL-03, BL-04, BL-26, BL-27,
  BL-31) are honest carried debt, not defects: each names its provisioning
  precondition — they read *excluded pending HUMAN-H1/H2/H3 provisioning*
  (the paths are live, nothing is provisioned yet), never a fabricated
  pass. The round-3 round-closure set — the discharged rows
  BL-10/14/15/16/19/44/45, the rulings on BL-30, the env/human/accepted
  residuals, and the standing rows BL-41/42/43 — was enumerated with
  reason codes in the round-3 accounting
  (`planning/2026-10-14_round3-decomposition.md`) and dispositioned at
  GATE-G5 (2026-10-16: 16 CARRY / 5 ACCEPT / 0 WAIVE).

*Amended in place 2026-10-16 (R3.11):* the GATE-G5 outcome and the
latest ci-boundary read are now recorded above; every capability row
otherwise stands as written at R3.10.
