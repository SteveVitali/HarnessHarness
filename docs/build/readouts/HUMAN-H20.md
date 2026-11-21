# HUMAN-H20 readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE | DEFERRED -->

> An operator-named reviewer supplies the dispositions; an agent never reviews, clears, confirms or
> infers silence as a disposition. The scan's flags held the R6.57 results freeze vetoed until this
> signature landed.

## Criterion (verbatim)

Spec §10.3 HUMAN table: "Flagged-pass review | operator-named reviewer | each results freeze (zero
flags recorded trivially) | dispositions with excerpt hashes | results packets". The R6.57 W4x fold
(`docs/build/runs/R6.57.md`) routed its 69 flagged scored trials (34 on passes) to this row via
`hh-flagged-review-input/1`.

## Evidence (per item: source, `date -u` of the read, layer reached)

- `hh-fp-answer-scan-report/1` `docs/build/reports/2026-11-20-fp-w4x-answer-scan.json` — detector
  `answer-scan/1` over the W4x executed set: **150 scored trials (pi + claude-code a4; v5 a5),
  69 flagged, 34 flagged passes**; 50 v5-a4 members typed
  `no_scored_outcome{verifier_recorded_127}` (superseded by w4xv a5) — `engineered`.
- `hh-flagged-review-input/1` `docs/build/reports/2026-11-20-fp-w4x-flagged-review-input.json`
  sha256 `e4665d8f9bbe8a47aa5d7b6f2a9a97d41d2973b0367a4df9b4d08c95b86a3ef5` — `input_to:
  HUMAN-H20`, `disposition: pending{human}`, the full flagged-pass list — `engineered`.
- `hh-fp-w4x-flag-analysis/1` `docs/build/reports/2026-11-20-fp-w4x-flag-analysis.json` sha256
  `d8d98441e0ee73b5b18bc55374ffeda9fa6746dc89341d09c2391830de1325ce` — independent analysis:
  69/69 flags resolved, **213/213 excerpt hashes** to named corpus members — `engineered`.
- Freeze veto exercised: `fp-w4x-results freeze` refused all w4x packets verbatim
  (`withheld{flagged_review}`) before this signature — `engineered`, re-runnable offline.

<!-- agent-drafted:begin -->
## Decision content — W4x flagged-pass review (operator act, recorded verbatim)

**What the scan found.** 150 scored trials scanned; **69 flagged, 34 of them passes**. Every flag
is an observed veto against the R6.57 superseding packets until dispositioned.

**Analysis finding** (all-resolved, no metadata guesses): the flags fold to four
task-design-consistent mechanisms — M1 prompt-quoted fixture/spec (50 excerpt hits), M2
provided-workspace-file echoes (1,768 — tool_result reads of task-given files incl. the dclm repo
under sanitization and the partially-public `/app/eval.py`), M3 agent-authored overlap (88), M4
benign substrings/JSON boundaries (33). No canary, no solution-fetch egress, no tamper signal in
any scored trial.

**Coordinator inspection** (requested by the operator after the mass ruling): closest calls
verified against retained artifact text — `fix-code-vulnerability`'s `from bottle import _hkey` is
inside the agent's own CRLF exploit probe; `headlessterminal`/`axes3d` hits are prompt text and the
agent's own `Write`; `largest-eigenval`'s hits are echoes of the task-provided `eval.py` (which
shares held-out fingerprints by task design — the held `test_outputs.py` is a timing harness with
monkeypatch guards, not a verbatim copy; the record's `detector_scope_note` already carries the
benchmark-design observation for operator awareness). Inspection changed no disposition.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Operator:** Steve Vitali · **UTC:** 2026-11-20T17:57Z · **Channel:** operator CLI session.
- **Verbatim disposition:** "all 69 cleared. continue" — a mass ruling covering every listed
  excerpt hash; supplemented by the operator's follow-up "actually, maybe worth inspecting if you
  think so" — the coordinator-level spot inspection above confirms each closest call's mechanism.
- **Basis:** `hh-fp-w4x-flag-analysis/1` `52567ef2…` — 69/69 analyzed, 213/213 excerpt hashes
  resolved to actual artifact text; coordinator spot-checks on the closest calls (three arms,
  incl. the provided-`eval.py` overlap task-design class).
- **Dispositions (all 69, verbatim shape):** `cleared` — reason per flag: task-design-consistent
  overlap or inert agent-authored content, never evidence of answer access. Excerpt hashes: the
  committed scan record's per-slot lists (canonical JSON is the hash source).
- **Counts:** flagged 69 · confirmed 0 · cleared 69 · flagged-passes cleared 34 · "the confirmed
  count is a minimum" · detector `answer-scan/1` · masking `harness_masked`.
- Layer: `signed` — dispositions recorded under the operator's verbatim mass ruling + analysis
  record; no excerpt content beyond hashes enters this readout.
