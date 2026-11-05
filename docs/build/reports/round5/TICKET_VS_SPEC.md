<!--
  docs/build/reports/round5/TICKET_VS_SPEC.md — reconcile-build mode=spec deliverable (BM-RECON-02),
  round-5 scope. Written by REC5.2 (2026-11-05). Append-only. The round-1–4 record stays at
  docs/build/TICKET_VS_SPEC.md (REC.2, 2026-10-06) — never rewritten or extended here.
-->
# TICKET vs SPEC — round 5: landed code, ADRs and supplemental coverage vs `docs/evidence-and-adoption-spec.md`

**Spec under reconciliation.** `docs/evidence-and-adoption-spec.md` — the ratified round-5
supplemental contract (frozen sha256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`,
167 adopted targets + 12 merged + 1 superseded alias). `spec/CANONICAL_SPEC.md` is **preserved
unchanged** (REQ-H-001/REQ-H-004): round-5 tags resolve against the supplement; the canonical
contract's own amendment candidates live in `SPEC_RECONCILIATION_PLAN.md` as proposals only.

**Tags** (same vocabulary as the round-1–4 record). `in-spec` — the supplement required the
deliverable (the ticket's stamped `REQ-*` ids are the citation). `spec-implied` — a reasonable
reading of the supplement, not stated verbatim: the §9.3 dossier/PADR mechanism and GATE-G9/G11–G18
admission machinery, per-ticket owner seams under an admitted packet, the honest
`unselected`/`withheld`/`failed`/`inconclusive` disposition records the spec's honesty rules
require, independent-verification batteries under AC-9-2. `ticket-added` — beyond the supplement:
build tooling, checkers, fixture batteries, evidence packets and build-memory artifacts.

**Dispositions.** `ID` — leave as an implementation detail (the item lives correctly in
code/records). `AX` — note in an appendix. `FB→plan` — fold-back candidate carried to
`SPEC_RECONCILIATION_PLAN.md`. Requirement ids are append-only; no renumbering.

**Scope note.** 63 landed BUILD_INDEX rows (seq 169–242 — the whole landed round-5 chain at
`169e3c25`). Gate/human rows (GATE-G8/G9/G10/G18/G11/G12/G13/G14/G15/G16/G17, HUMAN-H8…H11) land
no build row — their verbatim operator decisions are the admission authority the spec-implied
cells cite and are verified under the gate↔record check below. Rows are per landed chain row;
deliverable groups are tagged, not every file.

## Landed-record verification (reconcile AC-1: every landed module/packet/ADR/coverage-anchor vs the committed record)

All checks read committed state at `169e3c25` (records-in, offline, USD $0).

| # | reconciliation check | result |
|---|---|---|
| V-1 | Dependency-admission audit — `git diff 001490c1…169e3c25` over every `Cargo.toml` + `Cargo.lock` | **clean.** Only `crates/hh-env/Cargo.toml` gained a `[dev-dependencies]` section: 7 internal path crates (hh-gateway, hh-eval, hh-bench, hh-lab, hh-plugin, hh-bundle, hh-analysis), each annotated test-only/no-cycle; `Cargo.lock` +7 lines. Zero new external dependencies; the admitted set (`ring`@hharness-secrets, rustls graph@hharness-tls, test-only `tempfile`@hharness-varhost) is unchanged — no unadmitted dependency exists |
| V-2 | Reverse-edge audit — none of the 7 dev-dep targets may list `hh-env`/`hharness-env` in any dependency section | **clean.** 0 hits; every edge points upward from env's test graph only. Cargo dev-deps cannot produce a shipping cycle; no production reverse edge exists |
| V-3 | Schema/generator/generated audit — diff under `schema/`, `clients/`, `crates/hh-codegen`, `hh-embed-schema`, `hh-embed-client-generated`, `THIRD_PARTY_NOTICES.md` | **clean — zero files changed.** No contract-schema or generator admission was needed and none occurred; no generated artifact can have been hand-edited in round 5 |
| V-4 | `bash scripts/check-drift.sh` — regenerate all listed outputs and compare | **ok** — generated artifacts byte-identical to the schema source (CC7 holds; no hand edit) |
| V-5 | Tier/feature audit — feature changes in the round-5 diff | **clean.** The only manifest change is V-1's dev-deps; no feature/tier addition, removal or rename — no unadmitted tier exists |
| V-6 | Amendment-admission map — §9.3's seven register rows → PADR-R5-1…7 → GATE-G9 admission → landing ADRs | **complete.** All 7 rows prepared in `docs/build/planning/2026-10-30_amendment-dossier.md` (R5.3/R5.4/R5.5), admitted verbatim at GATE-G9 (readouts/GATE-G9.md, operator "yes" 2026-11-02T02:06:07Z), landed under 39 ADRs: PADR-R5-1→ADR-0385/0386/0388/0389/0390/0393, PADR-R5-2→0387/0391/0392/0396/0397/0398, PADR-R5-3→0399–0405, PADR-R5-4→0395 (+0418 repair), PADR-R5-5→0417, PADR-R5-6→0416, PADR-R5-7→0414/0415; shared method contract S.1–S.6→0394 (+ method ADRs). Every semantic landing traces to an admitted packet or a direct adopted requirement — no silent amendment |
| V-7 | Existing-contract fixes (§9.3's named fix list) | **landed as fixes, not amendments:** M1 `created`→`finished` origin (ADR-0389, R5.10), R0 open/finished mapping (ADR-0402, R5.24) — conformant with the clause "existing-contract fixes, not automatic amendments" |
| V-8 | ADR index/trigger generation through tools — `adr-index.sh --check docs/adr` vs committed `docs/adr/README.md`; `bash scripts/gen-revisit-triggers.sh --check` | **in sync.** Regenerated index == committed generated section; revisit triggers in sync (no hand edits, no missing triggers) |
| V-9 | Event-class one-home (CC10) — new round-5 event classes | **clean.** 6 new classes (`measurement.experiment.{launch_bound,execution_settled,plan_disposition,selection_revision}`, `verification.oracle.{invoked,verdict}`) — all registered only in `crates/hh-ledger/src/classes.rs`; other files reference, never re-register |
| V-10 | Landed-module existence — every `hh-*/1` contract marker the records name | **verified.** `hh-supplement-guard/1`, `hh-dispatch-driver/1`, `hh-recipe/1`, `hh-examples/1`, `evidence-package/1`, `population-receipt/1`, `sim-battery/1`, `hh-study-report/1`, `hh-parity-n0/1`, `hh-cap5-closure/1`, `finite_manifest_contrast`, `closure_walk`, `directional_kl`, `exposure_quote`, `central_protocol` all present at HEAD |
| V-11 | Evidence-packet inventory — 20 committed `docs/build/reports/2026-11-*.json` | **verified.** All 20 parse; dispositions honest: executed ×4 (ops-live-second, activation, pilot, cap5.2-composed), failed ×2 (s1-useful, ops-live-first), inconclusive ×2 (preflight + reprobe), withheld ×6 (parity-n0, confirm, outside, formative, publication, value), unselected ×2 (plugin, interchange), reported ×2 (study-report, cap5.3-closure), schema-verdict packets ×2 (`sim-battery/1`, `hh-capstone-gap-hunt/1`) — no positive claim over absent legs (matches CAP5.1 NC-1/NC-2 folds) |
| V-12 | Deviation↔record consistency — every BUILD_INDEX "deviation"/SPLIT/AMEND cell has a dated manifest note | **verified.** 47 `NNN note` manifest rows cover all recorded size deviations and split rulings; DF-R5.17-1 (landed defect) is DONE via the owning admission ADR-0418 D1 — deviations are on the record, none silent |
| V-13 | Coverage-anchor resolution — register hooks + matrix evidence pointers | **verified.** `req-e-210`/`req-h-004`/`s9`/`ac-9-1`/`ac-9-2` anchors exist once each in the supplement; `ac-rec5-2-3` exists in the contract; the supplement guard PASSes anchor/ownership/routing checks (180 candidates, 0 viol). COVERAGE_MATRIX evidence pointers for the 107 MET rows resolve to committed files except exactly two — `docs/build/pr/REC5.2.md` + `docs/build/runs/REC5.2.md` — which this ticket lands |
| V-14 | Gate↔record verbatim — operator decisions vs landed scope | **verified.** GATE-G9 (admit 7 PADRs), GATE-G18 (bounded preflight), GATE-G11 (S1+O-matrix), GATE-G12 (activation+pilot), GATE-G13 (N=0 parity), GATE-G14 (quote-only → withheld confirmation), GATE-G15/G16 (out-of-scope dispositions), GATE-G17 (fresh acceptance) — readouts + LEDGER GATE DECISIONS agree with the landed packets (CAP5.1 NC-3, re-verified here) |
| V-15 | Withheld never promoted — over-claim scan | **clean.** DF-H9-1/DF-R5.1-1/DF-CAP5.1-1 all OPEN at HEAD; the six withheld/unselected packets carry `df_h9_1:open`/`claim_awarded:false`; no live/participant/adoption claim anywhere in the landed record |

## Per-row tag table

| seq | ticket | PR | in-spec deliverables (stamped REQ ids) | spec-implied deliverables | ticket-added deliverables | ADRs |
|---|---|---|---|---|---|---|
| 169 | R5.1 | #209 | `REQ-C-104`, `REQ-C-107`, `REQ-C-108`, `REQ-H-001`/`002`/`003`/`005`/`006`/`007`/`011` (all contributing; primary = the §9.4 guard charter itself) `leave as implementation detail` | the `hh-supplement-guard/1` versioned read-only guard contract + input-digest evidence model (§9.4 names the retained guard + rejection fixtures; the schema/digest shape is the implied mechanism, ADR-0382) `ID` | `docs/build/tools/{supplement_guard.py,check-supplement-guard.sh}` + `docs/build/fixtures/supplement-guard/` 13-case rejection battery `ID` | ADR-0382 |
| 170 | R5.2 | #210 | `REQ-H-001`, `REQ-H-008` (contributing) `leave as implementation detail` | the `hh-dispatch-driver/1` + `hh-dispatch-envelope/1` bounded launch-path wrapper (§9.4 "a tested bounded wrapper may call external driver `--max-iters 1`"; ADR-0383) `ID`; the guard chain-tip→stacked-ancestry repair ruling (ADR-0383) `ID` | `docs/build/tools/{dispatch_driver.py,dispatch-driver.sh}` + `docs/build/fixtures/dispatch-driver/` 22-case battery `ID` | ADR-0383 |
| 171 | R5.3 | #211 | `REQ-E-205`, `REQ-E-210`, `REQ-H-004` (contributing) `leave as implementation detail` | the common seven-row `2026-10-30_amendment-dossier.md` register shape + `∗`-candidate convention (§9.3 names the register; its file/checker home is implied, ADR-0384) `ID`; Packet A/B (PADR-R5-1∗/2∗) texts `ID` | `docs/build/tools/check-amendment-dossier.sh` `ID` | ADR-0384 |
| 172 | R5.4 | #212 | `REQ-E-210`, `REQ-F-001`, `REQ-H-004` (contributing) `leave as implementation detail` | Packet C (PADR-R5-3∗) dossier row — evidence-profile/closure proposal text `ID` | dossier checker extension (Packet C field set) `ID` | — |
| 173 | R5.5 | #213 | `REQ-D-108`, `REQ-E-210`, `REQ-H-004` (contributing) `leave as implementation detail` | Shared method contract S.1–S.6 + Packets D–G (PADR-R5-4∗/5∗/6∗/7∗) dossier rows `ID` | dossier checker extension (shared markers + D–G fields) `ID` | — |
| 175 | R5.6 | #214 | `REQ-D-006`, `REQ-E-201`, `REQ-E-207` (contributing) `leave as implementation detail` | PADR-R5-1 `launch_bound` owner-seam mechanism: `lbi-/subj-<idp>` deterministic ids, `LaunchPhase` fault seam, `abort_launch` owned release (admitted packet scope; mechanism implied, ADR-0385) `ID` | 8-test engine battery; `measurement.experiment.launch_bound` class `ID` | ADR-0385 |
| 176 | R5.7 | #215 | `REQ-C-103`, `REQ-E-001`, `REQ-E-201` (contributing) `leave as implementation detail` | in-process `adopt_subject*` EmbedService seam (dossier non-admission ledger: no RPC/new class; ADR-0386) `ID` | `crates/hh-embed/src/adopt.rs` + r5_7 battery; `lifecycle.session.attached{mode:adopt}` extension member `ID` | ADR-0386 |
| 177 | R5.8 | #218 | `REQ-B-105`, `REQ-C-006`, `REQ-D-203`–`207`, `REQ-E-002`, `REQ-E-012`, `REQ-E-110`, `REQ-E-201`, `REQ-E-205` (contributing) `leave as implementation detail` | PADR-R5-2 `DurableEffectRef*` dispatch seam: `dispatch_adopted` pre-mutation refusals, `DispatchSeam`/`EffectGate::dispatch_durable`, suppressed emitted+terminal appends (ADR-0387) `ID` | env 9 + ctl 2 + embed 3 batteries `ID` | ADR-0387 |
| 178 | R5.9 | #219 | `REQ-E-102`, `REQ-E-103`, `REQ-E-204` (contributing) `leave as implementation detail` | shared pure `hharness_eval::selection` contract — `ValidGradeSet` pins, `select_grade*` first-valid, `RevisionLog` append-only, `Pending` admission state (ADR-0388) `ID` | r5_9 battery `ID` | ADR-0388 |
| 179 | R5.10 | #220 | `REQ-E-102`/`103`, `REQ-E-201`, `REQ-E-203`, `REQ-E-204` (contributing) `leave as implementation detail` | seal-then-grade ordering at the subject-run finish + M1 `created`→`finished` origin repair — §9.3 existing-contract fix (ADR-0389) `ID` | r5_10 battery `ID` | ADR-0389 |
| 180 | R5.11 | #221 | `REQ-E-007`, `REQ-E-102`/`103`, `REQ-E-204` (contributing) `leave as implementation detail` | `OracleRunner::attempt` instrument-oracle persistence ordering — `verification.oracle.{invoked,verdict}` classes (ADR-0390) `ID` | `crates/hh-bench/src/oracle.rs` + r5_11 battery `ID` | ADR-0390 |
| 181 | R5.12 | #222 | `REQ-D-006`, `REQ-E-005`, `REQ-E-207` (contributing) `leave as implementation detail` | whole-vector `Account::reserve` admission — every bounded key on every ancestor vs projected level (ADR-0391) `ID` | r5_12 battery `ID` | ADR-0391 |
| 182 | R5.13 | #223 | `REQ-D-006`, `REQ-E-005`, `REQ-E-102`/`103`, `REQ-E-204`, `REQ-E-207` (contributing) `leave as implementation detail` | `Account::physical_use` — physical incurred/billed/unknown beside (never merged into) logical totals + dedup (ADR-0392) `ID` | r5_13 battery + results projection `ID` | ADR-0392 |
| 183 | R5.14 | #224 | `REQ-E-008`, `REQ-E-102`/`103`, `REQ-E-201`, `REQ-E-202`, `REQ-E-204` (contributing) `leave as implementation detail` | `grade_pending` admission-state owner model (`disposition::plan_disposition*`, `measurement.experiment.plan_disposition`) — never a seventh outcome (ADR-0393) `ID` | r5_14a battery; leg-(a) of recorded SPLIT `07b6be09` `ID` | ADR-0393 |
| 183a | R5.14b | #225 | same stamped set as R5.14 (contributing) `leave as implementation detail` | single-owner consumption wiring — every declared scored consumer through `plan_disposition_at` unchanged `ID` | r5_14b battery (eval/embed/results) `ID` | — |
| 184 | R5.15 | #227 | `REQ-A-004`, `REQ-A-102`, `REQ-D-001`/`002`/`003`, `REQ-D-104` (contributing) `leave as implementation detail` | `hharness_eval::population` — dossier S.1 shared planned-slot population/method-metadata contract (ADR-0394) `ID` | r5_15 battery; leg-(a) of SPLIT `4d13371f` `ID` | ADR-0394 |
| 184a | R5.15b | #228 | same stamped set as R5.15 (contributing) `leave as implementation detail` | `hharness_analysis::population::planned_population` adapter — consumer wiring, owner contract unchanged `ID` | r5_15 analysis battery `ID` | — |
| 185 | R5.16 | #229 | `REQ-D-008`, `REQ-D-107`, `REQ-D-109`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | PADR-R5-4 `finite_manifest_contrast` — per-cell Beta posterior contrast, seeded draws, integer ppm (ADR-0395) `ID` | r5_16 battery; leg-(a) of SPLIT `aa196851` `ID` | ADR-0395 |
| 185a | R5.16b | #230 | same stamped set as R5.16 (contributing) `leave as implementation detail` | `finite_manifest_contrast` routing through hh-analysis report machinery — owner contract unchanged `ID` | r5_16 analysis/embed batteries `ID` | — |
| 186 | R5.17 | #231 | `REQ-D-109`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | independent verification battery over the landed early-efficacy route (AC-9-2 independent derivation obligation) `ID`; honest defect find → DF-R5.17-1 (routed, repaired ADR-0418 D1) `ID` | `crates/hh-analysis/tests/r5_17.rs` `ID` | — |
| 188 | R5.18 | #232 | `REQ-E-003`, `REQ-E-104`, `REQ-E-206` (contributing) `leave as implementation detail` | PADR-R5-2 `retained_select` context persist + replay selection (`crates/hh-context/src/retained.rs`; ADR-0396; amended SPLIT review `b9ec0189`) `ID` | r5_18 battery `ID` | ADR-0396 |
| 189 | R5.19 | #233 | `REQ-D-006`, `REQ-E-005`, `REQ-E-006`, `REQ-E-105`, `REQ-E-106`, `REQ-E-207` (contributing) `leave as implementation detail` | PADR-R5-2 prepared gateway reservations + persistent deadline (`crates/hh-gateway/src/prepared.rs`; ADR-0397; amended SPLIT review `4ec29023`) `ID` | r5_19 battery `ID` | ADR-0397 |
| 190 | R5.20 | #234 | `REQ-A-007`, `REQ-D-005`, `REQ-E-003`, `REQ-E-104`, `REQ-E-206` (contributing) `leave as implementation detail` | retained-view→wire — `GatewayModel` consumes durable `retained_select` to exact provider bytes (ADR-0398) `ID` | r5_20 battery `ID` | ADR-0398 |
| 191 | R5.21 | #235 | `REQ-A-101`, `REQ-A-103`, `REQ-F-001`, `REQ-F-002` (contributing) `leave as implementation detail` | `evidence-package/1` typed member/role profile inside `hh-bundle/1` — derived index, never a second store (ADR-0399) `ID` | `crates/hh-bundle/src/profile.rs` + r5_21 battery `ID` | ADR-0399 |
| 192 | R5.22 | #236 | `REQ-A-103`, `REQ-E-208`, `REQ-F-005` (contributing) `leave as implementation detail` | `closure_walk`/`closure_report` owner-typed recipient closure (ADR-0400) `ID` | `crates/hh-bundle/src/closure.rs` + r5_22 battery `ID` | ADR-0400 |
| 193 | R5.23 | #237 | `REQ-A-101`, `REQ-C-002`, `REQ-E-208`, `REQ-F-003`, `REQ-F-004`, `REQ-F-008` (contributing) `leave as implementation detail` | `population-receipt/1` all-planned closure reconcile — six-hop set-equality chain (ADR-0401) `ID` | `crates/hh-analysis/src/receipt.rs` + r5_23 battery `ID` | ADR-0401 |
| 194 | R5.24 | #238 | `REQ-A-103`, `REQ-A-204`, `REQ-C-003`, `REQ-C-007`, `REQ-D-208`, `REQ-E-208`, `REQ-F-007`, `REQ-F-201`/`202`, `REQ-G-101`–`105`, `REQ-G-107`, `REQ-G-203` (contributing) `leave as implementation detail` | R0 open/finished lifecycle repair — §9.3 existing-contract fix + clean-recipient verifyfixed (ADR-0402) `ID` | r5_24 battery `ID` | ADR-0402 |
| 195 | R5.25 | #239 | `REQ-A-103`, `REQ-C-003`, `REQ-F-005`, `REQ-F-007`, `REQ-F-203` (contributing) `leave as implementation detail` | §6.3 `recompute_analysis` recipient procedure on the native owner (ADR-0403) `ID` | `crates/hh-analysis/src/recompute.rs` + r5_25 battery `ID` | ADR-0403 |
| 196 | R5.26 | #240 | `REQ-A-005`, `REQ-A-103`, `REQ-A-105`, `REQ-B-104`, `REQ-C-003`, `REQ-D-010`, `REQ-D-202`, `REQ-F-005`, `REQ-F-007`, `REQ-F-204`, `REQ-F-205` (contributing) `leave as implementation detail` | §6.3 `admit_replay`/`admit_fresh` admission cards + independence compute (ADR-0404) `ID` | `crates/hh-bundle/src/cards.rs` + r5_26 battery `ID` | ADR-0404 |
| 197 | R5.27 | #241 | `REQ-F-006`, `REQ-F-201`/`202`, `REQ-F-207`, `REQ-F-208`, `REQ-G-104`, `REQ-G-203` (contributing) `leave as implementation detail` | §6.4 `admit_handoff`/`sanitize_replace` rights/loss/quota validation (ADR-0405) `ID` | `crates/hh-bundle/src/rights.rs` + r5_27 battery `ID` | ADR-0405 |
| 198 | R5.28 | #242 | `REQ-A-002`, `REQ-A-106`, `REQ-A-201`/`202`, `REQ-B-002`, `REQ-E-009`, `REQ-F-201`/`202`, `REQ-G-001`, `REQ-G-003`, `REQ-G-005`, `REQ-G-008`, `REQ-G-009`, `REQ-G-104`, `REQ-G-203` (contributing) `leave as implementation detail` | `hh-recipe/1` — the one versioned in-process recipe + exact CLI forwarding (ADR-0406) `ID` | `crates/hharness/src/recipe.rs` + r5_28 battery `ID` | ADR-0406 |
| 199 | R5.29 | #243 | `REQ-A-008`, `REQ-F-101`–`105` (contributing) `leave as implementation detail` | `hh-examples/1` checked example runner over committed `example-card/1` records (ADR-0407) `ID` | `crates/hharness/src/examples.rs` + `scripts/examples/` cards + r5_29 battery `ID` | ADR-0407 |
| 200 | R5.30 | #244 | `REQ-F-106`–`108` (contributing) `leave as implementation detail` | §7.3 both-modes static check + gated full-offline leg under ADR-0362's classifier + working-tree/committed-HEAD separation (ADR-0408) `ID` | `scripts/{check-examples.sh,run-examples.sh}` + `scripts/ci/test-check-examples.sh` `ID` | ADR-0408 |
| 201 | R5.31 | #245 | `REQ-A-205`, `REQ-B-004`, `REQ-C-007`, `REQ-G-008`, `REQ-G-101`–`103`, `REQ-G-105`, `REQ-G-107` (contributing) `leave as implementation detail` | §8.1 no-run/existing-run contribution + failure-report lanes in CONTRIBUTING.md (ADR-0409) `ID` | `scripts/check-contributing.sh` + `scripts/ci/test-check-contributing.sh` `ID` | ADR-0409 |
| 202 | R5.32 | #246 | `REQ-B-105`, `REQ-C-006`, `REQ-D-203`–`207`, `REQ-E-012`, `REQ-E-110`, `REQ-E-205` (contributing) `leave as implementation detail` | §5.2/D3-F3 confined observer apparatus + Kp4 — read-only `ops` instrument (never appends/dispatches; ADR-0410) `ID` | `crates/hh-env/src/ops.rs` + r5_32 battery `ID` | ADR-0410 |
| 203 | R5.33 | #247 | same stamped set as R5.32 (contributing) `leave as implementation detail` | §5.2/D3-O4/O5 real permission wait/answer/resume — `hh-monitor/src/approval.rs` semantic tightening (ADR-0411) `ID` | r5_33 battery `ID` | ADR-0411 |
| 204 | R5.34 | #248 | same stamped set as R5.32 (contributing) `leave as implementation detail` | §5.2/D3-F4 Account refusal/exhaustion cells + §5.3/D3-F5 fixed 8×3 report machinery (ADR-0412) `ID` | r5_34 battery + hh-env dev-dep on hh-gateway (test-only, annotated no-cycle) `ID` | ADR-0412 |
| 205 | R5.35 | #249 | `REQ-A-001`, `REQ-A-201`, `REQ-B-001`/`002`, `REQ-B-107`, `REQ-C-001`, `REQ-C-008`, `REQ-C-101`/`102`, `REQ-G-001`, `REQ-G-003`, `REQ-G-005`, `REQ-G-008`, `REQ-G-009`, `REQ-G-201`/`202`, `REQ-G-207`, `REQ-G-209` (contributing) `leave as implementation detail` | §7.1 reader spine + `docs/evidence.md` claim index — 11 evidence cards, closed 4-state vocabulary (ADR-0413) `ID` | `docs/{evidence,getting-started,status}.md` + `scripts/check-position.sh` `ID` | ADR-0413 |
| 206 | R5.36 | #250 | `REQ-D-005`, `REQ-D-106`, `REQ-D-109`, `REQ-D-310`, `REQ-E-004`, `REQ-E-107`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | PADR-R5-7 M-C mechanism registry + windowed denominators (`hh-eval/src/mechanism.rs`, `facts.rs` join coordinate; ADR-0414) `ID` | r5_36 battery `ID` | ADR-0414 |
| 207 | R5.37 | #251 | `REQ-D-008`, `REQ-D-109`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | M-D task-mixture sensitivity + sharp-null families + effect tails + routed kind (ADR-0415) `ID` | `hh-eval/src/task_mixture.rs` + r5_37 battery `ID` | ADR-0415 |
| 208 | R5.38 | #252 | `REQ-D-008`, `REQ-D-109`, `REQ-D-304`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | PADR-R5-6 M-E directional-KL `K_η` bounds + TOST η=.025/.0125 + one IUT exception + bounded/relative-mean tests (ADR-0416) `ID` | `hh-eval/src/directional_kl.rs` + r5_38 battery `ID` | ADR-0416 |
| 209 | R5.39 | #253 | `REQ-D-004`, `REQ-D-109`, `REQ-D-301`–`303`, `REQ-D-308`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | PADR-R5-5 M-F `parity_admission` validator over supplied authentic evidence — one admissible `authentic_first_attempt` shape (ADR-0417) `ID` | `hh-eval/src/parity_admission.rs` + r5_39 battery `ID` | ADR-0417 |
| 210 | R5.40 | #254 | `REQ-D-008`, `REQ-D-109`, `REQ-D-310`, `REQ-E-209`, `REQ-H-010` (contributing) `leave as implementation detail` | M-G composed numerical validation — frozen `sim-battery/1` protocol + DF-R5.17-1 prior repair at the owning admission (ADR-0418 D1) `ID` | `hh-eval/src/sim_battery.rs` + `examples/r5_40_battery.rs` + packet `2026-11-03-r5.40-sim-battery.json` `ID` | ADR-0418 |
| 211 | R5.41 | #255 | `REQ-B-101`, `REQ-B-103`, `REQ-B-106`, `REQ-D-007`, `REQ-D-101`–`103`, `REQ-E-011` (contributing) `leave as implementation detail` | M-H `materialize_protocol` — deterministic records-in central-protocol materializer validating the supplied manifest/split (ADR-0419) `ID` | `hh-eval/src/central_protocol.rs` + r5_41 battery `ID` | ADR-0419 |
| 212 | R5.42 | #256 | `REQ-C-004`, `REQ-C-105`, `REQ-C-106`, `REQ-D-105`, `REQ-D-110`, `REQ-D-309` (contributing) `leave as implementation detail` | M-I `build_quote` — total records-in exposure-quote builder over supplied binding inputs (ADR-0420) `ID` | `hh-eval/src/exposure_quote.rs` + r5_42 battery `ID` | ADR-0420 |
| 215 | R5.42a | #257 | `REQ-C-004`, `REQ-C-105`, `REQ-C-106`, `REQ-D-105`, `REQ-D-110`, `REQ-D-309` (contributing) `leave as implementation detail` | GATE-G18-admitted `hh-backend-preflight/1` bounded feasibility leg over HUMAN-H8's supplied worksheet (inconclusive disposition honestly carried) `ID` | `crates/hh-env/src/preflight.rs` + r5_42a battery + two probe packets `ID` | — |
| 217 | R5.43 | #258 | `REQ-A-003`, `REQ-A-104`, `REQ-D-201` (contributing) `leave as implementation detail` | GATE-G11-admitted `hh-s1-useful/1` two-subject S1 execution leg — failed disposition honestly emitted + carried (never retried) `ID` | `crates/hh-env/src/useful.rs` + r5_43 battery + failed packet `ID` | — |
| 218 | R5.44 | #259 | `REQ-B-105`, `REQ-C-006`, `REQ-D-203`, `REQ-E-012`, `REQ-E-110`, `REQ-E-205` (contributing) `leave as implementation detail` | GATE-G11-admitted `hh-ops-live/1` O1–O8×3 confined operational-proof matrix — first failed (machinery defect), second executed `ID` | `crates/hh-env/src/opslive.rs` + helper repair + two packets `ID` | — |
| 219 | R5.45 | #260 | `REQ-A-002`, `REQ-A-005`, `REQ-A-008`, `REQ-A-105`, `REQ-A-201`–`203`, `REQ-B-002`/`003`, `REQ-B-104`, `REQ-B-107`, `REQ-C-003`, `REQ-D-208`, `REQ-F-101`–`105`, `REQ-G-001`–`009`, `REQ-G-201`/`202`, `REQ-G-207`, `REQ-G-209` (contributing) `leave as implementation detail` | §7.1 actual annotated trace + four example-family integration + bounded edit `ID` | `docs/{experiments,inspect-a-run,getting-started,evidence}.md` + `scripts/check-trace.sh` + r5_45 battery `ID` | — |
| 221 | R5.46 | #261 | `REQ-C-105`, `REQ-D-001`–`003`, `REQ-D-005`, `REQ-D-008`, `REQ-D-101`–`106`, `REQ-D-109`, `REQ-D-110`, `REQ-E-004`, `REQ-E-107`, `REQ-E-209` (contributing) `leave as implementation detail` | GATE-G12-admitted `hh-activation/1` + `hh-pilot/1` bounded executed evidence legs `ID` | `crates/hh-env/src/{activation,pilot}.rs` + two executed packets `ID` | — |
| 223 | R5.47 | #262 | `REQ-D-004`, `REQ-D-108`, `REQ-D-301`–`310` (contributing) `leave as implementation detail` | GATE-G13 `hh-parity-n0/1` declared-N=0 parity disposition record `ID` | `crates/hh-env/src/parity.rs` + withheld packet `ID` | — |
| 225 | R5.48 | #263 | `REQ-B-101`, `REQ-B-103`, `REQ-B-106`, `REQ-D-101`–`105`, `REQ-D-110`, `REQ-E-011` (contributing) `leave as implementation detail` | GATE-G14 `hh-confirm-withheld/1` withheld-confirmation disposition record `ID` | `crates/hh-env/src/confirm.rs` + withheld packet `ID` | — |
| 226 | R5.49 | #264 | `REQ-C-004`/`005`, `REQ-C-105`, `REQ-C-106`, `REQ-D-001`–`003`, `REQ-D-007`, `REQ-D-009`, `REQ-D-101`–`105`, `REQ-D-110` (contributing) `leave as implementation detail` | `hh-study-report/1` recompute — byte/consistency-equal refolds of the nine committed packets, adverse results verbatim (never re-picked) `ID` | `crates/hh-env/src/study.rs` + reported packet `ID` | — |
| 229 | R5.50 | #265 | `REQ-A-006`, `REQ-D-209`, `REQ-G-106` (contributing) `leave as implementation detail` | GATE-G15 `hh-plugin-unselected/1` `unselected{stage_voided}` honest record (voided ≠ discharged; DF-H9-1 stays OPEN) `ID` | `crates/hh-env/src/plugin.rs` + unselected packet + dev-dep on hh-plugin (test-only) `ID` | — |
| 230 | R5.51 | #266 | `REQ-B-102`, `REQ-C-007`, `REQ-G-008` (contributing) `leave as implementation detail` | GATE-G15 `hh-interchange-unselected/1` record — direction/unknown refusals through real `ForeignFormat` machinery `ID` | `crates/hh-env/src/interchange.rs` + unselected packet `ID` | — |
| 231 | R5.52 | #267 | `REQ-F-201`, `REQ-G-104`, `REQ-G-203` (contributing) `leave as implementation detail` | GATE-G15 `hh-outside-withheld/1` `withheld{stage_voided}`/reviewer-absent record — procedure vocabulary asserted verbatim `ID` | `crates/hh-env/src/outside.rs` + withheld packet `ID` | — |
| 232 | R5.53 | #268 | `REQ-A-201`, `REQ-B-002`, `REQ-B-005`, `REQ-B-107`, `REQ-C-007`, `REQ-G-001`, `REQ-G-101`, `REQ-G-204` (contributing) `leave as implementation detail` | GATE-G15 `hh-formative-withheld/1` cohort-absent record — rubric/denominator vocabulary verbatim `ID` | `crates/hharness/src/formative.rs` + withheld packet `ID` | — |
| 235 | R5.54 | #269 | `REQ-G-201`/`202`, `REQ-G-207`, `REQ-G-209` (contributing) `leave as implementation detail` | GATE-G16 `hh-publication-withheld/1` `no_publication` record — adverse/unmet verbatim `ID` | `crates/hh-env/src/publication.rs` + withheld packet `ID` | — |
| 237 | R5.55 | #270 | `REQ-G-206` (contributing) `leave as implementation detail` | GATE-G16 `hh-value-withheld/1` `no_observation` record — no contacts/raw observations ever existed `ID` | `crates/hh-env/src/value.rs` + withheld packet `ID` | — |
| 238 | CAP5.1 | #271 | `REQ-A-003` (contributing; capstone slice) `leave as implementation detail` | §9.4 full-tail CAP.1 independent whole-scope gap hunt — `hh-capstone-gap-hunt/1` 12 records-in checks `ID` | `docs/build/tools/cap5_gap_hunt.py` + `reports/round5/{COVERAGE_MATRIX.csv,CAPSTONE_GAP_ANALYSIS.md}` + hunt packet `ID` | — |
| 239 | CAP5.2 | #272 | (contributing slice — CAP.2 role; composed offline procedures) `leave as implementation detail` | §9.4 full-tail CAP.2 composed execution/methods/recipient verification — `hh-cap5-composed/1` over the four committed example cards `ID` | `crates/hharness/src/composed.rs` + `tests/cap5_2.rs` + fixtures + `reports/round5/COMPOSED_E2E_REPORT.md` `ID` | — |
| 240 | CAP5.3 | #273 | `REQ-C-005`, `REQ-H-009` (contributing) `leave as implementation detail` | §9.4 full-tail CAP.3 closure — `hh-cap5-closure/1` 10 checks: five-layer fold, owed-home uniqueness, signature list, GATE-G17 prep `ID` | `docs/build/tools/cap5_closure.py` + `reports/round5/CAPSTONE_CLOSURE.md` + closure packet `ID` | — |
| 242 | REC5.1 | #274 | (contributing slice — REC.1 role; no primary ownership) `leave as implementation detail` | §9.4 full-tail REC.1 backlog + readiness reconcile (`reconcile-build mode=backlog`) `ID` | BL-50…53 open rows; `docs/build/{BACKLOG.md,OPERATIONAL_READINESS.md}` §Round-5; `reports/round5/{BACKLOG_READINESS,READINESS_REPORT}.md` `ID` | — |

## Counts

- **63 landed rows tagged** (every round-5 BUILD_INDEX row seq 169–242): in-spec deliverable
  groups cover the stamped `REQ-*` set (all 167 adopted targets trace to exactly one primary
  owner per the coverage register; every row's stamped ids resolve in the supplement);
  spec-implied groups are per-row (admission machinery, owner seams, honest disposition
  records); ticket-added groups are build tooling/checkers/batteries/reports — all `ID`.
- **No landed deliverable contradicts the supplement.** The only frozen-surface mismatch is the
  pre-existing canonical Appendix-B ADR range (A-3 applied 2026-10-06 indexes ADR-0217–0330;
  the build set now runs to ADR-0420) — dispositioned as plan row R5A-1, never rewritten.
- **New requirement ids proposed: none.** Every `ticket-added` group is build/evidence
  infrastructure, outside the product spec's scope; none earns a requirement id.

## REQ-E-210 resolution (reconcile AC-3) — exact predicate → numbered checks → evidence

Literal predicate (supplement `req-e-210`, disposition `adopted`, required layer **D/E**,
hooks §9 + AC-9-1 + AC-9-2): *"preserves CC1/CC7/CC10/tier/dependency/amendment constraints"* —
the eight-package owner boundaries of §9.1 and the schema/tier/ADR/dependency discipline the
build must not violate. Resolved checks:

| check | predicate cell | verdict | evidence |
|---|---|---|---|
| E210-1 | dependency discipline — no new/expanded external admission, no reverse edge | **met (D/E engineered)** | V-1, V-2 — diff-audited, zero external additions; 7 test-only internal dev-deps annotated no-cycle |
| E210-2 | schema/generator discipline (CC7) — generated never hand-edited; contract changes only through schema source + regen | **met (D/E engineered)** | V-3, V-4 — zero generated-surface diff; check-drift byte-identical |
| E210-3 | tier discipline — no new/renamed/dropped feature tier | **met (D/E engineered)** | V-5 — zero feature diff |
| E210-4 | amendment discipline (REQ-H-004 path) — every changed contract semantics admitted via dossier→gate→ADR, never drafting-admitted | **met (D/E engineered)** | V-6 — 7/7 §9.3 rows → GATE-G9 → 39 landing ADRs; V-7 existing-contract fixes distinguished; V-14 gate↔record verbatim |
| E210-5 | one home per kind/class (CC10/CC1) — no second codec/store/class table; durable-before-visible | **met (D/E engineered)** | V-9 — 6 new classes all in `classes.rs`; new modules land inside owning crates (experiment/embed/env/gateway/context/bundle/analysis/eval/bench/hharness facade) — no new crate, no duplicate store |
| E210-6 | eight-package boundaries (§9.1 P1–P8) — landed owners stay inside their package | **met (D/E engineered)** | landed crates map P1={experiment,embed,results,budget}, P2={env,control,bench}, P3={bundle}, P4={hharness facade recipe/examples}, P5={env ops/opslive/account cells}, P6={context,gateway,embed}, P7={budget,account,gateway}, P8={eval,analysis,bench,lab metadata} — all pre-existing owner crates; no package boundary created or crossed |
| E210-7 | supplemental layer links — coverage-register anchors/owner hooks resolve; achieved D/E recorded separately from R/P or actual-execution E | **met (D/E engineered)** | V-13 — anchors resolve; matrix's two open pointers are this ticket's own artifacts (landed by this PR). REQ-E-210 requires D/E only — **no R/P leg exists on this target**; achieved D/E = engineered (fixture/static checks), never actual-execution/live E |

**Contributor evidence (evidence dependency, not forward dispatch).** REQ-E-210's contributor
cells name R5.3/R5.4/R5.5 — landed: the seven-row dossier register + `check-amendment-dossier.sh`
(runs/R5.3.md, runs/R5.4.md, runs/R5.5.md; ADR-0384). REQ-H-004's contributing cell names
REC5.2 — this reconcile is its evidence leg. Per the register's completion rule, the matrix's
`MET` for REQ-E-210 stands on this ticket's records plus the landed contributors; this close
**does not** assert the full-target predicate satisfied — GATE-G17's verbatim disposition and
the CAP5.3 recompute own that read; the 3 OPEN deferrals (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1)
carry their owned predicates forward unchanged.

## Invariant dispositions (gap-table cell per I-01…I-08)

| invariant | disposition |
|---|---|
| I-01 source/identity | **applicable — held.** frozen canonical spec + supplement bytes unchanged; amendments only proposed (unticked); no generated/schema/dependency/reverse-edge/second-codec change |
| I-02 owners/durability | **applicable — held.** reconcile is records-in; no state mutation, no new writer/projection |
| I-03 subject/grades | **applicable — verified, not changed.** V-6/V-7 confirm the landed launch/grade/settlement/pending semantics trace to admitted packets |
| I-04 treatment/caps | **applicable — verified.** retained-view→wire + reserve/physical-use landings traced to PADR-R5-2 (V-6) |
| I-05 methods/missingness | **applicable — verified.** method landings traced to admitted packets + shared contract; failed/inconclusive dispositions preserved verbatim (V-11) |
| I-06 receipts/readers | **applicable — verified.** Packet C landings + R0 repair traced; recipient procedures stay owner-typed (V-6, V-10) |
| I-07 required domains | **applicable — held.** D/E recorded separately from owed R/P / actual-execution E (REQ-E-210 table; 3 OPEN deferrals carried) |
| I-08 scope/gates | **applicable — held.** $0 spend; no outreach/publication/participant act; withheld predicates stay withheld (V-15) |

## Operating-clause dispositions (one row per O-01…O-08)

| clause | disposition |
|---|---|
| O-01 orientation/clock | deferrals-first done; bounded row/spec reads; `date -u` stamps only |
| O-02 state writer/stacking | single writer this run; branch forked from actual rec5.1 tip `169e3c25`; shell/python writes + git-diff verify |
| O-03 explicit gates | no decision inferred; GATE-G17 verbatim carried; 3 deferrals stay OPEN |
| O-04 routing guard | guard run pre-dispatch (PASS DISPATCH REC5.2) and again post-artifacts |
| O-05 verification | records-in: docs/row/link/guard/regen checks; exact commands + counts in the run ledger |
| O-06 close evidence | ci-boundary read at pushed head; run/PR/index/ledger updated at true completion |
| O-07 validation/history | stock check-build-memory separate from repo row tools + retained guard; 8 scoped warnings kept scoped (appendix mismatch → plan row, never rewritten) |
| O-08 production/sensitive | none — no production mutation, credentials, or collection; offline records-in only |
