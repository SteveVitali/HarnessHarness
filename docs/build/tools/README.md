# docs/build/tools — the retained supplemental coverage/routing guard

`hh-supplement-guard/1` (tool version 1.0.0), landed by R5.1 under the GATE-G8
bounded bootstrap authorization. It is the guard the supplemental spec §9.4
requires before any product dispatch; the named bootstrap exception covers only
its own implementation.

## Properties

- **Read-only.** It reads existing records and emits digests, counts and
  diagnostics on stdout. It never writes build state and is never a second
  state owner; every mutation in the fixture battery happens in a mktemp
  sandbox it owns.
- **Versioned.** Output carries `schema: hh-supplement-guard/1` and
  `guard_version`. `--version` prints them; `--json` emits the full record.
- **Supplemental, not a replacement.** The stock external build-memory
  validator (`check-build-memory.sh`) and the repo row-tool checks run
  unchanged. The guard consumes the stock checker's `build-memory-check/2`
  JSON sidecar for the scoped-baseline comparison — it does not reimplement it.

## Invocation

```sh
bash docs/build/tools/check-supplement-guard.sh            # runs stock checker via AGENT_SKILLS_ROOT
bash docs/build/tools/check-supplement-guard.sh --json     # machine-readable
python3 docs/build/tools/supplement_guard.py --root DIR \
    --checker-json FILE                                  # consume a captured checker output
python3 docs/build/tools/supplement_guard.py --no-checker  # structural checks only; baseline noted skipped
```

`CHECK_BUILD_MEMORY` may point at an alternate checker script. Exit codes:
`0` all checks pass (a `HOLD`/`RETURN PASS` route is a legal verdict),
`1` at least one error diagnostic, `2` invocation or missing prerequisite —
a missing prerequisite is never a pass.

## Checks

| Code family | Validates |
|---|---|
| `coverage-*`, `frozen-source` | The 180 literal candidates/aliases: 167 adopted / 12 merged / 1 superseded; frozen supplement sha256; md and json registers agree row-for-row |
| `owner`, `ac-anchor`, `layer` | Exactly one primary owner per adopted target; ticket file + numbered AC anchor resolve; D/E/R/P layer vocabulary; §/AC supplemental hooks resolve; contributors are chain rows |
| `alias-predicate`, `disposition` | Aliases retain literal predicate bytes (source_sha256) and resolve to adopted targets sharing the primary owner |
| `chain-*`, `dependency`, `forward-dependency` | Grammar-valid unique chain ids/files; skeleton contracts carry no Run line; every `Depends on:` resolves backward |
| `tail-roles` | Active-round tail keeps capstone ×3, operator-acceptance gate, reconcile ×3, docs ×2 in order |
| `nextticket`, `old-gate-routing`, `skeleton-dispatch`, `pending-prerequisite`, `failed-gate`, `last-completed`, `legacy-completion` | `nextTicket` is the active round's first row without landed evidence (BUILD_INDEX row for work kinds; PASSED GATE DECISIONS for gates; recorded decision/non-pending readout for human rows); legacy GATE-G1 completion evidence byte-preserved; skeletons and pre-round rows never dispatch |
| `signature-reuse` | Active-round gate verbatims never replay a pre-round signature from GATE DECISIONS or readouts |
| `chain-tip`, `git-ancestry` | `chainTip`/`buildBranchBase` agree; `pinnedBaseSha` is a git ancestor of the chain tip (noted skipped outside a checkout) |
| `unexplained-diagnostic`, `missing-historical-diagnostic`, `stock-violation`, `baseline` | Stock checker diagnostics match the reviewed baseline's exact check/path/row classes one-for-one — never an arbitrary warning count; violations are always fatal |

Route verdicts: `DISPATCH <id>` (first unlanded row, deps satisfied, Run line
present), `HOLD` (gate/human pending — RETURN PASS, or round complete),
`REFUSED` (skeleton first-unlanded or unmet prerequisites — recorded as errors).

## Fixtures

`docs/build/fixtures/supplement-guard/run-fixtures.sh` mutates scratch copies
of the inputs per `cases/<name>/patch.sh` and requires the named fatal
diagnostic in `cases/<name>/expected.txt`. `checker-green.json` is a stand-in
`build-memory-check/2` record of the conforming diagnostic set — a fixture,
not a live validation result.
