# I-3 repair — allowlist `docs/build` reader literals for R6.48–R6.50 emitters

Harness: devin-cli (repair worker) · Branch `codex/i3-allowlist-repair` ·
PR base `codex/harnessharness-cap6.3` @ `65d5b479` · offline, USD $0.00, no
merges, no secrets. Scope: the GATE-G32 operator ruling, verbatim —
**"Fix with allowlist"** — for INTEGRATION_PLAN item I-3 (CAP6.3 §1c stack-CI
red diagnosis: `check-ci-gating` readers scan, 57 un-allowlisted
`docs/build/**` literals in the R6.46/48/49/50 emitter sources and batteries
+ "3 stale entries").

## What changed

- `scripts/ci/docs-readers.allow` — **+53 entries, −0**. The 57 failing scan
  hits collapse to 53 unique `path<TAB>line` pairs (the matcher consumes one
  entry for all hits with identical `path`+`line`; four hits were duplicates:
  `fp_e_emit.rs:554/558`, `src/fp_results.rs:791/1165`,
  `r6_49.rs:113/161`, `r6_49.rs:114/162`). Entries replicate the established
  `path<TAB>full hit line<TAB># reason` convention, classified by actual use:
  - **14 write destinations** — `write_record(&repo, "docs/build/reports/…")`
    args in `src/bin/fp_results.rs` → the existing reason verbatim:
    `emitter output destination for the committed record — a write path,
    never opened as a checkout read`.
  - **25 record-content literals** — `Json::str("docs/build/…")` members,
    seal-open `cited`-list entries, `readout` args, `path:` labels, and
    `r#"…"#` fixture lines in `src/bin/fp_e_emit.rs`, `src/fp_results.rs`,
    `src/bin/fp_results.rs:313/435–453`, `tests/r6_46.rs`,
    `tests/r6_48.rs:74/130/135/140/145`, `tests/r6_49.rs:94/154` → the
    existing reason verbatim: `string literal or comment in test/source;
    never opened as a checkout path`.
  - **18 committed-record reads** — `load(&repo, …)` / `load_repo(…)` /
    `repo_root().join(…)` + `std::fs::read` in `src/bin/fp_results.rs`
    (`freeze` inputs), `tests/r6_48.rs` (AC-1 committed consumers, AC
    emitted-packet re-validation), `tests/r6_49.rs` (scan/review/frozen
    packet comparisons) → a new honest reason: `committed build-record
    read — declared fp-emitter-battery input; the committed record is the
    emit/test subject (I-3 allowlist, GATE-G32 ruling)`. These are real
    reads of committed build records — the records are the emitter's and
    battery's declared inputs/outputs; allowlisted per the operator's I-3
    ruling, not reclassified as non-reads.

## Deviation from the task brief — the "3 stale entries" were retained

CAP6.3's "3 stale" (`scripts/spike-s2-boundary.sh` ×2,
`scripts/check-docsrs.sh` ×1) are stale **only under `--readers` mode**,
which skips the gated-script scan (`GATED_SCRIPTS` is populated only in full
mode). The actual CI leg — `test-check-ci-gating.sh` and the `CI step-mode
lint` step — runs the full `check-ci-gating.sh .github/workflows/ci.yml`,
where all three match live hits (`82 = 79 git-grep + 3 gated-script`).
Verified both directions: removing them → `3 violation(s)` (unmatched hits);
retaining them → clean. "Fix with allowlist" is read as making the real CI
leg green, so they stay. Proposed DEFERRALS/LEDGER text below.

## Verification (executed, this worktree)

- `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml` →
  `check-ci-gating: readers scan clean (82 hit(s), 78 allowlisted)` ·
  `check-ci-gating: OK` (was: 57 un-allowlisted + 3 stale under --readers).
- `bash scripts/ci/test-check-ci-gating.sh` → `test-check-ci-gating: OK`
  (all fixture cases, incl. `allowlisted hit`, `stale (rejected)`,
  `unallowlisted readers-scan hit (rejected)`).
- Note: macOS BSD `grep` prints `grep: out of memory` on the ~4 KB fixture
  needle inside `grep -qF "$entry_line" "$hits"` (pre-existing on HEAD;
  exit-2 falls through to the pattern-check branch harmlessly — exact-match
  pairing is string equality, not grep). Cosmetic noise, not a verdict input.

## Proposed coordinator-owned text (not applied — coordinator applies)

- **DEFERRALS.md** (new row, if a row is wanted): `CLOSED — I-3 —
  check-ci-gating readers-scan literals for the R6.46–R6.50 fp emitters;
  discharged by codex/i3-allowlist-repair (allowlist +53; the "3 stale" were
  a --readers-mode artifact and stay live under the full check).`
- **LEDGER.md phase-log line** (proposed): `2026-11-19 · I-3 repair —
  docs-readers.allow +53 literal entries for the fp emitter battery;
  check-ci-gating OK (82 hits / 78 allowlisted); "3 stale" retained (live
  under the full check).`
- **BUILD_INDEX.md**: out of scope for a repair PR unless the coordinator
  mints a row for the intervention.

## I-3 closeout — 2026-11-19T16:56Z

Harness: codex/gpt-6/subagent

**I-3 done — reader allowlist repair, existing PR #420.** The verified repair
head is `c6e0f8b89636d3678eef34348e7b7f895e181d45`, tree
`f49a89fcadccf5ed3df92d4f539cccd1823e4b41`. Normal main inheritance brings the
previously approved CI/publication repairs. Eight original fp_e_emit/r6_46
allowlist literals became stale after that earlier source repair and were
removed; every other owner entry, comment and ordering is preserved. The
three full-mode gated-script entries remain live and retained. The final
independent scan matches 73 hits to 70 exact entries, with no missing or
stale entries. No broadened exception, checker relaxation, new ticket or
BUILD_INDEX row was introduced.

**Hosted verification:** [run 44567178740, attempt 1](https://github.com/SteveVitali/HarnessHarness/actions/runs/44567178740) completed
successfully on the exact repair head above, verified 2026-11-19T16:54:55Z.
All four jobs succeeded: cargo-deny (135712001382), macOS build/test
(135712001785), Ubuntu build/test (135712001792), and gitleaks
(135712001948). Both OS jobs actually passed CI scripts self-tests,
step-mode lint, results-site claims/provenance/a11y checks, Build, Test and
the offline GCE battery. Ubuntu additionally passed clean-clone Quickstart,
Rustdoc warnings and MSRV; those three steps were skipped by the existing
macOS conditions and are not claimed as macOS executions.

**Independent scope review:** root / Codex, separate from the implementation
worker, reviewed the exact repair head/tree above at
2026-11-19T16:32:45Z and returned PASS. Independent byte checks established
that the final allowlist is precisely the owner version minus the eight
reviewed lines (patch SHA256
`de8522f58fa61994468d2d667c8f5de449b488ff032d8ee375a642be8aef7ba0`).
Its remaining 70 full lines equal main as a multiset, differing only in
order; no permission or exception was added. Before this append, the
original I3-REPAIR.md was byte-identical to the owner's record. That record
and the allowlist were the only paths differing from main; all other source,
signed records, ledger and deferrals equalled main. Both expected merge
parents were retained. Root did not redundantly rerun the worker's local
checks; the scope review and subsequent hosted verification are distinct.

**Local evidence and limits:** 32 publication controls and 18 reader
self-tests passed; backlog validation covered 235 sources with engineering
and requirement sums both 66/66. Required tree-memory validation returned
zero violations and 19 legacy warnings (external tooling revision
`e981ec57d487a2c41d416672483b5a4df15b1233`). The original local full reader
checker returned zero but emitted `grep: out of memory`; this remains a
validation limitation, not an unqualified local scanner pass. Supplemental
pre-merge staged-history validation remained FAIL with two findings: the
exact R6.51 ledger entry and DF-R6.45l-1 disposition inherited from owner
commit `5cac2a4117e9c478202d82aee24ad8fab8fa67de` via landed main. No
historical line was manually edited and no checker exemption was added.

This dated append supersedes only the completion status of I-3's repair;
the original report and its unapplied coordinator proposals above remain
unchanged historical evidence. The separate results-site real trial/results
negative-control deferral remains **OPEN**; this repair does not discharge
that real-data coverage obligation. CURRENT STATE, manifest and BUILD_INDEX
are unchanged. The new records commit containing this closeout has not yet
passed CI; fresh exact-head CI, completion-identity review and the normal
settle/merge gates remain required.
