# Run ledger — DF-CAP5.1-1 (Re-execute the S1 useful-task bounded scope on repaired machinery)

- **Harness:** devin-desktop/swe-2-high/subagent
- **Observed runtime identity:** Devin Desktop subagent context (parent: devin-desktop coordinator); declared tier swe-2-high; exact model id is not exposed to this session — no independent self-report available, recorded honestly. Nesting depth 1: no subagent spawning this run.
- **Skills:** agent-skills @ e981ec5 — build-memory + orchestrate-build (ci-boundary)
- **Started:** 2026-11-05T14:30Z
- **Closed:** 2026-11-05
- **Spec:** `docs/tickets/DEFERRALS.md` row DF-CAP5.1-1 (verbatim scope: "An executed `hh-s1-useful/1` packet on the repaired machinery with an independent Decided artifact"); predecessor records `docs/build/runs/R5.43.md` + `reports/2026-11-04-r5.43-s1-useful.json` (the failed packet) + `docs/tickets/217_R5.43__useful.md` (the frozen contract) + `docs/build/runs/R5.44.md` (the machinery repair, live-verified)
- **Authorization:** fresh operator admission for the re-run, verbatim "approved, proceed" (the original R5.43 bounded attempt was consumed; this is a named re-execution admission — the deferral's `unblocked by` column's second disjunct, exercised ahead of GATE-G17)
- **Base:** dec5b96e (codex/harnessharness-guard-done tip — the current chain tip; LEDGER `chainTip` field reads codex/harnessharness-evidence-doc5.2, one commit behind; guard-done is doc5.2 + the terminal-DONE guard repair, PR #279 open)
- **Branch:** codex/harnessharness-evidence-df-cap5-1 → PR targets codex/harnessharness-guard-done
- **Config:** committed build-memory mode (root docs/build); operator-authorized re-execution inside the unchanged frozen two-subject scope; USD $0.00; scratch stores under `target/` only
- **ci:** (pending)

## Gate status

No new gate. This is deferral work, not a manifest row: the retained guard (`bash docs/build/tools/check-supplement-guard.sh`, AGENT_SKILLS_ROOT set) returns **PASS, route `HOLD — active round fully landed`** — the manifest chain is complete (nextTicket DONE), so no `DISPATCH — <id>` route exists for this work. Recorded honestly; proceeding under the operator's explicit verbatim authorization ("approved, proceed") rather than a guard dispatch. The governing gates stay R5.43's: GATE-G11 PASSED 2026-11-04T01:13:12Z (the frozen two-subject S1 scope — verbatim bounds consumed, never widened) and HUMAN-H8 COMPLETE 2026-11-03T23:17:50Z (backend/inputs/rights supply).

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-05T14:30Z — `DF-R5.1-1 | OPEN` (intentional; evidence-dependency only), `DF-H9-1 | OPEN` (withheld evidence-access — not this work), `DF-CAP5.1-1 | OPEN` ← **this run's target**.

## Backend state at SETUP (verified 2026-11-05T14:35Z)

podman 6.1.0 reachable (`podman info` ok; machine running); `docker.io/library/alpine:3.20` present locally (no pull); `target/debug/hh-helper` + `target/debug/hh-bench-adapter` built from this tree (`cargo build` finished no-op — binaries current with the R5.44 repairs already in the chain); `/bin/true` ABSENT (`/usr/bin/true` exists — the `probe_verb_enoent` mechanism, now inert post-repair); seatbelt available; one stale `hh-env-env-000007-e6f6e474` container from an earlier run swept before the drive (recorded, never a hidden retry).

## Scope held (verbatim from the R5.43 contract — unchanged, nothing re-picked)

- Two fixed subjects on the unchanged TB2 tasks: **subject-1 → `a-hello`** (pin `5bf206ca…`), **subject-2 → `a-sum`** (pin `e30c65b9…`) — the same frozen binds; sanctioned submission bytes `{"answer":"hello"}` / `{"answer":5}` (the fixture's expected bytes — no new inputs picked).
- Backend podman only (real handshake, `/work` mount, net none, `$HOME`-only workspaces); `hh-bench-adapter` unchanged oracle through `OracleRunner` (`oracle_digest` = sha256 of the executed binary `77b1b9da…`); three controls; surviving observer (3 intervals + sentinel); bind-time `probe()` verbatim; `seal_workspace`; ≤1 setup retry bound (0 consumed); the declared §9.2 worksheet through `build_quote`; all attempts retained.

## What this run repaired (records-in instrument fold — never machinery, never a re-pick)

The re-run's first emission surfaced a third instrument-fold defect in `tests/r5_43.rs` (same defect class as R5.44's emission-1 fold bugs — the machinery was fine, the fold was mislabeled): `grade_attempt` set `OracleOutcome.subject_id` to the subject **run id** (`subject_run.to_string()`), but `useful.rs::assemble`'s `useful_acceptance` fold — correctly — identifies subject attempts by the frozen **bind id** (`o.subject_id.starts_with("subject_")`). So even with both subjects executing `observed:applied` and independently grading `Decided 1e6`, `useful_acceptance` structurally folded `withheld` — the "independent Decided artifact" could never be recorded. (`OracleOutcome.subject_id`'s own doc — "The subject this attempt grades" — intends the bind id; the run coordinate stays bound in `spec.subject_run_id`, untouched.)

Repair (test-side only; the `hh-s1-useful/1` record codec is unchanged):

- `grade_attempt` takes `subject_id` (the bind id) explicitly; the four control call sites pass the instrument run id as before (controls never enter the subject fold); a fails-without assertion (`o.subject_id.starts_with("subject_")` over every subject outcome) pins the repair — reverted, it fails.
- The `oracle_chain` leg now records each attempt's verbatim outcome in facts (`subject_a: decided:1000000`, `subject_b: decided:1000000`) — the packet's own evidence of the independent Decided artifacts; leg names stay the closed seven-member `LEGS` (facts are the record's evidence channel).
- `HH_S1_EMIT_DECIDED` emits the companion `hh-s1-decided-artifacts/1` record — the verbatim artifact bytes (`payload_hex`), `artifact_sha256`, `submission_ref`, `outcome`, `reward_ppm`, `attempt_ref`, `verdict_ref`, `oracle_digest` per subject — the deferral's named "independent Decided artifact" as a literal committed record, minted by the same real drive, never hand-written.

## Executions (all retained — never retry-until-green; each a full real drive)

1. **First emission** (`reports/2026-11-05-df-cap5.1-s1-useful-first.json`): `disposition: executed`, all seven legs verified — but `useful_acceptance: withheld{no_independent_correct_artifact}` under the fold mislabel (subjects genuinely `observed:applied` + `executor_backend: container` + artifacts collected + oracle attempts run; the fold could not see the bind ids). Retained verbatim as the fold defect's manifestation — the R5.44 `-first.json` precedent.
2. **Committed emission** (`reports/2026-11-05-df-cap5.1-s1-useful.json`): **`disposition: executed`, `useful_acceptance: marked`** — every leg verified; subject_a `observed:applied`/`executor_backend: container`/artifact `716457c4…` (= sha256 `{"answer":"hello"}`)/sealed/3 intervals/`ok:undeterminable` probe verbatim; subject_b identical shape (`af67737a…` = `{"answer":5}`); `oracle_chain` verified with `subject_a: decided:1000000`, `subject_b: decided:1000000` facts; controls/observer_probe/evidence_join/worksheet verified; attempts `[subject_a-c0, subject_b-c0, oracle-subject_a, oracle-subject_b, ctl-correct, ctl-wrong, ctl-broken]`; `packet_id 7bbd15a2a53101dfef1a5ea3524bf670dc446a2b90a05c49e7818c28aec13a82`. The emission is byte-deterministic over honest outcomes (content-digest `packet_id` — a verification emit reproduced identical bytes).
3. **Companion** (`reports/2026-11-05-df-cap5.1-s1-decided-artifacts.json`): `hh-s1-decided-artifacts/1` — both subject artifacts `decided` `reward_ppm 1000000` with verbatim `payload_hex` (`7b22616e73776572223a2268656c6c6f227d` = `{"answer":"hello"}`; `7b22616e73776572223a357d` = `{"answer":5}`), `submission_ref`, `attempt_ref`, `verdict_ref`, `oracle_digest 77b1b9da…`.

Honest-field notes: `executor_backend: container` and `probe: ok:undeterminable` are the repaired machinery's verbatim answers (the R5.43 packet recorded `seatbelt`/`error:blob: ENOENT`). `carried_predicates: [probe_verb_enoent, exec_backend_hello_clobber]` is the record's closed field — the names carried *into* the S1 admission, retained verbatim per `hh-s1-useful/1`; their discharge evidence is the verbatim facts above + R5.44's `discharged_predicates` (the record format has no discharged field — amending it is an ADR, not this run).

## Progress

- 2026-11-05T14:30Z — deferrals read; branch cut from `dec5b96e` (guard-done tip); guard → PASS/HOLD (round landed — recorded, operator authorization governs); backend verified.
- 2026-11-05 — first emit run: executed + withheld ⇒ fold mislabel diagnosed (run-id vs bind-id in `OracleOutcome.subject_id`).
- 2026-11-05 — instrument-fold repair + fails-without assertion + verbatim oracle-chain facts + `HH_S1_EMIT_DECIDED` companion emit; `cargo test -p hharness-env --test r5_43 --no-run` clean.
- 2026-11-05 — full `cargo test -p hharness-env` green all suites (r5_43 6/6, r5_44 4/4 live 24-trial matrix, helper_live 15/15, r5_42a 10/10, r5_46 podman subjects); `cargo fmt --all --check` clean.
- 2026-11-05 — canonical emit to `docs/build/reports/`; stale container swept; ledger/DEFERRALS closeout.

## Evidence log

- `docs/build/reports/2026-11-05-df-cap5.1-s1-useful.json` — the executed `hh-s1-useful/1` packet: `disposition: executed`, `useful_acceptance: marked`, `packet_id 7bbd15a2…c13a82`, all seven legs verified, both subjects `observed:applied` + `decided:1000000`.
- `docs/build/reports/2026-11-05-df-cap5.1-s1-decided-artifacts.json` — `hh-s1-decided-artifacts/1`: both subject artifacts verbatim (`payload_hex`, sha256s, `submission_ref`, `attempt_ref`, `verdict_ref`, `decided`/`1000000`, `oracle_digest 77b1b9da…`) — the independent Decided artifacts.
- `docs/build/reports/2026-11-05-df-cap5.1-s1-useful-first.json` — retained first emission (`executed` + `withheld` under the fold mislabel — honest, never rewritten).
- Predecessors: `reports/2026-11-04-r5.43-s1-useful.json` (`failed` — carried), `reports/2026-11-04-r5.44-ops-live-second.json` (`executed` + `discharged_predicates`).

## Verification

- `cargo fmt --all --check` — clean (exit 0).
- `cargo test -p hharness-env` — **all suites green, 0 failures** (incl. live podman legs: r5_43 6/6 — `ac2_subject_cells_execute_on_podman` verifies `observed:applied`/`container`/artifact verbatim; `ac2_ac3_controls_and_oracle_chain_real` asserts `marked ⇒ Executed` + the bind-id regression assertion; `ac2_ac3_executed_packet_is_honest` emits both records — r5_44 4/4 live 24-trial matrix 27.8 s, helper_live 15/15, r5_42a 10/10, r5_46 11/11 podman subjects).
- `cargo build -p hharness-helper --bin hh-helper -p hharness-bench --bin hh-bench-adapter` — no-op, binaries current with this tree.
- `bash docs/build/tools/check-supplement-guard.sh` — PASS, route `HOLD — active round fully landed` (pre-boundary; post-boundary re-run recorded at close).
- `bash scripts/deferrals-open.sh` — DF-CAP5.1-1 flips out of the OPEN list at close (verified post-edit).
- bm tools (`bash scripts/check-bm-tools.sh`) + external `check-build-memory.sh .`/`--staged` — recorded at close.
- Container hygiene: `hh-env-*` sweep before the drive; one post-emit residue `hh-env-env-000007-3fd34a56` removed (`podman rm -f`), zero remaining.

## Deferral-row disposition

**DF-CAP5.1-1 → DONE** — the named deliverable landed: an executed `hh-s1-useful/1` packet on the repaired machinery (`disposition: executed`, `useful_acceptance: marked`, all legs verified) **with an independent Decided artifact** — in fact two: both subjects independently graded `Decided 1e6` through the unchanged `hh-bench-adapter` oracle (`invoked` durable before `verdict`, `charged_to: instrument`), recorded verbatim in the packet's `oracle_chain` facts and the companion `hh-s1-decided-artifacts/1` record. The residual machinery predicate never recurred (`executor_backend: container`, `probe: ok:undeterminable` — the repaired answers, verbatim). REQ-A-003/REQ-A-104 (+ REQ-D-201 contributing legs) earn their bounded S1 execution slice as executed evidence; full-target R-layer and downstream predicates stay owed per the coverage register (no widening claim — `admission: bounded_s1_execution`).
