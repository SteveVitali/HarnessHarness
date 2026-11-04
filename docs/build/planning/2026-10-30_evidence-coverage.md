# Round 5 — exact supplemental ownership and predicate trace

**Status:** decomposition draft, no implementation/research/participant evidence. Supplemental source/pattern: `docs/evidence-and-adoption-spec.md`, `REQ-[A-H]-[0-9]+`. Frozen source SHA-256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`.

180 original IDs: 167 adopted targets, 12 merged aliases and one superseded alias. Each adopted target has exactly one primary owner; aliases inherit that owner while preserving their literal source predicate. D108 targets D304 only, with D309/D310 dependencies. [Literal companion](2026-10-30_evidence-coverage.json) preserves original row/hash, legacy relation, source finding/OL, AC hooks, layer and contributor IDs. It is a plan, not a result store.

## Completion rules

A ticket owns a bounded deliverable; a requirement is satisfied only after **all** its original/alias predicates and domains have actual evidence. Earlier D/E code owners retain explicit owed R/P/real-execution E to named contributing tickets; these are acceptance-evidence dependencies, never forward branch/dispatch dependencies. On ticket closure open/update DF rows for still-owed predicates, with exact target, later owner/trigger and withheld claim; this register alone is not a waiver. CAP5.1/3 recompute full-target closure from those records. Never block an early engineering slice waiting for a later study it enables, and never count that slice as full research completion.

For every target, native proof goes into its actual run/PR/native receipt, not this planning document. D/E/R/P layer names are separate from fixture/live/foreign evidence classes. Optional exercises retain explicit unselected/withheld scope until an operator decision; they do not silently disappear. No target currently has MET evidence.

## Literal owner table

| Literal ID | Disposition / target | Primary owner / numbered acceptance | Required layer | Supplemental hooks | Contributing evidence owners | Status |
|---|---|---|---|---|---|---|
| [REQ-A-001](../../evidence-and-adoption-spec.md#req-a-001) | merged-with(REQ-C-102) | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1) | primary only | planned; unmet |
| [REQ-A-002](../../evidence-and-adoption-spec.md#req-a-002) | adopted | [R5.28 AC-3](../../tickets/198_R5.28__recipe.md#ac-r5-28-3) | E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-A-003](../../evidence-and-adoption-spec.md#req-a-003) | adopted | [R5.43 AC-1](../../tickets/217_R5.43__useful.md#ac-r5-43-1) | E/R | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1), [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1) | primary only | planned; unmet |
| [REQ-A-004](../../evidence-and-adoption-spec.md#req-a-004) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | E/D | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.43](../../tickets/217_R5.43__useful.md) | planned; unmet |
| [REQ-A-005](../../evidence-and-adoption-spec.md#req-a-005) | adopted | [R5.26 AC-3](../../tickets/196_R5.26__replay.md#ac-r5-26-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-A-006](../../evidence-and-adoption-spec.md#req-a-006) | adopted | [R5.50 AC-1](../../tickets/229_R5.50__plugin.md#ac-r5-50-1) | E/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1) | primary only | planned; unmet |
| [REQ-A-007](../../evidence-and-adoption-spec.md#req-a-007) | adopted | [R5.20 AC-3](../../tickets/190_R5.20__wire.md#ac-r5-20-3) | D/E | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1), [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2) | primary only | planned; unmet |
| [REQ-A-008](../../evidence-and-adoption-spec.md#req-a-008) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-A-101](../../evidence-and-adoption-spec.md#req-a-101) | adopted | [R5.21 AC-3](../../tickets/191_R5.21__profile.md#ac-r5-21-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-3](../../evidence-and-adoption-spec.md#ac-6-3) | [R5.23](../../tickets/193_R5.23__population-receipt.md) | planned; unmet |
| [REQ-A-102](../../evidence-and-adoption-spec.md#req-a-102) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | E/D | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.43](../../tickets/217_R5.43__useful.md) | planned; unmet |
| [REQ-A-103](../../evidence-and-adoption-spec.md#req-a-103) | adopted | [R5.21 AC-3](../../tickets/191_R5.21__profile.md#ac-r5-21-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-3](../../evidence-and-adoption-spec.md#ac-6-3) | [R5.22](../../tickets/192_R5.22__closure.md), [R5.24](../../tickets/194_R5.24__r0.md), [R5.25](../../tickets/195_R5.25__recompute.md), [R5.26](../../tickets/196_R5.26__replay.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-A-104](../../evidence-and-adoption-spec.md#req-a-104) | adopted | [R5.43 AC-1](../../tickets/217_R5.43__useful.md#ac-r5-43-1) | E/R | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1), [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1) | primary only | planned; unmet |
| [REQ-A-105](../../evidence-and-adoption-spec.md#req-a-105) | adopted | [R5.26 AC-3](../../tickets/196_R5.26__replay.md#ac-r5-26-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-A-106](../../evidence-and-adoption-spec.md#req-a-106) | adopted | [R5.28 AC-3](../../tickets/198_R5.28__recipe.md#ac-r5-28-3) | E/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1) | [R5.50](../../tickets/229_R5.50__plugin.md) | planned; unmet |
| [REQ-A-201](../../evidence-and-adoption-spec.md#req-a-201) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | E/P | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-A-202](../../evidence-and-adoption-spec.md#req-a-202) | adopted | [R5.28 AC-3](../../tickets/198_R5.28__recipe.md#ac-r5-28-3) | E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-A-203](../../evidence-and-adoption-spec.md#req-a-203) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | E/D | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | primary only | planned; unmet |
| [REQ-A-204](../../evidence-and-adoption-spec.md#req-a-204) | adopted | [R5.24 AC-3](../../tickets/194_R5.24__r0.md#ac-r5-24-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-A-205](../../evidence-and-adoption-spec.md#req-a-205) | adopted | [R5.31 AC-3](../../tickets/201_R5.31__contrib.md#ac-r5-31-3) | E/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1) | [R5.50](../../tickets/229_R5.50__plugin.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-B-001](../../evidence-and-adoption-spec.md#req-b-001) | adopted | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1) | primary only | planned; unmet |
| [REQ-B-002](../../evidence-and-adoption-spec.md#req-b-002) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | E/P | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-B-003](../../evidence-and-adoption-spec.md#req-b-003) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | primary only | planned; unmet |
| [REQ-B-004](../../evidence-and-adoption-spec.md#req-b-004) | adopted | [R5.31 AC-3](../../tickets/201_R5.31__contrib.md#ac-r5-31-3) | D/E | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1) | [R5.50](../../tickets/229_R5.50__plugin.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-B-005](../../evidence-and-adoption-spec.md#req-b-005) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | P/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [HUMAN-H11](../../tickets/236_HUMAN-H11__value-dates.md), [R5.55](../../tickets/237_R5.55__value.md) | planned; unmet |
| [REQ-B-006](../../evidence-and-adoption-spec.md#req-b-006) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | P/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-B-007](../../evidence-and-adoption-spec.md#req-b-007) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | P/D | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-B-101](../../evidence-and-adoption-spec.md#req-b-101) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-B-102](../../evidence-and-adoption-spec.md#req-b-102) | adopted | [R5.51 AC-1](../../tickets/230_R5.51__interchange.md#ac-r5-51-1) | E/D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2), [AC-1-2](../../evidence-and-adoption-spec.md#ac-1-2) | primary only | planned; unmet |
| [REQ-B-103](../../evidence-and-adoption-spec.md#req-b-103) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-B-104](../../evidence-and-adoption-spec.md#req-b-104) | adopted | [R5.26 AC-3](../../tickets/196_R5.26__replay.md#ac-r5-26-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-B-105](../../evidence-and-adoption-spec.md#req-b-105) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E/R | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-B-106](../../evidence-and-adoption-spec.md#req-b-106) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-B-107](../../evidence-and-adoption-spec.md#req-b-107) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/P | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.35](../../tickets/205_R5.35__position.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-C-001](../../evidence-and-adoption-spec.md#req-c-001) | merged-with(REQ-C-102) | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1) | primary only | planned; unmet |
| [REQ-C-002](../../evidence-and-adoption-spec.md#req-c-002) | adopted | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E/D | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1), [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | [R5.43](../../tickets/217_R5.43__useful.md) | planned; unmet |
| [REQ-C-003](../../evidence-and-adoption-spec.md#req-c-003) | adopted | [R5.24 AC-3](../../tickets/194_R5.24__r0.md#ac-r5-24-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2), [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.25](../../tickets/195_R5.25__recompute.md), [R5.26](../../tickets/196_R5.26__replay.md), [R5.45](../../tickets/219_R5.45__trace.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-C-004](../../evidence-and-adoption-spec.md#req-c-004) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-C-005](../../evidence-and-adoption-spec.md#req-c-005) | adopted | [CAP5.3 AC-3](../../tickets/240_CAP5.3__cap3.md#ac-cap5-3-3) | D/R | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-C-006](../../evidence-and-adoption-spec.md#req-c-006) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-C-007](../../evidence-and-adoption-spec.md#req-c-007) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-1-2](../../evidence-and-adoption-spec.md#ac-1-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.50](../../tickets/229_R5.50__plugin.md), [R5.51](../../tickets/230_R5.51__interchange.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-C-008](../../evidence-and-adoption-spec.md#req-c-008) | adopted | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-C-101](../../evidence-and-adoption-spec.md#req-c-101) | adopted | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-C-102](../../evidence-and-adoption-spec.md#req-c-102) | adopted | [R5.35 AC-3](../../tickets/205_R5.35__position.md#ac-r5-35-3) | D | [§1](../../evidence-and-adoption-spec.md#s1); [AC-1-1](../../evidence-and-adoption-spec.md#ac-1-1) | primary only | planned; unmet |
| [REQ-C-103](../../evidence-and-adoption-spec.md#req-c-103) | adopted | [R5.7 AC-3](../../tickets/176_R5.7__adopt.md#ac-r5-7-3) | E/D | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1), [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | primary only | planned; unmet |
| [REQ-C-104](../../evidence-and-adoption-spec.md#req-c-104) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-C-105](../../evidence-and-adoption-spec.md#req-c-105) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/R | [§9](../../evidence-and-adoption-spec.md#s9); [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4), [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-C-106](../../evidence-and-adoption-spec.md#req-c-106) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-C-107](../../evidence-and-adoption-spec.md#req-c-107) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-C-108](../../evidence-and-adoption-spec.md#req-c-108) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-D-001](../../evidence-and-adoption-spec.md#req-d-001) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | E/R | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-002](../../evidence-and-adoption-spec.md#req-d-002) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | E/R | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-003](../../evidence-and-adoption-spec.md#req-d-003) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | E/R | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-004](../../evidence-and-adoption-spec.md#req-d-004) | adopted | [R5.39 AC-3](../../tickets/209_R5.39__parity-admission.md#ac-r5-39-3) | E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-005](../../evidence-and-adoption-spec.md#req-d-005) | adopted | [R5.36 AC-3](../../tickets/206_R5.36__metrics.md#ac-r5-36-3) | E | [§4](../../evidence-and-adoption-spec.md#s4); [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2), [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1) | [R5.20](../../tickets/190_R5.20__wire.md) | planned; unmet |
| [REQ-D-006](../../evidence-and-adoption-spec.md#req-d-006) | adopted | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.12](../../tickets/181_R5.12__reserve.md), [R5.13](../../tickets/182_R5.13__physical.md) | planned; unmet |
| [REQ-D-007](../../evidence-and-adoption-spec.md#req-d-007) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/R | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-2](../../evidence-and-adoption-spec.md#ac-3-2), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-008](../../evidence-and-adoption-spec.md#req-d-008) | adopted | [R5.37 AC-3](../../tickets/207_R5.37__taskstats.md#ac-r5-37-3) | E | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-2](../../evidence-and-adoption-spec.md#ac-4-2), [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3) | [R5.16](../../tickets/185_R5.16__beta.md), [R5.38](../../tickets/208_R5.38__kl.md), [R5.40](../../tickets/210_R5.40__numerics.md) | planned; unmet |
| [REQ-D-009](../../evidence-and-adoption-spec.md#req-d-009) | adopted | [R5.49 AC-1](../../tickets/226_R5.49__study-report.md#ac-r5-49-1) | D/R | [§3](../../evidence-and-adoption-spec.md#s3); [AC-3-2](../../evidence-and-adoption-spec.md#ac-3-2), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-D-010](../../evidence-and-adoption-spec.md#req-d-010) | adopted | [R5.26 AC-3](../../tickets/196_R5.26__replay.md#ac-r5-26-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-D-101](../../evidence-and-adoption-spec.md#req-d-101) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-102](../../evidence-and-adoption-spec.md#req-d-102) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-103](../../evidence-and-adoption-spec.md#req-d-103) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-104](../../evidence-and-adoption-spec.md#req-d-104) | adopted | [R5.15 AC-3](../../tickets/184_R5.15__population.md#ac-r5-15-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-105](../../evidence-and-adoption-spec.md#req-d-105) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-106](../../evidence-and-adoption-spec.md#req-d-106) | adopted | [R5.36 AC-3](../../tickets/206_R5.36__metrics.md#ac-r5-36-3) | E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-107](../../evidence-and-adoption-spec.md#req-d-107) | adopted | [R5.16 AC-3](../../tickets/185_R5.16__beta.md#ac-r5-16-3) | E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-2](../../evidence-and-adoption-spec.md#ac-4-2), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-108](../../evidence-and-adoption-spec.md#req-d-108) | superseded-by(REQ-D-304); dependencies REQ-D-309/310 | [R5.38 AC-3](../../tickets/208_R5.38__kl.md#ac-r5-38-3) | D/E | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-109](../../evidence-and-adoption-spec.md#req-d-109) | adopted | [R5.40 AC-3](../../tickets/210_R5.40__numerics.md#ac-r5-40-3) | E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-2](../../evidence-and-adoption-spec.md#ac-4-2), [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1) | [R5.16](../../tickets/185_R5.16__beta.md), [R5.17](../../tickets/186_R5.17__early-numeric.md), [R5.36](../../tickets/206_R5.36__metrics.md), [R5.37](../../tickets/207_R5.37__taskstats.md), [R5.38](../../tickets/208_R5.38__kl.md), [R5.39](../../tickets/209_R5.39__parity-admission.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-110](../../evidence-and-adoption-spec.md#req-d-110) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/R | [§9](../../evidence-and-adoption-spec.md#s9); [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4), [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-201](../../evidence-and-adoption-spec.md#req-d-201) | adopted | [R5.43 AC-1](../../tickets/217_R5.43__useful.md#ac-r5-43-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1) | primary only | planned; unmet |
| [REQ-D-202](../../evidence-and-adoption-spec.md#req-d-202) | adopted | [R5.26 AC-3](../../tickets/196_R5.26__replay.md#ac-r5-26-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-D-203](../../evidence-and-adoption-spec.md#req-d-203) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-D-204](../../evidence-and-adoption-spec.md#req-d-204) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-D-205](../../evidence-and-adoption-spec.md#req-d-205) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-D-206](../../evidence-and-adoption-spec.md#req-d-206) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-D-207](../../evidence-and-adoption-spec.md#req-d-207) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-D-208](../../evidence-and-adoption-spec.md#req-d-208) | adopted | [R5.24 AC-3](../../tickets/194_R5.24__r0.md#ac-r5-24-3) | E | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-D-209](../../evidence-and-adoption-spec.md#req-d-209) | adopted | [R5.50 AC-1](../../tickets/229_R5.50__plugin.md#ac-r5-50-1) | E | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-D-301](../../evidence-and-adoption-spec.md#req-d-301) | adopted | [R5.39 AC-3](../../tickets/209_R5.39__parity-admission.md#ac-r5-39-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-302](../../evidence-and-adoption-spec.md#req-d-302) | adopted | [R5.39 AC-3](../../tickets/209_R5.39__parity-admission.md#ac-r5-39-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-303](../../evidence-and-adoption-spec.md#req-d-303) | adopted | [R5.39 AC-3](../../tickets/209_R5.39__parity-admission.md#ac-r5-39-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-304](../../evidence-and-adoption-spec.md#req-d-304) | adopted | [R5.38 AC-3](../../tickets/208_R5.38__kl.md#ac-r5-38-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-305](../../evidence-and-adoption-spec.md#req-d-305) | adopted | [R5.38 AC-3](../../tickets/208_R5.38__kl.md#ac-r5-38-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-306](../../evidence-and-adoption-spec.md#req-d-306) | adopted | [R5.38 AC-3](../../tickets/208_R5.38__kl.md#ac-r5-38-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-307](../../evidence-and-adoption-spec.md#req-d-307) | adopted | [R5.38 AC-3](../../tickets/208_R5.38__kl.md#ac-r5-38-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-308](../../evidence-and-adoption-spec.md#req-d-308) | adopted | [R5.39 AC-3](../../tickets/209_R5.39__parity-admission.md#ac-r5-39-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-309](../../evidence-and-adoption-spec.md#req-d-309) | adopted | [R5.42 AC-3](../../tickets/212_R5.42__quote.md#ac-r5-42-3) | D/R | [§9](../../evidence-and-adoption-spec.md#s9); [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4), [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1) | [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md), [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md), [R5.42a](../../tickets/215_R5.42a__backend-preflight.md), [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md), [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md), [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md), [R5.47](../../tickets/223_R5.47__parity-live.md), [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-D-310](../../evidence-and-adoption-spec.md#req-d-310) | adopted | [R5.40 AC-3](../../tickets/210_R5.40__numerics.md#ac-r5-40-3) | D/E/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.16](../../tickets/185_R5.16__beta.md), [R5.17](../../tickets/186_R5.17__early-numeric.md), [R5.36](../../tickets/206_R5.36__metrics.md), [R5.37](../../tickets/207_R5.37__taskstats.md), [R5.38](../../tickets/208_R5.38__kl.md), [R5.39](../../tickets/209_R5.39__parity-admission.md), [R5.47](../../tickets/223_R5.47__parity-live.md), [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-E-001](../../evidence-and-adoption-spec.md#req-e-001) | merged-with(REQ-E-201) | [R5.7 AC-3](../../tickets/176_R5.7__adopt.md#ac-r5-7-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.8](../../tickets/177_R5.8__effect.md), [R5.10](../../tickets/179_R5.10__seal.md), [R5.14](../../tickets/183_R5.14__pending.md) | planned; unmet |
| [REQ-E-002](../../evidence-and-adoption-spec.md#req-e-002) | adopted | [R5.8 AC-3](../../tickets/177_R5.8__effect.md#ac-r5-8-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.43](../../tickets/217_R5.43__useful.md) | planned; unmet |
| [REQ-E-003](../../evidence-and-adoption-spec.md#req-e-003) | merged-with(REQ-E-206) | [R5.20 AC-3](../../tickets/190_R5.20__wire.md#ac-r5-20-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2) | [R5.18](../../tickets/188_R5.18__retained.md) | planned; unmet |
| [REQ-E-004](../../evidence-and-adoption-spec.md#req-e-004) | adopted | [R5.36 AC-3](../../tickets/206_R5.36__metrics.md#ac-r5-36-3) | E | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1) | primary only | planned; unmet |
| [REQ-E-005](../../evidence-and-adoption-spec.md#req-e-005) | merged-with(REQ-E-207) | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.12](../../tickets/181_R5.12__reserve.md), [R5.13](../../tickets/182_R5.13__physical.md) | planned; unmet |
| [REQ-E-006](../../evidence-and-adoption-spec.md#req-e-006) | adopted | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | D/E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2), [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | primary only | planned; unmet |
| [REQ-E-007](../../evidence-and-adoption-spec.md#req-e-007) | adopted | [R5.11 AC-3](../../tickets/180_R5.11__oracle.md#ac-r5-11-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-5-1](../../evidence-and-adoption-spec.md#ac-5-1), [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.43](../../tickets/217_R5.43__useful.md) | planned; unmet |
| [REQ-E-008](../../evidence-and-adoption-spec.md#req-e-008) | adopted | [R5.14 AC-3](../../tickets/183_R5.14__pending.md#ac-r5-14-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | primary only | met — engineered D/E evidence landed seq 183a R5.14b (2026-11-02); see [dated note](#dated-note-2026-11-02-req-e-202-req-e-008-discharge) + runs/R5.14b.md |
| [REQ-E-009](../../evidence-and-adoption-spec.md#req-e-009) | adopted | [R5.28 AC-3](../../tickets/198_R5.28__recipe.md#ac-r5-28-3) | E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | primary only | planned; unmet |
| [REQ-E-010](../../evidence-and-adoption-spec.md#req-e-010) | merged-with(REQ-E-208) | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.22](../../tickets/192_R5.22__closure.md), [R5.24](../../tickets/194_R5.24__r0.md) | planned; unmet |
| [REQ-E-011](../../evidence-and-adoption-spec.md#req-e-011) | adopted | [R5.41 AC-3](../../tickets/211_R5.41__protocol.md#ac-r5-41-3) | D/R | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-4](../../evidence-and-adoption-spec.md#ac-4-4) | [R5.49](../../tickets/226_R5.49__study-report.md) | planned; unmet |
| [REQ-E-012](../../evidence-and-adoption-spec.md#req-e-012) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-E-101](../../evidence-and-adoption-spec.md#req-e-101) | merged-with(REQ-E-201) | [R5.7 AC-3](../../tickets/176_R5.7__adopt.md#ac-r5-7-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.8](../../tickets/177_R5.8__effect.md), [R5.10](../../tickets/179_R5.10__seal.md), [R5.14](../../tickets/183_R5.14__pending.md) | planned; unmet |
| [REQ-E-102](../../evidence-and-adoption-spec.md#req-e-102) | adopted | [R5.10 AC-3](../../tickets/179_R5.10__seal.md#ac-r5-10-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.9](../../tickets/178_R5.9__selection.md), [R5.11](../../tickets/180_R5.11__oracle.md), [R5.13](../../tickets/182_R5.13__physical.md), [R5.14](../../tickets/183_R5.14__pending.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.14b](../../tickets/183a_R5.14b__pending-consumers.md) | planned; unmet |
| [REQ-E-103](../../evidence-and-adoption-spec.md#req-e-103) | adopted | [R5.9 AC-3](../../tickets/178_R5.9__selection.md#ac-r5-9-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.10](../../tickets/179_R5.10__seal.md), [R5.11](../../tickets/180_R5.11__oracle.md), [R5.13](../../tickets/182_R5.13__physical.md), [R5.14](../../tickets/183_R5.14__pending.md), [R5.14b](../../tickets/183a_R5.14b__pending-consumers.md) | planned; unmet |
| [REQ-E-104](../../evidence-and-adoption-spec.md#req-e-104) | merged-with(REQ-E-206) | [R5.20 AC-3](../../tickets/190_R5.20__wire.md#ac-r5-20-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2) | [R5.18](../../tickets/188_R5.18__retained.md) | planned; unmet |
| [REQ-E-105](../../evidence-and-adoption-spec.md#req-e-105) | merged-with(REQ-E-207) | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.12](../../tickets/181_R5.12__reserve.md), [R5.13](../../tickets/182_R5.13__physical.md) | planned; unmet |
| [REQ-E-106](../../evidence-and-adoption-spec.md#req-e-106) | merged-with(REQ-E-207) | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.12](../../tickets/181_R5.12__reserve.md), [R5.13](../../tickets/182_R5.13__physical.md) | planned; unmet |
| [REQ-E-107](../../evidence-and-adoption-spec.md#req-e-107) | adopted | [R5.36 AC-3](../../tickets/206_R5.36__metrics.md#ac-r5-36-3) | E | [§4](../../evidence-and-adoption-spec.md#s4); [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1) | primary only | planned; unmet |
| [REQ-E-108](../../evidence-and-adoption-spec.md#req-e-108) | merged-with(REQ-E-208) | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.22](../../tickets/192_R5.22__closure.md), [R5.24](../../tickets/194_R5.24__r0.md) | planned; unmet |
| [REQ-E-109](../../evidence-and-adoption-spec.md#req-e-109) | merged-with(REQ-E-208) | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.22](../../tickets/192_R5.22__closure.md), [R5.24](../../tickets/194_R5.24__r0.md) | planned; unmet |
| [REQ-E-110](../../evidence-and-adoption-spec.md#req-e-110) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-E-201](../../evidence-and-adoption-spec.md#req-e-201) | adopted | [R5.7 AC-3](../../tickets/176_R5.7__adopt.md#ac-r5-7-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.8](../../tickets/177_R5.8__effect.md), [R5.10](../../tickets/179_R5.10__seal.md), [R5.14](../../tickets/183_R5.14__pending.md), [R5.14b](../../tickets/183a_R5.14b__pending-consumers.md) | planned; unmet |
| [REQ-E-202](../../evidence-and-adoption-spec.md#req-e-202) | adopted | [R5.14 AC-3](../../tickets/183_R5.14__pending.md#ac-r5-14-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | primary only | met — engineered D/E evidence landed seq 183a R5.14b (2026-11-02); see [dated note](#dated-note-2026-11-02-req-e-202-req-e-008-discharge) + runs/R5.14b.md |
| [REQ-E-203](../../evidence-and-adoption-spec.md#req-e-203) | adopted | [R5.10 AC-3](../../tickets/179_R5.10__seal.md#ac-r5-10-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | primary only | planned; unmet |
| [REQ-E-204](../../evidence-and-adoption-spec.md#req-e-204) | adopted | [R5.9 AC-3](../../tickets/178_R5.9__selection.md#ac-r5-9-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-1](../../evidence-and-adoption-spec.md#ac-2-1) | [R5.10](../../tickets/179_R5.10__seal.md), [R5.11](../../tickets/180_R5.11__oracle.md), [R5.13](../../tickets/182_R5.13__physical.md), [R5.14](../../tickets/183_R5.14__pending.md), [R5.14b](../../tickets/183a_R5.14b__pending-consumers.md) | planned; unmet |
| [REQ-E-205](../../evidence-and-adoption-spec.md#req-e-205) | adopted | [R5.44 AC-1](../../tickets/218_R5.44__ops-live.md#ac-r5-44-1) | E | [§5](../../evidence-and-adoption-spec.md#s5); [AC-5-2](../../evidence-and-adoption-spec.md#ac-5-2) | [R5.8](../../tickets/177_R5.8__effect.md), [R5.32](../../tickets/202_R5.32__observer.md), [R5.33](../../tickets/203_R5.33__permission.md), [R5.34](../../tickets/204_R5.34__operations.md) | planned; unmet |
| [REQ-E-206](../../evidence-and-adoption-spec.md#req-e-206) | adopted | [R5.20 AC-3](../../tickets/190_R5.20__wire.md#ac-r5-20-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-2](../../evidence-and-adoption-spec.md#ac-2-2) | [R5.18](../../tickets/188_R5.18__retained.md) | planned; unmet |
| [REQ-E-207](../../evidence-and-adoption-spec.md#req-e-207) | adopted | [R5.19 AC-3](../../tickets/189_R5.19__gateway.md#ac-r5-19-3) | E | [§2](../../evidence-and-adoption-spec.md#s2); [AC-2-3](../../evidence-and-adoption-spec.md#ac-2-3) | [R5.6](../../tickets/175_R5.6__launch.md), [R5.12](../../tickets/181_R5.12__reserve.md), [R5.13](../../tickets/182_R5.13__physical.md) | planned; unmet |
| [REQ-E-208](../../evidence-and-adoption-spec.md#req-e-208) | adopted | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.22](../../tickets/192_R5.22__closure.md), [R5.24](../../tickets/194_R5.24__r0.md) | planned; unmet |
| [REQ-E-209](../../evidence-and-adoption-spec.md#req-e-209) | adopted | [R5.40 AC-3](../../tickets/210_R5.40__numerics.md#ac-r5-40-3) | D/E | [§4](../../evidence-and-adoption-spec.md#s4); [AC-3-1](../../evidence-and-adoption-spec.md#ac-3-1), [AC-4-1](../../evidence-and-adoption-spec.md#ac-4-1), [AC-4-2](../../evidence-and-adoption-spec.md#ac-4-2), [AC-4-3](../../evidence-and-adoption-spec.md#ac-4-3) | [R5.16](../../tickets/185_R5.16__beta.md), [R5.17](../../tickets/186_R5.17__early-numeric.md), [R5.36](../../tickets/206_R5.36__metrics.md), [R5.37](../../tickets/207_R5.37__taskstats.md), [R5.38](../../tickets/208_R5.38__kl.md), [R5.39](../../tickets/209_R5.39__parity-admission.md) | planned; unmet |
| [REQ-E-210](../../evidence-and-adoption-spec.md#req-e-210) | adopted | [REC5.2 AC-3](../../tickets/243_REC5.2__rec2.md#ac-rec5-2-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | [R5.3](../../tickets/171_R5.3__adr-exec.md), [R5.4](../../tickets/172_R5.4__adr-receipt.md), [R5.5](../../tickets/173_R5.5__adr-methods.md) | planned; unmet |
| [REQ-F-001](../../evidence-and-adoption-spec.md#req-f-001) | adopted | [R5.21 AC-3](../../tickets/191_R5.21__profile.md#ac-r5-21-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | primary only | planned; unmet |
| [REQ-F-002](../../evidence-and-adoption-spec.md#req-f-002) | adopted | [R5.21 AC-3](../../tickets/191_R5.21__profile.md#ac-r5-21-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | primary only | planned; unmet |
| [REQ-F-003](../../evidence-and-adoption-spec.md#req-f-003) | adopted | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | primary only | planned; unmet |
| [REQ-F-004](../../evidence-and-adoption-spec.md#req-f-004) | adopted | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | primary only | planned; unmet |
| [REQ-F-005](../../evidence-and-adoption-spec.md#req-f-005) | adopted | [R5.22 AC-3](../../tickets/192_R5.22__closure.md#ac-r5-22-3) | E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1) | [R5.25](../../tickets/195_R5.25__recompute.md), [R5.26](../../tickets/196_R5.26__replay.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-F-006](../../evidence-and-adoption-spec.md#req-f-006) | adopted | [R5.27 AC-3](../../tickets/197_R5.27__rights.md#ac-r5-27-3) | D/E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-3](../../evidence-and-adoption-spec.md#ac-6-3) | [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-F-007](../../evidence-and-adoption-spec.md#req-f-007) | adopted | [R5.24 AC-3](../../tickets/194_R5.24__r0.md#ac-r5-24-3) | D/E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.25](../../tickets/195_R5.25__recompute.md), [R5.26](../../tickets/196_R5.26__replay.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-F-008](../../evidence-and-adoption-spec.md#req-f-008) | adopted | [R5.23 AC-3](../../tickets/193_R5.23__population-receipt.md#ac-r5-23-3) | D/E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-1](../../evidence-and-adoption-spec.md#ac-6-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-F-101](../../evidence-and-adoption-spec.md#req-f-101) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-F-102](../../evidence-and-adoption-spec.md#req-f-102) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-F-103](../../evidence-and-adoption-spec.md#req-f-103) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-F-104](../../evidence-and-adoption-spec.md#req-f-104) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-F-105](../../evidence-and-adoption-spec.md#req-f-105) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1) | [R5.29](../../tickets/199_R5.29__offline.md) | planned; unmet |
| [REQ-F-106](../../evidence-and-adoption-spec.md#req-f-106) | adopted | [R5.30 AC-3](../../tickets/200_R5.30__ci.md#ac-r5-30-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2) | primary only | planned; unmet |
| [REQ-F-107](../../evidence-and-adoption-spec.md#req-f-107) | adopted | [R5.30 AC-3](../../tickets/200_R5.30__ci.md#ac-r5-30-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2) | primary only | planned; unmet |
| [REQ-F-108](../../evidence-and-adoption-spec.md#req-f-108) | adopted | [R5.30 AC-3](../../tickets/200_R5.30__ci.md#ac-r5-30-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2) | primary only | planned; unmet |
| [REQ-F-201](../../evidence-and-adoption-spec.md#req-f-201) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.27](../../tickets/197_R5.27__rights.md), [R5.28](../../tickets/198_R5.28__recipe.md) | planned; unmet |
| [REQ-F-202](../../evidence-and-adoption-spec.md#req-f-202) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.27](../../tickets/197_R5.27__rights.md), [R5.28](../../tickets/198_R5.28__recipe.md) | planned; unmet |
| [REQ-F-203](../../evidence-and-adoption-spec.md#req-f-203) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.25](../../tickets/195_R5.25__recompute.md) | planned; unmet |
| [REQ-F-204](../../evidence-and-adoption-spec.md#req-f-204) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.26](../../tickets/196_R5.26__replay.md) | planned; unmet |
| [REQ-F-205](../../evidence-and-adoption-spec.md#req-f-205) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.26](../../tickets/196_R5.26__replay.md) | planned; unmet |
| [REQ-F-206](../../evidence-and-adoption-spec.md#req-f-206) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | E/P | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-F-207](../../evidence-and-adoption-spec.md#req-f-207) | adopted | [R5.27 AC-3](../../tickets/197_R5.27__rights.md#ac-r5-27-3) | D/E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-3](../../evidence-and-adoption-spec.md#ac-6-3) | [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-F-208](../../evidence-and-adoption-spec.md#req-f-208) | adopted | [R5.27 AC-3](../../tickets/197_R5.27__rights.md#ac-r5-27-3) | D/E | [§6](../../evidence-and-adoption-spec.md#s6); [AC-6-3](../../evidence-and-adoption-spec.md#ac-6-3), [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1) | [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-001](../../evidence-and-adoption-spec.md#req-g-001) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-002](../../evidence-and-adoption-spec.md#req-g-002) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-G-003](../../evidence-and-adoption-spec.md#req-g-003) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-004](../../evidence-and-adoption-spec.md#req-g-004) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-G-005](../../evidence-and-adoption-spec.md#req-g-005) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2); [§8 participant evidence](../../evidence-and-adoption-spec.md#s8), [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-006](../../evidence-and-adoption-spec.md#req-g-006) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-G-007](../../evidence-and-adoption-spec.md#req-g-007) | adopted | [R5.45 AC-3](../../tickets/219_R5.45__trace.md#ac-r5-45-3) | D/E | [§7](../../evidence-and-adoption-spec.md#s7); [AC-7-1](../../evidence-and-adoption-spec.md#ac-7-1), [AC-7-2](../../evidence-and-adoption-spec.md#ac-7-2), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-G-008](../../evidence-and-adoption-spec.md#req-g-008) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md), [R5.50](../../tickets/229_R5.50__plugin.md), [R5.51](../../tickets/230_R5.51__interchange.md) | planned; unmet |
| [REQ-G-009](../../evidence-and-adoption-spec.md#req-g-009) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.28](../../tickets/198_R5.28__recipe.md), [R5.35](../../tickets/205_R5.35__position.md), [R5.43](../../tickets/217_R5.43__useful.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-101](../../evidence-and-adoption-spec.md#req-g-101) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-102](../../evidence-and-adoption-spec.md#req-g-102) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-103](../../evidence-and-adoption-spec.md#req-g-103) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-104](../../evidence-and-adoption-spec.md#req-g-104) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.27](../../tickets/197_R5.27__rights.md), [R5.28](../../tickets/198_R5.28__recipe.md) | planned; unmet |
| [REQ-G-105](../../evidence-and-adoption-spec.md#req-g-105) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-106](../../evidence-and-adoption-spec.md#req-g-106) | adopted | [R5.50 AC-1](../../tickets/229_R5.50__plugin.md#ac-r5-50-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | primary only | planned; unmet |
| [REQ-G-107](../../evidence-and-adoption-spec.md#req-g-107) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/E/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-1](../../evidence-and-adoption-spec.md#ac-8-1), [AC-6-2](../../evidence-and-adoption-spec.md#ac-6-2) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.31](../../tickets/201_R5.31__contrib.md), [R5.52](../../tickets/231_R5.52__outside.md) | planned; unmet |
| [REQ-G-201](../../evidence-and-adoption-spec.md#req-g-201) | adopted | [R5.54 AC-1](../../tickets/235_R5.54__publication-record.md#ac-r5-54-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.35](../../tickets/205_R5.35__position.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-202](../../evidence-and-adoption-spec.md#req-g-202) | adopted | [R5.54 AC-1](../../tickets/235_R5.54__publication-record.md#ac-r5-54-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.35](../../tickets/205_R5.35__position.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-203](../../evidence-and-adoption-spec.md#req-g-203) | adopted | [R5.52 AC-1](../../tickets/231_R5.52__outside.md#ac-r5-52-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.24](../../tickets/194_R5.24__r0.md), [R5.27](../../tickets/197_R5.27__rights.md), [R5.28](../../tickets/198_R5.28__recipe.md) | planned; unmet |
| [REQ-G-204](../../evidence-and-adoption-spec.md#req-g-204) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-G-205](../../evidence-and-adoption-spec.md#req-g-205) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [HUMAN-H11](../../tickets/236_HUMAN-H11__value-dates.md), [R5.55](../../tickets/237_R5.55__value.md) | planned; unmet |
| [REQ-G-206](../../evidence-and-adoption-spec.md#req-g-206) | adopted | [R5.55 AC-1](../../tickets/237_R5.55__value.md#ac-r5-55-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | primary only | planned; unmet |
| [REQ-G-207](../../evidence-and-adoption-spec.md#req-g-207) | adopted | [R5.54 AC-1](../../tickets/235_R5.54__publication-record.md#ac-r5-54-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.35](../../tickets/205_R5.35__position.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-G-208](../../evidence-and-adoption-spec.md#req-g-208) | adopted | [R5.53 AC-1](../../tickets/232_R5.53__formative.md#ac-r5-53-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [HUMAN-H11](../../tickets/236_HUMAN-H11__value-dates.md), [R5.55](../../tickets/237_R5.55__value.md) | planned; unmet |
| [REQ-G-209](../../evidence-and-adoption-spec.md#req-g-209) | adopted | [R5.54 AC-1](../../tickets/235_R5.54__publication-record.md#ac-r5-54-1) | D/P | [§8](../../evidence-and-adoption-spec.md#s8); [AC-8-2](../../evidence-and-adoption-spec.md#ac-8-2), [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.35](../../tickets/205_R5.35__position.md), [R5.45](../../tickets/219_R5.45__trace.md) | planned; unmet |
| [REQ-H-001](../../evidence-and-adoption-spec.md#req-h-001) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-002](../../evidence-and-adoption-spec.md#req-h-002) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-003](../../evidence-and-adoption-spec.md#req-h-003) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-004](../../evidence-and-adoption-spec.md#req-h-004) | adopted | [R5.3 AC-3](../../tickets/171_R5.3__adr-exec.md#ac-r5-3-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | [R5.4](../../tickets/172_R5.4__adr-receipt.md), [R5.5](../../tickets/173_R5.5__adr-methods.md), [GATE-G9](../../tickets/174_GATE-G9__admit.md), [REC5.2](../../tickets/243_REC5.2__rec2.md) | planned; unmet |
| [REQ-H-005](../../evidence-and-adoption-spec.md#req-h-005) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-006](../../evidence-and-adoption-spec.md#req-h-006) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | [GATE-G17](../../tickets/241_GATE-G17__accept.md) | planned; unmet |
| [REQ-H-007](../../evidence-and-adoption-spec.md#req-h-007) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-008](../../evidence-and-adoption-spec.md#req-h-008) | adopted | [R5.2 AC-3](../../tickets/170_R5.2__dispatch.md#ac-r5-2-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |
| [REQ-H-009](../../evidence-and-adoption-spec.md#req-h-009) | adopted | [CAP5.3 AC-3](../../tickets/240_CAP5.3__cap3.md#ac-cap5-3-3) | D/R/P | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-3](../../evidence-and-adoption-spec.md#ac-9-3) | [R5.49](../../tickets/226_R5.49__study-report.md), [R5.52](../../tickets/231_R5.52__outside.md), [R5.53](../../tickets/232_R5.53__formative.md), [R5.54](../../tickets/235_R5.54__publication-record.md), [R5.55](../../tickets/237_R5.55__value.md) | planned; unmet |
| [REQ-H-010](../../evidence-and-adoption-spec.md#req-h-010) | adopted | [R5.40 AC-3](../../tickets/210_R5.40__numerics.md#ac-r5-40-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | [R5.16](../../tickets/185_R5.16__beta.md), [R5.17](../../tickets/186_R5.17__early-numeric.md), [R5.36](../../tickets/206_R5.36__metrics.md), [R5.37](../../tickets/207_R5.37__taskstats.md), [R5.38](../../tickets/208_R5.38__kl.md), [R5.39](../../tickets/209_R5.39__parity-admission.md) | planned; unmet |
| [REQ-H-011](../../evidence-and-adoption-spec.md#req-h-011) | adopted | [R5.1 AC-3](../../tickets/169_R5.1__guard.md#ac-r5-1-3) | D/E | [§9](../../evidence-and-adoption-spec.md#s9); [AC-9-1](../../evidence-and-adoption-spec.md#ac-9-1), [AC-9-2](../../evidence-and-adoption-spec.md#ac-9-2) | primary only | planned; unmet |

## Outline-to-ticket trace

The frozen upstream table retains exact note/finding/candidate history. This projection adds owner hooks without editing upstream coverage status.

| OL | Frozen source chain | Candidate targets / current owners | Acceptance status |
|---|---|---|---|
| OL-01 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-B-001 → [R5.35](../../tickets/205_R5.35__position.md); REQ-G-001 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-009 → [R5.53](../../tickets/232_R5.53__formative.md) | specification covered; implementation/evidence unmet |
| OL-02 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-A-001 → [R5.35](../../tickets/205_R5.35__position.md); REQ-A-007 → [R5.20](../../tickets/190_R5.20__wire.md); REQ-C-102 → [R5.35](../../tickets/205_R5.35__position.md) | specification covered; implementation/evidence unmet |
| OL-03 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-001 → [R5.35](../../tickets/205_R5.35__position.md); REQ-C-008 → [R5.35](../../tickets/205_R5.35__position.md); REQ-C-102 → [R5.35](../../tickets/205_R5.35__position.md); REQ-G-106 → [R5.50](../../tickets/229_R5.50__plugin.md) | specification covered; implementation/evidence unmet |
| OL-04 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-B-101 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-B-102 → [R5.51](../../tickets/230_R5.51__interchange.md); REQ-B-103 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-B-104 → [R5.26](../../tickets/196_R5.26__replay.md); REQ-B-105 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-B-106 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-B-107 → [R5.53](../../tickets/232_R5.53__formative.md) | specification covered; implementation/evidence unmet |
| OL-05 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-D-201 → [R5.43](../../tickets/217_R5.43__useful.md); REQ-E-201 → [R5.7](../../tickets/176_R5.7__adopt.md); REQ-E-202 → [R5.14](../../tickets/183_R5.14__pending.md); REQ-E-203 → [R5.10](../../tickets/179_R5.10__seal.md); REQ-E-204 → [R5.9](../../tickets/178_R5.9__selection.md); REQ-E-205 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-G-004 → [R5.45](../../tickets/219_R5.45__trace.md) | specification covered; implementation/evidence unmet |
| OL-06 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-A-203 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-105 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-G-003 → [R5.53](../../tickets/232_R5.53__formative.md) | specification covered; implementation/evidence unmet |
| OL-07 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-F-005 → [R5.22](../../tickets/192_R5.22__closure.md); REQ-F-007 → [R5.24](../../tickets/194_R5.24__r0.md); REQ-F-201 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-202 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-203 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-204 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-205 → [R5.52](../../tickets/231_R5.52__outside.md) | specification covered; implementation/evidence unmet |
| OL-08 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-F-101 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-102 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-103 → [R5.45](../../tickets/219_R5.45__trace.md) | specification covered; implementation/evidence unmet |
| OL-09 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-004 → [R5.42](../../tickets/212_R5.42__quote.md); REQ-D-101 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-D-105 → [R5.42](../../tickets/212_R5.42__quote.md); REQ-D-301 → [R5.39](../../tickets/209_R5.39__parity-admission.md); REQ-D-308 → [R5.39](../../tickets/209_R5.39__parity-admission.md) | specification covered; implementation/evidence unmet |
| OL-10 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-D-001 → [R5.15](../../tickets/184_R5.15__population.md); REQ-D-002 → [R5.15](../../tickets/184_R5.15__population.md); REQ-D-003 → [R5.15](../../tickets/184_R5.15__population.md); REQ-D-005 → [R5.36](../../tickets/206_R5.36__metrics.md); REQ-D-006 → [R5.19](../../tickets/189_R5.19__gateway.md); REQ-D-007 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-D-008 → [R5.37](../../tickets/207_R5.37__taskstats.md); REQ-D-009 → [R5.49](../../tickets/226_R5.49__study-report.md); REQ-D-102 → [R5.41](../../tickets/211_R5.41__protocol.md); REQ-D-107 → [R5.16](../../tickets/185_R5.16__beta.md); REQ-D-109 → [R5.40](../../tickets/210_R5.40__numerics.md); REQ-D-302 → [R5.39](../../tickets/209_R5.39__parity-admission.md); REQ-D-303 → [R5.39](../../tickets/209_R5.39__parity-admission.md); REQ-D-304 → [R5.38](../../tickets/208_R5.38__kl.md); REQ-D-305 → [R5.38](../../tickets/208_R5.38__kl.md); REQ-D-306 → [R5.38](../../tickets/208_R5.38__kl.md); REQ-D-307 → [R5.38](../../tickets/208_R5.38__kl.md) | specification covered; implementation/evidence unmet |
| OL-11 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-005 → [CAP5.3](../../tickets/240_CAP5.3__cap3.md); REQ-H-009 → [CAP5.3](../../tickets/240_CAP5.3__cap3.md) | specification covered; implementation/evidence unmet |
| OL-12 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-D-203 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-D-204 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-D-205 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-D-206 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-D-207 → [R5.44](../../tickets/218_R5.44__ops-live.md); REQ-E-205 → [R5.44](../../tickets/218_R5.44__ops-live.md) | specification covered; implementation/evidence unmet |
| OL-13 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-A-006 → [R5.50](../../tickets/229_R5.50__plugin.md); REQ-D-209 → [R5.50](../../tickets/229_R5.50__plugin.md); REQ-G-106 → [R5.50](../../tickets/229_R5.50__plugin.md) | specification covered; implementation/evidence unmet |
| OL-14 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-102 → [R5.35](../../tickets/205_R5.35__position.md); REQ-E-210 → [REC5.2](../../tickets/243_REC5.2__rec2.md) | specification covered; implementation/evidence unmet |
| OL-15 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-A-004 → [R5.15](../../tickets/184_R5.15__population.md); REQ-A-102 → [R5.15](../../tickets/184_R5.15__population.md); REQ-F-003 → [R5.23](../../tickets/193_R5.23__population-receipt.md); REQ-F-008 → [R5.23](../../tickets/193_R5.23__population-receipt.md) | specification covered; implementation/evidence unmet |
| OL-16 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-F-001 → [R5.21](../../tickets/191_R5.21__profile.md); REQ-F-002 → [R5.21](../../tickets/191_R5.21__profile.md); REQ-F-003 → [R5.23](../../tickets/193_R5.23__population-receipt.md); REQ-F-004 → [R5.23](../../tickets/193_R5.23__population-receipt.md); REQ-F-005 → [R5.22](../../tickets/192_R5.22__closure.md); REQ-F-006 → [R5.27](../../tickets/197_R5.27__rights.md); REQ-F-007 → [R5.24](../../tickets/194_R5.24__r0.md); REQ-F-008 → [R5.23](../../tickets/193_R5.23__population-receipt.md); REQ-F-201 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-202 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-203 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-204 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-205 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-206 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-F-207 → [R5.27](../../tickets/197_R5.27__rights.md); REQ-F-208 → [R5.27](../../tickets/197_R5.27__rights.md) | specification covered; implementation/evidence unmet |
| OL-17 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-F-101 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-102 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-103 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-104 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-105 → [R5.45](../../tickets/219_R5.45__trace.md); REQ-F-106 → [R5.30](../../tickets/200_R5.30__ci.md); REQ-F-107 → [R5.30](../../tickets/200_R5.30__ci.md); REQ-F-108 → [R5.30](../../tickets/200_R5.30__ci.md) | specification covered; implementation/evidence unmet |
| OL-18 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-G-101 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-102 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-103 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-104 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-G-105 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-106 → [R5.50](../../tickets/229_R5.50__plugin.md); REQ-G-107 → [R5.53](../../tickets/232_R5.53__formative.md) | specification covered; implementation/evidence unmet |
| OL-19 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-B-005 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-B-006 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-B-007 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-201 → [R5.54](../../tickets/235_R5.54__publication-record.md); REQ-G-202 → [R5.54](../../tickets/235_R5.54__publication-record.md); REQ-G-203 → [R5.52](../../tickets/231_R5.52__outside.md); REQ-G-204 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-205 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-206 → [R5.55](../../tickets/237_R5.55__value.md); REQ-G-207 → [R5.54](../../tickets/235_R5.54__publication-record.md); REQ-G-208 → [R5.53](../../tickets/232_R5.53__formative.md); REQ-G-209 → [R5.54](../../tickets/235_R5.54__publication-record.md) | specification covered; implementation/evidence unmet |
| OL-20 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-103 → [R5.7](../../tickets/176_R5.7__adopt.md); REQ-C-104 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-C-105 → [R5.42](../../tickets/212_R5.42__quote.md); REQ-C-106 → [R5.42](../../tickets/212_R5.42__quote.md); REQ-C-107 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-E-210 → [REC5.2](../../tickets/243_REC5.2__rec2.md); REQ-F-207 → [R5.27](../../tickets/197_R5.27__rights.md); REQ-F-208 → [R5.27](../../tickets/197_R5.27__rights.md) | specification covered; implementation/evidence unmet |
| OL-21 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-H-001 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-004 → [R5.3](../../tickets/171_R5.3__adr-exec.md); REQ-H-006 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-007 → [R5.1](../../tickets/169_R5.1__guard.md) | specification covered; implementation/evidence unmet |
| OL-22 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-H-005 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-008 → [R5.2](../../tickets/170_R5.2__dispatch.md); REQ-H-011 → [R5.1](../../tickets/169_R5.1__guard.md) | specification covered; implementation/evidence unmet |
| OL-23 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-H-002 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-003 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-008 → [R5.2](../../tickets/170_R5.2__dispatch.md) | specification covered; implementation/evidence unmet |
| OL-24 | [brief/notes/spec mapping](../../OUTLINE_TRACE.md) | REQ-C-108 → [R5.1](../../tickets/169_R5.1__guard.md); REQ-H-008 → [R5.2](../../tickets/170_R5.2__dispatch.md); REQ-H-010 → [R5.40](../../tickets/210_R5.40__numerics.md); REQ-H-011 → [R5.1](../../tickets/169_R5.1__guard.md) | specification covered; implementation/evidence unmet |

## Dated note — 2026-11-02 REQ-E-202 / REQ-E-008 ownership

R5.14 run-time split: seq 183 lands support-only producer evidence for REQ-E-202 and REQ-E-008 (owner-model numbered checks); seq **183a** (`183a_R5.14b__pending-consumers.md`) is the discharging leg for both targets' full D/E predicates (consumer legs + step-6 agreement + report reproduction). Single primary ownership is preserved: no second ticket may stamp a satisfied claim for these targets.


## Dated note — 2026-11-02 REQ-E-202 / REQ-E-008 discharge

Seq **183a** (`183a_R5.14b__pending-consumers.md`, branch
`codex/harnessharness-evidence-r5.14b`) landed the consumer legs and is
the discharging leg the 2026-11-02 ownership note named: every declared
scored consumer composes the same `hh-experiment::disposition::
plan_disposition_at` (hh-results projection/query withhold +
`disposition` filter, export `foreign_leaderboard_submission` refusal +
`outcome.disposition` loss entries, hh-eval `EvalRun.disposition`
compare gate, hh-embed `lab.analysis.analyze` refusal; scheduler/close
landed seq 183 unchanged); step-6 agreement and old-watermark
reproduction are engineered in `tests/r5_14b.rs` batteries
(hh-results 7, hh-eval 3, hh-embed 2 — each compile-red at `6baa09e4`).
REQ-E-202 and REQ-E-008 record `met` for their engineered E-layer
predicates; the stamp covers the D/E legs only — owed R/P or
actual-execution evidence does not move. REQ-E-102 / REQ-E-103 /
REQ-E-201 / REQ-E-204 gain R5.14b as a **contributing-only** consumer
evidence row (the pending-consumer legs exercise freeze ordering,
regrade revision coherence, one-subject ownership and immutable
selection); their primary targets keep `planned; unmet` — this ticket
never marks a deferred full-target predicate MET.
## Dated note — 2026-11-02 REQ-D-107 ownership

R5.16 run-time split: seq 185 lands contract-scope (support-only) evidence for REQ-D-107 (estimator + dialect member); seq **185a** (`185a_R5.16b__beta-routing.md`) is the routed-scope discharge leg for the engineered D/E claim. R/P + actual-execution remain owed to GATE-G10/R5.40/R5.49 per the GATE-G9 disposition. Single primary ownership preserved.


## Dated note — 2026-11-02 REQ-D-107 routed-scope discharge

Seq **185a** (`185a_R5.16b__beta-routing.md`, branch
`codex/harnessharness-evidence-r5.16b`) landed the routing leg the
2026-11-02 ownership note named: kind `finite_manifest_contrast` routes
through `analyze` → `analyze_and_record` → persisted
`analysis_report_body/1` (hh-analysis kind arm + `AnalysisInput.view`
bound-experiment `ExperimentView` + `AnalysisError::Contrast` +
`record_kind`; hh-embed `analysis_ops` view fold + strict integer param
decode → `SchemaViolation`), consuming the seq-185 estimator contract
UNCHANGED — engineered in `tests/r5_16.rs` batteries (hh-analysis 4,
hh-embed 1 — each fails-without at `0b5fba98`). REQ-D-107 records `met`
for its engineered D/E routed-scope predicates; the stamp covers the
routed D/E legs only — R/P + actual-execution evidence stays owed to
GATE-G10/R5.40/R5.49 per the GATE-G9 disposition. REQ-D-008 /
REQ-D-109 / REQ-D-310 / REQ-E-209 / REQ-H-010 gain R5.16b as a
**contributing-only** routed-scope evidence row (the typed-refusal
surface, boundary strict decode and records-in view fold exercise
their clauses); their primary targets keep `planned; unmet` — this
ticket never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-02 (seq 186, R5.17): the early
> offline efficacy route is independently verified through
> `crates/hh-analysis/tests/r5_17.rs` — a scenario harness whose
> rollup can never count an unrun scenario, discrepancy, missing
> method metadata or unknown error as a pass — driving the landed
> `analyze`/`analyze_and_record` route end-to-end: EX-02 known answers
> re-derived as exact rationals (observed 200_000, posterior 142_857),
> the 95% credible interval reproduced by an independent SplitMix64 +
> integer-Beta order-statistic sampler over the persisted cells
> (declared tol 20_000 ppm), prior placement labels, unequal-n
> heterogeneity (equal-task 425_000 ≠ pooled), missingness refusals
> (unrun member → UnresolvedCell, n=4 → ShortCell), and persisted-
> report instrumentation (idempotent report_id, record.kind, S.1
> metadata complete, 8,000→64,000 doubling ladder, seed/tol,
> integer-canonical ppm members). REQ-D-109 / REQ-D-310 / REQ-E-209 /
> REQ-H-010 gain R5.17 as a **contributing-only** evidence row — the
> exploratory-prior parameterization defect it surfaced is recorded
> as DF-R5.17-1 OPEN (owed to R5.40/GATE-G10, never fixed in-place);
> the four targets keep `planned; unmet` — this ticket never marks a
> deferred full-target predicate MET and makes no numerical/study
> validation claim (GATE-G9 exclusion stands; owed to GATE-G10/R5.40).

> Dated contributing note — 2026-11-03 (seq 188, R5.18): the durable
> retained-selection slice landed through
> `crates/hh-context/src/retained.rs` (`retained_selection/1` — the
> additive `selection` member on `context.compaction.completed`,
> ADR-0396): the exact admitted policy/parameters
> (`CompactionStrategy::selection_parameters()` — `clear_tool_results`
> persists `{capabilities, older_than, completed_invocations}`, the
> completion-age join it consumed), retained/omitted content ids plus
> the pair/batch/required-group/tool joins, the shared `SelectionInput`
> representation the prepared call consumes, and rebuild equality —
> `replay_selection` re-runs the production `assess`/`propose`/`execute`
> path and `verify_selection` reports `Reproduced`/`Diverged`/typed
> `SelectionRefusal` (nine distinct refusals; `Untriggered`/`Applied`/
> `Ineffective`/`Failed` outcomes with `via_fallback` kept distinct).
> Engineered evidence in `crates/hh-context/tests/r5_18.rs` (6/6 —
> compile-red at base `3c07e282`), mapping §2.3's "persist … joins;
> rebuild from this durable selection" + dossier B.9's AC-2-2 checks
> 1–2 (policy→selection joins; unknown policies, no trigger,
> ineffective reduction and fallback keep explicit distinct results).
> REQ-E-206 and merged aliases REQ-E-003 / REQ-E-104 gain R5.18 as a
> **contributing-only** evidence row — achieved engineered D/E recorded
> separately from the owed legs: the retained-view→wire predicate
> (existing-renderer serialization of exactly the selected view, no
> omitted-item resurrection, capturing-transport policy→selection→
> assembly→sealed-request→actual-wire joins) stays owed to the
> targets' owner R5.20 AC-3; all three targets keep `planned; unmet` —
> this ticket never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 189, R5.19): the
> retained-view→wire call ordering landed through
> `crates/hh-gateway/src/prepared.rs` (`PreparedCall` — Packet B B.1
> item 5's fixed order / B.2's one gateway owner; ADR-0397): the owned
> `retained_selection/1` member serialized by the existing codec
> (`view_ref` idp address), every admitted primary role/overhead bound
> (five token primaries + `model_calls` required ≥ 1;
> `retries`/`time.wall_ms` admitted when declared; `estimate_tokens`
> input floor — `tokens.*.total` stay derived, never charged),
> whole-vector `Account::reserve` (R5.12 consumed unchanged), sealed
> plan carrying `budget_reservation_id` → `request_bytes`/`request_ref`,
> and the created-origin deadline (`created_at_ms` + tightest path
> `time.wall_ms` hard cap, recomputed on durable inputs — identical
> across resume/approval/drain). Dispatch gates deadline → sealed-byte
> verify → outstanding reservation → durable `model.call.requested`
> (once per `model_call_id`; `view_hash`/`plan_hash`/`request_ref`/
> `budget_reservation_id`) → bounded `run_attempts` over the prepared
> bytes (same reservation) → durable attempt/terminal rows →
> `Account::charge` per admitted primary with `reservation_id` +
> terminal-event source (idempotent dedup) / `release` / expiry drain.
> Engineered evidence in `crates/hh-gateway/tests/r5_19.rs` (9/9 —
> compile-red at base `5caeb304`), mapping §2's REQ-E-207 cap
> match/whole-vector/derived-total and REQ-D-006 enforce-dimensions
> predicates + dossier B.9's AC-2-3 checks 3–4 (every admitted cap;
> actual ≠ reserved; failed/ambiguous usage; logical reversal after
> real spend; cache/model drift; restart/approval delay; expiry with
> committed work; zero dispatch when unbounded/unfunded; no
> replenished physical capacity) and REQ-E-006's requested/served
> identity + role/binding record (`served_model_mismatch` under
> `substitution_allowed=false`). REQ-E-207 / REQ-E-006 / REQ-D-006
> and merged aliases REQ-E-005 / REQ-E-105 / REQ-E-106 gain R5.19 as
> a **contributing-only** evidence row — achieved engineered D/E
> recorded separately from the owed legs: fixture-vs-live
> enforcement separation, cold-start snapshot match, actual provider
> execution, and the R5.20 wire-join predicate stay owed to their
> named later owners (evidence dependency, never dispatch
> dependency); all six targets keep `planned; unmet` — this ticket
> never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 190, R5.20): the
> retained-view→wire integration landed through
> `crates/hh-embed/src/gateway_model.rs` (ADR-0398): the render
> consumes the *latest* durable `context.compaction.completed
> {selection}` member — strict `retained_selection/1` decode +
> `verify_selection` replay (R5.18 consumed unchanged) — and drops
> every recorded `omitted` `context_item_id` from the rendered
> observation join (one shared `hh.embed.context_item/1`
> derivation, `pub(crate)` constant — the assembler's mint and the
> render's join cannot diverge); an omitted pair's result renders
> the honest `no_observation_recorded` placeholder. A malformed,
> unknown-variant, unverifiable or divergent record is a typed
> `MalformedHistory` plan refusal — a strategy label without
> execution never dispatches. `view_hash` (the landed §5b.1 member)
> is stamped on `model.call.requested` when a selection informed
> the render — the consumed member's own idp address, identical to
> `prepare`'s `view_ref` derivation. Engineered evidence in
> `crates/hh-embed/tests/r5_20.rs` (5/5 — compile/assertion-red at
> base `f99d806c`), mapping dossier B.9's AC-2-2 checks 1–2
> (both admitted policies change the captured payload bytes and
> preserve required/legal groups; unknown policies, label-without-
> execution, fallback, retry, restart covered; exact
> policy→selection→assembly→sealed-request→actual-wire joins;
> no omitted-item resurrection on literal socket bytes) and
> REQ-A-007's port/capability pin (unlisted endpoint, unadmitted
> capability refuse at `prepare`). REQ-E-206 and merged aliases
> REQ-E-003 / REQ-E-104, and REQ-A-007, keep `planned; unmet` —
> achieved engineered D/E recorded separately from the owed legs:
> fixture-vs-live separation, actual provider execution, and the
> R/P legs stay owed to their named later owners (evidence
> dependency, never dispatch dependency); REQ-D-005 gains R5.20
> as a **contributing-only** evidence row (detector/payload joins
> proven fixture-side; its §4 metrics predicates stay R5.36's) —
> this ticket never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 191, R5.21): the
> `evidence-package/1` typed member/role profile landed inside
> `hh-bundle/1` (ADR-0399; `crates/hh-bundle/src/profile.rs` +
> `assemble`/`validate`/`manifest` seams): the admitted versioned
> record (R-group members required even when partial — absent is
> `missing_required_member`), `ProfileIndex` create-if-absent
> attempt/grade-attempt/export identities (identical bytes →
> `Retained`; same id different bytes → `conflicting_identity`),
> typed `native_heads`/`watermarks`/`exports[].last_event`
> snapshot coordinates the assembler verifies against the durable
> prefix it derives (never caller-asserted), the `check()` closed
> set (`grade_retry_adds_n`, `build_note_as_wal`,
> `head_mismatch`, `missing_head`, `watermark_mismatch`), and
> `unknown_profile_version` refusing a new claim while the bundle
> still decodes — legacy manifests validate byte-identically.
> Engineered evidence in `crates/hh-bundle/tests/r5_21.rs` (9/9 —
> compile-red at base `d9e2dce9`), mapping Packet C C.9 checks 2–3
> (same-day attempts/regrades/export collision retained as unique
> evidence; stale watermark/build-note-head substitution and
> wrong head/last exported event refused) and the §6.5 label leg
> (build-note never a native head). REQ-F-001/REQ-F-002 and
> REQ-A-101/REQ-A-103 keep `planned; unmet` — achieved engineered
> D/E recorded separately from the owed legs: owner-typed closure
> (R5.22), population receipt (R5.23), R0 (R5.24), recompute
> (R5.25), replay (R5.26), outside reviewer (R5.52), the C.9
> producer-inaccessible recipient legs and every R/P predicate —
> evidence dependency, never dispatch dependency; this ticket
> never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 192, R5.22): the
> owner-typed recipient closure landed inside `hh-bundle`
> (ADR-0400; `crates/hh-bundle/src/closure.rs` — one in-process
> interface `closure_walk(&Decoded, WalkOpts) → ClosureReport`,
> the `closure-report/1` sidecar codec): the owning codecs
> enumerate every root class — manifest members, section and
> run-manifest ordinary refs, `contains:` manifest recursion,
> `evidence-package/1` claim/coordinate roots, trace
> tree/pages/`blob_index`/head/checkpoint/lineage, event
> `causes`/`refs`/`ir_refs` and class-table `content_refs` — each
> edge carrying `{parent, role, field, media, size}` provenance +
> owner; visited dedup on that tuple; `ClosureLimits` bounds and
> expansion-stack cycle detection fail `traversal_limit`/`cycle`,
> never pass partially. `present` requires the recipient's own
> member bytes recomputing to the address (digest/size agreed);
> `fetch` materializes only through the landed `fetch::fetch`
> under a caller `FetchPermit` (verified arrival, never executed,
> refusals verbatim); `redacted`/`gc` withhold; nested `contains:`
> statuses resolve against their declaring index; no producer
> home/store/cache/credential fallback. Engineered evidence in
> `crates/hh-bundle/tests/r5_22.rs` (9/9 — compile-red at base
> `79b13533`), mapping Packet C C.9 checks 1/3/4 (the typed
> recipient traversal, truthful access/loss verdicts, and the
> producer-inaccessible materialization legs). REQ-F-005 keeps
> `planned; unmet` — achieved engineered D/E recorded separately
> from the owed legs: population receipt (R5.23), R0 (R5.24),
> recompute (R5.25), replay (R5.26), rights (R5.27), outside
> reviewer (R5.52), the §6.3 procedure cards and every R/P/
> actual-execution predicate — evidence dependency, never
> dispatch dependency. REQ-A-103/REQ-E-208 are stamped
> **contributing-only** — this ticket never marks a deferred
> full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 193, R5.23): the
> §6.1 all-planned reconcile landed inside `hh-analysis`
> (ADR-0401; `crates/hh-analysis/src/receipt.rs` — one in-process
> interface `assemble_receipt(&ExperimentView, &[EvalRun],
> &[TaskContext], metric_ref, pins, &DeclaredReceipt) →
> Result<PopulationReceipt, AnalysisError>`, the
> `population-receipt/1` canonical derived-index record): six
> set-equality legs `slots→plans→first/all
> attempts→settlements/selected grades→rows→analysis` over the
> pinned owner views, the expected side folded by the landed
> R5.15/15b population contract verbatim (`planned_population`,
> X7 — never a second rule, `PopulationError` refusals propagated),
> records-in declared side (frozen slots, per-plan attempts,
> settlement `{run, settlement_ref, cost_version}`, grade `{run,
> grade_ref, selection_ref, ordinal, counts_subject, status}`,
> row `{run, version_id, plan_ref}`, analysis
> `{protocol_ref, prices, method, seed, draws, weights, …}`),
> the closed fourteen-code set under `receipt::code`
> (`dropped_slot`/`extra_slot`/`orphan_launch`/
> `dropped_attempt`/`missing_settlement`/`missing_row`/
> `duplicate_grade`/`duplicate_record`/`grade_retry_adds_n`/
> `grade_status_violation`/`stale_selection`/
> `stale_cost_version`/`wrong_member_join`/
> `missing_analysis_pin`), four separate per-slot verdict axes
> (prerequisite/invocation/task/veto — a finished invocation never
> erases a skipped/format failure, a grade retry never counts
> subject n), the fixed `derived_index` label — no evidence-class
> promotion from hash or process success. Engineered evidence in
> `crates/hh-analysis/tests/r5_23.rs` (9/9 — compile-red at base
> `d5f6810a`), mapping Packet C C.9 checks 2/6 (the all-planned
> set-equality reconcile and the retained-not-run/refused/pending
> retention legs). REQ-E-208 (+ merged-with aliases
> REQ-E-010/108/109), REQ-F-003, REQ-F-004, REQ-F-008 and
> REQ-C-002 keep `planned; unmet` — achieved engineered D/E
> recorded separately from the owed legs: the named contributors
> R5.22 (landed), R5.24 and R5.43, and every R/P/actual-execution
> predicate — evidence dependency, never dispatch dependency.
> REQ-A-101 is stamped **contributing-only** — this ticket never
> marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 194, R5.24): the §9.3
> admitted *existing-contract fix* "R0 open/finished mapping
> (R-2.9.3)" landed inside `hh-embed` (ADR-0402;
> `crates/hh-embed/src/bundle_ops.rs` `kernel_reproduce` — the R0
> fold derives `open`/`finished` from `lifecycle.run.finished`
> presence, the producer's own rule, independent of the
> outcome-plane `payload.status` — A3 F6) plus the §6.3 recipient
> decode bounds inside `hh-bundle` (`crates/hh-bundle/src/codec.rs`
> `decode_dir` — `MemberEscape` on non-regular/`bundle.json` link
> entries, `MemberExpansion` beyond the declared index `size`, the
> native blob bound when undeclared; `BundleError` variants in
> `error.rs`). Engineered evidence in
> `crates/hh-embed/tests/r5_24.rs` (9/9 — the six R0 controls:
> `succeeded`/`failed_honest`→`finished`, `open`→`open`, changed
> bytes/`manifest_version` drift, absent member/`ledger_page_missing`,
> resealed wrong lifecycle/`lifecycle_status` drift — verdict fails
> at exit zero; link escape + expansion refuse; clean-recipient
> verifyfixed: producer store deleted, argument-file `validate`+R0
> pass over byte-identical delivered dir, old-fold derivation
> recorded as labelled diagnostic equivalent per §5.3 REQ-D-208 —
> the original A3 bytes are unavailable, never an original-rerun
> claim). REQ-F-007, REQ-A-204, REQ-C-003 and REQ-D-208 keep
> `planned; unmet` — achieved engineered D/E recorded separately
> from the owed legs: the named contributors R5.25 (recompute),
> R5.26 (replay), R5.45 (trace) and R5.52 (outside review), and
> every R/P/actual-execution predicate — evidence dependency, never
> dispatch dependency. REQ-A-103, REQ-C-007, REQ-E-208 (+ aliases
> REQ-E-010/108/109), REQ-F-201, REQ-F-202, REQ-G-101/102/103/104/
> 105/107 and REQ-G-203 are stamped **contributing-only** — this
> ticket never marks a deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 195, R5.25): the §6.3
> Recompute procedure landed inside `hh-analysis` (ADR-0403;
> `crates/hh-analysis/src/recompute.rs` — one in-process owner
> interface `recompute_analysis(&RecomputePlan, &AnalysisInput) →
> Result<RecomputeReport, AnalysisError>`, the derived
> `analysis-recompute/1` record). Legs are the §6.3 row verbatim:
> `verify_inputs` (withheld/malformed/nonfinite refusals; declared
> `spec_ref`/`prices`/`method`/`weights`/`seed`/`draws` member-wise
> against the frozen reference and the rebuilt input;
> `generated_from` + `report_id` recomputed exactly — no rerun under
> altered selector/price/weight/family/draw pins) → `population`
> (frozen grade selection + all-planned reconcile through the
> landed `assemble_receipt`; non-`closed` is a verify-first
> `SelectionNotClosed` refusal, never a drift) → `rerun` (the one
> `analyze` kernel; `UnknownMethod`/`UnsupportedPrior`/
> `NonFiniteInput` refusals verbatim) → `compare` (identities,
> membership/order, counts/missingness, categorical verdicts exact;
> estimate scalars under the frozen `abs(new−reference) ≤ 1e−10 ×
> max(1,abs(reference))` tolerance; the closed exact-member set —
> pins/counts/ordinals/levels — never rides it: a threshold/verdict
> change fails even inside tolerance). `reproduced` is arithmetic
> agreement only — never a hidden-history/oracle/R-level claim.
> Engineered evidence in `crates/hh-analysis/tests/r5_25.rs`
> (13/13 — compile-red at base `924ec646`): wrong
> selector/price/weight/family/draw, withheld input, unsupported
> estimator and nonfinite refusals; verdict/threshold/scalar/
> identity/order drifts fail the verdict at exit zero; unclosed
> selection and declared-side drift refuse. Deviation: 1,468
> changed source/test lines vs ≤700 — AMEND ruling f2dd2fac
> (`planning/2026-11-03_R5.25-split-review.md`). REQ-F-203,
> REQ-F-005, REQ-F-007, REQ-C-003 and REQ-A-103 keep `planned;
> unmet` — achieved engineered D/E recorded separately from the
> owed legs: the named contributors R5.26 (replay), R5.28 (recipe/
> CLI), R5.45 (trace) and R5.52 (outside review), and every
> R/P/actual-execution predicate — evidence dependency, never
> dispatch dependency. All five are stamped **contributing-only** —
> this ticket never marks a deferred full-target predicate MET.
> Dated contributing note — 2026-11-03 (seq 196, R5.26): the §6.3
> isolated-replay and fresh-execution admission cards landed inside
> `hh-bundle` (ADR-0404; `crates/hh-bundle/src/cards.rs` — two
> records-in/records-out owner interfaces beside `closure.rs`,
> `admit_replay(&Decoded, run_id, &ReplayAttempt) →
> Result<AdmissionCard, BundleError>` and
> `admit_fresh(&Decoded, run_id, &FreshClaim) →
> Result<AdmissionCard, BundleError>`; `replay-card/1` and
> `fresh-card/1` records, `idp/1`-sealed — nothing executes, the
> live replay driver stays in `hh-embed`). `admit_replay`'s legs
> are the §6.3 row verbatim: `inputs` (definition + compiled_bundle
> member bytes, recorded response rows, a declared
> `control.decision`, both isolation flags — typed `CardRefused`)
> → `prefix` (source hash/chain recompute + copy prefix
> envelope-exact — `original_mutation`) → `suffix` (only
> `lifecycle.replay.*` markers may append continuing the chain;
> other classes `replay_suffix`) → `counters` (the card recounts
> appended `model.call.*`/`action.effect.*` classes itself —
> declared counts must agree; any nonzero attempt is
> `attempted_live_dispatch` even when the transport blocked it) →
> `decisions` (replayed `control.decision`/`context.assembled`
> sequences exact — `changed_derivation`) → `rederivation` (R1:
> `version_id` + envelope hashes + `present` member addresses + the
> run's ledger `tree` each recomputed from authoritative bytes;
> `n_a{not_claimed}` when not claimed). `admit_fresh` refuses
> `unfunded`/`unavailable{model_retired}`/`not_fresh`/
> `charging_attribution`, the per-level required inputs (R2
> seeds/margins/validators; R3 seed_distribution/MatchSpec/
> margins/seeds; transfer outside_relationship/no_new_phase/
> comparison/new_question) and `not_independent` through the
> landed `compute_independence` — an author-only clean directory
> admits nothing — then evaluates `native_basis` through
> `levels::derive` and `arms` (n ≥ k ≥ 2: one fresh demo awards
> no level; R3 not cumulative R2). Cards stay separate —
> `procedure`/`level` members with `prerequisites`/`resources`/
> `counters`/`independence` recorded apart from `verdict`.
> Engineered evidence in `crates/hh-bundle/tests/r5_26.rs`
> (20/20 — compile-red at base `d5f355fb`): honest R1 and non-R1
> replays admit, R2/R3/transfer admit, separation proven;
> missing inputs, undeclared replay, enabled isolation, mutated/
> truncated prefix, non-marker suffix, attempted live dispatch
> (declared blocked or not), changed derivation, R1 gaps, every
> fresh gate, R3 blocks, transfer independence gates and the
> unpinned-model basis all refuse or fail as typed. Deviation:
> 1,823 changed source/test lines vs ≤750 — AMEND ruling 7f0fbe76
> (`planning/2026-11-03_R5.26-split-review.md`). REQ-A-005,
> REQ-A-105, REQ-B-104, REQ-D-010 and REQ-D-202 keep `planned;
> unmet` — achieved engineered D/E recorded separately from the
> owed legs: the named contributor R5.45 (trace) and every
> R/P/actual-execution predicate — evidence dependency, never
> dispatch dependency; this ticket never marks a deferred
> full-target predicate MET. REQ-A-103, REQ-C-003, REQ-F-005,
> REQ-F-007, REQ-F-204 and REQ-F-205 are stamped
> **contributing-only** — the same rule.

> Dated contributing note — 2026-11-03 (seq 197, R5.27): the §6.4
> rights, losses, quotas and recipient handoff validation landed
> inside `hh-bundle` (ADR-0405; `crates/hh-bundle/src/rights.rs` —
> two records-in/records-out owner interfaces beside
> `closure.rs`/`cards.rs`,
> `admit_handoff(&Decoded, &HandoffPlan, now_ms) →
> Result<HandoffReport, BundleError>` and
> `sanitize_replace(&Decoded, member, new_bytes,
> &SanitizeTransform) → Result<SanitizedItem, BundleError>`;
> `rights-record/1` per-item binding inputs, `handoff-report/1` +
> `rights-tombstone/1` + `rights-supersession/1` records,
> `idp/1`-sealed — nothing executes or materializes).
> `admit_handoff`'s legs are the §6.4 clause verbatim:
> `authority` (`optional_hosting` scope or an
> `s3:`/`account:`/`http(s)://` host → `separate_authority` —
> provisioning needs its own operator decision) → `metadata`
> (contact/region bound; `quoted_at`/`decided_at` parse and are ≤
> now — `missing_input`/`future_date`; `review_before` must
> parse) → `grant` (`access_grant` only — ≤30 days, links ≤1 h →
> `quota_breach`; write/delete/list → `grant_overreach`) →
> `quotas` (compressed/expanded/first-wave declarations vs the
> scope ceilings — `quota_breach`, never discard to fit); the
> per-item loop records withheld/refused rows — unknown member,
> absent rights record or `decision: unknown` → withheld
> `unknown_rights`; `withheld` → withheld; `restricted` in
> `public_review` → refused `scope_mismatch` (controlled closure
> is not public closure); restricted/release without the named
> recipient → withheld `unauthorized_recipient`; record expiry ≤
> now → withheld `fetch_expired`; held-out
> `solution`/`oracle`/`model_io` roles without license/consent
> evidence → withheld — default restricted; over-ceiling asset →
> `quota_breach` refusal; `present` bytes scanned —
> `hharness_secrets::detect` → `secret_material`, a declared
> restricted identifier → `sensitive_identifier` (hard refusals);
> `fetch` members re-admit each location leg through
> `trust::admit` (`fetch_expired`/`unauthorized_fetch`);
> `redacted`/`gc` withhold by status. Departures need a real
> disposition, evidence and an actual date ≤ now — a bare marker
> is `marker_not_deletion`; each emits `rights-tombstone/1` plus
> the role→procedure `disabled_procedures` set. Verdict
> `admitted`/`withheld`/`refused`; `retention` records
> `effective_until` = min(earliest item expiry, declared
> `review_before`) — shorter rights override honestly.
> `sanitize_replace` requires kind/version +
> `mapping_permitted` (`unpermitted_mapping` refuses), scans the
> new bytes, mints a NEW `hh-identity` address (identical →
> `unchanged_bytes` — changed bytes never retain native
> hashes/R1), recalculates dependent claims `{field, from, to}`
> and emits `{procedure, native_identity, R0}` losses plus the
> sealed supersession. Engineered evidence in
> `crates/hh-bundle/tests/r5_27.rs` (18/18 — compile-red at base
> `bc446fb7`): public/controlled/fetch admits, tombstoned
> departures disabling dependent procedures, shorter-rights
> retention, sanitize new-identity/supersession/recalculation;
> every AC-2 refusal typed — unknown/withheld rights, secret or
> sensitive bytes, unauthorized/expired fetch, quota/expansion
> breach, marker-only deletion, restricted-in-public and
> controlled-as-public, optional hosting/S3 authority,
> missing/future dates, grant ceilings/overreach, held-out
> defaults, sanitize refusals. Deviation: 1,502 changed
> source/test lines vs ≤650 — AMEND ruling 963ff8f5
> (`planning/2026-11-03_R5.27-split-review.md`). REQ-F-006,
> REQ-F-207 and REQ-F-208 keep `planned; unmet` — achieved
> engineered D/E recorded separately from the owed legs: the
> named contributor R5.52 (outside review) and every
> R/P/actual-execution predicate — evidence dependency, never
> dispatch dependency; this ticket never marks a deferred
> full-target predicate MET. REQ-F-201, REQ-F-202, REQ-G-104 and
> REQ-G-203 are stamped **contributing-only** — the same rule.

> Dated contributing note — 2026-11-03 (seq 198, R5.28): the §2.1
> one supported native recipe and the §7.1 exact CLI forwarding
> landed (ADR-0406; `crates/hharness/src/recipe.rs` —
> `hh-recipe/1`, `run_recipe(&mut EmbedService, &RecipePlan) →
> Result<RecipeReceipt, RecipeError>` — records-in/records-out
> over binding (a)'s `handle` plus the owner seams; no public RPC,
> no second runner/store/codec, no `hharness → hharness-cli`
> edge). `RecipePlan` is inputs-only over a closed member set —
> authored records (`slots`/`attempts`/`rows`/`metrics`/
> `analysis.rows`/…) refuse `unsupported_shape`, absent required
> members `missing_field`, wrong types `type_mismatch`. The phase
> chain composes the landed owners verbatim: `hello` →
> `lab.experiment.{register, expand, open_experiment}` → bounded
> `next`/`claim`/`launch` drain (`drain_stalled`) → `adopt_subject`
> (`launch_binding` re-folded durable through `ExperimentView::fold`,
> the launch's `subject_writer` re-presented) → `submit` driving the
> scripted `EmbedModel` lane → the durable evidence gate
> (`lifecycle.run.finished` + observed `hh.submit` submission ref —
> `execution_incomplete`/`no_submission`; process-only success never
> grades) → the declared grade committed `measurement.metric.emitted`
> through `surface_commit_row` under the persisted writer generation
> (`oracle_ref = hh-recipe/1`, `evidence_ref` = the submission ref;
> a bound slot with no entry → `grade_unspecified`) → `settle` →
> `close` → `ResultsStore::project_and_record` →
> `lab.analysis.analyze` (`rows[]` = the projected row key ids,
> never supplied) → `drain_notifications` verbatim or
> `n/a{no_callback_channels}` → `assemble_receipt` over the folded
> view + `project_runs` EvalRuns + an actual-fact `DeclaredReceipt`
> (settlement/grade/row refs are the durable event/version ids).
> `RecipeReceipt` records the negotiated `hello` verbatim, every
> `{method, outcome}` request, every `{seam, outcome}` owner call,
> per-slot coordinates and the `population-receipt/1`. The K-2 CLI
> repair forwards `run_plan_id`/`lease_id`/`holder`/
> `subject_holder`/`partial`/`bundle_id`/`reason`/`probe` verbatim
> on `experiment` ops (absent flags stay absent — the boundary owns
> required-member refusal); `compare report` takes the canonical
> analyze params-file then `render{report_id, view:"report"}`;
> `compare scorecard <report-id>` → `render{report_id,
> view:"scorecard"}` — the `{baseline,candidate}`/`{view,
> experiment_id}` wrapper shapes removed (unsupported inventions;
> the §9.3 existing-contract-fix class). Engineered evidence in
> `crates/hharness/tests/r5_28.rs` (5/5 — compile-red at base
> `a2bed920`): the full recipe reconciles `closed` on a scratch
> store; authored/unknown members, missing required members,
> process-only execution and an unspecified bound-slot grade all
> refuse typed; `crates/hh-cli/tests/in_process.rs` boundary-shape
> regressions (2 — assertion-red at base) pin the exact forwarded
> members. Deviation: 2,005 changed source/test lines vs ≤850 —
> AMEND ruling `6ff01462`
> (`planning/2026-11-03_R5.28-split-review.md`). REQ-E-009,
> REQ-A-002, REQ-A-202 and REQ-A-106 keep `planned; unmet` —
> achieved engineered D/E recorded separately from the owed legs:
> the named contributors R5.50 (plugin) and R5.45 (trace) and every
> R/P/actual-execution predicate — evidence dependency, never
> dispatch dependency; this ticket never marks a deferred
> full-target predicate MET. REQ-A-201, REQ-B-002, REQ-F-201,
> REQ-F-202, REQ-G-001, REQ-G-003, REQ-G-005, REQ-G-008, REQ-G-009,
> REQ-G-104 and REQ-G-203 are stamped **contributing-only** — the
> same rule.

> Dated contributing note — 2026-11-03 (seq 199, R5.29): the §7.2
> early offline examples and the informative Lab lesson landed
> (ADR-0407; `crates/hharness/src/examples.rs` — `example-card/1`
> versioned committed cards under `scripts/examples/` +
> `run_example(&mut EmbedService, &ExampleCard, …) →
> Result<ExampleReport, ExampleError>`, runner version
> `hh-examples/1` — records-in/records-out over binding (a)'s
> `handle` plus the landed owner seams; no public RPC, no second
> runner/store/codec). Card decode is a closed member set —
> unknown members `unsupported_shape`, absent required
> `missing_field`, wrong types `type_mismatch`, unadmitted `case`
> `unknown_case`; every declared `sha256:` pin recomputes over the
> pinned file's bytes and drift refuses `stale_pin` **before any
> execution**. EX-01 drives `hello` → `open_session` (the pinned
> fixture definition) → `submit{hh.submit}` → attach by the
> returned run id → six excerpt checks resolving actual event
> id/seq/payload/causes → `kernel.bundle` → clean-recipient
> `check_completeness` + `kernel.reproduce` R0 `reproduced` (a
> fresh service on its own scratch store — no producer fallback)
> → `admit_replay` (typed `CardRefused{missing_input}` recorded
> verbatim as `expected_refusal` — the scripted lane mints no
> `compiled_bundle`; tampered-copy `not_admitted` control). EX-02
> drives one `run_recipe` per task over the pinned
> `ex-02-fixture/1` (2 experiments × 2 arms × 5 replicates = 20
> subjects — a shared grade vector would be the constant fixture
> AC-2 refuses), both `population-receipt/1` `closed`; durable
> rows re-read through fresh `Store`/`ResultsStore`/`LabDocs`
> handles feed the merged `ExperimentView` → `analyze_and_record`
> `finite_manifest_contrast` `beta{1,1}` → persisted
> `analysis_report_body/1` (idempotent `report_id`) →
> `DeclaredReceipt` + `recompute_analysis` `reproduced`. Semantic
> checks `subjects_20`/`cells_pinned`/`informative_contrast`/
> `report_identity`/`recipient_recompute`/`exploratory_label`/
> `refusal_controls` — `AnalysisStatus::Exploratory` only, a
> configured sample/budget never becomes a confirmation default.
> Engineered evidence `crates/hharness/tests/r5_29.rs` 7/7 —
> compile/missing-file red at base `4dd6159e`; negatives: stale
> pin, wrong count (independent `expected` pin), constant fixture,
> withheld recompute input (typed `Withheld` — never a producer-
> side fallback), card decode refusals, expected-refusal controls
> verbatim. Deviation: 2,564 changed source/test lines vs ≤850
> (~3.0× — new build high-water mark) — AMEND ruling `ff31b141`
> (`planning/2026-11-03_R5.29-split-review.md`). REQ-A-008 and
> REQ-F-101…105 keep `planned; unmet` — achieved engineered D/E
> recorded separately from the owed legs: primary owner R5.45's
> AC-3 plus every R/P/actual-execution predicate — evidence
> dependency, never dispatch dependency; this ticket never marks
> a deferred full-target predicate MET. All six ids are stamped
> **contributing-only** — the same rule.

> Dated primary-owner note — 2026-11-03 (seq 200, R5.30): the §7.3
> CI-and-refresh lanes landed (ADR-0408). REQ-F-106 (both-mode
> equivalence / full execution under ADR-0362): N1 —
> `scripts/check-examples.sh` decodes every committed
> `example-card/1` against the contract extracted from the canonical
> runner (`CARD_MEMBERS`, the admitted `case` set — no second
> table), recomputes each `sha256:` pin over the pinned bytes,
> resolves every declared check id to a runner-emitted literal and
> refuses unpinned committed fixture data; N2 — displayed↔canonical
> equivalence between the `docs/getting-started.md`
> `<!-- examples:… -->` block / `scripts/examples/README.md` and the
> cards' `run` members; N3 — the `ci.yml` `ci-mode: both` step plus
> the gated `mode != 'fast'` ubuntu leg running
> `scripts/run-examples.sh` (`cargo test -p hharness --test r5_29`,
> scratch stores, canonical inputs only — the gated-step readers
> scan enforces zero committed docs/research reads).
> REQ-F-107 (separate working-tree/HEAD/live checks): N4 —
> `check-examples.sh` (working tree) vs `--head` (clean clone of
> committed HEAD; uncommitted edits are never the tested head);
> N5 — the live lane recorded `skipped(…)` on every run, never a
> pass. REQ-F-108 (code/freshness/study refresh with supersession):
> N6 — MAINTAINING.md "Example checks and refresh": the three check
> classes, the trigger→action table, card-version supersession,
> preserved historical negatives and the 30-day refresh-due rule.
> Evidence `scripts/ci/test-check-examples.sh` 14/14 — missing-file
> red at base `c989088a`; negatives: extra/missing member,
> unadmitted case, drifted sha256, edited pinned file, stale or
> absent displayed block, runner-unmapped check id, stray fixture
> data, working-tree dirt invisible to `--head`, committed drift
> failing `--head`, and the stripped `ci-mode: both` annotation
> failing `check-ci-gating.sh`. Achieved **engineered D/E**;
> achieved layer `engineered` (self-tested lint + fixture drives —
> the actual CI execution on pushed heads is read at close per
> O-06). Owed: none beyond this ticket and the common closure audit
> — full-target R/P predicates per the register; this note does not
> mark REQ-F-106/107/108 fully satisfied.

> Dated primary-owner note — 2026-11-03 (seq 201, R5.31): the §8.1
> ordinary contribution and failure-report routes landed (ADR-0409).
> REQ-A-205 (preserves R-2.12.2/R-2.11.* ABI/client/corpus limits):
> N1 — the routes reuse CONTRIBUTING, the existing issue/PR forms and
> the private security route only; no new platform, RPC, schema,
> dependency or event class; `commands_real` binds every cited `hh`
> command to the real `cli.rs` dispatch arms, never a second
> interface. REQ-B-004 (preserves CONTRIBUTING/GOVERNANCE ordinary
> PR and operator merge): N2 — `lanes`/`no_run`/`receipt`/`privacy`
> check the separate no-run bug/setup and receipt lanes with their
> field sets and the secrets/rights inspection; N3 — `verifyfixed`
> pins the failed-receipt→candidate disposition (original preserved,
> regression fails without, unable-to-reproduce ≠ fixed, distinct
> verifier classes); N4 — `no_ritual`/`blocked`/`codeowners` keep
> ordinary PRs free of the internal question register/build
> ledger/manifest ritual while `* @SteveVitali` review and the
> manifest-ticket conditional stay intact (the spec's
> governance-preserving reconcile); N5 — `gated` keeps the optional
> packaged component and scientific replication explicitly
> unopened; N6 — `form_fields` mirrors the documented fields into
> `bug_report.yml` (exit code/signal, dirty state, toolchain/
> features/backend/prerequisites, receipt-or-loss, required
> sanitization attestation) with `config.yml`'s private security
> route kept. Evidence `scripts/ci/test-check-contributing.sh`
> 20/20 — `check-contributing.sh --head` red at base `928e9287`
> (10/12 legs: every lane/field/privacy/verifyfixed/ritual/gated/
> form/blocked/codeowners leg absent); negatives: lane-heading
> drift, dropped exit-code field, fictitious `hh bug-bundle`,
> removed secrets block, removed loss-reason rule, ungated
> component lane, removed issue-form field/attestation, restored
> broad CODEOWNERS wording, dropped owner line, manifest machinery
> on the ordinary path, broken link, working-tree dirt invisible
> to `--head`, committed drift failing `--head`, stripped
> `ci-mode: both` annotation failing `check-ci-gating.sh`.
> Achieved **engineered D/E**; achieved layer `engineered`
> (self-tested lint + fixture drives — actual CI execution on
> pushed heads read at close per O-06). Owed: full-target R/P and
> actual-execution predicates per the register — R5.50 owns the
> packaged-component predicate, R5.52 the outside-review/
> replication contributor evidence, R5.53 the participant
> observations for the contributing-only ids (evidence dependency,
> never dispatch); this note does not mark REQ-A-205/REQ-B-004
> fully satisfied. REQ-C-007, REQ-G-008, REQ-G-101/102/103/105/107
> are stamped **contributing-only** — the same rule.

> Dated contributing note — 2026-11-03 (seq 202, R5.32): the
> §5.2/D3-F3 confined observer apparatus and Kp4 controls landed
> through `crates/hh-env/src/ops.rs` (`pub mod ops`, ADR-0410) —
> `TrialMarker`/`MarkerSink` (the durable append-only marker log
> standing in for the external world: one fixed non-idempotent
> append, flush+fsync, disk re-read `markers()`/`count_for`/
> `duplicates()`/`digest()`), `Sentinel` (denied/read-only target,
> digest-pinned `intact()`), `OpsObserver` (independently surviving
> `observer.jsonl` on its own path — re-open retains rows,
> `before_launch`/`at_boundary`/`after_quiescence` snapshots),
> the probe vocabulary (`ProbeSource`/`RecordedProbe`/
> `probe_authorization` — `ExternalSinkZero` inadmissible, terminal
> `Refused`), and the pure verdict folds (`o1_outcome`,
> `o8_outcome`, `kp4_outcome`; closed `TrialFailure` set;
> `defect_detected` ≠ `pass`; `Kp4Outcome` retains `Unknown*`,
> never `pass`). Engineered evidence in
> `crates/hh-env/tests/r5_32.rs` (7/7 — compile-red at base
> `08838f54`) driving the real Store/WAL, Π `Monitor` allow rows,
> `Dispatcher` and the R5.8 `inject_kill_point` seam: O1 allowed
> control passes with complete evidence (one marker, matching
> digests, three intervals, intact sentinel); O2 kp-4 injected →
> boundary sink delta 0 → new-fence restore →
> `unknown{worker_lost}` + probe obligation, repeated restoration
> adds no commit/dispatch, outcome retained unknown; O8 seeded
> duplicate `defect_detected` 3/3; observer record survives
> subject death; the AC-2 negative matrix (missing observer,
> unreached fault, duplicate restoration, stale fence, no valid
> probe, undeterminable, sentinel mutation, external-zero
> inadmissible) cannot pass. REQ-B-105 / REQ-C-006 / REQ-D-203 /
> REQ-D-204 / REQ-D-205 / REQ-D-206 / REQ-D-207 / REQ-E-012 /
> REQ-E-110 / REQ-E-205 gain R5.32 as a **contributing-only**
> evidence row — achieved engineered D/E recorded separately from
> the owed legs: the O×3 real-trial / R / P / actual-execution
> predicates stay owed to their primary owner R5.44 AC-1 (evidence
> dependency, never dispatch); all ten targets keep
> `planned; unmet` — this ticket never marks a deferred
> full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 203, R5.33): the real
> permission wait/answer/resume matrix landed through the monitor's
> fold-level **offered-answer gate** (ADR-0411 —
> `crates/hh-monitor/src/approval.rs`): `ApprovalState::fold` now
> folds the durable `request.options[]` ids (ADR-0343 D4 — absent
> member folds empty, pre-R2.11 rows ungated; unparseable id drops,
> never widens) and `respond_inner` refuses a conferring/
> option-consuming answer outside the durable offer with typed
> `ApprovalError::UnofferedOption{option_id}` — pending restored,
> never decided, never minted (`deny`/`cancelled` exempt — a refusal
> confers nothing; the wire's deny spellings predate the closed id
> set); `remedy` checked against `request.remedies`. The env O4/O5
> cells gained the closed `PermissionFailure` set and the pure
> `o4_outcome`/`o5_outcome` folds in `crates/hh-env/src/ops.rs`
> (read-only, ADR-0410 confinement unchanged). Engineered evidence
> in `crates/hh-env/tests/r5_33.rs` (4/4) driving the real
> Store/WAL, Π `Monitor`, `Dispatcher` irreversible
> `PermissionRequest` ask, `MarkerSink`/`OpsObserver`/`Sentinel`,
> real `respond_with_ctx` + `decided` mint under the fenced writer,
> `deliver_wakeup` (occurred→fired) and same-input re-dispatch:
> O4 allow — durable pending carrying the offered set, two clean
> no-mutation intervals, restart-preserved pending, offered
> authority-checked `allow_once` → `decided` → wakeup → resume on
> the same effect with exactly one marker + matching digest,
> idempotent identical re-answer, conflicting new-key refusal; O5
> deny — offered `deny` → `decided{deny}` → refused resume, zero
> prepare/commit/entry/sink delta, conflicting later answer cannot
> authorize; unoffered (`allow_lease`/`modify`/`more_info`/
> `remedy` outside the offer) and unauthorized (delegate endorser,
> uncovered grant) answers refuse typed and mutate nothing; the
> paused `input_required` label alone is no permission wait (the
> effect may already be committed). REQ-B-105 / REQ-C-006 /
> REQ-D-203 / REQ-D-204 / REQ-D-205 / REQ-D-206 / REQ-D-207 /
> REQ-E-012 / REQ-E-110 / REQ-E-205 gain R5.33 as a
> **contributing-only** evidence row — achieved engineered D/E
> recorded separately from the owed legs: the O×3 real-trial /
> R / P / actual-execution predicates stay owed to their primary
> owner R5.44 AC-1 (evidence dependency, never dispatch); all ten
> targets keep `planned; unmet` — this ticket never marks a
> deferred full-target predicate MET.

> Dated contributing note — 2026-11-03 (seq 204, R5.34): the
> Account refusal/exhaustion cells and the §5.3 stage-count report
> machinery landed as the read-only instrument's third packet
> (ADR-0412 — `crates/hh-env/src/ops.rs`): the closed
> `AccountFailure` set + `AccountOutcome` with the pure
> `o6_outcome`/`o7_outcome` folds — O6 requires the identical O1
> positive path demonstrated, the starved `model_calls` reserve
> refused durably (`reserved{refused}`/`InsufficientBudget`), zero
> independently counted transport entries, no dependent mutation and
> no terminal-exhaustion spelling; O7 requires a committed fact
> behind a declared bounded barrier (bound+interval+fail-safe before
> execution, grace=0), expiry-before-commit checked first as the
> unreachable precondition, a durable-origin deadline actually
> reached, new dispatch blocked, drain to released/unknown,
> exceeded+stop landed, no later marker, no duplicate charge and
> committed work preserved on rebuild — plus the report machinery
> (`OpsCell`/`OPS_CELLS`, frozen seeded 8×3 permutation, retained
> `TrialOutcome` kinds, append-only `TrialRecord`s refusing
> `ReplacedOriginal`, ≤1 new-identity setup rerun, ppm ratios with
> zero-denominator `undefined`, the `[V/P,(V+U)/P]` missingness
> bound, O8 `detected/3` separate, failed-control/integrity-stop
> acceptance blocks, `ops_report/1` render). Engineered evidence in
> `crates/hh-env/tests/r5_34.rs` (7/7) over the real Store/WAL,
> `ManualClock`, real `Account` (root + instrument budgets), the
> landed `prepared` path behind a counting diagnostic `Transport`
> (hh-gateway dev-dep — fixture only, dev-deps do not ship) and
> MarkerSink/Sentinel/OpsObserver: O6 zero-model_calls refusal
> before transport, O7 wall-expiry drain-then-stop with
> rebuild/reopen preservation and dedup, both negative matrices,
> expiry-before-commit unreachable, and the report's counts/bounds/
> O8/replaced-original/excess-retry/failed-control legs.
> REQ-B-105 / REQ-C-006 / REQ-D-203 / REQ-D-204 / REQ-D-205 /
> REQ-D-206 / REQ-D-207 / REQ-E-012 / REQ-E-110 / REQ-E-205 gain
> R5.34 as a **contributing-only** evidence row — achieved
> engineered D/E recorded separately from the owed legs: the O×3
> real-trial / R / P / actual-execution predicates stay owed to
> their primary owner R5.44 AC-1 (evidence dependency, never
> dispatch); all ten targets keep `planned; unmet` — this ticket
> never marks a deferred full-target predicate MET.

> Dated primary-owner note — 2026-11-03 (seq 205, R5.35): the §7.1
> reader spine, evidence cards and portfolio positioning landed
> (ADR-0413). REQ-B-001 (one selected audience/job, prerequisites,
> secondary roles, first-wave exclusions, inferred-demand label): N1 —
> `docs/evidence.md` names the technically prepared harness evaluator
> job with prerequisites, secondary roles, the deliberate exclusions
> and the explicit demand-is-a-hypothesis label; the `portfolio` leg
> of `scripts/check-position.sh` fails if the job or the label is
> dropped. REQ-C-008 (preserve unmet objectives + recorded rescope):
> N2 — the `<!-- unmet -->` partial-release clause names S1/S3 unmet,
> cites the GATE-G8/G9/G10 dispositions and the partition, and forbids
> fixture substitution; the `unmet` leg pins it. REQ-C-101 (separate
> task/teaching/study gates; checked-task minimum retained): N3 —
> eleven `### Card —` records carry the closed four-state vocabulary;
> the `gates` leg requires useful-task/study/operations =
> `Not yet available`, component/interchange/evidence-access =
> `Restricted`, setup/EX-01/EX-02/receipt/contribute = `Available`;
> the `labels` leg refuses invented states. REQ-C-102 (preserve 22
> families / nine later groups / optional ceilings / explicit
> disposition changes — merged REQ-A-001 coverage table + REQ-C-001
> primary job): N4 — the maintained capability table retains all 22
> family names with leading disposition class, all nine L-groups, all
> six scenarios and the one job; `portfolio` derives the canon from
> the frozen supplement §1 tables — a silent deletion or flipped
> disposition fails; N5 — the `sources` leg requires the
> benefit/overlap/exclusion/revisit claim map to resolve to real
> dated sources; N6 — `entries`/`spine_page`/`nav` pin the three
> entry choices (readiness first, failure/report exit) in README and
> docs/evidence.md plus the docs/README rows; `honesty` bans
> favorable-treatment/scientific-claim phrases on the surface.
> Evidence `scripts/ci/test-check-position.sh` 20/20 —
> `check-position.sh --head` red at base `bfc0b508` (entries/
> spine_page/nav legs: page and spine absent); negatives: entry
> renamed, exit removed, invented state, fixture-substitution
> (useful-task marked available), dropped field, family/later/
> scenario deleted, job unnamed, demand label dropped, unmet clause
> weakened, nonexistent source, banned phrase, nav drift,
> working-tree dirt invisible to `--head`, committed drift failing
> `--head`, stripped `ci-mode: both` annotation failing
> check-ci-gating. Achieved **engineered D/E**; achieved layer
> `engineered` (self-tested lint + fixture drives — actual CI
> execution on pushed heads read at close per O-06). Owed: the R/P
> and actual-execution predicates stay with the register — S1's real
> task/study evidence (R5.43 skeleton under GATE-G11), S3 collection
> (later numeric/protocol owners + funding), the §8 participant
> records (R5.53) and publication record (R5.54) for the
> contributing-only ids (evidence dependency, never dispatch); this
> note does not mark REQ-B-001/REQ-C-008/REQ-C-101/REQ-C-102 fully
> satisfied. REQ-A-201, REQ-B-002, REQ-B-107, REQ-G-001, REQ-G-003,
> REQ-G-005, REQ-G-008, REQ-G-009, REQ-G-201, REQ-G-202, REQ-G-207,
> REQ-G-209 are stamped **contributing-only** — the same rule.
> Dated primary-owner note — 2026-11-03 (seq 206, R5.36): the §4.2
> mechanism joins and versioned denominator definitions landed
> (ADR-0414). REQ-D-005 (policy-to-payload and versioned
> detector/rates): N1 — `MECHANISM_REGISTRY`
> (crates/hh-eval/src/mechanism.rs) declares 21 rows at `version: 1`
> before collection — name/family/unit/bounds/direction/
> detector-extractor id/reducer/windowed/required; the exact
> idempotency-key join `reliability.repeated_action_rate` and the
> retained approximation `reliability.repeated_action_similarity`
> (`required: false`, the old `repeated_action_count` oracle keeps
> its name) are distinct rows — `ac2_missing_key_extractor_and_
> zero_denominators` pins both spellings; fallback counts the
> started rows whose `variant_ref` differs from the declared
> intended variant (assigned-arm deviation, never a relabelled arm).
> REQ-D-106 (registered rate windows/joins/coverage): N2 — for
> every completed compaction c the subject window is
> `(completion_seq(c), subject_execution_stop_seq]`; golden
> `ac1_overlapping_windows_retain_joins_and_intensity` shows the
> seq-18 read joining BOTH c8 and c16 windows (overlapping
> `(c,event)` membership retained — joins `(8,10,x) (8,12,x)
> (8,18,x) (16,18,x) (16,18,z)`) and the seq-28 post-execution
> delivery never joining; `subject_stop_seq` is the projected
> earliest durable boundary (first phase→verify|teardown, first
> `control.decision{kind:"stop"}`, `lifecycle.run.finished`) —
> `ac2_post_execution_grade_cannot_enter_windows` proves the
> boundary, not the row class, does the excluding; coverage is
> disclosed per task×arm cell (`TaskMean.available/total`, unpaired
> tasks retained beside in `MetricContrast.unpaired`, never folded).
> REQ-E-004 (declared control/trigger semantics and exact
> detector/denominator mapping): N3 — every registry row binds a
> named detector id; missing required inputs are typed, never
> guessed: missing `idempotency_key` / absent extractor / undeclared
> totals ⇒ `no_detector`, zero denominator ⇒ `estimator_undefined`,
> absent or inapplicable summary ⇒ `inapplicable`
> (`ac2_missing_key_extractor_and_zero_denominators`); no-trigger /
> no-detector / fallback / ineffective outcomes stay distinct in the
> fold (`trigger_first_subject_rate` non-first ⇒ inapplicable).
> REQ-E-107 (declared mechanism detectors, eligible denominators
> and missingness before native counts→rates): N4 —
> `mechanism.eligible_count` carries the declared eligible pressure
> opportunities separately from starts/completions; unavailable
> required members stay in the family report
> (`ac2_unavailable_required_members_stay_in_family` — family rows
> enumerate the whole registry, `unavailable` status, never
> dropped); `subject_ratio→task_mean→equal_task` reducers average
> *available* subject ratios within task then weight `w_t = 1/T` —
> `ac1_subject_ratio_task_mean_equal_task_contrast` hand-counts the
> −0.625 two-task contrast in declared `ratio` units; events are
> never pooled and tasks never weighted by opportunities.
> Evidence `crates/hh-eval/tests/r5_36.rs` 5/5 — the golden
> fixtures hand-count Value{5,4} reacquisition intensity (>1,
> unclamped), Value{1,1} exact-idempotency repeated action, Value
> {2,3}/{2,2}/{2,2}/{1,2}/{1,1}/{2,2}/{40,100} activation/fidelity/
> request-change/ineffective/trigger/recall/overhead, and the
> distinct eligible(3)/starts(2)/completions(2)/ineffective(1)/
> fallback(0)/compaction(2) counts. Achieved **engineered D/E**;
> achieved layer `engineered` (golden-fixture recompute over
> projected durable rows — actual CI execution on pushed heads read
> at close per O-06). Owed: the R/P and actual-execution predicates
> stay with the register — the registered protocol freeze of
> detector/extractor versions and windows before any collection
> (R5.41), the study report legs (primary contributor REQ-D-106 →
> R5.49) and the wire contract (REQ-D-005 → R5.20, evidence
> dependency never dispatch); this note does not mark
> REQ-D-005/REQ-D-106/REQ-E-004/REQ-E-107 fully satisfied.
> REQ-D-109 / REQ-D-310 / REQ-E-209 / REQ-H-010 gain R5.36 as a
> **contributing-only** evidence row — achieved engineered D/E
> recorded separately from the owed legs; their primary owner stays
> R5.40 AC-3 (evidence dependency, never dispatch); all four targets
> keep `planned; unmet` — this ticket never marks a deferred
> full-target predicate MET.
> Dated primary-owner note — 2026-11-03 (seq 207, R5.37): the
> task-mixture sensitivity, sharp-null family and tail slice landed
> (ADR-0415). REQ-D-008 (task-aware estimators/floors/families/tails
> with actual declared inputs — owner-routed estimator, version,
> target, keys, seed, actual draws, floors, refusal and interval
> type/level): N1 — the routed kind `task_mixture_sensitivity`
> (hh-analysis kernel arm → `hh_eval::task_mixture` at
> `task-mixture/1`) persists `analysis_report_body/1` through the
> R5.16/16b records-in path carrying the S.1 method metadata —
> `routed_positive_persisted` and `changed_draws_change_computation`
> prove declared draws are computation inputs (2,000 vs 4,000 →
> different resample stream, body and `report_id`; `draws.used`
> equals the declared count — the additive `bootstrap_paired_with`
> carries the parameter while `bootstrap_paired` delegates at the
> frozen 4,000; no metadata-only satisfaction). N2 — whole-task
> paired bootstrap on the R5.15 `contrast_bounds` cells (one
> counting rule; tasks are the resampling unit, never flattened
> events/replicate n): `bca_floor_boundary` selects percentile at
> T=29 and BCa at T=30 (a declared method contradicting the floor
> refuses `BootstrapFloorViolation`); `signflip_mc_above_twenty`
> shows the sharp-null leg at T=21 → seeded 8,000 Monte Carlo with
> the +1 correction under the registered `lane_randomization`
> exchangeability (exact enumeration at T ≤ 20);
> `projection_feeds_binary_cells` binds the cell feed.
> N3 — complete frozen families across split reports:
> `split_reports_same_holm` recomputes identical Holm α=.05
> adjustments from one frozen family under two different
> split-report self-assignments with an unavailable member retained
> (it can never improve another member's multiplicity); the named
> exploratory family adjusts by BH under its own member — interval
> coverage and multiplicity-adjusted tests stay distinct members.
> N4 — typed refusals, no invented inference: `refusal_table`
> exercises OneTask (one task permits raw observations only),
> UnresolvedCell (missing keys), UnpairedTask, ShortCell (R<5),
> DegenerateBootstrap (constant deltas), UnsupportedDependence
> (shared_shocks), UnsupportedNull ×3 (weak_mean/practical_margin/
> other), ExchangeabilityNotAdmitted ×4 (absent/unregistered),
> UnsupportedVerdict (omnibus), DrawsBelowFloor (1,999),
> UnsupportedLevel, FamilyIdentity, UnknownMethod, MetadataOnly and
> MissingSpecRef (no bound view); signed and absolute tails are
> separate members and the observed max is labelled
> not-a-population-bound. Evidence `crates/hh-analysis/tests/
> r5_37.rs` 7/7 routed through `analyze` → `analyze_and_record`.
> Achieved **engineered D/E**; achieved layer `engineered`
> (routed fixture recompute — actual CI execution on pushed heads
> read at close per O-06). Owed: the R/P and actual-execution
> predicates stay with the register and the named later contributors
> (R5.16, R5.38, R5.40 — evidence dependency, never dispatch); this
> note does not mark REQ-D-008 fully satisfied. REQ-D-109 /
> REQ-D-310 / REQ-E-209 / REQ-H-010 gain R5.37 as a
> **contributing-only** evidence row — achieved engineered D/E
> recorded separately from the owed legs; their primary owner stays
> R5.40 AC-3; all four targets keep `planned; unmet` — this ticket
> never marks a deferred full-target predicate MET.

> 2026-11-03 · R5.38 acceptance note (REQ-D-304/305/306/307 primary;
> REQ-D-108 alias → D-304 only). Numbered checks → actual evidence,
> all routed through `analyze` → `analyze_and_record` → persisted
> `analysis_report_body/1` (battery `crates/hh-analysis/tests/
> r5_38.rs` 9/9):
> N1 — REQ-D-304 (declared KL bounds/TOST levels):
> `kl_known_answers` reproduces spec §4.5 pinned widths (N10/30/150
> zero-discordance .308497/.115703/.024293; N=150 equal counts
> {1,3,8,38} → .042944/.065355/.100601/.190899 ±≤3 ppm), x=0/x=1
> continuous conventions, monotonicity and domain refusals;
> `routed_binary_equivalent_touching` (strict band — touching stays
> inconclusive); `bounded_absolute_units_and_sign_reversal` (sign
> reversal negates).
> N2 — REQ-D-305 (planned missingness/veto populations):
> `completion_envelope_preserves_n` — u=10 uncertain pairs widen
> `[K(c/N).L, K((c+u)/N).U]` at preserved N=150 → conservative
> inconclusive; `completion_envelope{c,u,n}` member recorded.
> N3 — REQ-D-306 (bounded relative-mean tests):
> `relative_mean_and_zero_reference` — C−/C+ ratio-of-means legs
> equivalent; zero-containing reference refuses `ZeroReference`;
> frozen `absolute` convention runs the registered
> `{endpoint}:absolute` fallback. `bounded_absolute_*` covers the
> normalized-mean absolute construction.
> N4 — REQ-D-307 (all-endpoint vs selected-claim multiplicity):
> `iut_conjunction_unknown_members` — all-equivalent members pass;
> never-supplied member listed `unknown_members[]` and blocks;
> required endpoint absent → `FamilyIdentity`. `refusal_table` —
> selected_claims under-declaration, efficacy-Holm fusion
> (`UnsupportedVerdict`), MissingKey/ReusedSubject/PairIdentity/
> UnequalDesign/UnsupportedDependence/InvalidCounts/UnknownRange/
> MissingMargin/MarginKindMismatch/UnsupportedLevel/UnknownMethod/
> MetadataOnly/MissingSpecRef.
> N5 — S11 negative/reporting split:
> `s11_negative_requires_reporting_interval` — .90 disjoint alone
> is descriptive (note member); .95 wholly-outside ⇒
> `not_equivalent`.
> Achieved **engineered D/E** (routed fixture recompute — actual CI
> on pushed head read at close per O-06). Owed: R/P and
> actual-execution legs stay with R5.47 (parity-live) and R5.49
> (study report) — evidence dependency, never dispatch; REQ-D-309/
> 310 dependencies kept as contributing-only. This note does not
> mark any target fully satisfied.
> 2026-11-03 · R5.39 acceptance note (REQ-D-004/301/302/303/308
> primary). Numbered checks → actual evidence, all routed through
> `analyze` → `analyze_and_record` → persisted
> `analysis_report_body/1` (battery `crates/hh-analysis/tests/
> r5_39.rs` 12/12):
> N1 — REQ-D-301 (authentic admission — same
> participant/configuration, pinned first attempts):
> `admitted_full_fixture` — the pinned T=4·R=5 fixture admits
> through the routed kind (`admission=admitted`,
> design{4,5,N=20}, adapter ≥3 + S3 ≥5 floors on the report);
> `admitted_pairs_route_to_kl_unchanged` — the admitted pair ids
> drive R5.38's `directional_kl_equivalence` unchanged (M-E
> route); `persisted_report_route` — `analysis_report_body/1`
> section + `record.kind = "parity_admission"` + idempotent
> replay.
> N2 — REQ-D-302 (preassigned H/O first-attempt grouping,
> independent-across-pairs): `manifest_integrity_refusals` —
> pairs ≠ T·R / unequal weights (`UnequalDesign`), (task,repeat)
> collision and phase/stratum/policy mismatches (`PairIdentity`,
> `MissingIdentity`), `attempt ≠ 1` (`NotFirstAttempt`), reused
> subjects (`ReusedSubject`), honoured-seed coupling
> (`UnsupportedDependence` — the grouping exception grants no
> CRN/honoured-seed/R2 claim; `FORBIDDEN_USES` in
> `unavailable_and_unknown_members`).
> N3 — REQ-D-303 (signed pre-data dimension map):
> `late_or_absent_map_refused` (MissingMap / LateMap at
> map_seq ≥ first_outcome_seq / unsigned map);
> `dropped_and_inapplicable_dimensions` — required dimension
> dropped (`MissingDimension`), required typed-inapplicable
> (`RequiredInapplicable` — reason recorded, never a pass),
> non-required inapplicable recorded `status=inapplicable`;
> `unavailable_and_unknown_members` — `pass^k`/
> `conditional_eligible` stay `UnavailableMethod`; unknown
> dimension/estimand/applicability (`DimensionIdentity`);
> unregistered margin (`MissingMargin`) — margins bind only the
> pre-registration authority.
> N4 — REQ-D-308 + REQ-D-004 (both-policy applicability /
  transfer):
> `baseline_only_policy_refused` — `baseline_only` covered=[A]
> and `both_policies` missing B evidence both refuse
> `PolicyScope{policy:"B"}` (the b_A=−.10/b_B=.10 counterexample
> — baseline bias alone leaves treatment bias unknown);
> `identical_bridge_predeclared_admits` — predeclared bridge
> (seq 50 < 200) admits, late bridge (`LateBridge`);
> `transfer_claims_refused` — `MixedUncertainty` (Bayesian +
> frequentist legs), `BaselineOnlyTransfer` (b_A alone),
> `TransferBudget` (|b_A|+|b_B| > κ), `UnsupportedTransfer`
> (no certificate admitted here).
> N5 — inauthentic evidence shapes (spec §4.4 "empty maps,
> wrappers, supplied model-I/O fixtures or oracle transport
> equality alone fail"): `inauthentic_shapes_refused` —
> `recorded_model_io` (the s3_12b corpus shape),
> `artifact_benefit_wrapper`, `model_snapshot_maps` (equal
> empty maps), `oracle_transport_equality`, `seed_presence`
> and unknown spellings all refuse `InauthenticEvidence`;
> `routing_refusals` — `MetadataOnly` (method on wrong kind),
> `UnknownMethod`, `MissingSpecRef` (margin authority
> mandatory); seed records are counted on the report, never an
> admission basis.
> Achieved **engineered D/E** (routed fixture recompute — actual
> CI on pushed head read at close per O-06). Owed: R/P and
> actual-execution legs stay with R5.47 (parity-live under
> GATE-G13) and R5.49 (study report) — evidence dependency,
> never dispatch; REQ-D-109/310, REQ-E-209, REQ-H-010 remain
> contributing-only. This note does not mark any target fully
> satisfied.
