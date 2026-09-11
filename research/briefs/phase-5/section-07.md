# Spec section brief — §07 Surfaces (K-track)

You are a fresh **section author** for the HarnessHarness Canonical Spec (Phase 5, doc 3 §11.6). Write **Spec §07 — Surfaces (K-track)** to `/Users/stevenvitali/MetaHarness/spec/sections/07-surfaces.md`.

## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in /Users/stevenvitali/MetaHarness/research/decisions/ (index in /Users/stevenvitali/MetaHarness/research/LEDGER.md) → synthesis memos /Users/stevenvitali/MetaHarness/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers /Users/stevenvitali/MetaHarness/research/dossiers/WS-*.md → registers (/Users/stevenvitali/MetaHarness/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.

## Your assignment
- **Scope-register items you must cover:** R-2.11.1, R-2.11.2, R-2.11.3, R-2.11.4 (every one ends as *specified* here or is explicitly cross-referenced to the section that specifies it; nothing is dropped).
- **Primary dossiers:** WS-K1, WS-K2, WS-K3, WS-K4 (read fully) and every ratified ADR they produced or that amends them (find them in the LEDGER ADR index and the ADR files' amendment logs; Phase 2–4 amendments to Phase 1 ADRs are additive and binding).
- **Also read:** registers/scope.md rows for your R-ids (stage notes there are binding), the relevant "settled" sections of every synthesis memo, and the reconciliation ADRs (ADR-0048, ADR-0145, ADR-0183, and the Phase 4 one).

## What to write
For **each** scope item assigned to you (by R-id), write a subsection with exactly these headings, in this order (doc 3 §7.2 item 5; §11.6 step 1):
1. **Responsibility** (2–5 sentences; tier C0–C4; participant-class applicability)
2. **Interface contract** — operations (name, inputs, outputs, errors), invariants, pre/postconditions; cite ADRs
3. **Data model** — entities/records/enums with fields and identity/provenance rules; cite ADRs
4. **Dependencies** — which other subsystems/contracts it consumes and which consume it (by R-id and ADR)
5. **Failure modes** — enumerated, each with detection and containment
6. **Security & provenance obligations** — authority classes, labels, monitor checks, audit events it must emit (ADR-0033/34/35 and the H-track ADRs)
7. **Acceptance criteria** — numbered `AC-<R-id>-n`, checkable, including the T-LCD tests that apply and the matched-budget conditionality where the ADR requires it
8. **Test-matrix hooks** — the fault-injection points, fixtures, conformance suites and process metrics the ADRs name
9. **Build-stage assignment** — tier + Stage N for the C0 slice and for each extension slice, with the ADR that assigns it
Include the C0 minimal variant vs the extension family explicitly (doc 3 §1.3 rule 3).
State explicitly that every surface is a client of the embedding contract (ADR-0176…0179 and the Phase 3 seam rulings in ADR-0183).

## Return (JSON, schema enforced)
{section, path, words, r_ids_covered[], adrs_cited[], gaps[] (each `[[GAP: …]]` you left), cross_refs_needed[] (things another section must define), notes}
