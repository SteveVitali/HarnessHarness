#!/usr/bin/env python3
"""Generate Phase 5 (spec assembly) briefs into research/briefs/phase-5/."""
import os
ROOT = '/Users/stevenvitali/MetaHarness'
OUT = f'{ROOT}/research/briefs/phase-5'
os.makedirs(OUT, exist_ok=True)

COMMON = f"""
## Program context (binding)
- Product: **HarnessHarness** (ADR-0011). Never "MetaHarness" except in historical quotes/paths.
- You are writing part of the **Canonical Spec** (doc 3 §7.2 skeleton; §11.6). The spec is language/ecosystem-neutral in every contract; the ratified language decision ADR-0050 §8 governs the only permitted references to ecosystems (host requirements, protocol SDK availability, sandbox/isolation primitives). No code, no library APIs.
- Sources of truth, in precedence order: ratified ADRs in {ROOT}/research/decisions/ (index in {ROOT}/research/LEDGER.md) → synthesis memos {ROOT}/research/synthesis/phase-*.md ("settled" sections and ADR-0048/0145/0183-class reconciliation ADRs) → dossiers {ROOT}/research/dossiers/WS-*.md → registers ({ROOT}/research/registers/: ontology.md (canonical vocabulary — use it verbatim; the glossary check fails on unregistered synonyms), scope.md (R-ids), lcd-test-battery.md, conflicts.md, open-questions.md).
- **Every normative claim traces** to a ratified ADR (cite `ADR-NNNN`) and, where useful, the dossier section. Do not invent decisions. If a needed decision is missing, write `[[GAP: …]]` inline and list it in your return — do not resolve it yourself.
- **Evidence discipline (doc 3 §3.1):** provisional (2026 single-team/preprint) evidence may not be load-bearing for a C0 statement; mark C3/C4 mechanisms with their maturity flag and matched-budget conditionality as the ADRs state them.
- Markdown; headings start at `##` inside your section (the assembler nests them); use tables for contracts and criteria; keep prose dense. Write to the path given below. Return ONLY the JSON the schema asks for.
"""

SUBSYSTEM_TEMPLATE = """For **each** scope item assigned to you (by R-id), write a subsection with exactly these headings, in this order (doc 3 §7.2 item 5; §11.6 step 1):
1. **Responsibility** (2–5 sentences; tier C0–C4; participant-class applicability)
2. **Interface contract** — operations (name, inputs, outputs, errors), invariants, pre/postconditions; cite ADRs
3. **Data model** — entities/records/enums with fields and identity/provenance rules; cite ADRs
4. **Dependencies** — which other subsystems/contracts it consumes and which consume it (by R-id and ADR)
5. **Failure modes** — enumerated, each with detection and containment
6. **Security & provenance obligations** — authority classes, labels, monitor checks, audit events it must emit (ADR-0033/34/35 and the H-track ADRs)
7. **Acceptance criteria** — numbered `AC-<R-id>-n`, checkable, including the T-LCD tests that apply and the matched-budget conditionality where the ADR requires it
8. **Test-matrix hooks** — the fault-injection points, fixtures, conformance suites and process metrics the ADRs name
9. **Build-stage assignment** — tier + Stage N for the C0 slice and for each extension slice, with the ADR that assigns it
Include the C0 minimal variant vs the extension family explicitly (doc 3 §1.3 rule 3)."""

SECTIONS = [
  ('01', 'vision-thesis-nongoals', 'Vision, thesis, non-goals', 'R-2.12.5 (and the positioning/novelty ADRs ADR-0003…0008, 0010, 0011)', ['WS-A1', 'WS-L7'],
   "Write Spec §1: the vision and the HarnessHarness thesis (doc 3 §1.1 as ratified/narrowed by ADR-0003/0004/0006), the two participant classes and hosting stance (ADR-0001/0005/0010), the differentiation thesis with the honest novelty claims and 'renamed' list (ADR-0004/0006, CF-011 outcome after Phases 1–3 — check the LEDGER/conflicts register for its current status), the canonical non-goals N1–N13 (ADR-0006), and the LCD-trap battery as a binding design constraint (ADR-0007; summarize T-LCD-01…15 in a table with the ADRs that discharge each). 1,500–3,000 words."),
  ('02', 'ontology-formal-model', 'Ontology & formal model', 'R-2.1.1', ['WS-A2'],
   "Write Spec §2 from Ontology v3 (registers/ontology.md) and ADR-0012/0013/0014 (+ amendments): the two-level ontology, seven planes with the home-plane rule, the two boundaries, the formal model (model snapshot/set, θ, harness step, π_system = H[π_M], objective, harness effect Δ, control boundary β, compatibility surface as derived view), validity vs compliance as a measured pair, participant classes and admissible granularities, and a glossary table of every canonical term used elsewhere in the spec with its ADR. 3,000–6,000 words."),
  ('03', 'harness-ir-compilation', 'Harness IR & compilation', 'R-2.1.2, R-2.1.3, R-2.1.4', ['WS-A3', 'WS-A4', 'WS-A5'],
   "Write Spec §3: HIR/1 type discipline and expressiveness ceiling (ADR-0015), the entity/edge catalogue with provenance/version records and kernel invariants (ADR-0016), typed diff (ADR-0017), hosted processes (ADR-0018); the compilation pipeline, Model Profile contract, protocol targets and equivalence obligations (ADR-0019…0022); the composition model, code-vs-config rule and assembly validation (ADR-0023…0025); plus all Phase 2–4 amendments logged on those ADRs. Use the subsystem template for each of the three R-ids. 5,000–9,000 words."),
  ('05a', 'runtime-durability', 'Core runtime & durability (B-track)', 'R-2.2.1, R-2.2.2, R-2.2.3, R-2.2.4, R-2.2.5', ['WS-B1', 'WS-B2', 'WS-B3', 'WS-B4', 'WS-B5'], SUBSYSTEM_TEMPLATE),
  ('05b', 'model-plane', 'Model plane (C-track)', 'R-2.3.1, R-2.3.2, R-2.3.3, R-2.3.4', ['WS-C1', 'WS-C2', 'WS-C3', 'WS-C4'], SUBSYSTEM_TEMPLATE),
  ('05c', 'context-memory', 'Context & memory plane (D-track)', 'R-2.4.1, R-2.4.2, R-2.4.3, R-2.4.4, R-2.4.5', ['WS-D1', 'WS-D2', 'WS-D3', 'WS-D4', 'WS-D5'], SUBSYSTEM_TEMPLATE),
  ('05d', 'tools-action', 'Tools & action plane (E-track)', 'R-2.5.1, R-2.5.2, R-2.5.3, R-2.5.4, R-2.5.5', ['WS-E1', 'WS-E2', 'WS-E3', 'WS-E4', 'WS-E5'], SUBSYSTEM_TEMPLATE),
  ('05e', 'control-orchestration', 'Control & orchestration plane (F-track)', 'R-2.6.1, R-2.6.2, R-2.6.3, R-2.6.4, R-2.6.5', ['WS-F1', 'WS-F2', 'WS-F3', 'WS-F4', 'WS-F5'], SUBSYSTEM_TEMPLATE),
  ('05f', 'verification', 'Verification plane (G-track)', 'R-2.7.1, R-2.7.2a, R-2.7.2b, R-2.7.3', ['WS-G1', 'WS-G2', 'WS-G3'], SUBSYSTEM_TEMPLATE),
  ('05g', 'security-governance', 'Security & governance plane (H-track)', 'R-2.8.1, R-2.8.2, R-2.8.3, R-2.8.4, R-2.8.5, R-2.8.6, R-2.8.7', ['WS-H1', 'WS-H2', 'WS-H3', 'WS-H4', 'WS-H5', 'WS-H6', 'WS-H7'], SUBSYSTEM_TEMPLATE),
  ('05h', 'measurement-evolution', 'Measurement & evolution plane (I-track)', 'R-2.9.1, R-2.9.2, R-2.9.3, R-2.9.4, R-2.9.5, R-2.9.6, R-2.9.7, R-2.9.8', ['WS-I1', 'WS-I2', 'WS-I3', 'WS-I4', 'WS-I5', 'WS-I6', 'WS-I7', 'WS-I8'], SUBSYSTEM_TEMPLATE + "\nFor R-2.9.5…R-2.9.8 (C4): carry the maturity flag, the matched-budget conditionality and the governance invariants of ADR-0002 verbatim as acceptance criteria."),
  ('05i', 'organizational-layer', 'Human-agent organizational layer', 'R-2.12.6', ['WS-L8'], SUBSYSTEM_TEMPLATE),
  ('06', 'laboratory', 'The Harness Lab (J-track)', 'R-2.10.1, R-2.10.2, R-2.10.3, R-2.10.4, R-2.10.5, R-2.10.6', ['WS-J1', 'WS-J2', 'WS-J3', 'WS-J4', 'WS-J5', 'WS-J6'], SUBSYSTEM_TEMPLATE + "\nOpen with a 400–800-word overview of the lab chain (assembly → registry → experiments → results → analysis; hosting as the strictly secondary participant path, ADR-0001/0005/0010/0164…0166)."),
  ('07', 'surfaces', 'Surfaces (K-track)', 'R-2.11.1, R-2.11.2, R-2.11.3, R-2.11.4', ['WS-K1', 'WS-K2', 'WS-K3', 'WS-K4'], SUBSYSTEM_TEMPLATE + "\nState explicitly that every surface is a client of the embedding contract (ADR-0176…0179 and the Phase 3 seam rulings in ADR-0183)."),
  ('08', 'cross-cutting-contracts', 'Cross-cutting contracts', 'R-2.1.5, R-2.1.6, R-2.12.1, R-2.12.2', ['WS-L3', 'WS-L2', 'WS-L4', 'WS-L5'], SUBSYSTEM_TEMPLATE + "\nThese are consumed by every other section: provenance/authority (ADR-0033…0035), resource accounting (ADR-0039…0041), identity/versioning (ADR-0036…0038), extensibility/plugins (ADR-0180…0182 + L5 amendments). Make the shared schemas canonical here and have other sections reference them."),
  ('10', 'evaluation-reproducibility-plan', 'Evaluation & reproducibility plan', 'R-2.9.2, R-2.9.3, R-2.9.4 (plan view), plus the lab exemplar experiments', ['WS-I2', 'WS-I3', 'WS-I4', 'WS-J3', 'WS-J4'],
   "Write Spec §10 as a plan (not a subsystem spec): the factorial design and scorecard as normative evaluation protocol (ADR-0045…0047), the reproducible bundle (ADR-0139…0141 or as indexed), benchmark families and the first-stage benchmark set with the SWE-bench deviation (ADR-0144), the matched-budget protocol and C4 admissibility gates (ADR-0041/0046, ADR-0002), the two canonical exemplar experiments (compaction family, control-strategy family), and the Stage-0 spike specification (research/synthesis/l1-spike-spec.md) as a build-phase acceptance check. 2,500–5,000 words."),
  ('11', 'deferred-out-of-scope', 'Deferred and out-of-scope items', 'all `deferred(ADR-…)` scope rows; all open questions unresolved at Phase 4; non-goals', ['(registers)'],
   "Write Spec §11: (a) the non-goals N1–N13 (ADR-0006) restated as out-of-scope with rationale; (b) every scope-register row whose status is `deferred(...)`, with its ADR; (c) every open question in registers/open-questions.md still `open` after Phase 4 — group them by owning subsystem and build stage, and for each state whether it blocks a stage (per the phase-4 memo §5) or is program-level; **doc 3 §8 requires that unresolved strategic questions become explicit deferral ADRs** — for each blocking one that lacks a deferral ADR, write `[[GAP: deferral ADR needed for OQ-…]]` and list it in your return so the architect/assembler can author them; (d) the language-decision revalidation triggers (ADR-0050 §6) and the surface-binding deferral (OQ-130). 1,500–4,000 words."),
]

for num, slug, title, reqs, ws, body in SECTIONS:
    path = f'{ROOT}/spec/sections/{num}-{slug}.md'
    brief = f"""# Spec section brief — §{num} {title}

You are a fresh **section author** for the HarnessHarness Canonical Spec (Phase 5, doc 3 §11.6). Write **Spec §{num} — {title}** to `{path}`.
{COMMON}
## Your assignment
- **Scope-register items you must cover:** {reqs} (every one ends as *specified* here or is explicitly cross-referenced to the section that specifies it; nothing is dropped).
- **Primary dossiers:** {', '.join(ws)} (read fully) and every ratified ADR they produced or that amends them (find them in the LEDGER ADR index and the ADR files' amendment logs; Phase 2–4 amendments to Phase 1 ADRs are additive and binding).
- **Also read:** registers/scope.md rows for your R-ids (stage notes there are binding), the relevant "settled" sections of every synthesis memo, and the reconciliation ADRs (ADR-0048, ADR-0145, ADR-0183, and the Phase 4 one).

## What to write
{body}

## Return (JSON, schema enforced)
{{section, path, words, r_ids_covered[], adrs_cited[], gaps[] (each `[[GAP: …]]` you left), cross_refs_needed[] (things another section must define), notes}}
"""
    open(f'{OUT}/section-{num}.md', 'w').write(brief)

ARCHITECT = f"""# Phase 5 brief — Architect / Assembler

You are the fresh **architect–assembler** for the HarnessHarness Canonical Spec (Phase 5, doc 3 §11.6 steps 1–3; §7.2 items 4 and 9). The per-subsystem sections are already written under {ROOT}/spec/sections/ (01, 02, 03, 05a–05i, 06, 07, 08, 10, 11). Read them ALL, plus {ROOT}/research/LEDGER.md, registers/scope.md, registers/ontology.md, and every synthesis memo's "settled" section.
{COMMON}
## Do (persist each)
1. Write `{ROOT}/spec/sections/04-architecture-overview.md` — Spec §4: the control-plane / execution-plane split (ADR-0012 boundaries; ADR-0050 layers); the **C0 Core** and the **C1–C4 extension tiers** as an explicit dependency DAG (a table: every scope item → tier → the Core contracts it depends on → the extensions it depends on), enforcing doc 3 §1.3: no extension reaches another extension's internals except through Core-defined contracts (cite ADR-0180…0182 / L5 rules). Include a Mermaid diagram of the DAG. 2,000–4,000 words.
2. Write `{ROOT}/spec/sections/09-build-ladder.md` — Spec §9: the staged build ladder Stage 0→N that `decompose-spec` inherits (doc 2 §11 stages re-derived from the ADR build-stage assignments, the scope-register stage notes and ADR-0146/0184's C0 slices): for each stage — the goal, the exact subsystem slices (R-id + ADR + tier) that land, the acceptance criteria that gate the stage (by AC ids from the sections), what is demonstrably working at the stage boundary, and the stage's dependencies. Verify the ladder is a valid DAG with no cross-extension internal coupling; state the verification. Include the Stage-0 spike (l1-spike-spec.md) as a Stage-0 acceptance check. 2,000–4,000 words.
3. **Assemble** `{ROOT}/spec/CANONICAL_SPEC.md`: front matter (title, version 1.0-rc1, date, provenance: ADR range, ontology version, git commit placeholder), a short reader's guide, the full table of contents linking every section file in §7.2 order (01, 02, 03, 04, 05a…05i, 06, 07, 08, 09, 10, 11), then **inline the sections' content** in order (use a heading-level shift so section files' `##` become `###` under a `## §N` heading), followed by two appendices: **A. Coverage matrix** (every scope-register R-id → section anchor → tier → stage → ADRs; status specified/deferred) and **B. ADR index** (ADR-0001…latest with title, status, section(s)). The file may be large; that is expected.
4. **Glossary check:** grep the assembled spec for retired spellings named in the synthesis memos (e.g. `measurement.artifact.*`, `step_id`, `call_id`, `ResolvedDefinition`, bare "capability", "MetaHarness" outside historical citations) and for terms not in registers/ontology.md; fix in the section files and re-assemble. Record the check result at the end of the spec front matter.
5. **Gap sweep:** collect every `[[GAP: …]]` marker across sections. For each: if a ratified ADR resolves it, fix the text and cite; if a deferral ADR is required (doc 3 §8) author it at the LEDGER "Next number" (status ratified, evidence fields a–e, referencing the OQ) and update registers/open-questions.md and the LEDGER ADR index; otherwise leave the marker and list it. Re-assemble.
6. Update `{ROOT}/research/LEDGER.md`: Phase 5 row → `in-progress (assembled; review pending)` with a bullet log.

## Return (JSON, schema enforced)
{{spec_path, sections_inlined[], words_total, coverage: {{specified: n, deferred: n, missing: [R-ids]}}, dag_valid: bool, cross_extension_coupling_found[], gaps_remaining[], deferral_adrs_authored[], glossary_check: "clean"|"issues: …"}}
"""
open(f'{OUT}/architect.md', 'w').write(ARCHITECT)

REVIEW_LENSES = {
 'contradiction': "Cross-subsystem contradiction hunt: find every place two sections (or a section and a ratified ADR) disagree on a name, field, enum value, invariant, ordering, authority rule, ID, stage, or tier. Treat the ADRs as authoritative. Also flag any section that re-decides something a ratified ADR settled.",
 'completeness': "Decompose-Readiness completeness (doc 3 §7.3): for EVERY scope-register R-id verify the section carries all nine subsystem headings with real content (not placeholders): responsibility, interface contract (operations with inputs/outputs/errors), data model, dependencies, failure modes, security/provenance obligations, acceptance criteria (numbered, checkable), test-matrix hooks, build-stage assignment. Flag every missing or vacuous item; flag any R-id neither specified nor explicitly deferred.",
 'dag': "Build-ladder and tier DAG validity: verify §4's C0/C1–C4 DAG and §9's stage ladder are acyclic; that no extension-tier item reaches another extension's internals except through a Core contract; that every C0 slice named in ADR-0146/0184 lands at or before the stage that needs it; that every stage's gating acceptance criteria exist in the sections; that Stage 0 is buildable from the spec alone.",
 'traceability': "Evidence and ADR traceability: sample at least 60 normative statements across all sections (proportionally) and check each cites a ratified ADR that actually says that; flag fabricated or mis-cited claims; flag any C0 statement resting on a `provisional` source (check registers/sources.md); verify every ADR in the LEDGER index is referenced by at least one section or the deferred list; check C3/C4 items carry maturity flags and matched-budget conditionality.",
 'lcd': "LCD-trap battery and language neutrality: for each T-LCD-01…15 in registers/lcd-test-battery.md, find where the spec discharges it (or flag it undischarged); flag any contract that names a language, runtime, framework or library API beyond what ADR-0050 §8 permits; flag any 'escape hatch' abstraction the battery forbids; flag residual 'MetaHarness'.",
 'editorial': "Editorial and argument review (editorial-review style): does §1–§4 make a coherent, honest argument (novelty claims match ADR-0004/0006; non-goals consistent); is the reader's guide adequate; are terms used consistently with the glossary; are there sections whose prose contradicts their own tables; is anything unreadable to a decompose-spec agent (ambiguous MUST/SHOULD, undefined acronyms, dangling cross-references, broken anchors).",
}
for lens, text in REVIEW_LENSES.items():
    open(f'{OUT}/review-{lens}.md', 'w').write(f"""# Phase 5 brief — Adversarial coherence review, lens: {lens}

You are a fresh, independent **reviewer** of the HarnessHarness Canonical Spec (doc 3 §7.3, §11.6 step 4). You were not involved in authoring it; be adversarial — your job is to find what is wrong, missing, or contradictory. Read `{ROOT}/spec/CANONICAL_SPEC.md` in full (it is large; read it section by section), and consult `{ROOT}/research/decisions/ADR-*.md`, `{ROOT}/research/registers/` and `{ROOT}/research/LEDGER.md` as needed.
{COMMON}
## Your lens
{text}

## Rules
- Every finding must name the section anchor/heading, quote the offending text (≤ 2 lines), state the defect precisely, cite the authoritative source (ADR/register) that shows it, and propose the exact fix. Severity: `blocker` (fails a §7.3 criterion), `major` (would mislead decompose-spec/implementers), `minor`.
- Do not edit any file. Return ONLY the JSON: {{lens, findings: [{{id, severity, section, quote, defect, authority, fix}}], sampled: n, summary}}.
""")

FIXER = f"""# Phase 5 brief — Fixer

You are the fresh **fixer** for the HarnessHarness Canonical Spec. You receive a list of review findings (appended to your prompt). For each finding, in severity order: open the section file under `{ROOT}/spec/sections/`, verify the finding against the cited authority (ratified ADR / register); if valid, apply the exact fix (or a better one that satisfies the authority) in the section file; if invalid, record why. Where a fix requires a new deferral ADR (doc 3 §8), author it at the LEDGER "Next number" with evidence fields a–e and update the LEDGER index + registers. Never weaken an ADR to satisfy a finding — the ADR is authoritative; if two ADRs genuinely conflict, log a CF row in registers/conflicts.md, resolve it by a synthesis-style amendment log on the losing ADR, and record it in your return. After all fixes, **re-assemble** `{ROOT}/spec/CANONICAL_SPEC.md` exactly as the architect brief specifies (same front matter, TOC, inlining rule, appendices; refresh Appendix A/B).
{COMMON}
Return ONLY JSON: {{fixed: [ids], rejected: [{{id, reason}}], adrs_authored[], conflicts_logged[], reassembled: bool}}.
"""
open(f'{OUT}/fixer.md', 'w').write(FIXER)

READINESS = f"""# Phase 5 brief — Decompose-Readiness Gate

You are the fresh **readiness auditor** (doc 3 §7.3, §11.7). Produce `{ROOT}/spec/READINESS_REPORT.md`: a checklist with evidence links for every criterion, each marked PASS / FAIL with the evidence (section anchors, AC ids, ADR ids, appendix rows). Criteria:
1. Every scope-register item (registers/scope.md, all R-ids incl. splits) is specified in the spec with interface contract + data model + acceptance criteria + build-stage assignment, or explicitly deferred with an ADR. Produce the full table (R-id → section → status → ADRs).
2. Every subsystem section carries all nine required headings with substantive content.
3. The C0/C1–C4 DAG (§4) and the staged build ladder (§9) are valid DAGs with no cross-extension internal coupling; Stage 0 is buildable from the spec alone.
4. Every load-bearing decision has a ratified ADR meeting the §3.3 synthesis contract (all ADR files: status ratified, evidence fields a–e present); no C0 statement rests on provisional evidence; C3/C4 carry maturity flags + matched-budget conditionality.
5. The adversarial coherence review (research/synthesis/phase-5-review-*.json, appended to your prompt as a digest) has no unresolved `blocker` or `major` finding; list any residual `minor` findings.
6. Glossary/naming/neutrality: no retired spellings, no residual "MetaHarness" outside historical citations, no language/framework commitments beyond ADR-0050 §8, LCD battery T-LCD-01…15 each discharged (cite where).
7. Every open question still `open` is either non-blocking for Stages 0–3 or converted to a deferral ADR (doc 3 §8).
Then: update `{ROOT}/research/LEDGER.md` Phase 5 row → `passed` or `failed: <criteria>` with a final progress-log block, and update registers/scope.md statuses to `specified`/`deferred(...)` as evidenced. Do NOT run decompose-spec/orchestrate-build/implement-spec and write no implementation code.
{COMMON}
Return ONLY JSON: {{gate_pass: bool, failed_criteria: [..], report_path, coverage: {{specified: n, deferred: n, missing: n}}, residual_minor: n, summary}}.
"""
open(f'{OUT}/readiness.md', 'w').write(READINESS)
print('phase-5 briefs:', sorted(os.listdir(OUT)))
