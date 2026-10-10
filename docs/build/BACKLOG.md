# BACKLOG — deduplicated owed-work register (REC.1, 2026-10-06 · refreshed R2.26, 2026-10-10)

> The single deduplicated register of everything the build left owed at
> GATE-ACCEPT (2026-10-02, accepted). One row in `BACKLOG.csv` is one
> deliverable item; an obligation named by several sources (a matrix
> PARTIAL verdict, a DEFERRALS row, an ADR trigger, a register row)
> appears on exactly one row and every gathered source identifier occurs
> in exactly one `sources` cell. Nothing owed is lost; nothing is
> claimed. This register catalogues state — it signs nothing and flips
> no deferral cell.

- **Mode:** `reconcile-build mode=backlog` · 2026-10-06 · branch `svitali/harnessharness-rec.1` ·
  refreshed `reconcile-build mode=backlog` · 2026-10-10 (R2.26) · branch `svitali/harnessharness-r2.26`
- **Machine form:** `docs/build/BACKLOG.csv` (42 rows; `check-backlog.sh` exit 0 — complete,
  non-duplicating, verdict-consistent; expected sources 156)
- **Sources consumed (R2.26 re-sweep):** `docs/tickets/DEFERRALS.md` (28 owed rows of 100 — 25 OPEN +
  3 PARTIAL) · `docs/build/COVERAGE_MATRIX.csv` (19 PARTIAL rows after the round-2 flips) ·
  `docs/adr/` (137 `## Revisit trigger` sections, ADR-0217…ADR-0353) · `research/registers/open-questions.md`
  (the ADR-0210…0216 deferred-question packages, revalidated at R2.22–R2.25) ·
  `research/registers/scope.md` (R-2.12.4 `deferred(ADR-0210)`) ·
  `research/registers/spec-debt.md` (216 active AssumptionDebtRecords, first sweep graded at R2.25) ·
  `research/registers/conflicts.md` (6 open conflicts — CF-476/CF-487 + CF-488/489/490/491) ·
  `docs/build/LEDGER.md` OPEN FINDINGS (5 entries) · `docs/build/readouts/GATE-ACCEPT.md` (10 signed deviations)

## How to read

- `landing` names the ticket/round/gate that discharges the row — never `TBD`.
- `accepted` rows are not forgotten: they carry a recorded deviation signed at
  GATE-ACCEPT or a discharged answer; they stay on the register so a future
  reconcile round re-arms their triggers.
- The 39 discharged DEFERRALS rows (latest cell `DONE`) are **not** owed and are
  not gathered, per `modes/backlog.md`; the two bookkeeping rows whose work is
  done but whose status cell was never flipped are BL-29.

## The backlog by landing

### Operator (humans-in-scope) — 3 rows

| BL | Item | Sources |
|---|---|---|
| BL-02 | R2 cross-camp human reviewer signature — machine cells DONE; the signature cannot be minted in-build | DF-S0.3-3, ADR-0225 |
| BL-03 | HUMAN-H1 — E3 surface-ecosystem binding for generated clients (+ ADR-0319, homed at R2.26) | DF-S4.10-1, ADR-0218, ADR-0301, ADR-0302, ADR-0319 |
| BL-04 | HUMAN-H2 — real issue tracker + signed webhook for the fleet adapter | DF-S5.6-1, ADR-0317 |

### round-2 — discharged (14 rows, closed with dated evidence R2.1–R2.25)

| BL | Item | Discharged at |
|---|---|---|
| BL-11 | Ledger retention/GC producer legs + S3.6 replay driver | R2.2 (residual R-2.2.1 verdict re-homed to BL-30 at R2.26) |
| BL-12 | Durable-execution producer legs | R2.3 (carried residual re-homed to BL-44 at R2.26) |
| BL-13 | Env verbs + snapshot cadence | R2.4 (ADR-0272's unmet legs re-homed to BL-16) |
| BL-18 | Egress/containment mediator legs | R2.9a/b (TLS-bound sources re-homed to BL-31) |
| BL-20 | IFC labels_leaves + remedy machinery | R2.12 |
| BL-21 | Audit-trail residual — hosted n/a, attestation, emission halves | R2.13 (carried residual re-homed to BL-45) |
| BL-22 | Approvals residual — modify + mid-dispatch ask | R2.11 |
| BL-23 | Reference-monitor executable acceptance | R2.11 (ADR-0343 deduplicated to BL-42) |
| BL-24 | Telemetry emitter/view halves + §5h.1 battery | R2.14 (ADR-0346's live-sink leg re-homed to BL-31) |
| BL-25 | Eval residual | R2.16 |
| BL-28 | Extension-trust producer legs | R2.20 |
| BL-29 | DEFERRALS bookkeeping flips | R2.1 |
| BL-40 | spec-debt register first revalidation sweep | R2.25 |
| BL-46 | DF-DOC.1-1 traceability id (DOC.2 owed a BL id to the next reconcile) | bookkeeping — the work discharged at R2.1 |

### round-2 — carried open (10 rows to GATE-G4)

| BL | Item | Sources | Carried reason |
|---|---|---|---|
| BL-01 | Cross-impl conformance cells (gate-accepted, signed item 9) | R-2.12.1, DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1, ADR-0223, ADR-0239, ADR-0353 | `env` — foreign toolchain (HUMAN-H3) |
| BL-05 | Gate-accepted executable-battery arms (item 10) | DF-S1.13-3, DF-S1.15-1, DF-S1.17-1, DF-S1.21-2, DF-S1.22-1, DF-S1.24-1, DF-S1.14-4, ADR-0247, ADR-0287 | `accepted` — signed live-corpus/OOP arms |
| BL-10 | Assembly residual — emitter-vs-exemption adjudication for the 11 no-document-path codes | R-2.1.4, DF-S1.9-2, ADR-0240, ADR-0350 | `in-build` — machine-achievable, round-2 capacity exhausted |
| BL-14 | Control-plane residual — DF-S1.20-1 legs (delegation_reason real-run, workflow/program interpreters) | R-2.6.1, DF-S2.11-1, DF-S1.20-1 | `in-build` — carried residual |
| BL-15 | Context/memory producer legs + §5c battery residuals | R-2.4.3, R-2.4.4, DF-S1.19-1, DF-S1.19-2, ADR-0254, ADR-0255 | `in-build` — carried residual |
| BL-16 | Tool-exposure residual — DF-S1.17-1 members + sync_source legs | R-2.5.3, DF-S1.17-2, DF-S1.17-3, ADR-0272 | `in-build` — carried residual |
| BL-17 | Verification-plane legs — judge binding + live-judge admissibility | R-2.7.1, R-2.7.2b, DF-S1.21-1, DF-S1.21-3, ADR-0300, ADR-0311, ADR-0347, ADR-0348 | `env` — offline-only ceiling (live judge not admissible) |
| BL-19 | Credential-broker residual — dpop + wrapped_long_lived | R-2.8.3, DF-S1.13-1, DF-S1.13-4, ADR-0244, ADR-0342 | `dep` — pure-std AEAD/asymmetric-verifier absence |
| BL-26 | Benchmark/measurement legs — live foreign-manifest import | R-2.9.4, R-2.9.6, DF-S1.24-2, ADR-0349 | `env` — foreign toolchain (HUMAN-H3) |
| BL-27 | Embed/MCP residual — oauth/mtls CallerCredential exchange legs | R-2.11.3, DF-S4.11-2, ADR-0303, ADR-0351 | `env` — live provider leg |
| BL-44 | Durable-execution residual — DF-S2.3-1 remaining producer legs (resume_set / defer / child-lease / non-fleet ingress / heal surface) | R-2.2.3, DF-S2.3-1, ADR-0265 | `in-build` — carried residual (new id at R2.26; the legs belong to rows whose requirement verdicts already landed — GATE-G4 dispositions) |
| BL-45 | Audit residual — per-event blob encryption (no conforming pure-std AEAD) + R-2.8.6 emission legs | R-2.8.6, DF-S1.15-3, ADR-0345 | `dep` — crypto-primitive absence (new id at R2.26) |

### REC.2 — 1 row (spec reconciliation; stays open)

| BL-30 | Spec/governance rulings — signer custody (ADR-0213/OQ-170), OQ-388 latency ratification, conflicts CF-476/487 + CF-488/489/490/491; also carries R-2.2.1 (its residual is the OQ-388 leg) | DF-S2.5-1, DF-S4.13-1, DF-S1.24-3, DF-S1.26-2, CF-476, CF-487, CF-488, CF-489, CF-490, CF-491, R-2.2.1, ADR-0260, ADR-0261, ADR-0267, ADR-0274, ADR-0305, ADR-0331 |

*R2.26 read: the four ruling legs are answered as rulings but not discharged — OQ-170 custody stays external/workstream-reserved (ADR-0331 D3); OQ-388 has a measured fixture bound but the WS-B1/K2 ratification is owed; CF-476/CF-487 re-checked still-pending at R2.24; CF-488–CF-491 opened in-round (all named-workstream conformances). The row stays `open`.*

### REC.3 — 2 rows (integration)

| BL-31 | Live provider transports + TLS-terminating production transport (+ R-2.8.4's owed leg and the TLS/live-sink-bound triggers ADR-0266/0340/0341/0346, homed at R2.26) | DF-S1.18-1, DF-S2.4-2, R-2.8.4, ADR-0253, ADR-0266, ADR-0284, ADR-0313, ADR-0340, ADR-0341, ADR-0346 |
| BL-32 | Remote fetch transports + remaining foreign import/export vocabularies | R-2.9.3, R-2.10.3, R-2.10.5, DF-S4.2-1/-2, ADR-0292–0294, ADR-0308 |

### round-2 revalidation — 4 rows (all swept at R2.22–R2.25, stay open)

| BL-35 | ADR-0212 Stage 0–1 package — 12 OQs re-checked at R2.22 (OQ-402 answered; OQ-131/-235/-240 narrowed; rest open with interim rules) |
| BL-36 | ADR-0213 Stage 2–3 package — 23 OQs re-checked at R2.23 (none answered; 13 narrowed; OQ-170 custody external) |
| BL-37 | ADR-0214 Stage 4–6 package — 34 OQs re-checked at R2.24 (OQ-177 answered; 26 narrowed; 7 open) |
| BL-39 | ADR-0216 fixer deferrals — re-checked at R2.22 (all open; CF-488 records the group-placement divergence) |

### program-resume (WS-L6 window) — 3 rows

| BL-33 | WS-L6 deferral package (ADR-0210): R-2.12.4 packaging/licensing + 13 owned OQs |
| BL-34 | ADR-0211 ratified deferral set D-1…D-6 + OQ-461/462/463 |
| BL-38 | ADR-0215 program-level candidates + OQ-440 (15 OQs) |

### Standing — 3 rows (re-issued, never closed)

| BL-41 | Ledger OPEN-FINDINGS dispositions — edit-tool stale-view standing rule honoured through R2.26 | accepted |
| BL-42 | ADR revisit triggers — quiet/dormant (44 standing conditions after the round-2 sweep) | open |
| BL-43 | ADR revisit triggers fired and answered during the build (45) | accepted |

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

*R2.26 note (2026-10-10):* round 2 discharged the machine-achievable core of
the carried set — 14 backlog rows closed and 12 matrix PARTIAL→MET flips landed
in-round (9 recorded here at closure). What remains is honest carried debt:
environment-gated (foreign toolchains, live providers), human-in-scope,
spec-ruling, dependency-blocked, and a small in-build residual now named by
BL-10/14/15/16/19/44. No source was dropped or double-counted —
`check-backlog.sh` exit 0.

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


## Revisit-trigger sweep — dated 2026-10-10 (R2.26)

All 137 `## Revisit trigger` sections re-evaluated against the landed record
(post-R2.25). Verdict changes vs the 2026-10-06 sweep: **15 fired-unanswered →
fired-answered** (ADR-0245, -0246, -0248, -0252, -0257, -0258, -0259, -0269,
-0273, -0286, -0288, -0289, -0316, -0318, -0330 — legs landed at R2.4–R2.20,
re-homed to BL-43); **6 re-graded quiet** off discharged rows (ADR-0271, -0290,
-0307, -0343, -0344, -0352 → BL-42); **ADR-0350 re-homed** BL-42 → BL-10
(homable — its trigger is DF-S1.9-2's residual); **ADR-0265 → BL-44,
ADR-0345 → BL-45** (new carried-residual rows); **ADR-0266/-0340/-0341/-0346 →
BL-31** (TLS/live-sink-bound); **ADR-0272 → BL-16, ADR-0347/-0348 → BL-17,
ADR-0319 → BL-03** (conditions bound to those rows' residuals); ADR-0343
deduplicated (was on BL-22 and BL-23). Round-2 ADRs ADR-0332…ADR-0353 are swept
here for the first time (ADR-0332–0339 quiet; -0342/-0349/-0351/-0353
fired-unanswered on their open homes; the rest read in the table).

| ADR | Verdict | Reading | Home |
|---|---|---|---|
| ADR-0217 | quiet | condition unmet; stands | BL-42 |
| ADR-0218 | fired-unanswered | E3 surface bind owed (HUMAN-H1); unchanged | BL-03 |
| ADR-0219 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0220 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0221 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0222 | quiet | condition unmet; stands | BL-42 |
| ADR-0223 | fired-unanswered | E2/E3 cross-impl cells environment-pending (gate-accepted); unchanged | BL-01 |
| ADR-0224 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0225 | fired-unanswered | cross-camp signature owed (HUMAN-H4); unchanged | BL-02 |
| ADR-0226 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0227 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0228 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0229 | quiet | condition unmet; stands | BL-42 |
| ADR-0230 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0231 | fired-unanswered | register package revalidated 2026-10-09 (R2.22–R2.25); interim rules in force | BL-39 |
| ADR-0232 | quiet | condition unmet; stands | BL-42 |
| ADR-0233 | quiet | condition unmet; stands | BL-42 |
| ADR-0234 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0235 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0236 | quiet | condition unmet; stands | BL-42 |
| ADR-0237 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0238 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0239 | fired-unanswered | E2/E3 cross-impl cells environment-pending (gate-accepted); unchanged | BL-01 |
| ADR-0240 | fired-unanswered | assembly residual legs; unchanged | BL-10 |
| ADR-0241 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0242 | fired-unanswered | register package revalidated 2026-10-09 (R2.22–R2.25); interim rules in force | BL-35 |
| ADR-0243 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0244 | fired-unanswered | credential-broker residual (dpop / wrapped_long_lived); unchanged | BL-19 |
| ADR-0245 | fired-answered | the M5/M6/M13–M15 emitter classes + §03 lowering carriers landed 2026-10-08 (R2.14); the pending-stamp revisit discharged | BL-43 |
| ADR-0246 | fired-answered | audit_view classes + ProcessMetric computation landed at R2.14 | BL-43 |
| ADR-0247 | fired-unanswered | gate-accepted battery arms; unchanged | BL-05 |
| ADR-0248 | fired-answered | the Stage-2 audit halves landed 2026-10-08 (R2.13): attestation, witnessed checkpoints, receiver receipts; DEFERRED_OBLIGATIONS discharged | BL-43 |
| ADR-0249 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0250 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0251 | quiet | condition unmet; stands | BL-42 |
| ADR-0252 | fired-answered | PlanMap/composite bindings + code_mode + surface_rejected emitters landed 2026-10-07 (R2.8) | BL-43 |
| ADR-0253 | fired-unanswered | live-transport legs; unchanged | BL-31 |
| ADR-0254 | fired-unanswered | context/memory producer legs owed (DF-S1.19-1/-2); unchanged | BL-15 |
| ADR-0255 | fired-unanswered | context/memory producer legs owed (DF-S1.19-1/-2); unchanged | BL-15 |
| ADR-0256 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0257 | fired-answered | diff_sanity conditioned rule + gate call-site halves landed 2026-10-08 (R2.15) | BL-43 |
| ADR-0258 | fired-answered | combined-failure / search_budget=unknown / LeakedSplit / FaultType legs landed 2026-10-08 (R2.16) | BL-43 |
| ADR-0259 | fired-answered | ApprovalOption.modify + mid-dispatch ask legs landed 2026-10-07 (R2.11) | BL-43 |
| ADR-0260 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0261 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0262 | quiet | condition unmet; stands | BL-42 |
| ADR-0263 | quiet | condition unmet; stands | BL-42 |
| ADR-0264 | quiet | condition unmet; stands | BL-42 |
| ADR-0265 | fired-unanswered | producer legs landed at R2.3; the residual producer set rides DF-S2.3-1 (BL-44) | BL-44 |
| ADR-0266 | fired-unanswered | amend()+AmendmentDiff landed (R2.9a); the TLS-terminating-transport condition rides DF-S2.4-2 | BL-31 |
| ADR-0267 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0268 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0269 | fired-answered | labels_leaves walk + remedy-consumed machinery landed 2026-10-07/08 (R2.9a/b, R2.12) | BL-43 |
| ADR-0270 | quiet | condition unmet; stands | BL-42 |
| ADR-0271 | quiet | the S3.6 replay-driver leg discharged (R2.2); remaining triggers are conditionals (D1 hide-post-head, D3 shared_live, D4 time-varying snapshots, D5 compensation order) — none met | BL-42 |
| ADR-0272 | fired-unanswered | sync_source + retrieval_eval fixture legs unmet (DF-S1.17-1/BL-16) | BL-16 |
| ADR-0273 | fired-answered | queue_next_turn steer delivery + OOP conformance landed 2026-10-07 (R2.6) | BL-43 |
| ADR-0274 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0275 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0276 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0277 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0278 | fired-unanswered | register package revalidated 2026-10-09 (R2.22–R2.25); interim rules in force | BL-36 |
| ADR-0279 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0280 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0281 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0282 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0283 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0284 | fired-unanswered | live-transport legs; unchanged | BL-31 |
| ADR-0285 | quiet | condition unmet; stands | BL-42 |
| ADR-0286 | fired-answered | code_mode exposure + the exposure-aware turn loop landed 2026-10-07 (R2.8) | BL-43 |
| ADR-0287 | fired-unanswered | gate-accepted battery arms; unchanged | BL-05 |
| ADR-0288 | fired-answered | remedy-ingress production + labels_leaves landed 2026-10-08 (R2.12) | BL-43 |
| ADR-0289 | fired-answered | decide_egress order + AC-H4 battery + leak-scan surfaces landed 2026-10-07 (R2.9a/b) | BL-43 |
| ADR-0290 | quiet | task_id round-trip + resolver-backed checks landed (R2.17/R2.18); the S6.1b debt-manager and second-M2-surface conditions are unmet | BL-42 |
| ADR-0291 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0292 | fired-unanswered | remote fetch / foreign vocabulary legs; unchanged | BL-32 |
| ADR-0293 | fired-unanswered | remote fetch / foreign vocabulary legs; unchanged | BL-32 |
| ADR-0294 | fired-unanswered | remote fetch / foreign vocabulary legs; unchanged | BL-32 |
| ADR-0295 | dormant | stands | BL-42 |
| ADR-0296 | quiet | condition unmet; stands | BL-42 |
| ADR-0297 | quiet | condition unmet; stands | BL-42 |
| ADR-0298 | quiet | condition unmet; stands | BL-42 |
| ADR-0299 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0300 | fired-unanswered | verification legs owed; unchanged | BL-17 |
| ADR-0301 | fired-unanswered | E3 surface bind owed (HUMAN-H1); unchanged | BL-03 |
| ADR-0302 | fired-unanswered | E3 surface bind owed (HUMAN-H1); unchanged | BL-03 |
| ADR-0303 | fired-unanswered | SSE landed (R2.19); the oauth/mtls mediator legs remain (DF-S4.11-2) | BL-27 |
| ADR-0304 | quiet | condition unmet; stands | BL-42 |
| ADR-0305 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0306 | fired-unanswered | register package revalidated 2026-10-09 (R2.22–R2.25); interim rules in force | BL-37 |
| ADR-0307 | quiet | no second hosting transport landed; the retention-policy leg it named is discharged at R2.2 | BL-42 |
| ADR-0308 | fired-unanswered | remote fetch / foreign vocabulary legs; unchanged | BL-32 |
| ADR-0309 | quiet | condition unmet; stands | BL-42 |
| ADR-0310 | quiet | condition unmet; stands | BL-42 |
| ADR-0311 | fired-unanswered | verification legs owed; unchanged | BL-17 |
| ADR-0312 | quiet | condition unmet; stands | BL-42 |
| ADR-0313 | fired-unanswered | live-transport legs; unchanged | BL-31 |
| ADR-0314 | quiet | condition unmet; stands | BL-42 |
| ADR-0315 | dormant | stands | BL-42 |
| ADR-0316 | fired-answered | belief-probe runtime emitters landed 2026-10-08 (R2.15) | BL-43 |
| ADR-0317 | fired-unanswered | HUMAN-H2 owed; unchanged | BL-04 |
| ADR-0318 | fired-answered | DefinitionInput::Ref publishing + Streamable-HTTP SSE landed 2026-10-09 (R2.19) | BL-43 |
| ADR-0319 | fired-unanswered | the unmet trigger is the E3 ecosystem bind itself (HUMAN-H1) | BL-03 |
| ADR-0320 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0321 | quiet | condition unmet; stands | BL-42 |
| ADR-0322 | quiet | condition unmet; stands | BL-42 |
| ADR-0323 | quiet | condition unmet; stands | BL-42 |
| ADR-0324 | quiet | condition unmet; stands | BL-42 |
| ADR-0325 | quiet | condition unmet; stands | BL-42 |
| ADR-0326 | fired-answered | stands (see the 2026-10-06 sweep for the full read) | BL-43 |
| ADR-0327 | quiet | condition unmet; stands | BL-42 |
| ADR-0328 | quiet | condition unmet; stands | BL-42 |
| ADR-0329 | quiet | condition unmet; stands | BL-42 |
| ADR-0330 | fired-answered | the amending retain/rebind call sites landed 2026-10-07 (R2.9a/b); the D5/D6 set was GATE-ACCEPT-signed | BL-43 |
| ADR-0331 | fired-unanswered | spec/governance ruling owed (named workstreams); unchanged | BL-30 |
| ADR-0332 | quiet | fires only on a hh-embed/2 dialect bump | BL-42 |
| ADR-0333 | quiet | HHZ1/retention-class conditions unmet | BL-42 |
| ADR-0334 | quiet | durable-exec conditions unmet (residual producers ride DF-S2.3-1 -> BL-44) | BL-42 |
| ADR-0335 | quiet | env-lifecycle/cadence conditions unmet | BL-42 |
| ADR-0336 | quiet | context/memory conditions unmet | BL-42 |
| ADR-0337 | quiet | steer-transport conditions unmet | BL-42 |
| ADR-0338 | quiet | model-plane conditions unmet | BL-42 |
| ADR-0339 | quiet | exposure-corpus conditions unmet | BL-42 |
| ADR-0340 | fired-unanswered | R2.9b landed its named legs; D3 reopens on the real TLS transport (DF-S2.4-2/BL-31) | BL-31 |
| ADR-0341 | fired-unanswered | same TLS-bound condition as ADR-0340 | BL-31 |
| ADR-0342 | fired-unanswered | credential-broker residual (dpop / wrapped_long_lived); unchanged | BL-19 |
| ADR-0343 | quiet | the modify arm landed at R2.11; the remaining triggers are spec-amendment conditionals | BL-42 |
| ADR-0344 | quiet | the IFC walk landed at R2.12; triggers are spec-amendment conditionals | BL-42 |
| ADR-0345 | fired-unanswered | trigger (a) awaits a conforming pure-std AEAD (DF-S1.15-3/BL-45); (b) rides signer custody (OQ-170/BL-30) | BL-45 |
| ADR-0346 | fired-unanswered | the loopback subscriber landed (R2.14); reopens when a real external observability backend lands (BL-31) | BL-31 |
| ADR-0347 | fired-unanswered | live-judge admissibility is the offline-ceiling lift (DF-S1.21-3/BL-17) | BL-17 |
| ADR-0348 | fired-unanswered | same live-judge condition as ADR-0347 (+ OQ-377 / third-granularity conditionals) | BL-17 |
| ADR-0349 | fired-unanswered | fires when the live foreign-manifest import lands (DF-S1.24-2/BL-26) | BL-26 |
| ADR-0350 | fired-unanswered | re-fires as U->F emitter rows land; the adjudication is DF-S1.9-2/BL-10 | BL-10 |
| ADR-0351 | fired-unanswered | TLS transport / OAuth provider legs still owed (DF-S4.11-2/BL-27) | BL-27 |
| ADR-0352 | quiet | scanner-backend / vocabulary / dialect-bump / seal-semantics conditions unmet | BL-42 |
| ADR-0353 | fired-unanswered | HUMAN-H3 environment unprovisioned; the bundle + self-check landed at R2.21 | BL-01 |

## Coverage sums — recomputed 2026-10-06 vs the CAP.3 headline

Recomputed from `COVERAGE_MATRIX.csv` (66 scoped requirement rows):

- **engineering closed = MET + MET-DIFFERENTLY + MET-ENGINEERED = 29 + 6 + 0 = 35/66**
- **requirement satisfied = MET + MET-DIFFERENTLY = 29 + 6 = 35/66**

CAP.3's headline (CAPSTONE_CLOSURE.md / GATE-ACCEPT readout): 29 MET ·
6 MET-DIFFERENTLY · 31 PARTIAL · 0 MISSING · 0 AT-RISK-INTEGRATION —
31 + 35 = 66. **Match.** CAPSTONE_CLOSURE.md does not carry an explicit
two-sums line; the recomputation is consistent with its Matrix movement
section. No mismatch finding.

**Recomputed 2026-10-10 (R2.26) vs the R1 closeout baseline.** The matrix
moved 35/66 → **47/66 on both sums** — earned, not quiet corrections:

- **engineering closed = MET + MET-DIFFERENTLY + MET-ENGINEERED = 41 + 6 + 0 = 47/66**
- **requirement satisfied = MET + MET-DIFFERENTLY = 41 + 6 = 47/66**

The 12 in-round PARTIAL→MET flips, stated exactly: R-2.5.2 (R2.8),
R-2.8.5 (R2.20), R-2.8.7 (R2.11) — recorded at their landing tickets — and,
re-verdicted at this closure with dated evidence: R-2.2.4 + R-2.2.5 (R2.4:
snapshot cadence + env backing ops), R-2.4.1 + R-2.4.2 (R2.5/R2.6: DF-S2.8-1
members), R-2.8.2 (R2.12: IFC walk + reader-set legs), R-2.9.1 (R2.14: all
three §5h.1 deferral rows), R-2.9.2 (R2.16: eval residual cells), R-2.11.1
(R2.1+R2.4: parked-detach closing pass + env verbs — the closed-sum spec
ruling stays BL-30/ADR-0274's, a register obligation not a requirement leg),
R-2.11.4 (R2.19: definition-publishing surface). **19 PARTIAL remain** —
their owed legs are the carried set enumerated in the round-2 accounting
(`docs/build/planning/2026-10-06_round2-decomposition.md` §Round-2 accounting).

## Register review — dated 2026-10-06

`research/registers/risks.md` received its first `## Round 1 review` section
(append-only) re-checking all eleven register rows (RK-01…RK-11) against the
landed record. No risk fired during the build window and no row was re-rated;
the watch items are RK-02 (untestable-absence residual), RK-05 (conditional
novelty claim), RK-06 (spec-debt never yet revalidated → BL-40) and RK-09
(E2/E3 legs → BL-01). `conflicts.md` carries two open conflicts (CF-476/CF-487)
→ BL-30 (REC.2 scope: they are rulings, not code). `spec-debt.md` carries 216
active records → BL-40 first revalidation.

**Round-2 review appended 2026-10-10 (R2.26).** `risks.md` gained its
`## Round 2 review` — no row re-rated; RK-06's first-sweep disposition recorded
(BL-40 discharged at R2.25; cadence standing via OQ-445/BL-33). `conflicts.md`
now carries six open conflicts (CF-476/487 + CF-488/489/490/491) → all homed on
BL-30. `spec-debt.md`'s 216 records carry dated first-sweep grades (21
confirmed / 193 still-valid-interim / 2 fired-unanswered → CF-491).

## Findings mapping (LEDGER OPEN FINDINGS → BL-41)

1. Edit-tool stale-view anomaly → **open, standing rule** (shell/python writes +
   `git diff` verify on all build-memory files; honoured through R2.26 — every
   write on this branch went through python/shell and was diff-verified).
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
  and are covered as free-form source tokens in the same cells. The 156
  stable ids (137 ADR + 19 owed matrix) all appear exactly once at the R2.26
  sweep; the 28 DF ids, the OQ ids, 6 CF ids, 5 LEDGER-OF ids, `D-1..D-6`,
  `R-2.12.4` and the `spec-debt-ADR-*` range token are covered by eye per the
  checker's own reminder — every source identifier occurs in exactly one
  `sources` cell.
- Cells containing commas are quoted; `|` inside cells is written as `∣`/spelled
  out where needed; `D-1..D-6` expands to `D-1` `D-2` `D-3` `D-4` `D-5` `D-6`
  (the six ratified spec §11.4 deferrals).
