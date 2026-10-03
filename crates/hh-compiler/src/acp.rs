//! The ACP session-artefact projection (§5d.4 D2/D3/D7; R-2.1.3¹'s
//! `target: acp`; ADR-0096 D5 — projections live in the compiler's
//! target module). This module is the **single source** of the
//! event-class lowering table, the tool-kind projection and the
//! A2A `TaskState` projection (CC1 — `hh-acp`'s `render_session`
//! consumes these same rows; there is no second table).
//!
//! The `hh-acp-target/1` artefact (§5d.4 D2's `LowerTarget<acp>`
//! form) carries: the `InitializeResponse` the session serve loop
//! answers verbatim, `configOptions[]`, the event-class lowering
//! table, the tool-kind/task-state projections, the supported
//! `_hh/*` method set (`agentCapabilities._meta`), and the
//! `LoweringLossReport` at `granularity_ceiling: configuration`
//! (CC7 — the ACP artefact's own ceiling).
//!
//! `lift` reconstructs the `agent&run` minimum-carried-set — the
//! ACP artefact is a *lower-only* surface for tool definitions
//! (its session surface is protocol-owned); the lifted minimum is
//! the agent identity + the run coordinate, never a tool set.

use std::collections::BTreeMap;

use hh_hir::kinds::EffectDomain;
use hh_hir::records::KindRecord;
use hh_wire::json::Json;

use crate::lcd::{GranularityCeiling, LossEntry, LossKind, LossSeverity, LoweringLossReport};
use crate::link::{LinkedGraph, TargetSpec};
use crate::plan::RuntimePlan;
use crate::seal::ModelSurface;
use crate::CompileError;

/// The pinned ACP dialect version (the v2 pin).
pub const ACP_VERSION: &str = "0.1.0";
/// The v1 compatibility profile label (§5d.4 D2's v1 profile).
pub const ACP_LEGACY_VERSION: &str = "v1-compat";
/// The artefact schema id.
pub const ACP_ARTEFACT_SCHEMA: &str = "hh-acp-target/1";
/// The edge-loss-report schema id (`hh-edge-loss/1`).
pub const EDGE_LOSS_SCHEMA: &str = "hh-edge-loss/1";

/// The `_hh/*` extension-method set the artefact advertises via
/// `agentCapabilities._meta.hh_methods[]` (§5d.4 D2's declared set).
pub const HH_METHODS: [&str; 3] = ["_hh/ledger/read", "_hh/account", "_hh/participant/describe"];

/// The request kinds the session surface serves (`initialize` +
/// the session verb set + `subscriptions/listen`).
pub const ACP_REQUEST_KINDS: [&str; 8] = [
    "initialize",
    "session/new",
    "session/prompt",
    "session/cancel",
    "session/resume",
    "session/close",
    "session/request_permission",
    "subscriptions/listen",
];

/// One event-class lowering-table row (§5d.4 §3 — the pinned
/// `(class, session/update kind, loss class, ordering)` tuple;
/// `durable_seq` marks the durable-before-visible ordering the
/// renderer enforces: `state_update{idle}` lands only *after* the
/// durable `lifecycle.turn.finished` — the table's `lifecycle.
/// turn.finished → state_update` row is that edge's source).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventLowering {
    /// The ledger event class.
    pub class: &'static str,
    /// The `session/update` kind the class renders to (`"none"` is
    /// the table's catch-all — internal rows stay internal).
    pub update: &'static str,
    /// The lowering loss class the row accounts
    /// (`identity|hint_only|narrowed|collapsed|agent_control_only`).
    pub loss: &'static str,
    /// Whether the rendered update must follow its durable event
    /// (the durable-before-visible rule — `idle` never precedes the
    /// durable `turn.finished`).
    pub durable_seq: bool,
}

/// The event-class lowering table (§5d.4 §3 — every row is a pinned
/// decision, not a heuristic).
pub const EVENT_CLASS_LOWERING: &[EventLowering] = &[
    EventLowering {
        class: "lifecycle.turn.started",
        update: "state_update{running}",
        loss: "identity",
        durable_seq: true,
    },
    EventLowering {
        class: "lifecycle.turn.finished",
        update: "state_update{idle}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "action.tool.proposed",
        update: "tool_call_update{pending}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "action.effect.committed",
        update: "tool_call_update{in_progress}",
        loss: "collapsed",
        durable_seq: true,
    },
    EventLowering {
        class: "action.tool.completed",
        update: "tool_call_update{completed}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "action.tool.rejected",
        update: "tool_call_update{failed}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "action.tool.surface_rejected",
        update: "tool_call_update{failed}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "context.observation.recorded",
        update: "tool_call_update{content}",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "model.stream.delta",
        update: "agent_message_chunk",
        loss: "narrowed",
        durable_seq: false, // the ephemeral stream — never durable-first
    },
    EventLowering {
        class: "control.decision{kind:plan}",
        update: "plan_update",
        loss: "hint_only",
        durable_seq: true,
    },
    EventLowering {
        class: "control.budget.consumed",
        update: "usage_update",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "measurement.cost.attributed",
        update: "usage_update",
        loss: "narrowed",
        durable_seq: true,
    },
    EventLowering {
        class: "security.permission.requested{decider:human}",
        update: "session/request_permission",
        loss: "narrowed",
        durable_seq: true,
    },
    // The unstable extension surface — declared unstable on the
    // artefact (`unstable: true`), rendered under the v2 dialect only.
    EventLowering {
        class: "lifecycle.run.forked",
        update: "session/fork",
        loss: "hint_only",
        durable_seq: true,
    },
    EventLowering {
        class: "context.compaction.emitted",
        update: "compaction_update",
        loss: "hint_only",
        durable_seq: true,
    },
];

/// The `session/update` kinds the artefact's table can emit (the
/// advertised `update_kinds` member — the v1 profile drops the
/// unstable extension rows).
pub const V2_UPDATE_KINDS: [&str; 13] = [
    "state_update",
    "tool_call_update",
    "agent_message_chunk",
    "agent_thought_chunk",
    "plan_update",
    "usage_update",
    "session/request_permission",
    "session/fork",
    "compaction_update",
    "session/update",
    "current_mode_update",
    "available_commands_update",
    "config_option_update",
];

/// `project_tool_kind(domain)` — the ToolKind projection (§5d.4 §3's
/// capability-domain → ACP `kind` map): `read|edit|execute|think|
/// fetch|search|switch_mode|move|delete|other`.
pub fn project_tool_kind(domain: EffectDomain) -> &'static str {
    match domain {
        EffectDomain::FsRead => "read",
        EffectDomain::FsWrite => "edit",
        EffectDomain::Exec | EffectDomain::SpawnProcess => "execute",
        EffectDomain::ModelCall => "think",
        EffectDomain::NetEgress => "fetch",
        EffectDomain::SecretAccess
        | EffectDomain::Spend
        | EffectDomain::MessageHuman
        | EffectDomain::MemoryWrite
        | EffectDomain::PermissionRequest => "other",
    }
}

/// The A2A `TaskState` closed sum (§5d.4 §3's task-state table;
/// `UNSPECIFIED` is the honest fold for unknowable states).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// `SUBMITTED` — received, not yet executing.
    Submitted,
    /// `WORKING` — executing.
    Working,
    /// `COMPLETED` — terminal success.
    Completed,
    /// `FAILED` — terminal failure.
    Failed,
    /// `CANCELED` — cancelled.
    Canceled,
    /// `REJECTED` — refused before execution.
    Rejected,
    /// `INPUT_REQUIRED` — paused awaiting input (the §5d.4 D3 arm).
    InputRequired,
    /// `AUTH_REQUIRED` — awaiting an authorization decision.
    AuthRequired,
    /// `UNSPECIFIED` — the fold cannot say.
    Unspecified,
}

impl TaskState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Submitted => "submitted",
            TaskState::Working => "working",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
            TaskState::Rejected => "rejected",
            TaskState::InputRequired => "input_required",
            TaskState::AuthRequired => "auth_required",
            TaskState::Unspecified => "unspecified",
        }
    }
}

/// `project_task_state(phase, outcome, flags)` — the **exhaustive**
/// outcome-class → `TaskState` map (§5d.4 §3; AC-R-2.5.4-7). The
/// paused/permission flags come from the run's pause trail and pending
/// permission rows — they dominate (a paused working effect is
/// `INPUT_REQUIRED`; a permission-blocked one is `AUTH_REQUIRED`).
///
/// `phase`/`outcome` spell the ledger's `EffectPhase`/`ObservedOutcome`
/// names (string input so the A2A crate never mirrors the enum — the
/// map is total over the closed spellings; unknown spellings fold to
/// `UNSPECIFIED`, never a panic).
pub fn project_task_state(
    phase: &str,
    outcome: Option<&str>,
    paused: bool,
    auth_pending: bool,
) -> TaskState {
    if paused {
        return TaskState::InputRequired;
    }
    if auth_pending {
        return TaskState::AuthRequired;
    }
    match phase {
        "intended" => TaskState::Submitted,
        "authorized" | "prepared" | "committed" | "deferred" => TaskState::Working,
        "observed" => match outcome {
            Some("applied") => TaskState::Completed,
            Some("partial") => TaskState::Working, // non-terminal — still in flight
            Some("not_applied") | Some(_) | None => TaskState::Failed,
        },
        "refused" => TaskState::Rejected,
        "unknown" => TaskState::Unspecified,
        "compensated" | "reverted" => TaskState::Completed,
        "abandoned" => TaskState::Failed,
        _ => TaskState::Unspecified,
    }
}

/// `AcpDialect` — the v2 pin + the v1 compatibility profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpDialect {
    /// The pinned dialect (full update surface).
    V2,
    /// The v1 compatibility profile — the collapsed subset
    /// (`narrowed`/`collapsed` loss entries ride the artefact).
    V1,
}

impl AcpDialect {
    /// The dialect label.
    pub fn as_str(self) -> &'static str {
        match self {
            AcpDialect::V2 => "v2",
            AcpDialect::V1 => "v1",
        }
    }

    /// Whether an update kind survives the profile — v1 drops the
    /// unstable extension surface (`session/fork`, `compaction_update`)
    /// and `agent_thought_chunk` (the collapsed rows report
    /// `narrowed`/`collapsed` on the loss list).
    pub fn admits(self, update_kind: &str) -> bool {
        match self {
            AcpDialect::V2 => true,
            AcpDialect::V1 => !matches!(
                update_kind,
                "session/fork" | "compaction_update" | "agent_thought_chunk"
            ),
        }
    }
}

/// `lower_event(class, payload)` — the event-class lowering (the
/// renderer's per-event half): returns the `session/update` params
/// the class renders to, or `None` for an internal row (the catch-all).
/// Every narrowing is explicit in the emitted `sessionUpdate` member;
/// the dropped members are reported by the artefact's loss table,
/// never silently coerced.
pub fn lower_event(class: &str, payload: &Json) -> Option<(String, Json)> {
    // `control.decision{kind: plan}` / `security.permission.requested
    // {decider: human}` discriminate on payload members.
    let row = EVENT_CLASS_LOWERING.iter().find(|r| {
        r.class == class
            || (r.class.starts_with("control.decision{")
                && class == "control.decision"
                && payload.get("kind").and_then(Json::as_str) == Some("plan"))
            || (r.class.starts_with("security.permission.requested{")
                && class == "security.permission.requested"
                && payload.get("decider").and_then(Json::as_str) == Some("human"))
    })?;
    let update = match row.update {
        "state_update{running}" => ("state_update", Json::obj([("state", Json::str("running"))])),
        "state_update{idle}" => (
            "state_update",
            Json::obj([
                ("state", Json::str("idle")),
                (
                    "stop_reason",
                    payload
                        .get("stop_reason")
                        .cloned()
                        .unwrap_or(Json::str("end_turn")),
                ),
            ]),
        ),
        "tool_call_update{pending}" => (
            "tool_call_update",
            Json::obj([
                ("status", Json::str("pending")),
                (
                    "title",
                    payload
                        .get("capability_ref")
                        .and_then(|c| c.get("semantic_id"))
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        "tool_call_update{in_progress}" => (
            "tool_call_update",
            Json::obj([
                ("status", Json::str("in_progress")),
                (
                    "_meta",
                    Json::obj([("hh", Json::obj([("phase", Json::str("committed"))]))]),
                ),
            ]),
        ),
        "tool_call_update{completed}" => (
            "tool_call_update",
            Json::obj([(
                "status",
                payload
                    .get("status")
                    .cloned()
                    .unwrap_or(Json::str("completed")),
            )]),
        ),
        "tool_call_update{failed}" => (
            "tool_call_update",
            Json::obj([("status", Json::str("failed"))]),
        ),
        "tool_call_update{content}" => (
            "tool_call_update",
            Json::obj([(
                "content",
                payload.get("manifest_ref").cloned().unwrap_or(Json::Null),
            )]),
        ),
        "agent_message_chunk" => (
            "agent_message_chunk",
            Json::obj([(
                "content",
                payload
                    .get("text")
                    .or_else(|| payload.get("delta"))
                    .cloned()
                    .unwrap_or(Json::Null),
            )]),
        ),
        "plan_update" => (
            "plan_update",
            Json::obj([(
                "plan",
                payload.get("verdict").cloned().unwrap_or(Json::Null),
            )]),
        ),
        "usage_update" => (
            "usage_update",
            Json::obj([
                (
                    "used",
                    payload
                        .get("tokens")
                        .or_else(|| payload.get("amount"))
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
                ("size", payload.get("limit").cloned().unwrap_or(Json::Null)),
                ("cost", payload.get("cost").cloned().unwrap_or(Json::Null)),
            ]),
        ),
        "session/request_permission" => (
            "session/request_permission",
            Json::obj([
                (
                    "options",
                    payload
                        .get("options_presented")
                        .cloned()
                        .unwrap_or(Json::Arr(vec![])),
                ),
                (
                    "tool_call",
                    payload.get("capability_ref").cloned().unwrap_or(Json::Null),
                ),
            ]),
        ),
        "session/fork" => (
            "session/fork",
            Json::obj([(
                "forked_from",
                payload.get("parent_run_id").cloned().unwrap_or(Json::Null),
            )]),
        ),
        "compaction_update" => (
            "compaction_update",
            Json::obj([("detail", payload.clone())]),
        ),
        _ => return None,
    };
    Some((update.0.to_string(), update.1))
}

/// `edge_loss_report(binding, protocol, granularity)` — the
/// `hh-edge-loss/1` record (§5d.4 D2's `edge_loss_report(binding)` —
/// binding-indexed, never protocol-global).
pub fn edge_loss_report(binding: &Json, granularity: GranularityCeiling) -> Json {
    Json::obj([
        ("schema", Json::str(EDGE_LOSS_SCHEMA)),
        ("binding", binding.clone()),
        ("granularity", Json::str(granularity.name())),
    ])
}

/// `lower_acp(linked, plan, surface, spec)` — the `hh-acp-target/1`
/// artefact (R-2.1.3¹). `agent&run` → `InitializeResponse` +
/// `configOptions[]` + the lowering/projection tables + the loss
/// report at `configuration` granularity. The session surface is
/// protocol-owned — the artefact carries *no* tool catalogue (the
/// ACP agent's surface is the session protocol, never a `tools[]`).
pub fn lower_acp(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    _surface: &ModelSurface,
    spec: &TargetSpec,
) -> Result<(Json, LoweringLossReport), CompileError> {
    // The agent&run identity — the first `AgentProcess` node is the
    // session artefact's `agentInfo` (a compiled definition carries
    // exactly one at C1).
    let agent = linked
        .sealed
        .document
        .nodes
        .iter()
        .find(|n| matches!(n.semantic, KindRecord::AgentProcess(_)));
    let agent_id = agent
        .map(|n| n.semantic_id())
        .unwrap_or_else(|| plan.ids.definition_ref.semantic_id.clone());
    let agent_name = agent_id.clone();
    // `configOptions[]` — the session-configurable surface the run's
    // policy exposes (mode/model the client may select at
    // `session/new`; lowered from the bound slots honestly — the
    // bound set is the selectable set).
    let mut options: Vec<String> = plan.bound_slots.keys().cloned().collect();
    options.sort();
    let config_options = Json::Arr(
        options
            .iter()
            .map(|o| {
                Json::obj([
                    ("id", Json::str(o.clone())),
                    ("name", Json::str(format!("{o} (bound)"))),
                    (
                        "options",
                        Json::Arr(vec![Json::obj([(
                            "value",
                            Json::str(plan.bound_slots[o].variant_id.clone()),
                        )])]),
                    ),
                ])
            })
            .collect(),
    );
    let initialize_response = Json::obj([
        ("protocolVersion", Json::str(ACP_VERSION)),
        (
            "agentInfo",
            Json::obj([
                ("name", Json::str(agent_name)),
                (
                    "version",
                    Json::str(plan.ids.definition_ref.version_id.clone()),
                ),
            ]),
        ),
        (
            "agentCapabilities",
            Json::obj([
                ("loadSession", Json::Bool(true)),
                (
                    "promptCapabilities",
                    Json::obj([
                        ("text", Json::Bool(true)),
                        ("embeddedContext", Json::Bool(true)),
                        ("image", Json::Bool(false)),
                    ]),
                ),
                (
                    "mcpCapabilities",
                    Json::obj([("http", Json::Bool(false)), ("stdio", Json::Bool(true))]),
                ),
                // `sessionCapabilities` — the unstable extension
                // surface, declared unstable (never silently stable).
                (
                    "sessionCapabilities",
                    Json::obj([
                        ("resume", Json::obj([("unstable", Json::Bool(true))])),
                        ("fork", Json::obj([("unstable", Json::Bool(true))])),
                    ]),
                ),
                // `agentCapabilities._meta` — the supported `_hh/*`
                // methods + the request kinds + the pin (D2's
                // advertisement channel — data, never authority).
                (
                    "_meta",
                    Json::obj([(
                        "hh",
                        Json::obj([
                            (
                                "hh_methods",
                                Json::Arr(
                                    HH_METHODS
                                        .iter()
                                        .map(|m| Json::str(m.to_string()))
                                        .collect(),
                                ),
                            ),
                            (
                                "request_kinds",
                                Json::Arr(
                                    ACP_REQUEST_KINDS
                                        .iter()
                                        .map(|m| Json::str(m.to_string()))
                                        .collect(),
                                ),
                            ),
                            (
                                "update_kinds",
                                Json::Arr(
                                    V2_UPDATE_KINDS
                                        .iter()
                                        .map(|m| Json::str(m.to_string()))
                                        .collect(),
                                ),
                            ),
                            ("protocol_pin", Json::str(ACP_VERSION)),
                            ("agent_semantic_id", Json::str(agent_id)),
                        ]),
                    )]),
                ),
            ]),
        ),
    ]);
    let mut table_json = Vec::new();
    for r in EVENT_CLASS_LOWERING {
        table_json.push(Json::obj([
            ("class", Json::str(r.class)),
            ("update", Json::str(r.update)),
            ("loss", Json::str(r.loss)),
            ("durable_seq", Json::Bool(r.durable_seq)),
        ]));
    }
    let artefact = Json::obj([
        ("schema", Json::str(ACP_ARTEFACT_SCHEMA)),
        ("protocol", Json::str("acp")),
        ("protocol_version", Json::str(ACP_VERSION)),
        (
            "supported_versions",
            Json::Arr(vec![Json::str(ACP_VERSION), Json::str(ACP_LEGACY_VERSION)]),
        ),
        (
            "binding",
            Json::obj([(
                "acp",
                Json::obj([
                    ("transport", Json::str("attach_session")),
                    ("dialect", Json::str("v2")),
                ]),
            )]),
        ),
        ("initialize_response", initialize_response),
        ("config_options", config_options),
        ("lowering_table", Json::Arr(table_json)),
        (
            "tool_kind_table",
            Json::Obj(
                [
                    ("fs_read", "read"),
                    ("fs_write", "edit"),
                    ("exec", "execute"),
                    ("spawn_process", "execute"),
                    ("model_call", "think"),
                    ("net_egress", "fetch"),
                    ("secret_access", "other"),
                    ("spend", "other"),
                    ("message_human", "other"),
                    ("memory_write", "other"),
                    ("permission_request", "other"),
                ]
                .iter()
                .map(|(k, v)| (k.to_string(), Json::str(*v)))
                .collect::<BTreeMap<_, _>>(),
            ),
        ),
        ("spec_version", Json::str(spec.spec_version.clone())),
        ("target", Json::str("acp")),
    ]);
    Ok((
        artefact,
        LoweringLossReport {
            target: "acp".to_string(),
            target_version: spec.spec_version.clone(),
            entries: acp_losses(linked),
            granularity_ceiling: GranularityCeiling::Configuration,
        },
    ))
}

/// The ACP lowering's typed loss list — every row the session surface
/// cannot carry is named (none silently dropped; the artefact's own
/// granularity ceiling is `configuration`).
fn acp_losses(linked: &LinkedGraph) -> Vec<LossEntry> {
    let mut entries = Vec::new();
    // The v1 compatibility profile drops the unstable extension
    // surface — a `collapsed` entry per unstable row.
    for r in EVENT_CLASS_LOWERING {
        if matches!(r.update, "session/fork" | "compaction_update") {
            entries.push(LossEntry {
                hir_node_id: "acp:v1-profile".to_string(),
                field: r.update.to_string(),
                class: LossKind::NoSlot,
                severity: LossSeverity::Info,
                detail: format!(
                    "v1 compatibility profile drops {} (unstable extension)",
                    r.update
                ),
                debt_ref: None,
            });
        }
    }
    // The session surface is protocol-owned — a compiled tool surface
    // never lowers to an ACP `tools[]` (the class-level fact, reported
    // once at the definition root).
    entries.push(LossEntry {
        hir_node_id: linked
            .sealed
            .document
            .nodes
            .first()
            .map(|n| n.semantic_id())
            .unwrap_or_default(),
        field: "tools".to_string(),
        class: LossKind::Narrowed,
        severity: LossSeverity::Info,
        detail: "ACP session surface is protocol-owned — tool catalogue is not carried".to_string(),
        debt_ref: None,
    });
    entries
}

/// `lift_acp(artefact)` — reconstruct the `agent&run`
/// minimum-carried-set (§3.2.7's lift contract at `configuration`
/// granularity): the agent identity (`agentInfo` + the `_meta.hh`
/// semantic id) + the session-surface declarations
/// (`agentCapabilities`, `configOptions`). Everything the artefact
/// does not carry reports on the caller's loss list — the lift
/// fabricates nothing.
pub fn lift_acp(artefact: &Json) -> Result<crate::target::PartialHIR, CompileError> {
    let bad = |d: &str| CompileError::TargetError {
        detail: d.to_string(),
    };
    if artefact.get("schema").and_then(Json::as_str) != Some(ACP_ARTEFACT_SCHEMA) {
        return Err(bad("artefact is not hh-acp-target/1"));
    }
    let init = artefact
        .get("initialize_response")
        .ok_or_else(|| bad("hh-acp-target/1 carries no initialize_response"))?;
    let agent_info = init.get("agentInfo").cloned().unwrap_or(Json::obj([]));
    let agent_semantic = init
        .get("agentCapabilities")
        .and_then(|c| c.get("_meta"))
        .and_then(|m| m.get("hh"))
        .and_then(|h| h.get("agent_semantic_id"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let recovered = vec![
        Json::obj([
            ("kind", Json::str("agent")),
            ("semantic_id", Json::str(agent_semantic)),
            ("agent_info", agent_info),
            (
                "capabilities",
                init.get("agentCapabilities")
                    .cloned()
                    .unwrap_or(Json::obj([])),
            ),
        ]),
        Json::obj([
            ("kind", Json::str("run")),
            (
                "protocol_version",
                artefact
                    .get("protocol_version")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
            (
                "config_options",
                artefact
                    .get("config_options")
                    .cloned()
                    .unwrap_or(Json::Arr(vec![])),
            ),
        ]),
    ];
    Ok(crate::target::PartialHIR {
        recovered,
        unknown: Vec::new(),
        declared_unverified: Vec::new(),
    })
}
