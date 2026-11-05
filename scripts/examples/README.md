# scripts/examples — versioned offline example cards

Four committed `example-card/1` records (R5.29 + R5.45; spec §7.2). The cards are
the versioned reader-facing records; the checked runner and semantic
controls live in the owning composition crate (`hharness::examples`),
executed by `crates/hharness/tests/r5_29.rs`.

| Card | Family | Run |
|---|---|---|
| `ex-01.json` | setup/inspect/replay/receipt | `cargo test -p hharness --test r5_29 ex01` |
| `ex-02.json` + `ex-02-fixture.json` | offline Lab lesson (2 tasks × 2 arms × 5 reps = 20) | `cargo test -p hharness --test r5_29 ex02` |
| `ex-03.json` | useful task, sealed submission, oracle + bounded cap edit (+ EX-01 receipt) | `cargo test -p hharness --test r5_45 ex03` |
| `ex-04.json` | confined effect matrix + exhaustion + typed refusals | `cargo test -p hharness --test r5_45 ex04` |

- **Offline only:** scratch stores, the scripted `EmbedModel` lane, no
  network/provider/credentials, $0. Fixture labels stay on every report.
- **Checks (R5.30):** `scripts/check-examples.sh` validates the cards,
  their pins and the displayed docs block in the working tree;
  `scripts/check-examples.sh --head` runs the same check on committed
  HEAD in a clean clone — uncommitted edits are never the tested HEAD.
  `scripts/run-examples.sh` is the bounded full-offline lane; live legs
  record `skipped(…)` — ADR-0363 owns them, never a pass.
- **Costs (separate):** build = one `cargo test -p hharness` compile of
  the existing tree (no added dependencies); runtime = seconds.
- **Limitations:** fixture-verified only — never S1/S3, useful-task,
  live, parity or study evidence. The verbatim README quickstart leg is
  `scripts/check-quickstart.sh` (reads committed HEAD), not this runner.
- EX-03/EX-04 pin the *executed* R5.43/R5.44 packets and run the
  admitted deterministic legs — prerecorded inspection is never
  execution, and nothing here executes tasks, providers or operations
  beyond the retained executed records.
