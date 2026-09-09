# Research Program Templates

Binding templates for every research subagent and synthesis pass (doc 3 §4). Copy verbatim; do not drop sections — write `none` if empty.

## Dossier template — `research/dossiers/WS-XX.md`

```markdown
# WS-XX — <title>

**Track:** <A–L> · **Phase:** <0–5> · **Tier(s) fed:** <C0/C1/…> · **Status:** dossier-written
**Owner agent:** <label> · **Date:** YYYY-MM-DD
**Feeds spec section(s):** <§7.2 numbers> · **Scope register items:** <R-ids>

## 1. Scope & boundary
What this workstream decides; what it explicitly leaves to neighbours (name them).

## 2. Key questions
Numbered. Include the doc 3 §5 questions verbatim plus any you added.

## 3. Sources consulted (4 streams — mark which were actually opened)
| stream | source (S-id or new) | opened? | what it contributed |
Academic · OSS source-code audit (vertical-slice: file paths read) · Protocol specs · Production write-ups.
New sources: append to `registers/sources.md` with next S-id and list them here.

## 4. Findings
### 4a. Mechanism evidence (a failure mode exists / an architecture is implementable)
### 4b. Performance evidence (benchmark lifts) — each tagged `provisional` if 2026 single-team/preprint
### 4c. Source-code precedents (repo, path, what it does)

## 5. Disconfirming evidence & negative results
The strongest case against each recommendation. Mandatory.

## 6. Recommendation for the spec
Interface-contract sketch (language-agnostic: operations, inputs/outputs, invariants, failure modes) · data-model sketch · acceptance-criteria sketch · build-stage suggestion (C-tier + Stage N) · dependencies on other workstreams.

## 7. Open questions raised
Append to `registers/open-questions.md` (next OQ-id) and list ids here.

## 8. Conflicts flagged
Append to `registers/conflicts.md` (next CF-id) and list ids here.

## 9. Ontology terms proposed
Append to `registers/ontology.md` §5 and list here.

## 10. Confidence
high / medium / low — and why (evidence tier mix, replication, precedent).

## 11. ADRs produced
ADR-NNNN (proposed) — one line each.

## 12. Status
`dossier-written`
```

## ADR template — `research/decisions/ADR-NNNN.md`

```markdown
# ADR-NNNN — <decision title>

**Status:** proposed | ratified | amended | rejected | superseded-by ADR-MMMM
**Owner workstream(s):** · **Phase:** · **Related:** (ADRs, CF-ids, OQ-ids, R-ids)

## Context
## Options considered
Numbered; one line why each non-chosen option was rejected.
## Decision
## Evidence (doc 3 §3.3 synthesis contract — all five mandatory)
- (a) Mechanism evidence: [S-ids]
- (b) Source-code precedent: repo + path, OR `no-precedent / novel`
- (c) Disconfirming evidence considered: [S-ids] and why it does not overturn the decision
- (d) Conditionality: which model tiers / task classes / budgets it assumes
- (e) Build-stage assignment: C-tier + Stage N
## Consequences
## Reversibility
low | medium | high — and what it costs to reverse
```

## Evidence discipline reminders (doc 3 §3.1)

- Tag every 2026 single-team/preprint result `provisional`. A `provisional` claim may **never** be load-bearing for a C0 decision.
- Separate mechanism from performance evidence.
- Discount "evolution beats baseline" unless compute/feedback/eval-matched.
- Record the strongest disconfirming evidence.
- Keep everything **language/ecosystem-agnostic** (no language, runtime, or framework commitments) until WS-L1 ratifies.

## Research-agent JSON return schema

```json
{
  "ws_id": "WS-XX",
  "dossier_path": "research/dossiers/WS-XX.md",
  "recommendations": ["..."],
  "adrs_proposed": [{"id": "ADR-NNNN", "title": "...", "path": "research/decisions/ADR-NNNN.md"}],
  "open_questions": [{"id": "OQ-NNN", "question": "..."}],
  "conflicts_flagged": [{"id": "CF-NNN", "summary": "..."}],
  "ontology_terms": [{"term": "...", "definition": "..."}],
  "sources_added": [{"id": "S-NNN", "title": "...", "provisional": true}],
  "confidence": "high|medium|low",
  "language_neutral": true
}
```
