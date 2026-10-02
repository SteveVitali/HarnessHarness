//! The `hh-embed/1` operation registry — one entry per operation across
//! Groups H, S, W, R, M, L, U (§7.4 §2.4/§5, §9; ADR-0178 D3/D7,
//! ADR-0183). The registry is the *single source* for: the stability map
//! `hello` returns, the schema export's operation table, the dispatch
//! tables in both bindings, and the verb-parity conformance check —
//! CC6's "all bindings serve the same op set" is a property of this list.
//!
//! `implemented` marks the Stage-1 op set (§9). Ops declared at a later
//! stage ship their signatures now (CC1: clients bind at the schema; the
//! dialect never shrinks) and answer `Refused{stage_pending}` at runtime
//! — experimental-tier ops additionally require `capabilities.experimental`
//! (§7.4 §6, AC-R-2.11.4-8).

/// Call direction — `Call` is host→kernel, `Upcall` is kernel→host
/// (Group U; the four upcall signatures are declared for the closed
/// contract even though only `request_permission`/`notify_hook` have
/// Stage-1 emitters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Call,
    Upcall,
}

/// Stability tier (ADR-0178 D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Frozen within the major.
    Stable,
    /// May evolve additively; requires opt-in.
    Experimental,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Stable => "stable",
            Tier::Experimental => "experimental",
        }
    }
}

/// A staged assumption attached to an op — surfaces in the `hello`
/// stability map as `debt` (§7.4 §6, §5.3.7).
#[derive(Debug, Clone)]
pub struct OpDebt {
    pub assumption_id: &'static str,
    pub debt_id: &'static str,
    pub owner_layer: &'static str,
    pub stage_gate: &'static str,
    pub detail: &'static str,
}

/// `OpDeprecation` — the deprecation metadata a `stable` op carries for the
/// rest of its `contract_major` (§7.4 rule 5; ADR-0178 D5). A deprecated op
/// stays callable for the remainder of its major; every admitted use mints a
/// kernel-origin `lifecycle.contract.deprecated_use{method, client}` row the
/// store folds into per-`{method, client}` counts (the removal gate's
/// zero-uses evidence — removal only at the next major, only at zero uses).
#[derive(Debug, Clone, PartialEq)]
pub struct OpDeprecation {
    /// The minor the deprecation was declared at (`"1.3"` form).
    pub since: &'static str,
    /// The `contract_major` at which removal is admissible.
    pub removal_major: i64,
    /// The replacement method, when one exists.
    pub replacement: Option<&'static str>,
}

impl OpDeprecation {
    /// The canonical member form — `stability` map + schema export share it.
    pub fn to_json(&self) -> hh_wire::json::Json {
        hh_wire::json::Json::obj([
            ("since", hh_wire::json::Json::str(self.since)),
            (
                "removal_major",
                hh_wire::json::Json::Int(self.removal_major),
            ),
            (
                "replacement",
                match self.replacement {
                    Some(r) => hh_wire::json::Json::str(r),
                    None => hh_wire::json::Json::Null,
                },
            ),
        ])
    }
}

/// One registry row.
#[derive(Debug, Clone)]
pub struct OpSpec {
    /// The JSON-RPC method name (Group L/M names are dotted to keep
    /// their namespaces disjoint: `lab.registry.publish`, …).
    pub name: &'static str,
    /// `H | S | W | R | M | L | U`.
    pub group: &'static str,
    pub direction: Direction,
    /// Params type name in the schema export (`json` = opaque records).
    pub params: &'static str,
    /// Result type name (`json` = opaque records; `notification` =
    /// stream-side delivery for `stream_events`).
    pub result: &'static str,
    /// The declared error subset (variant tags) — must name members of
    /// the closed `EmbedError` sum (a test asserts it).
    pub errors: &'static [&'static str],
    pub tier: Tier,
    /// Served for real at this build.
    pub implemented: bool,
    /// Capability gate (a `HostCapabilities` member name) — call
    /// without it → `CapabilityNotDeclared{capability}`.
    pub requires_capability: Option<&'static str>,
    pub debt: Option<OpDebt>,
    /// Deprecation metadata (§7.4 rule 5) — `Some` marks a `stable` op
    /// deprecated for the rest of its major: still callable, every use
    /// is ledgered `lifecycle.contract.deprecated_use{method, client}`.
    /// Nothing in this contract is deprecated today — the member is the
    /// machinery, not a live claim.
    pub deprecated: Option<OpDeprecation>,
}

const SHAPE: &[&str] = &["UnknownField", "SchemaViolation"];
const SESS: &[&str] = &["UnknownField", "SchemaViolation", "UnknownSession"];
const STAGED: &[&str] = &[
    "UnknownField",
    "SchemaViolation",
    "ExperimentalRequired",
    "Refused",
];
const LAB: &[&str] = &[
    "UnknownField",
    "SchemaViolation",
    "ExperimentalRequired",
    "CapabilityNotDeclared",
    "Refused",
];
/// The `fleet.*` op set's declared errors (S4.9; §5i.1) — the closed
/// `EmbedError` members the C4 surface emits. `Unsupported{by}` covers
/// the `tier-c4`-absent build's typed refusal (`tier_unavailable`); every
/// engine `FleetError` maps onto `Refused{reason}`/`SchemaViolation`/
/// `UnknownRun`/`WouldBlock`/`InsufficientBudget`.
const FLEET_ERR: &[&str] = &[
    "UnknownField",
    "SchemaViolation",
    "ExperimentalRequired",
    "UnknownRun",
    "WouldBlock",
    "InsufficientBudget",
    "UnbudgetedArm",
    "Refused",
    "Unsupported",
];

/// The `lab.evolution.*` op set's declared errors (S6.1a; §05h R-2.9.5)
/// — the fleet set verbatim (the campaign engine surfaces the same
/// store/refusal shape; `Unsupported{by}` covers the `tier-c4`-absent
/// build).
const EVO_ERR: &[&str] = FLEET_ERR;

/// The `lab.debt.{register,sweep,settle,retire,propose,manager_open}` op
/// set's declared errors (S6.1b; §5h.6 R-2.9.6) — the fleet set verbatim
/// (the manager surfaces the same store/refusal shape; `Refused{reason}`
/// carries the closed `hh_debt::errors::Refusal` codes;
/// `Unsupported{by}` covers the `tier-c4`-absent build).
const DEBTMGR_ERR: &[&str] = FLEET_ERR;

const fn call(
    name: &'static str,
    group: &'static str,
    params: &'static str,
    result: &'static str,
    errors: &'static [&'static str],
    tier: Tier,
    implemented: bool,
) -> OpSpec {
    OpSpec {
        name,
        group,
        direction: Direction::Call,
        params,
        result,
        errors,
        tier,
        implemented,
        requires_capability: None,
        debt: None,
        deprecated: None,
    }
}

const fn staged_exp(
    name: &'static str,
    group: &'static str,
    params: &'static str,
    result: &'static str,
) -> OpSpec {
    OpSpec {
        tier: Tier::Experimental,
        implemented: false,
        ..call(
            name,
            group,
            params,
            result,
            STAGED,
            Tier::Experimental,
            false,
        )
    }
}

const fn lab(
    name: &'static str,
    group: &'static str,
    params: &'static str,
    result: &'static str,
) -> OpSpec {
    OpSpec {
        requires_capability: Some("serves_measurement"),
        ..call(name, group, params, result, LAB, Tier::Experimental, false)
    }
}

/// A Group L op whose dispatch is live in `hh-embed` (S2.12) — `import`/`export`
/// stay `lab(...)` (StagePending) until their slice lands.
const fn labi(
    name: &'static str,
    group: &'static str,
    params: &'static str,
    result: &'static str,
) -> OpSpec {
    OpSpec {
        requires_capability: Some("serves_measurement"),
        ..call(name, group, params, result, LAB, Tier::Experimental, true)
    }
}

const fn upcall(name: &'static str, params: &'static str, result: &'static str) -> OpSpec {
    OpSpec {
        direction: Direction::Upcall,
        ..call(name, "U", params, result, &[], Tier::Stable, false)
    }
}

/// The full operation table — Group order: H, S, W, R, M, L, U.
pub fn registry() -> Vec<OpSpec> {
    let mut v: Vec<OpSpec> = vec![
        // ── Group H — handshake ───────────────────────────────────────
        OpSpec {
            errors: &[
                "ContractMajorUnsupported",
                "SchemaMismatch",
                "KernelBelowFloor",
                "UnknownField",
                "SchemaViolation",
            ],
            ..call(
                "hello",
                "H",
                "HelloParams",
                "HelloResult",
                SHAPE,
                Tier::Stable,
                true,
            )
        },
        // ── Group S — session ─────────────────────────────────────────
        call(
            "open_session",
            "S",
            "OpenSessionParams",
            "Session",
            &[
                "UnknownField",
                "SchemaViolation",
                "SecretInPayload",
                "UnknownRun",
                "InvalidDefinition",
                "UnresolvedRef",
                "AmbiguousVersion",
                "AuthorityViolation",
                "UnexpressibleSurface",
                "LinkError",
                "InsufficientBudget",
                "UnbudgetedArm",
                "UnattendedRequiresInput",
                "WouldBlock",
                "DefinitionChanged",
                "EnvironmentUnavailable",
            ],
            Tier::Stable,
            true,
        ),
        call(
            "close",
            "S",
            "CloseParams",
            "Closed",
            SESS,
            Tier::Stable,
            true,
        ),
        // ── Group W — work ────────────────────────────────────────────
        call(
            "submit",
            "W",
            "SubmitParams",
            "Accepted",
            &[
                "UnknownField",
                "SchemaViolation",
                "SecretInPayload",
                "UnknownSession",
                "TurnActive",
                "Draining",
                "SessionDetached",
                "UnbudgetedArm",
                "Refused",
                "CapabilityNotDeclared",
            ],
            Tier::Stable,
            true,
        ),
        call(
            "cancel",
            "W",
            "CancelParams",
            "Acknowledged",
            &[
                "UnknownField",
                "SchemaViolation",
                "UnknownSession",
                "TurnMismatch",
                "SessionDetached",
                "CapabilityNotDeclared",
            ],
            Tier::Stable,
            true,
        ),
        OpSpec {
            requires_capability: Some("serves_permission_channel"),
            debt: Some(OpDebt {
                assumption_id: "A-embed-permission-bridge",
                debt_id: "D-embed-permission-resume",
                owner_layer: "hh-embed",
                stage_gate: "S2.6",
                detail: "the human decision is recorded (pending → decided); \
                         the blocked dispatch is not retried — the live \
                         approval round trip lands at S2.6 (OQ-246).",
            }),
            ..call(
                "respond_permission",
                "W",
                "RespondPermissionParams",
                "Recorded",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "UnknownSession",
                    "UnknownPermission",
                    "AlreadyDecided",
                    "OptionNotOffered",
                    "CapabilityNotDeclared",
                    "SessionDetached",
                ],
                Tier::Stable,
                true,
            )
        },
        // `steer` is implemented — the react/minimal strategy declines it
        // honestly (`Unsupported{by:"control_strategy"}`; the steering
        // mode is declared `unsupported` at Stage 1).
        call(
            "steer",
            "W",
            "SteerParams",
            "Accepted",
            &[
                "UnknownField",
                "SchemaViolation",
                "UnknownSession",
                "TurnMismatch",
                "Unsupported",
                "Draining",
            ],
            Tier::Stable,
            true,
        ),
        // `fork` is implemented — the S2.9 branch model: coherent cuts
        // (`ForkPointNotCoherent`/`coerce_to_boundary`), `env ∈
        // {snapshot, trace_only, none}` (`shared_live` is the typed
        // refusal), `lifecycle.run.forked` + source-prefix pinning on the
        // child (ADR-0271).
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "fork",
                "W",
                "ForkParams",
                "Session",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "EnvironmentUnavailable",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `navigate` is implemented (S2.9; ADR-0271) — the HEAD move lands
        // `lifecycle.head.moved` and subscribers receive `rewind`.
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "navigate",
                "W",
                "NavigateParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `respond_elicitation` is implemented — no elicitation is open
        // at Stage 1 (the scripted model never elicits), so the honest
        // surface is `Refused{no_open_elicitation}`.
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "respond_elicitation",
                "W",
                "RespondElicitationParams",
                "Recorded",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `report_host_effect` is implemented — the host reports the
        // terminal for an `invoke_host_capability` ask; the row lands
        // durable (`executor_class = host_process`).
        OpSpec {
            tier: Tier::Experimental,
            requires_capability: Some("serves_host_executor"),
            ..call(
                "report_host_effect",
                "W",
                "ReportHostEffectParams",
                "Recorded",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "CapabilityNotDeclared",
                    "UnknownSession",
                    "UnknownEffect",
                    "Fenced",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `amend` — the ADR-0216 OQ-468 interim op is implemented for
        // the C0 exhaustion path: `target = budget` mints
        // `control.budget.amended` + lifts the driver's ceiling + wakes
        // the parked loop (`amend{attendance|approval_mode}` answers the
        // honest `Refused{stage_pending}` at Stage 1 — DF-S1.26-*).
        OpSpec {
            tier: Tier::Experimental,
            implemented: true,
            ..call(
                "amend",
                "W",
                "AmendParams",
                "Session",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "AuthorityWideningRequiresHuman",
                    "Draining",
                    "SessionDetached",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `set_coordinate` is implemented (S4.12; §7.4; ADR-0177 D10;
        // AC-R-2.11.4-14): `model` re-lowers the bound profile
        // (`relower` + `model.surface.relowered` before the coordinate
        // applies); a name outside the run's coordinate space is
        // `UnknownCoordinate` — never silently ignored (T-13).
        OpSpec {
            tier: Tier::Experimental,
            implemented: true,
            ..call(
                "set_coordinate",
                "W",
                "SetCoordinateParams",
                "Recorded",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownCoordinate",
                    "UnresolvedRef",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `coherent_fork_points` is implemented (S2.9) — the pure
        // coherence projection over the run's WAL (AC-R-2.2.4-1).
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "coherent_fork_points",
                "R",
                "CoherentForkPointsParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // S4.13 (C2) — `branch.open` opens an *intra-run* branch under the
        // session's writer lease (`lifecycle.branch.opened`; coherent fork
        // point, `SpeculationPolicy` verbatim, containment at `fork`;
        // ADR-0133/0134). `promote`/`discard` close it.
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "branch.open",
                "W",
                "BranchOpenParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "AuthorityViolation",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "discard",
                "W",
                "DiscardParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Fenced",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "promote",
                "W",
                "PromoteParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Fenced",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // S4.13 (§5a.3/§5a.4) — the goal continuation chain + inbox runs +
        // the wakeup `subscribe`/`record_occurrence` pair
        // (`schedule`/`external`/`peer_message` triggers;
        // `AlreadyContinued`/`GoalFinished`/`TriggerUnsupported` surface
        // `Refused{reason}`).
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "continue_goal",
                "W",
                "ContinueGoalParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "open_inbox",
                "W",
                "OpenInboxParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "subscribe",
                "W",
                "SubscribeParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "record_occurrence",
                "W",
                "RecordOccurrenceParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        // `rollback` is implemented (S2.9; ADR-0271) — coherence gate,
        // scoped compensation saga, rewind-note blob, `rolled_back` +
        // `head.moved` rows, `rewind` frame; returns the `RollbackRecord`.
        OpSpec {
            tier: Tier::Experimental,
            ..call(
                "rollback",
                "W",
                "RollbackParams",
                "json",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "Refused",
                    "EnvironmentUnavailable",
                ],
                Tier::Experimental,
                true,
            )
        },
        // ── S3.6: `replay`/`counterfactual` land implemented
        // (R-2.2.4⁰ᵇ; §5a.4) — the staged shape's `from_seq`/`edits`
        // placeholders are replaced by the contract's params.
        OpSpec {
            implemented: true,
            ..call(
                "replay",
                "W",
                "ReplayParams",
                "ReplayResult",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "EnvironmentUnavailable",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "counterfactual",
                "W",
                "CounterfactualParams",
                "CounterfactualResult",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "ExperimentalRequired",
                    "UnknownSession",
                    "UnknownRun",
                    "UnbudgetedArm",
                    "EnvironmentUnavailable",
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        staged_exp("archive", "S", "ArchiveParams", "Acknowledged"),
        staged_exp("grant", "S", "GrantParams", "Recorded"),
        staged_exp("revoke_lease", "S", "RevokeLeaseParams", "Acknowledged"),
        // ── Group R — read ────────────────────────────────────────────
        call(
            "stream_events",
            "R",
            "StreamEventsParams",
            "StreamTicket",
            SESS,
            Tier::Stable,
            true,
        ),
        call("read", "R", "ReadParams", "Page", SESS, Tier::Stable, true),
        call("head", "R", "HeadParams", "Head", SESS, Tier::Stable, true),
        call(
            "project",
            "R",
            "ProjectParams",
            "View",
            SESS,
            Tier::Stable,
            true,
        ),
        call(
            "account",
            "R",
            "AccountParams",
            "AccountView",
            SESS,
            Tier::Stable,
            true,
        ),
        call(
            "describe",
            "R",
            "DescribeParams",
            "DescribeResult",
            SESS,
            Tier::Stable,
            true,
        ),
        OpSpec {
            tier: Tier::Experimental,
            requires_capability: None,
            ..call(
                "list_leases",
                "R",
                "ListLeasesParams",
                "ListLeasesResult",
                &[
                    "UnknownField",
                    "SchemaViolation",
                    "UnknownSession",
                    "ExperimentalRequired",
                ],
                Tier::Experimental,
                true,
            )
        },
        call(
            "lineage",
            "R",
            "LineageParams",
            "Page",
            &[
                "UnknownField",
                "SchemaViolation",
                "UnknownSession",
                "Refused",
            ],
            Tier::Stable,
            true,
        ),
        call(
            "get_artifact",
            "R",
            "GetArtifactParams",
            "json",
            &[
                "UnknownField",
                "SchemaViolation",
                "UnknownSession",
                "Refused",
            ],
            Tier::Stable,
            true,
        ),
        // S4.10 (R-2.11.2 V1; ADR-0301 D5) — the store-level run listing.
        // Session-free by design: a cross-run index is not a run read and
        // V1 must serve before any per-run attach exists (precedent:
        // `lab.results.*` Group-L reads are session-free).
        call(
            "run_index",
            "R",
            "RunIndexParams",
            "json",
            &["UnknownField", "SchemaViolation", "Refused"],
            Tier::Stable,
            true,
        ),
        // S2.5 (R-2.8.6): the audit surface is live — `audit_view` reads
        // the ledger (no second store), `verify` returns the `Tampered`
        // taxonomy, `prove_*` return RFC 6962-style proofs.
        call(
            "audit_view",
            "R",
            "AuditViewParams",
            "json",
            &[
                "UnknownField",
                "SchemaViolation",
                "UnknownSession",
                "AuthorityViolation",
                "Refused",
            ],
            Tier::Stable,
            true,
        ),
        call(
            "verify",
            "R",
            "VerifyParams",
            "json",
            SESS,
            Tier::Stable,
            true,
        ),
        call(
            "prove_inclusion",
            "R",
            "ProveInclusionParams",
            "json",
            SESS,
            Tier::Stable,
            true,
        ),
        call(
            "prove_consistency",
            "R",
            "ProveConsistencyParams",
            "json",
            SESS,
            Tier::Stable,
            true,
        ),
        staged_exp(
            "request_redaction",
            "R",
            "RequestRedactionParams",
            "Acknowledged",
        ),
        // ── Group M — measurement/kernel-direct (serves_measurement) ──
        // S3.1 (R-2.11.4⁰ᵇ; ADR-0176): the Lab's measurement channel —
        // `measurement.metric.emitted` rows the Lab appends are minted by
        // the kernel and are indistinguishable from any client's
        // (AC-R-2.11.4-10).
        labi("measurement.emit_metric", "M", "json", "json"),
        lab("kernel.compose", "M", "json", "json"),
        lab("kernel.resolve", "M", "json", "json"),
        lab("kernel.validate_assembly", "M", "json", "json"),
        lab("kernel.seal", "M", "json", "json"),
        lab("kernel.space", "M", "json", "json"),
        lab("kernel.enumerate", "M", "json", "json"),
        // S3.1 (R-2.9.3⁰; ADR-0139/0140/0141): the run-bundle verbs —
        // `bundle(kind = run)`, the staged `validate_bundle`/
        // `check_completeness` report, `reproduce` (R0/R1/R3 native),
        // and `import(ledger_native)` lifting a bundle back into the
        // store with `origin = import` provenance.
        labi("kernel.bundle", "M", "json", "json"),
        labi("kernel.check_completeness", "M", "json", "json"),
        labi("kernel.reproduce", "M", "json", "json"),
        labi("kernel.import", "M", "json", "json"),
        // S4.2 (§5h.3 §2/§3/§6; AC-R-2.9.3-{7,10,12}): the scoped kinds
        // ride `kernel.bundle{kind}`; `validate` is the staged gate
        // (S1..S9 at `publication`), `diff`/`fetch`/`export` are the
        // transport verbs, `status`/`set_status` are the status book,
        // `attest`/`audit_bundle`/`supersede`/`migrate`/`lineage` the
        // lifecycle surface.
        labi("kernel.validate", "M", "json", "json"),
        labi("kernel.diff", "M", "json", "json"),
        labi("kernel.fetch", "M", "json", "json"),
        labi("kernel.export", "M", "json", "json"),
        labi("kernel.status", "M", "json", "json"),
        labi("kernel.set_status", "M", "json", "json"),
        labi("kernel.attest", "M", "json", "json"),
        labi("kernel.audit_bundle", "M", "json", "json"),
        labi("kernel.supersede", "M", "json", "json"),
        labi("kernel.migrate", "M", "json", "json"),
        labi("kernel.lineage", "M", "json", "json"),
        // The Group M environment ops (ADR-0137/0138; ADR-0177 D7) —
        // implemented at S2.10: `env.snapshot` (fs_tree, `instrument`-
        // charged), `env.derive` (fresh_from_image/fork_snapshot/
        // scoped_subtree), `env.set_phase` (the sealed phase schedule —
        // fail-closed when undeclared). `serves_measurement` gated.
        OpSpec {
            implemented: true,
            ..lab("env.derive", "M", "json", "json")
        },
        OpSpec {
            implemented: true,
            ..lab("env.snapshot", "M", "json", "json")
        },
        OpSpec {
            implemented: true,
            ..lab("env.set_phase", "M", "json", "json")
        },
        // S4.13 (R-2.2.5¹; §5a.5) — `env.suspend`/`env.resume`: the
        // `suspended` state pair; `suspend` refuses `Unsupported` on a
        // class that never declared the capability; `resume` requires
        // `suspended` and lands `suspended_ms` on the `resumed` row.
        OpSpec {
            implemented: true,
            ..lab("env.suspend", "M", "json", "json")
        },
        OpSpec {
            implemented: true,
            ..lab("env.resume", "M", "json", "json")
        },
        // ── Group L — Lab/design-time (ADR-0183; records-in/records-out)
        // S3.5 (§6.1; R-2.10.1): the assembly service — `assemble`/`plan`/
        // `apply`/`validate_batch`/`explain`/`diff`/`drift`/`adopt`/`identity`
        // are live dispatch; `compile` stays `stage_pending` until the
        // `CompileInputs`/`ModelProfile` wire codec lands (no D2 schema for
        // profile binding exists yet — never fabricate one).
        labi("lab.assembly.assemble", "L", "json", "json"),
        labi("lab.assembly.plan", "L", "json", "json"),
        labi("lab.assembly.apply", "L", "json", "json"),
        labi("lab.assembly.validate_batch", "L", "json", "json"),
        labi("lab.assembly.explain", "L", "json", "json"),
        labi("lab.assembly.diff", "L", "json", "json"),
        labi("lab.assembly.drift", "L", "json", "json"),
        labi("lab.assembly.adopt", "L", "json", "json"),
        labi("lab.assembly.identity", "L", "json", "json"),
        // `lab.assembly.compile` is implemented (S4.12) — the
        // definition→bundle compile `lab.serve`'s on-demand arm and
        // external tooling share; records-in/records-out, no lift.
        labi("lab.assembly.compile", "L", "json", "json"),
        labi("lab.registry.register", "L", "json", "json"),
        labi("lab.registry.publish", "L", "json", "json"),
        labi("lab.registry.resolve", "L", "json", "json"),
        labi("lab.registry.query", "L", "json", "json"),
        labi("lab.registry.catalog", "L", "json", "json"),
        labi("lab.registry.slot_choices", "L", "json", "json"),
        labi("lab.registry.substitutable", "L", "json", "json"),
        labi("lab.registry.snapshot", "L", "json", "json"),
        labi("lab.registry.record_conformance", "L", "json", "json"),
        labi("lab.registry.deprecate", "L", "json", "json"),
        labi("lab.registry.yank", "L", "json", "json"),
        labi("lab.registry.revoke", "L", "json", "json"),
        labi("lab.registry.lineage", "L", "json", "json"),
        labi("lab.registry.sameness", "L", "json", "json"),
        // S3.9 (§5d.1 §2; R-2.5.4⁰): `import`/`refresh` over
        // `hh-mcp-listing/1` documents are live dispatch.
        // S4.1 (§6.2; R-2.10.2): the general `import` head
        // (`{foreign_ref, document}` vs `{listing}`), `export` →
        // `plugin_manifest/1`, `pin` endorsement, and the
        // `publisher_claims` review projection are all live dispatch.
        labi("lab.registry.import", "L", "json", "json"),
        labi("lab.registry.refresh", "L", "json", "json"),
        labi("lab.registry.export", "L", "json", "json"),
        labi("lab.registry.pin", "L", "json", "json"),
        labi("lab.registry.publisher_claims", "L", "json", "json"),
        labi("lab.registry.verify", "L", "json", "json"),
        // S4.14a (§5g.5/§6.2; R-2.8.5 C1, R-2.12.2¹): the extension
        // lifecycle surface — `discover`/`resolve` run the TrustView gate
        // (source allowlists + attestation verification + the
        // signature-required quarantine), `review` emits the review
        // document, `check_surface` the pin-vs-live drift check,
        // `update` the widening/label-regression assessment, `revoke` the
        // revocation-propagation plan, `install` the model-install
        // procedure. `import_plugin`/`export_plugin` are the three named
        // foreign formats (`claude_plugin_json/1`, `gemini_extension/1`,
        // `codex_agent_plugin/1`) — imports land quarantined with a
        // LossReport, exports re-emit the verbatim substrate.
        labi("lab.extension.discover", "L", "json", "json"),
        labi("lab.extension.resolve", "L", "json", "json"),
        labi("lab.extension.review", "L", "json", "json"),
        labi("lab.extension.check_surface", "L", "json", "json"),
        labi("lab.extension.update", "L", "json", "json"),
        labi("lab.extension.revoke", "L", "json", "json"),
        labi("lab.extension.install", "L", "json", "json"),
        labi("lab.extension.import_plugin", "L", "json", "json"),
        labi("lab.extension.export_plugin", "L", "json", "json"),
        // §5g.7 §4 (ADR-0070; OQ-177's interim): the `ApproverGrant`
        // lifecycle — `grant_approver` issues the durable
        // `security.permission.grant_issued` row (principal+ or covered
        // grantor; delegation never widens), `revoke_approver` the
        // `grant_revoked` row. The respond path folds both.
        labi("lab.permission.grant_approver", "L", "json", "json"),
        labi("lab.permission.revoke_approver", "L", "json", "json"),
        // S3.3: the `lab.eval.*` eval-kernel boundary (R-2.9.2/R-2.9.4⁰ᵇ;
        // records-in/records-out).
        labi("lab.eval.catalogue", "L", "json", "json"),
        labi("lab.eval.compare", "L", "json", "json"),
        labi("lab.eval.render_scorecard", "L", "json", "json"),
        labi("lab.eval.equivalence_run", "L", "json", "json"),
        labi("lab.eval.loss_report", "L", "json", "json"),
        labi("lab.experiment.register", "L", "json", "json"),
        labi("lab.experiment.expand", "L", "json", "json"),
        labi("lab.experiment.open_experiment", "L", "json", "json"),
        labi("lab.experiment.next", "L", "json", "json"),
        labi("lab.experiment.claim", "L", "json", "json"),
        labi("lab.experiment.launch", "L", "json", "json"),
        labi("lab.experiment.settle", "L", "json", "json"),
        labi("lab.experiment.pause", "L", "json", "json"),
        labi("lab.experiment.resume", "L", "json", "json"),
        labi("lab.experiment.close", "L", "json", "json"),
        labi("lab.producer.declare", "L", "json", "json"),
        labi("lab.producer.bind", "L", "json", "json"),
        labi("lab.producer.exclude", "L", "json", "json"),
        labi("lab.producer.amend", "L", "json", "json"),
        labi("lab.producer.record_analysis", "L", "json", "json"),
        // `lab.analysis.analyze` is implemented at S3.4c (the estimator
        // kernel over the results store — R-2.10.4⁰ᵇ).
        labi("lab.analysis.analyze", "L", "json", "json"),
        labi("lab.analysis.render", "L", "json", "json"),
        labi("lab.analysis.diff_reports", "L", "json", "json"),
        labi("lab.analysis.power", "L", "json", "json"),
        labi("lab.results.get_row", "L", "json", "json"),
        labi("lab.results.row_history", "L", "json", "json"),
        labi("lab.results.query_rows", "L", "json", "json"),
        labi("lab.results.cells", "L", "json", "json"),
        labi("lab.results.distribution", "L", "json", "json"),
        labi("lab.results.catalogue", "L", "json", "json"),
        labi("lab.results.subscribe", "L", "json", "json"),
        labi("lab.results.verify_row", "L", "json", "json"),
        labi("lab.results.verify_snapshot", "L", "json", "json"),
        labi("lab.results.verify_citation", "L", "json", "json"),
        labi("lab.results.export_rows", "L", "json", "json"),
        labi("lab.leaderboard.define", "L", "json", "json"),
        labi("lab.leaderboard.leaderboard", "L", "json", "json"),
        labi("lab.leaderboard.diff_snapshots", "L", "json", "json"),
        labi("lab.leaderboard.publish", "L", "json", "json"),
        labi("lab.leaderboard.retract_entry", "L", "json", "json"),
        // S4.4 — §6.5 §2.2's `snapshots(definition_ref) → [snapshot_id]`
        // read (the retained-snapshot list; `leaderboard` returns the
        // snapshot document itself).
        labi("lab.leaderboard.snapshots", "L", "json", "json"),
        // S4.5a (§6.6; R-2.10.6): the Hosting ABI boundary — `describe`
        // (registry read + reconciled capability_vector), `probe`
        // (conformance entries + P0 quarantine; `drive` through the
        // removable HostingPlane seam), `attach` (the hosted-session
        // ledger write + metric emissions).
        labi("lab.hosting.describe", "L", "json", "json"),
        labi("lab.hosting.probe", "L", "json", "json"),
        labi("lab.hosting.attach", "L", "json", "json"),
        // ── S5.4 (R-2.9.6¹/R-2.9.8¹/R-2.9.7¹): the debt-manager surface
        // (`evaluate` = the live all-home trigger fold; `index`/`report` =
        // the DebtIndex + DebtReport/notifier projections — records-in/
        // records-out, the manager is out-of-process by construction,
        // AC-R-2.9.6-10), the model-compat surface (`snapshot_claim` =
        // the synthetic provider-drift claim; `regression` =
        // `run_regression_suite`), and the M1 design surface
        // (`component_targets`, `attribution_design`).
        labi("lab.debt.evaluate", "L", "json", "json"),
        labi("lab.debt.index", "L", "json", "json"),
        labi("lab.debt.report", "L", "json", "json"),
        labi("lab.model.snapshot_claim", "L", "json", "json"),
        labi("lab.model.regression", "L", "json", "json"),
        labi("lab.analysis.component_targets", "L", "json", "json"),
        labi("lab.analysis.attribution_design", "L", "json", "json"),
        // ── S6.1b (§5h.6 R-2.9.6): the assumption-debt *manager*
        // service — `hh-debt` over the registry run's `lifecycle.debt.*`
        // book of record (records-in/records-out; C4-tier — a
        // `--no-default-features` build answers
        // `Unsupported{by: "tier-c4"}`).
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.manager_open",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.register",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.sweep",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.settle",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.retire",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.debt.propose",
                "L",
                "json",
                "json",
                DEBTMGR_ERR,
                Tier::Experimental,
                true,
            )
        },
        // ── S6.1a (§05h R-2.9.5): the evolution-pipeline surface —
        // `lab.evolution.*` drives the S0–S10 campaign driver
        // (`hh-evolution`; records-in/records-out like `lab.experiment.*`,
        // C4-tier — a `--no-default-features` build answers
        // `Unsupported{by: "tier-c4"}`).
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.campaign_open",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.campaign_ensure",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.propose",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.hypothesize",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.screen",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.matched_eval",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.held_out_eval",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.transfer",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.security_check",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.seal",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.canary",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.canary_settle",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.expire",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.retire",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.revalidate",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.reactivate",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.withdraw",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.revert",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.stop",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.close",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            requires_capability: Some("serves_measurement"),
            ..call(
                "lab.evolution.view",
                "L",
                "json",
                "json",
                EVO_ERR,
                Tier::Experimental,
                true,
            )
        },
        // S3.1 (R-2.11.3⁰; ADR-0097 D7/ADR-0173): `serve(bundle)` — the
        // Stage-3 fixture MCP server over stdio. The op decodes the
        // bundle, extracts its compiled `target:mcp` member and returns
        // the ServerHandle + the `stdio_launch` CallerBinding (fixed to
        // the test principal); the caller (the CLI surface) spawns
        // `hh-mcp-serve` and owns the stdio pair.
        labi("lab.serve", "L", "json", "json"),
        // ── S4.9: the `fleet.*` surface — §5i.1's human-agent
        // organizational layer (C4; ADR-0205 D8's record family +
        // ADR-0206 D1's reconciliation family, Groups S/W/R,
        // records-in/records-out). `implemented` — the dispatch lands
        // in `hh-embed`'s `tier-c4` feature; a `--no-default-features`
        // build answers `Unsupported{by: "tier-c4"}` (typed refusal,
        // never silent degrade — CC6).
        OpSpec {
            implemented: true,
            ..call(
                "fleet.open",
                "S",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.ensure",
                "S",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.restore",
                "S",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.observe",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.reconcile",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.create_work_item",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.bind_source",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.claim",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.dispatch",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.dispatch_note",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.settle",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.handoff",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.resume_from_handoff",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.transfer_owner",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.acknowledge_owner",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.cancel",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.annotate",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.block",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.unblock",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.stop",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.set_owner",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.escalate",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.resolve_escalation",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.work_item",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.list",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.fleet_view",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        // ── S5.6: the adapter surface — signed-webhook ingress (W), the
        // capability probe + source read minimum (R) (§5i.1 #3;
        // ADR-0205 D5).
        OpSpec {
            implemented: true,
            ..call(
                "fleet.webhook_ingress",
                "W",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.source_capabilities",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.source_records",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.state_map",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.accountability_record",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.audit_link",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
        OpSpec {
            implemented: true,
            ..call(
                "fleet.check_activation_delta",
                "R",
                "json",
                "json",
                FLEET_ERR,
                Tier::Experimental,
                true,
            )
        },
    ];

    // ── Group U — upcalls (kernel→host signatures) ────────────────────
    v.extend([
        OpSpec {
            requires_capability: Some("serves_permission_channel"),
            ..upcall(
                "upcall.request_permission",
                "RequestPermission",
                "PermissionOutcome",
            )
        },
        OpSpec {
            requires_capability: Some("serves_host_executor"),
            ..upcall(
                "upcall.invoke_host_capability",
                "InvokeHostCapability",
                "HostResult",
            )
        },
        OpSpec {
            requires_capability: Some("serves_hook_observer"),
            ..upcall("upcall.notify_hook", "HookNotification", "HookResult")
        },
        OpSpec {
            requires_capability: Some("serves_elicitation"),
            ..upcall("upcall.elicit", "ElicitParams", "ElicitResult")
        },
    ]);
    v
}

/// Look an op up by method name.
pub fn lookup(name: &str) -> Option<OpSpec> {
    registry().into_iter().find(|o| o.name == name)
}

/// The machine-readable stability map `hello` returns:
/// `method → {tier, deprecated?, debt?}` (§7.4 §2.3, §7.4 §6; `deprecated`
/// emits `{since, removal_major, replacement}` — nothing in this contract is
/// deprecated today, so no entry carries the member).
pub fn stability_map() -> hh_wire::json::Json {
    let mut m = std::collections::BTreeMap::new();
    for op in registry() {
        m.insert(op.name.to_string(), stability_entry(&op));
    }
    hh_wire::json::Json::Obj(m)
}

/// One op's stability-map entry — `{tier, deprecated?, debt?}`. Factored out
/// so a fabricated `OpSpec` (a test's deprecation fixture) exercises the same
/// emission path the registry iterates.
pub fn stability_entry(op: &OpSpec) -> hh_wire::json::Json {
    let mut entry = std::collections::BTreeMap::new();
    entry.insert(
        "tier".to_string(),
        hh_wire::json::Json::str(op.tier.as_str()),
    );
    if let Some(dep) = &op.deprecated {
        entry.insert("deprecated".to_string(), dep.to_json());
    }
    if let Some(d) = &op.debt {
        entry.insert(
            "debt".to_string(),
            hh_wire::json::Json::obj([
                ("assumption_id", hh_wire::json::Json::str(d.assumption_id)),
                ("debt_id", hh_wire::json::Json::str(d.debt_id)),
                ("owner_layer", hh_wire::json::Json::str(d.owner_layer)),
                ("stage_gate", hh_wire::json::Json::str(d.stage_gate)),
                ("status", hh_wire::json::Json::str("open")),
                ("detail", hh_wire::json::Json::str(d.detail)),
            ]),
        );
    }
    hh_wire::json::Json::Obj(entry)
}
