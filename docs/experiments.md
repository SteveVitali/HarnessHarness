# Offline example families

Four versioned `example-card/1` records under `scripts/examples/`,
executed by the checked runner (`hharness::examples`) on scratch
stores. Every card carries its own prerequisites, pins, costs,
limitations and fixture label — a green card never upgrades fixture
evidence to a task, live or study result. The static check is
`scripts/check-examples.sh` (both modes); the bounded full-offline lane
is `scripts/run-examples.sh`; live legs stay `skipped` — ADR-0363 owns
them.

## EX-01 — setup, inspect, replay, verify

- **Job:** run the scripted quickstart, attach by returned run id,
  resolve the six trace excerpts, export, verify, replay.
- **Version/pins:** `scripts/examples/ex-01.json` —
  `clients/ts/fixture-definition.json` (sha-pinned).
- **Cost/network:** seconds; `network:false`, `$0`, no credentials.
- **Input → commands:** `cargo test -p hharness --test r5_29 ex01`.
- **Semantic checks:** negotiated, run_succeeded, six excerpt legs,
  export_verify_r0, replay_admission.
- **Failure route / next:** stale pin or tampered copy refuses typed;
  failures → [troubleshooting](getting-started.md#troubleshooting);
  next → EX-02 or [inspect a run](inspect-a-run.md).

## EX-02 — the informative two-task lesson

- **Job:** two tasks × two arms × five repetitions = 20 subjects —
  register→expand→open→next→claim→launch→settle→close for real,
  persisted and recomputed analysis.
- **Version/pins:** `scripts/examples/ex-02.json` +
  `ex-02-fixture.json` (sha-pinned).
- **Cost/network:** seconds; `network:false`, `$0`.
- **Input → commands:** `cargo test -p hharness --test r5_29 ex02`.
- **Semantic checks:** subjects_20, cells_pinned, informative_contrast,
  report_identity, recipient_recompute, exploratory_label.
- **Failure route / next:** wrong count/mean or a constant fixture
  fails the lesson; producer fallback is `Withheld`, never silent.

## EX-03 — useful task, sealed submission, oracle, bounded edit

- **Job:** a new useful-task drive with a sealed submission and the
  unchanged deterministic oracle, the bounded cap-field edit, plus the
  EX-01 receipt join.
- **Version/pins:** `scripts/examples/ex-03.json` — the executed
  [hh-s1-useful/1 packet](build/reports/2026-11-04-r5.43-s1-useful.json)
  and the pinned definition.
- **Cost/network:** seconds; `network:false`, `$0`.
- **Input → commands:** `cargo test -p hharness --test r5_45 ex03`.
- **Semantic checks:** packet, sealed_submission, oracle_chain
  (wrong output → task fail; unavailable oracle → typed failure),
  field_edit (validate→compile→rerun, adaptation not causal),
  edit_refusal, receipt.
- **Failure route / next:** a fake packet leg or missing recipient
  bytes fails — see the negative controls on the card.

## EX-04 — confined effect matrix, exhaustion, refusals

- **Job:** the shared confined sink/effect §5 matrix — allow/Kp4, deny,
  pause/respond/resume, exhaustion — verified on the retained executed
  packet plus deterministic exhaustion and typed refusal legs offline.
- **Version/pins:** `scripts/examples/ex-04.json` — the executed
  [hh-ops-live/1 packet](build/reports/2026-11-04-r5.44-ops-live-second.json)
  and the pinned definition.
- **Cost/network:** seconds; `network:false`, `$0`.
- **Input → commands:** `cargo test -p hharness --test r5_45 ex04`.
- **Semantic checks:** packet, matrix (P=A=R=T=3, V=0; honest
  `undeterminable` probes verbatim; `discharged_predicates`),
  exhaustion (`control.budget.exceeded` → `decision{stop}`), and the
  typed refusals suspend/resume/respond/amend — input-required is not approval;
  scratch is not sandbox.
- **Failure route / next:** an untyped packet verdict or mutated
  discharged predicate fails `packet`/`matrix`.

All four families: fixture-verified posture only — the executed packets
retain their own evidence class verbatim; nothing here is useful-task,
live, parity or study evidence.
