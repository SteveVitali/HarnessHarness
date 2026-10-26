# ADR-0212 + ADR-0216 package briefs — BL-35 + BL-39 (R4.11, 2026-10-22)

<!-- Brief-file convention OWNED BY R4.8 (docs/build/planning/round4-bl33-ws-l6-briefs.md;
     ticket 157): this file reuses its table columns, adopt-all-except line and the four
     disposition tokens verbatim. This ticket never rules — `recording-act` rows only cite
     an operator ruling already in LEDGER GATE DECISIONS or a landed ADR; everything else
     is a proposal for GATE-G7. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it — cite
tests/ADRs) · `adopt-interim-as-final` (the interim rule in force becomes the ratified
answer — quoted) · `descope-by-ADR` (permanently out of the build — ADR text + authorising
operator words named) · `needs-code-leg(<row>)` (post-GATE-G7 conform row + the exact
change). Every interim rule below is quoted from the ratified ADR-0212/ADR-0216 Decision
tables; every landed-evidence citation was re-checked at base ce1a201.

## Batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-131 | resolve-with-evidence | machine legs discharged (S0.3/S0.3b sheets appended to ADR-0050's log; trigger 5 evaluated ADR-0224/0226 + GATE-G1 readout; S2 bounds became the §2.4 constants at S1.25); the one human residual is discharged by the operator's H4 decline — GATE DECISIONS 2026-10-20 "Record a decline instead" (determining ruling; R4.15 records the waiver verbatim + closes DF-S0.3-3/BL-02) | R4.15a |
| OQ-132 | adopt-interim-as-final | the landed scope matcher `{literal, prefix-*, *}` over canonicalized paths — fail-closed, `*` covers `ScopeBindings::Unknown` — is the ratified answer; the closed `ResourcePattern` grammar + `_meta` lowering legs fold into the adoption (sound-but-incomplete is the design, ADR-0242 §4) | R4.15a |
| OQ-133 | adopt-interim-as-final | partial parse ⇒ `unknown` ⇒ the most dangerous class is landed (`classify_command` minimal grammar; `ParseOutcome` → `RiskClass::UNKNOWN`) — the conservative rule is the answer; no command-class list = every unclassifiable stays `unknown` | R4.15a |
| OQ-142 | adopt-interim-as-final | unattended `ask → deny{UnattendedAsk}` + `approval_denied_unattended` ExitClass + the closed `Remedy` sum with `remedy_taken` stamping are landed; the unanswered half resolves to: no Π row admits `sanitize`/`branch`/`substitute` under `UnattendedPolicy` | R4.15a |
| OQ-149 | adopt-interim-as-final | the constrained `SecretRef{channel_id, description}` reference + verifier-side `leak_scan`/`refuse_secrets` over canonical bytes are landed (ADR-0244 §5); leaf-vs-`Ref<SecretChannel>` resolves to the landed constrained reference | R4.15a |
| OQ-165 | adopt-interim-as-final | the host form `HookResult{raise, deny, annotate, none}` bound as the deterministic `hook` stage of the approval chain — never `allow`, never resolves — is landed; HIR placement resolves to the landed host form | R4.15a |
| OQ-201 | adopt-interim-as-final | the `dom_mem_*` domain rows (allow ≤ `run`, `ask` at `session`, deny above) + Π-7 raise + the persistence ceiling + unattended `ask → deny` are landed; exemption-vs-`ask` resolves to the landed rows — no exemption | R4.15a |
| OQ-219 | adopt-interim-as-final | JSON-Schema 2020-12 as the declared `SchemaDialect` + the landed permissive keyword subset (`param_path_exists`: `properties` walked; `anyOf`/`oneOf`/`allOf`/`$ref` permissive) is the ratified answer; the E3 fixture re-key stays the re-entry check | R4.15a |
| OQ-223 | adopt-interim-as-final | the "no domain declared" arm is what landed (`ScopeBindings::Unknown` + `Resources::Unknown`; `unknown_domain_effects()` never mints `pure`; `*`-only grants) — ratifying the interim picks the landed arm, no `unknown_domain` dialect bump | R4.15a |
| OQ-235 | adopt-interim-as-final | all eight `action.tool.*` classes are registered with durability/observability rows and emitted live; the drafted amendment-log ratification text below discharges the owed WS-B1/E3/E1 formal act | R4.15a |
| OQ-240 | adopt-interim-as-final | E3's assumed members are confirmed landed (`exposure_mode{mode, pinned?, admitted_modes?}`, `search_text_fields` on `IndexForm`, `ResultRenderSpec`) — no relocation needed; the drafted ratification text below discharges the record-home act | R4.15a |
| OQ-402 | resolve-with-evidence | answered — the S2 values are the §2.4 constants (durable-backlog 1024, ephemeral 256 eldest-dropped, in-flight 64, `cancel_deadline_ms` echo; `closed{slow_consumer}` past the bound) — re-confirmed landed | R4.15a |
| OQ-465 | adopt-interim-as-final | the Rule-C type-criterion partition is enforced structurally (audit-grade `OPEN_AUDIT` rows; `Text` leaf ⇒ `SchemaViolation` at append; `check_audit_partition` rejects undeclared members) — the interim partition applied to ADR-0205 D1's record is the named table | R4.15a |
| OQ-467 | adopt-interim-as-final | the published `IR_ENTITY_KINDS` fifteen rows (13 §3.1.3 + ADR-0231's interim leaf homes `Text → P1`, `CompiledPayload → P2`) are total and machine-checked — ratify the table as-is; a later changed row stays a dialect bump | R4.15a |
| OQ-468 | needs-code-leg(R4.15c) | promote the five `experimental` ops to `stable` — published Group signatures, error sums and conformance-suite rows (ADR-0176/0177 amendment-log act); the group-membership input is CF-488's adjudication (R4.15 owns that brief — this row does not pre-answer it) | R4.15c |

## OQ-131 — recording act (the H4-decline discharge)

OQ-131's machine legs were discharged before this round (register row, verbatim): S1/S2
ran at S0.3/S0.3b, the measurement sheets are appended to ADR-0050's amendment log,
trigger 5 was evaluated and armed (E2 C5 4→1 / C7 2→5 outside ±1) with ADR-0009 steps 5–7
satisfied-by-analysis per ADR-0226 + the GATE-G1 readout, and the S2 bounds became the
§2.4 constants at S1.25 (OQ-402). The one residual — the R2 cross-camp reviewer signature
(DF-S0.3-3 → HUMAN-H4) — is a human deliverable the operator ruled on:

- **Determining ruling (LEDGER GATE DECISIONS, 2026-10-20, HUMAN-H4 → decline):**
  answer (verbatim) `"Record a decline instead"`; consequence (verbatim): "supersedes
  the 14:21Z 'I'll name a reviewer' direction: no cross-camp signature will be sought;
  a round-4 ticket records the decline verbatim on the measurement-sheet readout and
  closes DF-S0.3-3 / BL-02 waived-by-ADR (GATE-ACCEPT item 7 discharged by the
  operator's decline, not re-litigated)".

This ticket records the discharge on OQ-131's register cell citing that ruling — a
recording act, not a proposal: the ruling fully determines the residual. The waiver's
verbatim record on `docs/build/reports/S0.3-measurement-sheet.md` (or its readout) and
the DF-S0.3-3 / BL-02 `waived-by-ADR` closure are **R4.15's** (manifest row 164; this
ticket's out-of-scope). Proposed disposition `resolve-with-evidence`: the landed
measurement evidence plus the recorded ruling answer the question; the conform at
R4.15a flips the register.

## OQ-402 — answered (re-confirmed)

The S2 measurement values are the landed §2.4 constants: durable-backlog bound 1024
(`SUB_BUFFER`), ephemeral per-connection buffer 256 eldest-dropped counted into
`lagged{dropped:{ephemeral:n}}`, in-flight session cap 64, `cancel_deadline_ms` echoed
from the armed envelope policy; `closed{slow_consumer}` past the bound and the writer
never stalls (`docs/build/runs/S1.25.md`; `crates/hh-embed/src/frames.rs`;
`crates/hh-embed-schema/src/frames.rs`). Entry test discharged: sheets appended
(S0.3) + trigger 5 evaluated (ADR-0224/0226, GATE-G1). Proposed disposition
`resolve-with-evidence` — re-confirmed, nothing outstanding.

## Interim-rule OQs — `adopt-interim-as-final` proposals

Each row quotes the ratified interim rule (ADR-0212 Decision table) as the proposed
final answer; the outstanding workstream legs fold into the adoption — the WS-* ruling
venues no longer run, so GATE-G7's act *is* the ratification the entry tests awaited.
The ADR-0212 spec-debt entry tests stay preserved verbatim as re-entry triggers.

- **OQ-132 — closed `ResourcePattern` grammar.** Interim (quoted): "the handle table and
  `ResourcePattern` grammar shape (path roots after canonicalisation, command classes,
  host sets, secret names) per ADR-0051 H-5; grants match canonical parameters; AT-H1-04
  in its Stage-1 static form". Landed: `scope_covers` `{literal, prefix-*, *}` over
  canonicalized paths, fail-closed, `ScopeBindings::Unknown` covered by `*` only
  (`crates/hh-monitor/src/args.rs`); sound-but-incomplete by design (ADR-0242 §4 — a
  scope the grammar cannot express reads uncovered, never over-covered). The final
  answer: the landed matcher *is* the grammar; the closed-grammar and `_meta` lowering
  legs are descoped into the adoption (a future grammar lands by a new ADR under the
  fail-closed rule).
- **OQ-133 — command grammar inside the TCB.** Interim (quoted): "partial parse ⇒
  `unknown` ⇒ the most dangerous class (ADR-0100; ADR-0052); no command-class list
  beyond the Stage-1 fixtures". Landed: `classify_command` minimal grammar
  (`crates/hh-env/src/dispatch.rs`), `ParseOutcome` → `RiskClass::UNKNOWN`
  (`crates/hh-monitor/src/assess.rs`), assessors executor-registered with `authorize`
  consuming only the recorded verdict (ADR-0242 §3). The final answer: the conservative
  rule stands — the declared class list stays the landed minimal grammar, and the
  Stage-3 approval-volume cost leg is descoped into the adoption (the rule's cost is
  bounded by construction: unclassified ⇒ `unknown` ⇒ most dangerous class).
- **OQ-142 — unattended runs.** Interim (quoted): "unattended `ask → deny`, remedies
  untaken reported as `approval_denied_unattended` (ADR-0052 check 7; ADR-0208 §D.5;
  ADR-0168 D2)". Landed: `UnattendedPolicy::Deny` (Π-12,
  `crates/hh-monitor/src/policy.rs`), `approval_denied_unattended` `ExitClass` off the
  durable `decided{reason: unattended}` count (`crates/hh-cli/src/exit_class.rs`), the
  closed `Remedy` sum with `remedy_taken` (`crates/hh-provenance/src/flow.rs`; ingress
  consume at R2.12, ADR-0344). The final answer: under `UnattendedPolicy` no Π row
  admits `sanitize`/`branch`/`substitute` — the deny default is the answer to "which
  rows admit remedies"; H7's report is the landed `ExitClass`.
- **OQ-149 — `SecretRef` in HIR/1.** Interim (quoted): "`SecretRef` per ADR-0057 as a
  constrained reference to a `SecretChannel`; `leak_scan` runs over canonical bytes at
  `seal` with the ADR-0057 detector set; leaf-vs-`Ref` open (LT-08)". Landed:
  `SecretRef{channel_id, description}` (`crates/hh-secrets/src/types.rs`;
  `ScopeKind::SecretRef` in `crates/hh-hir/src/tools.rs`); `leak_scan` a verifier
  battery + request-boundary scan (`SecretInPayload`,
  `crates/hh-embed/src/inject.rs::refuse_secrets`; ADR-0244 §5 — verifier, never a live
  interceptor). The final answer: the constrained `Ref<SecretChannel>`-shaped
  `SecretRef` is the HIR home (no new leaf kind, no dialect bump); the `validate`
  false-positive policy is the landed boundary-scan form.
- **OQ-165 — hooks as HIR.** Interim (quoted): "the output sum is `GuardVerdict`
  (ADR-0181 D6) and the host form `HookResult{raise, deny, annotate, none}` (ADR-0177
  D4); a hook never `allow`s; placement — guard on `ControlBoundary` per
  `DecisionPoint` vs `HarnessRule` with a `CompiledPayload` action — is open, as is the
  hook-event → `DecisionPoint` map". Landed: `HookResult` on `upcall.notify_hook`
  (`crates/hh-embed-schema/src/types.rs`) bound as the attested deterministic `hook`
  stage of `policy_rule → lease → hook → auto_reviewer → human`
  (`crates/hh-monitor/src/approval.rs::run_chain`, `HookVerdict`) — a hook `allow` only
  feeds `auto_review`, never resolves. The final answer: the landed host form is the
  binding; the HIR-placement half resolves to "no `HarnessRule`/`CompiledPayload` hook
  action at launch" (the `one_shot` placement join with OQ-408 is preserved as the
  re-entry question).
- **OQ-201 — Π taint gate vs `memory_write`.** Interim (quoted): "every
  `memory_write{store: memory}` after an open-world read is an `ask` under Π's taint
  gate; the persisted item is capped at `external` (ADR-0035 D3 as amended; ADR-0080);
  a background consolidation run without a reachable principal is `unattended`
  (`ask → deny`)". Landed: `dom_mem_*` domain rows allow at scope ≤ `run`, `ask` at
  `session`, deny above (`crates/hh-monitor/src/policy.rs`); the Π-7 raise and step-5
  persistence ceiling apply after an open-world read (`crates/hh-monitor/src/monitor.rs`);
  unreachable-principal consolidation is `unattended ⇒ ask → deny`. The final answer:
  no exemption — the landed domain rows are the Π ruling; the consolidation-run policy
  is the landed unattended transform.
- **OQ-219 — `SchemaDialect` set + keyword subset.** Interim (quoted):
  "`input_schema`/`output_schema` declare JSON-Schema 2020-12 as the default
  `SchemaDialect`; the C0 keyword subset for E3 inclusion checks is the subset the
  Stage-1 fixtures exercise (ADR-0087; ADR-0022)". Landed: JSON-Schema bodies under the
  2020-12 default; `param_path_exists` permissive resolver (`properties` walked;
  `anyOf`/`oneOf`/`allOf`/`$ref` permissive — `crates/hh-hir/src/tools.rs`), shared with
  `conforms` and leaf admission (CC1). The final answer: the admitted set is the landed
  2020-12 default; the closed keyword subset is the landed permissive resolver; the E3
  fixture-suite re-run stays the re-entry check.
- **OQ-223 — effects lifted from hints.** Interim (quoted): "a hint-lifted effect takes
  `world` from `openWorldHint`, carries `scope_bindings_unknown` and projects to the
  most dangerous risk class (ADR-0087; ADR-0088 D2; ADR-0031)". Landed: the "no domain
  declared" arm — hint-lifted imports carry `ScopeBindings::Unknown` +
  `Resources::Unknown`; `unknown_domain_effects()` never mints `pure`
  (`crates/hh-registry/src/import.rs::lift_effects`); a pattern grant can never cover an
  unknown binding — `*`-only (ADR-0242 §4); no `unknown_domain` member exists in
  `EffectDomain`. The final answer: ratify the landed arm — "no domain declared" +
  `scope_bindings_unknown`, no `unknown_domain` dialect bump; fixture re-key is the
  re-entry trigger.

## OQ-235 / OQ-240 — narrowed answers + drafted ratification text (deliverable 1)

Both questions were narrowed at R2.22: the provisional spellings/fields are exactly what
landed. What stayed owed is the *formal* amendment-log ratification (WS-B1/E3/E1 venue —
it no longer runs) plus, for OQ-235, the hosted lifting-table re-key. The ratification
text is drafted below for GATE-G7's adoption; R4.15a lands it as amendment-log appends.

- **OQ-235 — draft ratification text (ADR-0026 amendment log; the ADR-0092/0093 homes
  ride it per ADR-0252 §7):** *"The tool-plane event-class names, scopes and durability
  rows are ratified as landed: `action.tool.catalog.{built, delta, epoch}`,
  `action.tool.exposure.planned` (scope `context_assembly`),
  `action.tool.discovery.searched` (scope `tool_call`),
  `action.tool.surface.{revealed, evicted}`, `action.tool.call.refused` — the
  registered rows of `crates/hh-ledger/src/classes.rs` with their declared
  durability/observability levels, emitted by the armed lane (`catalog.built` →
  `exposure.planned` → `delivered`; `discovery.searched`;
  `surface.revealed`/`evicted`; `call.refused`; `catalog.{delta,epoch}`). The hosted
  lifting table is re-keyed to these spellings."*
- **OQ-240 — draft ratification text (record-home amendment logs — ADR-0087/0093 via
  the ADR-0026 log; ADR-0252 §6/§8 already adopt the interim verbatim):** *"The
  tool-surface record carries, as ratified members: `exposure_mode{mode, pinned?,
  admitted_modes?}` (`crates/hh-hir/src/tools.rs`), `search_text_fields` on `IndexForm`
  (`crates/hh-compiler/src/exposure.rs`), and `result_render` as
  `ProfileRuleKind::ResultRender` + schema-first `ResultRenderSpec`. E3's assumed field
  set is confirmed — no relocation."*
- Proposed disposition for both: `adopt-interim-as-final` — the interim text is the
  landed contract; GATE-G7's adoption performs the ratification the entry test named;
  R4.15a appends the drafted text to the amendment logs and flips the registers.

## OQ-465 / OQ-467 — `adopt-interim-as-final` proposals (BL-39)

- **OQ-465 — `control.work_item.*` audit_fields partition.** Interim (ADR-0216 Decision,
  quoted): "for every audit-grade `control.work_item.*` class (ADR-0066 Phase 4 log),
  `audit_fields` = the id, hash, closed-enum, integer and `EventRef` fields of
  ADR-0205 D1's record (`work_item_id`, `activation_no`, `owner_ref`, `state`,
  `from`/`to`, `cause: EscalationCause`, `basis`, `run_id`, `causes[]`, `source_ref`
  hash, `spec_hash`) and `content_refs` = every `Text` field — Rule C's type criterion
  applied to the ratified record; a `Text` leaf in `audit_fields` is refused at
  `append` (`AuditFieldsTooLarge`/schema error)". Landed: the seven classes are audit-grade `OPEN_AUDIT` rows; a `Text` leaf
  refuses `SchemaViolation` at append; `check_audit_partition` rejects undeclared
  members (`crates/hh-ledger/src/store.rs:1572,4130`, `classes.rs:162`). The final
  answer: the type-criterion partition *is* the named table — the per-class field lists
  derive mechanically from ADR-0205 D1's record under the criterion already enforced;
  the ADR-0026/0066 amendment-log append (R4.15a) spells them out verbatim.
- **OQ-467 — per-kind `classify_home` table.** Interim (ADR-0216 Decision, quoted): the
  thirteen-row §3.1.3 table (`Goal` P3, `Observation` P1, `ContextItem` P1, `Memory`
  P5, `Procedure` P3, `ToolCapability` P2, `Permission` P6, `Effect` P2, `Artifact` P5,
  `Validator` P4, `AgentProcess` P3, `Budget` P7, `HarnessRule` P7) — total under
  ADR-0012 D2, `home_plane` derived on every node (ADR-0016). Landed:
  `IR_ENTITY_KINDS` publishes all fifteen rows — the thirteen + ADR-0231's interim leaf
  homes `Text → P1` (opaque observed content) and `CompiledPayload → P2` (executable
  artifact with a declared interface) — `classify_home` total and machine-checked
  (`UnclassifiedKind`; `crates/hh-ontology/src/planes.rs:371`). The final answer: the
  published fifteen-row table is ratified as-is (the ADR-0048-class ruling the entry
  test named is GATE-G7's adoption + the ADR-0012/0016 amendment-log act at R4.15a); a
  later changed row remains a dialect bump per ADR-0015.

## OQ-468 — `needs-code-leg(R4.15c)` proposal

- **The question:** stable `hh-embed/1` operations for the five ratified CLI verbs —
  `run amend`, `run archive`, `approval leases`, `approval revoke-lease`,
  `approval grant` — Group W/S signatures, error sums and conformance rows (ADR-0167
  N-4; ADR-0168 D1/D6; ADR-0070 D6).
- **Landed:** all five declared at `experimental` tier, gated `ExperimentalRequired`:
  `amend` Group W implemented (`ops.rs:485-493`), `list_leases` Group R implemented
  (`ops.rs:866-880`), `archive`/`grant`/`revoke_lease` Group S staged
  `Refused{stage_pending}` (`ops.rs:825-827`). The landed grouping diverges from the
  interim text's W/W/W/W+S placement — CF-488.
- **Proposed code leg (R4.15c):** promote the five ops from `experimental` to `stable` —
  published signatures in `crates/hh-embed-schema/src/ops.rs` (tier flip + the
  `ExperimentalRequired` gate dropped), the staged ops implemented or explicitly
  kept-staged by the adopted ruling, error sums frozen, `schema/` artifacts
  regenerated (CC7), conformance-suite rows added, the five `AssumptionDebtRecord`s
  retired (`promoted_to_stable`), the ADR-0176/0177 amendment log appended, and
  AC-R-2.11.1-2 / AC-R-2.11.4-8 re-run.
- **Dependency:** the group-membership input to the signatures is CF-488's
  adjudication — owned by R4.15's brief (manifest row 164). This proposal does not
  pre-answer the grouping; the code leg lands whichever grouping GATE-G7 adopts.

## CF-488 — register-side note (deliverable 2; adjudication is R4.15's)

The register row (`research/registers/conflicts.md`) gains a dated note pointing at
R4.15's adjudication brief — manifest row 164 owns the finding's adjudication brief
(the WS-K4/K1/H7 stable-signature ruling: conform the spec/ADR text or the schema by
amendment). This ticket records the pointer and the OQ-468 code-leg dependency above;
it proposes no group-membership answer. CF-488 is therefore deliberately absent from
the batch-adoptable table (single-owner rule — a second proposed disposition from this
package would pre-empt R4.15's); its FINAL proposed disposition lands in R4.15's brief.

## ADR-0242 — DONE legs re-confirmed (deliverable 3)

ADR-0242's Consequences opened DF-S1.11-1 (Stage-2 halves), DF-S1.11-2 (Stage-3/4
executable acceptance), DF-S1.11-3 (OOP `authorize` byte-equality). All three rows read
**DONE** at this date — DF-S1.11-1 2026-09-19 (S2.6), DF-S1.11-2 2026-10-07 (R2.11,
after S3.12b's benchmarkSet + S4.14b's hosted-boundary cells), DF-S1.11-3 2026-09-19
(S2.6 `hh-authorize` byte-equality). A dated re-confirmation note is appended to the
ADR; no obligation outstanding.

## What this brief does not decide

No register status changes, no amendment-log appends, no schema or spec conform land
here — every cell above is a proposal the operator adopts, amends or declines at
GATE-G7 (the records conform is R4.15a's; OQ-468's code leg is R4.15c's; CF-488's
adjudication is R4.15's; the DF-S0.3-3 waiver record is R4.15's). The interim rules
stay in force unchanged until then; every ADR-0212/0216 entry test is preserved
verbatim as a re-entry trigger. BL-35's and BL-39's flips are R4.16's accounting after
GATE-G7 adoption and the conforms land.

> *Reconciled 2026-10-22 (GATE-G7 fleshing):* this package's records conform lands at **R4.15a2** (164c — the GATE-G7 insert); the OQ-468 code leg at **R4.15c** (164h) as tabled.
