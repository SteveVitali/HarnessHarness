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

const fn staged_stable(
    name: &'static str,
    group: &'static str,
    params: &'static str,
    result: &'static str,
    errors: &'static [&'static str],
) -> OpSpec {
    call(name, group, params, result, errors, Tier::Stable, false)
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
        // `fork` is implemented — `open_run` binds `forked_from` with the
        // same anchor coherence the ledger enforces everywhere.
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
                    "Refused",
                ],
                Tier::Experimental,
                true,
            )
        },
        staged_exp("navigate", "W", "NavigateParams", "Accepted"),
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
        staged_exp("set_coordinate", "W", "SetCoordinateParams", "Recorded"),
        staged_exp(
            "coherent_fork_points",
            "W",
            "CoherentForkPointsParams",
            "json",
        ),
        staged_exp("discard", "W", "DiscardParams", "Acknowledged"),
        staged_exp("promote", "W", "PromoteParams", "Session"),
        staged_exp("rollback", "W", "RollbackParams", "Session"),
        staged_exp("replay", "W", "ReplayParams", "Session"),
        staged_exp("counterfactual", "W", "CounterfactualParams", "Session"),
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
        staged_stable(
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
        ),
        staged_stable("verify", "R", "VerifyParams", "json", SESS),
        staged_stable("prove_inclusion", "R", "ProveInclusionParams", "json", SESS),
        staged_exp(
            "request_redaction",
            "R",
            "RequestRedactionParams",
            "Acknowledged",
        ),
        // ── Group M — measurement/kernel-direct (serves_measurement) ──
        lab("measurement.emit_metric", "M", "json", "json"),
        lab("kernel.compose", "M", "json", "json"),
        lab("kernel.resolve", "M", "json", "json"),
        lab("kernel.validate_assembly", "M", "json", "json"),
        lab("kernel.seal", "M", "json", "json"),
        lab("kernel.space", "M", "json", "json"),
        lab("kernel.enumerate", "M", "json", "json"),
        lab("kernel.bundle", "M", "json", "json"),
        lab("kernel.check_completeness", "M", "json", "json"),
        lab("kernel.reproduce", "M", "json", "json"),
        lab("env.derive", "M", "json", "json"),
        lab("env.snapshot", "M", "json", "json"),
        lab("env.set_phase", "M", "json", "json"),
        // ── Group L — Lab/design-time (ADR-0183; records-in/records-out)
        lab("lab.assembly.assemble", "L", "json", "json"),
        lab("lab.assembly.plan", "L", "json", "json"),
        lab("lab.assembly.apply", "L", "json", "json"),
        lab("lab.assembly.validate_batch", "L", "json", "json"),
        lab("lab.assembly.explain", "L", "json", "json"),
        lab("lab.assembly.diff", "L", "json", "json"),
        lab("lab.assembly.identity", "L", "json", "json"),
        lab("lab.assembly.compile", "L", "json", "json"),
        lab("lab.registry.register", "L", "json", "json"),
        lab("lab.registry.publish", "L", "json", "json"),
        lab("lab.registry.resolve", "L", "json", "json"),
        lab("lab.registry.query", "L", "json", "json"),
        lab("lab.registry.catalog", "L", "json", "json"),
        lab("lab.registry.slot_choices", "L", "json", "json"),
        lab("lab.registry.substitutable", "L", "json", "json"),
        lab("lab.registry.snapshot", "L", "json", "json"),
        lab("lab.registry.record_conformance", "L", "json", "json"),
        lab("lab.registry.deprecate", "L", "json", "json"),
        lab("lab.registry.yank", "L", "json", "json"),
        lab("lab.registry.revoke", "L", "json", "json"),
        lab("lab.registry.lineage", "L", "json", "json"),
        lab("lab.registry.sameness", "L", "json", "json"),
        lab("lab.registry.import", "L", "json", "json"),
        lab("lab.registry.export", "L", "json", "json"),
        lab("lab.registry.verify", "L", "json", "json"),
        lab("lab.experiment.register", "L", "json", "json"),
        lab("lab.experiment.expand", "L", "json", "json"),
        lab("lab.experiment.open_experiment", "L", "json", "json"),
        lab("lab.experiment.next", "L", "json", "json"),
        lab("lab.experiment.claim", "L", "json", "json"),
        lab("lab.experiment.launch", "L", "json", "json"),
        lab("lab.experiment.settle", "L", "json", "json"),
        lab("lab.experiment.pause", "L", "json", "json"),
        lab("lab.experiment.resume", "L", "json", "json"),
        lab("lab.experiment.close", "L", "json", "json"),
        lab("lab.producer.declare", "L", "json", "json"),
        lab("lab.producer.bind", "L", "json", "json"),
        lab("lab.producer.exclude", "L", "json", "json"),
        lab("lab.producer.amend", "L", "json", "json"),
        lab("lab.producer.record_analysis", "L", "json", "json"),
        lab("lab.analysis.analyze", "L", "json", "json"),
        lab("lab.analysis.render", "L", "json", "json"),
        lab("lab.analysis.diff_reports", "L", "json", "json"),
        lab("lab.analysis.power", "L", "json", "json"),
        lab("lab.results.get_row", "L", "json", "json"),
        lab("lab.results.row_history", "L", "json", "json"),
        lab("lab.results.query_rows", "L", "json", "json"),
        lab("lab.results.cells", "L", "json", "json"),
        lab("lab.results.distribution", "L", "json", "json"),
        lab("lab.results.catalogue", "L", "json", "json"),
        lab("lab.results.subscribe", "L", "json", "json"),
        lab("lab.results.verify_row", "L", "json", "json"),
        lab("lab.results.verify_snapshot", "L", "json", "json"),
        lab("lab.results.verify_citation", "L", "json", "json"),
        lab("lab.results.export_rows", "L", "json", "json"),
        lab("lab.leaderboard.define", "L", "json", "json"),
        lab("lab.leaderboard.leaderboard", "L", "json", "json"),
        lab("lab.leaderboard.diff_snapshots", "L", "json", "json"),
        lab("lab.leaderboard.publish", "L", "json", "json"),
        lab("lab.leaderboard.retract_entry", "L", "json", "json"),
        lab("lab.hosting.describe", "L", "json", "json"),
        lab("lab.hosting.probe", "L", "json", "json"),
        lab("lab.hosting.attach", "L", "json", "json"),
        lab("lab.serve", "L", "json", "json"),
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
/// `method → {tier, deprecated?, debt?}` (§7.4 §2.3, §7.4 §6; the
/// `deprecated` member is reserved — nothing in this contract is
/// deprecated).
pub fn stability_map() -> hh_wire::json::Json {
    let mut m = std::collections::BTreeMap::new();
    for op in registry() {
        let mut entry = std::collections::BTreeMap::new();
        entry.insert(
            "tier".to_string(),
            hh_wire::json::Json::str(op.tier.as_str()),
        );
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
        m.insert(op.name.to_string(), hh_wire::json::Json::Obj(entry));
    }
    hh_wire::json::Json::Obj(m)
}
