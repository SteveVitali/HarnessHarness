# DF-CAP5.1-1 — Re-execute the S1 useful-task bounded scope on repaired machinery

Discharges `docs/tickets/DEFERRALS.md` row **DF-CAP5.1-1** — the failed
S1 useful-task residual. R5.43's `hh-s1-useful/1` emitted
`disposition: failed` (`dispatch: unknown:executor_error` from the
`exec_backend_hello_clobber` machinery defect — `hh-helper`'s
`Hello{policy}` clobbered the spawned `--backend podman`). The
machinery was repaired and live-verified at R5.44 (ops-live executed
24/24 on real podman; both carried predicates discharged). This run is
the **freshly operator-authorized re-execution** ("approved, proceed"
verbatim — the original R5.43 bounded attempt was consumed) of the
unchanged frozen two-subject scope on the repaired machinery: same
binds (subject-1 → `a-hello`, subject-2 → `a-sum`), unchanged TB2
tasks/oracle, verified podman backend, no re-picked inputs, USD $0.

## Honest disposition — `executed` + `marked`

`docs/build/reports/2026-11-05-df-cap5.1-s1-useful.json` —
`hh-s1-useful/1` `disposition: executed`, all seven legs `verified`,
`useful_acceptance: marked`, `packet_id 7bbd15a2…c13a82`. Both
subjects: `dispatch: observed:applied`, `executor_backend: container`
(the repaired machinery's verbatim answers — R5.43 recorded
`seatbelt`), artifacts collected (`716457c4…` = `{"answer":"hello"}`,
`af67737a…` = `{"answer":5}`), `probe: ok:undeterminable` verbatim
(the repaired verb — R5.43 recorded `error:blob: ENOENT`), sealed,
3 observer intervals, `setup_retries: 0`.

**The independent Decided artifacts** (the deferral's named
deliverable): `docs/build/reports/2026-11-05-df-cap5.1-s1-decided-artifacts.json`
— `hh-s1-decided-artifacts/1`, both subject artifacts independently
graded `decided` `reward_ppm 1000000` through the unchanged
`hh-bench-adapter` oracle (`OracleRunner`; `invoked` durable before
`verdict`; `charged_to: instrument`), with verbatim `payload_hex`,
`submission_ref`, `attempt_ref`, `verdict_ref`, `oracle_digest
77b1b9da…`. The packet's `oracle_chain` leg records
`subject_a: decided:1000000`, `subject_b: decided:1000000` verbatim.

Retained for honesty: `reports/2026-11-05-df-cap5.1-s1-useful-first.json`
— the first emission (`executed` + `withheld`), the manifestation of
the instrument-fold defect repaired below (R5.44 `-first`/`-second`
precedent — all emissions retained, never retry-until-green).

## Instrument-fold repair (records-in; the `hh-s1-useful/1` codec untouched)

`tests/r5_43.rs` — the re-run surfaced a third fold defect (same class
as R5.44's emission-1 instrument bugs): `grade_attempt` labeled
`OracleOutcome.subject_id` with the subject **run id**, while
`useful.rs::assemble`'s `useful_acceptance` fold correctly identifies
subject attempts by the frozen **bind id** (`starts_with("subject_")`)
— so a genuinely `Decided 1e6` subject artifact could never record
`marked`. Repaired: `subject_id` carries the bind id (run coordinate
stays in `spec.subject_run_id`); a fails-without assertion pins it;
the `oracle_chain` leg records per-attempt verbatim outcomes;
`HH_S1_EMIT_DECIDED` emits the companion decided-artifact record from
the same real drive. No machinery, task, oracle, or input changes;
zero changed lines outside `tests/r5_43.rs` (~90 added).

## Verification

- `cargo test -p hharness-env` — all suites green, 0 failures (r5_43
  6/6, r5_44 4/4 live 24-trial podman matrix, helper_live 15/15,
  r5_42a 10/10, r5_46 11/11 podman subjects).
- `cargo fmt --all --check` — clean.
- Guard `check-supplement-guard.sh` — PASS, route
  `HOLD — active round fully landed` (round-5 manifest complete;
  deferral work proceeds under the operator's explicit authorization,
  recorded — a HOLD route is RETURN PASS, not a dispatch refusal).
- bm tools + external `check-build-memory.sh` — see run ledger.

Requirement ids: REQ-A-003, REQ-A-104 (+ REQ-D-201 contributing legs)
— the bounded S1 execution slice as **executed** evidence; full-target
R-layer and downstream predicates stay owed per the coverage register.
`admission: bounded_s1_execution` — no S4/study/R2/R3/efficacy claim.

## Files

- `docs/build/reports/2026-11-05-df-cap5.1-s1-useful.json` (new)
- `docs/build/reports/2026-11-05-df-cap5.1-s1-decided-artifacts.json` (new)
- `docs/build/reports/2026-11-05-df-cap5.1-s1-useful-first.json` (new, retained)
- `crates/hh-env/tests/r5_43.rs` (instrument-fold repair + decided emit)
- `docs/build/runs/DF-CAP5.1-1.md` (new) · `docs/build/pr/DF-CAP5.1-1.md` (new)
- `docs/tickets/DEFERRALS.md` (DF-CAP5.1-1 OPEN → DONE, dated disposition)
- `docs/build/LEDGER.md` (PHASE LOG entry)
- `docs/build/BUILD_INDEX.md` (deferral-discharge row)
