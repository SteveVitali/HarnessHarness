# ADR-0214 package briefs — BL-37 (R4.13 part 1 + R4.13a part 2, 2026-10-22)

<!-- Brief-file convention OWNED BY R4.8 (docs/build/planning/round4-bl33-ws-l6-briefs.md;
     ticket 157): this file reuses its table columns, adopt-all-except line and the four
     disposition tokens verbatim. This ticket never rules — `recording-act` rows only cite
     an operator ruling already in LEDGER GATE DECISIONS or a landed ADR; everything else
     is a proposal for GATE-G7. Part 2 (OQ-418 … OQ-458) is R4.13a's append below — the
     adopt-all-except line covers both parts. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it — cite
tests/ADRs) · `adopt-interim-as-final` (the interim rule in force becomes the ratified
answer — quoted) · `descope-by-ADR` (permanently out of the build — ADR text + authorising
operator words named) · `needs-code-leg(<row>)` (post-GATE-G7 conform row + the exact
change). Every interim rule below is quoted from the ratified ADR-0214 Decision table;
every landed-evidence citation was re-checked at base 833f39a.

## Part 1 (R4.13) — batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-116 | resolve-with-evidence | the consent pair is landed and answers "both": the participant declaration carries `model_io_intercept` verbatim (`crates/hh-hosting/src/records.rs`) and consent is recorded in the run manifest's granted `sink_id` set — a `requires_consent` sink absent from it refuses `ConsentMissing` (`crates/hh-telemetry/src/export.rs`); the owner is the run's human principal who seals the manifest | R4.15a |
| OQ-177 | resolve-with-evidence | answered — recording act citing ADR-0306 D1 (the determining landed ADR): the `ApproverGrant` record confers; `EndorserRef::ApproverGrant{grant_ref}` serves only while a live grant `covers_grant`s capability+risk; delegation-by-grant won — no `principal(delegated)` minted; `covers_grant` narrows monotonically | R4.15a |
| OQ-242 | adopt-interim-as-final | the landed pin `ACP_VERSION = "0.1.0"` with `v1-compat` era negotiation (`ProtocolVersionMismatch`, never coerced — `crates/hh-acp/src/protocol.rs`) is the ratified pin; the removal test is ADR-0197's `zero_uses{participant registry hosts speaking v1}` + `date` window; the alpha.N-vs-GA fork resolves to the pinned version id — the artefact names a version string, never an upstream release class | R4.15a |
| OQ-246 | adopt-interim-as-final | the interim's `ApprovalLease` form is the landed amendment: `respond_permission` carries `allow_lease` minting `ApprovalLease{pattern?, scope ∈ {turn, run, session}, max_uses?}` sealed by a principal (`crates/hh-embed/src/work.rs`, `crates/hh-monitor/src/mint.rs`); the ACP lowering admits `{allow_once, allow_lease, deny, more_info}` and names every unexpressible member an `hh-edge-loss/1` loss (ADR-0306 D3); `allow_lease_requires_capability_material` guards minting; `approvals.requested` exhaustion ⇒ `deny` | R4.15a |
| OQ-368 | adopt-interim-as-final | the interim stands as the launch posture: `max_age` default 90 days (the placeholder becomes the ratified default) and the hosted re-probe cadence is `revalidation.schedule` on the report — landed as `SurfaceExpiry{max_age_ms, superseded, expiring}` + the `revalidation` trigger (`crates/hh-analysis/src/ops.rs`); the ADR-0214 entry test (re-pin from Stage-5 expiry data) is preserved verbatim as the re-entry trigger | R4.15a |
| OQ-378 | adopt-interim-as-final | the landed lift is the interim verbatim: `lift_stop_reason` maps `other`, vendor `_`-prefixed and unknown stops to `infrastructure_failure{participant_unclassified}` with the raw value preserved (`crates/hh-hosting/src/proj.rs`, `adapter_a.rs`); the `other`-with-passing-oracle arm resolves to the same class — the lift never consults an end-state oracle | R4.15a |
| OQ-379 | resolve-with-evidence | the landed `ProcessPlacement{InEnvironment, LabHost, …}` sum answers the disjunction (`crates/hh-hosting/src/records.rs`): the C1 default is `in_environment` with the protocol bridged through the helper's exec channel (adapter zero defaults it — `adapter_zero.rs`); `lab_host` + path-mounted `local_sandboxed` stays a declared non-default member; `enforcement_evidence` is the recorded `containment.enforcement_evidence` manifest member (`crates/hh-embed/src/open.rs`, `crates/hh-containment/src/attach.rs`) | R4.15a |
| OQ-380 | adopt-interim-as-final | the interim verbatim is what landed: `is_stale` is `participant_version_identity != current_version_identity` (`crates/hh-registry/src/records.rs`) — staleness keys on `version_identity` change only, never a calendar/run-count basis; the stale-vs-unknown arm resolves to annotate — `stale_dimensions` is surfaced as data and a stale-only dimension falls back through declaration→`unknown` by the reconcile order (`crates/hh-registry/src/hosted.rs`), never a silent downgrade | R4.15a |
| OQ-381 | adopt-interim-as-final | the interim verbatim is the landed shape: `cancel_deadline_ms` and time-to-first-event are adapter parameters without kernel defaults; `ensure_terminal` mints the synthesized `unobserved` terminal when an adapter's deadline passes (`crates/hh-hosting/src/events.rs`; adapters A/B/C exercise it) — the `EnvelopePolicy`-vs-adapter fork resolves to adapter data | R4.15a |
| OQ-386 | adopt-interim-as-final | riding OQ-246's adoption: the attended terminal's default option set stays `{allow_once, deny_once}` for every `effective_risk_class` (`crates/hh-cli/src/cli.rs`) — no per-class lease-option table at launch; `NarrowingHint` renders as a suggestion requiring an explicit `allow_lease` response (ADR-0170's landed rule) — it can never mint or widen | R4.15a |
| OQ-388 | — (pointer only — no proposal here) | the ratification proposal is R4.15's (single-owner rule; DF-S4.13-1 → BL-30); the measured bound + per-projection maintenance policy stand landed (ADR-0305 D1; folded into spec at REC.2 per ADR-0331 D4; `ac_r_2_2_1_16_latency_1e5_fixture` ≈15 ms for 100-of-10⁵) — "adopt all" has nothing to adopt on this row | R4.15 |
| OQ-392 | adopt-interim-as-final | the interim posture is landed: per-surface-session `SinkPolicy` is declared data with `{content} ⇒ requires_consent` mandatory and `ConsentMissing` at export (`crates/hh-telemetry/src/sinks.rs`, `export.rs`); the per-kind default table resolves to declared policies over the `{accounting}` floor (`SinkPolicy::accounting`); an `ApproverGrant` reviewer or read-only observer carries a declared policy scoped to its grant/role; `requires_consent` applies to hosted `model_io` through the OQ-116 pair | R4.15a |
| OQ-400 | resolve-with-evidence | the first disjunction arm landed: `serve_bundle` is the launch-class op wrapping `lab.serve` — it returns `connection_info` and mints `session_handle` on the surface run bound to the session (ADR-0318; `crates/hh-mcp-lab/src/dispatch.rs`, `exposure.rs`); the server is service-owned and `close(session)` releases it through the handle lifecycle — never an adapter `spawn_process` | R4.15a |
| OQ-401 | adopt-interim-as-final | the interim verbatim is landed: binding (c) is loopback-only at C1 and the ADR-0301 D3 ordered gate is the ratified (c) scheme — `?token=` → 400, `Host` ≠ bound authority → 400, `Origin`/`Sec-Fetch-*` → 403, `Authorization: Bearer` constant-time → 401 with `WWW-Authenticate`; non-local exposure stays a fourth-binding decision by its own ADR — never an implicit widening (ADR-0301's revisit trigger names this row) | R4.15a |
| OQ-405 | adopt-interim-as-final | riding OQ-246's adoption: the `allow_pattern` spelling resolves to `allow_lease{pattern}` carried by `respond_permission` through `RespondPermissionParams` (`crates/hh-embed/src/work.rs`); `attach`-mode viewers are read-only by construction (`crates/hh-embed/src/open.rs`) and see the H7 cache state through `list_leases` — the landed (experimental) visibility path | R4.15a |
| OQ-413 | adopt-interim-as-final | the interim verbatim is landed: `check_return_contract` runs as the validator at the child's `stop` — `over_max_tokens` is a violation kind and `return_contract_satisfied` rides `SubagentResult` (`crates/hh-subagent/src/result.rs`, `types.rs`); the default `summary.max_tokens` is `none` (`ReturnContract::default()` admits it — C1); profile-conditioned defaults stay inadmissible | R4.15a |
| OQ-417 | adopt-interim-as-final | the interim verbatim is landed: message caps are kernel constants + `MessagingPolicy` definition data (`max_pending = 16`, `coalesce`, direction flags — `crates/hh-orchestrator/src/topology.rs`; definition data per ADR-0191 M-2 — `crates/hh-subagent/src/types.rs`); queue-full is `MessageRefused{queue}`, never a drop; neither fork arm lands — no `messages.*` dialect bump and no registered `ext` dimensions on the child's budget node | R4.15a |

## Part 1 — per-OQ basis (ADR-0214 interim quoted; landed evidence re-checked)

Each row quotes the ratified interim rule (ADR-0214 Decision table); the landed shape is
the interim already in force or the evidence that answers it; the outstanding workstream
legs fold into the adoption — the WS-* ruling venues no longer run, so GATE-G7's act *is*
the ratification the entry tests awaited. The ADR-0214 spec-debt entry tests stay
preserved verbatim as re-entry triggers; R4.15a lands the adopted rulings as register/ADR
amendment-log appends.

- **OQ-116 — hosted L2 consent owner + record home.** Interim (quoted): "hosted L2
  (`model_io`) capture waits on consent; `SinkPolicy.requires_consent` and
  `PublicationPolicy` restrict by default (ADR-0044; ADR-0141 D4; ADR-0165 (d))". Landed:
  the participant declaration carries `model_io_intercept` verbatim
  (`crates/hh-hosting/src/records.rs:116-117, 240-241`); `{content} ⇒ requires_consent` is
  a mandatory `SinkPolicy` invariant (`crates/hh-telemetry/src/sinks.rs:249-264`); the
  consent record is the manifest's granted `sink_id` set — "`consents` is the granted
  `sink_id` set — the manifest consent member's Stage-1 seam; absent ⇒ `ConsentMissing`,
  never a silent" (`crates/hh-telemetry/src/export.rs:425-442`). The final answer: *both*
  — the capability declaration declares the interception; the run manifest's granted set
  records the consent; the owner is the run's human principal, who seals the manifest
  (declaration alone never consents — the granted set is the seam the export gate reads).
- **OQ-242 — ACP v2 schema pin.** Interim (quoted): "the ACP v2 stable core is the
  session-ABI baseline with the v1 compatibility profile carrying a debt record whose
  removal test is fixed (ADR-0098; ADR-0197); the exact schema pin is open". Landed:
  `ACP_VERSION = "0.1.0"` + `ACP_LEGACY_LABEL = "v1-compat"` + `SUPPORTED_VERSIONS`
  (`crates/hh-acp/src/protocol.rs:11-16`); negotiation maps to `AcpEra::{V2, V1}` and any
  offered `protocolVersion` outside the pinned set is `ProtocolVersionMismatch` — never
  coerced (`protocol.rs:60-66`); the removal-test half is resolved (ADR-0197:
  `zero_uses{participant registry hosts speaking v1}` + a `date` window). The final
  answer: the landed `0.1.0` pin + era negotiation is the ratified pin; the alpha.N-vs-GA
  fork dissolves — the artefact pins a version id, it makes no upstream release-class
  claim; the removal test stands as ADR-0197's `zero_uses` + window.
- **OQ-246 — ledgered policy amendment across an edge.** Interim (quoted, the shared
  OQ-246/386/405 row): "`allow_once`/`deny_once` only across every edge (ACP, MCP
  `human_principal`, `respond_permission`, the CLI and web inbox); `allow_always` lowers
  to the H7 pattern cache only as an `ApprovalLease` at `scope ≤ run` sealed by a
  principal (ADR-0070 D3; ADR-0071; ADR-0098 D3; ADR-0174 consequences; ADR-0177 D5)".
  Landed: `respond_permission` carries the `allow_lease` option kind and mints the lease
  through the monitor (`crates/hh-embed/src/work.rs:636-672, 1108-1141`;
  `crates/hh-monitor/src/mint.rs:193-253` — `approval`-basis endorsement, `scope ∈
  {turn, run, session}`, `session` requires a declared pattern); absent capability
  material the mint refuses `allow_lease_requires_capability_material`; the ACP lowering
  admits exactly `{allow_once, allow_lease, deny, more_info}` and every unexpressible
  member is a named `hh-edge-loss/1` loss (`crates/hh-acp/src/permission.rs:135-136`;
  ADR-0306 D3); the attended CLI presets still offer only `allow_once`/`deny_once`
  (`crates/hh-cli/src/cli.rs`). The final answer: the `ApprovalLease` is the ledgered
  amendment form — `allow_lease` is the option-kind spelling, scope ≤ run with `session`
  pattern-gated, `max_uses` declared; `approvals.requested` exhaustion ⇒ `deny`; the
  `allow_always`-class selection never enters any edge verbatim — it lowers only to the
  lease the interim named.
- **OQ-368 — surface `max_age` + re-probe cadence.** Interim (quoted): "surface `max_age`
  placeholder 90 days; hosted re-probe cadence = `revalidation.schedule` on the report
  (ADR-0160 D5)". Landed: `SurfaceExpiry{fitted_at, now_ms, superseded, expiring,
  max_age_ms, owner}` with `max_age_ms` documented "OQ-368's 90-day placeholder"
  (`crates/hh-analysis/src/ops.rs:729-747`) and the `revalidation` trigger evaluated in
  `fit_surface`'s expiry pass (`ops.rs:1623-1728`). The final answer: the 90-day
  placeholder is the ratified launch default and `revalidation.schedule` is the re-probe
  cadence; the ADR-0214 entry test — WS-J4/I6/J6 re-pin from Stage-5 expiry data — is
  preserved verbatim as the re-entry trigger (the value re-pins by amendment once that
  data exists; the fixture lane supplies none).
- **OQ-378 — hosted `StopReason` lifting.** Interim (quoted, the shared
  OQ-378/379/380/381 row): "hosted `StopReason = other` lifts to
  `infrastructure_failure{participant_unclassified}` with the raw value preserved".
  Landed: `lift_stop_reason` maps `other`, vendor `_`-prefixed and unknown values to
  `infrastructure_failure{participant_unclassified}` with the raw value carried
  (`crates/hh-hosting/src/proj.rs`, exercised by `adapter_a.rs`). The final answer: the
  unconditional lift is the ratified rule — the `other`-with-passing-oracle arm resolves
  to `infrastructure_failure{participant_unclassified}`: the lift never consults an
  end-state oracle (a hosted run that stops `other` is unclassified participant
  infrastructure failure by construction; an oracle verdict cannot reclassify a lifted
  stop).
- **OQ-379 — `process_placement` + bridging for session-ABI CLIs.** Interim (quoted, the
  shared row): "`process_placement` and transport bridging for session-ABI CLIs unset".
  Landed: the `ProcessPlacement{InEnvironment, LabHost, …}` sum exists
  (`crates/hh-hosting/src/records.rs`) with adapter zero defaulting `in_environment`
  (`crates/hh-hosting/src/adapter_zero.rs`); `containment.enforcement_evidence` is a
  recorded manifest member consumed at `open` (`crates/hh-embed/src/open.rs:636,
  671-683, 1139, 3713-3716`; `crates/hh-containment/src/attach.rs:377`). The final answer:
  the C1 default is `in_environment` with the protocol bridged through the helper's exec
  channel — the arm WS-B5's helper supports at C1 (the helper IS the exec-channel
  bridge); `lab_host` with a path-mounted `local_sandboxed` environment stays a declared
  member, never the default; `enforcement_evidence` records the environment handle's
  evidence map on the manifest.
- **OQ-380 — `ConformanceRecord` expiry + stale semantics.** Interim (quoted, the shared
  row): "conformance records go `stale` only on `version_identity` change". Landed:
  `is_stale` is exactly `participant_version_identity != current_version_identity`
  (`crates/hh-registry/src/records.rs:381-383`) — no calendar/run-count expiry exists;
  the reconcile is "latest non-stale then declaration then `unknown`" and
  `stale_dimensions` surfaces the stale-only set as report data
  (`crates/hh-registry/src/hosted.rs:27-40, 52-103`; surfaced by `describe` —
  `crates/hh-embed/src/hosting_ops.rs:238-239`). The final answer: no calendar/run-count
  expiry — `version_identity` change is the only staleness basis; a stale record
  *annotates* — it exits the fresh fold and the dimension reports through
  `stale_dimensions`, falling back to declaration then `unknown`; the staleness is the
  report, never a silent downgrade.
- **OQ-381 — liveness deadlines.** Interim (quoted, the shared row):
  "`cancel_deadline_ms` and time-to-first-event are adapter parameters without
  defaults". Landed: `ensure_terminal` mints synthesized `unobserved` terminals —
  `participant_unclassified("unobserved")` status rows — when an adapter's deadline
  passes (`crates/hh-hosting/src/events.rs:702-833`; adapters A/B/C exercise it). The
  final answer: the deadlines stay adapter-declared parameters with no kernel defaults —
  the `EnvelopePolicy` arm rejected (the hosted channel is the adapter's own data, F2's
  envelope policy governs kernel envelopes only); `unobserved` synthesis is the landed
  missing-terminal mechanism.
- **OQ-386 — attended lease options + `NarrowingHint` rendering.** Interim (quoted, the
  shared OQ-246/386/405 row): "`allow_once`/`deny_once` only across every edge". Landed:
  the attended CLI presets offer only `allow_once`/`deny_once`
  (`crates/hh-cli/src/cli.rs`); the lease machinery the hint may name lands through
  `respond_permission`'s `allow_lease` (OQ-246). The final answer: the default
  per-`effective_risk_class` option set is `{allow_once, deny_once}` for every class at
  launch — no per-class lease table; `NarrowingHint` renders per ADR-0170's landed rule
  as a suggestion requiring an explicit `allow_lease` response — a hint is display data
  and can never mint, so it is never a widening path.
- **OQ-392 — per-session-kind `SinkPolicy` defaults.** Interim (quoted): "surface
  `SinkPolicy` shape fixed (P8); defaults per surface-session kind open; `content`-class
  items delivered only within the reader set (ADR-0172 P8; ADR-0034)". Landed:
  `SinkPolicy{sink_id, content_classes ⊆ {accounting, structural, content, diagnostic},
  redaction, requires_consent, …}` with `{content} ⇒ requires_consent` mandatory and a
  `{content, diagnostic}` combination refused (`crates/hh-telemetry/src/sinks.rs:212-264`);
  `SinkPolicy::accounting` is the minimal `{accounting}` floor (`sinks.rs:234-245`);
  `ConsentMissing` enforces at export (`export.rs:425-442`). The final answer: the
  per-session-kind default is declared `SinkPolicy` data over the `{accounting}` floor —
  the run's human principal's local surface session declares `{accounting, structural,
  content}`; an `ApproverGrant` reviewer declares a policy scoped to its grant (classes ⊆
  what the grant covers); a read-only observer declares without `content`; hosted
  `model_io` falls under the same `{content} ⇒ requires_consent` rule — the OQ-116 pair
  (declaration declares, manifest grants) carries the consent.
- **OQ-400 — J6 Adapter-A supply-surface lifecycle.** Interim (quoted): "the
  supply-surface ref passed at `open` is `(bundle_id, profile_binding)` (CF-365);
  lifecycle ownership (adapter calls `serve_bundle` vs spawns the server as its own
  `spawn_process` effect; who closes it) open (ADR-0173/0174 D9; ADR-0164)". Landed:
  `serve_bundle` is a `spawn_process`-class hosting tool wrapping `lab.serve` — it
  returns `connection_info` and mints a `session_handle` (`HandleKind::Session`) on the
  surface run bound to the calling binding (`crates/hh-mcp-lab/src/dispatch.rs:574-608`;
  `exposure.rs:56, 1764-1771`; ADR-0318). The final answer: the first disjunction arm is
  the landed answer — Adapter A calls `serve_bundle` and passes the returned
  `connection_info` into `HostedRunSpec`; the server is service-owned: the minted
  `session_handle` rides the surface session's handle table, so `close(session)`
  releases it through the ordinary handle lifecycle — the adapter never spawns the
  server as its own `spawn_process` effect.
- **OQ-401 — non-local hosts + the (c) scheme.** Interim (quoted): "binding (c) is
  localhost-only at C1; a non-local host is a fourth binding by ADR with WS-H3
  authentication and WS-H4 egress rules (ADR-0179 D5); the (c) authentication scheme is
  WS-H3/K2's". Landed: the (c) scheme was ruled at S4.10 (ADR-0301 D3) — loopback-only,
  ordered transport-level gate: `?token=` → 400, `Host` ≠ bound authority → 400,
  `Origin`/`Sec-Fetch-*` → 403, `Authorization: Bearer` constant-time → 401 with
  `WWW-Authenticate` and no diagnostic; `hh-web`/`NetClient` share `hh-wire::http`. The
  final answer: the interim verbatim — (c) stays loopback-only at C1 with the ADR-0301
  D3 gate the ratified scheme; non-local exposure is never an implicit widening — a
  fourth binding lands only by its own ADR carrying the WS-H3 authentication + WS-H4
  egress rulings.
- **OQ-405 — `allow_pattern` + attach-visibility.** Interim (quoted, the shared
  OQ-246/386/405 row): "`allow_once`/`deny_once` only across every edge". Landed:
  `respond_permission` carries the lease through `RespondPermissionParams`' `allow_lease`
  option kind (`crates/hh-embed/src/work.rs:636-672`; `hh-embed-schema` types);
  `attach` is read-only by construction (`crates/hh-embed/src/open.rs`); `list_leases` is
  the landed Group-R visibility path over the H7 cache. The final answer (riding
  OQ-246's adoption): the `allow_pattern` spelling is `allow_lease{pattern}` — a
  pattern-bearing `ApprovalLease` IS the ledgered `allow_always`-class amendment; the
  H7 cache is visible to `attach`-mode viewers read-only through `list_leases` — never a
  write or mint path.
- **OQ-413 — `ReturnContract.summary.max_tokens` default + enforcement site.** Interim
  (quoted): "`ReturnContract.summary.max_tokens` enforced as a validator at the child's
  `stop`; C1 admits `none`; profile-conditioned defaults inadmissible until ruled
  (ADR-0187)". Landed: `check_return_contract` runs at the child's result assembly;
  `over_max_tokens` is a named violation kind; `return_contract_satisfied =
  violations.is_empty()` rides `SubagentResult`
  (`crates/hh-subagent/src/result.rs:151-247`); `ReturnContract.summary` is
  `{schema?, max_tokens?}` and `ReturnContract::default()` admits `max_tokens = none`
  (`crates/hh-subagent/src/types.rs:398-420`). The final answer: the validator-at-`stop`
  is the ratified enforcement site (the child-side gauge-cap arm rejected); the launch
  default is `none`; profile-conditioned defaults stay inadmissible.
- **OQ-417 — message caps home.** Interim (quoted): "message-rate/size/pending caps are
  kernel constants and `MessagingPolicy` data (ADR-0191 M-2); no dimension added to
  ADR-0039". Landed: `MessagingPolicy{max_pending = 16, coalesce, direction flags}` is
  sealed-definition data (`crates/hh-orchestrator/src/topology.rs:143-144`;
  `crates/hh-subagent/src/types.rs:1088` — MUST-data per ADR-0191 M-2); a full queue
  answers `MessageRefused{queue}`, never a drop; the only landed `message_*` budget
  dimension is `message_human` (ADR-0207 D6 — a different concern). The final answer:
  kernel constants + `MessagingPolicy` data is the ratified home — both fork arms
  rejected: no `messages.sent`/`messages.pending` dialect bump and no registered `ext`
  dimensions on the child's budget node.

## OQ-177 — recording act (the ADR-0306 D1 re-confirmation)

OQ-177 asked the authority class of a human reviewer acting under an `ApproverGrant` —
delegation carrying `principal` with the grant as `basis_ref`, or a new
`principal(delegated)` sub-class. The interim rule in force (quoted): "a reviewer under
an `ApproverGrant` endorses one `effect_id` with the grant as `basis_ref`; the
endorsement's class is the grantor's `principal` by delegation, never a new class
(ADR-0070 D6; ADR-0033 D2 closed set)". The R2.24 revalidation recorded the question
answered.

**Determining landed ADR — ADR-0306 D1 (S4.14a, accepted):** "`ApproverGrant` is a
ledgered record; the record confers, never the name": `grant_approver` mints
`{grant_ref, grantor, grantee, capability_prefixes[], max_risk, expires_at?,
revoked_at?}` content-addressed; `security.permission.grant_issued` /
`grant_revoked` fold through `ApprovalState::project`; the respond-path gate accepts an
`EndorserRef::ApproverGrant{grant_ref}` only while a live grant `covers_grant`s the
pending's capability and decided risk; `revoke_approver` is grantor-or-principal+ only;
`covers_grant` makes delegation monotonically narrowing (prefixes ⊆, `max_risk` ≤)
(`crates/hh-monitor/src/approval.rs`, `mint.rs`; battery
`crates/hh-monitor/tests/s4_14a.rs`).

This ticket records the answer on OQ-177's register cell citing the ADR — a recording
act, not a proposal: ADR-0306 D1 fully determines it. Delegation-by-grant won — no
`principal(delegated)` `AuthorityClass` member was minted (the seven-class sum is
unchanged — `crates/hh-provenance/src/authority.rs`); the interim's endorsement shape is
the landed gate. Proposed disposition `resolve-with-evidence` — re-confirmed, nothing
outstanding; the conform at R4.15a flips the register.

## OQ-388 — package-side cell + pointer (ratification proposal is R4.15's)

OQ-388 asked which ledger projections are incrementally maintained vs rebuilt on demand
and the `read(filter)`/`project(until_seq)` query-latency bound on a 10⁵-event run. The
measurement leg is discharged: S4.13 measured (ADR-0305 D1, 2026-09-28) —
`by_event_id`/`by_class`/`ir_index` incremental at append and rebuild-identical;
`read(filter)` result-proportional at ≈15 ms for a 100-of-10⁵ filtered read
(`ac_r_2_2_1_16_latency_1e5_fixture`, CI-enforced); the numbers folded into the spec at
REC.2 (ADR-0331 D4). The WS-B1/K2 ratification act stays owed — **DF-S4.13-1 → BL-30**
names R4.15 its single owner (round-4 amendment: R4.15 drafts the ratification →
GATE-G7 → R4.15b records it). This ticket therefore lands the package-side register
cell + this pointer and proposes **no** disposition — a second proposed disposition
here would pre-empt R4.15's (the OQ-170/DF-S2.5-1 precedent at R4.12). OQ-388 is absent
from the adoptable set by design: "adopt all" has nothing to adopt on this row.

## CF-476 / CF-487 — package-side evidence (adjudication is R4.15's)

Package-side evidence paragraphs for R4.15's adjudication brief — recorded, not
pre-answered (the CF-488 precedent at R4.11: the pointer cell lands here, the FINAL
proposed disposition lands in R4.15's brief).

- **CF-476** — `outcome_bounds.verdict = robust` gate-7 conflict (Spec §10.2 ×
  ADR-0046 D7 six gates × ADR-0190 D6). Register state: the interim reading keeps gate
  7 out of the verbatim table — `outcome_bounds`/`multiplicity` stand as a reporting
  rule (ADR-0157 (e) fields; ADR-0158) with ADR-0190 D6 cited for scope
  (non-`static` `compute_policy` default only); the general-C4-admissibility question
  is registered to WS-I5/WS-J4 as OQ-471 (Phase-5 hygiene pass, due Stage 6). The
  package-side evidence this package carries: the BL-37 revalidation (R2.24) re-checked
  the row still-pending — the reporting-rule restatement remains the operative reading
  and no landed code widened the `robust` verdict into a general admissibility gate.
  R4.15's adjudication owns the OQ-471 line (the WS-I5/WS-J4 venues no longer run —
  the adopt/decline act lands at GATE-G7 through R4.15's brief).
- **CF-487** — `refused_by_kernel` derivation gap (ADR-0169 D6 `ExitClass` sum ×
  ADR-0176 D5 refusal set × Spec §07.1). Register state: the spec-only interim reading
  stands — `refused_by_kernel` = a kernel refusal of the invocation after `hello` and
  before a turn runs (the ADR-0176 D5 errors); it opens no turn and carries
  `result.reason`; ratification by an ADR-0169 amendment is registered as OQ-470
  (WS-K1 with WS-K4, WS-F2; Stage 2). The package-side evidence: R2.24 re-checked the
  row still-pending; ADR-0169 D6 is under-specified, not contradicted — the interim
  reading fills the derivation gap without amending. R4.15's adjudication owns the
  OQ-470 amendment question.

## Conflicts (part 1)

None registered. Every part-1 row's landed shape is consistent with its interim text
per the R2.24 revalidation cell; CF-476/CF-487 are already-registered conflicts the
package points at for R4.15, not new divergences. No new CF row.

## What this brief does not decide

No register status changes, no amendment-log appends, no schema or spec conform land
here — every cell above is a proposal the operator adopts, amends or declines at
GATE-G7 (the records conform is R4.15a's; OQ-388's ratification proposal and the
CF-476/CF-487 adjudications are R4.15's; a post-GATE-G7 `needs-code-leg` lands at
R4.15c by default). The interim rules stay in force unchanged until then; every
ADR-0214 entry test is preserved verbatim as a re-entry trigger. BL-37's flip is
R4.16's accounting after the adopted set conforms.

<!-- Part 2 (R4.13a) appends below this marker — the adopt-all-except line at the head
     covers both parts; GATE-G7's act performs the ratifications; R4.15a conforms the
     adopted set. -->

## Part 2 (R4.13a) — batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| OQ-418 | resolve-with-evidence | the landed unconditional inheritance answers both halves: `attendance: parent_manifest.attendance` at spawn with no input selecting otherwise (`crates/hh-subagent/src/spawn.rs`) — an interactive parent *cannot* spawn an unattended child, so the sealed pre-authorization arm is moot; the unattended-column interaction is Π-12's landed transform (`ask` → `deny`/`defer`/`auto_review` — `crates/hh-monitor/src/decision.rs`) | R4.15a |
| OQ-420 | adopt-interim-as-final | the landed `bandit` kind is the interim's `unknown`-below-`min_n` rule over declared params (`min_n`, `explore_ppm`, `lambda_ppm`, `cold_start_ppm`) on non-pooling `(task_class, model_ref)` cells — a deterministic integer-ppm fold (`crates/hh-gateway/src/router.rs`); window/discount + drift-reset grace stay view-side projection policy; the `lab/value-of-compute-v1` `bandit`-arm re-pin is preserved verbatim as the re-entry trigger | R4.15a |
| OQ-424 | adopt-interim-as-final | the interim verbatim is the landed gate: `bind` runs in process for every admitted estimator and `bandit`/`surface_prior`/`predictor` are registered but refused `PolicyInvalid{variant_not_admitted}` at bind (`crates/hh-control/src/compute.rs`); the Stage-4 `hot_path` ceiling measurement (OQ-075's M19 `boundary_overhead_ms` method) is preserved verbatim as the re-entry trigger for predictor placement | R4.15a |
| OQ-430 | needs-code-leg(R4.15c) | proposed ruling: the ADR-0192 G-4 touched-handles default — but it stands unimplemented (`MergePolicy`/`validator_selected`/`MergeReport::blocks_completion` landed, no touched-handle selection — `crates/hh-subagent/src/merge.rs`); the leg: the merge record carries the evidence handles each conflict touched, the default pass selects only validators whose criteria's handles intersect the touched set (the full contract stays opt-in), and `MergeReport` surfaces the selected set + touched handles so the completion gate consults only the selected verdicts | R4.15c |
| OQ-432 | adopt-interim-as-final | the interim's ledgered reason is the landed basis: every resolution lands a `control.merge.resolved` row carrying an explicit `resolver` member (the parent for `parent_decides`, the `validator_ref` for `validator_selected`) and `supersede{new_ref}` is a parent `delegate`-class effect (`crates/hh-subagent/src/merge.rs`); no recency tie-break (ADR-0082) — the stronger-basis arm rejected: the ledgered reason suffices and a validator verdict is a chosen resolver, never a required extra basis | R4.15a |
| OQ-433 | resolve-with-evidence | the envelope placement the interim left open is landed: `hlc` is a top-level optional member of `Event`/`EventEnvelope` (`crates/hh-ledger/src/event.rs`; `hlc.rs` the clock) and `require_hlc` is mandatory once a `share` child exists, optional otherwise (`crates/hh-subagent/src/spawn.rs`, `crates/hh-ledger/src/store.rs` — the ADR-0193 K-2 rule verbatim) — only the WS-B1/B3 confirmation act was owed | R4.15a |
| OQ-434 | adopt-interim-as-final | the interim verbatim is the landed table: the closed `ChildOutcome{result, cancelled, infrastructure_failure}` sum lifts hosted `scored ↦ result` and terminal values through `lift_stop_reason`/adapter facts (`crates/hh-subagent/src/types.rs`, `crates/hh-hosting/src/proj.rs`, `adapter_a.rs`); `refused_at_spawn` is no `HostedEvent` terminal and has no landed spelling — not observable through `hh-hosting/1`, the outcome stays `unknown` by construction | R4.15a |
| OQ-435 | adopt-interim-as-final | `min_flip_share` is declared MUST-data enforced as a floor (`share_ppm < min_flip_share_ppm` refuses — `crates/hh-evolution/src/campaign.rs`); replicates and the pass^k threshold are pre-registered campaign data; the 0.5 default stands as the launch value — the 6a re-pin and the `disagreement_weighted` screen-substitution ruling are preserved verbatim as re-entry triggers | R4.15a |
| OQ-436 | adopt-interim-as-final | `held_out_eval` is a landed campaign stage enforcing the pre-registered retention margins and veto checks (`crates/hh-evolution/src/campaign.rs`); "tasks the base passed" = the pre-registered pass^k threshold (the majority arm rejected); the `retention` margin value's 6a re-pin is preserved verbatim — joints OQ-113/OQ-301 stay named | R4.15a |
| OQ-438 | adopt-interim-as-final | the interim's arm landed: `rebase` (ADR-0195 D12) replays the same edit onto the moved head — semantic-op targets must match, `apply(target, invert(diff)) ≠ head` refuses, and `rebased → validated` re-runs the S1 gates (`crates/hh-evolution/src/campaign.rs`); GumTree-class matching (OQ-061) is not assumed — the S5 re-entry-cost measurement and `CompiledPayload`-body sufficiency preserved verbatim | R4.15a |
| OQ-441 | adopt-interim-as-final | the CF-489-conformed `DebtPolicy` values are this row's proposals verbatim — 60 d `min_runway` (`InsufficientRunway`), 30 d `warn_within`, 90 d `hypothesized_max_age` (the "one experiment cycle" arm is the ledgered probation `due_at_ms`, DF-S5.4-1), 30 d `grace_period` (`grace_elapsed` ∨ bound), 180 d `evidence_max_age` — joint with OQ-304: instrument-produced evidence only, Tier-A `source` refs never age by themselves; `DebtIndex`/`DebtReport` read the policy at runtime so the re-pin is a data amendment — the Stage-5 `DebtIndex` re-pin preserved verbatim | R4.15a |
| OQ-446 | resolve-with-evidence | the landed `OwnerRef{kind: principal|team, id, reach_via[]}` sum answers the kind disjunction — both spellings are ref members and no `TeamRef` record kind was minted (`crates/hh-ontology/src/debt.rs`); "reachable by a declared sink" = `reach_via` ∩ the caller-declared sink set (policy `notice_sinks`) ≠ ∅, enforced by `OwnerUnreachable` (`crates/hh-hir/src/debt.rs`); unrouted notices surface `unrouted`, never dropped (`route_notices`) | R4.15a |
| OQ-448 | adopt-interim-as-final | the OQ-323 placeholders stand landed as MUST-data constants (`K_REEXECUTED = 8`, `K_DETERMINISTIC = 4`, `K_CRN_SEQUENCE = 1`, `K_CRN_OUTCOME = 4`, `TARGET_CAP = 8`; `n_permutations`/`samples_per_eval` declared per design — `crates/hh-analysis/src/attribution.rs`) — ratified as launch constants; the A13 `power` re-check on the Stage-3 suite at n = 5 replicates is preserved verbatim as the re-entry trigger | R4.15a |
| OQ-450 | adopt-interim-as-final | the interim's "campaign data" is ratified: the landed surface offers refuse — `TooManyTargets` at `TARGET_CAP = 8`, `estimate_rollouts` declared, I4 `budget_reserved` reserve-before-spend (`crates/hh-analysis/src/attribution.rs`); no automatic M3→M2 degrade path exists, so `refuse` is the only admissible campaign declaration at C1; the §05e pricing rule (the OQ-364 ceiling) is preserved verbatim — a degrade arm, if ever ruled, is its own leg | R4.15a |
| OQ-453 | adopt-interim-as-final | the interim verbatim is landed: `observation_substitution` is the one hosted admissible form — `configuration` granularity under `model_io` interception only, `provisional` per ADR-0200 V9, component-level fields `n/a{class}` (the V9 design gate — `crates/hh-analysis/src/attribution.rs`; the hosted exception rows — `crates/hh-analysis/src/kernel.rs`); the KA-I7 admissibility ruling on intercepted participants preserved verbatim | R4.15a |
| OQ-457 | adopt-interim-as-final | the admissibility enforcement is landed (`guard_at_bind`: `ExploratoryOnly` lifted only by the `exploratory` declaration or `allow_unverified_snapshot`; `verified`/`trained_under` proceed, `drifted`/`unknown` annotate, `broken` refuses `CompatibilityBroken` — `crates/hh-lab/src/coevolution.rs`); `ConsolidationPolicy{k_cycles = 1, economics_factor = 1.0}` placeholders ratified — the 6d pre-registration re-pin and the `Design.kind` admissibility list preserved verbatim | R4.15a |
| OQ-458 | adopt-interim-as-final | ADR-0203 D1's family default is ratified: `scope.snapshots` defaults to the bound profile's selector family, not only the `evidence_refs`-named snapshots; the landed guard refuses `MissingSnapshotScope` on a model-conditioned/evolution-origin rule with no `scope.model_selectors` and requires the `model_version_change` expiry (AC-R-2.9.8-12 — `crates/hh-hir/src/debt.rs`) | R4.15a |

## Part 2 — per-OQ basis (ADR-0214 interim quoted; landed evidence re-checked)

Same convention as part 1: each row quotes the ratified interim rule (ADR-0214's
Decision table); the landed shape is the interim already in force or the evidence
that answers it; the outstanding workstream legs fold into the adoption — the WS-*
venues no longer run, so GATE-G7's act *is* the ratification the entry tests
awaited. Every ADR-0214 entry test stays preserved verbatim as a re-entry trigger;
R4.15a lands the adopted rulings as register/ADR amendment-log appends.

- **OQ-418 — child `attendance` inheritance + the interactive→unattended fork.**
  Interim (quoted): "a child inherits the parent's `attendance`; an `interactive`
  parent may not spawn an `unattended` child without a sealed pre-authorization
  (ADR-0185; ADR-0052 check 7; ADR-0208 §D.5)". Landed: the child manifest carries
  `attendance: parent_manifest.attendance` unconditionally at `finish_child_open`
  (`crates/hh-subagent/src/spawn.rs`) — no spawn input selects a different value,
  so the interactive→unattended spawn the interim guarded against cannot be
  constructed at all; Π-12's unattended transform (`ask` → `deny`/`defer`/
  `auto_review` — `crates/hh-monitor/src/decision.rs`, `monitor.rs`) applies to
  the child's asks under its inherited mode. The final answer: inheritance is
  unconditional — stronger than the interim (the sealed pre-authorization arm is
  moot while no selection exists; if attendance ever becomes selectable the
  pre-authorization question re-opens by amendment); the approvals-slice
  interaction resolves to Π-12's landed unattended column.
- **OQ-420 — `bandit` estimator defaults.** Interim (quoted): "`bandit`
  window/discount/`min_n` unset; a cell stays `unknown` below `min_n`
  (ADR-0189)". Landed: `RoutingPolicyKind::Bandit` scores `reward_ppm + bonus` —
  `success_ppm − lambda_ppm·cost_millis/1000` plus `explore_ppm·√(N/n)` — over
  non-pooling `(task_class, model_ref)` `BanditCell`s; below `min_n` the cell is
  `unknown` and scores the declared `cold_start_ppm` prior (ADR-0312 d.1;
  `crates/hh-gateway/src/router.rs`); the window/discount shape lives in the
  `BanditView` projection, not the policy — the view supplies the aggregates.
  The final answer: the declared-param form + `unknown`-below-`min_n` +
  cold-start prior + non-pooling task-class cells is the ratified posture
  (per-family vs global resolves to per-`task_class` cells, never global
  pooling); the measured re-pin on `lab/value-of-compute-v1`'s `bandit` arm —
  window-vs-discount and drift-reset grace — is preserved verbatim as the
  re-entry trigger.
- **OQ-424 — `hot_path` ceiling + predictor placement.** Interim (quoted):
  "`bind` runs in process for every in-loop estimator; `predictor` variants may
  not run out of process until the ceiling is measured (ADR-0188; ADR-0181 D8;
  joins OQ-075/407)". Landed: `bind` is pure over `(decision, ctx)` in process;
  `bandit`/`surface_prior`/`predictor` are registered variants refused
  `PolicyInvalid{variant_not_admitted}` at bind-time (`crates/hh-control/src/
  compute.rs`); the M19 `boundary_overhead_ms` method landed at S3.12 (OQ-075)
  but no bind-loop ceiling was measured. The final answer: the in-process-only
  posture is the ratified C1 rule — every admitted estimator is kernel-side and
  out-of-process `predictor` variants stay unadmitted; the Stage-4 ceiling
  measurement is preserved verbatim as the re-entry trigger that could admit a
  `predictor` arm by amendment.
- **OQ-430 — G-4 validator selection on the merged state.** Interim (quoted):
  "G-4 runs only the validators whose evidence handles the merge touched
  (ADR-0192 default)". Landed: the merge machinery carries `MergePolicy`
  (`parent_decides`/`validator_selected`/…), the `validator_selected` port with
  an accountable `validator_ref`, `MergeReport::blocks_completion` and
  `control.merge.resolved` rows (`crates/hh-subagent/src/merge.rs`) — but no
  landed code records which evidence handles a merge touched, selects validators
  by that set, or teaches the completion gate the touched set. The final answer
  needs the leg, not just the ruling: `needs-code-leg(R4.15c)` — (a) the merge
  record carries the evidence-handle set each conflict touched, (b) the default
  G-4 pass selects only the validators whose criteria's evidence handles
  intersect the touched set (the full-contract pass stays an opt-in
  `MergePolicy` arm), and (c) `MergeReport` surfaces the selected validator set
  + touched handles so `blocks_completion` consults only the selected verdicts.
- **OQ-432 — `supersede` basis.** Interim (quoted): "a parent's equal-authority
  `supersede` over a sibling's `delegate` memory version needs a ledgered
  reason; no recency tie-break (ADR-0082)". Landed: `resolve_conflict` lands
  every resolution as a `control.merge.resolved` row carrying an explicit
  `resolver` member (the parent for `parent_decides`, the `validator_ref` for
  `validator_selected`) and `supersede{new_ref}` is a parent `delegate`-class
  effect (`crates/hh-subagent/src/merge.rs`). The final answer: the ledgered
  reason is the ratified basis — no stronger basis (a reconciliation record or
  a mandatory validator verdict) is required; where a verdict *is* the basis it
  rides the `validator_selected` arm as the named resolver — the basis's
  strength is its ledgered accountability, not a second artifact.
- **OQ-433 — HLC placement (closes OQ-081 for delegation-bearing runs).**
  Interim (quoted): "HLC mandatory in `share` mode, optional otherwise
  (ADR-0193 K-2); envelope placement open". Landed: `hlc` is a top-level
  optional member of `Event`/`EventEnvelope` (`crates/hh-ledger/src/event.rs`),
  `hlc.rs` implements the clock, and `require_hlc` is mandatory once a `share`
  child exists — invoked at `share`-child spawn — and optional otherwise
  (`crates/hh-subagent/src/spawn.rs`, `crates/hh-ledger/src/store.rs`). The
  final answer: the open placement leg resolves to a top-level optional
  envelope member; the K-2 mandatory-in-`share` rule is landed verbatim — only
  the WS-B1/B3 confirmation act remained, which GATE-G7's adoption performs.
- **OQ-434 — hosted `ChildOutcome` lifting.** Interim (quoted): "hosted
  `ChildOutcome` lifts only the values `HostedEvent` terminals carry; the rest
  is `unknown` (ADR-0187 C-10; ADR-0164)". Landed: `ChildOutcome` is the closed
  sum `{result, cancelled, infrastructure_failure}` with hosted `scored ↦
  result` (`crates/hh-subagent/src/types.rs`) and terminal values lift through
  `lift_stop_reason`/adapter facts (`crates/hh-hosting/src/proj.rs`,
  `adapter_a.rs`); `refused_at_spawn` has no landed spelling anywhere in
  `hh-hosting` — a refused spawn never produces a `HostedEvent` terminal. The
  final answer: the lifting table is the landed closed sum; `refused_at_spawn`
  is not observable through `hh-hosting/1` — the outcome stays `unknown` by
  construction (the refusal surfaces on the adapter's own error channel, never
  as an outcome value).
- **OQ-435 — S3 screen defaults.** Interim (quoted, the shared OQ-435/436 row):
  "S3 `min_flip_share = 0.5`; S5 `retention` margin pre-registered per campaign;
  'tasks the base passed' by the pre-registered pass^k threshold (ADR-0194
  S3/S5; joins OQ-113/OQ-301)". Landed: `min_flip_share_ppm` is a declared spec
  member enforced as a floor — `share_ppm < min_flip_share_ppm` refuses
  (`crates/hh-evolution/src/campaign.rs`); replicate counts are pre-registered
  (`min_replicates` enforced). The final answer: the 0.5 default stands as the
  ratified launch value over the declared floor + pre-registered replicates;
  the `disagreement_weighted` screen-substitution arm is not adopted — a screen
  substitution would weaken the hypothesis-named set without a measurement, so
  the 6a re-pin + the substitution ruling are preserved verbatim as re-entry
  triggers.
- **OQ-436 — S5 `retention` margin + "tasks the base passed".** Interim
  (quoted, the shared row): as OQ-435. Landed: `held_out_eval` is a landed
  campaign stage enforcing the pre-registered `retention` margins and veto
  checks (`crates/hh-evolution/src/campaign.rs`); observational hypotheses
  cannot pass S5. The final answer: "tasks the base passed" = the
  pre-registered pass^k threshold per campaign (the majority arm rejected); the
  margin *value* re-pins from 6a data — preserved verbatim; joints
  OQ-113/OQ-301 stay named as the margin's siblings.
- **OQ-438 — `rebase` at Stage 6c.** Interim (quoted): "code-search candidates
  re-propose against the moved head (ADR-0195 D12); GumTree-class matching
  (OQ-061) not assumed". Landed: `rebase` landed at S6.3a (ADR-0195 D12) — a
  moved lineage head rebases a cleared candidate by replaying the same edit:
  semantic-op targets must match, `apply(target, invert(diff)) ≠ head` refuses,
  and the `rebased → validated` transition re-runs the S1 gates and deposits
  the rebased proposal doc (`crates/hh-evolution/src/campaign.rs`). The final
  answer: the re-propose arm is the landed mechanism — rebase replays the
  identical diff, so `CompiledPayload` bodies need no GumTree-class matching at
  C1 (matching is not assumed, per the interim); the S5 re-entry-cost
  measurement for parallel campaigns is preserved verbatim as the re-entry
  trigger.
- **OQ-441 — `DebtPolicy` defaults + the Stage-5 `DebtIndex` re-pin.** Interim
  (quoted): "`DebtPolicy` proposed defaults (runway 60 d, probation one cycle
  or 90 d, warn 30 d, grace one cycle) are placeholders (ADR-0197)". See the
  joint OQ-441/OQ-304 section below — proposed `adopt-interim-as-final`: the
  CF-489-conformed values are the ratified launch defaults; the Stage-5
  `DebtIndex`-data re-pin is preserved verbatim as the re-entry trigger.
- **OQ-446 — debt-owner kind + "reachable by a declared sink".** Interim
  (quoted): "debt-record owners are `PrincipalRef`s; 'reachable by a declared
  sink' means a first-party surface or the audit sink (ADR-0197)". Landed:
  `OwnerRef` is the sum `{kind: principal|team, id, reach_via[]}` — both
  spellings admitted as ref members and no separate `TeamRef` record kind
  minted (`crates/hh-ontology/src/debt.rs`); `OwnerUnreachable` refuses a
  record whose `reach_via` intersects none of the caller-declared sink set —
  the policy's `notice_sinks` — and unrouted notices surface `unrouted`, never
  dropped (`crates/hh-hir/src/debt.rs`, `route_notices` in
  `crates/hh-lab/src/debt.rs`). The final answer: the kind disjunction resolves
  to the landed `OwnerRef` sum — owner identity is a ref spelling
  (`principal{id}` or `team{id}`), not a new record kind; "reachable" = the
  record's `reach_via` ∩ the declared `notice_sinks` ≠ ∅ — the "first-party
  surface or the audit sink" enumeration is *data* (which sink ids the policy
  declares), not a mechanism; the WS-L8 D-1 join is covered by R4.10's
  `descope-by-ADR` proposal on D-1 — its disposition is GATE-G7's, not this
  package's (no `PrincipalDirectory` class contract was ever proposed;
  owner-issued `ApproverGrant`s + declared sinks are the landed mechanism).
- **OQ-448 — attribution-constant recalibration.** Interim (quoted):
  "attribution constants (`k` 8/4/1, `target_cap` 8, `n_permutations`,
  `samples_per_eval`, KA-I7-7 pass rate 80 %) are the OQ-323 placeholders until
  A13 `power` recalibrates them (ADR-0199 D6; ADR-0201)". Landed:
  `K_REEXECUTED = 8`, `K_DETERMINISTIC = 4`, `K_CRN_SEQUENCE = 1`,
  `K_CRN_OUTCOME = 4`, `TARGET_CAP = 8` are MUST-data constants;
  `n_permutations`/`samples_per_eval` are declared per design with an
  `Underpowered` floor (`crates/hh-analysis/src/attribution.rs`). The final
  answer: the placeholders are the ratified launch constants; the A13 `power`
  run on the Stage-3 suite at n = 5 replicates is preserved verbatim as the
  re-entry trigger — the constants re-pin by amendment once that measurement
  exists.
- **OQ-450 — coalition `target_cap` + the refuse-vs-degrade pricing rule.**
  Interim (quoted): "refuse-vs-degrade for Shapley walks above the
  per-experiment ceiling is campaign data (ADR-0200 D3 `estimate_rollouts`;
  OQ-364 ceiling)". Landed: `TARGET_CAP = 8` refuses `TooManyTargets` at design
  validation, `estimate_rollouts` is a declared member, and `budget_reserved`
  enforces I4 reserve-before-spend (`crates/hh-analysis/src/attribution.rs`) —
  the code itself marks the cap "unruled until the §05e pricing rule lands";
  no automatic M3→M2 degrade path exists anywhere. The final answer: the
  rule *is* campaign data — and the landed surface offers only the refuse arm,
  so `refuse` is the sole admissible campaign declaration at C1 (`degrade`
  would name a mechanism that does not exist); the §05e pricing rule at the
  OQ-364 ceiling is preserved verbatim — if a degrade arm is ever ruled it
  lands by its own leg.
- **OQ-453 — hosted `observation_substitution` admissibility.** Interim
  (quoted): "`observation_substitution` at a hosted interception point is
  admissible only at configuration granularity with `n/a{class}` on
  component-level fields, and is `provisional` (ADR-0200 V9)". Landed: the V9
  gate refuses every hosted target except `observation_substitution` at
  `configuration` granularity under `interception: model_io`
  (`crates/hh-analysis/src/attribution.rs` — `ClassInadmissible` otherwise);
  the hosted exception rows compute over hosted runs reporting
  `validity_mode: hosted_interception` and every non-exception hosted target
  stays `n/a{class}` (`crates/hh-analysis/src/kernel.rs`). The final answer:
  the provisional configuration-granularity admission is the ratified launch
  posture — `observation_substitution` admissible at `configuration` under
  `model_io`, component-level fields `n/a{class}`; the KA-I7 admissibility
  ruling on intercepted participants is preserved verbatim as the re-entry
  trigger that could lift `provisional`.
- **OQ-457 — consolidation defaults + `allow_unverified_snapshot`.** Interim
  (quoted, the shared OQ-457/458 row): "`k_cycles`, the economics factor and
  non-inferiority margins are pre-registered per cycle; `scope.snapshots`
  defaults to the bound profile's selector family (ADR-0203 D1; ADR-0204)".
  Landed: `guard_at_bind` enforces the admissibility — an evolution-origin arm
  with no `CompatibilityRecord` refuses `ExploratoryOnly` unless the experiment
  declares `exploratory` or the caller passes `allow_unverified_snapshot`;
  `verified`/`trained_under` proceed, `drifted`/`unknown` annotate, `broken`
  refuses `CompatibilityBroken` (`crates/hh-lab/src/coevolution.rs`);
  `ConsolidationPolicy{k_cycles = 1, economics_factor = 1.0}` carries the
  placeholder defaults. The final answer: the pre-registered placeholders are
  the ratified launch values and `allow_unverified_snapshot` stays a
  caller-flagged escape, never a `Design.kind` default — the `Design.kind`
  admissibility list and the 6d pre-registration re-pin are preserved verbatim
  as re-entry triggers.
- **OQ-458 — `scope.snapshots` default.** Interim (quoted, the shared row):
  "`scope.snapshots` defaults to the bound profile's selector family
  (ADR-0203 D1)". Landed: a model-conditioned/evolution-origin rule with no
  `scope.model_selectors` refuses `MissingSnapshotScope` and the
  `model_version_change` expiry is required (AC-R-2.9.8-12 —
  `crates/hh-hir/src/debt.rs`); ADR-0203 D1's family default remains the
  operative rule. The final answer: the bound profile's selector family is the
  ratified default — not only the `evidence_refs`-named snapshots (the
  family-level scope is what the `model_version_change` expiry needs to
  observe); the evidence-named-only arm is rejected as under-inclusive.

## OQ-441 — the Stage-5 `DebtIndex` re-pin (joint with OQ-304, WS-I6)

OQ-441 asked the defaults for `DebtPolicy.min_runway`,
`hypothesized_max_age` (probation), `warn_within`, `grace_period` — proposed:
runway 60 d, probation one experiment cycle or 90 d, warn 30 d, grace one
experiment boundary — joint with OQ-304's `evidence_max_age`. The interim rule
(quoted): "`DebtPolicy` proposed defaults (runway 60 d, probation one cycle or
90 d, warn 30 d, grace one cycle) are placeholders (ADR-0197)"; the entry test:
"WS-I6 re-pins from the Stage-5 `DebtIndex` data; joins OQ-445 (ADR-0210) on
`evidence_max_age`".

**Post-CF-489 evaluation — the shapes now match the proposals.** CF-489 (the
R2.24-registered divergence: landed 30 d/14 d vs proposed 60 d/30 d) resolved at
R3.9 on the operator's direction ruling — the landed `DebtPolicy` placeholders
conformed to this row's proposed values (`crates/hh-ontology/src/debt.rs` —
the dated ruling comment stands at the struct). The landed defaults, all
enforced: `min_runway_ms` 60 d (`InsufficientRunway` when `until − created_at`
is below it — `crates/hh-hir/src/debt.rs`); `warn_within_ms` 30 d (the
`expiring` window — `DebtReport`/`expiring_within` and the
`debt.expiring_within_30d` health metric); `hypothesized_max_age_ms` 90 d (the
probation bound — the "one experiment cycle" arm is the *ledgered* probation
row the sweep opens at `due_at = now + hypothesized_max_age`, DF-S5.4-1's book
of record, with the record-derived `created_at + 90 d` fallback —
`crates/hh-debt/src/manager.rs`, `crates/hh-lab/src/debt.rs`);
`grace_period_ms` 30 d (the "one experiment boundary" arm rides the
caller-flagged `grace_elapsed` observable ∨ the 30 d bound on the
profile-change leg); `evidence_max_age_ms` 180 d; `max_open_removal_tests` 64;
`priority "expiry_urgency"`; `schedule "daily"`.

**The `DebtIndex` shape.** `debt_index(entries) → DebtIndexRow[]` is the landed
fold over the registered `AssumptionDebtRecord`s (`crates/hh-lab/src/debt.rs` —
`staleness_reasons[]`, `expired_used_runs`, `evidence_grade`, the
`removal_test{executable, last_report_ref?, last_verdict?}` state);
`evaluate_debt` emits the `expiring`/`expired`/`revalidated` transitions with
`causes[]`; `DebtReport`/`DebtNotice` read `warn_within_ms`,
`hypothesized_max_age_ms`, `grace_period_ms` and `evidence_max_age_ms` from the
policy *at runtime* (`crates/hh-debt/src/manager.rs` — `report` passes
`policy.warn_within_ms`; `route_notices` routes only to declared
`notice_sinks`, unrouted surfaced as `unrouted`). **The re-pin is a data
amendment, never a code leg** — every constant OQ-441 names is a `DebtPolicy`
member the running index reads.

**Joint with OQ-304.** OQ-304 resolved at Phase 4 (ADR-0197 D5):
`evidence_max_age` ages only instrument-produced evidence; Tier-A `source`
refs never age out by themselves; the default value is OQ-441's. The landed
pair honours that division: the policy's `evidence_max_age_ms` 180 d governs
only the `evidence_refresh_due` fallback leg (a record naming the kind without
`expiry.params.evidence_max_age_ms` reads the policy bound —
`evaluate_debt`'s §(5) leg), and the record-level
`expiry.params.evidence_max_age_ms` override wins when declared. Tier-A
`source` refs carry no aging leg anywhere in the evaluator. The entry test's
second join — OQ-445 (ADR-0210) on `evidence_max_age` — is R4.8's
`needs-code-leg(R4.15a)` proposal (`evidence_max_age` = one model generation at
the spec-debt register header): a different surface (the register's
revalidation cadence, not the `DebtPolicy` member), so the two dispositions
stand independently at GATE-G7 — no contradiction.

**Proposed disposition — `adopt-interim-as-final` (the final re-pin).** The
interim's proposed values are the landed, enforced values: the re-pin this row
awaited resolves to *adopting the conformed placeholders as the ratified
defaults* — 60 d `min_runway`, 30 d `warn_within`, 90 d
`hypothesized_max_age` (probation = ledgered `due_at_ms` ∨ the 90 d bound),
30 d `grace_period` (`grace_elapsed` ∨ the 30 d bound), 180 d
`evidence_max_age` (instrument-produced evidence only, per OQ-304). The
ADR-0214 entry test — WS-I6 re-pins from the Stage-5 `DebtIndex` data — is
preserved verbatim as the re-entry trigger: the `[[DEFERRED ADR-0214: OQ-441]]`
markers stand until the running index supplies real aging data (the fixture
lane supplies none) and the values re-pin by amendment. Conform row R4.15a.

## Conflicts (part 2)

None registered. CF-489 (the OQ-441 `DebtPolicy` placeholder divergence) was
resolved at R3.9 — the conform is recorded on the OQ-441 register row and in
the `debt.rs` comment; it is evidence for this package, not a new divergence.
No part-2 row's landed shape contradicts its interim text per the R2.24
revalidation cells. No new CF row.

## What part 2 does not decide

Identical to part 1's standing statement: no register status changes, no
amendment-log appends, no schema or spec conform land here — every cell above
is a proposal the operator adopts, amends or declines at GATE-G7 (the records
conform is R4.15a's; OQ-430's `needs-code-leg` lands at R4.15c). The interim
rules stay in force unchanged until then; every ADR-0214 entry test is
preserved verbatim as a re-entry trigger — including OQ-441's Stage-5
`DebtIndex` re-pin, which this proposal resolves as *adopt the conformed
defaults now, re-pin by amendment when the index data exists*. BL-37's flip
stays R4.16's accounting after the adopted set conforms.

> *Reconciled 2026-10-22 (GATE-G7 fleshing):* the `R4.15a` conform cells name the pre-split row — this package's records conform lands at **R4.15b** (164e) per the ADR-0214 package scoping; OQ-430's code leg at **R4.15c** (164h) as tabled.
