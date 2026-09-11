# Spec section brief — §10 Evaluation & reproducibility plan

You are a fresh **section author** for the HarnessHarness Canonical Spec (Phase 5, doc 3 §11.6). Write **Spec §10 — Evaluation & reproducibility plan** to `/Users/stevenvitali/MetaHarness/spec/sections/10-evaluation-reproducibility-plan.md`.

## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in /Users/stevenvitali/MetaHarness/research/decisions/ (index in /Users/stevenvitali/MetaHarness/research/LEDGER.md) → synthesis memos /Users/stevenvitali/MetaHarness/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers /Users/stevenvitali/MetaHarness/research/dossiers/WS-*.md → registers (/Users/stevenvitali/MetaHarness/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.

## Your assignment
- **Scope-register items you must cover:** R-2.9.2, R-2.9.3, R-2.9.4 (plan view), plus the lab exemplar experiments (every one ends as *specified* here or is explicitly cross-referenced to the section that specifies it; nothing is dropped).
- **Primary dossiers:** WS-I2, WS-I3, WS-I4, WS-J3, WS-J4 (read fully) and every ratified ADR they produced or that amends them (find them in the LEDGER ADR index and the ADR files' amendment logs; Phase 2–4 amendments to Phase 1 ADRs are additive and binding).
- **Also read:** registers/scope.md rows for your R-ids (stage notes there are binding), the relevant "settled" sections of every synthesis memo, and the reconciliation ADRs (ADR-0048, ADR-0145, ADR-0183, and the Phase 4 one).

## What to write
Write Spec §10 as a plan (not a subsystem spec): the factorial design and scorecard as normative evaluation protocol (ADR-0045…0047), the reproducible bundle (ADR-0139…0141 or as indexed), benchmark families and the first-stage benchmark set with the SWE-bench deviation (ADR-0144), the matched-budget protocol and C4 admissibility gates (ADR-0041/0046, ADR-0002), the two canonical exemplar experiments (compaction family, control-strategy family), and the Stage-0 spike specification (research/synthesis/l1-spike-spec.md) as a build-phase acceptance check. 2,500–5,000 words.

## Return (JSON, schema enforced)
{section, path, words, r_ids_covered[], adrs_cited[], gaps[] (each `[[GAP: …]]` you left), cross_refs_needed[] (things another section must define), notes}
