# Phase 5 brief — Decompose-Readiness Gate

You are the fresh **readiness auditor** (doc 3 §7.3, §11.7). Produce `/Users/stevenvitali/MetaHarness/spec/READINESS_REPORT.md`: a checklist with evidence links for every criterion, each marked PASS / FAIL with the evidence (section anchors, AC ids, ADR ids, appendix rows). Criteria:
1. Every scope-register item (registers/scope.md, all R-ids incl. splits) is specified in the spec with interface contract + data model + acceptance criteria + build-stage assignment, or explicitly deferred with an ADR. Produce the full table (R-id → section → status → ADRs).
2. Every subsystem section carries all nine required headings with substantive content.
3. The C0/C1–C4 DAG (§4) and the staged build ladder (§9) are valid DAGs with no cross-extension internal coupling; Stage 0 is buildable from the spec alone.
4. Every load-bearing decision has a ratified ADR meeting the §3.3 synthesis contract (all ADR files: status ratified, evidence fields a–e present); no C0 statement rests on provisional evidence; C3/C4 carry maturity flags + matched-budget conditionality.
5. The adversarial coherence review (research/synthesis/phase-5-review-*.json, appended to your prompt as a digest) has no unresolved `blocker` or `major` finding; list any residual `minor` findings.
6. Glossary/naming/neutrality: no retired spellings, no residual "MetaHarness" outside historical citations, no language/framework commitments beyond ADR-0050 §8, LCD battery T-LCD-01…15 each discharged (cite where).
7. Every open question still `open` is either non-blocking for Stages 0–3 or converted to a deferral ADR (doc 3 §8).
Then: update `/Users/stevenvitali/MetaHarness/research/LEDGER.md` Phase 5 row → `passed` or `failed: <criteria>` with a final progress-log block, and update registers/scope.md statuses to `specified`/`deferred(...)` as evidenced. Do NOT run decompose-spec/orchestrate-build/implement-spec and write no implementation code.

## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in /Users/stevenvitali/MetaHarness/research/decisions/ (index in /Users/stevenvitali/MetaHarness/research/LEDGER.md) → synthesis memos /Users/stevenvitali/MetaHarness/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers /Users/stevenvitali/MetaHarness/research/dossiers/WS-*.md → registers (/Users/stevenvitali/MetaHarness/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.

Return ONLY JSON: {gate_pass: bool, failed_criteria: [..], report_path, coverage: {specified: n, deferred: n, missing: n}, residual_minor: n, summary}.
