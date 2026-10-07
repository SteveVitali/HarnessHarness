# HarnessHarness — release notes (DRAFT)

> **Draft for the operator to edit — not a published note.** Derived
> from `docs/build/BUILD_INDEX.md` (one landed row = one line; PR links
> are canonical `SteveVitali/HarnessHarness` URLs — the MetaHarness
> remote name redirects). Generated 2026-10-06 by REC.3.

## Headline

The HarnessHarness canonical-spec build: a complete agent-harness
instrument — HIR/1 compilation, durable event ledger, reference-monitor
security kernel, sandboxed tool execution, evaluation/lab plane,
hosting ABI for external harnesses, and the governed self-evolution
pipeline — delivered as a 99-ticket stacked chain (PRs #2–#102), four
gates passed (G1, G2, G3, GATE-ACCEPT with a signed
accepted-deviations set), capstone suite **280 result blocks / 3028
tests / 0 failures** at `82ef6b0`.

### Stage 0 — toolchain, baseline, measurement spikes

- **S0.1** — toolchain codegen ([#2](https://github.com/SteveVitali/HarnessHarness/pull/2))
- **S0.2** — hand authored baseline ([#3](https://github.com/SteveVitali/HarnessHarness/pull/3))
- **S0.3** — stage0 spikes ([#4](https://github.com/SteveVitali/HarnessHarness/pull/4))
- **S0.3b** — cross candidate online spike ([#5](https://github.com/SteveVitali/HarnessHarness/pull/5))

### Stage 1 — kernel slices (C0)

- **S1.1** — ontology formal model ([#6](https://github.com/SteveVitali/HarnessHarness/pull/6))
- **S1.2** — identity versioning ([#7](https://github.com/SteveVitali/HarnessHarness/pull/7))
- **S1.3** — provenance authority ([#8](https://github.com/SteveVitali/HarnessHarness/pull/8))
- **S1.4** — harness ir dialect ([#9](https://github.com/SteveVitali/HarnessHarness/pull/9))
- **S1.5** — run ledger ([#10](https://github.com/SteveVitali/HarnessHarness/pull/10))
- **S1.6** — budgets accounting ([#11](https://github.com/SteveVitali/HarnessHarness/pull/11))
- **S1.7** — effect model durability slice ([#12](https://github.com/SteveVitali/HarnessHarness/pull/12))
- **S1.8** — registry store slice ([#13](https://github.com/SteveVitali/HarnessHarness/pull/13))
- **S1.9** — config composition ([#14](https://github.com/SteveVitali/HarnessHarness/pull/14))
- **S1.10** — compilation model ([#15](https://github.com/SteveVitali/HarnessHarness/pull/15))
- **S1.11** — reference monitor ([#16](https://github.com/SteveVitali/HarnessHarness/pull/16))
- **S1.12** — egress network ([#17](https://github.com/SteveVitali/HarnessHarness/pull/17))
- **S1.13** — credential broker ([#18](https://github.com/SteveVitali/HarnessHarness/pull/18))
- **S1.14** — telemetry ([#19](https://github.com/SteveVitali/HarnessHarness/pull/19))
- **S1.15** — audit trail ([#20](https://github.com/SteveVitali/HarnessHarness/pull/20))
- **S1.16** — environment and sandboxed exec ([#21](https://github.com/SteveVitali/HarnessHarness/pull/21))
- **S1.17** — tool registry and exposure ([#22](https://github.com/SteveVitali/HarnessHarness/pull/22))
- **S1.18** — model gateway and profile slices ([#23](https://github.com/SteveVitali/HarnessHarness/pull/23))
- **S1.19** — context builder and memory slices ([#24](https://github.com/SteveVitali/HarnessHarness/pull/24))
- **S1.20** — control strategy and envelope ([#25](https://github.com/SteveVitali/HarnessHarness/pull/25))
- **S1.21** — verification substrate ([#26](https://github.com/SteveVitali/HarnessHarness/pull/26))
- **S1.22** — eval framework ([#27](https://github.com/SteveVitali/HarnessHarness/pull/27))
- **S1.23** — approvals and extension trust slices ([#28](https://github.com/SteveVitali/HarnessHarness/pull/28))
- **S1.24** — measurement and lab schemas ([#29](https://github.com/SteveVitali/HarnessHarness/pull/29))
- **S1.25** — sdk embed contract ([#30](https://github.com/SteveVitali/HarnessHarness/pull/30))
- **S1.26** — attended cli ([#31](https://github.com/SteveVitali/HarnessHarness/pull/31))
- **S1.27** — extensibility plugin architecture ([#32](https://github.com/SteveVitali/HarnessHarness/pull/32))

### Stage 2 — live lane, containment, IFC, durability

- **S2.1** — helper and container ([#33](https://github.com/SteveVitali/HarnessHarness/pull/33))
- **S2.2** — variant host ([#34](https://github.com/SteveVitali/HarnessHarness/pull/34))
- **S2.3** — durable execution ([#35](https://github.com/SteveVitali/HarnessHarness/pull/35))
- **S2.4** — egress mediation and credentials ([#36](https://github.com/SteveVitali/HarnessHarness/pull/36))
- **S2.5** — audit checkpoints ([#37](https://github.com/SteveVitali/HarnessHarness/pull/37))
- **S2.6** — approvals and hooks ([#38](https://github.com/SteveVitali/HarnessHarness/pull/38))
- **S2.7** — ifc taint labeling ([#39](https://github.com/SteveVitali/HarnessHarness/pull/39))
- **S2.8** — context label and memory ([#40](https://github.com/SteveVitali/HarnessHarness/pull/40))
- **S2.9** — branch model ([#41](https://github.com/SteveVitali/HarnessHarness/pull/41))
- **S2.10** — tool scaling ([#42](https://github.com/SteveVitali/HarnessHarness/pull/42))
- **S2.11** — steerable control and nudges ([#43](https://github.com/SteveVitali/HarnessHarness/pull/43))
- **S2.12** — attended cli item and registry enforcement ([#44](https://github.com/SteveVitali/HarnessHarness/pull/44))

### Stage 3 — evaluation-first: lab, bundle, compare, security battery

- **S3.1** — lab embed groups and bundle ([#45](https://github.com/SteveVitali/HarnessHarness/pull/45))
- **S3.2** — compiler full ([#46](https://github.com/SteveVitali/HarnessHarness/pull/46))
- **S3.3** — metric catalogue and compare ([#47](https://github.com/SteveVitali/HarnessHarness/pull/47))
- **S3.4a** — experiment engine ([#49](https://github.com/SteveVitali/HarnessHarness/pull/49))
- **S3.4b** — results store ([#50](https://github.com/SteveVitali/HarnessHarness/pull/50))
- **S3.4c** — estimator kernel ([#51](https://github.com/SteveVitali/HarnessHarness/pull/51))
- **S3.4d** — hosting abi schemas ([#52](https://github.com/SteveVitali/HarnessHarness/pull/52))
- **S3.5** — assembly service ([#53](https://github.com/SteveVitali/HarnessHarness/pull/53))
- **S3.6** — replay fault and counterfactual ([#54](https://github.com/SteveVitali/HarnessHarness/pull/54))
- **S3.7** — model plane eval ([#55](https://github.com/SteveVitali/HarnessHarness/pull/55))
- **S3.8** — context memory eval ([#56](https://github.com/SteveVitali/HarnessHarness/pull/56))
- **S3.9** — tool protocol edges ([#57](https://github.com/SteveVitali/HarnessHarness/pull/57))
- **S3.10** — control verification eval ([#58](https://github.com/SteveVitali/HarnessHarness/pull/58))
- **S3.11a** — security eval monitor ifc ([#59](https://github.com/SteveVitali/HarnessHarness/pull/59))
- **S3.11b** — security eval broker audit trust ([#60](https://github.com/SteveVitali/HarnessHarness/pull/60))
- **S3.12** — bundle and plugin conformance ([#61](https://github.com/SteveVitali/HarnessHarness/pull/61))
- **S3.12b** — gate g2 gap closure ([#62](https://github.com/SteveVitali/HarnessHarness/pull/62))

### Stage 4 — hosting ABI, fleet, web, MCP, supply chain

- **S4.1** — registry service ([#63](https://github.com/SteveVitali/HarnessHarness/pull/63))
- **S4.2** — experiment service ([#64](https://github.com/SteveVitali/HarnessHarness/pull/64))
- **S4.3** — analysis engine surfaces ([#65](https://github.com/SteveVitali/HarnessHarness/pull/65))
- **S4.4** — results leaderboard publication ([#66](https://github.com/SteveVitali/HarnessHarness/pull/66))
- **S4.5a** — hosting abi ([#67](https://github.com/SteveVitali/HarnessHarness/pull/67))
- **S4.5b** — protocol edges and acp ([#68](https://github.com/SteveVitali/HarnessHarness/pull/68))
- **S4.6** — subagent orchestrator ([#69](https://github.com/SteveVitali/HarnessHarness/pull/69))
- **S4.7** — value of compute scheduler ([#70](https://github.com/SteveVitali/HarnessHarness/pull/70))
- **S4.8** — coordination ([#71](https://github.com/SteveVitali/HarnessHarness/pull/71))
- **S4.9** — fleet reconciler ([#72](https://github.com/SteveVitali/HarnessHarness/pull/72))
- **S4.10** — read only web instrument ([#73](https://github.com/SteveVitali/HarnessHarness/pull/73))
- **S4.11** — mcp server lab groups ([#74](https://github.com/SteveVitali/HarnessHarness/pull/74))
- **S4.12** — sdk full and serve ([#75](https://github.com/SteveVitali/HarnessHarness/pull/75))
- **S4.13** — hosted lineage and durability ([#76](https://github.com/SteveVitali/HarnessHarness/pull/76))
- **S4.14a** — supply chain trust and third party install ([#77](https://github.com/SteveVitali/HarnessHarness/pull/77))
- **S4.14b** — security enrichments subagent hosted ([#78](https://github.com/SteveVitali/HarnessHarness/pull/78))
- **S4.15** — measurement c1 and hosted ([#79](https://github.com/SteveVitali/HarnessHarness/pull/79))
- **S4.16a** — router c1 ([#80](https://github.com/SteveVitali/HarnessHarness/pull/80))
- **S4.16b** — retrieval and memory c1c2 ([#81](https://github.com/SteveVitali/HarnessHarness/pull/81))
- **S4.16c** — verification c1c2 ([#82](https://github.com/SteveVitali/HarnessHarness/pull/82))

### Stage 5 — C2/C3 depth: router, compaction, orchestration, surfaces

- **S5.1** — profile compiler and router ([#83](https://github.com/SteveVitali/HarnessHarness/pull/83))
- **S5.2** — compaction and tool surfaces ([#84](https://github.com/SteveVitali/HarnessHarness/pull/84))
- **S5.3** — analysis c2 ([#85](https://github.com/SteveVitali/HarnessHarness/pull/85))
- **S5.4** — designed ablation and live debt ([#86](https://github.com/SteveVitali/HarnessHarness/pull/86))
- **S5.5** — orchestration c3 depth ([#87](https://github.com/SteveVitali/HarnessHarness/pull/87))
- **S5.6** — fleet adapters ([#88](https://github.com/SteveVitali/HarnessHarness/pull/88))
- **S5.7** — web surface editing ([#89](https://github.com/SteveVitali/HarnessHarness/pull/89))
- **S5.8** — surface ecosystem and compat ([#90](https://github.com/SteveVitali/HarnessHarness/pull/90))

### Stage 6 — governed evolution pipeline

- **S6.1a** — evolution pipeline recipe ([#91](https://github.com/SteveVitali/HarnessHarness/pull/91))
- **S6.1b** — assumption debt manager ([#92](https://github.com/SteveVitali/HarnessHarness/pull/92))
- **S6.2** — automated family one class ([#93](https://github.com/SteveVitali/HarnessHarness/pull/93))
- **S6.3a** — multi family code search ([#94](https://github.com/SteveVitali/HarnessHarness/pull/94))
- **S6.3b** — causal attribution and judge integrity ([#95](https://github.com/SteveVitali/HarnessHarness/pull/95))
- **S6.4** — co evolution interface ([#96](https://github.com/SteveVitali/HarnessHarness/pull/96))

### Capstone & reconciliation

- **CAP.1** — capstone gap analysis ([#97](https://github.com/SteveVitali/HarnessHarness/pull/97))
- **CAP.2** — capstone composed verification ([#98](https://github.com/SteveVitali/HarnessHarness/pull/98))
- **CAP.3** — capstone closure ([#99](https://github.com/SteveVitali/HarnessHarness/pull/99))
- **REC.1** — backlog and readiness ([#100](https://github.com/SteveVitali/HarnessHarness/pull/100))
- **REC.2** — spec reconciliation ([#101](https://github.com/SteveVitali/HarnessHarness/pull/101))
- **REC.3** — integration plan ([#102](https://github.com/SteveVitali/HarnessHarness/pull/102))

### Gates & human checkpoints

- **GATE-G1** — Stage-0 acceptance & ecosystem-decision revalidation: PASSED (ADR-0226).
- **GATE-G2** — Stage-3 evaluation-first acceptance (first claim may exist): PASSED.
- **GATE-G3** — Stage-6 evolution preconditions: PASSED.
- **GATE-ACCEPT** — operator signed all 10 accepted-deviation rows: PASSED (readout `docs/build/readouts/GATE-ACCEPT.md`).
- **HUMAN-H1** (bind surface ecosystem) / **HUMAN-H2** (provision real tracker) — pending human acts, non-blocking; carried by backlog rows.

## Breaking changes

None recorded for an external consumer — this is the first landed trunk of the build; the spec's
compat matrix (`compat-1.matrix.json`) is the forward contract. *(Operator: edit as needed.)*

## Known gaps (carried forward — `docs/build/BACKLOG.csv`, 43 rows: 32 open / 7 accepted)

- **Live provider transports + TLS-terminating production transport** (BL-31) — real
  `Transport`/`CredentialPort` impls, non-static routing arms, real
  Embedder/Summarizer/ProviderCompaction adapters. Requirement family: `R-2.3.1`, `R-2.8.3`, `R-2.8.4`.
- **Remote fetch transports + remaining foreign import/export vocabularies** (BL-32) —
  `manifest.locations[]` remote transports; `swebench_submission` / `inspect_eval_log` /
  `in_toto_bundle` / `telemetry_trace` lifts. Requirement family: `R-2.9.3`, `R-2.10.3`, `R-2.10.5`.
- **Round-2 revalidation packages** (BL-35…BL-40) — the six ratified deferral-ADR registers
  (ADR-0210…0216), 103 deferred open questions, and the spec-debt register's first revalidation sweep.
- **Program-resume (WS-L6 window)** (BL-33, BL-34, BL-38) and **standing** rows (BL-41, BL-43).
- Everything owed lives in exactly one backlog row — `check-backlog.sh` enforces complete +
  non-duplicating placement (`docs/build/BACKLOG.md`).
