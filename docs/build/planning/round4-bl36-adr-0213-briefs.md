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

## Part 2 (R4.12a) — batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-307 | adopt-interim-as-final | `static_hash` = `idp` over the static-tier `context_item_id`s — membership resolves to item *identities* (`version_id`), never rendered bytes; the `role_map` collapse and demotion wrappers are render-time transforms declared out of hash scope; `prefix_affecting_params` enter per LC-5 | R4.15a |
| OQ-336 | adopt-interim-as-final | a benchmark-supplied verifier rides as a `foreign.digest_claim` — a claim, never a `ContentAddress` or vendored executable tree member; unpinned ⇒ `unpinned[]` caps `claimed_level` at {R0, R1, R3} — R2 requires the pinned `image_digest`; the N13 "integrate, never author" answer is integrate-by-claim | R4.15a |
| OQ-338 | adopt-interim-as-final | stratum B's launch anchor is the committed `adapter_b` FreshTemporal corpus whose `SuiteValidityRecord` (the manifest's committed `audit` block) is supplied by the suite author — N13 holds by construction (the instrument validates, never produces it); `headline_admissible` gates on `audited_at`; a named third-party pool enters only as a new suite manifest through the same gate — pool identity is suite-authoring data | R4.15a |
| OQ-342 | adopt-interim-as-final | the parity margin is the R3-style pre-registered margin per OQ-113's part-1 adoption (`PreRegistration` data, never kernel constants); subset design is the `ParityReport`'s declared `original_runs`/`replay_verdicts`; `estimated{…}` is adaptive-strategy-only and never a headline discharge; `parity = none` keeps the I-5 product-granularity restriction | R4.15a |
| OQ-360 | adopt-interim-as-final | `ReattemptPolicy{max_per_plan = 2, max_fraction_of_plans_ppm = 100_000}` + the declared `backoff` member are the packaged defaults, engine-enforced at both bounds; the Stage-3 infrastructure-failure-rate measurement stays the verbatim re-entry trigger | R4.15a |
| OQ-363 | adopt-interim-as-final | the duplicate-run control is final — `iso_cost`/`matched_cap` arms never share subject runs (ADR-0278's landed ruling shape; GATE-G2's recorded "OQ-363 interim rule stands"); cross-mode `compare` stays `IncommensurableMatch`; ADR-0278's revisit trigger preserves "one run, two views" for a later S-2/E-2-preserving ADR | R4.15a |
| OQ-366 | adopt-interim-as-final | `confirmatory` requires ≥ 2000 `resampling_draws` + pre-registration coverage; `preview` never persists as confirmatory; draws are caller-declared `AnalysisInput` data (no per-report-class default table ships); the >10⁴-row resample-index caching rule resolves to no cache — a report resamples | R4.15a |
| OQ-367 | adopt-interim-as-final | the global floors `T_CLT = 100`, `T_BCA = 30` (ADR-0158's values via the S3.4c ADR-0280 correction; KA-1/KA-2 coverage ≥93% at nominal) are the packaged defaults — floors are not per metric family; a per-family set lands only by amendment (`EstimatorSelection.floors`/`substituted` already record the applied set) | R4.15a |
| OQ-375 | adopt-interim-as-final | the per-stage defaults are the landed pair: `new` = `Validated` admission + no claimed-level gate (Stage-3 lab-internal), `public` = `Reproduced` (Stage-4 published); `min_replicates` stays suite-declared disclosure data (OQ-121's replicate interaction); `unadmitted[]{reason}` canonical | R4.15a |
| OQ-387 | adopt-interim-as-final | the claim is recorded verbatim — absent ⇒ `unknown`, never inferred, inherited at subagent spawn; `unknown` narrows as `untrusted` (interim verbatim); the narrowed Π set is empty at launch — the interim names no rows and no `hh-monitor` row consumes it; a leaf naming rows lands only by ADR amendment | R4.15a |
| OQ-407 | adopt-interim-as-final | the landed measurement form is final: per-invocation `boundary_overhead_ms` (M19) with the ceiling an absolute-p95 `AssumptionDebtRecord` per class (`OQ-075.hot_path_ceiling.{class_id}`, removal `probe:m19_boundary_overhead`) — the fraction-of-`model_latency_ms` form rejected for the landed absolute bound; the per-step op list is each class's declared `hot_path_operations[]`; breach refuses `LocalityTooSlow` (the interim's named refusal adopted) | R4.15a |

## Part 2 — per-OQ basis (ADR-0213 interim quoted; landed evidence re-checked)

Each proposal is `adopt-interim-as-final`: the ratified interim rule is quoted; the landed
shape is the interim already in force; the outstanding workstream legs fold into the
adoption — the WS-* ruling venues no longer run, so GATE-G7's act *is* the ratification the
entry tests awaited. The ADR-0213 spec-debt entry tests stay preserved verbatim as
re-entry triggers; R4.15a lands the adopted rulings as register/ADR amendment-log appends.

- **OQ-307 — `static_hash` membership.** Interim (quoted): "`static_hash` per ADR-0127
  LC-2 over static-tier bytes, tool surfaces and dialect-listed parameters; `role_map`
  collapse and demotion wrappers not yet in scope". Landed: `static_hash` is `idp` over
  the static-tier slots' `context_item_id`s (`crates/hh-context/src/assemble.rs:734-735,
  787`; the `context_static.1` domain at `plan.rs:41`); `prefix_affecting_params` is the
  LC-5 dialect-listed member (`crates/hh-gateway/src/dialect.rs:428-430`); demotion
  wrappers are profile-declared `prompt_layout` data recorded on
  `AssemblyOutcome.demotions` (`assemble.rs:89-93, 250-254`) and never enter the hash; the
  `role_map` collapse is `hh-provenance`'s `role_map_collapse` applied at lower-profile
  (`crates/hh-provenance/src/lower.rs:465-489`) — a render-time transform. The final
  answer: `static_hash` covers static-tier item *identities* — `definition`-slot `Text`
  leaves hash by `version_id`, never by rendered bytes (the provider's rendered-bytes
  cache key is the dialect layer's own `cache_markers`/`prefix_affecting_params` concern);
  `role_map` collapse and demotion wrappers stay out of the hash — the rendered form is a
  profile projection while the LC-2 run half is identity.
- **OQ-336 — foreign-verifier carrying.** Interim (quoted, the shared OQ-336/338/342 row):
  "a benchmark-supplied verifier is carried as a `foreign` digest (R2 unsupported for
  it)". Landed: `foreign{name, version, source_ref, digest_claim}` — `digest_claim` a
  `ForeignRef` claim, never a `ContentAddress` (I-2; `crates/hh-lab/src/bench.rs:38-40`);
  a non-pinned `image_digest` (tag, empty, foreign version string) is a claim landing the
  task in `unpinned[]`, capping `claimed_level` at {R0, R1, R3} — R2 requires the pinned
  image digest (`crates/hh-bench/src/adapter.rs:217-219, 467-491`); `verifier_isolation`
  is per-family with `separate` the stratum-B default (`adapters.rs`, `benchset.rs:
  581-594`). The final answer: digest-only carrying is the ratified N13 answer — the
  instrument integrates the foreign verifier by claim and never authors it as a vendored
  executable tree member; a digest-only verifier suite supports {R0, R1, R3} — R2 stays
  gated on the pinned `image_digest`; vendoring lands only by a new ADR.
- **OQ-338 — stratum-B pool + `SuiteValidityRecord` custodian.** Interim (quoted, the
  shared row): "stratum B's pool is unnamed (headline issue-resolution capability blocked,
  not Stage-3 acceptance)". Landed: stratum B is `adapter_b` — `FreshTemporal`,
  `coding_terminal`, mandatory `separate` isolation (`crates/hh-bench/src/adapters.rs:
  351-371`); the `SuiteValidityRecord` is the suite manifest's committed `audit` block
  (`benchset.rs:533-547, 771-773`); `headline_admissible` requires `audited_at` — an
  undated audit loads but never headlines (`benchset.rs:211-216`; S4.15). The final
  answer: the launch anchor is the committed `adapter_b` corpus; the custodian resolves
  to the suite author — the `audit` block is corpus-declared data the instrument
  validates, never instrument-produced, so N13 holds by construction; a named third-party
  pool (the S-496/S-495 fork) enters only as a new suite manifest carrying its own
  `SuiteValidityRecord` through the same `audited_at` gate — pool identity is
  suite-authoring data, never a kernel ruling.
- **OQ-342 — parity margin + subset design.** Interim (quoted, the shared row): "adapter
  parity uses the R3-style pre-registered margin (ADR-0142 D3; ADR-0144 D5/option 2;
  ADR-0146 D3)". Landed: `AdapterDeclaration.parity` is the `ParityReport` ref — `None`
  spells `parity = none` restricting the adapter to product-granularity rows (I-5;
  `crates/hh-bench/src/adapter.rs:319-331`); `ParityReport` is a
  `ComparisonReport{benefit_kind = artifact_benefit}` plus
  `{original_runner_ref, original_runs, replay_verdicts}` (`crates/hh-lab/src/
  analysis.rs:1288-1303`); `estimated{estimator, inclusion_probabilities_ref}` is set on
  adaptive-strategy rows only (`search_time_benefit`; `analysis.rs:961-965`) — a
  `full_set` comparison carries no `estimated` member. The final answer: the margin is
  the R3-style pre-registered margin — per-suite `PreRegistration` data per OQ-113's
  part-1 adoption, never a kernel constant and never stricter-by-default; the subset
  design (minimum n per task) is suite-authoring data declared on the `ParityReport`;
  `estimated{…}` rows never suffice for headline eligibility — the member is scoped to
  adaptive-strategy comparisons and cannot discharge an `artifact_benefit` parity claim.
- **OQ-360 — re-attempt defaults.** Interim (quoted, the shared OQ-360/366/367/375 row):
  "`ReattemptPolicy{max_per_plan = 2, max_fraction_of_plans = 0.10}`". Landed:
  `ReattemptPolicy{max_per_plan: 2, max_fraction_of_plans_ppm: 100_000}` is the packaged
  exemplar default (`crates/hh-lab/src/exemplars.rs:98-102`) with `backoff` a declared
  member (`crates/hh-lab/src/experiment.rs:548-557`); the engine enforces both bounds
  (`crates/hh-experiment/src/engine.rs:1618-1663`). The final answer: the interim values
  are the packaged defaults — the Stage-3 infrastructure-failure-rate measurement per
  environment class (with the OQ-121/OQ-339 replicate interaction) stays the ADR-0213
  entry test, preserved verbatim as the re-entry trigger that re-pins the values by
  amendment when the measurement exists.
- **OQ-363 — `iso_cost`/`matched_cap` run-sharing.** Interim (quoted): "the engine
  duplicates subject runs for `iso_cost` and `matched_cap` arms (ADR-0155 S-2; ADR-0156
  D2); AC-R-2.10.3-9 waits". Re-ruled: ADR-0278 (accepted, S3.4a) kept the OQ under
  ADR-0213's interim rule with the landed ruling shape — the `iso_cost` companion
  duplicates runs rather than sharing across `MatchSpec`s, cross-mode `compare`
  stays `IncommensurableMatch` (CF-334; the
  unenforced-`matched_cap`-dimension arm CF-350), and GATE-G2's disposition recorded
  "OQ-363 interim rule stands". Landed: the engine's doc contract states the rule
  verbatim — arms never share subject runs, each arm's cells expand independently
  (`crates/hh-experiment/src/engine.rs:42-44`); `lab/control-strategy-family-v1` rides
  one `iso_cost` companion arm sharing a design point with the `matched_cap` group with
  duplicated runs (`crates/hh-lab/src/exemplars.rs:858-895`). The final answer: the
  duplicate-run control is the ratified answer — one run never serves two `MatchSpec`s
  (S-2 and E-2 hold by construction); ADR-0278's revisit trigger is preserved verbatim —
  a "one run, two views" ruling lands only by a later ADR that preserves S-2 and E-2.
- **OQ-366 — `draws` defaults + preview policy + resample caching.** Interim (quoted, the
  shared row): "`draws` 2000/200". Landed: `report_label` forces `preview` below
  `CONFIRMATORY_DRAWS = 2000` `resampling_draws`; `confirmatory` additionally needs
  pre-registration coverage (ADR-0157 D5); `preview` never persists as confirmatory
  (`crates/hh-analysis/src/report.rs:213-236`); `resampling_draws` is a declared
  `AnalysisInput` member (`kernel.rs:97-98`); no resample-index cache exists in
  `hh-analysis`. The final answer: the 2000 confirmatory floor is the ratified gate and
  `preview` stays the reduced-draws, never-persisted class; per-report-class defaults
  resolve to caller-declared `resampling_draws` — the 2000/200 labels stay the guidance
  values, not a second constant table; the >10⁴-row caching rule resolves to no cache —
  a report resamples, and an index cache lands only by ADR amendment.
- **OQ-367 — `T_CLT`/`T_BCA` floors.** Interim (quoted, the shared row): "`T_clt = 100`,
  `T_bca = 30`". Landed: `T_CLT = 100`, `T_BCA = 30` constants
  (`crates/hh-eval/src/scorecard.rs:49-51` — the S3.4c ADR-0280 correction to the
  ADR-0158 placeholder); a floor violation substitutes the first admissible fallback and
  records it in `EstimatorSelection.substituted` with the applied set on
  `EstimatorSelection.floors` (`scorecard.rs:165-251`); the KA-1/KA-2 coverage sims
  landed ≥93% recovery at nominal (`crates/hh-analysis/tests/kernel.rs:1364, 1435`). The
  final answer: the global floors {100, 30} are the packaged defaults — the measured
  coverage already backs them; floors are global, not per metric family; a per-family
  set lands only by amendment (the applied floors are already declared data on
  `EstimatorSelection.floors`).
- **OQ-375 — `AdmissionPolicy` per-stage defaults.** Interim (quoted, the shared row):
  "`AdmissionPolicy` defaults `validated`/R0" (the ADR-0163 D2 segment). Landed:
  `LeaderboardDefinition::new` — `min_status = Validated`, `min_claimed_level = None`
  (no claimed-level gate — the R0 floor), all splits (`crates/hh-results/src/
  leaderboard.rs:164-184`); `::public` — `min_status = Reproduced`, the C2/Stage-5
  published tier (`leaderboard.rs:186-194`); `disclosure_policy.min_replicates` is the
  L6 replicate floor failing `insufficient_replicates` (`leaderboard.rs:43-60, 851-852`);
  `unadmitted[]{reason}` canonical (`leaderboard.rs:463-510`). The final answer: the
  per-stage pair is the landed default set — Stage-3 lab-internal `Validated` admission
  with no claimed-level gate, Stage-4 published `Reproduced`; `min_replicates` stays
  suite-declared disclosure data — OQ-121's replicate interaction folds into the suite's
  own declared floor, never a kernel constant.
- **OQ-387 — `workspace_trust` Π leaf.** Interim (quoted): "`workspace_trust` is a
  manifest claim from the H5 trust store; `unknown` narrows as `untrusted`; no Π rows
  named (ADR-0168 D7)". Landed: `WorkspaceTrust ∈ {trusted, untrusted, unknown}` on
  `RunManifest` (`crates/hh-ledger/src/manifest.rs:199-228, 436`), recorded verbatim from
  the H5 trust store — absent, unreadable or malformed ⇒ `unknown`, never inferred
  (`crates/hh-cli/src/trust.rs:40-64`; `crates/hh-embed/src/open.rs:666-670`); subagent
  spawn inherits it (`crates/hh-subagent/src/spawn.rs:1457`); `manifest.rs:199` documents
  "never a Π input at this layer" — no `hh-monitor`/`hh-control`/`hh-containment` row
  consumes it (grep-verified at base 23ccfbd). The final answer: `unknown` narrows as
  `untrusted` is adopted verbatim; the narrowed Π set is the empty set at launch — the
  claim is run-scoped audit data (recorded, inherited, never consumed), and a Π leaf
  naming its rows lands only by an ADR amendment — the ADR-0213 entry test preserved
  verbatim as re-entry.
- **OQ-407 — OQ-075 measurement design.** Interim (quoted, the shared OQ-407/075 row):
  "first-party `hot_path` variants stay in process; out-of-process admissibility for
  `hot_path` classes is subject to a ceiling recorded as an assumption-debt record;
  `LocalityTooSlow` proposed (ADR-0181 D8)". Landed: `boundary_overhead_ms` (M19) is
  kernel-measured per invocation on `lifecycle.component.invoked`
  (`crates/hh-varhost/src/host.rs:281`); the ceiling lands as an absolute-p95-ms
  `AssumptionDebtRecord` keyed `OQ-075.hot_path_ceiling.{class_id}` with
  `removal_test_ref: probe:m19_boundary_overhead` (`crates/hh-varhost/tests/
  overhead.rs:301-343`; AC-R-2.12.2-9); `hot_path` is a declared `CapabilityClass` member
  and `hot_path_operations[]` a manifest claim lowered to `Permission`
  (`crates/hh-registry/src/records.rs:120`, `extension/plugin.rs:251-268`);
  `LocalityTooSlow` stays proposed-only in spec text (§8 crossings table — never a
  landed typed refusal). The final answer: the landed absolute-p95 form is the ceiling —
  the `model_latency_ms` fraction is rejected in favour of the recorded debt bound; the
  per-step op list is each class's declared `hot_path_operations[]`, measured per
  invocation; refusal-vs-stratification resolves to refusal — `LocalityTooSlow` is
  adopted as the named typed refusal for ceiling breach (the spec-text "proposed" marker
  becomes the adopted name); stratification-only would leave a breach unenforced.

## Conflicts (part 2)

None registered. The two conflict rows the OQ-363 answer rides — CF-334 (cross-mode
`compare` incommensurable) and CF-350 (an unenforced `matched_cap` dimension refuses) —
are already-registered conflicts the proposal cites, not new divergences; every other
row's landed shape is consistent with its interim text per the R2.23 revalidation cell.
No new CF row.

<!-- Part 2 (R4.12a) appended 2026-10-22 — the adopt-all-except line at the head covers
     both parts; GATE-G7's act performs the ratifications; R4.15a conforms the adopted set. -->

> *Reconciled 2026-10-22 (GATE-G7 fleshing):* the `R4.15a` conform cells name the pre-split row — this package's records conform lands at **R4.15a2** (164c); the OQ-170 pointer cell is written there.
