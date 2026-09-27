//! `HostPorts` — the kernel-facing callback surface the variant host
//! mediates (§8.4 §2 "Host callbacks", the closed set `propose_effect |
//! read_view | request_budget | emit_diagnostic`; ADR-0181 D3). The host
//! enforces the grant/budget guards *before* the port is touched — a port
//! implementation sees only calls the session's sealed Permission covers:
//!
//! - `propose_effect` — the plugin proposes, never executes; the port
//!   returns the `EffectRef` of the proposal entering
//!   `resolve → authorize → …` under the plugin's Permission.
//! - `read_view` — only projections the session's view grants cover;
//!   `peer_plugin_private` and unknown kinds never reach the port.
//! - `request_budget` — the reservation comes back or the call is
//!   `InsufficientBudget`.
//! - `emit_diagnostic` — ephemeral L3; the kernel appends.
//!
//! There is no fifth callback. Anything else the plugin sends is refused at
//! the protocol layer (`SchemaViolation` on decode — the `HostCallback` sum
//! is closed), never reaching here.

use hh_embed_schema::plugin_abi::AbiError;
use hh_wire::json::Json;

/// The session context a callback is attributed to. `binding_id` is the
/// in-flight invocation's binding when one is set — callbacks are
/// session-scoped on the wire (the ABI carries no per-callback binding);
/// attribution for grant checks and ledgering uses the session.
#[derive(Debug, Clone)]
pub struct CallbackCtx {
    /// The host session id (`hello`'s `host_id`).
    pub session_id: String,
    /// The plugin identity (`namespace/name`).
    pub plugin_id: String,
    /// The in-flight invocation's binding, when the callback arrived
    /// mid-invocation.
    pub binding_id: Option<String>,
    /// Whether the callback arrived inside a `guard` invocation (the
    /// `propose_effect`-in-guard refusal — §8.4 §2 guard row).
    pub in_guard: bool,
}

/// The host's callback surface. Every method returns a typed `AbiError` on
/// refusal — the host forwards it as `CallbackResult::Refused`; there is no
/// untyped failure path.
pub trait HostPorts: Send {
    /// `propose_effect(proposal) → EffectRef`. The host has already checked
    /// the proposal claims no authority above `variant` and (inside a
    /// guard) refused outright; the port records the proposal and returns
    /// the ref the kernel will resolve under the plugin's Permission.
    fn propose_effect(&mut self, ctx: &CallbackCtx, proposal: Json) -> Result<String, AbiError>;

    /// `read_view(view_kind, until_seq) → projection`. `view_kind` ∈ the
    /// session's granted view set (checked before dispatch).
    fn read_view(
        &mut self,
        ctx: &CallbackCtx,
        view_kind: &str,
        until_seq: i64,
    ) -> Result<Json, AbiError>;

    /// `request_budget(extent) → reservation_ref`.
    fn request_budget(&mut self, ctx: &CallbackCtx, extent: &Json) -> Result<String, AbiError>;

    /// `emit_diagnostic(record)` — ephemeral; the kernel appends.
    fn emit_diagnostic(&mut self, ctx: &CallbackCtx, record: Json);
}

/// `NullPorts` — every callback refused `NotGranted` (diagnostics
/// acknowledged and dropped). The minimal legal surface — a session whose
/// sealed Permission grants nothing.
pub struct NullPorts;

impl HostPorts for NullPorts {
    fn propose_effect(&mut self, _ctx: &CallbackCtx, _proposal: Json) -> Result<String, AbiError> {
        Err(AbiError::NotGranted)
    }
    fn read_view(
        &mut self,
        _ctx: &CallbackCtx,
        _view_kind: &str,
        _until_seq: i64,
    ) -> Result<Json, AbiError> {
        Err(AbiError::NotGranted)
    }
    fn request_budget(&mut self, _ctx: &CallbackCtx, _extent: &Json) -> Result<String, AbiError> {
        Err(AbiError::InsufficientBudget)
    }
    fn emit_diagnostic(&mut self, _ctx: &CallbackCtx, _record: Json) {}
}

/// `RecordingPorts` — the conformance/test surface: records every call,
/// answers from configured tables. Deterministic and inspectable — the
/// protocol suite asserts on the record.
#[derive(Default)]
pub struct RecordingPorts {
    /// `(ctx.binding_id, proposal)` pairs — every proposed effect.
    pub proposals: Vec<(Option<String>, Json)>,
    /// `(view_kind, until_seq)` requests.
    pub reads: Vec<(String, i64)>,
    /// Budget requests.
    pub budgets: Vec<Json>,
    /// Diagnostic records.
    pub diagnostics: Vec<Json>,
    /// The view table: `kind → projection` (absent kind → `NotGranted`).
    pub views: std::collections::BTreeMap<String, Json>,
    /// Budget verdict (true → `reservation:<n>` granted).
    pub grant_budget: bool,
    /// Propose verdict (true → `effect:<n>` refs).
    pub grant_effects: bool,
}

impl RecordingPorts {
    /// A ports surface granting views and budget but not effects.
    pub fn views_and_budget() -> RecordingPorts {
        RecordingPorts {
            grant_budget: true,
            ..RecordingPorts::default()
        }
    }
}

impl HostPorts for RecordingPorts {
    fn propose_effect(&mut self, ctx: &CallbackCtx, proposal: Json) -> Result<String, AbiError> {
        self.proposals.push((ctx.binding_id.clone(), proposal));
        if self.grant_effects {
            Ok(format!("effect:{}", self.proposals.len()))
        } else {
            Err(AbiError::NotGranted)
        }
    }
    fn read_view(
        &mut self,
        _ctx: &CallbackCtx,
        view_kind: &str,
        until_seq: i64,
    ) -> Result<Json, AbiError> {
        self.reads.push((view_kind.to_string(), until_seq));
        self.views
            .get(view_kind)
            .cloned()
            .ok_or(AbiError::NotGranted)
    }
    fn request_budget(&mut self, _ctx: &CallbackCtx, extent: &Json) -> Result<String, AbiError> {
        self.budgets.push(extent.clone());
        if self.grant_budget {
            Ok(format!("reservation:{}", self.budgets.len()))
        } else {
            Err(AbiError::InsufficientBudget)
        }
    }
    fn emit_diagnostic(&mut self, _ctx: &CallbackCtx, record: Json) {
        self.diagnostics.push(record);
    }
}
