# AR-24 (AMEND-C, DRAFT — unapplied) — canonical §5e.1 `stop_rule` gains `submit{marker, line, require_returncode}`

- **Status:** DRAFTED for presentation at GATE-G24 — **not applied**. This
  text does not amend `spec/`; `git diff --stat -- spec/` over R6.34 is
  empty. Application is R6.38's, verbatim, only on a GATE-G24 `approve`
  (a change to this text after presentation voids the approval).
- **Ticket:** R6.34 (E2-T0 docs bundle, manifest row 292) · presented by
  `docs/tickets/296_GATE-G24__ar24-amendment.md`
- **Authority:** `docs/experiments-spec.md` §10.5 row **AR-24** ("§5e.1
  `stop_rule.submit{marker, line, require_returncode}`", kind AMEND-C,
  "E2-T0 → T5"); §7.3 gap **G10** ("The submit marker is documented but
  unimplemented, and its literal has no declared home. That needs a
  canonical-spec amendment (AR-24)"); **REQ-XE-116**
  (`stop_rule = submit{marker, line, require_returncode}` detected by the
  dispatch gate on the settled raw output under this amendment).
- **Companion records (context only, not decided by the gate):**
  ADR-0440 (AR-21), ADR-0441 (AR-22), ADR-0442 (AR-23).
- **Protocol precedent:** `docs/tickets/00_MANIFEST.md`
  `## Spec amendments applied` (the R4.15a/b/c dated entries approved at
  GATE-G7) — the applying row appends a dated entry naming GATE-G24
  approver, plus the canonical front-matter status-line note.

## §5e.1 — before → after

**Before** (`spec/sections/05e-control-orchestration.md` l.59, and its
inline copy in `spec/CANONICAL_SPEC.md` §5e.1 "Parameters" ~l.3894 —
re-confirmed verbatim at R6.34's base):

> `stop_rule ∈ {no_action, submit, either}`, `max_continue_nudges`,
> `max_consecutive_format_errors`, `max_consecutive_errors`,
> `tool_batch_mode ∈ {sequential, parallel}`, `on_truncated_response`
> (default `fail_calls`), `plan_provenance`,
> `replan_on ∈ {never, failure, always}`, `loop_guards[]`;

and (`react/minimal` ~l.3906): `stop_rule = submit` described as
"(marker on the first line of the effect's observed output with
`returncode == 0`, detected by the **driver**, never the environment)" —
a marker with no declared literal home (G10).

**After** — the `stop_rule` value shape gains the record form
`submit{marker, line, require_returncode}`:

> `stop_rule ∈ {no_action, submit{marker, line, require_returncode},
> either}`, `max_continue_nudges`, `max_consecutive_format_errors`,
> `max_consecutive_errors`, `tool_batch_mode ∈ {sequential, parallel}`,
> `on_truncated_response` (default `fail_calls`), `plan_provenance`,
> `replan_on ∈ {never, failure, always}`, `loop_guards[]`;
>
> `submit{marker, line, require_returncode}` — the submission marker: the
> dispatch gate detects submission on the settled raw output of the
> effect — `marker` (a `Text` leaf, `authority = definition`) is the
> declared marker literal, `line ∈ {first}` (closed sum; extension by
> dialect bump) names the line the marker must occupy, and
> `require_returncode` is the integer the settled `returncode` must
> equal. The marker literal is **harness data, never a kernel
> constant**: it lives in the sealed definition's `Text` leaf the
> parameter names, so a harness's marker is data the L1 corpus and the
> representation ledger can assign (`data` locus); the kernel detects
> the declared literal, never one of its own. The bare `submit`
> spelling stays the shorthand for `react/minimal`'s declared marker
> (first line of the effect's observed output with `returncode == 0`,
> detected by the **driver**, never the environment) — the `react/minimal`
> variant text gains the parenthetical's spelled form
> `submit{marker: <the variant's declared Text leaf>, line: first,
> require_returncode: 0}` while its detection semantics are unchanged.

## Scope notes

- One canonical amendment: §5e.1's `stop_rule` value shape only. The
  dispatch-gate detection, the `react/minimal` parameter spelling's
  consumer, and REQ-XE-116's BM12/BM13 fixtures implement at R6.38
  under this text — this draft changes no implementation and no spec.
- Reading §5b.3 row 7's `mode` set as closed is expressly out of scope
  (R6.34's stop clause) — this draft does not touch it.
- sha256: recorded in `docs/build/runs/R6.34.md` (the gate readout
  presents this text exactly as committed, at that digest).
