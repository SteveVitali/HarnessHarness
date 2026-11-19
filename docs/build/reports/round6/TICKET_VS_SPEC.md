<!--
  docs/build/reports/round6/TICKET_VS_SPEC.md — reconcile-build mode=spec deliverable
  (BM-RECON-02), round-6 scope. Written by CAP6.3 (2026-11-19, ticket 323). Append-only.
  Historical same-named records: docs/build/TICKET_VS_SPEC.md (rounds 1–4, REC.2) and
  docs/build/reports/round5/TICKET_VS_SPEC.md (round 5, REC5.2) — never rewritten.
-->
# TICKET vs SPEC — round 6: landed code, ADRs and supplemental coverage vs `docs/experiments-spec.md`

**Spec under reconciliation.** `docs/experiments-spec.md` — the ratified round-6
supplemental contract (canonical v0.3, frozen sha256
`1879824c5fe556424ed01b083bd4edb8853f41a7c0db1cd68c6cd7f1ed62aebd` — the re-pinned
digest carrying the §10.5 "First-pass intervention amendments" rows A1–A7 appended
2026-11-16; the spec body stays frozen, X-01). The canonical
`spec/CANONICAL_SPEC.md` is preserved unchanged except the AR-24 amendment applied at
§5e.1 through GATE-G24's protocol (manifest `## Spec amendments applied`, 2026-11-10);
the evidence-and-adoption supplement is likewise preserved unchanged.

**Tags** (round-5 vocabulary). `in-spec` — the supplement required the deliverable
(the row's stamped `REQ-X*` ids are the citation; primary ids per
`docs/build/planning/2026-11-07_round6-coverage.json`). `spec-implied` — a reasonable
reading of the supplement, not stated verbatim: per-ticket record schemas and emitters
under §3.9/§3.10/§4.3/§4.10/§9.3, gate/human readout machinery under §10.3, the
shared-decision machinery, the honest `withheld{}`/`n/a{}`/`unused{}` outcome records
the spec's honesty rules require. `ticket-added` — beyond the supplement: build
tooling, checkers, fixture batteries, capstone instruments, build-memory artifacts.

**Dispositions.** `ID` — leave as an implementation detail. `AX` — note in an
appendix. `FB→plan` — fold-back candidate carried to `SPEC_RECONCILIATION_PLAN.md`.
Requirement ids are append-only; no renumbering.

**Scope note.** 85 landed BUILD_INDEX rows (seq 253–322 incl. inserts — the whole
landed round-6 chain at `ada3f40f`). Gate/human rows (GATE-G20…G31, G28a…G28n, G33,
HUMAN-H12…H18) land no product code — their verbatim operator decisions are the
admission authority the spec-implied cells cite, verified under the gate↔record check
(V-8). Rows are per landed chain row; deliverable groups are tagged, not every file.
Round-6 rows carry `REQ-X[A-H]-[0-9]+` stamps verbatim; alias ids inherit their
targets' verdicts and are listed at the target row only (REQ-XH-003).

## Landed-record verification (reconcile AC-1: every landed module/packet/ADR/coverage-anchor vs the committed record)

All checks read committed state at `ada3f40f` (records-in, offline, USD $0) unless a
live read is named.

| # | reconciliation check | result |
|---|---|---|
| V-1 | Dependency-admission audit — `git diff origin/main..ada3f40f` over every `Cargo.toml` + `Cargo.lock` | **clean.** Zero new external dependencies: no `+name =` line exists in the `Cargo.lock` diff across the whole round-6 chain; the admitted set (`ring`@hharness-secrets, rustls graph@hharness-tls, test-only `tempfile`@hharness-varhost) is unchanged — no unadmitted dependency exists |
| V-2 | Schema/generator/generated audit — `bash scripts/check-drift.sh` regenerates every listed output | **ok** — generated artifacts byte-identical to the schema source. AR-25's `open_session.exec` schema change (R6.38) regenerated through hh-codegen per CC7; no hand-edited generated file exists |
| V-3 | Amendment-admission map — §10.5 intervention rows A1–A7 → ADRs + manifest amendment lines | **complete.** A1→ADR-0443, A2→ADR-0444, A3→ADR-0445, A4→ADR-0446, A5→ADR-0447, A6→ADR-0448, A7→ADR-0449; manifest `## Spec amendments applied` lines 919–925 each carry date · section · approver (operator rulings R-1…R-11 verbatim, `intervention-2026-11-15`) · ADR. AR-24's canonical amendment: GATE-G24 PASSED (verbatim "yes, authorized, proceed" 2026-11-10) → manifest line 917 → `spec/sections/05e-control-orchestration.md` + inline CANONICAL copies, applied by R6.38. No silent amendment exists |
| V-4 | Spec identity — `shasum -a 256 docs/experiments-spec.md` vs the register re-pin | **exact.** Working-tree digest `1879824c5fe5…` equals the coverage register's revision note re-pin (`6370b6cec69b -> 1879824c5fe5`, the A1–A7 append); the spec body is otherwise untouched since ratification |
| V-5 | ADR index/trigger generation through tools — `gen-revisit-triggers.sh --check`; `adr-index.sh` freshness; stock validator | **in sync.** `gen-revisit-triggers --check` clean (ADR-0450/0451 placed in BL-47 by CAP6.2); `docs/adr/README.md` regenerated; check-build-memory 0 violations / 64 warnings — the only ADR-class warning is BM-ADR-04 (canonical Appendix-B by-reference range names ADR-0217–0330 vs landed set → ADR-0451), carried as dispositioned legacy → plan row R6A-1 |
| V-6 | `.sha256` sidecars — every committed round-6 record under `docs/build/reports/` | **verified.** CAP6.1 NC-4: 35/35 sidecars recompute over payload; this ticket adds sidecars for each new artifact in the same `shasum -a 256` format |
| V-7 | Register↔spec counts — coverage register vs spec Appendix A/§0.2 | **verified (inherited, re-read).** 456 candidates = 370 adopted + 81 merged-with + 5 superseded-by; layers E=330, D=92, R=33, P=1 (CAP6.1 NC-11); `check_round6_coverage.py` 35/35 (CAP6.2) |
| V-8 | Gate↔record verbatim — every dispatched round-6 gate/human readout vs landed scope | **verified.** All dispatched gate/human rows have committed non-PENDING readouts (CAP6.1 NC-5); the 15 misfiled decision rows are restated verbatim inside `## GATE DECISIONS` (CAP6.2, DF-CAP6.1-1); `supplement_guard.load_decisions` reads all 15; guard PASS `DISPATCH — CAP6.3`. Dispositions match records: GATE-G29 `full` ⇄ strict-replay fold; GATE-G30 `re-scope{end-first-pass-early}` ⇄ W4 `n/a{scope_end}`/`unused{scope_end}`; GATE-G23 ⇄ ADR-0438/0439 admitted |
| V-9 | Deferral discharge text vs verify clauses — every round-6 `DF-*` row | **mostly clean, two seams.** Discharged with dated rows: DF-R6.7-1 (legs at R6.10+R6.16), DF-R6.23-1 (DONE 2026-11-19 R6.47, `n/a{scope_end}` + complete citation chain), DF-R6.28-1 (R6.32b), DF-R6.31-1 (pages.yml read-back), DF-R6.32-1 (R6.32b), DF-R6.50-1 (dated DONE, HUMAN-H17 — lead token still OPEN, the recorded row-reader seam). **Seam S-3:** DF-R6.45l-1's dated DONE text exists only on the unmerged `codex/harnessharness-r6.51` branch (PR #413) — at the chain tip the row still emits OPEN; **Seam S-4:** DF-CAP6.1-1…5 were minted OPEN and dispositioned in CAPSTONE_CLOSURE.md but carry no dated status rows in DEFERRALS.md (same row-reader family as DF-R6.50-1) → dispositioned rows in the plan |
| V-10 | Withheld/typed tokens never promoted — over-claim scan | **clean (inherited).** CAP6.1 NC-10: 23 distinct typed tokens; every sampled use is a typed refusal with basis — `n/a{scope_end}`, `withheld{un_reproofed_successor}`, `unused{scope_end}`, `unexecuted{no_executor}`, `unavailable{no_billing_export}`; `live-executed` machine claims all map to committed `hh-live-evidence/1` records; `outside_verified: false` on every publication |
| V-11 | §10.5 register ADR coverage — every first-pass AR row vs `docs/adr/` | **19/21 minted; two missing.** Missing: **AR-08** (model-mediation proxy — mechanism landed at R6.10/R6.11) and **AR-10** (`GenParamPolicy` path-qualified pins — landed at R6.10). R6.10's contract item 12 ("ADRs for AR-08 … and AR-10") was never discharged — a real gap, not an inference → plan row R6A-2. All other needed-in-round-6 ADRs minted and cited |
| V-12 | Backlog contract — `check-backlog.sh` | **PASS** — complete + non-duplicating + verdict-consistent (235 expected sources; two sums 66/66 for the register's own scope) |
| V-13 | REQ-X coverage state — CAP6.1 matrix × CAP6.2 closure | **410/456 closed** (MET 409 + MET-DIFFERENTLY(ADR-0439) 1; 0 MISSING). 46 PARTIAL, each dispositioned: 34 routed to a later pass (owner = the named pass, trigger per row), 5 discharged by CAP6.2's artifacts (XH-110/021/029/106/015), 4 scope-ended under GATE-G30 `re-scope{end-first-pass-early}` (XD-110/XD-119/XH-111/XH-115 — typed honest outcomes), 1 unexecuted (XC-325 — `unexecuted{no_executor}` + waiver request), 2 alias rows (XD-124/XE-127) inheriting at targets |
| V-14 | Landed-row records — every chain row vs run/PR/BUILD_INDEX | **verified.** CAP6.1 NC-2/2b: every adopted id's owner has a BUILD_INDEX row + run/pr anchors; CAP6.2 backfilled the 12 unrecorded landings with a dated normalization row; the index carries 85 round-6 rows |
| V-15 | Frozen-record integrity — prereg/results emit-identical legs | **bound by dated amendment.** `check-fp-prereg` + `check-fp-results` emit-identical FAILs on drifted fixture inputs are bound by `2026-11-19-fp-prereg-hash-drift.md` (DF-CAP6.1-2); frozen records never rewritten |

## Per-row tag table

| seq | ticket | PR | in-spec deliverables (primary REQ-X ids) | spec-implied deliverables | ticket-added deliverables | ADRs |
|---|---|---|---|---|---|---|
| 253 | R6.1 | (chain) | `REQ-XH-001/002/003/004/008/024/025/026` `ID` | `hh-supplement-guard/1` v1.1.0 second-frozen-source extension + REQ-X register wiring (§10.6 AR-31's recorded decision) `ID` | coverage register `round6-coverage.{json,md}` + guard fixture battery `ID` | — |
| 254 | R6.2 | (chain) | `REQ-XA-002/003/004/006/011/016/101…113`, `REQ-XH-032` `ID` | scratch-driver provenance manifest + hardening records-in fold (§2.2/§2.3) `ID` | provenance manifest + verbatim driver inventory `ID` | — |
| 255 | R6.3 | #296 | `REQ-XA-001/005/007/008/013/014/015/114…125`, `REQ-XB-222`, `REQ-XH-019/028` `ID` | AR-02 S4 deviation record + AR-03 dated corrections as committed records (`hh-protocol-deviations/1`) `ID`; AR-32 evidence-card register leg (§9.6) `ID` | corrections emitter + record cards `ID` | ADR-0423 (AR-01) |
| 256 | R6.4 | #297 | `REQ-XF-005/018/020/101…107/114` `ID` | "Instrument" identity + tokens/themes (§9.1/§9.2) `ID` | `scripts/site/` tokens + 8-slot palette `ID` | — |
| 258 | R6.5 | (chain) | `REQ-XF-001/002/003/007/008/011/012/014/017/021/108/109/116…122/125/139/141`, `REQ-XH-020` `ID` | `hh-site-data/1` records→site-data contract + always-present/phase-1 pages (§9.3) `ID` | `build.py --emit/--check` generator `ID` | — |
| 259 | R6.6 | #300 | `REQ-XB-015`, `REQ-XC-014/120/212`, `REQ-XD-003/091/092`, `REQ-XF-006/009/010/019/022/126…130/134/135/137/138/140/142`, `REQ-XH-018/022` `ID` | claims lexicon + `hh-site-publication/1` (§9.4/§9.6) `ID` | `check-site.sh` 14-leg checker + mutation battery + `pages.yml` `ID` | ADR-0424 (AR-28) |
| 260 | R6.7 | #301 | `REQ-XC-001/008/104/116` `ID` | integer-µ$ rate unit + rounding role + `NoPrice{unrepresentable_rate}` (§4.5, AR-09); Ollama lane dialects + `forward_raw` (§4.1, AR-11) `ID` | lane fixtures + r6_7 battery `ID` | ADR-0425 (AR-09), ADR-0426 (AR-11) |
| 261 | R6.8 | #302 | `REQ-XA-010`, `REQ-XB-012/101/102/103/105/108/120` `ID` | hharness-runner placement + coordinator sole-writes model (§3.2, AR-04) `ID` | runner skeleton + r6_8 battery `ID` | ADR-0427 (AR-04) |
| 262 | R6.9 | #303 | `REQ-XA-009/127`, `REQ-XB-003/006/007/009/106/107/109/110/111/112/117/118/119` `ID` | cell WORKDIR/cpu/mem spec (AR-06); task-declared deadlines (AR-07); `harbor_reward_file/1` + `tb10_pytest_log/1` verdict channels (§3.3/§3.4) `ID` | helper flags + r6_9 battery `ID` | ADR-0428 (AR-06), ADR-0429 (AR-07) |
| 263 | R6.10 | #304 | `REQ-XC-002/007/012/013/101/103/105/106/107/108/109/206/208/209/213`, `REQ-XD-105`, `REQ-XE-005` `ID` | `hh-model-proxy` + token gate + §4.2 route/field policy + §4.3 durable-before-visible ledger order (AR-08, S-9 INTERP); `GenParamPolicy` pins (AR-10); `hh-l1-canon/1` `ID` | `hh-model-proxy` binary + control channel + r6_10 battery `ID` | **none — AR-08/AR-10 owed (R6A-2)** |
| 264 | R6.11 | #305 | `REQ-XC-003/004/009/011/110…115/119/123/124/126/210/322`, `REQ-XH-005/120` `ID` | release modes + budget bound/reservation/slice/settlement machinery (§4.4/§4.8; AR-08 continuation) `ID` | r6_11 battery `ID` | — |
| 265 | R6.12 | #306 | `REQ-XC-122/125`, `REQ-XD-108`, `REQ-XF-124`, `REQ-XH-101` `ID` | `hh.model_proxy.recording/1` + `RequestCanonicalizer` + `RecordedTransport` strict/lenient replay (§4.9); `hh-receipt-index/1` + condenser (§4.10, AR-20) `ID` | `check-proxy-offline.sh` + r6_12 battery `ID` | ADR-0430 (AR-20) |
| 266 | R6.13 | #307 | `REQ-XB-201…212/219/220/223/224`, `REQ-XH-009/023` `ID` | `gce-backend.sh` recipe + `hh-compute-decl/1`/`hh-compute-receipt/1` + nine-step teardown (§3.9/§3.11, AR-15) `ID` | fake-shim battery + `compute-open.sh` `ID` | ADR-0431 (AR-15) |
| 267 | R6.14 | (chain) | `REQ-XA-012`, `REQ-XB-213/215/216/217/218`, `REQ-XC-005/006/117/118/121/127`, `REQ-XH-006/118` `ID` | `hh-live-pricing/2` compute rows + phase spend ledger + `evaluate_admitted_costs` + `hh-slot-costs/1` (§3.10/§4.5/§4.7, AR-14; AR-16 is a SUPP section — landed as mechanism) `ID` | cost tooling + settlement poller `ID` | ADR-0432 (AR-14) |
| 268 | R6.15 | (chain) | `REQ-XB-008/113/114/115/116/129`, `REQ-XC-102` `ID` | `allowlist_mediated` profile + static-musl `hh-cell-relay` + mediator CONNECT + decide-before-byte (§3.5, AR-05) `ID` | mediator + r6_15 battery `ID` | ADR-0433 (AR-05) |
| 269 | R6.16 | #310 | `REQ-XB-010/104/121/127/128/214`, `REQ-XD-060/111`, `REQ-XH-016/017/125` `ID` | `hh-runner-window/1` scheduled drive + slot/token supervision (§3.3/§6.3) `ID` | drive packet machinery + r6_16 battery `ID` | — |
| 270 | R6.17 | #311 | `REQ-XB-002/011/018/123/124/126`, `REQ-XC-306/315`, `REQ-XH-014/027` `ID` | Harbor tool admission at the pinned tag + re-proof machinery (§3.7/§3.8; AR-04's deferred pin leg — Q-X41 live-read cited) `ID` | `hh-reproof-packet/1`/`hh-reproof-register/1` emitters `ID` | — |
| 271 | R6.18 | #312 | `REQ-XB-001/017/122/221`, `REQ-XC-010`, `REQ-XD-002/004/011/020/021/050/051`, `REQ-XH-012/030` `ID` | `task-selection/1` + `comparative_protocol/1`/`comparative-quote/1` + support gate + envelopes (§6.1/§6.2/§6.3/§6.7, AR-12/AR-13) `ID` | selection materializer + r6_18 battery `ID` | ADR-0434 (AR-12), ADR-0435 (AR-13) |
| 272 | R6.19 | #313 | `REQ-XD-001/022/030/031/061`, `REQ-XF-123` `ID` | `finite_manifest_rate` + one-Holm-family + `hh-comparative-results/1` (§4.3/§6.5; admitted under AR-17 at GATE-G23) `ID` | estimator batteries `ID` | — |
| 273 | R6.20 | #314 | `REQ-XF-004/110/111/115` `ID` | leaderboard + frontier renderers over `hh-comparative-results/1` (§9.2/§9.3) `ID` | site renderers `ID` | — |
| 274 | R6.21 | #315 | `REQ-XF-013/015/016/112/113/131/132/133` `ID` | noise/pilot/quote/grids/trials + dormant viewer pages (§9.2) `ID` | viewer + fixture records `ID` | — |
| 275 | R6.22 | #316 | `REQ-XB-014`, `REQ-XC-201/202/204/214/301/302/303/304/305`, `REQ-XD-080/081` `ID` | `hh-harness-admission/1` AG1–AG9 + two-stage build + ST2 verify-only (§5.1/§5.2/§5.3, AR-18) `ID` | admission validator `ID` | ADR-0436 (AR-18) |
| 276 | R6.23 | (chain) | `REQ-XC-330`, `REQ-XH-102/103/104/107` `ID` | carried binary sets + layer export/load/verify + HF upload kit + live-credential scan (§4.10, AR-20 tail) `ID` | `hf-upload.sh` + `check-hf-upload.sh` `ID` | ADR-0437 (AR-15/AR-20 upload leg) |
| 277 | R6.24 | (chain) | `REQ-XC-307/308/309/310/320/327`, `REQ-XD-082/121/122`, `REQ-XE-001/003` `ID` | `hh-harness-render/1` pure admission×slot renderers for the three hosted harnesses (§5.3/§5.4) `ID` | renderers + byte-parity battery `ID` | — |
| 278 | R6.25 | #319 | `REQ-XB-013`, `REQ-XC-203/205/207/311/312/317/318/321/323/324/328/329` `ID` | trajectory copy-out + integrity capture + admission smoke battery (§5.6/§5.7) `ID` | ST4 copy-out + sm battery `ID` | — |
| 279 | R6.26 | (chain) | `REQ-XC-325/326`, `REQ-XH-031` `ID` | PS0 probe planner + SM-*/R-replay runner-check surface (§5.7) — **its execution is the XC-325 PARTIAL leg (unexecuted, waiver rides G32)** `ID` | ps0 planner + runner-check scaffolding `ID` | — |
| 280 | R6.27 | (chain) | `REQ-XB-016`, `REQ-XC-313/319`, `REQ-XD-023/024/070/223` `ID` | comparative battery + `answer-scan/1` integrity scanner + review record + AR-17/AR-33 dossier for GATE-G23 (§6.4/§6.9) `ID` | battery + scanner `ID` | ADR-0438 (AR-17), ADR-0439 (AR-33) |
| 282 | R6.28 | (chain) | `REQ-XD-103/104/109`, `REQ-XE-004` `ID` | L1 apparatus: `l1_obs`/`ObsReplay`/`l1_drive` over the one recording codec (§7.4) `ID` | L1 drive machinery `ID` | — |
| 283 | R6.29 | (chain) | `REQ-XC-211`, `REQ-XD-040/042`, `REQ-XH-112/117/123/124/126/127` `ID` | continuation rules reading cost fields only + hold declarations (§6.7/§10.1.4, X-07) `ID` | `continuation.rs` + holds `ID` | — |
| 284 | R6.30 | #325 | `REQ-XB-004`, `REQ-XD-010/012/014`, `REQ-XH-121` `ID` | `hh-fp-sealed-view/1` + `hh-fp-dev-projection/1` — outcome masking until results (X-07) `ID` | seal/commit emitters `ID` | — |
| 287 | R6.31 | #329 | `REQ-XF-136` `ID` | `hh-site-publication/1` phase-1 record committed exactly as GATE-G22 presented + deploy read-back legs (§9.6; DF-R6.31-1 discharged at merge) `ID` | publication record + README spine `ID` | — |
| 290 | R6.32 | #335 | `REQ-XB-005/125`, `REQ-XH-116` `ID` | W1 PROV+TEARDOWN under GATE-G25; `Blocked{backend.package_pins_unavailable}` typed outcome (§3.9/§10.1.4) `ID` | `hh_w1` driver + window receipts `ID` | — |
| 290a | R6.32a | #337 | — (declaration repair inside the W1 admission) `ID` | re-cut `fp-w1.json` pins verified on the declared image (§3.9 declaration discipline) `ID` | pin verification record `ID` | — |
| 290c | R6.32b | (chain) | `REQ-XB-005/125`, `REQ-XH-116` (discharged legs incl. DF-R6.28-1/DF-R6.32-1) `ID` | W1 window body under GATE-G33: freeze + observe + 159-trial re-proof `live-executed` + harvest/teardown (§3.8/§10.1) `ID` | window evidence + live-evidence record `ID` | — |
| 291 | R6.33 | #342 | `REQ-XD-115`, `REQ-XH-111/113/115` `ID` | `hh-fp-prereg/1` packet freeze + replacement/exclusion application + readiness record (§10.1.3; ε=±.20 departure → XD-115 MET-DIFFERENTLY(ADR-0439)) `ID` | prereg emitter + freeze checker `ID` | — |
| 292 | R6.34 | #344 | `REQ-XE-107/108` `ID` | E2-T0 ADR bundle + model-port wire fidelity (§7.3; AR-21/22/23 + AR-24 text) `ID` | docs-stage records `ID` | ADR-0440 (AR-21+AR-24), ADR-0441 (AR-22+AR-25), ADR-0442 (AR-23) |
| 293 | R6.35 | #345 | `REQ-XE-103/104/112/113` `ID` | `hh-tmpl/1` + `json_head_tail@1`/`text_head_tail@1` + error-format classes (§7.3, AR-23) `ID` | tmpl module `ID` | — |
| 294 | R6.36 | (chain) | `REQ-XE-105/106/110/111` `ID` | tools surface + typed renderers on the armed `model_port` path (§7.3) `ID` | renderers `ID` | — |
| 295 | R6.37 | #347 | `REQ-XE-109` `ID` | observation plumbing — `result_renderer` mint inside `dispatch_at` (§7.3) `ID` | wiring + battery `ID` | — |
| 296 | R6.38 | #349 | `REQ-XE-115/116/117/118` `ID` | AR-24 applied verbatim (§5e.1 `stop_rule.submit{…}`) + `open_session.exec` member + regeneration (AR-25, CC7) `ID` | regenerated clients `ID` | — |
| 298 | R6.39 | #350 | `REQ-XE-119/120` `ID` | `exec_defaults` + `stdio=merged` + HH-native topology rung record (AR-22 tail; rung-3 would have raised the AR-04 amendment — rung recorded, never triggered) `ID` | spike records `ID` | — |
| 301 | R6.40 | #355 | `REQ-XD-041`, `REQ-XH-010` `ID` | paid W2 window under GATE-G26/FP-S: UPLOAD-W1 `archive_verified` (704/704) + PS0 + PP0/CORPUS withheld legs verbatim + TEARDOWN (§10.1) `ID` | window evidence `ID` | — |
| 302 | R6.41 | #356 | `REQ-XD-102`, `REQ-XE-121/122/126` `ID` | compaction slot + HH-native participant topology (§7.2/§8.3; AR-22 continuation) `ID` | slot machinery `ID` | — |
| 303 | R6.42 | #357 | `REQ-XD-106/123`, `REQ-XE-101/102/114/123`, `REQ-XH-011/114` `ID` | `hh-fp-partition/1` — W2 L1 corpus partition under cost-only continuation (§6.3) `ID` | partition emitter `ID` | — |
| 304 | R6.43 | #358 | `REQ-XD-125/201/202/203/220`, `REQ-XE-014/124/125` `ID` | `sweep-registry/1` frozen registry + phase-4 D3-FP wiring (§8.1/§8.7; SR-25 shared-baseline reading under AR-33) `ID` | sweep machinery `ID` | — |
| 306 | R6.44 | (chain) | `REQ-XD-107`, `REQ-XE-006/007/008/009` `ID` | paid W3 window $0-model under GATE-G27: PROV+body+L1 gate partition + teardown (§10.1) `ID` | window evidence `ID` | — |
| 308 | R6.45 | (chain) | — (window row; executed under GATE-G28 `triggered{W1′}`) | Wc contingency window: O-surface machinery repair (compose-frontend + `cp_in` nesting, REQ-XC-330 new runner version) + 14-member re-proof, `task_lists.state: blocked` verbatim (§10.1.4 + GATE-G28) `ID` | wc-repair-record + reproof live evidence `ID` | — |
| 308a/308b/308c/308e | R6.45b/c/d + GATE-G28b | (chain) | — (window rows) | Wc2/Wc3/Wc4 repair windows + PROV-Wc2 authorization — the repair chain under GATE-G28's envelope (§10.1.4; intervention lineage) `ID` | window evidence `ID` | — |
| 309a/c/e/h | R6.45e–h | #382/#386/#391 | — (window rows) | Wc5–Wc8 repair windows — byte-identical runner lineage, machinery verdicts (same) `ID` | window evidence `ID` | — |
| 309i | R6.45i | #392 | — (records row) | §10.5 intervention amendments A1–A7 appended + dated notes (spec body frozen — the register's own amendment protocol) `ID` | `intervention-2026-11-15` records `ID` | ADR-0443…0449 (A1–A7) |
| 309j | R6.45j | #393 | — (repair row) | cell-fidelity repair set D15/D17–D19 (A6 amendment's implementing machinery) `ID` | deadline/task.toml machinery `ID` | — |
| 309k | R6.45k | #394 | — (repair row) | egress hardening D8–D14 + `egress-real-clients` live leg + D16 epoch-4 scan (A4/A5 machinery) `ID` | `scan_hosts_in_text` + live leg `ID` | — |
| 309l | GATE-G28i | #395 | — (gate) | PROV-SMOKE envelope readout (A1; §10.3 machinery) `ID` | readout `ID` | — |
| 309m | R6.45l | #391 | — (window rows ×2) | smoke1 + smoke2 non-evidence smokes — never re-proof evidence (A1 verbatim label); epoch-5 declared-data record `ID` | smoke records + epoch-5 amendment `ID` | — |
| 309n | GATE-G28j | #391 | — (gate) | PROV-Wc9 readout incl. declared-data call + early-inspection checkpoint (A3) `ID` | readout `ID` | — |
| 309o | R6.45m | (chain) | — (window) | Wc9 evidence re-proof — early-inspection + machinery-signature stop rule (A3) `ID` | window evidence `ID` | — |
| 309q | R6.45n | #405 | — (window) | smoke4/smoke5 diagnostic smokes (non-evidence label, GATE-G28k envelope) `ID` | smoke records `ID` | — |
| 309s | R6.45o | (chain) | — (window) | Wc10 evidence re-proof `ID` | window evidence `ID` | — |
| 309t | GATE-G28m | (chain) | — (gate) | PROV-Wcs1 readout `ID` | readout `ID` | — |
| 309u | R6.45p | (chain) | — (window) | Wcs1 bounded successor re-proof `ID` | window evidence `ID` | — |
| 309w | R6.45q | (chain) | `REQ-XH-111/115` legs; PP0/L1 withheld legs discharged | Ws1 withheld-subject-legs window: PP0 9 cells + L1 RECORD corpus 28 + L1 gate-A `hh-l1-conformance/1` `result: fail` verbatim + **compute estimate-over-ceiling 5,250,954 µ$ vs 4,136,280 declared (~127%) recorded** (G32 waiver item) `ID` | `hh-l1-corpus/1` manifest tooling + WAL recovery fold `ID` | ADR-0450, ADR-0451 (unplanned record ADRs — placed BL-47) |
| 309p | GATE-G28k | #405 | — (gate) | PROV-SMOKE diagnostic envelope under `re-scope{harden-again}` `ID` | readout `ID` | — |
| 309r | GATE-G28l | #407 | — (gate) | PROV-Wc10 `run` authorization `ID` | readout `ID` | — |
| 309v | GATE-G28n | #407 | — (gate) | PROV-Ws1 `run` authorization (first metered under the directive) `ID` | readout `ID` | — |
| 309 | GATE-G29 | #407 | — (gate) | G-L1 `full` ruling + standing sign-off (§6.5/§10.3) `ID` | readout `ID` | — |
| 310 | R6.46 | #408 | `REQ-XC-015`, `REQ-XD-043/044/062/101/110/112/113/114/116/117/118` `ID` | `hh-fp-trials/1` + `hh-fp-e/1` FP-E quote + K7 L2 wiring; XD-110's truncated-pair honest outcome (§6.7/§10.1.4) `ID` | quote + measurement emitters `ID` | — |
| 311 | HUMAN-H15 | #409 | — (human) | `hh-fp-e/1` `purchase_decision: NOT_NEEDED` — A_eff covers needed µ$ (§10.1.5; Q-X63) `ID` | readout `ID` | — |
| 312 | GATE-G30 | #411 | — (gate) | `re-scope{end-first-pass-early}` — FP-E quote admits zero executable work; W4 skipped; claims withheld (A3 ladder) `ID` | readout `ID` | — |
| 313 | R6.47 | #412 | — (window close act) | W4 `unused{scope_end}` — `gcloud projects delete` + `n/a{scope_end}` upload/settlement records (GATE-G30 arm; REQ-XB-211 check repaired) `ID` | teardown + typed-outcome records `ID` | — |
| 314 | R6.48 | #414 | `REQ-XD-005/090/119/120/224` `ID` | `hh-fp-seal-open/1` one-open seal + `answer-scan/1` over 53 scored trials + P2-FP/P3-L2/P4-FP packets (§6.11/§10.1.3; XD-119 `sampled_n=0` honest) `ID` | `fp_results.rs` emitter — **its `docs/build/**` literals trip the ci-gating readers scan → the red-stack item** `ID` | — |
| 315 | HUMAN-H16 | #414 | `REQ-XD-071` `ID` | flagged-pass review: 35/35 cleared, 0 confirmed (§6.9) `ID` | readout `ID` | — |
| 316 | R6.49 | #415 | `REQ-XD-126/222`, `REQ-XF-143`, `REQ-XH-119/122` `ID` | `fp_results freeze` — `hh-flagged-review/1` + first-pass labels + phase-3/4 pages (§9.6/§10.1.3) `ID` | freeze + renderers `ID` | — |
| 317 | GATE-G31 | #415 | — (gate) | publication presentation disposition (links-only, sanitized-only) `ID` | readout `ID` | — |
| 318 | HUMAN-H17 | #416 | — (human) | dataset flip → public at `a11591e9` + superseding flip records (§9.6; discharges DF-R6.50-1) `ID` | readout + flip records `ID` | — |
| 319 | R6.50 | #416 | `REQ-XH-105` `ID` | first-pass `hh-site-publication/1` records under `docs/results/publication/` pinning GATE-G31 verbatim (§9.6) `ID` | publication records `ID` | — |
| 320 | HUMAN-H18 | #417 | — (human) | billing reconcile: model $4.52 billed; compute billing `unavailable{no_billing_export}` (§10.1.5) `ID` | readout `ID` | — |
| 321 | CAP6.1 | #417 | — (capstone; no owned ids) | `hh-cap6-gap-hunt/1` + 456-row `COVERAGE_MATRIX.csv` — verdicts recorded before run-ledger reads (§10.6 tail machinery) `ID` | `cap6_gap_hunt.py` + machine packet `ID` | — |
| 321a | R6.51 | #413 | — (closure insert) | `hh-verify-host-derivation/1` — runtime-evidence verify-host derivation emitting exactly smoke1's three observed hosts (DF-R6.45l-1's verify clause, met verbatim) `ID` | `fp_verify_hosts` emitter + derivation record `ID` | — |
| 322 | CAP6.2 | #418 | `REQ-XH-110/021/029/106/015` + 41 routed per O4/REQ-XH-001 `ID` | closure record — two sums recomputed, A2-amended accounting, live reads, §0.3 history, 34-id later-pass register, G32 signature list `ID` | closure/routing/readiness artifacts `ID` | — |

## Seam findings carried to SPEC_RECONCILIATION_PLAN.md

- **S-1 — two §10.5 ADRs never minted.** AR-08 (proxy) and AR-10 (`GenParamPolicy`):
  R6.10 contract item 12 required them; the mechanisms landed and are cited in run/PR
  records, but no `docs/adr/` file exists for either → plan row R6A-2 (mint-as-record
  proposal; the register's intent is an ADR per landed design).
- **S-2 — canonical Appendix-B range still stale.** R5A-1 remains unticked; the landed
  set is now ADR-0217–0451 → plan row R6A-1 carries the same amendment, widened.
- **S-3 — DF-R6.45l-1 discharge rides the unmerged #413.** Its dated DONE text exists
  on `codex/harnessharness-r6.51` only (with a malformed `DONE  **OPEN**` lead token);
  at the chain tip the row emits OPEN → dispositioned, and it makes the #413 merge (or
  a coordinator-appended dated row) load-bearing for the deferrals file.
- **S-4 — DF-CAP6.1-1…5 have no dated status rows.** Dispositions live in
  `CAPSTONE_CLOSURE.md`; `deferrals-open.sh` still emits all five OPEN → proposed
  dated status-update rows for the coordinator (append-only), same pattern as
  DF-R6.50-1's status-update row.
- **S-5 — §10.5 "21 ADRs needed" vs landed set.** 19 minted + AR-08/AR-10 owed;
  plus 7 intervention ADRs (A1–A7) and 2 unplanned record ADRs (0450/0451) = 29
  round-6 ADRs → dispositioned; count-line correction rides R6A-2's resolution.
- **S-6 — inherited red stack.** `check-ci-gating.sh` readers scan FAILs 57 violations
  at this tip (`fp_e_emit.rs`, `fp_results.rs` bin+lib, `tests/r6_48.rs`,
  `tests/r6_49.rs` `docs/build/**` literals; 31 violations at #414's head where the
  red first appears — R6.48/R6.49 introduced them). Diagnosed in
  `INTEGRATION_PLAN.md` as a named item; the repair needs the coordinator/operator's
  call (the hint forbids allowlisting a real read).
