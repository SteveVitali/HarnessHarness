# ADR-0213 package briefs — BL-36 (R4.12 part 1 + R4.12a part 2, 2026-10-22)

<!-- Brief-file convention OWNED BY R4.8 (docs/build/planning/round4-bl33-ws-l6-briefs.md;
     ticket 157): this file reuses its table columns, adopt-all-except line and the four
     disposition tokens verbatim. This ticket never rules — `recording-act` rows only cite
     an operator ruling already in LEDGER GATE DECISIONS or a landed ADR; everything else
     is a proposal for GATE-G7. Part 2 (OQ-307 … OQ-407) is R4.12a's append below — the
     adopt-all-except line covers both parts. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it — cite
tests/ADRs) · `adopt-interim-as-final` (the interim rule in force becomes the ratified
answer — quoted) · `descope-by-ADR` (permanently out of the build — ADR text + authorising
operator words named) · `needs-code-leg(<row>)` (post-GATE-G7 conform row + the exact
change). Every interim rule below is quoted from the ratified ADR-0213 Decision table;
every landed-evidence citation was re-checked at base 819336d.

## Part 1 (R4.12) — batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-030 | adopt-interim-as-final | no ATIF binding at launch — the landed export posture is the interchange answer: `export` returns `trajectory_ref` gated on the `trajectory_export` capability, degradation to `product` granularity lists loss, the minimum compaction marker is `applied_ops[{kind}]` (ADR-0075), `require_experiment`-unbound rows render `undisclosed` never ranked | R4.15a |
| OQ-075 | adopt-interim-as-final | first-party `hot_path` variants stay in process; out-of-process `hot_path` admissibility is governed by the recorded `OQ-075.hot_path_ceiling.{class_id}` `AssumptionDebtRecord` (`probe:m19_boundary_overhead` is the re-entry trigger) with refusal on breach (`LocalityTooSlow` — the interim's named refusal); the ceiling's fraction-vs-absolute form is OQ-407's own row | R4.15a |
| OQ-113 | adopt-interim-as-final | margins are `PreRegistration.equivalence_margin` data — the only margin source; absent/empty ⇒ `NoRegisteredMargins` typed refusal, never a kernel default; pairing per the landed ADR-0199 D6 rule (R2 on counterfactual rows is a seed-set claim; same-seed only under `deterministic`+`crn`); per-suite values are suite-authoring data, not kernel constants | R4.15a |
| OQ-150 | adopt-interim-as-final | termination site = the helper/kernel-owned egress hop — `hh_tls::ClientTls` wraps the resolved-address stream (`egress.rs`); no in-environment per-env CA — `tls.terminate` stays `false` and an unhonourable TLS leg refuses typed-refusal; the non-HTTP list resolves to all non-HTTP(S) — unsupported for mode A, channels for them declare B or C | R4.15a |
| OQ-157 | adopt-interim-as-final | the Lab default is the class-implied `isolation_class` (`LocalSandboxed → process_sandbox` developer-machine floor); `environment` is a declared `FactorDeclaration` with `FactorLevel`s; attested images are supplied as the record's `image_attestation`, enforced fail-closed for the attesting classes (`AttestationMissing`) | R4.15a |
| OQ-160 | adopt-interim-as-final | the approval cache is the landed `allow_lease` → `LeaseSpec{pattern?, scope ∈ {turn, run, session}, max_uses?}` — lifetime is the declared `scope` member (no `principal`/`project`/`user` member — the ADR-0035 ceiling holds by construction); `session` requires a declared pattern (`SessionScopeRequiresPattern`); `irreversible` classes require pattern + `max_uses` + `scope ≤ run`; `approvals.requested` exhaustion ⇒ `deny` | R4.15a |
| OQ-170 | — (pointer only — no proposal here) | the custody ruling is R4.15's (single-owner rule; DF-S2.5-1 → BL-30); custody stays external per ADR-0331 D3 — the `AuditSigner`/`AuditKeyResolver` seam, `signer_key_ids` empty, `audit_view` `n/a{no signer_key_ids declared}`; "adopt all" has nothing to adopt on this row | R4.15 |
| OQ-178 | adopt-interim-as-final | `pattern = None` exact-hash leases over the whole canonical-args tuple are the final default — no per-class projection table ships at launch; `SurfaceArgMap` soundness holds by construction (a whole-args match admits nothing outside the approved domain); `session`-scoped leases refuse without a declared pattern | R4.15a |
| OQ-186 | adopt-interim-as-final | the kernel-vs-per-surface fork resolves to per-surface: `read_artifact` is bound per-bundle at offload use (`UnexpressibleSurface` when absent; `has_read_artifact` a caller-supplied binding-time fact, never guessed); Π treatment is the floor — Π-1 `read_only` ⇒ `allow` | R4.15a |
| OQ-210 | adopt-interim-as-final | the landed `deterministic_detector` table is the per-kind assignment — `citation_in_action` / `procedure_invoked` / `rule_postcondition` / `args_conform` / `validates_edge`, unassigned kinds render `n/a{no_detector}` (ADR-0014), `judged`/`human` arms landed at R2.5; `activation_observable` is a `context.artefact.delivered` member and `activated` never emits for false kinds | R4.15a |
| OQ-256 | adopt-interim-as-final | `control.reproducibility` stays a non-headline, replay-mode-labelled declaration — no estimator is ratified at launch; a future estimator lands by a `MetricDeclaration` amendment to ADR-0105 D3 (the ADR-0213 entry test preserved verbatim as the re-entry trigger) | R4.15a |
| OQ-273 | adopt-interim-as-final | the admissible autonomous set = the bound executable validators — `evaluate_gate` emits `hold{d5, require_validator(refs)}` only for them; they run as ordinary effects under Π charged to the run's ordinary budget; no `gate_runnable` member exists — the bound-executable set is the answer | R4.15a |

## Part 1 — per-OQ basis (ADR-0213 interim quoted; landed evidence re-checked)

Each proposal is `adopt-interim-as-final`: the ratified interim rule is quoted; the landed
shape is the interim already in force; the outstanding workstream legs fold into the
adoption — the WS-* ruling venues no longer run, so GATE-G7's act *is* the ratification the
entry tests awaited. The ADR-0213 spec-debt entry tests stay preserved verbatim as
re-entry triggers; R4.15a lands the adopted rulings as register/ADR amendment-log appends.

- **OQ-030 — hosted trajectory interchange.** Interim (quoted): "`interchange_trajectory`
  export degrades to `product` granularity with loss listed; the minimum compaction marker
  is `applied_ops` (ADR-0075); J5 import rows are `undisclosed` under `require_experiment`
  (ADR-0141; ADR-0162)". Landed: `export` returns `trajectory_ref` gated on
  `trajectory_export` (`crates/hh-hosting/src/service.rs:624-626`; capability table at
  `abi.rs:148`); `require_experiment` unbound rows render `undisclosed` and are never
  ranked (`crates/hh-results/src/leaderboard.rs:769-778`); the compaction marker is
  `applied_ops[{kind}]` (`crates/hh-hosting/src/compact.rs`; the hosting test at
  `crates/hh-hosting/tests/hosting.rs:1135`). The final answer: no ATIF (Harbor RFC 0001)
  binding ships — the landed degraded-export posture is the hosted interchange form; a
  later ATIF/superset binding lands by a new ADR against this answer (the compaction/
  subagent marker set is the landed `applied_ops` minimum).
- **OQ-075 — `hot_path` out-of-process ceiling.** Interim (quoted): "first-party
  `hot_path` variants stay in process; out-of-process admissibility for `hot_path`
  classes is subject to a ceiling recorded as an assumption-debt record; `LocalityTooSlow`
  proposed (ADR-0181 D8)". Landed: `boundary_overhead_ms` (M19) is emitted per op on
  `lifecycle.component.invoked` (`crates/hh-varhost/src/host.rs:281`); the measured-p95
  ceiling lands as an `AssumptionDebtRecord` keyed `OQ-075.hot_path_ceiling.{class_id}`
  with `probe:m19_boundary_overhead` as removal test (`crates/hh-varhost/tests/
  overhead.rs:301-343`; AC-R-2.12.2-9). The final answer: in-process for `hot_path`
  classes stands; out-of-process admissibility is governed by the recorded debt ceiling
  and refuses on breach under the interim's named `LocalityTooSlow`; the ceiling's
  fraction-vs-absolute *form* is OQ-407's own row (part 2 — this row does not pre-answer
  it).
- **OQ-113 — equivalence margins + pairing.** Interim (quoted): "consumers fixed
  (ADR-0140: margins from `pre_registration`; ADR-0142: parity reuses R3 margins;
  ADR-0199 D6); margin *values* are pre-registration data per suite". Landed:
  `PreRegistration.equivalence_margin` is the landed typed member and the only margin
  source — absent/empty ⇒ `NoRegisteredMargins` typed refusal, never a kernel default
  (`crates/hh-eval/src/benefits.rs:364-385`; member at
  `crates/hh-ontology/src/eval.rs:1842`); the shipped exemplars carry `None`
  (`crates/hh-lab/src/exemplars.rs:150`); the coevolution margins flow consumes the
  declared set (`crates/hh-lab/src/coevolution.rs:999-1002`); pairing landed per
  ADR-0199 D6 (R2 on counterfactual rows is a seed-set claim; same-seed only under
  `deterministic`+`crn`). The final answer: margins are pre-registration data per suite —
  the ratified mechanism; per-suite *values* are suite-authoring data each suite declares
  in its own `PreRegistration`, not kernel constants; equivalence under undeclared
  margins refuses honestly.
- **OQ-150 — `proxy_injected` TLS placement + non-HTTP list.** Interim (quoted):
  "`proxy_injected` credentials with EP3 mediation (ADR-0061); TLS termination site open
  (LT-02)". Landed: `proxy_injected` delivery + EP3 mediation
  (`crates/hh-secrets/src/channel.rs:417`); `tls.terminate` is a declared
  `policy.net.tls` member but always `false`; a TLS leg under terminate or declared
  inspect-hooks the transport cannot honour refuses (`crates/hh-env/src/egress.rs:
  1188-1195`); the landed transport terminates destination TLS at the egress hop —
  `hh_tls::ClientTls` wraps the resolved-address `TcpStream`, `terminates_tls()` answers
  `true` (`egress.rs:217-235`); LT-02's wire halves verified at S2.4. The final answer:
  the termination site is the helper/kernel-owned egress hop — no in-environment
  per-env CA interception at launch (`tls.terminate` stays `false`); the
  declared-unsupported non-HTTP list resolves to all non-HTTP(S) protocols for mode A
  (SSH, git+ssh, database drivers, gRPC with custom TLS) — channels for them must
  declare delivery B or C; an in-environment per-env CA lands only by a new ADR.
- **OQ-157 — Lab default `isolation_class` + attested images.** Interim (quoted): "the
  environment class enters `environment_ref` as a factor (ADR-0137; ADR-0142); Lab
  default `isolation_class` for experiments unset". Landed: `environment` is a declared
  `FactorDeclaration` with `FactorLevel`s in the landed exemplars
  (`env:terminal-bench-2.0` et al — `crates/hh-lab/src/exemplars.rs`); `isolation_class`
  is an environment-record member implied by class (`LocalSandboxed → process_sandbox`,
  `crates/hh-env/src/record.rs:107-114`); attestation enforced fail-closed for the
  attesting classes — `user_space_kernel`/`microvm` require a well-formed
  `image_attestation` else `AttestationMissing` (`record.rs:465-489`). The final answer:
  the default `isolation_class` per environment family is the class-implied floor — the
  developer-machine Lab default is `process_sandbox`; attested images are supplied by the
  environment record's author as `image_attestation`, enforced at resolve — who supplies
  them resolves to "the image's supplier records attestation on the record".
- **OQ-160 — egress approval cache.** Interim (quoted): "egress approval cache scope
  undecided; never persisted at `principal` scope without a principal (ADR-0061 D5;
  ADR-0071; ADR-0035 D3)". Landed: `allow_lease` mints
  `LeaseSpec{pattern?, scope ∈ {turn, run, session}, max_uses?}` — a closed scope sum
  with no `principal`/`project`/`user` member, so the ADR-0035 ceiling holds by
  construction (`crates/hh-monitor/src/approval.rs:252-265`); `session` requires a declared
  pattern (`SessionScopeRequiresPattern` — `mint.rs:236-259`); `irreversible` classes
  require pattern + `max_uses` + `scope ≤ run` (`approval.rs:1004`); `approvals.requested`
  exhaustion converts the next `ask` into `deny`, never `allow`
  (ADR-0040 — `crates/hh-budget/src/account.rs:811`). The final answer: lifetime is the declared
  `scope` member; the canonical pattern shape is the `ActionPattern` projection over
  canonical args (`None` = the whole-args exact-hash); "don't ask again" is `allow_lease`
  — never a `principal`/`project`/`user` scope; the approvals-budget interaction is the
  landed `approvals.requested` ceiling.
- **OQ-178 — `ActionPattern` projection + soundness.** Interim (quoted): "`ActionPattern`
  per capability class in the L8 form (ADR-0206 D3); leases keyed on exact hashes at
  `scope = run` until patterns are validated". Landed: `ActionPattern` is the declared
  canonical-parameter projection (`crates/hh-monitor/src/approval.rs`);
  `LeaseSpec.pattern: Option` where `None` is the exact-hash lease over the whole
  canonical-args tuple; `session`-scoped leases refuse without a declared pattern
  (`SessionScopeRequiresPattern` — `mint.rs`, the I-H6 Stage-2 admission leg); the
  ADR-0206 D3 source-write `(native_id, target_state)` pattern is landed for lifted
  work sources. The final answer: the exact-hash whole-args lease is the default and no
  per-class projection table ships at launch — `SurfaceArgMap` soundness holds by
  construction (the whole-args match admits nothing outside the approved domain); a
  per-class projection (exec prefix / path-prefix set / host:port:protocol /
  server+tool+shape / payee+ceiling) lands later only by a new ADR with its soundness
  check.
- **OQ-186 — offload `read_artifact` capability.** Interim (quoted): "one kernel
  `read_artifact` capability (`fs_read`-domain, closed world) present in every sealed
  definition is assumed (ADR-0078; ADR-0073)". Landed: `offload` requires a bound
  `read_artifact` surface in the bundle else `UnexpressibleSurface`; `has_read_artifact`
  is a caller-supplied binding-time fact, never guessed
  (`crates/hh-compiler/src/surface.rs:510-545`) — the landed shape binds the capability
  per-bundle rather than auto-installing it in every sealed definition; Π treats
  `fs_read` under the floor (Π-1 `read_only` ⇒ `allow`;
  `crates/hh-monitor/src/policy.rs:439`). The final answer: the fork resolves to
  per-surface — `read_artifact` is a bundle-declared capability bound at offload use;
  its Π treatment is the floor's `read_only ∧ closed` ⇒ `allow` (never a privileged
  row); the E5/D3 executor-ownership rider (OQ-230) stays Stage-5's.
- **OQ-210 — per-kind `activation_observable` + `followed` detectors.** Interim (quoted):
  "deterministic `followed` detectors for `recovery` and `procedure_pointer`; judged for
  the other `MemoryKind`s per the OQ-049 table (ADR-0111; ADR-0083)". Landed:
  `deterministic_detector` assigns `citation_in_action` (closed-schema/recovery
  canonical-args match), `procedure_invoked`, `rule_postcondition`, `args_conform` and
  `validates_edge`; kinds with no deterministic detector render `n/a{no_detector}`
  (ADR-0014 — never a proxy) (`crates/hh-verification/src/followed.rs:52-75`); the
  `judged`/`human` arms landed at R2.5 (`followed_payload_det`); `activation_observable`
  is a `context.artefact.delivered` member and `activated` never emits for
  `activation_observable = false` kinds (`crates/hh-ledger/src/classes.rs:1957-1989`);
  the compliance chain folded at S3.8. The final answer: the landed table is the
  ratified per-kind assignment — the WS-A2/G1/B1/D4 confirmation act is GATE-G7's
  adoption itself; the canonical-args detector's ownership resolves to the landed
  `hh-verification` home.
- **OQ-256 — `control.reproducibility` estimator.** Interim (quoted):
  "`control.reproducibility` (ADR-0105 D3 declaration) is labelled by replay mode
  (`deterministic_replay` | `re-executed`) and is **not headline-eligible** until its
  estimator is ratified; the exemplar experiment runs with the metric as a non-headline,
  labelled declaration". Landed: `lab/control-strategy-family-v1` declares
  `control.reproducibility` in `PreRegistration.primary_metrics`
  (`crates/hh-lab/src/exemplars.rs:869-885`); the metric is absent from `hh-eval`'s
  headline catalogue — the non-headline, labelled posture is in force. The final answer:
  the labelled, non-headline declaration *is* the ratified launch posture — no estimator
  is ratified at launch; the estimator ruling (decision-sequence agreement across seeds /
  end-state hash agreement / a Determinism-Index form) and headline eligibility land only
  by a `MetricDeclaration` amendment to ADR-0105 D3 — the ADR-0213 entry test preserved
  verbatim as the re-entry trigger.
- **OQ-273 — autonomous `require_validator` set.** Interim (quoted): "the gate runs
  autonomously only `read_only ∧ closed-world` validators the definition binds, charged
  to the run's budget under Π (ADR-0113; ADR-0111)". Landed: `evaluate_gate` emits
  `hold{d5, require_validator(refs)}` only for bound executable validators
  (`crates/hh-verification/src/gate.rs:552-615`); `hh-control` feeds them to the strategy
  as next-step inputs that run as ordinary effects under Π — never auto-resolved, never
  privileged (`crates/hh-control/src/plan_exec.rs:459-581`); no `gate_runnable` member
  exists on `Validator`. The final answer: the admissible autonomous set is exactly the
  bound executable validators — the landed bound-executable shape is the answer to "who
  declares `gate_runnable`" (nobody: the member does not exist; binding *is* the
  declaration); the budget dimension is the run's ordinary effect budget; the Π decision
  is the ordinary-effects path.

## OQ-170 — pointer only (proposal owned by R4.15)

OQ-170's proposed custody ruling is **R4.15's** (manifest row 164 — the BL-30 adjudication
brief; DF-S2.5-1 → BL-30; the implement skeleton is R4.15e, manifest row 164m). This
package writes only the package-side engineering cell + this pointer — the single-owner
rule: a second proposed disposition here would pre-answer the ruling R4.15 must draft.
What this row records, per ADR-0331 D3 and the R2.23 cell: custody stays external and
workstream-reserved — `hh-ledger` consumes the `AuditSigner`/`AuditKeyResolver` traits so
custody lives outside the store (`crates/hh-ledger/src/audit.rs`); manifests declare
`signer_key_ids` (empty until custody lands); `audit_view` reports `checkpoints:
n/a{no signer_key_ids declared}` rather than a fabricated row; `FixedSigner`/`KeyTable`
are fixture constructions (DF-S2.5-1 stays OPEN). The four sub-questions — which process
holds the key, per-installation provisioning, the co-signed `rotation` checkpoint, the
`idp`-rotation interaction (ADR-0036) — are R4.15's to propose and GATE-G7's to rule.
In the batch table the row carries no token: "adopt all except" has nothing to adopt on
it; GATE-G7 rules OQ-170 through R4.15's brief.

## Conflicts

None registered. The one landed-vs-interim-text divergence in scope — OQ-186's
per-bundle binding vs the interim's "kernel capability in every sealed definition" — is
the R2.23 cell's already-recorded residual ("consistent with, but not a ruling on, the
kernel-vs-per-surface fork"), resolved here by proposal, not silently absorbed. No new CF
row.

<!-- R4.12a appends Part 2 (OQ-307 OQ-336 OQ-338 OQ-342 OQ-360 OQ-363 OQ-366 OQ-367
     OQ-375 OQ-387 OQ-407) below this line — the adopt-all-except line covers both parts. -->
