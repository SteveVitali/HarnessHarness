# Round 5 independent coverage/layer review

Verdict: **changes-required** on the inspected draft. No P0/P1 defect established; the P2 findings below prevent a clean exact-predicate coverage verdict. Root is the sole repository writer and has been alerted to the findings. This is a planning review, not runtime, scientific, participant, rights or funding verification.

## Scope and method

Fresh-context inputs: `docs/evidence-and-adoption-spec.md`; `docs/build/planning/2026-10-30_evidence-decomposition.md`; its linked new 168–244 contracts; coverage Markdown/JSON; common invariants. Read root/docs AGENTS and the local decompose-spec skill. Deferrals-first returned no open rows. Manifest/state were explicitly outside the initial review while root appended them; no finding relies on their transient absence.

Read the full supplemental body, all 180 original requirement/acceptance rows through the exact literal companion, all concrete positive/negative contract ACs, skeleton scopes/coverage, and full-tail role/output clauses. Read-only Python analysis compared the companion against the actual Appendix A rows/dispositions/hooks/layers/hashes and resolved ownership/AC anchors. No repository writes, product tests, task/provider/container execution, branches or commits were performed.

## Structural results

- Supplement digest matches `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`.
- Exactly 180 source rows and 180 disposition rows; 167 adopted, 12 merged aliases, 1 superseded alias. Zero literal-row/hash/disposition/hook/legacy/layer discrepancies in JSON.
- Exactly 167 primary memberships across full contracts and skeleton coverage; no duplicate adopted owner or owner/file/anchor mismatch.
- All aliases retain the adopted target owner. D108 has only D304 as canonical target and explicitly retains D309/D310 dependencies; no second CP engine scheduled.
- All 24 distinct OL rows and all 23 distinct source AC anchors are mapped.
- Exactly 77 contracts, sequences 168–244; no duplicate sequence, forward semantic dependency or skeleton Run line found.
- All nine full-tail roles are explicit: CAP5.1/2/3, GATE-G17, REC5.1/2/3, DOC5.1/2. Their round-specific outputs, distinct read obligations and fresh acceptance are present.
- D/E/R/P and fixture/live/foreign distinctions are retained. No favorable-effect completion criterion found. Genuine task, operational, parity, confirmation, outside and formative work remain gated skeletons.
- The participant plan retains one outside verify/recompute reviewer and one shared 4–6-person branched cohort, with overlap disclosure, existing session/effort limits, no mandatory second cohort and no extra outside fresh/replay quota. No participant expansion established.

Counts are necessary but not sufficient: the findings below concern a predicate's actual evidence route or timing, not a missing ID.

## Findings

### COV-01 — P2 — Mandatory selected-client proof is owned by an optional component skeleton

Paths: `docs/build/planning/2026-10-30_evidence-coverage.md:30`, JSON REQ-A-106; `docs/tickets/227_R5.50__plugin.md:15`; `docs/tickets/198_R5.28__recipe.md:35`.

ID: **REQ-A-106** (also the §1.4 selected-client requests/notifications/callback-failure predicate).

The source requires the chosen client to complete required requests and updates and retain callback/notification failures. R5.50 is the sole owner and explicitly does nothing if the optional package is unselected. Its scope mentions client versions/callbacks/notifications, but that makes mandatory client evidence contingent on an unrelated package decision. R5.28's concrete ACs currently prove request fields only. Thus a no-plugin plan has no unconditional owning executable slice for the selected client's remaining predicate.

Minimal repair: move A106 primary ownership to R5.28 (or another unconditional existing client owner), add concrete positive required-request/update evidence and negative callback/notification/subleg failure checks with exact versions/negotiation and typed inapplicability where applicable. R5.50 may contribute the conditional extension-specific proof. Preserve one primary and update both coverage companions/contracts; do not add a client/cohort/framework.

### COV-02 — P2 — Compound predicates need explicit contributing checks, not only generic AC-3

Principal paths: `docs/build/planning/2026-10-30_evidence-coverage.md:27,70,73,118,119,130,189,191`; corresponding JSON entries; `191_R5.21__profile.md:41`, `179_R5.10__seal.md:41`, `178_R5.9__selection.md:41`, `180_R5.11__oracle.md:35`.

The general completion/DF rules correctly prohibit automatic full-target MET. The remaining problem is that several rows say primary-only, and primary AC-3 says no later contributor beyond the common audit, while the concrete AC-1/2 covers only one slice. An eventual instruction to remap predicates does not yet name the check/evidence owner that closes them:

| IDs | Retained predicate beyond current concrete slice | Minimal existing contributor/check route |
|---|---|---|
| A103 | Clean recipient verifies selected artifacts and completes every advertised procedure; profile/coordinate implementation cannot prove later recipient procedures | R5.22, R5.24–26 and appropriate actual recipient R5.52 evidence; retain unavailable-procedure limits |
| E102 | Correct/wrong/broken oracle controls, bounded instrument-only verifier retry, crash/resume and hidden-feedback separation, beyond seal/M1 | R5.9/11/14 (plus actual R5.43 when claimed); explicitly make one existing oracle owner enforce/test the frozen retry cap, same sealed submission, instrument-only charging and no participant feedback/relaunch |
| E103/E204 | Regrade crash/settlement/denominator/no recharge/no participant rerun, beyond pure immutable selection | R5.10/11/13/14 as applicable; keep R5.9 sole primary |
| D005 | Policy-to-payload proof beyond metric joins | Existing R5.18/20 wire proof |
| D008 | Admitted task-aware estimators/known-answer and heterogeneous validation beyond bootstrap/sign-flip implementation | Existing R5.16/38/40 evidence (and common metadata owner as needed) |
| A205/B004 | If selected, actual package/replication exercise beyond contribution-route prose/checks | Conditional R5.50 and relevant R5.52; unselected claims remain withheld |
| C007/G008 | If selected, package invocation/tamper and external-consumer acceptance beyond participant interpretation/forms | Conditional R5.50/51 and contribution owner R5.31; reuse the same cohort |
| H004 | Actual ADR/amendment approval provenance beyond dossier preparation | GATE-G9 provenance / later landed reconciliation; no dossier pass equals admission |
| H006 | Actual new acceptance signature beyond guard grammar/rejection fixtures | GATE-G17 provenance; retain owed E until actual fresh signature |

This is not a demand to duplicate primary ownership or turn evidence edges into forward dispatch dependencies. Existing ordering schedules most slices. Repair the per-target contributor/evidence routes and corresponding contract AC-3 obligations, give genuinely missing retry enforcement a concrete AC, and retain DF owner/trigger/withheld claims until the later predicate exists. C003's every-advertised-procedure predicate should be reconciled with the same recipient route rather than presumed closed by an R0-only check.

### COV-03 — P2 — Follow-up denominators and raw-observation deletion cannot close at formative sessions

Paths: `docs/build/planning/2026-10-30_evidence-coverage.md:181,184`; `docs/tickets/230_R5.53__formative.md:15,24`; `docs/tickets/234_HUMAN-H11__value-dates.md:17`; `docs/tickets/235_R5.55__value.md:15`.

IDs: **REQ-G-205 / REQ-G-208**, §8.4 and AC-8-2.

G205/G208 are primary-only R5.53. The source also requires later own-task/repeat-use/missing-follow-up denominators and actual deletion of **contacts and raw observations** 30 days after day-90 review, or earlier withdrawal where possible. That P evidence cannot exist at formative closure. The later H11/R5.55 already provide the natural route, but explicitly mention only contact deletion; raw-observation deletion is absent from their actual scheduled work.

Minimal repair: add R5.55/H11 evidence dependencies to G205/G208, explicitly keep those follow-up/deletion P predicates owed at R5.53 closure, and state contacts **and raw observations**, actual dates/withdrawal treatment, with consented non-identifying summaries and scientific receipt retention treated according to the original separate policies. Do not add sessions, automation or a new follow-up quota. Note that deletion completes after the day-90 review rather than at that review.

### COV-04 — P3 — Method dossier's row count is inconsistent with its exact admission partition

Path: `docs/tickets/173_R5.5__adr-methods.md:35`.

ID: H004 / §9.3 admission ownership.

R5.3 owns the first two amendment rows and R5.4 the native profile row. Of the source's seven rows, four remain for R5.5, whose AC-1 says five method/detector rows. This can create an invented/duplicate admission item.

Minimal repair: say four and identify the four source rows (finite-task efficacy, statistical parity grouping, finite-manifest equivalence, detectors/analysis metadata), keeping all seven exact rows uniquely assigned.

## Limits

No conclusion here establishes source-code feasibility, regression quality, numerical correctness, actual dispatch enforcement, authentic independence, access/rights, live capability or funding. Contracts may legitimately cite the source rather than duplicate it; the concern is only where a generic citation plus incorrect primary-only/optional routing leaves the promised full-predicate evidence unowned or temporally unavailable. The existing no-MET/DF/gate safeguards reduce immediate harm but do not eliminate these planning repairs. Recheck the repaired exact artifacts before recording the independent partition review as clean.
