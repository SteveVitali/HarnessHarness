# GATE-G24 — AR-24 (AMEND-C): canonical §5e.1 `stop_rule` gains `submit{marker, line, require_returncode}` · readout

Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> consent. A gate is RETURN PASS; a failed technical prerequisite is blockedOn.

## Criterion (verbatim from the spec)

§10.5 row **AR-24** — "§5e.1 `stop_rule.submit{marker, line, require_returncode}`", kind AMEND-C, "E2-T0 → T5";
§7.3 gap **G10** — "The submit marker is documented but unimplemented, and its literal has no declared home.
That needs a canonical-spec amendment (AR-24)"; **REQ-XE-116** — `stop_rule = submit{marker, line,
require_returncode}` detected by the dispatch gate on the settled raw output under this amendment. This is a
canonical-spec amendment — the frozen spec is amended through an ADR and the manifest protocol, never in place
(AGENTS.md); the amendment protocol precedent is `docs/tickets/00_MANIFEST.md` `## Spec amendments applied`
(the R4.15a/b/c dated entries approved at GATE-G7).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The drafted AMEND-C text — `docs/build/planning/2026-11-10_ar-24-amendment-draft.md` (landed by R6.34,
  sha256 `3168f50898d6d4448b5dd6f8533de3c81e256fdfe5f84aff513f31061de138bb`, re-read 2026-11-10, byte-identical
  to the recorded digest). Presented exactly as committed; a change to this text after presentation voids the
  approval.
- The canonical §5e.1 before-text — `spec/sections/05e-control-orchestration.md` ~l.59 and the inline copy
  `spec/CANONICAL_SPEC.md` ~l.3894 / `react/minimal` ~l.3906 — re-confirmed verbatim at R6.34's base; `git diff
  --stat -- spec/` over the whole row is empty (the draft is unapplied).
- Companion records (context only, not decided by the gate): ADR-0440 (AR-21), ADR-0441 (AR-22), ADR-0442
  (AR-23) — the design ADRs landed by R6.34.
- All R6.34–R6.37 implementation rows landed: the opt-in `model_port`/`transcript_render` members (R6.34),
  `hh-tmpl/1` + the single compiled-`ModelSurface` renderer (R6.35), `tools[]` verbatim + completion surface +
  the typed result/error renderers (R6.36), the durable model-view exec-observation plumbing (R6.37). The
  submitting-marker detection this amendment admits is R6.38's work under this text — REQ-XE-116's
  BM12/BM13 fixtures.

<!-- agent-drafted:begin sha256=25be9ac83a950c1d -->
## Decision content for the operator

**1. What the amendment changes (the draft's before → after, verbatim).**

*Before* — `spec/sections/05e-control-orchestration.md` l.59 / `spec/CANONICAL_SPEC.md` ~l.3894 Parameters:
`stop_rule ∈ {no_action, submit, either}`, `max_continue_nudges`, `max_consecutive_format_errors`,
`max_consecutive_errors`, `tool_batch_mode ∈ {sequential, parallel}`, `on_truncated_response` (default
`fail_calls`), `plan_provenance`, `replan_on ∈ {never, failure, always}`, `loop_guards[]`; and
(`react/minimal` ~l.3906): `stop_rule = submit` described as "(marker on the first line of the effect's
observed output with `returncode == 0`, detected by the **driver**, never the environment)" — a marker with no
declared literal home (G10).

*After* — the `stop_rule` value shape gains the record form `submit{marker, line, require_returncode}`:
`stop_rule ∈ {no_action, submit{marker, line, require_returncode}, either}` (the same parameter list
otherwise); and `submit{marker, line, require_returncode}` — the dispatch gate detects submission on the
settled raw output of the effect: `marker` (a `Text` leaf, `authority = definition`) is the declared marker
literal, `line ∈ {first}` (closed sum; extension by dialect bump) names the line the marker must occupy, and
`require_returncode` is the integer the settled `returncode` must equal. **The marker literal is harness data,
never a kernel constant** — it lives in the sealed definition's `Text` leaf the parameter names, so a
harness's marker is data the L1 corpus and the representation ledger can assign (`data` locus); the kernel
detects the declared literal, never one of its own. The bare `submit` spelling stays the shorthand for
`react/minimal`'s declared marker — the `react/minimal` variant text gains the parenthetical's spelled form
`submit{marker: <the variant's declared Text leaf>, line: first, require_returncode: 0}` while its detection
semantics are unchanged.

**2. Scope.** One canonical amendment: §5e.1's `stop_rule` value shape only. The dispatch-gate detection, the
`react/minimal` parameter spelling's consumer, and REQ-XE-116's BM12/BM13 fixtures implement at R6.38 under
this text — this gate approves the text only; it changes no implementation and no spec here. Reading §5b.3
row 7's `mode` set as closed is expressly out of scope (R6.34's stop clause).

**3. Why it's needed.** G10 — the submit marker is documented but unimplemented and its literal has no declared
home; without this amendment the marker literal has no canonical place to live (it must not be a kernel
constant). R6.38's submit-detection work is blocked on the approved value shape.

**4. What stays unchanged.** Everything outside the `stop_rule` value shape; the `react/minimal` detection
semantics (driver-side, first-line marker, `returncode == 0`) are unchanged — only the marker literal's home
moves into the declared `Text` leaf. Every other canonical parameter and the frozen contract are untouched.

**Dispositions.** `approve` (or `PASSED`) approves the AR-24 amendment text as presented — R6.38 then applies
it verbatim to `spec/sections/05e-control-orchestration.md` (appending the dated `## Spec amendments applied`
entry naming this gate and the canonical front-matter status-line note) and implements the submit detection
under it. `refuse` leaves `spec/` unchanged — the marker stays without a declared home and the submit-detection
path is deferred/reworked (a deferral row would be written). Any text change after presentation voids the
approval and re-presents.
<!-- agent-drafted:end -->

## Signature

Operator decision (verbatim, received 2026-11-10 via interactive chat): "yes, authorized, proceed"
Dispositions recorded from the verbatim: (a) AR-24 AMEND-C approved — canonical §5e.1 `stop_rule` gains `submit{marker, line, require_returncode}` exactly as drafted (`docs/build/planning/2026-11-10_ar-24-amendment-draft.md`, sha256 `3168f50898d6d4448b5dd6f8533de3c81e256fdfe5f84aff513f31061de138bb`); R6.38 applies it verbatim to `spec/sections/05e-control-orchestration.md` (dated `## Spec amendments applied` entry + canonical front-matter status-line note) and implements REQ-XE-116 submit detection under it. (b) Companion records context only — ADR-0440/41/42 not decided here.
verbatim_sha256 = sha256_text("yes, authorized, proceed") = fe8252a50e1b4632ea47c3637b33796708291b486c4921dfe5c83764480ed5a3 — stamped below; the gate does not touch a declaration (no runner.commit — this is a spec amendment, not a window authorization).
GATE DECISIONS row: 2026-11-10 | GATE-G24
Operator confirmation of agent-drafted text (verbatim, 2026-11-10): "yes, authorized, proceed" — covers sha256:25be9ac83a950c1d
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.
