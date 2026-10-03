# ADR-0329: CAP.3 `respond_approval` — the supply protocol's own answer verb

- **Status:** Accepted
- **Date:** 2026-10-02
- **Ticket:** CAP.3
- **Requirement ids:** R-2.11.3
  (the supply surface) · R-2.8.7
  (the §5g.7 answer path) ·
  DF-S4.11-3.
- **Spec:** §7.3 (the surface
  catalogue rule — `callable ⇔
  revealed`) · §5g.7 (pending /
  decided / granted) · ADR-0303
  (the supply rulings) · ADR-0301
  D4 (the responder declaration).
- **Rung (ADR-0024 obligation):** **C2** — the supply-surface rung.

## Context

A supply-surface Π `ask` mints the durable
`security.permission.pending{permission_id}` on the caller's *surface
run*, but no verb could answer it: the hosted caller's own catalogue is
its artifact (`respond_approval` was `unknown_tool`), and the
launched-run `respond_permission` resolves its run to a live `hnd-run-*`
session — a surface run is never minted as one, so the run-less call
refused `SchemaViolation{respond_permission/session_id}` (DF-S4.11-3,
pinned by CAP.2). The ask was durable; the reply path was a
surface-verb ruling still owed.

## Decision

D1. **`respond_approval` is a protocol builtin on every supply surface.**
It is not an artifact member — the participant does not declare the
Lab's reply path (`callable ⇔ revealed` is preserved: the artifact's
catalogue is unchanged). The dispatch short-circuits the builtin before
the artifact lookup, under the same caller-kind gate the Lab catalogue
runs (`human_principal` only → `IllegitimateEndorsement`).

D2. **The answer is run-less and surface-run-scoped.** The pending
resolves on the caller's own surface run and the surface session's own
writer lease serves the `decided` mint — the run *is* the scope, no
`hnd-run-*` handle exists to mint.
`EmbedService::surface_respond_permission` re-derives from the durable
fold everything `SessionState` hands the session op (CC1): the pending,
the already-decided check, the asked-risk row, the approval fold's
grants, the active turn.

D3. **Same invariants as the session path.** The caller's idempotency
key rides the `decided` row as `request_id` — a replayed answer returns
the recorded result, never double-mints; `decided + lease.granted` land
in one batch; the `approval`-basis handle mints only over the pending's
*recorded* material (`subject_ref`/`capability_ref`/`requested_grants`
— never fabricated); the responder declaration stamps
`responder_provenance{subject_ref, surface_session_ref}` on the row
(the P12 surface discipline).

D4. **The retry reads the durable fold.** A retried `tools/call`
consults the surface run's `security.permission.decided` rows
(`answered_ask` on `capability_ref` + `args_canonical_hash`): a serving
`allow` proceeds, a serving `deny` refuses `DeniedByPolicy`, an
unanswered ask mints a fresh pending.

## Consequences

- The deferral's done check runs green un-ignored:
  `cap2_supply_ask_respond_approval_round_trip` — ask → pending →
  answer → `decided` → retried call applies. DF-S4.11-3 closes.
- The run-less Lab-catalogue `human_principal` call still refuses
  `SchemaViolation{respond_permission/session_id}` — the reply path is
  the surface's own verb, not the launched-run op; nothing about the
  launched-run contract changed.
- A non-principal caller hits `IllegitimateEndorsement` before the
  catalogue check — the gate is the caller-kind bar verbatim.

## Alternatives considered

- **Mint `hnd-run-*` handles for surface runs:** rejected — it
  conflates the surface scope with launched runs; the surface run *is*
  the scope the pending already names.
- **Declare `respond_approval` in the participant artifact:** rejected —
  `callable ⇔ revealed`; the artifact is data the participant owns and
  cannot mint the Lab's reply path. The verb belongs to the protocol.
- **Route through the session-bound `respond_permission` with a
  synthetic `session_id`:** rejected — a fabricated session coordinate
  mints a lie; the surface session's own lease is the honest writer.

## Revisit trigger

A non-`human_principal` caller kind needs the answer path (the
caller-kind gate re-opens), or a supply-protocol version lands that
carries the verb in the artifact grammar — at which point the builtin
moves from dispatch short-circuit to declared catalogue member.
