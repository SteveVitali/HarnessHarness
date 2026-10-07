<!--
  docs/build/TICKET_VS_SPEC.md — reconcile-build mode=spec deliverable (BM-RECON-02).
  Written by REC.2 (2026-10-06). Append-only.
-->
# TICKET vs SPEC — what each landed ticket shipped, tagged against `spec/CANONICAL_SPEC.md`

**Tags.** `in-spec` — the spec required the deliverable (the ticket's stamped `R-2.*` ids; the
requirement is the citation). `spec-implied` — a reasonable reading of the spec, not stated
verbatim (the E1 build binding under ADR-0050 §8/CC4; MUST-data interim rulings under the
ADR-0210–0216 deferred questions; honest-proxy substitutes under the spec's `n/a{reason}`
discipline). `ticket-added` — the ticket introduced it beyond the spec (program/build
infrastructure, tooling, retained spike artifacts, generated artifacts).

**Dispositions.** `ID` — leave as an implementation detail (no spec change; the item lives
correctly in code/build memory). `AX` — note in an appendix. `FB→plan` — fold-back candidate
carried to `SPEC_RECONCILIATION_PLAN.md` (new or amended requirement text — proposed, unticked).

**Scope note.** Rows are per landed chain row (BUILD_INDEX seq). Deliverable groups are tagged,
not every file — a ticket's `in-spec` cell names the requirement ids its slice discharges, the
`spec-implied`/`ticket-added` cells name the non-stamp deliverables. The four BL-30 ruling rows
(DF-S2.5-1, DF-S4.13-1, DF-S1.24-3, DF-S1.26-2) surface here as `FB→plan`.

| seq | ticket | in-spec deliverables (req ids) | spec-implied deliverables | ticket-added deliverables | ADRs |
|---|---|---|---|---|---|
| 1 | S0.1 | `R-2.11.4`, `R-2.12.3` `leave as implementation detail` | the schema-export → codegen → check-drift pipeline as the single-schema-source mechanism (CC7; ADR-0218) `ID`; the E1 Rust toolchain binding of the language-neutral contracts (ADR-0050 §8; ADR-0217) `ID` | repository layout + CI pipeline (ADR-0217) `ID`; the retained boundary-spike harness and its first-measurement report `docs/build/reports/S0.1-boundary-spike.md` (ADR-0220) `ID` | ADR-0217, ADR-0218, ADR-0219, ADR-0220 |
| 2 | S0.2 | `R-2.1.2`, `R-2.1.5`, `R-2.1.6`, `R-2.11.1`, `R-2.11.4`, `R-2.2.5`, `R-2.3.1`, `R-2.3.2`, `R-2.4.1`, `R-2.5.5`, `R-2.6.1`, `R-2.6.2`, `R-2.8.3`, `R-2.9.1` `leave as implementation detail` | the throwaway `hh-baseline` crate as the §9.1/doc-3-§11.0 de-risking spike, deleted at the Stage-0 boundary (ADR-0221) `ID`; the Stage-0 interim exit-class numeral mapping (ADR-0222) `ID` | — | ADR-0221, ADR-0222 |
| 3 | S0.3 | `R-2.12.3`, `R-2.2.5`, `R-2.5.5` `leave as implementation detail` | the S1/S2 measurement spikes as the Stage-0 acceptance check (§9.1; ADR-0223/0224) `ID` | the measurement sheet + throwaway spike trees `spikes/` (R1–R6, committed-but-deletable) `ID` | ADR-0223, ADR-0224 |
| 3a | S0.3b | `R-2.12.3`, `R-2.2.5`, `R-2.5.5` `leave as implementation detail` | the online cross-candidate spike closing DF-S0.3-1/-3's machine cells (official-SDK MCP/ACP round-trip, E2/E3 byte-identity, cross-candidate C5/C7 + E5b/E5c; ADR-0225/0226) `ID` | `spikes/s0.3b-online-spike/` + sheet extension (inserted-ticket scope, manifest Plan-extensions row) `ID` | ADR-0225, ADR-0226 |
| 5 | S1.1 | `R-2.1.1` `leave as implementation detail` | `hh-ontology` as the E1 build binding of §2 (CC4; ADR-0227) `ID`; the per-kind `classify_home` delegation to S1.4 under ADR-0216/OQ-467 (ADR-0228) `ID` | — | ADR-0227, ADR-0228 |
| 6 | S1.2 | `R-2.12.1` `leave as implementation detail` | `hh-identity`/`idp/1` as the E1 binding of §8.3 (ADR-0229) `ID` | the codegen `EXPECTED_SCHEMA_HASH` pin + drift artifacts `ID` | ADR-0229 |
| 7 | S1.3 | `R-2.1.2`, `R-2.1.5` `leave as implementation detail` | `hh-provenance` crate shape below `hh-identity` (ADR-0230) `ID` | — | ADR-0230 |
| 8 | S1.4 | `R-2.1.2` `leave as implementation detail` | `hh-hir` crate shape + seal-order/leaf homes (ADR-0231) `ID`; the closed-world tool minting ruling (ADR-0232) `ID` | — | ADR-0231, ADR-0232 |
| 9 | S1.5 | `R-2.10.6`, `R-2.2.1`, `R-2.2.3` `leave as implementation detail` | `hh-ledger` crate shape and error additions (ADR-0233) `ID`; the C0 persistence-policy class table as MUST-data interim rulings (ADR-0234) `ID`; hash-chain/WAL/lease mechanics rulings (ADR-0235) `ID` | — | ADR-0233, ADR-0234, ADR-0235 |
| 10 | S1.6 | `R-2.1.6` `leave as implementation detail` | `hh-budget` + the closed kernel dimension registry (ADR-0236/0237) `ID` | — | ADR-0236, ADR-0237 |
| 11 | S1.7 | `R-2.2.2`, `R-2.2.3` `leave as implementation detail` | the attempt-vs-effect terminality rulings (ADR-0238) `ID` | — | ADR-0238 |
| 12 | S1.8 | `R-2.10.2` `leave as implementation detail` | the registry-store shape and persistence rulings (ADR-0239) `ID` | — | ADR-0239 |
| 13 | S1.9 | `R-2.1.4` `leave as implementation detail` | `hh-assembly` crate shape, assembly-in-HIR and resolver rulings (ADR-0240) `ID` | — | ADR-0240 |
| 14 | S1.10 | `R-2.1.3` `leave as implementation detail` | `hh-compiler` crate shape and the snapshot-view rulings (ADR-0241) `ID` | the `hh-compile` binary `ID` | ADR-0241 |
| 15 | S1.11 | `R-2.8.1` `leave as implementation detail` | `hh-monitor` crate shape + completeness-guard rulings (ADR-0242) `ID` | — | ADR-0242 |
| 16 | S1.12 | `R-2.8.1`, `R-2.8.4` `leave as implementation detail` | `hh-containment` crate shape + recorded-gate edge (ADR-0243) `ID` | — | ADR-0243 |
| 17 | S1.13 | `R-2.8.3` `leave as implementation detail` | `hh-secrets` crate shape + the PDP/CDP grammar (ADR-0244) `ID` | — | ADR-0244 |
| 18 | S1.14 | `R-2.9.1` `leave as implementation detail` | `hh-telemetry` crate shape (ADR-0245) `ID`; the measurement-convention and sink-policy interim rulings as MUST-data (ADR-0246/0247) `ID` | — | ADR-0245, ADR-0246, ADR-0247 |
| 19 | S1.15 | `R-2.8.6` `leave as implementation detail` | the Rule-C audit-grade partition encoding ruling (ADR-0248) `ID` | — | ADR-0248 |
| 20 | S1.16 | `R-2.2.5`, `R-2.5.5` `leave as implementation detail` | `hh-env` crate shape + `EnvHandle`/`EnvDriver` (ADR-0249) `ID`; the seven-stage `Dispatcher` rulings (ADR-0250) `ID` | — | ADR-0249, ADR-0250 |
| 21 | S1.17 | `R-2.5.1`, `R-2.5.2`, `R-2.5.3` `leave as implementation detail` | `ToolCapability` in HIR + derived verb surface (ADR-0251) `ID`; the extended `SurfaceBinding`/exposure rulings (ADR-0252) `ID` | — | ADR-0251, ADR-0252 |
| 22 | S1.18 | `R-2.3.1`, `R-2.3.2`, `R-2.3.3`, `R-2.3.4` `leave as implementation detail` | `hh-gateway` boundary-crate rulings (ADR-0253) `ID` | — | ADR-0253 |
| 23 | S1.19 | `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.4` `leave as implementation detail` | `hh-context` crate + context-builder rulings (ADR-0254) `ID`; the memory-slice `put`-order/endpoint rulings (ADR-0255) `ID` | — | ADR-0254, ADR-0255 |
| 24 | S1.20 | `R-2.6.1`, `R-2.6.2` `leave as implementation detail` | the control-strategy/envelope rulings (ADR-0256) `ID` | — | ADR-0256 |
| 25 | S1.21 | `R-2.7.1`, `R-2.7.2a`, `R-2.7.2b`, `R-2.7.3` `leave as implementation detail` | `hh-verification` claim-ledger rulings (ADR-0257) `ID` | — | ADR-0257 |
| 26 | S1.22 | `R-2.9.2` `leave as implementation detail` | the eval-framework rulings — one complete `MetricDeclaration` (ADR-0258) `ID` | — | ADR-0258 |
| 27 | S1.23 | `R-2.8.5`, `R-2.8.7` `leave as implementation detail` | the approvals + extension-trust rulings (ADR-0259) `ID` | — | ADR-0259 |
| 28 | S1.24 | `R-2.10.3`, `R-2.10.4`, `R-2.10.5`, `R-2.9.4`, `R-2.9.6`, `R-2.9.8` `leave as implementation detail` | `hh-lab` schema-plane rulings (ADR-0260) `ID`; the `debt.hypothesis` string-vs-`Text` interim (DF-S1.24-3 → ruling issued at ADR-0331 D1) `FB→plan` | — | ADR-0260 |
| 29 | S1.25 | `R-2.11.4` `leave as implementation detail` | `hh-embed/1` as the single boundary contract owning ADR-0176–0179 (ticket-owned; no new ADR) `ID` | the `hh-embed-client-generated` crate + check-drift wiring `ID` | ADR-0176, ADR-0177, ADR-0178, ADR-0179 (ticket-owned; no new ADR) |
| 30 | S1.26 | `R-2.11.1` `leave as implementation detail` | `hh-cli` as a thin generated-client surface (ADR-0261) `ID`; the interim `detached:"parked"` result member (DF-S1.26-2 → fold-back ruling at ADR-0331 D2) `FB→plan` | — | ADR-0261 |
| 31 | S1.27 | `R-2.12.2` `leave as implementation detail` | `hh-plugin` contract-surface rulings (ADR-0262) `ID` | `hh-plugin-fixture` binary `ID` | ADR-0262 |
| 32 | S2.1 | `R-2.2.5`, `R-2.5.5` `leave as implementation detail` | `hh-helper/1` + the helper-binary layer (ADR-0050 helper ecosystem; ADR-0263) `ID` | the `hh-helper` binary (seatbelt/podman/fstree backends) `ID` | ADR-0263 |
| 33 | S2.2 | `R-2.12.2` `leave as implementation detail` | `hh-varhost` over `hh-helper` (ADR-0264) `ID` | — | ADR-0264 |
| 34 | S2.3 | `R-2.2.2`, `R-2.2.3`, `R-2.2.4` `leave as implementation detail` | the checkpoint-as-projection durable-execution rulings (ADR-0265) `ID` | — | ADR-0265 |
| 35 | S2.4 | `R-2.8.3`, `R-2.8.4` `leave as implementation detail` | the resolve-only egress-mediation rulings (ADR-0266) `ID` | — | ADR-0266 |
| 36 | S2.5 | `R-2.8.6` `leave as implementation detail` | the C0 HMAC signed-checkpoint construction + `AuditSigner`/`AuditKeyResolver` custody seam (ADR-0267) `ID`; the kernel-custody/checkpoint-cadence interim under ADR-0213/OQ-170 (DF-S2.5-1 → ruling owed; ADR-0331 D3) `FB→plan` | — | ADR-0267 |
| 37 | S2.6 | `R-2.8.1`, `R-2.8.7` `leave as implementation detail` | the out-of-process `authorize` rulings (ADR-0268) `ID` | the `hh-authorize` binary `ID` | ADR-0268 |
| 38 | S2.7 | `R-2.8.2`, `R-2.8.4` `leave as implementation detail` | the IFC/taint rulings incl. the per-basis `approval`/`policy_rule` table (ADR-0269) `ID` | — | ADR-0269 |
| 39 | S2.8 | `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.4`, `R-2.4.5` `leave as implementation detail` | the `ProcedureProfile/1`, context-label and compaction rulings (ADR-0270) `ID` | — | ADR-0270 |
| 40 | S2.9 | `R-2.2.1`, `R-2.2.4` `leave as implementation detail` | the logical-HEAD/WAL branch-model rulings (ADR-0271) `ID` | — | ADR-0271 |
| 41 | S2.10 | `R-2.11.1`, `R-2.3.4`, `R-2.5.3` `leave as implementation detail` | the C1 tool-scaling scorer and CLI-surface rulings (ADR-0272) `ID` | — | ADR-0272 |
| 42 | S2.11 | `R-2.5.1`, `R-2.6.1`, `R-2.6.2`, `R-2.7.1` `leave as implementation detail` | the steerable-control/environment-binding rulings (ADR-0273) `ID` | — | ADR-0273 |
| 43 | S2.12 | `R-2.10.2`, `R-2.11.1`, `R-2.12.1`, `R-2.12.2`, `R-2.3.4` `leave as implementation detail` | the attended-CLI + registry-enforcement rulings (ADR-0274; D9 retains the parked-detach interim → fold-back at ADR-0331 D2) `FB→plan` | — | ADR-0274 |
| 44 | S3.1 | `R-2.11.1`, `R-2.11.3`, `R-2.11.4`, `R-2.9.3` `leave as implementation detail` | the lab-boundary/`hh-embed` Groups M/L + bundle rulings (ADR-0275) `ID` | — | ADR-0275 |
| 45 | S3.2 | `R-2.1.2`, `R-2.1.3`, `R-2.1.4` `leave as implementation detail` | the lowering/composition rulings (ADR-0276) `ID` | — | ADR-0276 |
| 46 | S3.3 | `R-2.9.2`, `R-2.9.4` `leave as implementation detail` | the eval-kernel and benchmark-adapter rulings (ADR-0277) `ID` | — | ADR-0277 |
| 47 | S3.4a | `R-2.10.2`, `R-2.10.3` `leave as implementation detail` | the experiment-engine rulings (ADR-0278) `ID` | — | ADR-0278 |
| 48 | S3.4b | `R-2.10.5` `leave as implementation detail` | the results-store rulings (ADR-0279) `ID` | — | ADR-0279 |
| 49 | S3.4c | `R-2.10.4` `leave as implementation detail` | the estimator-kernel rulings (ADR-0280) `ID` | — | ADR-0280 |
| 50 | S3.4d | `R-2.10.6` `leave as implementation detail` | the Hosting-ABI schema rulings (ADR-0281) `ID` | — | ADR-0281 |
| 51 | S3.5 | `R-2.10.1` `leave as implementation detail` | the Assembly-service rulings (ADR-0282) `ID` | — | ADR-0282 |
| 52 | S3.6 | `R-2.2.1`, `R-2.2.2`, `R-2.2.3`, `R-2.2.4` `leave as implementation detail` | the replay/fault-battery/counterfactual rulings (ADR-0283) `ID` | — | ADR-0283 |
| 53 | S3.7 | `R-2.3.1`, `R-2.3.2`, `R-2.3.3`, `R-2.3.4` `leave as implementation detail` | the model-plane evaluation rulings (ADR-0284) `ID` | — | ADR-0284 |
| 54 | S3.8 | `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.4`, `R-2.4.5` `leave as implementation detail` | the context/memory evaluation rulings (ADR-0285) `ID` | — | ADR-0285 |
| 55 | S3.9 | `R-2.5.1`, `R-2.5.2`, `R-2.5.3`, `R-2.5.4` `leave as implementation detail` | the protocol-edge rulings (ADR-0286) `ID` | — | ADR-0286 |
| 56 | S3.10 | `R-2.6.1`, `R-2.6.2`, `R-2.7.1`, `R-2.7.2a`, `R-2.7.3` `leave as implementation detail` | the control/verification/eval rulings (ADR-0287) `ID` | — | ADR-0287 |
| 57 | S3.11a | `R-2.1.5`, `R-2.8.1`, `R-2.8.2` `leave as implementation detail` | the monitor/IFC security rulings (ADR-0288) `ID` | — | ADR-0288 |
| 58 | S3.11b | `R-2.8.3`, `R-2.8.4`, `R-2.8.5`, `R-2.8.6`, `R-2.8.7` `leave as implementation detail` | the credential/egress/audit/trust/approval rulings (ADR-0289) `ID` | — | ADR-0289 |
| 59 | S3.12 | `R-2.1.6`, `R-2.12.1`, `R-2.12.2`, `R-2.9.6` `leave as implementation detail` | the bundle-completeness/reproduction/plugin-conformance rulings (ADR-0290) `ID` | — | ADR-0290 |
| 59a | S3.12b | `R-2.10.3`, `R-2.10.4`, `R-2.10.6`, `R-2.11.4`, `R-2.12.2`, `R-2.5.3`, `R-2.9.4` `leave as implementation detail` | the GATE-G2 gap-closure rulings — the hermetic `benchmarkSet` `benchset.stage3.v1` shape (ADR-0291; §9/§10 gate-authorized insertion) `ID` | — | ADR-0291 |
| 62 | S4.1 | `R-2.10.2` `leave as implementation detail` | the registry-service namespace/signer rulings (ADR-0292) `ID` | — | ADR-0292 |
| 63 | S4.2 | `R-2.10.3`, `R-2.9.3` `leave as implementation detail` | the experiment/bundle `inference_budget` rulings (ADR-0293) `ID` | — | ADR-0293 |
| 64 | S4.3 | `R-2.10.4`, `R-2.10.5` `leave as implementation detail` | the analysis-engine/results-store C1 rulings (ADR-0294) `ID` | — | ADR-0294 |
| 65 | S4.4 | `R-2.10.5`, `R-2.9.3` `leave as implementation detail` | the `ReproReport{independent}` declared-material rulings (ADR-0295) `ID` | — | ADR-0295 |
| 66 | S4.5a | `R-2.10.1`, `R-2.10.3`, `R-2.10.6`, `R-2.11.1` `leave as implementation detail` | `hh-hosting` + the JSON HostingPlane seam rulings (ADR-0296) `ID` | — | ADR-0296 |
| 67 | S4.5b | `R-2.1.3`, `R-2.5.4` `leave as implementation detail` | `hh-acp` protocol-edge + `input_required` rulings (ADR-0297) `ID` | — | ADR-0297 |
| 68 | S4.6 | `R-2.1.6`, `R-2.6.3` `leave as implementation detail` | `hh-subagent` + orchestrator rulings (ADR-0298) `ID` | — | ADR-0298 |
| 69 | S4.7 | `R-2.6.4` `leave as implementation detail` | the value-of-compute `compute_policy` rulings (ADR-0299) `ID` | — | ADR-0299 |
| 70 | S4.8 | `R-2.6.2`, `R-2.6.5` `leave as implementation detail` | the coordination/consistency rulings (ADR-0300) `ID` | — | ADR-0300 |
| 71 | S4.9 | `R-2.12.6` `leave as implementation detail` | the fleet reconciler under owned ADR-0205–0207/0209 D2 (no new ADR) `ID` | — | ADR-0205–0207, ADR-0209 D2 (owned; none new) |
| 72 | S4.10 | `R-2.11.2` `leave as implementation detail` | binding (c) `local_network` rulings (ADR-0301) `ID`; the `hh-web` surface rulings (ADR-0302) `ID`; the in-ecosystem generated client as the declared honest proxy for the unbound E3 ecosystem (DF-S4.10-1 P; HUMAN-H1) `ID` | — | ADR-0301, ADR-0302 |
| 73 | S4.11 | `R-2.11.3` `leave as implementation detail` | `hh-mcp-lab` surface rulings (ADR-0303) `ID` | — | ADR-0303 |
| 74 | S4.12 | `R-2.11.1`, `R-2.11.4` `leave as implementation detail` | the `hh-embed/1` SDK + CLI-surface rulings (ADR-0304) `ID` | — | ADR-0304 |
| 75 | S4.13 | `R-2.2.1`, `R-2.2.2`, `R-2.2.3`, `R-2.2.4`, `R-2.2.5` `leave as implementation detail` | the hosted-lineage/durability rulings incl. the OQ-388 maintenance-policy + latency measurement (ADR-0305 D1 → ratification ruling owed; ADR-0331 D4) `FB→plan` | `hh-ledger/tests/s4_13.rs` latency fixture (the CI-enforced interim bound) `ID` | ADR-0305 |
| 76 | S4.14a | `R-2.12.2`, `R-2.8.5`, `R-2.8.7` `leave as implementation detail` | the supply-chain trust/third-party-install rulings (ADR-0306) `ID` | — | ADR-0306 |
| 77 | S4.14b | `R-2.1.5`, `R-2.8.1`, `R-2.8.2`, `R-2.8.3`, `R-2.8.4`, `R-2.8.6` `leave as implementation detail` | the backend/attestation/rotation security enrichments (ADR-0307) `ID` | — | ADR-0307 |
| 78 | S4.15 | `R-2.12.1`, `R-2.9.1`, `R-2.9.2`, `R-2.9.4` `leave as implementation detail` | the C1 measurement-depth rulings (ADR-0308) `ID` | — | ADR-0308 |
| 79 | S4.16a | `R-2.3.2`, `R-2.3.3`, `R-2.3.4` `leave as implementation detail` | the C1 router/cache-projection rulings (ADR-0309) `ID` | — | ADR-0309 |
| 80 | S4.16b | `R-2.4.2`, `R-2.4.3`, `R-2.4.4`, `R-2.4.5` `leave as implementation detail` | the C1/C2 retrieval/memory-depth rulings (ADR-0310) `ID` | — | ADR-0310 |
| 81 | S4.16c | `R-2.7.1`, `R-2.7.2b`, `R-2.7.3` `leave as implementation detail` | the C1/C2 verification-depth rulings (ADR-0311) `ID` | — | ADR-0311 |
| 82 | S5.1 | `R-2.1.4`, `R-2.3.2`, `R-2.3.3`, `R-2.3.4` `leave as implementation detail` | the Profile-Compiler + router `bandit` rulings (ADR-0312) `ID` | — | ADR-0312 |
| 83 | S5.2 | `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.5`, `R-2.5.2` `leave as implementation detail` | the compaction-family + tool-surface rulings (ADR-0313) `ID` | — | ADR-0313 |
| 84 | S5.3 | `R-2.10.1`, `R-2.10.2`, `R-2.10.3`, `R-2.10.4`, `R-2.10.5`, `R-2.10.6` `leave as implementation detail` | the analysis-C2 + hosted-lifecycle rulings (ADR-0314) `ID` | — | ADR-0314 |
| 85 | S5.4 | `R-2.1.5`, `R-2.12.1`, `R-2.9.3`, `R-2.9.6`, `R-2.9.7`, `R-2.9.8` `leave as implementation detail` | the M1-ablation + live-debt rulings (ADR-0315) `ID` | — | ADR-0315 |
| 86 | S5.5 | `R-2.6.2`, `R-2.6.3`, `R-2.6.4`, `R-2.6.5`, `R-2.7.2b` `leave as implementation detail` | the C3-orchestration-depth rulings (ADR-0316) `ID` | — | ADR-0316 |
| 87 | S5.6 | `R-2.12.6` `leave as implementation detail` | the fleet-adapter rulings (ADR-0317) `ID`; the `memory` tracker fixture as the declared honest proxy for the unprovisioned real tracker (DF-S5.6-1 P; HUMAN-H2) `ID` | — | ADR-0317 |
| 88 | S5.7 | `R-2.11.2`, `R-2.11.3` `leave as implementation detail` | the C2 web-surface + MCP write/analysis rulings (ADR-0318) `ID` | — | ADR-0318 |
| 89 | S5.8 | `R-2.11.4`, `R-2.2.3`, `R-2.2.5` `leave as implementation detail` | the surface-ecosystem/compatibility rulings (ADR-0319) `ID` | `schema/compat-1.matrix.json` as a check-drift generated artifact `ID` | ADR-0319 |
| 90 | S6.1a | `R-2.1.6`, `R-2.10.3`, `R-2.11.2`, `R-2.12.2`, `R-2.12.6`, `R-2.3.2`, `R-2.4.5`, `R-2.6.1`, `R-2.9.4`, `R-2.9.5` `leave as implementation detail` | `hh-evolution` + the S0–S10 pipeline rulings (ADR-0320) `ID` | — | ADR-0320 |
| 91 | S6.1b | `R-2.9.6` `leave as implementation detail` | `hh-debt` assumption-debt-manager rulings (ADR-0321) `ID` | — | ADR-0321 |
| 92 | S6.2 | `R-2.10.3`, `R-2.4.1`, `R-2.4.2`, `R-2.4.4`, `R-2.6.4`, `R-2.6.5`, `R-2.9.5` `leave as implementation detail` | the automated-family one-class rulings (ADR-0322) `ID` | `hh-plugin-fixture --mode proposer` executable witness `ID` | ADR-0322 |
| 93 | S6.3a | `R-2.8.5`, `R-2.9.5` `leave as implementation detail` | the multi-family code-search + hosted-coordinate rulings (ADR-0323) `ID` | — | ADR-0323 |
| 94 | S6.3b | `R-2.10.4`, `R-2.12.1`, `R-2.7.3`, `R-2.9.7` `leave as implementation detail` | the causal-attribution + judge-integrity rulings (ADR-0324) `ID` | — | ADR-0324 |
| 95 | S6.4 | `R-2.12.6`, `R-2.9.5`, `R-2.9.6`, `R-2.9.8` `leave as implementation detail` | the co-evolution-interface rulings (ADR-0325) `ID` | — | ADR-0325 |
| 96 | CAP.1 | `R-2.1.1`, `R-2.1.2`, `R-2.1.3`, `R-2.1.4`, `R-2.1.5`, `R-2.1.6`, `R-2.10.1`, `R-2.10.2`, `R-2.10.3`, `R-2.10.4`, `R-2.10.5`, `R-2.10.6`, `R-2.11.1`, `R-2.11.2`, `R-2.11.3`, `R-2.11.4`, `R-2.12.1`, `R-2.12.2`, `R-2.12.3`, `R-2.12.4`, `R-2.12.5`, `R-2.12.6`, `R-2.2.1`, `R-2.2.2`, `R-2.2.3`, `R-2.2.4`, `R-2.2.5`, `R-2.3.1`, `R-2.3.2`, `R-2.3.3`, `R-2.3.4`, `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.4`, `R-2.4.5`, `R-2.5.1`, `R-2.5.2`, `R-2.5.3`, `R-2.5.4`, `R-2.5.5`, `R-2.6.1`, `R-2.6.2`, `R-2.6.3`, `R-2.6.4`, `R-2.6.5`, `R-2.7.1`, `R-2.7.2`, `R-2.7.2a`, `R-2.7.2b`, `R-2.7.3`, `R-2.8.1`, `R-2.8.2`, `R-2.8.3`, `R-2.8.4`, `R-2.8.5`, `R-2.8.6`, `R-2.8.7`, `R-2.9.1`, `R-2.9.2`, `R-2.9.3`, `R-2.9.4`, `R-2.9.5`, `R-2.9.6`, `R-2.9.7`, `R-2.9.8` `leave as implementation detail` | the 66-row `COVERAGE_MATRIX.csv` + `CAPSTONE_GAP_ANALYSIS.md` as the spec's §10 coverage evidence (ADR-0326) `ID` | the fresh-context verdict protocol (verdicts committed before run ledgers were read) `ID` | ADR-0326 |
| 97 | CAP.2 | `R-2.10.3`, `R-2.11.3`, `R-2.4.1`, `R-2.8.4`, `R-2.9.3` `leave as implementation detail` | the composed-E2E spine + `COMPOSED_E2E_REPORT.md` as honest integration evidence; xfail pins each carrying a DF id `ID` | `docs/build/COMPOSED_E2E_REPORT.md` `ID` | (none — verification ticket; no deviations owned — the two composed-found defects route to CAP.3's closure + ADR surface) |
| 98 | CAP.3 | `R-2.1.4`, `R-2.10.3`, `R-2.10.4`, `R-2.10.6`, `R-2.11.3`, `R-2.4.1`, `R-2.8.4`, `R-2.8.7` `leave as implementation detail` | `CAPSTONE_CLOSURE.md` + the 10-item ACCEPTED-deviations set (ADR-0327–0330) as the gate's signature input `ID` | the closure/gap-analysis documents `ID` | ADR-0327 (experiment-close audit partition) + ADR-0328 (hosted permission-lift shaping) + ADR-0329 (respond_approval supply-protocol builtin) + ADR-0330 (closure rulings + accepted-deviation set) |
| 99 | REC.1 | `R-2.1.1`, `R-2.1.4`, `R-2.1.5`, `R-2.10.3`, `R-2.10.5`, `R-2.11.1`, `R-2.11.2`, `R-2.11.3`, `R-2.11.4`, `R-2.12.1`, `R-2.12.4`, `R-2.12.6`, `R-2.2.1`, `R-2.2.3`, `R-2.2.4`, `R-2.2.5`, `R-2.3.3`, `R-2.4.1`, `R-2.4.2`, `R-2.4.3`, `R-2.4.4`, `R-2.5.2`, `R-2.5.3`, `R-2.6.1`, `R-2.7.1`, `R-2.7.2b`, `R-2.8.1`, `R-2.8.2`, `R-2.8.3`, `R-2.8.4`, `R-2.8.5`, `R-2.8.6`, `R-2.8.7`, `R-2.9.1`, `R-2.9.2`, `R-2.9.3`, `R-2.9.4`, `R-2.9.6` `leave as implementation detail` | `BACKLOG.csv`/`BACKLOG.md` + `OPERATIONAL_READINESS.md` as closeout program artifacts (reconcile-build mode=backlog) `ID` | `research/registers/risks.md` `## Round 1 review` appendix note `ID` | — |

## Roll-up

- Landed rows tagged: **98** (BUILD_INDEX seq 1–99; S0.3b/S3.12b/S4.16a-c/S6.1a-b/S6.3a-b counted separately).
- Tagged deliverable groups: **in-spec 389** (one per stamped requirement id) ·
  **spec-implied 114** · **ticket-added 17**.
- Dispositions: every `in-spec` and nearly every `spec-implied`/`ticket-added` item is
  `ID` (implementation detail). The `FB→plan` items are the four BL-30 ruling surfaces
  (DF-S2.5-1 custody, DF-S4.13-1 OQ-388 ratification, DF-S1.24-3 `debt.hypothesis`,
  DF-S1.26-2 parked-detach) — rulings recorded in **ADR-0331**; spec folds proposed in
  `SPEC_RECONCILIATION_PLAN.md` (all unticked — the operator ticks and re-runs with
  `apply_amendments=true`).
- No landed deliverable was found **contradicting** the spec: the six `MET-DIFFERENTLY`
  coverage rows (R-2.7.2, R-2.11.2, R-2.12.3, R-2.12.4, R-2.12.5, R-2.12.6) are
  spec-sanctioned decompositions/deferrals or signed GATE-ACCEPT deviations, dispositioned
  in the plan §ADR-appendix/deviation rows — not amendments.
