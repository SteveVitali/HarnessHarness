# Phase 5 brief — Adversarial coherence review, lens: traceability

You are a fresh, independent **reviewer** of the HarnessHarness Canonical Spec (doc 3 §7.3, §11.6 step 4). You were not involved in authoring it; be adversarial — your job is to find what is wrong, missing, or contradictory. Read `/Users/stevenvitali/MetaHarness/spec/CANONICAL_SPEC.md` in full (it is large; read it section by section), and consult `/Users/stevenvitali/MetaHarness/research/decisions/ADR-*.md`, `/Users/stevenvitali/MetaHarness/research/registers/` and `/Users/stevenvitali/MetaHarness/research/LEDGER.md` as needed.

## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in /Users/stevenvitali/MetaHarness/research/decisions/ (index in /Users/stevenvitali/MetaHarness/research/LEDGER.md) → synthesis memos /Users/stevenvitali/MetaHarness/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers /Users/stevenvitali/MetaHarness/research/dossiers/WS-*.md → registers (/Users/stevenvitali/MetaHarness/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.

## Your lens
Evidence and ADR traceability: sample at least 60 normative statements across all sections (proportionally) and check each cites a ratified ADR that actually says that; flag fabricated or mis-cited claims; flag any C0 statement resting on a `provisional` source (check registers/sources.md); verify every ADR in the LEDGER index is referenced by at least one section or the deferred list; check C3/C4 items carry maturity flags and matched-budget conditionality.

## Rules
- Every finding must name the section anchor/heading, quote the offending text (≤ 2 lines), state the defect precisely, cite the authoritative source (ADR/register) that shows it, and propose the exact fix. Severity: `blocker` (fails a §7.3 criterion), `major` (would mislead decompose-spec/implementers), `minor`.
- Do not edit any file. Return ONLY the JSON: {lens, findings: [{id, severity, section, quote, defect, authority, fix}], sampled: n, summary}.
