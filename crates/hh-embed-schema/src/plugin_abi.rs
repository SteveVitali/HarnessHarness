//! `plugin_abi/1` — the kernel↔extension binding (spec §8.4; R-2.12.2;
//! ADR-0180/0181; ticket S1.27). This module is the **single schema source**
//! (V6/CC7) for the variant-host protocol: the `{protocol_version, schema_hash,
//! seq, payload}` envelope, the closed verb set, the closed `GuardVerdict` sum
//! (**no `allow`** — removing it is the point, ADR-0181 D6), the closed
//! `HostCallback` set, the closed `BindFailure`/`AbiError` failure sums, and
//! the tri-state capability vocabulary (T-LCD-07).
//!
//! Stage-1 scope (ADR-0184 D2): the *schema* and its codec land now — the
//! variant host that drives it is C0/Stage 2. Nothing here executes or spawns;
//! the payloads are canonical `idp/1`-encodable data. `hh-hosting/1` is a
//! **different binding** (kernel↔hosted participants — CF-208); this schema
//! never references it (AC-R-2.12.2-12; T-LCD-06).

use std::collections::BTreeMap;

use hh_wire::json::Json;

/// The contract name, as it appears in the exported schema `$id` and in
/// `requires.plugin_abi` ranges.
pub const PLUGIN_ABI_NAME: &str = "plugin_abi/1";

/// The contract major version (`protocol_version` on every message; V5).
pub const PLUGIN_ABI_MAJOR: i64 = 1;

// ── TriState (T-LCD-07) ───────────────────────────────────────────────────────

/// The capability vocabulary: `SUPPORTED | UNSUPPORTED | UNKNOWN`. `Unknown` is
/// never coerced — an undeclared capability stays `UNKNOWN`, never silently
/// `UNSUPPORTED` (T-LCD-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TriState {
    /// Observed/declared support.
    Supported,
    /// Declared non-support.
    Unsupported,
    /// Not declared — never coerced.
    Unknown,
}

impl TriState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TriState::Supported => "SUPPORTED",
            TriState::Unsupported => "UNSUPPORTED",
            TriState::Unknown => "UNKNOWN",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<TriState> {
        Some(match s {
            "SUPPORTED" => TriState::Supported,
            "UNSUPPORTED" => TriState::Unsupported,
            "UNKNOWN" => TriState::Unknown,
            _ => return None,
        })
    }
}

// ── GuardVerdict (ADR-0181 D6) ────────────────────────────────────────────────

/// The narrowing arm of `GuardVerdict::narrow` — `deny(reason) | ask(reason) |
/// attenuate(scope ⊂ requested)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Narrow {
    /// Refuse the proposal.
    Deny {
        /// The refusal reason.
        reason: String,
    },
    /// Escalate to an approval request.
    Ask {
        /// The escalation reason.
        reason: String,
    },
    /// Admit a strict subset of the requested scope (`scope ⊂ requested`).
    Attenuate {
        /// The narrowed scope (a canonical document).
        scope: Json,
    },
}

/// `GuardVerdict ∈ {pass, annotate(Text ≤ external), narrow(…),
/// propose_replacement(Proposal at ≤ hook authority), no_decision}` — the
/// closed output sum of `guard(binding, …)` (§8.4 §2/§3; ADR-0181 D6).
///
/// There is **no `allow` member and never will be**: guards narrow only — a
/// hook cannot widen, approve or rewrite; `propose_replacement` re-enters
/// `resolve → authorize` as a *new* proposal with `derived_from` the hook.
/// Adding a verdict is a dialect bump. A non-`GuardVerdict` output (or an
/// `allow` spelling) is `AuthorityCrossing` — `security.extension.violation`
/// appended, run proceeds with the class fallback.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardVerdict {
    /// The proposal proceeds unchanged.
    Pass,
    /// Attach an annotation (`Text` at `authority ≤ external` — carried as its
    /// canonical rendering; never model-facing beyond that authority).
    Annotate(String),
    /// Narrow the decision.
    Narrow(Narrow),
    /// Propose a replacement proposal (≤ hook authority; re-enters
    /// `resolve → authorize` under a fresh `effect_id`).
    ProposeReplacement(Json),
    /// Timeout / crash / malformed output — the identity for optional guards,
    /// `deny` for `required` ones ([`meet_verdicts`] applies the rule).
    NoDecision,
}

impl GuardVerdict {
    /// The canonical JSON form (`{verdict, …}` — deterministic member set).
    pub fn to_json(&self) -> Json {
        match self {
            GuardVerdict::Pass => Json::obj([("verdict", Json::str("pass"))]),
            GuardVerdict::Annotate(t) => Json::obj([
                ("verdict", Json::str("annotate")),
                ("text", Json::str(t.clone())),
            ]),
            GuardVerdict::Narrow(n) => {
                let inner = match n {
                    Narrow::Deny { reason } => Json::obj([
                        ("kind", Json::str("deny")),
                        ("reason", Json::str(reason.clone())),
                    ]),
                    Narrow::Ask { reason } => Json::obj([
                        ("kind", Json::str("ask")),
                        ("reason", Json::str(reason.clone())),
                    ]),
                    Narrow::Attenuate { scope } => {
                        Json::obj([("kind", Json::str("attenuate")), ("scope", scope.clone())])
                    }
                };
                Json::obj([("verdict", Json::str("narrow")), ("narrow", inner)])
            }
            GuardVerdict::ProposeReplacement(p) => Json::obj([
                ("verdict", Json::str("propose_replacement")),
                ("proposal", p.clone()),
            ]),
            GuardVerdict::NoDecision => Json::obj([("verdict", Json::str("no_decision"))]),
        }
    }

    /// Decode a verdict. Anything else — including an `allow` spelling — is not
    /// a `GuardVerdict` (the caller reports `AuthorityCrossing`). The envelope
    /// `verb` member is tolerated so a `guard_result` payload decodes
    /// in place.
    pub fn from_json(j: &Json) -> Option<GuardVerdict> {
        let Json::Obj(m) = j else { return None };
        for k in m.keys() {
            if !matches!(
                k.as_str(),
                "verdict" | "text" | "narrow" | "proposal" | "verb" | "ext"
            ) {
                return None;
            }
        }
        match m.get("verdict")?.as_str()? {
            "pass" => Some(GuardVerdict::Pass),
            "annotate" => m
                .get("text")?
                .as_str()
                .map(|t| GuardVerdict::Annotate(t.to_string())),
            "no_decision" => Some(GuardVerdict::NoDecision),
            "propose_replacement" => m
                .get("proposal")
                .map(|p| GuardVerdict::ProposeReplacement(p.clone())),
            "narrow" => {
                let Json::Obj(nm) = m.get("narrow")? else {
                    return None;
                };
                match nm.get("kind")?.as_str()? {
                    "deny" => nm.get("reason")?.as_str().map(|r| {
                        GuardVerdict::Narrow(Narrow::Deny {
                            reason: r.to_string(),
                        })
                    }),
                    "ask" => nm.get("reason")?.as_str().map(|r| {
                        GuardVerdict::Narrow(Narrow::Ask {
                            reason: r.to_string(),
                        })
                    }),
                    "attenuate" => nm
                        .get("scope")
                        .map(|s| GuardVerdict::Narrow(Narrow::Attenuate { scope: s.clone() })),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// The meet order (§8.4 §2: `deny > ask > attenuate > annotate > pass`,
/// order-independent so parallel and sequential execution agree).
/// `propose_replacement` ranks between `attenuate` and `annotate` — it
/// substitutes the proposal rather than narrowing the decision (interim
/// ordering; a per-point policy may refine it at Stage 2 — DF-S1.27 candidate).
fn verdict_rank(v: &GuardVerdict) -> u8 {
    match v {
        GuardVerdict::Narrow(Narrow::Deny { .. }) => 5,
        GuardVerdict::Narrow(Narrow::Ask { .. }) => 4,
        GuardVerdict::Narrow(Narrow::Attenuate { .. }) => 3,
        GuardVerdict::ProposeReplacement(_) => 2,
        GuardVerdict::Annotate(_) => 1,
        GuardVerdict::Pass | GuardVerdict::NoDecision => 0,
    }
}

/// Meet-compose the verdicts of the hooks bound at one decision point
/// (§8.4 §2; AC-5's pure half): the highest-rank verdict wins; ties between
/// `narrow`/`propose_replacement`/`annotate` at the same rank resolve to the
/// **lexically smallest canonical encoding** so composition is total,
/// deterministic and order-independent. `no_decision` is the identity —
/// unless every verdict is `no_decision` and the rule is `required`, where the
/// result is `deny("required hook produced no decision")`.
pub fn meet_verdicts(verdicts: &[GuardVerdict], required: bool) -> GuardVerdict {
    let effective: Vec<&GuardVerdict> = verdicts
        .iter()
        .filter(|v| **v != GuardVerdict::NoDecision)
        .collect();
    if effective.is_empty() {
        if required && !verdicts.is_empty() {
            return GuardVerdict::Narrow(Narrow::Deny {
                reason: "required hook produced no decision".to_string(),
            });
        }
        return GuardVerdict::Pass;
    }
    let top = effective.iter().map(|v| verdict_rank(v)).max().unwrap_or(0);
    let mut winners: Vec<&GuardVerdict> = effective
        .iter()
        .copied()
        .filter(|v| verdict_rank(v) == top)
        .collect();
    winners.sort_by_key(|v| v.to_json().to_canonical_string());
    winners[0].clone()
}

// ── BindFailure / AbiError (the closed failure sums) ──────────────────────────

/// `BindFailure{not_installed | trust_denied | locality_unsupported |
/// contract_mismatch | isolation_unavailable}` (§8.4 §2 `bind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindFailure {
    /// The variant record is not installed/resolved.
    NotInstalled,
    /// The extension's trust record denies the bind.
    TrustDenied,
    /// The placement is not executable here (Stage 1: anything but
    /// `in_process`; Stage 2+: an unavailable locality).
    LocalityUnsupported,
    /// `contract_version`/`contract_range` miss at bind time.
    ContractMismatch,
    /// The declared isolation cannot be provisioned.
    IsolationUnavailable,
}

impl BindFailure {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BindFailure::NotInstalled => "not_installed",
            BindFailure::TrustDenied => "trust_denied",
            BindFailure::LocalityUnsupported => "locality_unsupported",
            BindFailure::ContractMismatch => "contract_mismatch",
            BindFailure::IsolationUnavailable => "isolation_unavailable",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<BindFailure> {
        Some(match s {
            "not_installed" => BindFailure::NotInstalled,
            "trust_denied" => BindFailure::TrustDenied,
            "locality_unsupported" => BindFailure::LocalityUnsupported,
            "contract_mismatch" => BindFailure::ContractMismatch,
            "isolation_unavailable" => BindFailure::IsolationUnavailable,
            _ => return None,
        })
    }
}

/// The closed protocol-error sum (`Refused` payloads and the `invoke`/
/// callback `TypedFailure`s — §8.4 §2 error columns). `ContractIncompatible`
/// is **not** here — it is a `resolve`-time failure raised before any plugin
/// message exists (AC-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiError {
    /// `protocol_version`/`plugin_abi` range miss at `hello` (V5).
    ProtocolVersionMismatch,
    /// The plugin's `version_id` ≠ the sealed pin (V5).
    PinMismatch,
    /// A message fails the schema (unknown member, wrong type).
    SchemaViolation,
    /// `request_budget` / `invoke` reservation miss (08.2).
    InsufficientBudget,
    /// The invocation did not honour `deadline`.
    InvocationTimeout,
    /// The plugin does not implement the class operation.
    UnhandledOperation,
    /// The plugin process died mid-call.
    PluginCrashed,
    /// A message attempted to carry authority inward — a `Permission`, a
    /// label, a `KernelDecision`, an `allow` verdict, a rewritten proposal
    /// (V1; `security.extension.violation`).
    AuthorityCrossing,
    /// A callback named a grant the plugin does not hold.
    NotGranted,
    /// The binding/host/invocation id names nothing live.
    UnknownBinding,
    /// The session detached (crash between messages); re-`hello` from the
    /// ledger.
    SessionDetached,
}

impl AbiError {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AbiError::ProtocolVersionMismatch => "ProtocolVersionMismatch",
            AbiError::PinMismatch => "PinMismatch",
            AbiError::SchemaViolation => "SchemaViolation",
            AbiError::InsufficientBudget => "InsufficientBudget",
            AbiError::InvocationTimeout => "InvocationTimeout",
            AbiError::UnhandledOperation => "UnhandledOperation",
            AbiError::PluginCrashed => "PluginCrashed",
            AbiError::AuthorityCrossing => "AuthorityCrossing",
            AbiError::NotGranted => "NotGranted",
            AbiError::UnknownBinding => "UnknownBinding",
            AbiError::SessionDetached => "SessionDetached",
        }
    }

    /// Every error kind, in declaration order.
    pub const ALL: [AbiError; 11] = [
        AbiError::ProtocolVersionMismatch,
        AbiError::PinMismatch,
        AbiError::SchemaViolation,
        AbiError::InsufficientBudget,
        AbiError::InvocationTimeout,
        AbiError::UnhandledOperation,
        AbiError::PluginCrashed,
        AbiError::AuthorityCrossing,
        AbiError::NotGranted,
        AbiError::UnknownBinding,
        AbiError::SessionDetached,
    ];
}

// ── HostCallback (closed — ADR-0181 D3) ───────────────────────────────────────

/// The closed plugin→kernel callback set: `propose_effect`, `read_view`,
/// `request_budget`, `emit_diagnostic`. **No other plugin→kernel operation
/// exists** — a message naming anything else is refused (`AuthorityCrossing`/
/// `SchemaViolation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCallback {
    /// `propose_effect(Proposal) → EffectRef` — the only way plugin code causes
    /// a world effect (enters `resolve → authorize → prepare → commit →
    /// execute → capture → observe` under the plugin's Permission).
    ProposeEffect,
    /// `read_view(view_kind, until_seq) → projection` — only projections the
    /// plugin's grants cover; never raw handles, Π, or another plugin's
    /// private records.
    ReadView,
    /// `request_budget(amount) → reservation | InsufficientBudget`.
    RequestBudget,
    /// `emit_diagnostic(record)` — ephemeral L3; plugins propose, the kernel
    /// appends.
    EmitDiagnostic,
}

impl HostCallback {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HostCallback::ProposeEffect => "propose_effect",
            HostCallback::ReadView => "read_view",
            HostCallback::RequestBudget => "request_budget",
            HostCallback::EmitDiagnostic => "emit_diagnostic",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<HostCallback> {
        Some(match s {
            "propose_effect" => HostCallback::ProposeEffect,
            "read_view" => HostCallback::ReadView,
            "request_budget" => HostCallback::RequestBudget,
            "emit_diagnostic" => HostCallback::EmitDiagnostic,
            _ => return None,
        })
    }

    /// Every callback, in declaration order.
    pub const ALL: [HostCallback; 4] = [
        HostCallback::ProposeEffect,
        HostCallback::ReadView,
        HostCallback::RequestBudget,
        HostCallback::EmitDiagnostic,
    ];
}

// ── Envelope + payload ────────────────────────────────────────────────────────

/// The `plugin_abi/1` message envelope `{protocol_version, schema_hash, seq,
/// payload}` (§8.4 §3; V2/V5: every message carries all three headers; the
/// `seq` resume and the sealed `schema_hash` are checked per message).
#[derive(Debug, Clone, PartialEq)]
pub struct AbiEnvelope {
    /// `plugin_abi` protocol version (`PLUGIN_ABI_MAJOR` at this dialect).
    pub protocol_version: i64,
    /// The content address of the canonical schema export (V5/V6 — asserted
    /// both ways).
    pub schema_hash: String,
    /// The per-session sequence number (`seq`-based resume).
    pub seq: i64,
    /// The verb payload.
    pub payload: AbiPayload,
}

/// The closed `payload` sum — the §8.4 §2 verb set. Member spellings are the
/// verb names; the payload object is `{verb, …members}`.
#[derive(Debug, Clone, PartialEq)]
pub enum AbiPayload {
    /// `hello` — the probe-first handshake.
    Hello(HelloParams),
    /// `hello` result — `{host_id, negotiated, registry_snapshot_id,
    /// kernel_capabilities}`.
    HelloAck(HelloResult),
    /// `bind` — atomic per slot.
    Bind(BindParams),
    /// `bind` result — `{binding_id}` or a [`BindFailure`].
    BindResult(BindResult),
    /// `invoke` — one class operation.
    Invoke(InvokeParams),
    /// `invoke` result — `{outputs}` or a typed failure.
    InvokeResult(InvokeResult),
    /// `stream` — `seq`-resumable document stream.
    Stream(StreamParams),
    /// One stream item (a canonical document).
    StreamItem(Json),
    /// `cancel(invocation_id)` — honoured within the deadline.
    Cancel {
        /// The invocation to cancel.
        invocation_id: String,
    },
    /// `unbind(binding_id)`.
    Unbind {
        /// The binding to release.
        binding_id: String,
    },
    /// `close(host_id)` — end the session.
    Close {
        /// The host session to close.
        host_id: String,
    },
    /// A plugin→kernel callback invocation (the closed [`HostCallback`] set).
    Callback(CallbackCall),
    /// The kernel's answer to a callback.
    CallbackResult(CallbackResult),
    /// `guard` — a decision-point invocation (`invoke(binding, guard, …)`).
    GuardInvoke(GuardParams),
    /// The guard's verdict.
    GuardResult(GuardVerdict),
    /// `run_conformance` — the `registry_ci` driver.
    RunConformance(ConformanceParams),
    /// The conformance report (`produced_by = registry_ci`).
    ConformanceResult {
        /// The report document.
        report: Json,
    },
    /// A protocol-level refusal (the [`AbiError`] sum).
    Refused(AbiError),
}

/// `hello` params (§8.4 §2): `{plugin_abi_version, plugin{version_id,
/// content}, contract_versions_offered: [ContractRef], capabilities:
/// map<string, TriState>}`.
#[derive(Debug, Clone, PartialEq)]
pub struct HelloParams {
    /// The plugin_abi version the plugin speaks (its declared range's choice).
    pub plugin_abi_version: String,
    /// `plugin.version_id` — must equal the sealed pin (V5).
    pub plugin_version_id: String,
    /// `plugin.content` — the package content address.
    pub plugin_content: String,
    /// The contract versions the plugin offers (`ContractRef` canonical JSON —
    /// `hh_plugin::ContractRef::from_json` decodes them; the ABI schema carries
    /// them as canonical objects, CC7).
    pub contract_versions_offered: Vec<Json>,
    /// Tri-state capabilities the plugin declares (`SUPPORTED|UNSUPPORTED|
    /// UNKNOWN` — additive contract surface, T-LCD-07).
    pub capabilities: BTreeMap<String, TriState>,
}

/// `hello` result (§8.4 §2).
#[derive(Debug, Clone, PartialEq)]
pub struct HelloResult {
    /// The host session id.
    pub host_id: String,
    /// The negotiated `plugin_abi` version (highest common — AC-2).
    pub plugin_abi_version: String,
    /// `negotiated.contract_versions_chosen` — `contract:<kind>:<id>` → the
    /// chosen version (what `check_compatibility` computed).
    pub contract_versions_chosen: BTreeMap<String, String>,
    /// The registry snapshot the session resolves against.
    pub registry_snapshot_id: String,
    /// The kernel's own capability map.
    pub kernel_capabilities: BTreeMap<String, TriState>,
}

/// `bind` params (§8.4 §2 — T-LCD-08 by construction: profile and account are
/// inputs).
#[derive(Debug, Clone, PartialEq)]
pub struct BindParams {
    /// The slot being bound.
    pub slot: String,
    /// The class id.
    pub class_id: String,
    /// The class contract version.
    pub contract_version: String,
    /// The variant `VersionedRef` (canonical JSON).
    pub variant: Json,
    /// Per-variant parameters (`parameters` in the manifest).
    pub params: Json,
    /// `profile: ModelProfileRef`.
    pub profile: Json,
    /// `account: ResourceAccountRef`.
    pub account: Json,
    /// `budget: BudgetNodeRef`.
    pub budget: Json,
    /// The variant's placement (the `BindingRecord.placement` claim —
    /// recorded on `lifecycle.component.bound`; the *evidence* is the event).
    pub placement: String,
}

/// `bind` result.
#[derive(Debug, Clone, PartialEq)]
pub enum BindResult {
    /// Bound — the `binding_id`.
    Bound {
        /// The new binding id (the `BindingRecord` handle).
        binding_id: String,
    },
    /// Refused — a closed [`BindFailure`].
    Failed(BindFailure),
}

/// `invoke` params (§8.4 §2).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeParams {
    /// The binding.
    pub binding_id: String,
    /// The class operation.
    pub operation: String,
    /// The inputs — the class's declared documents, canonical.
    pub inputs: Vec<Json>,
    /// `reservation: BudgetReservationRef` (reserve-before-spend).
    pub reservation: String,
    /// The deadline (kernel clock units — deterministic, no wall clock).
    pub deadline: i64,
}

/// `invoke` result — `outputs` (canonical documents, byte-identical — V2) or a
/// typed failure.
#[derive(Debug, Clone, PartialEq)]
pub enum InvokeResult {
    /// The outputs (the class's declared documents).
    Outputs(Vec<Json>),
    /// A typed failure ([`AbiError`]).
    Failed(AbiError),
}

/// `stream` params — `invoke` plus resumability.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamParams {
    /// The invocation descriptor.
    pub invoke: InvokeParams,
    /// Resume from this seq (0 = fresh).
    pub resume_from_seq: i64,
}

/// A plugin→kernel callback call: `{callback, args}` over the closed
/// [`HostCallback`] set.
#[derive(Debug, Clone, PartialEq)]
pub struct CallbackCall {
    /// Which callback.
    pub callback: HostCallback,
    /// Its arguments (`Proposal` / `{view_kind, until_seq}` / `{amount}` /
    /// `record` — canonical JSON).
    pub args: Json,
}

/// The kernel's answer to a callback (a closed sum — `NotGranted`/
/// `AuthorityCrossing`/`InsufficientBudget` are typed).
#[derive(Debug, Clone, PartialEq)]
pub enum CallbackResult {
    /// `propose_effect` → the new `EffectRef`.
    EffectRef(String),
    /// `read_view` → the projection document.
    Projection(Json),
    /// `request_budget` → the reservation ref.
    Reservation(String),
    /// `emit_diagnostic` → acknowledged.
    Acknowledged,
    /// A typed refusal.
    Refused(AbiError),
}

/// `guard` params — `{binding, decision_point, subject, ledger_cursor}`; the
/// subject is one of `proposal | cue | context_plan` (a closed sum).
#[derive(Debug, Clone, PartialEq)]
pub struct GuardParams {
    /// The hook's binding.
    pub binding_id: String,
    /// The decision point (β) id.
    pub decision_point: String,
    /// The subject kind — `proposal` | `cue` | `context_plan`.
    pub subject_kind: GuardSubjectKind,
    /// The subject document (canonical).
    pub subject: Json,
    /// The ledger cursor the guard may `read_view` up to.
    pub ledger_cursor: i64,
}

/// The `guard` subject kinds (§8.4 §2 `proposal | cue | context_plan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardSubjectKind {
    /// An effect proposal.
    Proposal,
    /// A control cue.
    Cue,
    /// A context plan.
    ContextPlan,
}

impl GuardSubjectKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            GuardSubjectKind::Proposal => "proposal",
            GuardSubjectKind::Cue => "cue",
            GuardSubjectKind::ContextPlan => "context_plan",
        }
    }
    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<GuardSubjectKind> {
        Some(match s {
            "proposal" => GuardSubjectKind::Proposal,
            "cue" => GuardSubjectKind::Cue,
            "context_plan" => GuardSubjectKind::ContextPlan,
            _ => return None,
        })
    }
}

/// `run_conformance` params.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceParams {
    /// The plugin `VersionedRef` under test.
    pub plugin: Json,
    /// The layers to run (`protocol`, `isolation`, `class`, `reach` — the four
    /// conformance layers).
    pub layers: Vec<String>,
    /// The placement the report covers.
    pub placement: String,
}

impl AbiPayload {
    /// The verb spelling (the `payload.verb` member).
    pub fn verb(&self) -> &'static str {
        match self {
            AbiPayload::Hello(_) => "hello",
            AbiPayload::HelloAck(_) => "hello_ack",
            AbiPayload::Bind(_) => "bind",
            AbiPayload::BindResult(_) => "bind_result",
            AbiPayload::Invoke(_) => "invoke",
            AbiPayload::InvokeResult(_) => "invoke_result",
            AbiPayload::Stream(_) => "stream",
            AbiPayload::StreamItem(_) => "stream_item",
            AbiPayload::Cancel { .. } => "cancel",
            AbiPayload::Unbind { .. } => "unbind",
            AbiPayload::Close { .. } => "close",
            AbiPayload::Callback(_) => "callback",
            AbiPayload::CallbackResult(_) => "callback_result",
            AbiPayload::GuardInvoke(_) => "guard",
            AbiPayload::GuardResult(_) => "guard_result",
            AbiPayload::RunConformance(_) => "run_conformance",
            AbiPayload::ConformanceResult { .. } => "conformance_result",
            AbiPayload::Refused(_) => "refused",
        }
    }

    /// The payload's canonical object form (`{verb, …members}`).
    pub fn to_json(&self) -> Json {
        let verb = Json::str(self.verb());
        let members = |extra: Vec<(&str, Json)>| {
            let mut m = BTreeMap::new();
            m.insert("verb".to_string(), verb.clone());
            for (k, v) in extra {
                m.insert(k.to_string(), v);
            }
            Json::Obj(m)
        };
        match self {
            AbiPayload::Hello(p) => {
                let caps: BTreeMap<String, Json> = p
                    .capabilities
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                    .collect();
                members(vec![
                    (
                        "plugin_abi_version",
                        Json::str(p.plugin_abi_version.clone()),
                    ),
                    (
                        "plugin",
                        Json::obj([
                            ("version_id", Json::str(p.plugin_version_id.clone())),
                            ("content", Json::str(p.plugin_content.clone())),
                        ]),
                    ),
                    (
                        "contract_versions_offered",
                        Json::Arr(p.contract_versions_offered.clone()),
                    ),
                    ("capabilities", Json::Obj(caps)),
                ])
            }
            AbiPayload::HelloAck(r) => {
                let chosen: BTreeMap<String, Json> = r
                    .contract_versions_chosen
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect();
                let caps: BTreeMap<String, Json> = r
                    .kernel_capabilities
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                    .collect();
                members(vec![
                    ("host_id", Json::str(r.host_id.clone())),
                    (
                        "negotiated",
                        Json::obj([
                            (
                                "plugin_abi_version",
                                Json::str(r.plugin_abi_version.clone()),
                            ),
                            ("contract_versions_chosen", Json::Obj(chosen)),
                        ]),
                    ),
                    (
                        "registry_snapshot_id",
                        Json::str(r.registry_snapshot_id.clone()),
                    ),
                    ("kernel_capabilities", Json::Obj(caps)),
                ])
            }
            AbiPayload::Bind(p) => members(vec![
                ("slot", Json::str(p.slot.clone())),
                ("class_id", Json::str(p.class_id.clone())),
                ("contract_version", Json::str(p.contract_version.clone())),
                ("variant", p.variant.clone()),
                ("params", p.params.clone()),
                ("profile", p.profile.clone()),
                ("account", p.account.clone()),
                ("budget", p.budget.clone()),
                ("placement", Json::str(p.placement.clone())),
            ]),
            AbiPayload::BindResult(BindResult::Bound { binding_id }) => {
                members(vec![("binding_id", Json::str(binding_id.clone()))])
            }
            AbiPayload::BindResult(BindResult::Failed(f)) => {
                members(vec![("failure", Json::str(f.as_str()))])
            }
            AbiPayload::Invoke(p) => members(vec![
                ("binding_id", Json::str(p.binding_id.clone())),
                ("operation", Json::str(p.operation.clone())),
                ("inputs", Json::Arr(p.inputs.clone())),
                ("reservation", Json::str(p.reservation.clone())),
                ("deadline", Json::Int(p.deadline)),
            ]),
            AbiPayload::InvokeResult(InvokeResult::Outputs(o)) => {
                members(vec![("outputs", Json::Arr(o.clone()))])
            }
            AbiPayload::InvokeResult(InvokeResult::Failed(e)) => {
                members(vec![("failure", Json::str(e.as_str()))])
            }
            AbiPayload::Stream(p) => members(vec![
                ("invoke", invoke_json(&p.invoke)),
                ("resume_from_seq", Json::Int(p.resume_from_seq)),
            ]),
            AbiPayload::StreamItem(d) => members(vec![("document", d.clone())]),
            AbiPayload::Cancel { invocation_id } => {
                members(vec![("invocation_id", Json::str(invocation_id.clone()))])
            }
            AbiPayload::Unbind { binding_id } => {
                members(vec![("binding_id", Json::str(binding_id.clone()))])
            }
            AbiPayload::Close { host_id } => members(vec![("host_id", Json::str(host_id.clone()))]),
            AbiPayload::Callback(c) => members(vec![
                ("callback", Json::str(c.callback.as_str())),
                ("args", c.args.clone()),
            ]),
            AbiPayload::CallbackResult(r) => members(vec![match r {
                CallbackResult::EffectRef(e) => ("effect_ref", Json::str(e.clone())),
                CallbackResult::Projection(p) => ("projection", p.clone()),
                CallbackResult::Reservation(r) => ("reservation", Json::str(r.clone())),
                CallbackResult::Acknowledged => ("acknowledged", Json::Bool(true)),
                CallbackResult::Refused(e) => ("failure", Json::str(e.as_str())),
            }]),
            AbiPayload::GuardInvoke(p) => members(vec![
                ("binding_id", Json::str(p.binding_id.clone())),
                ("decision_point", Json::str(p.decision_point.clone())),
                ("subject_kind", Json::str(p.subject_kind.as_str())),
                ("subject", p.subject.clone()),
                ("ledger_cursor", Json::Int(p.ledger_cursor)),
            ]),
            AbiPayload::GuardResult(v) => {
                let Json::Obj(m) = v.to_json() else {
                    unreachable!("GuardVerdict::to_json is an object")
                };
                let mut o = BTreeMap::new();
                o.insert("verb".to_string(), verb);
                o.extend(m);
                Json::Obj(o)
            }
            AbiPayload::RunConformance(p) => members(vec![
                ("plugin", p.plugin.clone()),
                (
                    "layers",
                    Json::Arr(p.layers.iter().map(|l| Json::str(l.clone())).collect()),
                ),
                ("placement", Json::str(p.placement.clone())),
            ]),
            AbiPayload::ConformanceResult { report } => members(vec![("report", report.clone())]),
            AbiPayload::Refused(e) => members(vec![("error", Json::str(e.as_str()))]),
        }
    }
}

fn invoke_json(p: &InvokeParams) -> Json {
    Json::obj([
        ("binding_id", Json::str(p.binding_id.clone())),
        ("operation", Json::str(p.operation.clone())),
        ("inputs", Json::Arr(p.inputs.clone())),
        ("reservation", Json::str(p.reservation.clone())),
        ("deadline", Json::Int(p.deadline)),
    ])
}

impl AbiEnvelope {
    /// The canonical envelope object.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("protocol_version", Json::Int(self.protocol_version)),
            ("schema_hash", Json::str(self.schema_hash.clone())),
            ("seq", Json::Int(self.seq)),
            ("payload", self.payload.to_json()),
        ])
    }

    /// Decode an envelope (strict — the three headers are required ints/
    /// strings; `payload.verb` must be in the closed sum; unknown envelope
    /// members are `SchemaViolation`, V2).
    pub fn from_json(j: &Json) -> Result<AbiEnvelope, AbiError> {
        let Json::Obj(m) = j else {
            return Err(AbiError::SchemaViolation);
        };
        for k in m.keys() {
            if !matches!(
                k.as_str(),
                "protocol_version" | "schema_hash" | "seq" | "payload"
            ) {
                return Err(AbiError::SchemaViolation);
            }
        }
        let protocol_version = match m.get("protocol_version") {
            Some(Json::Int(n)) => *n,
            _ => return Err(AbiError::SchemaViolation),
        };
        let schema_hash = m
            .get("schema_hash")
            .and_then(Json::as_str)
            .ok_or(AbiError::SchemaViolation)?
            .to_string();
        let seq = match m.get("seq") {
            Some(Json::Int(n)) => *n,
            _ => return Err(AbiError::SchemaViolation),
        };
        let payload = decode_payload(m.get("payload").ok_or(AbiError::SchemaViolation)?)?;
        Ok(AbiEnvelope {
            protocol_version,
            schema_hash,
            seq,
            payload,
        })
    }
}

fn decode_payload(j: &Json) -> Result<AbiPayload, AbiError> {
    let Json::Obj(m) = j else {
        return Err(AbiError::SchemaViolation);
    };
    let verb = m
        .get("verb")
        .and_then(Json::as_str)
        .ok_or(AbiError::SchemaViolation)?;
    let s = |k: &str| m.get(k).and_then(Json::as_str).map(str::to_string);
    let i = |k: &str| match m.get(k) {
        Some(Json::Int(n)) => Some(*n),
        _ => None,
    };
    let tri_map = |k: &str| -> Option<BTreeMap<String, TriState>> {
        let Json::Obj(cm) = m.get(k)? else {
            return None;
        };
        cm.iter()
            .map(|(k, v)| Some((k.clone(), TriState::parse(v.as_str()?)?)))
            .collect()
    };
    let invoke_of = |v: &Json| -> Option<InvokeParams> {
        let Json::Obj(im) = v else { return None };
        let Json::Arr(inputs) = im.get("inputs")? else {
            return None;
        };
        Some(InvokeParams {
            binding_id: im.get("binding_id")?.as_str()?.to_string(),
            operation: im.get("operation")?.as_str()?.to_string(),
            inputs: inputs.clone(),
            reservation: im.get("reservation")?.as_str()?.to_string(),
            deadline: im.get("deadline")?.as_int()?,
        })
    };
    match verb {
        "hello" => {
            let Json::Obj(pm) = m.get("plugin").ok_or(AbiError::SchemaViolation)? else {
                return Err(AbiError::SchemaViolation);
            };
            let offered = match m.get("contract_versions_offered") {
                Some(Json::Arr(a)) => a.clone(),
                _ => return Err(AbiError::SchemaViolation),
            };
            Ok(AbiPayload::Hello(HelloParams {
                plugin_abi_version: s("plugin_abi_version").ok_or(AbiError::SchemaViolation)?,
                plugin_version_id: pm
                    .get("version_id")
                    .and_then(Json::as_str)
                    .ok_or(AbiError::SchemaViolation)?
                    .to_string(),
                plugin_content: pm
                    .get("content")
                    .and_then(Json::as_str)
                    .ok_or(AbiError::SchemaViolation)?
                    .to_string(),
                contract_versions_offered: offered,
                capabilities: tri_map("capabilities").ok_or(AbiError::SchemaViolation)?,
            }))
        }
        "hello_ack" => {
            let Json::Obj(nm) = m.get("negotiated").ok_or(AbiError::SchemaViolation)? else {
                return Err(AbiError::SchemaViolation);
            };
            let Json::Obj(chm) = nm
                .get("contract_versions_chosen")
                .ok_or(AbiError::SchemaViolation)?
            else {
                return Err(AbiError::SchemaViolation);
            };
            let mut chosen = BTreeMap::new();
            for (k, v) in chm {
                chosen.insert(
                    k.clone(),
                    v.as_str().ok_or(AbiError::SchemaViolation)?.to_string(),
                );
            }
            Ok(AbiPayload::HelloAck(HelloResult {
                host_id: s("host_id").ok_or(AbiError::SchemaViolation)?,
                plugin_abi_version: nm
                    .get("plugin_abi_version")
                    .and_then(Json::as_str)
                    .ok_or(AbiError::SchemaViolation)?
                    .to_string(),
                contract_versions_chosen: chosen,
                registry_snapshot_id: s("registry_snapshot_id").ok_or(AbiError::SchemaViolation)?,
                kernel_capabilities: tri_map("kernel_capabilities")
                    .ok_or(AbiError::SchemaViolation)?,
            }))
        }
        "bind" => Ok(AbiPayload::Bind(BindParams {
            slot: s("slot").ok_or(AbiError::SchemaViolation)?,
            class_id: s("class_id").ok_or(AbiError::SchemaViolation)?,
            contract_version: s("contract_version").ok_or(AbiError::SchemaViolation)?,
            variant: m.get("variant").cloned().unwrap_or(Json::Null),
            params: m.get("params").cloned().unwrap_or(Json::Null),
            profile: m.get("profile").cloned().unwrap_or(Json::Null),
            account: m.get("account").cloned().unwrap_or(Json::Null),
            budget: m.get("budget").cloned().unwrap_or(Json::Null),
            placement: s("placement").ok_or(AbiError::SchemaViolation)?,
        })),
        "bind_result" => {
            if let Some(id) = s("binding_id") {
                Ok(AbiPayload::BindResult(BindResult::Bound { binding_id: id }))
            } else {
                Ok(AbiPayload::BindResult(BindResult::Failed(
                    BindFailure::parse(&s("failure").ok_or(AbiError::SchemaViolation)?)
                        .ok_or(AbiError::SchemaViolation)?,
                )))
            }
        }
        "invoke" => Ok(AbiPayload::Invoke(
            invoke_of(j).ok_or(AbiError::SchemaViolation)?,
        )),
        "invoke_result" => {
            if let Some(Json::Arr(o)) = m.get("outputs") {
                Ok(AbiPayload::InvokeResult(InvokeResult::Outputs(o.clone())))
            } else {
                let e = AbiError::ALL
                    .iter()
                    .find(|e| e.as_str() == s("failure").unwrap_or_default())
                    .copied()
                    .ok_or(AbiError::SchemaViolation)?;
                Ok(AbiPayload::InvokeResult(InvokeResult::Failed(e)))
            }
        }
        "stream" => Ok(AbiPayload::Stream(StreamParams {
            invoke: invoke_of(m.get("invoke").ok_or(AbiError::SchemaViolation)?)
                .ok_or(AbiError::SchemaViolation)?,
            resume_from_seq: i("resume_from_seq").ok_or(AbiError::SchemaViolation)?,
        })),
        "stream_item" => Ok(AbiPayload::StreamItem(
            m.get("document")
                .cloned()
                .ok_or(AbiError::SchemaViolation)?,
        )),
        "cancel" => Ok(AbiPayload::Cancel {
            invocation_id: s("invocation_id").ok_or(AbiError::SchemaViolation)?,
        }),
        "unbind" => Ok(AbiPayload::Unbind {
            binding_id: s("binding_id").ok_or(AbiError::SchemaViolation)?,
        }),
        "close" => Ok(AbiPayload::Close {
            host_id: s("host_id").ok_or(AbiError::SchemaViolation)?,
        }),
        "callback" => Ok(AbiPayload::Callback(CallbackCall {
            callback: HostCallback::parse(&s("callback").ok_or(AbiError::SchemaViolation)?)
                .ok_or(AbiError::SchemaViolation)?,
            args: m.get("args").cloned().unwrap_or(Json::Null),
        })),
        "callback_result" => {
            if let Some(e) = s("effect_ref") {
                Ok(AbiPayload::CallbackResult(CallbackResult::EffectRef(e)))
            } else if let Some(p) = m.get("projection") {
                Ok(AbiPayload::CallbackResult(CallbackResult::Projection(
                    p.clone(),
                )))
            } else if let Some(r) = s("reservation") {
                Ok(AbiPayload::CallbackResult(CallbackResult::Reservation(r)))
            } else if m.get("acknowledged").is_some() {
                Ok(AbiPayload::CallbackResult(CallbackResult::Acknowledged))
            } else {
                let e = AbiError::ALL
                    .iter()
                    .find(|e| e.as_str() == s("failure").unwrap_or_default())
                    .copied()
                    .ok_or(AbiError::SchemaViolation)?;
                Ok(AbiPayload::CallbackResult(CallbackResult::Refused(e)))
            }
        }
        "guard" => Ok(AbiPayload::GuardInvoke(GuardParams {
            binding_id: s("binding_id").ok_or(AbiError::SchemaViolation)?,
            decision_point: s("decision_point").ok_or(AbiError::SchemaViolation)?,
            subject_kind: GuardSubjectKind::parse(
                &s("subject_kind").ok_or(AbiError::SchemaViolation)?,
            )
            .ok_or(AbiError::SchemaViolation)?,
            subject: m.get("subject").cloned().unwrap_or(Json::Null),
            ledger_cursor: i("ledger_cursor").ok_or(AbiError::SchemaViolation)?,
        })),
        "guard_result" => Ok(AbiPayload::GuardResult(
            GuardVerdict::from_json(j).ok_or(AbiError::AuthorityCrossing)?,
        )),
        "run_conformance" => {
            let layers = match m.get("layers") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or(AbiError::SchemaViolation)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(AbiError::SchemaViolation),
            };
            Ok(AbiPayload::RunConformance(ConformanceParams {
                plugin: m.get("plugin").cloned().unwrap_or(Json::Null),
                layers,
                placement: s("placement").ok_or(AbiError::SchemaViolation)?,
            }))
        }
        "conformance_result" => Ok(AbiPayload::ConformanceResult {
            report: m.get("report").cloned().ok_or(AbiError::SchemaViolation)?,
        }),
        "refused" => Ok(AbiPayload::Refused(
            AbiError::ALL
                .iter()
                .find(|e| e.as_str() == s("error").unwrap_or_default())
                .copied()
                .ok_or(AbiError::SchemaViolation)?,
        )),
        _ => Err(AbiError::SchemaViolation),
    }
}

// ── Schema export (V6 — the one schema source) ────────────────────────────────

/// The canonical export of the `plugin_abi/1` contract — the *single source*
/// the generated `schema/plugin-abi-1.schema.json` artifact is emitted from
/// (`hh-codegen`; the drift check diffs it — V6: "schema drift is a build
/// failure, never a runtime negotiation"). Pure function of this module.
pub fn export_plugin_abi_schema() -> Json {
    Json::obj([
        ("$schema", Json::str("hh-embed/schema-export/1")),
        ("$id", Json::str(PLUGIN_ABI_NAME)),
        ("contract_major", Json::Int(PLUGIN_ABI_MAJOR)),
        (
            "envelope",
            Json::obj([
                ("protocol_version", Json::str("integer")),
                ("schema_hash", Json::str("string")),
                ("seq", Json::str("integer")),
                ("payload", Json::str("Payload")),
            ]),
        ),
        ("verbs", verbs_schema()),
        ("closed_sums", closed_sums_schema()),
        ("types", abi_types_schema()),
    ])
}

/// The verb table (payload spelling → member list; `*` = required).
fn verbs_schema() -> Json {
    fn v(dir: &str, fields: &[&str]) -> Json {
        Json::obj([
            ("direction", Json::str(dir)),
            (
                "fields",
                Json::Arr(fields.iter().map(|f| Json::str(*f)).collect()),
            ),
        ])
    }
    Json::Obj(BTreeMap::from([
        (
            "hello".to_string(),
            v(
                "plugin→kernel",
                &[
                    "plugin_abi_version*",
                    "plugin{version_id*,content*}*",
                    "contract_versions_offered[]*",
                    "capabilities{}*",
                ],
            ),
        ),
        (
            "hello_ack".to_string(),
            v(
                "kernel→plugin",
                &[
                    "host_id*",
                    "negotiated{plugin_abi_version*,contract_versions_chosen{}}*",
                    "registry_snapshot_id*",
                    "kernel_capabilities{}*",
                ],
            ),
        ),
        (
            "bind".to_string(),
            v(
                "kernel→plugin",
                &[
                    "slot*",
                    "class_id*",
                    "contract_version*",
                    "variant*",
                    "params*",
                    "profile*",
                    "account*",
                    "budget*",
                    "placement*",
                ],
            ),
        ),
        (
            "bind_result".to_string(),
            v("plugin→kernel", &["binding_id", "failure"]),
        ),
        (
            "invoke".to_string(),
            v(
                "kernel→plugin",
                &[
                    "binding_id*",
                    "operation*",
                    "inputs[]*",
                    "reservation*",
                    "deadline*",
                ],
            ),
        ),
        (
            "invoke_result".to_string(),
            v("plugin→kernel", &["outputs[]", "failure"]),
        ),
        (
            "stream".to_string(),
            v("kernel→plugin", &["invoke*", "resume_from_seq*"]),
        ),
        (
            "stream_item".to_string(),
            v("plugin→kernel", &["document*"]),
        ),
        (
            "cancel".to_string(),
            v("kernel→plugin", &["invocation_id*"]),
        ),
        ("unbind".to_string(), v("kernel→plugin", &["binding_id*"])),
        ("close".to_string(), v("kernel→plugin", &["host_id*"])),
        (
            "callback".to_string(),
            v("plugin→kernel", &["callback*", "args*"]),
        ),
        (
            "callback_result".to_string(),
            v(
                "kernel→plugin",
                &[
                    "effect_ref",
                    "projection",
                    "reservation",
                    "acknowledged",
                    "failure",
                ],
            ),
        ),
        (
            "guard".to_string(),
            v(
                "kernel→plugin",
                &[
                    "binding_id*",
                    "decision_point*",
                    "subject_kind*",
                    "subject*",
                    "ledger_cursor*",
                ],
            ),
        ),
        (
            "guard_result".to_string(),
            v("plugin→kernel", &["verdict*", "…"]),
        ),
        (
            "run_conformance".to_string(),
            v("kernel→plugin", &["plugin*", "layers[]*", "placement*"]),
        ),
        (
            "conformance_result".to_string(),
            v("plugin→kernel", &["report*"]),
        ),
        ("refused".to_string(), v("either", &["error*"])),
    ]))
}

/// The closed-sum vocabularies (the sums a peer may not extend — growth is a
/// `plugin_abi/2` dialect bump).
fn closed_sums_schema() -> Json {
    Json::obj([
        (
            "GuardVerdict",
            Json::Arr(
                [
                    "pass",
                    "annotate",
                    "narrow",
                    "propose_replacement",
                    "no_decision",
                ]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            ),
        ),
        (
            "Narrow",
            Json::Arr(
                ["deny", "ask", "attenuate"]
                    .iter()
                    .map(|s| Json::str(*s))
                    .collect(),
            ),
        ),
        (
            "HostCallback",
            Json::Arr(
                HostCallback::ALL
                    .iter()
                    .map(|c| Json::str(c.as_str()))
                    .collect(),
            ),
        ),
        (
            "BindFailure",
            Json::Arr(
                [
                    "not_installed",
                    "trust_denied",
                    "locality_unsupported",
                    "contract_mismatch",
                    "isolation_unavailable",
                ]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            ),
        ),
        (
            "AbiError",
            Json::Arr(
                AbiError::ALL
                    .iter()
                    .map(|e| Json::str(e.as_str()))
                    .collect(),
            ),
        ),
        (
            "TriState",
            Json::Arr(
                ["SUPPORTED", "UNSUPPORTED", "UNKNOWN"]
                    .iter()
                    .map(|s| Json::str(*s))
                    .collect(),
            ),
        ),
        (
            "GuardSubjectKind",
            Json::Arr(
                ["proposal", "cue", "context_plan"]
                    .iter()
                    .map(|s| Json::str(*s))
                    .collect(),
            ),
        ),
        (
            "Placement",
            Json::Arr(
                [
                    "in_process",
                    "subprocess_confined",
                    "container",
                    "remote",
                    "component_model",
                ]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            ),
        ),
    ])
}

/// The named record types the verbs carry (`BindingRecord` is §8.4 §3's).
fn abi_types_schema() -> Json {
    let strct = |fields: &[(&str, &str)]| {
        Json::obj([
            ("kind", Json::str("struct")),
            (
                "fields",
                Json::Arr(
                    fields
                        .iter()
                        .map(|(n, t)| Json::obj([("name", Json::str(*n)), ("type", Json::str(*t))]))
                        .collect(),
                ),
            ),
        ])
    };
    Json::Obj(BTreeMap::from([
        (
            "BindingRecord".to_string(),
            strct(&[
                ("binding_id", "string"),
                ("slot", "string"),
                ("class_id", "string"),
                ("contract_version", "string"),
                ("variant", "VersionedRef"),
                ("placement", "Placement"),
                ("host_id", "string"),
                ("permission", "VersionedRef"),
                ("budget", "BudgetNodeRef"),
            ]),
        ),
        (
            "PluginRef".to_string(),
            strct(&[("version_id", "string"), ("content", "string")]),
        ),
    ]))
}

/// The canonical bytes of the `plugin_abi/1` schema export.
pub fn canonical_plugin_abi_bytes() -> String {
    export_plugin_abi_schema().to_canonical_string()
}

/// The content address of the `plugin_abi/1` schema export — the value every
/// envelope's `schema_hash` asserts (V5/V6; `idp/1`, CC1).
pub fn plugin_abi_schema_hash() -> String {
    hh_identity::address(
        canonical_plugin_abi_bytes().as_bytes(),
        "application/schema+json",
    )
    .id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_allow_verdict_by_construction() {
        // AC-R-2.12.2-4's static half: the closed sum has no `allow` member,
        // and a document spelling `allow` does not decode (AuthorityCrossing).
        assert!(GuardVerdict::from_json(&Json::obj([("verdict", Json::str("allow"))])).is_none());
        let schema = export_plugin_abi_schema().to_canonical_string();
        let sums = export_plugin_abi_schema();
        let verdicts = match sums.get("closed_sums").and_then(|c| c.get("GuardVerdict")) {
            Some(Json::Arr(a)) => a.clone(),
            _ => panic!("closed_sums.GuardVerdict missing"),
        };
        assert!(!verdicts.iter().any(|v| v.as_str() == Some("allow")));
        assert_eq!(verdicts.len(), 5);
        // The only `allow` in the schema text is the fs allowlist phrase —
        // assert the verdict vocabulary carries none.
        assert!(!schema.contains("\"allow\""));
    }

    #[test]
    fn verdict_round_trip_and_meet() {
        for v in [
            GuardVerdict::Pass,
            GuardVerdict::Annotate("note".into()),
            GuardVerdict::Narrow(Narrow::Deny {
                reason: "no".into(),
            }),
            GuardVerdict::Narrow(Narrow::Ask {
                reason: "check".into(),
            }),
            GuardVerdict::Narrow(Narrow::Attenuate {
                scope: Json::obj([("scope", Json::str("subset"))]),
            }),
            GuardVerdict::ProposeReplacement(Json::obj([("id", Json::str("p1"))])),
            GuardVerdict::NoDecision,
        ] {
            assert_eq!(GuardVerdict::from_json(&v.to_json()), Some(v));
        }
        // AC-5's pure half: {pass, ask, annotate} in any order → ask.
        let ask = || {
            GuardVerdict::Narrow(Narrow::Ask {
                reason: "check".into(),
            })
        };
        for order in [
            vec![
                GuardVerdict::Pass,
                ask(),
                GuardVerdict::Annotate("n".into()),
            ],
            vec![
                GuardVerdict::Annotate("n".into()),
                GuardVerdict::Pass,
                ask(),
            ],
            vec![
                ask(),
                GuardVerdict::Annotate("n".into()),
                GuardVerdict::Pass,
            ],
        ] {
            assert_eq!(meet_verdicts(&order, false), ask());
        }
        // no_decision: identity for optional, deny for required.
        assert_eq!(
            meet_verdicts(&[GuardVerdict::NoDecision], false),
            GuardVerdict::Pass
        );
        assert!(matches!(
            meet_verdicts(&[GuardVerdict::NoDecision], true),
            GuardVerdict::Narrow(Narrow::Deny { .. })
        ));
        // deny wins over everything.
        let deny = GuardVerdict::Narrow(Narrow::Deny { reason: "d".into() });
        assert_eq!(
            meet_verdicts(&[ask(), deny.clone(), GuardVerdict::Pass], false),
            deny
        );
    }

    #[test]
    fn envelope_round_trip_and_strictness() {
        let env = AbiEnvelope {
            protocol_version: PLUGIN_ABI_MAJOR,
            schema_hash: plugin_abi_schema_hash(),
            seq: 7,
            payload: AbiPayload::Cancel {
                invocation_id: "inv-1".into(),
            },
        };
        let j = env.to_json();
        assert_eq!(AbiEnvelope::from_json(&j).unwrap(), env);
        // unknown envelope member → SchemaViolation.
        let mut m = match j.clone() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("extra".to_string(), Json::Null);
        assert_eq!(
            AbiEnvelope::from_json(&Json::Obj(m)),
            Err(AbiError::SchemaViolation)
        );
        // a callback naming a non-closed op fails.
        let bad = Json::obj([
            ("verb", Json::str("callback")),
            ("callback", Json::str("delete_everything")),
            ("args", Json::Null),
        ]);
        assert_eq!(decode_payload(&bad), Err(AbiError::SchemaViolation));
    }

    #[test]
    fn schema_hash_is_stable_and_idp1() {
        assert_eq!(plugin_abi_schema_hash(), plugin_abi_schema_hash());
        assert!(plugin_abi_schema_hash().starts_with("sha256:"));
        assert_eq!(PLUGIN_ABI_NAME, "plugin_abi/1");
    }

    #[test]
    fn no_hosting_abi_reference() {
        // AC-12: nothing in this schema names the Hosting ABI.
        let s = canonical_plugin_abi_bytes();
        assert!(!s.contains("hh-hosting"));
        assert!(!s.contains("OpaqueProcess"));
        assert!(!s.contains("hosting"));
    }
}
