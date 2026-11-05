# Inspect a recorded run

How to read a durable HarnessHarness run: the six annotated excerpts
below resolve to actual event classes, sequence positions and payload
members produced by a real scripted-lane run (the same drive the EX-01
and EX-03 cards execute). For the immutable useful-task and confined
operations coordinates, cite the retained executed packets —
[hh-s1-useful/1](build/reports/2026-11-04-r5.43-s1-useful.json)
(`packet_id 08817205fefe071723656e80497954b6925f68203b94996000b9fc2f7f00d26d`,
disposition `failed` with `carried_predicates` verbatim) and
[hh-ops-live/1](build/reports/2026-11-04-r5.44-ops-live-second.json)
(`packet_id 26c9d5afe97c135a36bb544301ccd1d58343db3366a62c98696ba2680e5d395b`,
disposition `executed`, `discharged_predicates` verbatim). Prerecorded
inspection is never execution — rerun the cards for fresh evidence.

**Prerequisites:** Rust 1.94, the pinned toolchain, scratch store/workspace
roots (`HH_STORE_ROOT`, `HH_WORKSPACE_ROOT` under a temp dir — never a
shared store). **Recovery:** a failed, refused or inconclusive leg is the
record, not a retry hint — controls are expected refusals; see
[troubleshooting](getting-started.md#troubleshooting).

## The six excerpts

Durable run id `run-*` vs session `sess-*`: the run owns the event
stream; the session is one attachment. `AssemblySource` is the authored
record; a bare HIR document is what `open_session{definition:document}`
seals and lowers.

1. **Run / definition / task / submission** — `lifecycle.run.created`
   (configuration_id, budget ref), `lifecycle.session.attached`,
   `context.artefact.delivered` (the staged input), and the submission
   ref inside `action.effect.observed.submission_ref`.
2. **Action ↔ model relation** — `model.call.requested` →
   `action.tool.proposed` → `model.call.completed`; the proposal's
   `tool_call_id` joins `action.effect.intended.intent`. The scripted
   lane's proposal precedes completion — no universal ordering is
   invented; the effect chain has no `decided` stage.
3. **Effect intended + permission/rule** — `action.effect.intended`
   then `security.permission.decided` (`decision:"allow"`,
   `decider:"policy"`, `decision_scope:"run"`, `effect_id`).
4. **Authorized → prepared → committed → observed** — the in-order
   sequence `action.effect.authorized`, `action.effect.prepared`,
   `action.effect.committed` (fencing token), `action.effect.observed`
   carrying the observed bytes.
5. **Lifecycle finish, separately** — `lifecycle.turn.finished` /
   `lifecycle.run.finished` (`outcome_class`, `drain_report_ref`,
   `wall_ms`) are *not* the oracle/grade/veto rows:
   `verification.claim.recorded`, `verification.claim.reconciled`,
   `verification.gate.evaluated`, `verification.completion.decided`,
   `measurement.metric.emitted` carry their own ids and stay separate.
6. **Context / replay** — `context.assembled` rows plus the replay
   copy's `lifecycle.replay.started`/`lifecycle.replay.finished` markers
   — the only permitted added suffix on the original prefix.

## Illustrations (optional, equivalent text)

Each excerpt above *is* the equivalent-text illustration of the same
checked receipt — terminal/context only, no new viewer. Raw output is
the card report's `checks[]`/`records` JSON; the useful-task and
operations coordinates come from the retained packets linked above,
never relabelled `hh.submit` fixtures.

## The bounded cap edit (adaptation, not causal)

`budget.dimensions.model_calls.hard` — the open-spec cap input — is the
one editable supported field demonstrated end to end: validate (the
boundary's typed decode; a malformed kind refuses at `open_session`),
compile (the realized `configuration_id` differs per config), rerun as
a new attempt (`hard:8` completes; `hard:0` parks on
`control.budget.exceeded{budget_id}` — the id is the cap node's own
content address — then `control.decision{stop}` and
`lifecycle.turn.finished`). Both configs are retained; this proves the
field is consumed and enforced, not any causal effect. Recipe: the
EX-03 card (`scripts/examples/ex-03.json`).
