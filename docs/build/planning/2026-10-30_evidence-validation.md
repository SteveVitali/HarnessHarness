# Round 5 — decomposition validation record

This validates authored documentation/build records only. No Rust build/product tests, task/backend/provider/container execution, numerical study, outside review, outreach or publication occurred. Future retained guard/native dispatch proof is **not implemented**. Command failures/skips are not promoted to product passes.

## Checks

| Check | Result and scope |
|---|---|
| External stock build-memory whole tree | Exit 0; zero violations, eight exactly scoped historical warnings. The transient new preauthorization-expiry warning was repaired; it is not waived. |
| External staged-history audit | Exit 0; zero violations/warnings. Temporary independent Git index includes actual uncommitted documentation, including new files. Real user index unchanged; no commit created. |
| External planning-ledger freshness | Exit 0 required on the final DONE/P5 ledger; retained report linked below. |
| External backlog | Exit 0; 165 expected historical sources, 66/66 engineering closed and 66/66 requirement satisfied. These are previous-build totals, not new-program acceptance. |
| Repo living-docs | Exit 0; 387 living files scanned, no personal paths. Historical build records retain their applicable mode. |
| Repo row-tool checks | Exit 0; zero-open-deferral index legal, discharged row omitted, single-section/unknown-ID/BL readers correct. |
| Existing live-evidence catalogue | Exit 0; 34 historical records, cumulative $25.2598. No new live execution/spend and no inherited authority. |
| Revisit catalogue | Exit 0; generated catalogue in sync; no ADR accepted or frozen spec edited. |
| Frozen-input/source audit | 180 originals, 167 adopted/12 merged/one superseded; 23 AC,24 OL,219 explicit anchors; ratified supplemental digest unchanged. 24 accepted input hashes unchanged. |
| Scoped partition/history/link audit | 79 unique contracts; backward dependencies, skeleton no-Run, nine tail roles, exact literal rows/layers/contributor symmetry. 2,819 baseline protected files unchanged; old manifest/ledger historical lines preserved, replaced head archived exactly. Final scope/link counts in retained JSON. |
| Independent review | Initial findings repaired; partition and exact-predicate/layer focused rereviews clean. This is review evidence, not runtime verification. |
| Diff whitespace | `git diff --check` and new Markdown scoped whitespace checks pass. |

## Commands and method

External tools use the pinned `agent-skills` checkout resolved as documented by MAINTAINING. Commands executed include:

```sh
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" . --json <report>
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" . --staged --json <report>
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" . --planning docs/build/planning/2026-10-30_evidence-planning-ledger.md --json <report>
bash "$AGENT_SKILLS_ROOT/skills/reconcile-build/scripts/check-backlog.sh" --json <report>
bash scripts/check-living-docs.sh
bash scripts/check-bm-tools.sh
bash scripts/check-live-evidence.sh
bash scripts/gen-revisit-triggers.sh --check
git diff --check
```

The staged audit used `GIT_INDEX_FILE` in a temporary directory, populated from HEAD plus the explicitly selected changed/untracked documentation files; before/after hashes prove the real index unchanged. This checks the actual pending artifacts; a no-op HEAD range or committed-HEAD quickstart would not do so. No quickstart or product test pass is claimed.

Temporary read-only authoring scripts compared source literals/hashes, header/dependency/manifest entries, exact owners/aliases/layers, source AC/OL sets, protected historical bytes and relative targets/anchors. Their diagnostic output is retained; they are deliberately **not** the separately authorized retained coverage/routing guard, and they exercise no launch path. The external stock validator remains unchanged.

[Scoped historical diagnostic baseline](2026-10-30_evidence-legacy-baseline.json) preserves exact check/path/row classes, including the genuine second GATE-G1 PASSED reading. It does not allow arbitrary eight warnings or rewrite old gate history. New/unknown diagnostics remain fatal under the planned guard.

## Final check evidence

Final reports are retained under `docs/build/reports/decomposition/2026-10-30/` after the last checks. No execution readiness or operator gate is satisfied by this record.

- 2026-10-30T03:17:48Z — Final authored scope: 79 contracts, 180 candidates/167 primary targets/13 aliases, 24 OL/23 AC/nine full-tail roles. Scoped link audit checked 95 Markdown files, 2,628 relative links and 1,578 anchors, zero diagnostics. 2,819 protected baseline files unchanged. Zed opened the three review surfaces, exit 0.
- Retained diagnostics: [whole tree](../reports/decomposition/2026-10-30/build-memory.json), [backlog](../reports/decomposition/2026-10-30/backlog.json), [authoring structure/links](../reports/decomposition/2026-10-30/authoring-audit.json), [actual staged history](../reports/decomposition/2026-10-30/history.json), [planning freshness](../reports/decomposition/2026-10-30/planning.json). These are documentation/build-record checks only.
