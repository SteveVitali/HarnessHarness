# docs/build/tools — the retained supplemental coverage/routing guard

`hh-supplement-guard/1` (tool version 1.1.0), landed by R5.1 under the GATE-G8
bounded bootstrap authorization and extended to the Round-6 experiments
supplement by R6.1 (GUARD-X) under GATE-G20's scoped bootstrap authorization
(REQ-XH-001/002/003/004/008/024/025/026, AR-31). It is the guard the
supplemental spec §9.4 and experiments spec REQ-XH-002 require before any
product dispatch; the named bootstrap exceptions cover only the guard's own
implementation rows. One guard, two frozen sources (CC1): the Round-5
evidence-and-adoption register's checks and counts stay byte-for-byte in
force beside the Round-6 REQ-X register.

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
| `tail-roles` | Active-round tail keeps capstone ×3, operator-acceptance gate, reconcile ×3, docs ×2 in order — or, when the round banner declares `tail=minimal` (added 2026-11-07 for Round 6 by operator decision), capstone ×2, operator-acceptance gate, docs ×1 |
| `nextticket`, `old-gate-routing`, `skeleton-dispatch`, `pending-prerequisite`, `failed-gate`, `last-completed`, `legacy-completion` | `nextTicket` is the active round's first row without landed evidence (BUILD_INDEX row for work kinds; PASSED GATE DECISIONS for gates; recorded decision/non-pending readout for human rows); legacy GATE-G1 completion evidence byte-preserved; skeletons and pre-round rows never dispatch |
| `signature-reuse` | Active-round gate verbatims never replay a pre-round signature from GATE DECISIONS or readouts |
| `chain-tip`, `git-ancestry` | `chainTip`/`buildBranchBase` agree; `pinnedBaseSha` is a git ancestor of the chain tip (noted skipped outside a checkout) |
| `unexplained-diagnostic`, `missing-historical-diagnostic`, `stock-violation`, `baseline`, `classified-diagnostic` | Stock checker diagnostics match the reviewed baseline's exact check/path/row classes one-for-one — never an arbitrary warning count; violations are always fatal. Append-only dated `amendments` may classify a bounded warning *family* (check + file + message patterns + `max_emitted`) with recorded provenance: emissions within the bound are explained, a further warning still fails `unexplained-diagnostic`, and an unemitted family never fails `missing-historical-diagnostic` (emission is validator-version dependent). The R6.1 amendment classifies the 46 observed legacy `reqcov` warnings on rows 083–164f (list sha256 `n/a{empty_at_presentation}` at GATE-G20) |
| REQ-X register (same `coverage-*`, `owner`, `ac-anchor`, `alias-predicate`, `disposition`, `layer`, `foreign-id`, `omitted-id`, `duplicate-id`, `coverage-md`, `frozen-source`, `coverage-json` families) | The 456 REQ-X candidates: 370 adopted / 81 merged / 5 superseded; the second frozen-source digest (`docs/experiments-spec.md` sha256 `4329b274…`); ids sourced from spec Appendix A (no `id="req-…"` anchors); `spec_anchor` existence; AC hooks resolved `appendix-a-row` under the widened `AC-\d+(\.\d+)*-\d+`; `literal_row`/`source_sha256` and `literal_source(_sha256)` recompute and verbatim-in-source; one primary owner per adopted id with contract file + `acceptance_anchor`; owner-kind and CAP6.2 routing rules; md owner table + owner index agree with json |
| `plan-*` (table, totals, chain, deps, tail, rows, skeleton, human, size, fold, linearization, owner, shared) | The frozen Round-6 partition plan as a committed planning record — the legs folded from `check_round6_coverage.py` (row table, totals, backward-only deps and the B′ permutation, minimal tail, five gate-dependent skeletons, HUMAN placement, sizes, record-only fold, row-table↔register owner equality, shared-decision ordering) so one implementation remains (SR-23, CC1) |
| `gate-criterion`, `gate-disposition`, `readout-fields` | Every active-round gate contract's `## Criterion (verbatim from the spec)` `> |` quotes appear in the frozen spec (REQ-XH-025); a recorded PASSED disposition-gate decision carries a pre-registered form (G27 `run`/`not_run{…}`, G28 `triggered{…}`/`not triggered; Wc unused`, G29 `full`/`fallback`, G30 `run{…}`/`end_after_w2`; a basis containing `FAILED` or `NOT PASSABLE` stays non-PASSED); the spend gates carry the REQ-XH-004 readout fields (model authorization, compute authorization separately, quote id, hard-cap formula) |
| `human-marker`, `human-deferred` | Each HUMAN contract names owner, trigger, exit and withheld claims (REQ-XH-026); a `DEFERRED` readout must cite an existing DEFERRALS row naming the HUMAN row — `DECLINED`, `NOT_NEEDED` and `N/A` land the row |
| `paid-body`, `window-order` | A paid window body is a skeleton until its gate is PASSED (REQ-XH-001/024); a later-round (7/8) row may appear only as a skeleton; per window the PROV gate → window body → HUMAN-reconcile entry stays an ordered subsequence (consolidated HUMAN-H18; GATE-G20 D-1/D-2) |

Route verdicts: `DISPATCH <id>` (first unlanded row, deps satisfied, Run line
present), `HOLD` (gate/human pending — RETURN PASS, or round complete),
`REFUSED` (skeleton first-unlanded or unmet prerequisites — recorded as errors).

## Fixtures

`docs/build/fixtures/supplement-guard/run-fixtures.sh` mutates scratch copies
of the inputs per `cases/<name>/patch.sh` and requires the named fatal
diagnostic in `cases/<name>/expected.txt`. `checker-green.json` is a stand-in
`build-memory-check/2` record of the conforming diagnostic set — a fixture,
not a live validation result.

## dispatch-driver — the guarded native launch path (R5.2)

`hh-dispatch-driver/1` (tool version 1.0.0), landed by R5.2 under the same
GATE-G8 bootstrap authorization. It performs every step of the coordinator's
launch path **up to the literal spawn** and validates the recorded result —
on this harness the spawn primitive is `run_subagent` (profile
`subagent_general`), callable only by the root coordinator, so the driver
emits the exact envelope and the coordinator executes it.

```sh
bash docs/build/tools/dispatch-driver.sh plan --leg happy --ci-json FILE [--json]
bash docs/build/tools/dispatch-driver.sh validate --envelope FILE --transcript FILE
```

`plan` runs the ordered preflight — `dispatch-target` (ledger must route
`subagent`), `guard` (hh-supplement-guard/1 must PASS with a `DISPATCH` route;
it is invoked, never mocked), `ancestry` (chainTip descends from
buildBranchBase, pinnedBaseSha is an ancestor), `ci` (a recorded
`ci-boundary/1` read for the previous ticket's PR must be green; a missing
record is never green), `resolve`, `emit` — and either emits one
`hh-dispatch-envelope/1` (primitive, profile, title, prompt, recorded guard
digests/counts/versions, expected base sha, transcript contract) inside a
`hh-dispatch-decision/1`, or refuses with named diagnostics and emits nothing.
Each gate is independently load-bearing.

`validate` checks a `hh-dispatch-transcript/1` against the envelope bytes:
`guard.pre` before the spawn and `guard.post` after the boundary/state
transition (pre/post input digests must be identical — build-memory state
untouched), exactly the contracted spawn count, prompt/profile/row identity
by sha, every attempted spawn answered (no-progress is not a launch), zero
spend, writes confined to the allowed report path, `tree_status: clean`, and —
for the resume leg — a `ledger_reread` event before the resumed stub acts.
Diagnostics: `guard-required` (absent/mocked guard), `ordering`,
`envelope-mismatch`, `spawn-count`, `state-mutation`, `product-mutation`,
`provider-budget`, `no-progress`, `stale-state`, `validation-error`.

### dispatch fixtures

`docs/build/fixtures/dispatch-driver/run-fixtures.sh` — plan-mode cases build
a git-initialized scratch input tree and require a named refusal (or `EMIT`);
validate-mode cases pair a committed envelope with a `transcript.json` and
require `VALID` or the named diagnostic. `ci-pass-209-e2b24b8.json` and
`ci-fail-209-ef40a13.json` are the **real recorded** ci-boundary/1 reads for
PR #209 (pass at closeout head; inherited-red record at commit `7b5ac8df`);
`ci-pending.json` and `checker-green.json` are labeled stand-ins
(`capture_kind`). The stub-leg transcripts returned by the coordinator are
validated against the committed `envelope-*.json`; `transcripts/valid-*.json`
are stand-in contracts, not execution records.

## p1_records.py — `hh-p1-records-check/1` (R6.2, REQ-XA-106)

Stdlib, read-only records-in verifier for the phase-1 provenance tree. It
verifies archive + raw hashes (RC-1), dataset content digests when a tree is
supplied (RC-2, else `skipped(dataset_absent)`), raw ↔ packet slots (RC-3),
fold histograms + the REQ-XA-006 arm fold (RC-4), fixture ↔ docs byte
equality (RC-5) and committed-emission equality (RC-6). Exit 0/1/2; a
missing input exits 2. Battery: `bash docs/build/fixtures/p1-records/run-fixtures.sh`.
