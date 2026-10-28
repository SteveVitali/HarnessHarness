# AGENTS.md — documentation

## Purpose

- Apply the root `AGENTS.md` plus this guide to `docs/`. The [documentation map](README.md) assigns each surface its edit mode.
- Help readers use the current project while preserving the contracts and evidence that explain its history.

## Key files

| File | Lines (approx) | Purpose |
|---|---|---|
| `docs/README.md` | ~100 | Navigation and authoritative documentation mode map |
| `scripts/bm-row.sh` | ~200 | Read a single register row or its status |
| `scripts/spec-section.sh` | 71 | Read one named spec section |
| `scripts/check-live-evidence.sh` | ~150 | Validate committed live-evidence records |

## Build & Test

- Run from the repository root:

```sh
bash scripts/check-living-docs.sh
bash scripts/check-bm-tools.sh
bash scripts/check-live-evidence.sh
bash scripts/gen-revisit-triggers.sh --check
```

- For build-ticket records, run the external build-memory and backlog checks described in [MAINTAINING.md](../MAINTAINING.md#build-memory-tools). Repo-local row-tool checks are not substitutes.
- Check relative links and anchors in every changed living document. Execute documented offline examples before claiming they work; record prerequisites and actual results. Live examples follow ADR-0363's evidence workflow.

## Code conventions

| Mode | Correct ✓ | Wrong ✗ |
|---|---|---|
| Living | Edit the current guide; verify its commands against source | Repeat an old build report as current capability |
| Generated | Change the source and regenerate | Patch the rendered index or catalogue |
| Historical / append-only | Append a dated correction or new record | Rewrite the old decision or delete a discharged row |
| Frozen | Follow the explicit amendment protocol | Reword the original record during a docs refresh |

- The research reports, build ADRs, ticket contracts, and build evidence keep their assigned modes. A human-facing summary can explain a later correction without modifying the original report.
- Cite the exact dated evidence when describing a tested capability. Label a fixture, foreign replay, skipped run, and live-service run distinctly.
- Use repository-relative links. Keep local checkout paths, credentials, and service tokens out of living docs; secret presence is `provided: yes/no` in records.
- Regenerate the ADR index with `bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/adr-index.sh" docs/adr`; use the external-tool checkout setup in [MAINTAINING.md](../MAINTAINING.md#build-memory-tools). Revisit triggers use `bash scripts/gen-revisit-triggers.sh`; third-party notices use `bash scripts/gen-third-party-notices.sh`.

## Critical gotchas

1. Start with `bash scripts/deferrals-open.sh`, then read named rows with `scripts/bm-row.sh`. Do not open the whole deferrals, backlog, coverage, or build ledger to orient yourself.
2. A build report describes its recorded commit and environment. Repository visibility, a release tag, or a newly edited README does not discharge HUMAN-H7/R4.18 or raise an evidence class.
3. A closed row may contain older OPEN text. Use the dated disposition and preserve its history.
4. `docs/build/logs/` is ignored. Force-add only the specific evidence record intentionally being committed.
5. For README examples, `scripts/check-quickstart.sh` extracts the marked block from committed HEAD in a fresh clone. Running it before committing an edited README checks the old version.

## Terminology

| Term | Meaning |
|---|---|
| Living | Current guidance that may be revised in place |
| Historical | A record of what happened, corrected by a new dated entry |
| Evidence class | A scoped assertion backed by the required record; see `MAINTAINING.md` |

## Do

- Keep the navigation map and human/agent instructions consistent; verify facts against source and dated records.

## Don't

- Do not rewrite contracts or evidence for readability, infer a human approval, or turn an unexecuted check into a pass.
