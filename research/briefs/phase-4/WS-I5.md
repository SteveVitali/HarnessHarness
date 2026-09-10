# Research brief — WS-I5 (Phase 4)

You are a fresh research subagent in the HarnessHarness program (Phase 4). You own workstream **WS-I5 — Evolution service as a governed deployment pipeline; safe self-modification; matched-budget gates**.

## Your contract (doc 3 §11.4, binding)
Conduct deep research and write a durable dossier. Triangulate **at least 2 of the 4 source streams** (doc 3 §3.2): (1) academic literature — start from the seeded Source Registry and doc 2, extend via WebSearch/WebFetch (arXiv abstract + HTML pages are fine); (2) OSS source-code audit — actually read source of the named repos using the vertical-slice method (clone shallowly into /tmp/hh-repos/<name> with `git clone --depth 1` if not already present, then read the specific files that implement your concern; record file paths and the commit you read); (3) protocol specifications (MCP, A2A, ACP, etc.); (4) production engineering write-ups (mine mechanisms, discount product-specific assumptions).

Apply the evidence discipline (doc 3 §3.1) strictly: tag every 2026 single-team/preprint claim `provisional`; separate **mechanism** evidence from **performance** evidence; discount unmatched-budget "X beats baseline" claims; record the strongest **disconfirming** evidence for every recommendation. A provisional claim may never be load-bearing for a C0 decision.

**Contracts stay language/ecosystem-neutral** (operations/inputs/outputs/invariants). The WS-L1 decision ADR (see the LEDGER ADR index) governs the only permitted references to the chosen ecosystem.

## Your workstream row (doc 3 §5, quoted)
- **ID:** WS-I5
- **Scope / key questions:** Design the evolution service per ADR-0002: candidate generation (from traces: AHE/RHO/Meta-Harness/Harness-R1 families) → explicit failure hypothesis → targeted counterexample set → matched-budget evaluation → held-out regression → cross-model compatibility → security-invariant check (H1: the service cannot enlarge its own authority) → canary → rollout → retirement test; separation of proposal and deployment; edits as typed IR diffs (A3) on registry components (J2); attribution hooks (I7); experiment engine (J3) as the evaluation substrate; the seven-point evidence standard as acceptance criteria; the taxonomy of self-improving harnesses (offline search, continual compilation, test-time adaptation, learned editing, co-evolution, meta-harnessing).
- **Primary anchors & sources:** doc 2 §5.8 (all families + taxonomy + credit/validation methods + negative evidence), §11 ('treat evolution as a deployment pipeline'; 'security kernel before self-modification'), §12 direction 3; AHE (S-061), RHO (S-063), Meta-Harness (S-080), Harness-R1 (S-081), MemoHarness (S-066), JIT-Agent (S-071), HarnessBank (S-087), HarnessLens (S-089), HarnessEvolve (S-090), Safe Harness Self-Evolution (S-094), SafeEvolve (S-086); COUNTER: Rethinking (S-083), HarnessDev (S-075), EVOHARNESSBENCH (S-091), Where-does-value-live (S-092), PRISM (S-093); lineage: DGM (S-026/S-027 — read source), AFlow (S-025 — read source), GEPA (S-028), ADAS (S-023)
- **Feeds spec area:** Spec §5 (evolution service); WS-I6, I7, I8, F4
- **Tier(s):** C4
- **Scope-register items:** R-2.9.5

**Additional phase-specific instructions:**
NAMING (ADR-0011, sponsor decision 2026-09-09): the product is **HarnessHarness**; use that name (never 'MetaHarness') except when quoting historical text. Before writing, read the synthesis memos research/synthesis/phase-0.md §5 and research/synthesis/phase-3.md 'settled' sections (binding), research/registers/lcd-test-battery.md, and research/synthesis/l1-inputs.md / the ratified WS-L1 language ADR if present (the language decision is ratified at end of Phase 1; after that, you may reference the chosen ecosystem only where the brief allows, and must still keep contracts described abstractly).
Read DGM and AFlow source for how candidates are represented, evaluated, and archived. Deliver: pipeline stage contracts (inputs/outputs/gates) with the matched-budget gate normative; edit representation as typed IR diff; governance invariants (proposal ≠ deployment; no self-authorization; reversibility; expiry); per-family adapter sketches; acceptance criteria = the seven-point standard; ADR(s) with explicit matched-budget conditionality and maturity flags.

## Read first (in this order)
1. /Users/stevenvitali/MetaHarness/research/TEMPLATES.md — the dossier + ADR templates you must follow exactly.
2. /Users/stevenvitali/MetaHarness/docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md — §1, §2, §3, §5 (your row and neighbours), §7.2 (spec skeleton you feed).
3. /Users/stevenvitali/MetaHarness/docs/2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md — the sections named in your anchors (use Grep to find them; read them fully) plus §6, §11, §12.
4. /Users/stevenvitali/MetaHarness/research/synthesis/phase-0.md §5 and every later `research/synthesis/phase-*.md` "settled" section — binding; /Users/stevenvitali/MetaHarness/research/registers/lcd-test-battery.md — binding.
5. /Users/stevenvitali/MetaHarness/research/registers/sources.md (cite S-ids), ontology.md (use its vocabulary verbatim; propose terms only where missing), open-questions.md, conflicts.md, scope.md (your R-ids), and the ratified ADRs listed in /Users/stevenvitali/MetaHarness/research/LEDGER.md that your row's blocking-edges and anchors point to (do not contradict a ratified ADR; deviate only by proposing a superseding ADR).
6. Upstream dossiers you depend on (read fully; do not re-decide what they decided; flag disagreements as conflicts):
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-I2.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-I3.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-H1.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-B4.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-J2.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-J3.md
  - /Users/stevenvitali/MetaHarness/research/dossiers/WS-A3.md

## Write (persist BEFORE returning — your prose is otherwise lost)
A. **Dossier:** /Users/stevenvitali/MetaHarness/research/dossiers/WS-I5.md using the dossier template, every section present. Target 2,500–6,000 words of dense, specific content: real file paths for source-code precedents, real citations (S-ids or new sources), explicit interface-contract sketches (operations, inputs/outputs, invariants, failure modes), data-model sketches, acceptance-criteria sketches, build-stage suggestions, and the T-LCD tests your decisions must pass.
B. **Proposed ADRs:** one file per load-bearing decision at /Users/stevenvitali/MetaHarness/research/decisions/proposed/WS-I5-adr-<n>.md using the ADR template with `Status: proposed` and ALL FIVE evidence fields (a–e). Produce at least one. Do NOT allocate ADR-NNNN numbers — synthesis renumbers.
C. **Register additions sidecar:** /Users/stevenvitali/MetaHarness/research/dossiers/WS-I5.additions.md with four sections, using temporary ids that synthesis will renumber:
   - `## Sources` — markdown table with columns exactly: | temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds | (temp-id like S-WS-I5-01). Also list which existing S-ids you actually opened.
   - `## Open questions` — | temp-id | question | resolver | due | blocking? |
   - `## Conflicts` — | temp-id | parties | contradiction | proposed resolution |
   - `## Ontology terms` — | term | definition | notes |
   Do NOT edit the shared registers directly (other agents are writing concurrently).
D. Scratch notes go under /tmp/hh-scratch/WS-I5/.

## Return
Return ONLY the structured JSON result (schema enforced). Keep `notes_for_synthesis` to the 3–6 things the synthesis pass most needs to know (contract decisions neighbours must agree with, disagreements with upstream dossiers, anything provisional you leaned on).
