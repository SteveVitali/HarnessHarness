# Spec section brief — §11 Deferred and out-of-scope items

You are a fresh **section author** for the HarnessHarness Canonical Spec (Phase 5, doc 3 §11.6). Write **Spec §11 — Deferred and out-of-scope items** to `/Users/stevenvitali/MetaHarness/spec/sections/11-deferred-out-of-scope.md`.

## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in /Users/stevenvitali/MetaHarness/research/decisions/ (index in /Users/stevenvitali/MetaHarness/research/LEDGER.md) → synthesis memos /Users/stevenvitali/MetaHarness/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers /Users/stevenvitali/MetaHarness/research/dossiers/WS-*.md → registers (/Users/stevenvitali/MetaHarness/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.

## Your assignment
- **Scope-register items you must cover:** all `deferred(ADR-…)` scope rows; all open questions unresolved at Phase 4; non-goals (every one ends as *specified* here or is explicitly cross-referenced to the section that specifies it; nothing is dropped).
- **Primary dossiers:** (registers) (read fully) and every ratified ADR they produced or that amends them (find them in the LEDGER ADR index and the ADR files' amendment logs; Phase 2–4 amendments to Phase 1 ADRs are additive and binding).
- **Also read:** registers/scope.md rows for your R-ids (stage notes there are binding), the relevant "settled" sections of every synthesis memo, and the reconciliation ADRs (ADR-0048, ADR-0145, ADR-0183, and the Phase 4 one).

## What to write
Write Spec §11: (a) the non-goals N1–N13 (ADR-0006) restated as out-of-scope with rationale; (b) every scope-register row whose status is `deferred(...)`, with its ADR; (c) every open question in registers/open-questions.md still `open` after Phase 4 — group them by owning subsystem and build stage, and for each state whether it blocks a stage (per the phase-4 memo §5) or is program-level; **doc 3 §8 requires that unresolved strategic questions become explicit deferral ADRs** — for each blocking one that lacks a deferral ADR, write `[[GAP: deferral ADR needed for OQ-…]]` and list it in your return so the architect/assembler can author them; (d) the language-decision revalidation triggers (ADR-0050 §6) and the surface-binding deferral (OQ-130). 1,500–4,000 words.

## Return (JSON, schema enforced)
{section, path, words, r_ids_covered[], adrs_cited[], gaps[] (each `[[GAP: …]]` you left), cross_refs_needed[] (things another section must define), notes}
