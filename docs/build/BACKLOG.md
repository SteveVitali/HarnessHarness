# BACKLOG — deduplicated owed-work register (REC.1, 2026-10-06)

> The single deduplicated register of everything the build left owed at
> GATE-ACCEPT (2026-10-02, accepted). One row in `BACKLOG.csv` is one
> deliverable item; an obligation named by several sources (a matrix
> PARTIAL verdict, a DEFERRALS row, an ADR trigger, a register row)
> appears on exactly one row and every gathered source identifier occurs
> in exactly one `sources` cell. Nothing owed is lost; nothing is
> claimed. This register catalogues state — it signs nothing and flips
> no deferral cell.

- **Mode:** `reconcile-build mode=backlog` · 2026-10-06 · branch `svitali/harnessharness-rec.1`
- **Machine form:** `docs/build/BACKLOG.csv` (39 rows; `check-backlog.sh` exit 0)
- **Sources consumed:** `docs/tickets/DEFERRALS.md` (60 owed rows of 99) ·
  `docs/build/COVERAGE_MATRIX.csv` (31 PARTIAL rows) · `docs/adr/` (114 `## Revisit trigger`
  sections, ADR-0217…ADR-0331 — +0331 appended 2026-10-06) · `research/registers/open-questions.md` (103 `deferred(ADR-*)`
  questions) · `research/registers/scope.md` (R-2.12.4 `deferred(ADR-0210)`) ·
  `research/registers/spec-debt.md` (216 active AssumptionDebtRecords) ·
  `research/registers/conflicts.md` (2 open conflicts) · `docs/build/LEDGER.md` OPEN FINDINGS
  (5 entries) · `docs/build/readouts/GATE-ACCEPT.md` (10 signed deviations)

## How to read

- `landing` names the ticket/round/gate that discharges the row — never `TBD`.
- `accepted` rows are not forgotten: they carry a recorded deviation signed at
  GATE-ACCEPT or a discharged answer; they stay on the register so a future
  reconcile round re-arms their triggers.
- The 39 discharged DEFERRALS rows (latest cell `DONE`) are **not** owed and are
  not gathered, per `modes/backlog.md`; the two bookkeeping rows whose work is
  done but whose status cell was never flipped are BL-29.

## The backlog by landing

### Operator (humans-in-scope) — 2 rows

| BL | Item | Sources |
|---|---|---|
| BL-02 | R2 cross-camp human reviewer signature — machine cells DONE; the signature cannot be minted in-build | DF-S0.3-3, ADR-0225 |
| BL-03 | HUMAN-H1 — E3 surface-ecosystem binding for generated clients | DF-S4.10-1, ADR-0218, ADR-0301, ADR-0302 |
| BL-04 | HUMAN-H2 — real issue tracker + signed webhook for the fleet adapter | DF-S5.6-1, ADR-0317 |

### round-2 — 22 rows (the machine-achievable carried-forward set)

| BL | Item | Sources |
|---|---|---|
| BL-01 | Cross-implementation conformance cells — E2/E3 byte-equality re-runs (foreign toolchains; gate-accepted) | R-2.12.1, DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1, ADR-0223, ADR-0239 |
| BL-05 | Gate-accepted executable-battery arms (G2/G3 recorded set) | DF-S1.13-3, DF-S1.14-4, DF-S1.15-1, DF-S1.17-1, DF-S1.21-2, DF-S1.22-1, DF-S1.24-1, ADR-0247, ADR-0287 |
| BL-10 | Assembly Stage-3+ residual — profile_binding grammar, organisation layers, AC-CC-11 corpus gate | R-2.1.4, DF-S1.9-2, ADR-0240 |
| BL-11 | Ledger retention/GC producer legs + S3.6 replay driver | R-2.2.1, DF-S1.5-1, DF-S2.9-1, ADR-0271, ADR-0307 |
| BL-12 | Durable-execution producer legs — suspend/compensate/heal, healing_policy_ref, wakeup producers | R-2.2.3, DF-S2.3-1, ADR-0265 |
| BL-13 | Env verbs + snapshot cadence (un-ignores the last composed xfail) | R-2.2.4, R-2.2.5, R-2.11.1, DF-S2.9-3, DF-S2.10-1, ADR-0272 |
| BL-14 | Control-plane steer legs — queue_next_turn, steerable-interpreter, OOP conformance | R-2.6.1, DF-S2.11-1, DF-S1.20-1, ADR-0273 |
| BL-15 | Context/memory producer legs + §5c executable battery | R-2.4.1–R-2.4.4, DF-S2.8-1, DF-S1.19-1, DF-S1.19-2, ADR-0254, ADR-0255 |
| BL-16 | Tool-exposure residual — PlanMap/composite bindings, code_mode, surface_rejected emitters | R-2.5.2, R-2.5.3, DF-S1.17-2, DF-S1.17-3, ADR-0252, ADR-0286 |
| BL-17 | Verification-plane legs — diff_sanity rule, gate call-site halves, execution_alignment C2, judge binding, belief probes | R-2.7.1, R-2.7.2b, DF-S1.21-1, DF-S1.21-3, ADR-0257, ADR-0300, ADR-0311, ADR-0316 |
| BL-18 | Egress/containment mediator legs — decide_egress order, amend(), AC-H4 battery, tls.terminate/inspect_hooks, fork rebind call site, leak_scan surfaces | R-2.8.4, DF-S1.12-1/-2/-4/-6, DF-S2.4-1/-3, ADR-0266, ADR-0289, ADR-0330 |
| BL-19 | Credential-broker legs — Injected{carrier} mediated delivery, DPoP, known_value_encoded + LT-12 | R-2.8.3, DF-S1.13-1/-4, ADR-0244 |
| BL-20 | IFC labels_leaves + remedy machinery | R-2.8.2, DF-S2.7-1, ADR-0269, ADR-0288 |
| BL-21 | Audit residual — hosted n/a completeness, run-bundle attestation, 3-of-7 emission halves | R-2.8.6, DF-S1.15-3, ADR-0248 |
| BL-22 | Approvals residual — ApprovalOption.modify C2 + mid-dispatch human-ask legs | R-2.8.7, DF-S1.23-1, ADR-0259 |
| BL-23 | Reference-monitor Stage-3/4 executable acceptance (AT-H1, Π cell) | DF-S1.11-2 |
| BL-24 | Telemetry emitter/view halves + §5h.1 battery | R-2.9.1, DF-S1.14-1/-2, ADR-0245, ADR-0246 |
| BL-25 | Eval residual — combined-failure fixture, search_budget=unknown, LeakedSplit, HAL FaultType | R-2.9.2, DF-S1.22-2, ADR-0258 |
| BL-26 | Benchmark/measurement legs — foreign-manifest import, task_id round-trip, resolver-backed removal checks | R-2.9.4, R-2.9.6, DF-S1.24-2, ADR-0290 |
| BL-27 | Embed/MCP surface legs — DefinitionInput::Ref, Streamable HTTP SSE, oauth/mtls mediator | R-2.11.3, R-2.11.4, DF-S1.25-1, DF-S4.11-1/-2, ADR-0303, ADR-0318, ADR-0319 |
| BL-28 | Extension-trust producer legs — security.extension.* emitters | R-2.8.5, DF-S1.23-2 |
| BL-29 | DEFERRALS bookkeeping flips — DF-S1.3-2 and DF-S1.9-4 unflipped cells (work is done) | DF-S1.3-2, DF-S1.9-4 |
| BL-42 | ADR revisit triggers — quiet/dormant this round (30 standing conditions; sweep below) | 30 ADRs, per-trigger re-fire |

### REC.2 — 1 row (spec reconciliation)

| BL-30 | Spec/governance rulings — signer custody (ADR-0213), OQ-388 latency ratification, ModelProfile/2 `debt.hypothesis`, parked-detach closed sum; open conflicts CF-476/CF-487 | DF-S2.5-1, DF-S4.13-1, DF-S1.24-3, DF-S1.26-2, CF-476, CF-487, ADR-0260, ADR-0261, ADR-0267, ADR-0274, ADR-0305 |

*Progress 2026-10-06 (REC.2): rulings issued at **ADR-0331** — D1 discharges DF-S1.24-3 (DONE); D2 rules the parked-detach surface (plan A-1, unticked); D3 confirms custody external (OQ-170/WS-H3-H6-L4); D4 proposes the OQ-388 measurement fold (plan A-2, unticked); D5 dispositions the ADR-appendix delta (plan A-3). The row stays `open` — the spec folds await the operator's tick and the two ratifications their named workstreams. ADR-0331 appended to the sources cell and the sweep table.*

### REC.3 — 2 rows (integration)

| BL-31 | Live provider transports + TLS-terminating production transport | DF-S1.18-1, DF-S2.4-2, ADR-0253, ADR-0284, ADR-0313 |
| BL-32 | Remote fetch transports + remaining foreign import/export vocabularies | R-2.9.3, R-2.10.3, R-2.10.5, DF-S4.2-1/-2, ADR-0292–0294, ADR-0308 |

### round-2 revalidation — 4 rows (register stage-blocking packages)

| BL-35 | ADR-0212 Stage 0–1 stage-blocking questions (12 OQs + ADR-0242's deferred legs) |
| BL-36 | ADR-0213 Stage 2–3 stage-blocking questions (23 OQs + ADR-0278) |
| BL-37 | ADR-0214 Stage 4–6 stage-blocking questions (34 OQs + ADR-0306) |
| BL-39 | ADR-0216 fixer deferrals (OQ-465/467/468 + ADR-0231) |
| BL-40 | spec-debt register first revalidation sweep — 216 active AssumptionDebtRecords |

### program-resume (WS-L6 window) — 3 rows

| BL-33 | WS-L6 deferral package (ADR-0210): R-2.12.4 packaging/licensing + 13 owned OQs |
| BL-34 | ADR-0211 ratified deferral set D-1…D-6 + OQ-461/462/463 |
| BL-38 | ADR-0215 program-level candidates + OQ-440 (15 OQs) |

### Standing — 2 rows

| BL-41 | Ledger OPEN-FINDINGS dispositions — edit-tool stale-view standing rule; other findings discharged | accepted |
| BL-43 | ADR revisit triggers fired and answered during the build (30) | accepted |

## Themes

- **The PARTIAL matrix is producer-side debt, not consumer-side.** Almost every
  PARTIAL verdict exists because a declared ledger member or op has a parseable
  schema but no wired emitter. The dominant round-2 lever is emitter wiring
  (BL-11..BL-28), not new types.
- **One deferral row often owes several requirement rows.** DF-S2.8-1 is owed by
  R-2.4.1/R-2.4.2/R-2.4.3/R-2.4.4 simultaneously; DF-S2.4-1 by R-2.8.3/R-2.8.4;
  DF-S4.2-2 by R-2.9.3/R-2.10.3/R-2.10.5. They sit on one backlog row each —
  that is the deduplication.
- **ADR triggers and DEFERRALS rows name the same residuals.** 54 of the 114
  revisit triggers name a leg that an owed DEFERRALS row or PARTIAL verdict
  already carries; the trigger is filed on the same row as that leg so the
  trigger's re-fire condition and the residual's discharge travel together.
- **The register packages are the spec's own debt.** 103 deferred
  open-questions sit under six ratified deferral ADRs (ADR-0210…ADR-0216);
  they are owed as *answers/rulings*, not code — the round-2 revalidation rows
  re-check each against landed data (e.g. OQ-131's Stage-0 spike data is in the
  S0.3b worksheet, awaiting ratification).
- **Humans-in-scope are honest, signed, and small.** Three rows (BL-02..BL-04)
  need an operator or outside party; all three are signed deviations, not
  silent gaps.
- **Nothing is `TBD`.** Every row lands at a named ticket, round, or program
  window; standing triggers (BL-42) land on their own named re-fire condition.

## Revisit-trigger sweep — dated 2026-10-06

All 114 `## Revisit trigger` sections were swept against the landed record.
Verdicts: `quiet` (condition not met) · `fired-unanswered` (condition met,
residual owed → a BL row) · `fired-answered` (condition met and discharged this
build) · `dormant` (trigger exists but its subject is inert) · `superseded`
(none this round — no ADR was superseded).

| ADR | Verdict | Reading | Home |
|---|---|---|---|
| ADR-0217 | quiet | schemas/surfaces still homogeneous E1-generated; derive exporters still fit | BL-42 |
| ADR-0218 | fired-unanswered | generated client bound in-ecosystem (S4.10b/S5.3); E3 far-side binding unexercised | BL-03 |
| ADR-0219 | fired-answered | S1.2 crossed to `idp/1` canonical form (DF-S1.2-1 DONE); E2/E3 re-runs are BL-01 | BL-43 |
| ADR-0220 | fired-answered | E1 corpus + durable-frame/ephemeral-drop measurements landed (S0.3/S0.3b) | BL-43 |
| ADR-0221 | fired-answered | baseline crate deleted at the S1.26 boundary; protocol owns the crate | BL-43 |
| ADR-0222 | quiet | OQ-432 unresolved; S1.26-class mapping revisited at CAP.1, classification stands | BL-42 |
| ADR-0223 | fired-unanswered | E2/E3 conformance cells unrun — gate-accepted (item 9) | BL-01 |
| ADR-0224 | fired-answered | G1 pass ran over the D3 candidates (DF-S1.12-4 closed S2.4a; ADR-0266/0289 landed) | BL-43 |
| ADR-0225 | fired-unanswered | R2 machine cells complete; cross-camp human reviewer unsigned (item 7) | BL-02 |
| ADR-0226 | fired-answered | gate revalidations ran at G1/G3; ADR-0050 ecosystem decision stands | BL-43 |
| ADR-0227 | fired-answered | audit catalogue + attestation consumers landed (S1.15/S2.5/CAP.3); no drift | BL-43 |
| ADR-0228 | fired-answered | SessionPlan/EnvelopedOrdinaryEvent audited and migrated at S1.4 | BL-43 |
| ADR-0229 | quiet | Stage-3 C2 schema work did not surface during S3 | BL-42 |
| ADR-0230 | fired-answered | hosting landed S4.5a (DF-S1.3-2/-3-3 discharged); sealed-def half done | BL-43 |
| ADR-0231 | fired-unanswered | OQ-467 interim leaf homes still pending the fixer pass | BL-39 |
| ADR-0232 | quiet | judgement views landed without a second operator surface | BL-42 |
| ADR-0233 | quiet | OQ-383 deferred (ADR-0210); no env/projection-backing pressure | BL-42 |
| ADR-0234 | fired-answered | enriched ops + measured audit members landed (S3.12b/CAP.3) | BL-43 |
| ADR-0235 | fired-answered | heartbeat/lease TTL in force; no concurrent writers observed | BL-43 |
| ADR-0236 | quiet | HH-PROTOCOL/5 unpublished; D2/D3 conditions unmet | BL-42 |
| ADR-0237 | fired-answered | risk_class surfaced to ProjectionSurface at S1.7 | BL-43 |
| ADR-0238 | fired-answered | durable lease/gate ordering landed Stage 2 | BL-43 |
| ADR-0239 | fired-unanswered | generated exports exist (S4.4) but E2/E3 verification unrun | BL-01 |
| ADR-0240 | fired-unanswered | AC-CC-11 corpus gate + profile_binding grammar owed | BL-10 |
| ADR-0241 | fired-answered | S3.2 op family + receipt emission landed | BL-43 |
| ADR-0242 | fired-unanswered | S2 steps 5/7/8 landed; OQ-132/133 legs sit on the ADR-0212 package | BL-35 |
| ADR-0243 | fired-answered | mediation landed S2.4 + CAP.3 wire | BL-43 |
| ADR-0244 | fired-unanswered | Injected{carrier}/DPoP legs owed | BL-19 |
| ADR-0245 | fired-unanswered | §5h.1 exporter seam + halves owed | BL-24 |
| ADR-0246 | fired-unanswered | M5/M6/M13–M15 emitter legs owed | BL-24 |
| ADR-0247 | fired-unanswered | measurement.export leg owed — gate-accepted (item 10) | BL-05 |
| ADR-0248 | fired-unanswered | 3-of-7 emission halves + attestation owed | BL-21 |
| ADR-0249 | fired-answered | delivery receipts landed S2.1 | BL-43 |
| ADR-0250 | fired-answered | pending-events delivered/durable terminal landed | BL-43 |
| ADR-0251 | quiet | R-2.5.1¹ C1/provider-hosted source lift not crossed in-stage | BL-42 |
| ADR-0252 | fired-unanswered | PlanMap/composite bindings + surface_rejected emitters owed | BL-16 |
| ADR-0253 | fired-unanswered | real Embedder/Summarizer/ProviderCompaction adapters owed | BL-31 |
| ADR-0254 | fired-unanswered | turn-loop emitter call sites partially landed at CAP.3; residual DF-S2.8-1 | BL-15 |
| ADR-0255 | fired-unanswered | context.health producer leg owed | BL-15 |
| ADR-0256 | fired-answered | C1 interpreters + compute-policy emission landed (S4.7/S5.5) | BL-43 |
| ADR-0257 | fired-unanswered | diff_sanity conditioned rule + call-site halves owed | BL-17 |
| ADR-0258 | fired-unanswered | combined-failure/search_budget/LeakedSplit legs owed | BL-25 |
| ADR-0259 | fired-unanswered | ApprovalOption.modify + mid-dispatch ask legs owed | BL-22 |
| ADR-0260 | fired-unanswered | doctrine-kind deferred products await a spec ruling | BL-30 |
| ADR-0261 | fired-unanswered | DF-S1.26-1 DONE; DF-S1.26-2 closed-sum ruling owed | BL-30 |
| ADR-0262 | quiet | no refinement round has returned a provisional verdict | BL-42 |
| ADR-0263 | quiet | E1 body projection remains sole owner; no T5/T6 host | BL-42 |
| ADR-0264 | quiet | no non-macOS SandboxingBackend landed | BL-42 |
| ADR-0265 | fired-unanswered | healing_policy_ref + wakeup producer legs owed | BL-12 |
| ADR-0266 | fired-unanswered | amend() + AmendmentDiff provenance owed | BL-18 |
| ADR-0267 | fired-unanswered | WS-B1/K2 latency ruling owed (OQ-388 sits on BL-37) | BL-30 |
| ADR-0268 | fired-answered | extension-hook admission machinery landed S2.6; cross-process stdio untried | BL-43 |
| ADR-0269 | fired-unanswered | labels_leaves walk owed | BL-20 |
| ADR-0270 | quiet | no live PreconditionEnv; OQ-198 unresolved | BL-42 |
| ADR-0271 | fired-unanswered | S3.6 replay-driver leg owed (DF-S2.9-1) | BL-11 |
| ADR-0272 | fired-unanswered | env verbs + fs_tree cadence owed | BL-13 |
| ADR-0273 | fired-unanswered | queue_next_turn + OOP conformance owed | BL-14 |
| ADR-0274 | fired-unanswered | parked-detach closed-sum ruling owed | BL-30 |
| ADR-0275 | fired-answered | Group L ops + MCP transport landed | BL-43 |
| ADR-0276 | fired-answered | Stage-5 data + AC-CP-09 emission landed | BL-43 |
| ADR-0277 | fired-answered | S3.4a–c catalog/consumers landed | BL-43 |
| ADR-0278 | fired-unanswered | OQ-363 ruling carried on the ADR-0213 package | BL-36 |
| ADR-0279 | fired-answered | S4 publish/snapshots landed | BL-43 |
| ADR-0280 | fired-answered | S4.3 C1 kinds landed | BL-43 |
| ADR-0281 | fired-answered | S4.5a hosting service landed | BL-43 |
| ADR-0282 | fired-answered | DF-S3.5-1 codec landed | BL-43 |
| ADR-0283 | fired-answered | replay_mode policy surface; Stage-4/6 replay drivers landed | BL-43 |
| ADR-0284 | fired-unanswered | C1 non-static routing arms owed (with DF-S1.18-1) | BL-31 |
| ADR-0285 | quiet | OQ-203 unresolved | BL-42 |
| ADR-0286 | fired-unanswered | code_mode exposure owed | BL-16 |
| ADR-0287 | fired-unanswered | DF-S1.21-2 remaining half owed — gate-accepted | BL-05 |
| ADR-0288 | fired-unanswered | labels_leaves + remedy machinery owed | BL-20 |
| ADR-0289 | fired-unanswered | decide_egress order + AC-H4 battery owed | BL-18 |
| ADR-0290 | fired-unanswered | task_id round-trip + resolver-backed checks owed | BL-26 |
| ADR-0291 | fired-answered | every named item DONE at CAP.3 | BL-43 |
| ADR-0292 | fired-unanswered | swebench_submission foreign export owed | BL-32 |
| ADR-0293 | fired-unanswered | manifest.locations[] fetch transports + trust fields owed | BL-32 |
| ADR-0294 | fired-unanswered | inspect_eval_log / in_toto_bundle / telemetry_trace lifts owed | BL-32 |
| ADR-0295 | dormant | OQ-334 ruling awaits the deferred OQ-208/OQ-262 comparisons | BL-42 |
| ADR-0296 | quiet | no live hosted composite harness exists | BL-42 |
| ADR-0297 | quiet | no second operator surface beyond the CLI | BL-42 |
| ADR-0298 | quiet | T3/T4/T5/T7 not implemented | BL-42 |
| ADR-0299 | fired-answered | Stage-5/6 interpreter variants landed (bandit/predictor) | BL-43 |
| ADR-0300 | fired-unanswered | judge binding residual (DF-S1.21-3) | BL-17 |
| ADR-0301 | fired-unanswered | E3 surface binding leg | BL-03 |
| ADR-0302 | fired-unanswered | same E3 leg (honest-proxy posture stands) | BL-03 |
| ADR-0303 | fired-unanswered | DefinitionInput::Ref + Streamable HTTP SSE half owed | BL-27 |
| ADR-0304 | quiet | competition admission requires no schema change today | BL-42 |
| ADR-0305 | fired-unanswered | ModelProfile/2 `debt.hypothesis` ruling owed | BL-30 |
| ADR-0306 | fired-unanswered | OQ-177 grant-surface re-evaluation owed | BL-37 |
| ADR-0307 | quiet | no second hosting transport; the retention-policy leg rides BL-11 (OQ-388 interim stands) | BL-42 |
| ADR-0308 | fired-unanswered | awaiting tagged foreign adapters/manifests | BL-32 |
| ADR-0309 | quiet | Stage-5/6 migration views landed; richer refusal kind is conditional | BL-42 |
| ADR-0310 | quiet | multi-lane benchmarks are lab compositions — intended | BL-42 |
| ADR-0311 | fired-unanswered | CriticDeclaration{kind=judge} binding owed | BL-17 |
| ADR-0312 | quiet | no second promoted variant yet | BL-42 |
| ADR-0313 | fired-unanswered | TLS-terminating transport owed | BL-31 |
| ADR-0314 | quiet | delegates_charge non-deterministic leg held deferred (OQ-361 placed / OQ-462 deferred) | BL-42 |
| ADR-0315 | dormant | trainer landed; regression suites over real pairs not run | BL-42 |
| ADR-0316 | fired-unanswered | belief-probe emitters owed | BL-17 |
| ADR-0317 | fired-unanswered | HUMAN-H2 owed | BL-04 |
| ADR-0318 | fired-unanswered | DefinitionInput::Ref leg owed | BL-27 |
| ADR-0319 | fired-unanswered | oauth/mtls CallerCredential mediator owed | BL-27 |
| ADR-0320 | fired-answered | S6.2 automated protocol landed | BL-43 |
| ADR-0321 | quiet | foreign toolchains not provisioned | BL-42 |
| ADR-0322 | quiet | no second ecosystem precompiles these paths | BL-42 |
| ADR-0323 | quiet | Stage-6 retrospectives landed; body leg awaits live runs | BL-42 |
| ADR-0324 | quiet | live-drift checks await real TrackD sources | BL-42 |
| ADR-0325 | quiet | S5.6 adapter landed; DriftReport/2 conditional | BL-42 |
| ADR-0326 | fired-answered | CAP.3/GATE-ACCEPT discharged every named row | BL-43 |
| ADR-0327 | quiet | sealed op-enum placeholder stands; $18 price laments dormant | BL-42 |
| ADR-0328 | quiet | work-units dominance trigger stands | BL-42 |
| ADR-0329 | quiet | run-less respond_approval landed; rebind conditions stand | BL-42 |
| ADR-0330 | fired-unanswered | amending retain/rebind call sites owed | BL-18 |
| ADR-0331 | fired-unanswered | REC.2 rulings issued (D1–D5); ticked folds (plan A-1/A-2/A-3) + external ratifications (OQ-170 WS-H3/H6/L4, OQ-388 WS-B1/K2) owed | BL-30 |

Sweep tally: **55 fired-unanswered · 30 fired-answered · 28 quiet · 2 dormant ·
0 superseded** (115 total — REC.2 appended ADR-0331, 2026-10-06).

*Update 2026-10-06 (R2.1):* **ADR-0332** (doctor-identity ruling —
`KernelDescriptor.version` is the SemVer-class label) classified **quiet** —
its revisit trigger fires only on a `hh-embed/2` dialect bump; sources cell at
BL-42 (standing conditions 30 → 31; total 116). **BL-29 closed** — the
bookkeeping flips landed this ticket: DF-S1.3-2, DF-S1.9-4, DF-S1.26-2 and
DF-DOC.1-1 are all DONE with dated evidence in `DEFERRALS.md`.

*Update 2026-10-09 (R2.25):* **BL-40 discharged** — the first RK-06
revalidation of `research/registers/spec-debt.md` ran: all 216 active
AssumptionDebtRecords carry dated in-cell first-sweep grades (**21
confirmed · 193 still-valid-interim · 2 fired-unanswered · 0
superseded**); the fired-unanswered pair (ADR-0183/ADR-0208 — recorded
closure-before-stage clauses that did not hold literally) is recorded as
CF-491, and the register's header misidentification of its sole
`hypothesized` row (ADR-0202, not ADR-0049) as CF-490. RK-06 gains the
disposition note in `risks.md`; the revalidation cadence stays a standing
obligation (the `evidence_max_age` default rides OQ-445 → BL-33's WS-L6
window).

## Coverage sums — recomputed 2026-10-06 vs the CAP.3 headline

Recomputed from `COVERAGE_MATRIX.csv` (66 scoped requirement rows):

- **engineering closed = MET + MET-DIFFERENTLY + MET-ENGINEERED = 29 + 6 + 0 = 35/66**
- **requirement satisfied = MET + MET-DIFFERENTLY = 29 + 6 = 35/66**

CAP.3's headline (CAPSTONE_CLOSURE.md / GATE-ACCEPT readout): 29 MET ·
6 MET-DIFFERENTLY · 31 PARTIAL · 0 MISSING · 0 AT-RISK-INTEGRATION —
31 + 35 = 66. **Match.** CAPSTONE_CLOSURE.md does not carry an explicit
two-sums line; the recomputation is consistent with its Matrix movement
section. No mismatch finding.

## Register review — dated 2026-10-06

`research/registers/risks.md` received its first `## Round 1 review` section
(append-only) re-checking all eleven register rows (RK-01…RK-11) against the
landed record. No risk fired during the build window and no row was re-rated;
the watch items are RK-02 (untestable-absence residual), RK-05 (conditional
novelty claim), RK-06 (spec-debt never yet revalidated → BL-40) and RK-09
(E2/E3 legs → BL-01). `conflicts.md` carries two open conflicts (CF-476/CF-487)
→ BL-30 (REC.2 scope: they are rulings, not code). `spec-debt.md` carries 216
active records → BL-40 first revalidation.

## Findings mapping (LEDGER OPEN FINDINGS → BL-41)

1. Edit-tool stale-view anomaly → **open, standing rule** (shell/python writes +
   `git diff` verify on all build-memory files; this session honours it).
2. Concurrent-writer claim → **resolved** — the "writer" was the runner's own
   nested re-read; no live writer violation observed since.
3. Foreign-writer claim → **resolved** — same sweep; `git fsck` clean, no
   non-review mutation since.
4. G1 operator deviations → **discharged** — GATE-ACCEPT accepted all ten.
5. CC4 report-prose nit → **adjudicated out-of-scope** at CAP.3 (run-ledger
   prose, not a required record); the contextual-language sweep landed
   2026-09-29.

## Hygiene notes

- The checker `check-backlog.sh` gathers stable ids of the shapes `ADR-*`,
  `R-2.*` and `D-*`-family tokens; this repository's deferral ids are `DF-*`
  and are covered as free-form source tokens in the same cells. The 145
  stable ids (114 ADR + 31 matrix) all appear exactly once; the 60 DF ids,
  103 OQ ids, 2 CF ids, 5 LEDGER-OF ids, `D-1..D-6`, `R-2.12.4` and the
  `spec-debt-ADR-*` range token are covered by eye per the checker's own
  reminder — every source identifier occurs in exactly one `sources` cell.
- Cells containing commas are quoted; `|` inside cells is written as `∣`/spelled
  out where needed; `D-1..D-6` expands to `D-1` `D-2` `D-3` `D-4` `D-5` `D-6`
  (the six ratified spec §11.4 deferrals).
