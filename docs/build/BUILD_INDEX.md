<!--
  Template: docs/build/BUILD_INDEX.md (BM-INDEX-01). Header written by build-memory init;
  one row appended by implement-spec at each ticket close (D4) — never reconstructed later.
  evidence points at runs/<ID>.md#evidence or pr/<ID>.md.
-->
# BUILD_INDEX — one row per landed chain row

| seq | ticket | kind | branch | PR | base | landed | ADRs | deferrals opened → closed | live verification (run / fixture-only / n-a / gate-pending) | evidence |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | S0.1 | ticket | svitali/harnessharness-s0.1 | [#2](https://github.com/SteveVitali/MetaHarness/pull/2) | svitali/harnessharness | 2026-09-15 (PR open) | ADR-0217, ADR-0218, ADR-0219, ADR-0220 | DF-S0.1-1, DF-S0.1-2 → (none closed) | run (offline/hermetic: cargo test, drift check, boundary spike) | runs/S0.1.md#evidence-report · pr/S0.1.md |
| 2 | S0.2 | ticket | svitali/harnessharness-s0.2 | [#3](https://github.com/SteveVitali/MetaHarness/pull/3) | svitali/harnessharness-s0.1 | 2026-09-15 (PR open) | ADR-0221, ADR-0222 | DF-S0.2-1 → (none closed) | run (offline/hermetic: cargo test 84 green, live headless CLI run, jsonl==events byte-equality, leak scan) | runs/S0.2.md#evidence-report · pr/S0.2.md |
| 3 | S0.3 | ticket | svitali/harnessharness-s0.3 | [#4](https://github.com/SteveVitali/MetaHarness/pull/4) | svitali/harnessharness-s0.2 | 2026-09-15 (PR open) | ADR-0223, ADR-0224 | DF-S0.3-1, DF-S0.3-2, DF-S0.3-3 → DF-S0.1-1, DF-S0.2-1 | run (offline/hermetic: N=5 S1/S2 spikes, gates G1–G4, cargo test 84 green) | runs/S0.3.md#evidence-report · pr/S0.3.md |
| 3a | S0.3b | ticket | svitali/harnessharness-s0.3b | [#5](https://github.com/SteveVitali/MetaHarness/pull/5) | svitali/harnessharness-s0.3 | 2026-09-15 (PR open) | ADR-0225, ADR-0226 | (none opened) → DF-S0.3-1, DF-S0.3-3 (machine cells; R2 residual OPEN) | run (online: N=5 cross-candidate spikes, G1 byte-identity, official-SDK MCP/ACP, E5b/E5c splits; gate suite exit 0) | runs/S0.3b.md#evidence-report · pr/S0.3b.md |
| 5 | S1.1 | ticket | svitali/harnessharness-s1.1 | [#6](https://github.com/SteveVitali/MetaHarness/pull/6) | svitali/harnessharness-s0.3b | 2026-09-15 (PR open) | ADR-0227, ADR-0228 | DF-S1.1-1 → (none closed) | run (offline/hermetic: cargo test 143 green, spec-DAG check clean, fmt/clippy/drift) | runs/S1.1.md#evidence-report · pr/S1.1.md |
| 6 | S1.2 | ticket | svitali/harnessharness-s1.2 | [#7](https://github.com/SteveVitali/MetaHarness/pull/7) | svitali/harnessharness-s1.1 | 2026-09-15 (PR open) | ADR-0229 | DF-S1.2-1, DF-S1.2-2 → DF-S0.1-2 | run (offline/hermetic: cargo test 204 green, AC-R-2.12.1-{1-5,12}, drift regenerated idp/1 pin, fmt/clippy) | runs/S1.2.md#evidence-report · pr/S1.2.md |
| 7 | S1.3 | ticket | svitali/harnessharness-s1.3 | [#8](https://github.com/SteveVitali/MetaHarness/pull/8) | svitali/harnessharness-s1.2 | 2026-09-15 (PR open) | ADR-0230 | DF-S1.3-1, DF-S1.3-2, DF-S1.3-3 → DF-S1.2-1 | n/a (offline/hermetic: cargo test 254 green, AC-R-2.1.5-{1,2} executable, fmt/clippy/drift clean) | runs/S1.3.md#evidence-report · pr/S1.3.md |
