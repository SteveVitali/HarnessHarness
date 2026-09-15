<!--
  Template: docs/build/BUILD_INDEX.md (BM-INDEX-01). Header written by build-memory init;
  one row appended by implement-spec at each ticket close (D4) — never reconstructed later.
  evidence points at runs/<ID>.md#evidence or pr/<ID>.md.
-->
# BUILD_INDEX — one row per landed chain row

| seq | ticket | kind | branch | PR | base | landed | ADRs | deferrals opened → closed | live verification (run / fixture-only / n-a / gate-pending) | evidence |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | S0.1 | ticket | svitali/harnessharness-s0.1 | [#2](https://github.com/SteveVitali/MetaHarness/pull/2) | svitali/harnessharness | 2026-09-15 (PR open) | ADR-0217, ADR-0218, ADR-0219, ADR-0220 | DF-S0.1-1, DF-S0.1-2 → (none closed) | run (offline/hermetic: cargo test, drift check, boundary spike) | runs/S0.1.md#evidence-report · pr/S0.1.md |
| 2 | S0.2 | ticket | svitali/harnessharness-s0.2 | (PR pending push) | svitali/harnessharness-s0.1 | 2026-09-15 (PR open) | ADR-0221, ADR-0222 | DF-S0.2-1 → (none closed) | run (offline/hermetic: cargo test 84 green, live headless CLI run, jsonl==events byte-equality, leak scan) | runs/S0.2.md#evidence-report · pr/S0.2.md |
