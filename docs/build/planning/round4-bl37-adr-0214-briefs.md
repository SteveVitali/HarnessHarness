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
