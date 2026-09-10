#!/usr/bin/env python3
"""Generate per-workstream research briefs and the synthesis brief for a phase
from research/briefs/phaseN-args.json. Usage: gen_briefs.py N"""
import json, sys, os
N = sys.argv[1]
ROOT = '/Users/stevenvitali/MetaHarness'
REPOS = '/tmp/hh-repos'
a = json.load(open(f'{ROOT}/research/briefs/phase{N}-args.json'))
out = f'{ROOT}/research/briefs/phase-{N}'
os.makedirs(out, exist_ok=True)

def research_brief(ws):
    upstream = '\n'.join(f'  - {ROOT}/research/dossiers/{u}.md' for u in ws.get('upstream', [])) or '  (none — this is an upstream workstream)'
    extra = f"\n**Additional phase-specific instructions:**\n{ws['extra']}\n" if ws.get('extra') else ''
    return f"""# Research brief — {ws['id']} (Phase {N})

You are a fresh research subagent in the HarnessHarness program (Phase {N}). You own workstream **{ws['id']} — {ws['title']}**.

## Your contract (doc 3 §11.4, binding)
Conduct deep research and write a durable dossier. Triangulate **at least 2 of the 4 source streams** (doc 3 §3.2): (1) academic literature — start from the seeded Source Registry and doc 2, extend via WebSearch/WebFetch (arXiv abstract + HTML pages are fine); (2) OSS source-code audit — actually read source of the named repos using the vertical-slice method (clone shallowly into {REPOS}/<name> with `git clone --depth 1` if not already present, then read the specific files that implement your concern; record file paths and the commit you read); (3) protocol specifications (MCP, A2A, ACP, etc.); (4) production engineering write-ups (mine mechanisms, discount product-specific assumptions).

Apply the evidence discipline (doc 3 §3.1) strictly: tag every 2026 single-team/preprint claim `provisional`; separate **mechanism** evidence from **performance** evidence; discount unmatched-budget "X beats baseline" claims; record the strongest **disconfirming** evidence for every recommendation. A provisional claim may never be load-bearing for a C0 decision.

**Contracts stay language/ecosystem-neutral** (operations/inputs/outputs/invariants). The WS-L1 decision ADR (see the LEDGER ADR index) governs the only permitted references to the chosen ecosystem.

## Your workstream row (doc 3 §5, quoted)
- **ID:** {ws['id']}
- **Scope / key questions:** {ws['scope']}
- **Primary anchors & sources:** {ws['anchors']}
- **Feeds spec area:** {ws['feeds']}
- **Tier(s):** {ws['tier']}
- **Scope-register items:** {ws.get('reqs', 'see registers/scope.md')}
{extra}
## Read first (in this order)
1. {ROOT}/research/TEMPLATES.md — the dossier + ADR templates you must follow exactly.
2. {ROOT}/docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md — §1, §2, §3, §5 (your row and neighbours), §7.2 (spec skeleton you feed).
3. {ROOT}/docs/2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md — the sections named in your anchors (use Grep to find them; read them fully) plus §6, §11, §12.
4. {ROOT}/research/synthesis/phase-0.md §5 and every later `research/synthesis/phase-*.md` "settled" section — binding; {ROOT}/research/registers/lcd-test-battery.md — binding.
5. {ROOT}/research/registers/sources.md (cite S-ids), ontology.md (use its vocabulary verbatim; propose terms only where missing), open-questions.md, conflicts.md, scope.md (your R-ids), and the ratified ADRs listed in {ROOT}/research/LEDGER.md that your row's blocking-edges and anchors point to (do not contradict a ratified ADR; deviate only by proposing a superseding ADR).
6. Upstream dossiers you depend on (read fully; do not re-decide what they decided; flag disagreements as conflicts):
{upstream}

## Write (persist BEFORE returning — your prose is otherwise lost)
A. **Dossier:** {ROOT}/research/dossiers/{ws['id']}.md using the dossier template, every section present. Target 2,500–6,000 words of dense, specific content: real file paths for source-code precedents, real citations (S-ids or new sources), explicit interface-contract sketches (operations, inputs/outputs, invariants, failure modes), data-model sketches, acceptance-criteria sketches, build-stage suggestions, and the T-LCD tests your decisions must pass.
B. **Proposed ADRs:** one file per load-bearing decision at {ROOT}/research/decisions/proposed/{ws['id']}-adr-<n>.md using the ADR template with `Status: proposed` and ALL FIVE evidence fields (a–e). Produce at least one. Do NOT allocate ADR-NNNN numbers — synthesis renumbers.
C. **Register additions sidecar:** {ROOT}/research/dossiers/{ws['id']}.additions.md with four sections, using temporary ids that synthesis will renumber:
   - `## Sources` — markdown table with columns exactly: | temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds | (temp-id like S-{ws['id']}-01). Also list which existing S-ids you actually opened.
   - `## Open questions` — | temp-id | question | resolver | due | blocking? |
   - `## Conflicts` — | temp-id | parties | contradiction | proposed resolution |
   - `## Ontology terms` — | term | definition | notes |
   Do NOT edit the shared registers directly (other agents are writing concurrently).
D. Scratch notes go under /tmp/hh-scratch/{ws['id']}/.

## Return
Return ONLY the structured JSON result (schema enforced). Keep `notes_for_synthesis` to the 3–6 things the synthesis pass most needs to know (contract decisions neighbours must agree with, disagreements with upstream dossiers, anything provisional you leaned on).
"""

for ws in a['workstreams']:
    open(f'{out}/{ws["id"]}.md', 'w').write(research_brief(ws))

ids = [w['id'] for w in a['workstreams']]
synth = f"""# Synthesis brief — Phase {N}

You are the **Phase {N} synthesis agent** for the HarnessHarness research program (doc 3 §11.5 steps 3–6, §7.1). Fresh context. Work carefully and persist everything.

## Read first
1. {ROOT}/docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md — §3, §4, §6, §7.1, §11.5.
2. {ROOT}/research/TEMPLATES.md, {ROOT}/research/LEDGER.md (note the "Next number" for ADRs and each register's "next id"), every register under {ROOT}/research/registers/, and every `research/synthesis/phase-*.md`.
3. Every ratified ADR listed in the LEDGER ADR index (at least skim titles + Decision sections; read fully those the phase's dossiers cite).
4. The phase's dossiers, their `.additions.md` sidecars, and their proposed ADRs under {ROOT}/research/decisions/proposed/ — workstreams: {', '.join(ids)}. A per-workstream results digest is appended to your prompt by the orchestrator.
5. Adjacent dossiers from earlier phases that these touch (LEDGER blocking-edges).

## Phase-specific synthesis focus
{a['synthesisFocus']}

## Do (in this order; each step persists to disk)
1. **Fold registers.** Append every sidecar addition into the shared registers with proper sequential ids (continue from each register's "next id"); dedupe across sidecars; promote opened sources S→P; rewrite the temp ids inside the dossiers/proposed ADRs to the final ids (Edit). Append ontology terms to registers/ontology.md §5 with owner + status (`proposed` or `ratified`); bump the ontology version line if you ratify terms. Never silently drop an addition.
2. **Reconcile contradictions.** Compare the dossiers with each other, with earlier ratified ADRs, and with the ontology. Log every contradiction in registers/conflicts.md (new CF-ids) with a resolution and status. Apply the phase focus above.
3. **Disposition ADRs.** For each proposed ADR: verify all five §3.3 evidence fields and that no C0 decision rests on `provisional` evidence (check cited S-ids against sources.md). Ratify, amend (edit the text and log what changed), or reject (keep the file, status rejected with rationale). Move to {ROOT}/research/decisions/ADR-NNNN.md with sequential numbers from the LEDGER "Next number", set `Status:`, delete the proposed/ copy. Update the LEDGER ADR index. If a needed load-bearing decision has NO proposed ADR, author it yourself citing the dossiers.
4. **Scope register.** Update registers/scope.md statuses/owners; log any scope addition/removal as an ADR (never silent).
5. **Neutrality + naming audit.** Grep the phase dossiers and ADRs for language/runtime/framework commitments beyond what the WS-L1 decision ADR permits, and for residual "MetaHarness" outside historical quotes; fix and record each fix in conflicts.md.
6. **LEDGER.** Set each phase workstream row to `done` with its ADR ids (or `blocked` with reason); set the phase-gate row status to `passed` or `failed: <reason>`; update the "Next number"/"next id" lines; append a "### Phase {N} — <date>" block to the Phase progress log with 8–15 bullets.
7. **Synthesis memo.** Write {ROOT}/research/synthesis/phase-{N}.md: decisions ratified, contract convergence achieved, open items, and a 'settled for the next phase' section with explicit binding instructions.
8. Do not write any framework implementation code, and do not run decompose-spec/orchestrate-build/implement-spec.

## Gate criteria to enforce
{a['gateCriteria']}

Return ONLY the structured JSON (schema enforced). `progress_summary` is a ~150-word summary for the human sponsor.
"""
open(f'{out}/synthesis.md', 'w').write(synth)
print(f'phase {N}: {len(ids)} briefs + synthesis.md -> {out}')
