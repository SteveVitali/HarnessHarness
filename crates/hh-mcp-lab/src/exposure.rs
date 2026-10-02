//! The `hh-lab/1` exposure definition — the sealed document the Lab
//! catalogue is generated from (spec §7.3 §2.1–2.2; ADR-0173 D2/D3).
//!
//! The document is *data*: `Assembly{dialect, profile_binding,
//! slots{lab_operations[], supply_surfaces?[]}, entities{Permission[],
//! Budget[], HarnessRule[]}, policy: ExposurePolicy, caller_bindings[],
//! assumption_debt[], ext}`. It is lowered deterministically into the
//! served `ServedArtifact` plus the server's tool-routing table — the
//! one serving path (AC-R-2.11.3-1's Lab form: same definition bytes ⇒
//! byte-identical `server/discover` and `tools/list`).
//!
//! `link` (this module's `parse` + [`required_debt`] check) refuses a
//! definition that lacks an assumption-debt record for a declared
//! carrier — the tasks carrier, the legacy-era projection, and every
//! `CallerBinding` credential kind present (AC-R-2.11.3-12).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_mcp::artifact::{ArtifactTool, ServedArtifact};
use hh_telemetry::sinks::{ContentClass, Redaction, Sampling, SinkPolicy};
use hh_wire::json::Json;

use crate::binding::CallerBinding;

/// The exposure definition's schema id.
pub const EXPOSURE_SCHEMA: &str = "hh-lab/1";

/// The canonical `hh-lab/1` semantic id.
pub const LAB_SEMANTIC_ID: &str = "hh-lab/1";

/// The surfaced effect-class spellings a tool declares (`kind` on the
/// intended row — the dossier's `capability` leg).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGroup {
    /// `fs_read{ledger}` reads — read_only, closed world.
    Read,
    /// Run control — launches `{spawn_process, spend}` compensable;
    /// submit/cancel `idempotent` control inputs; `respond_approval`
    /// permission-request domain.
    Launch,
    /// `lab.experiment.*` wraps.
    Experiment,
    /// `lab.results.*`/`lab.leaderboard.*` wraps — `fs_read{results}`.
    Results,
    /// `lab.analysis.*` wraps (C2 / Stage 5) — `fs_read{results}` plus
    /// `model_call`/`evaluator_calls` only where the operation itself
    /// spends (judged analyses charge `instrument`).
    Analysis,
    /// `memory_write{scope ∈ {project, organisation}}` — `apply`,
    /// `publish`, `register`, `define`, `record_conformance` and the
    /// permission-grant verbs (C2 / Stage 5). `delegate` callers are
    /// refused unless a sealed `entities.permissions[]` record covers
    /// the namespace (WS-K3 §6.2 R-5; ADR-0151 `who_may_publish`).
    Write,
    /// `serve_bundle` — the supply-surface spawn (C2 / Stage 5),
    /// `{spawn_process, spend}` compensable like any launch.
    Hosting,
    /// Supply-surface tools (a served harness bundle's compiled surface).
    Supply,
}

impl ToolGroup {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolGroup::Read => "read",
            ToolGroup::Launch => "launch",
            ToolGroup::Experiment => "experiment",
            ToolGroup::Results => "results",
            ToolGroup::Analysis => "analysis",
            ToolGroup::Write => "write",
            ToolGroup::Hosting => "hosting",
            ToolGroup::Supply => "supply",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<ToolGroup> {
        match s {
            "read" => Some(ToolGroup::Read),
            "launch" => Some(ToolGroup::Launch),
            "experiment" => Some(ToolGroup::Experiment),
            "results" => Some(ToolGroup::Results),
            "analysis" => Some(ToolGroup::Analysis),
            "write" => Some(ToolGroup::Write),
            "hosting" => Some(ToolGroup::Hosting),
            "supply" => Some(ToolGroup::Supply),
            _ => None,
        }
    }
}

/// The declared effect shape of one tool — the members the
/// `action.effect.intended` dossier carries verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectDecl {
    /// The declared `RiskClass` JSON (`{reversibility, repeat_safety,
    /// scope}`) — effective starts at declared and never lowers.
    pub risk_class: Json,
    /// Whether the effect is `mutability = read_only` (the write-ahead
    /// `committed` is skipped; `observed` may land right after
    /// `prepared`).
    pub read_only: bool,
    /// `compensable` declaration (launches).
    pub compensable: bool,
}

impl EffectDecl {
    /// `fs_read` — `read_only`, idempotent, workspace-local.
    pub fn read_only() -> EffectDecl {
        EffectDecl {
            risk_class: Json::obj([
                ("reversibility", Json::str("read_only")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("workspace_local")),
            ]),
            read_only: true,
            compensable: false,
        }
    }

    /// A control input — `idempotent` under `idempotency_key`,
    /// reversible-free (compensable=false), workspace-local.
    pub fn control() -> EffectDecl {
        EffectDecl {
            risk_class: Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("workspace_local")),
            ]),
            read_only: false,
            compensable: false,
        }
    }

    /// A launch — `{spawn_process, spend}`, compensable, non-idempotent
    /// except under the `idempotency_key` the caller forwards (the
    /// surface dedups *before* the op ever runs).
    pub fn launch() -> EffectDecl {
        EffectDecl {
            risk_class: Json::obj([
                ("reversibility", Json::str("compensable")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("external")),
            ]),
            read_only: false,
            compensable: true,
        }
    }

    /// A write outside the launch class (register/publish class —
    /// non-idempotent workspace write).
    pub fn write() -> EffectDecl {
        EffectDecl {
            risk_class: Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("non_idempotent")),
                ("scope", Json::str("workspace_local")),
            ]),
            read_only: false,
            compensable: false,
        }
    }
}

/// One `tools[]` member of the exposure definition — a Lab tool that
/// wraps exactly one operation record by identity (`op`); the handle
/// verbs (`run_status`, `read_ledger`, `respond_approval`, …) carry
/// `op = ""` — their body is the surface's own lowering of the run
/// envelope with its declared loss class (`loss_class`), never a new
/// operation (AC-R-2.11.3-13: "no run-exposure verb exists over MCP
/// that is not a lowering of the run envelope with a declared loss
/// class").
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureTool {
    /// The MCP surface name.
    pub name: String,
    /// The HIR semantic id — stable across renames.
    pub semantic_id: String,
    /// The `Text`-leaf description (definition authority).
    pub description: String,
    /// The operation this tool wraps (`hh-embed/1` method) or `""` for a
    /// declared run-envelope lowering.
    pub op: String,
    /// The tool's group.
    pub group: ToolGroup,
    /// The declared effect shape.
    pub effect: EffectDecl,
    /// The argument→op-params mapping: members of `arguments` renamed to
    /// op param names (`{"run": "run_id"}`); absent = pass-through.
    pub arg_map: BTreeMap<String, String>,
    /// Argument names that carry surface handles (`run`, `experiment`,
    /// `cursor`) — resolved through the handle table before the op runs.
    pub handle_args: Vec<String>,
    /// The declared input schema.
    pub input_schema: Json,
    /// The declared output schema.
    pub output_schema: Option<Json>,
    /// For `op = ""` lowerings: the declared loss class
    /// (`exact`/`narrowed`) and the wrapped record names.
    pub lowering: Option<Json>,
}

impl ExposureTool {
    /// `memory_write` class — the `apply`/`publish`/`register`/
    /// `define`/`record_conformance` family (effect `write`:
    /// non-idempotent, non-compensable). The delegate write-gate keys
    /// on this class (§7.3 §2.3 effect-classes row).
    pub fn is_memory_write(&self) -> bool {
        !self.effect.read_only
            && self
                .effect
                .risk_class
                .get("repeat_safety")
                .and_then(Json::as_str)
                == Some("non_idempotent")
    }
}

/// A Π row on a supply surface — `{match: semantic_id|name|*, decision ∈
/// {allow, deny, ask}, hidden?}`. `callable ⇔ revealed`: a `hidden` row
/// removes the tool from `tools/list` *and* from dispatch; `deny`
/// leaves it revealed but refused; `ask` revealed-but-pending.
#[derive(Debug, Clone, PartialEq)]
pub struct SupplyRule {
    /// The match — a tool `semantic_id`, a surface `name`, or `*`.
    pub match_: String,
    /// `allow` | `deny` | `ask`.
    pub decision: String,
    /// `hidden` — never revealed, never callable.
    pub hidden: bool,
}

/// One `supply_surfaces[]` member — a harness bundle served to a hosted
/// participant: `{surface_id, binding_id, bundle_id, profile_binding?,
/// artifact(<hh-mcp-target/1> doc), pi[]}`. The `(bundle_id,
/// profile_binding)` pair is the adapter's ref (CF-365); the artifact
/// member is the compiled `hh-mcp-target/1` document the supply surface
/// serves — the same record `lab.serve` lowers.
#[derive(Debug, Clone)]
pub struct SupplySurface {
    /// The surface's own id.
    pub surface_id: String,
    /// The `CallerBinding` (hosted participant) it serves.
    pub binding_id: String,
    /// The bundle ref (`version_id`).
    pub bundle_id: String,
    /// The supply surface's profile binding (`null` profile default).
    pub profile_binding: Option<String>,
    /// The lowered artifact (`hh-mcp-target/1` → `ServedArtifact`).
    pub artifact: ServedArtifact,
    /// The Π rows the server evaluates per `tools/call`.
    pub pi: Vec<SupplyRule>,
}

/// An `AssumptionDebtRecord` — `{assumption_id, carrier, statement,
/// removal_test}` (ADR-0197 shape). The carriers AC-R-2.11.3-12 names:
/// `tasks_carrier`, `resource_carrier`, `legacy_era_projection`,
/// `caller_binding.<kind>`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssumptionDebt {
    /// The record id.
    pub assumption_id: String,
    /// The assumed-away carrier the record discharges.
    pub carrier: String,
    /// What is assumed (the interim rule).
    pub statement: String,
    /// The removal test — what must run before the carrier may go.
    pub removal_test: String,
}

/// `ExposurePolicy = PublicationPolicy{readers, content_classes,
/// redaction, requires_consent, page_limit, max_field_bytes}` — the
/// sink the surface serves under (ADR-0174 D8). `readers` admits
/// `CallerBinding.readers_identity` values; everything else is the
/// shared `SinkPolicy` shape (`hh-telemetry` — one policy record, CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExposurePolicy {
    /// The admitted `readers_identity` values (empty = the declared
    /// readers set of the definition — for `hh-lab/1`, every declared
    /// binding).
    pub readers: BTreeSet<String>,
    /// The serving policy (`content_classes`, redaction, field cap,
    /// consent). `sink_id` names the surface sink.
    pub sink: SinkPolicy,
    /// `page_limit` — the maximum items one page serves (never a silent
    /// truncation: the page's `next_cursor`/`withheld` carries the cut).
    pub page_limit: usize,
}

impl ExposurePolicy {
    /// Does this policy admit a reader?
    pub fn admits_reader(&self, readers_identity: &str) -> bool {
        self.readers.is_empty() || self.readers.contains(readers_identity)
    }

    /// Does the policy admit a content class?
    pub fn admits_class(&self, c: ContentClass) -> bool {
        self.sink.content_classes.contains(&c)
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "readers",
                Json::Arr(self.readers.iter().map(Json::str).collect()),
            ),
            ("content_classes", {
                let mut v: Vec<Json> = self
                    .sink
                    .content_classes
                    .iter()
                    .map(|c| Json::str(c.as_str()))
                    .collect();
                v.sort_by_key(|a| a.to_canonical_string());
                Json::Arr(v)
            }),
            ("redaction", Json::str(self.sink.redaction.as_str())),
            ("requires_consent", Json::Bool(self.sink.requires_consent)),
            ("page_limit", Json::Int(self.page_limit as i64)),
            (
                "max_field_bytes",
                Json::Int(self.sink.max_field_bytes as i64),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ExposurePolicy, String> {
        let bad = |d: &str| -> String { format!("exposure_policy: {d}") };
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(bad("not an object")),
        };
        let readers: BTreeSet<String> = match m.get("readers") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => BTreeSet::new(),
        };
        let mut content_classes = BTreeSet::new();
        if let Some(Json::Arr(a)) = m.get("content_classes") {
            for c in a {
                let s = c
                    .as_str()
                    .ok_or_else(|| bad("content_classes[] not a string"))?;
                content_classes.insert(ContentClass::parse(s).map_err(|e| bad(&format!("{e}")))?);
            }
        }
        if content_classes.is_empty() {
            content_classes.insert(ContentClass::Accounting);
            content_classes.insert(ContentClass::Structural);
        }
        let redaction = match m.get("redaction").and_then(Json::as_str) {
            Some("allowlist") => Redaction::Allowlist,
            Some("pseudonymize") => Redaction::Pseudonymize,
            _ => Redaction::None,
        };
        let requires_consent = m
            .get("requires_consent")
            .and_then(|b| match b {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);
        let page_limit = m
            .get("page_limit")
            .and_then(Json::as_int)
            .map(|n| n.max(1) as usize)
            .unwrap_or(64);
        let max_field_bytes = m
            .get("max_field_bytes")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64)
            .unwrap_or(4096);
        let sink = SinkPolicy {
            sink_id: "mcp_surface".to_string(),
            content_classes,
            redaction,
            sampling: Sampling::ALL,
            rate_limit: None,
            max_field_bytes,
            requires_consent,
        };
        // The shared invariants hold (`{content} ⇒ requires_consent`;
        // content+diagnostic never mix) — an invalid policy refuses at
        // link, never at serve.
        sink.validate().map_err(|e| bad(&format!("{e}")))?;
        Ok(ExposurePolicy {
            readers,
            sink,
            page_limit,
        })
    }
}

/// The parsed exposure definition.
#[derive(Debug, Clone)]
pub struct ExposureDef {
    /// `semantic_id` (`hh-lab/1`).
    pub semantic_id: String,
    /// `version_id` — `idp/1` over the canonical document bytes.
    pub version_id: String,
    /// The profile binding (the loss/renaming profile — verbatim ref).
    pub profile_binding: Option<String>,
    /// The serving policy.
    pub policy: ExposurePolicy,
    /// `caller_bindings[]` — sealed bindings the transports resolve.
    pub bindings: Vec<CallerBinding>,
    /// `tools[]` — the exposed catalogue.
    pub tools: Vec<ExposureTool>,
    /// `supply_surfaces[]` — served harness bundles for hosted
    /// participants.
    pub supply_surfaces: Vec<SupplySurface>,
    /// `entities.permissions[]` — the sealed `Permission` records
    /// verbatim (the document is data; a permission is opaque to the
    /// server except for the delegate write-gate's coverage check —
    /// [`ExposureDef::permission_covers`]).
    pub permissions: Vec<Json>,
    /// `assumption_debt[]` — every declared carrier's record.
    pub assumption_debt: Vec<AssumptionDebt>,
    /// `ext` — preserved verbatim, never interpreted.
    pub ext: Json,
    /// The canonical bytes the `version_id` hashes over.
    pub canonical_bytes: Vec<u8>,
}

impl ExposureDef {
    /// The read-path admission (AC-R-2.11.3-7's per-item projection):
    /// the binding must be a declared `readers` identity (empty
    /// `readers[]` ⇒ every bound caller reads), and the sink's
    /// `content_classes` must admit the item's class — `read_ledger`
    /// items are L1 `structural` (the event rows themselves); a
    /// content-bearing event (an offloaded `payload_ref`/`refs` chain)
    /// additionally requires L2 `content`.
    pub fn admits_read_class(
        &self,
        binding: &crate::binding::CallerBinding,
        _kind: &str,
        ev_class: &str,
        carries_content: bool,
    ) -> bool {
        if !self.policy.readers.is_empty()
            && !self.policy.admits_reader(&binding.binding_id)
            && !self.policy.admits_reader(&binding.principal_ref)
        {
            return false;
        }
        if !self.policy.admits_class(ContentClass::Structural) {
            return false;
        }
        if carries_content && !self.policy.admits_class(ContentClass::Content) {
            return false;
        }
        let _ = ev_class;
        true
    }

    /// The view kinds `get_trace` may serve — the read-only policy
    /// declaration `served_context_views` narrows it (empty ⇒ every
    /// `ViewKind` — the projection op is itself the gate).
    pub fn allowed_views(&self, binding: &crate::binding::CallerBinding) -> Vec<String> {
        if !self.policy.readers.is_empty()
            && !self.policy.admits_reader(&binding.binding_id)
            && !self.policy.admits_reader(&binding.principal_ref)
        {
            return vec!["__none__".to_string()]; // admitted to nothing
        }
        vec![]
    }

    /// The delegate write-gate's coverage check (§7.3 §2.3 effect-class
    /// row: "`apply`/`publish`/`register`/`record_conformance`/`define`
    /// are `memory_write` … refused for `delegate` callers unless a
    /// sealed `Permission` covers the namespace" — WS-K3 §6.2 R-5).
    ///
    /// A sealed `entities.permissions[]` record covers `(binding,
    /// tool)` when:
    /// - it names the binding's `principal_ref` or `binding_id` under
    ///   any holder key (`holder` | `grantee` | `principal` |
    ///   `subject`) — or `"*"`;
    /// - its `scope` member covers the wrapped operation — `scope`
    ///   string `"*"`, or `scope.namespace` equal to / a `.*`-prefix of
    ///   `tool.op`, or `scope.tools`/`scope.ops` containing the tool
    ///   `name`/`semantic_id`/`op`;
    /// - `state` is absent or `"active"`.
    ///
    /// A record naming no holder key never covers (a holder-less
    /// Permission confers nothing — the same rule the kernel's Π rows
    /// apply). The check is deliberately conservative: ambiguity
    /// refuses.
    pub fn permission_covers(
        &self,
        binding: &crate::binding::CallerBinding,
        tool: &ExposureTool,
    ) -> bool {
        self.permissions.iter().any(|p| {
            let Json::Obj(pm) = p else { return false };
            if matches!(pm.get("state").and_then(Json::as_str), Some(s) if s != "active") {
                return false;
            }
            let holder_ok = ["holder", "grantee", "principal", "subject"]
                .iter()
                .filter_map(|k| pm.get(*k).and_then(Json::as_str))
                .any(|h| {
                    h == "*"
                        || h == binding.principal_ref
                        || h == binding.binding_id
                        || h == binding.readers_identity
                });
            if !holder_ok {
                return false;
            }
            match pm.get("scope") {
                Some(Json::Str(s)) => s == "*",
                Some(Json::Obj(sm)) => {
                    if let Some(ns) = sm.get("namespace").and_then(Json::as_str) {
                        if ns == "*" {
                            return true;
                        }
                        if let Some(prefix) = ns.strip_suffix(".*") {
                            if tool.op.starts_with(prefix) {
                                return true;
                            }
                        }
                        if tool.op == ns || tool.name == ns {
                            return true;
                        }
                    }
                    let listed = |key: &str| -> bool {
                        sm.get(key)
                            .and_then(|v| match v {
                                Json::Arr(a) => Some(a.clone()),
                                _ => None,
                            })
                            .is_some_and(|a| {
                                a.iter().filter_map(Json::as_str).any(|n| {
                                    n == tool.name || n == tool.semantic_id || n == tool.op
                                })
                            })
                    };
                    listed("tools") || listed("ops")
                }
                _ => false,
            }
        })
    }
}

/// A parse/link refusal — typed, never a panic.
#[derive(Debug)]
pub enum ExposureError {
    /// The document is not canonical JSON / not the schema.
    Malformed { detail: String },
    /// A required member is missing/invalid.
    Missing { member: String },
    /// AC-R-2.11.3-12 — an assumption-debt record is missing for a
    /// declared carrier.
    MissingDebt { carrier: String },
    /// A binding/credential/protocol member is invalid.
    Invalid { detail: String },
}

impl std::fmt::Display for ExposureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExposureError::Malformed { detail } => write!(f, "exposure malformed: {detail}"),
            ExposureError::Missing { member } => write!(f, "exposure missing {member}"),
            ExposureError::MissingDebt { carrier } => {
                write!(f, "exposure missing assumption_debt for {carrier}")
            }
            ExposureError::Invalid { detail } => write!(f, "exposure invalid: {detail}"),
        }
    }
}

impl std::error::Error for ExposureError {}

/// The carriers every exposure definition must debt-record
/// (AC-R-2.11.3-12): the Stage-5 tasks carrier, the resource/
/// subscription carrier and the legacy-era projection — the
/// reflexive-debt rule of ADR-0175 D6 — plus `caller_binding.<kind>`
/// for each credential kind `caller_bindings[]` uses.
pub fn required_debt(def_bindings: &[CallerBinding]) -> Vec<String> {
    let mut out = vec![
        "tasks_carrier".to_string(),
        "resource_carrier".to_string(),
        "legacy_era_projection".to_string(),
    ];
    let mut kinds = BTreeSet::new();
    for b in def_bindings {
        kinds.insert(b.credential.kind_str());
    }
    for k in kinds {
        out.push(format!("caller_binding.{k}"));
    }
    out
}

/// Parse + link an exposure-definition document — the one path a served
/// catalogue comes from. Returns the sealed `version_id` (`idp/1` over
/// the canonical bytes) inside `ExposureDef`.
pub fn parse_exposure(doc: &Json) -> Result<ExposureDef, ExposureError> {
    let bad = |d: &str| ExposureError::Malformed {
        detail: d.to_string(),
    };
    let m = match doc {
        Json::Obj(m) => m,
        _ => return Err(bad("not an object")),
    };
    let schema = m
        .get("schema")
        .and_then(Json::as_str)
        .ok_or_else(|| bad("schema missing"))?;
    if schema != EXPOSURE_SCHEMA {
        return Err(bad(&format!(
            "schema `{schema}` — expected {EXPOSURE_SCHEMA}"
        )));
    }
    let semantic_id = m
        .get("semantic_id")
        .and_then(Json::as_str)
        .ok_or(ExposureError::Missing {
            member: "semantic_id".to_string(),
        })?
        .to_string();
    let canonical_bytes = doc.to_canonical_string().into_bytes();
    let version_id =
        hh_identity::identify_bytes(hh_identity::RecordKind::SealedDefinition, &canonical_bytes);
    let profile_binding = m
        .get("profile_binding")
        .and_then(Json::as_str)
        .map(String::from);
    let policy = ExposurePolicy::from_json(m.get("policy").ok_or(ExposureError::Missing {
        member: "policy".to_string(),
    })?)
    .map_err(|detail| ExposureError::Invalid { detail })?;
    let mut bindings = Vec::new();
    if let Some(Json::Arr(a)) = m.get("caller_bindings") {
        for (i, b) in a.iter().enumerate() {
            bindings.push(CallerBinding::from_json(b).map_err(|detail| {
                ExposureError::Invalid {
                    detail: format!("caller_bindings[{i}]: {detail}"),
                }
            })?);
        }
    }
    let mut tools = Vec::new();
    if let Some(Json::Arr(a)) = m.get("tools") {
        for (i, t) in a.iter().enumerate() {
            tools.push(parse_tool(t, i)?);
        }
    }
    let mut supply_surfaces = Vec::new();
    if let Some(Json::Arr(a)) = m.get("supply_surfaces") {
        for (i, s) in a.iter().enumerate() {
            supply_surfaces.push(parse_supply(s, i)?);
        }
    }
    // `entities.permissions[]` — the sealed Permission records the
    // delegate write-gate reads (§7.3 §2.1: `entities{Permission[],
    // Budget[], HarnessRule[]}`). Preserved verbatim; only the
    // coverage check interprets them.
    let mut permissions = Vec::new();
    if let Some(Json::Obj(em)) = m.get("entities") {
        if let Some(Json::Arr(a)) = em.get("permissions") {
            for (i, p) in a.iter().enumerate() {
                match p {
                    Json::Obj(_) => permissions.push(p.clone()),
                    _ => {
                        return Err(ExposureError::Invalid {
                            detail: format!("entities.permissions[{i}] not an object"),
                        })
                    }
                }
            }
        }
    }
    let mut assumption_debt = Vec::new();
    if let Some(Json::Arr(a)) = m.get("assumption_debt") {
        for (i, d) in a.iter().enumerate() {
            let dm = match d {
                Json::Obj(dm) => dm,
                _ => {
                    return Err(ExposureError::Invalid {
                        detail: format!("assumption_debt[{i}] not an object"),
                    })
                }
            };
            assumption_debt.push(AssumptionDebt {
                assumption_id: dm
                    .get("assumption_id")
                    .and_then(Json::as_str)
                    .ok_or(ExposureError::Missing {
                        member: format!("assumption_debt[{i}].assumption_id"),
                    })?
                    .to_string(),
                carrier: dm
                    .get("carrier")
                    .and_then(Json::as_str)
                    .ok_or(ExposureError::Missing {
                        member: format!("assumption_debt[{i}].carrier"),
                    })?
                    .to_string(),
                statement: dm
                    .get("statement")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                removal_test: dm
                    .get("removal_test")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    // AC-R-2.11.3-12 — link refuses a definition whose declared carriers
    // lack assumption-debt records.
    let have: BTreeSet<&str> = assumption_debt.iter().map(|d| d.carrier.as_str()).collect();
    for carrier in required_debt(&bindings) {
        if !have.contains(carrier.as_str()) {
            return Err(ExposureError::MissingDebt { carrier });
        }
    }
    let ext = m.get("ext").cloned().unwrap_or_else(|| Json::obj([]));
    Ok(ExposureDef {
        semantic_id,
        version_id,
        profile_binding,
        policy,
        bindings,
        tools,
        supply_surfaces,
        permissions,
        assumption_debt,
        ext,
        canonical_bytes,
    })
}

fn parse_tool(t: &Json, i: usize) -> Result<ExposureTool, ExposureError> {
    let missing = |member: &str| ExposureError::Missing {
        member: format!("tools[{i}].{member}"),
    };
    let m = match t {
        Json::Obj(m) => m,
        _ => {
            return Err(ExposureError::Invalid {
                detail: format!("tools[{i}] not an object"),
            })
        }
    };
    let name = m
        .get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| missing("name"))?
        .to_string();
    let semantic_id = m
        .get("semantic_id")
        .and_then(Json::as_str)
        .unwrap_or(&name)
        .to_string();
    let description = m
        .get("description")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let op = m.get("op").and_then(Json::as_str).unwrap_or("").to_string();
    let group = ToolGroup::parse(
        m.get("group")
            .and_then(Json::as_str)
            .ok_or_else(|| missing("group"))?,
    )
    .ok_or_else(|| ExposureError::Invalid {
        detail: format!("tools[{i}].group unknown"),
    })?;
    let effect = match m.get("effect").and_then(Json::as_str) {
        Some("read_only") | None => EffectDecl::read_only(),
        Some("control") => EffectDecl::control(),
        Some("launch") => EffectDecl::launch(),
        Some("write") => EffectDecl::write(),
        Some(other) => {
            return Err(ExposureError::Invalid {
                detail: format!("tools[{i}].effect `{other}`"),
            })
        }
    };
    let mut arg_map = BTreeMap::new();
    if let Some(Json::Obj(am)) = m.get("arg_map") {
        for (k, v) in am {
            if let Some(vs) = v.as_str() {
                arg_map.insert(k.clone(), vs.to_string());
            }
        }
    }
    let handle_args: Vec<String> = match m.get("handle_args") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    };
    let input_schema = m
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(|| Json::obj([("type", Json::str("object"))]));
    let output_schema = m.get("outputSchema").cloned();
    let lowering = m.get("lowering").cloned();
    Ok(ExposureTool {
        name,
        semantic_id,
        description,
        op,
        group,
        effect,
        arg_map,
        handle_args,
        input_schema,
        output_schema,
        lowering,
    })
}

fn parse_supply(s: &Json, i: usize) -> Result<SupplySurface, ExposureError> {
    let missing = |member: &str| ExposureError::Missing {
        member: format!("supply_surfaces[{i}].{member}"),
    };
    let m = match s {
        Json::Obj(m) => m,
        _ => {
            return Err(ExposureError::Invalid {
                detail: format!("supply_surfaces[{i}] not an object"),
            })
        }
    };
    let surface_id = m
        .get("surface_id")
        .and_then(Json::as_str)
        .ok_or_else(|| missing("surface_id"))?
        .to_string();
    let binding_id = m
        .get("binding_id")
        .and_then(Json::as_str)
        .ok_or_else(|| missing("binding_id"))?
        .to_string();
    let bundle_id = m
        .get("bundle_id")
        .and_then(Json::as_str)
        .ok_or_else(|| missing("bundle_id"))?
        .to_string();
    let profile_binding = m
        .get("profile_binding")
        .and_then(Json::as_str)
        .map(String::from);
    let artifact_json = m.get("artifact").ok_or_else(|| missing("artifact"))?;
    let artifact = hh_mcp::artifact::lower_mcp_target(
        artifact_json.to_canonical_string().as_bytes(),
        &bundle_id,
        &bundle_id,
    )
    .map_err(|e| ExposureError::Invalid {
        detail: format!("supply_surfaces[{i}].artifact: {e:?}"),
    })?;
    let mut pi = Vec::new();
    if let Some(Json::Arr(a)) = m.get("pi") {
        for (j, r) in a.iter().enumerate() {
            let rm = match r {
                Json::Obj(rm) => rm,
                _ => {
                    return Err(ExposureError::Invalid {
                        detail: format!("supply_surfaces[{i}].pi[{j}] not an object"),
                    })
                }
            };
            let decision = rm
                .get("decision")
                .and_then(Json::as_str)
                .unwrap_or("allow")
                .to_string();
            if !matches!(decision.as_str(), "allow" | "deny" | "ask") {
                return Err(ExposureError::Invalid {
                    detail: format!("supply_surfaces[{i}].pi[{j}].decision `{decision}`"),
                });
            }
            pi.push(SupplyRule {
                match_: rm
                    .get("match")
                    .and_then(Json::as_str)
                    .unwrap_or("*")
                    .to_string(),
                decision,
                hidden: rm.get("hidden") == Some(&Json::Bool(true)),
            });
        }
    }
    Ok(SupplySurface {
        surface_id,
        binding_id,
        bundle_id,
        profile_binding,
        artifact,
        pi,
    })
}

/// The lowering — `(ServedArtifact, catalogue tools by name)`. The
/// served artifact is `hh-mcp-artifact/1`-shaped (one artefact schema
/// for both surfaces — CC1); `bundle_id` carries the exposure
/// definition's `version_id` (the served document coordinate).
pub fn lower(def: &ExposureDef) -> ServedArtifact {
    let tools: Vec<ArtifactTool> = def
        .tools
        .iter()
        .map(|t| ArtifactTool {
            name: t.name.clone(),
            description: if t.description.is_empty() {
                None
            } else {
                Some(t.description.clone())
            },
            input_schema: t.input_schema.clone(),
            output_schema: t.output_schema.clone(),
            annotations: None,
            semantic_id: t.semantic_id.clone(),
            hir_meta: Json::obj([
                ("op", Json::str(t.op.clone())),
                ("group", Json::str(t.group.as_str())),
            ]),
            ext_meta: Json::obj([]),
            input_requests: None,
        })
        .collect();
    // Canonical order — `(semantic_id, name)` (one catalogue rule for
    // every served surface, AC-R-2.11.3-1).
    let mut tools = tools;
    tools.sort_by(|a, b| {
        a.semantic_id
            .cmp(&b.semantic_id)
            .then_with(|| a.name.cmp(&b.name))
    });
    let projection = Json::Arr(
        tools
            .iter()
            .map(|t| t.to_mcp_json(&def.version_id))
            .collect(),
    );
    let catalogue_hash =
        hh_identity::idp_id("mcp.catalogue", projection.to_canonical_string().as_bytes());
    ServedArtifact {
        bundle_id: def.version_id.clone(),
        bundle_version: def.version_id.clone(),
        tools,
        catalogue_hash,
        ttl_ms: 0,
    }
}

/// One `tools[]` row for the canonical `hh-lab/1` document.
#[allow(clippy::too_many_arguments)] // a document row is a row — the arity is the table's.
fn tool(
    name: &str,
    semantic_id: &str,
    group: ToolGroup,
    op: &str,
    effect: &str,
    description: &str,
    handle_args: &[&str],
    arg_map: &[(&str, &str)],
    input_schema: Json,
    output_schema: Option<Json>,
    lowering: Option<Json>,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("name".into(), Json::str(name));
    m.insert("semantic_id".into(), Json::str(semantic_id));
    m.insert("description".into(), Json::str(description));
    m.insert("group".into(), Json::str(group.as_str()));
    m.insert("op".into(), Json::str(op));
    m.insert("effect".into(), Json::str(effect));
    if !handle_args.is_empty() {
        m.insert(
            "handle_args".into(),
            Json::Arr(handle_args.iter().map(|s| Json::str(*s)).collect()),
        );
    }
    if !arg_map.is_empty() {
        m.insert(
            "arg_map".into(),
            Json::Obj(
                arg_map
                    .iter()
                    .map(|(k, v)| (k.to_string(), Json::str(*v)))
                    .collect(),
            ),
        );
    }
    m.insert("inputSchema".into(), input_schema);
    if let Some(o) = output_schema {
        m.insert("outputSchema".into(), o);
    }
    if let Some(l) = lowering {
        m.insert("lowering".into(), l);
    }
    Json::Obj(m)
}

fn obj_schema(required: &[&str], props: &[(&str, Json)]) -> Json {
    let mut m = BTreeMap::new();
    m.insert("type".into(), Json::str("object"));
    let mut p = BTreeMap::new();
    for (k, v) in props {
        p.insert(k.to_string(), v.clone());
    }
    m.insert("properties".into(), Json::Obj(p));
    if !required.is_empty() {
        m.insert(
            "required".into(),
            Json::Arr(required.iter().map(|s| Json::str(*s)).collect()),
        );
    }
    Json::Obj(m)
}

fn t_str() -> Json {
    Json::obj([("type", Json::str("string"))])
}
fn t_obj() -> Json {
    Json::obj([("type", Json::str("object"))])
}
fn t_any() -> Json {
    Json::obj([])
}

/// The canonical `hh-lab/1` document — the C1 read/launch/experiment/
/// results groups (spec §7.3 §2.3 tool tables), one supply-surface slot
/// declared-empty, every assumption-debt carrier recorded (AC-R-2.11.3-12),
/// and the default `stdio_launch` binding. A deployment seals its own
/// document through the same `parse`/`link` — this is the shipped
/// default, not a second schema (CC7).
pub fn default_lab_document() -> Json {
    let read_in = |extra: &[(&str, Json)], required: &[&str]| {
        obj_schema(
            required,
            &[
                &[("run", t_str())][..],
                &[
                    ("from_seq", Json::obj([("type", Json::str("integer"))])),
                    ("until_seq", Json::obj([("type", Json::str("integer"))])),
                    ("content_classes", Json::obj([("type", Json::str("array"))])),
                    ("limit", Json::obj([("type", Json::str("integer"))])),
                    ("cursor", t_str()),
                ][..],
                extra,
            ]
            .concat(),
        )
    };
    Json::obj([
        ("schema", Json::str(EXPOSURE_SCHEMA)),
        ("semantic_id", Json::str(LAB_SEMANTIC_ID)),
        ("dialect", Json::str("hh-lab-c1")),
        (
            "profile_binding",
            Json::str("profile:lab-default"),
        ),
        (
            "policy",
            Json::obj([
                ("readers", Json::Arr(vec![])),
                (
                    "content_classes",
                    Json::Arr(vec![
                        Json::str("accounting"),
                        Json::str("structural"),
                        Json::str("content"),
                    ]),
                ),
                ("redaction", Json::str("none")),
                ("requires_consent", Json::Bool(true)),
                ("page_limit", Json::Int(64)),
                ("max_field_bytes", Json::Int(65536)),
            ]),
        ),
        (
            "caller_bindings",
            Json::Arr(vec![
                stdio_binding_json(),
                service_binding_json(),
                provider_binding_json(),
            ]),
        ),
        (
            "entities",
            Json::obj([
                // The delegate write-gate's sealed Permission set —
                // the shipped default grants the fixture principal
                // the registry/assembly namespaces so `human_principal`
                // and permission-covered delegates can write (a
                // deployment seals its own).
                (
                    "permissions",
                    Json::Arr(vec![Json::obj([
                        ("kind", Json::str("Permission")),
                        ("permission_id", Json::str("perm-lab-writes")),
                        ("holder", Json::str("principal:test")),
                        ("state", Json::str("active")),
                        (
                            "scope",
                            Json::obj([(
                                "namespace",
                                Json::str("lab.*"),
                            )]),
                        ),
                    ])]),
                ),
            ]),
        ),
        (
            "tools",
            Json::Arr(vec![
                // ── read group ───────────────────────────────────────
                tool(
                    "read_ledger",
                    "hh.lab/read_ledger/1",
                    ToolGroup::Read,
                    "",
                    "read_only",
                    "Page a run's durable ledger through the ExposurePolicy sink — items are event records; withheld rows are listed, never silent.",
                    &["run"],
                    &[("run", "run_id")],
                    read_in(&[], &["run"]),
                    Some(t_obj()),
                    Some(Json::obj([("class", Json::str("narrowed"))])),
                ),
                tool(
                    "run_status",
                    "hh.lab/run_status/1",
                    ToolGroup::Read,
                    "",
                    "read_only",
                    "RunStatusView{status, pending_approvals[], account, head_seq} — the declared lowering of the run tail + account projection.",
                    &["run"],
                    &[("run", "run_id")],
                    obj_schema(&["run"], &[("run", t_str())]),
                    Some(t_obj()),
                    Some(Json::obj([("class", Json::str("exact"))])),
                ),
                tool(
                    "get_trace",
                    "hh.lab/get_trace/1",
                    ToolGroup::Read,
                    "",
                    "read_only",
                    "A ledger projection (`view` = a `ViewKind` spelling) through the ExposurePolicy sink.",
                    &["run"],
                    &[("run", "run_id"), ("view", "view_kind")],
                    obj_schema(
                        &["run"],
                        &[("run", t_str()), ("view", t_str()), ("until_seq", Json::obj([("type", Json::str("integer"))]))],
                    ),
                    Some(t_obj()),
                    Some(Json::obj([("class", Json::str("narrowed"))])),
                ),
                tool(
                    "get_bundle",
                    "hh.lab/get_bundle/1",
                    ToolGroup::Read,
                    "kernel.bundle",
                    "read_only",
                    "The run's compiled bundle (`kernel.bundle`).",
                    &["run"],
                    &[("run", "run_id")],
                    obj_schema(&["run"], &[("run", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "list_pending_approvals",
                    "hh.lab/list_pending_approvals/1",
                    ToolGroup::Read,
                    "",
                    "read_only",
                    "The run's live `security.permission.pending` rows — `pending_approvals[permission_id]`.",
                    &["run"],
                    &[("run", "run_id")],
                    obj_schema(&["run"], &[("run", t_str())]),
                    Some(t_obj()),
                    Some(Json::obj([("class", Json::str("exact"))])),
                ),
                // ── launch group ─────────────────────────────────────
                tool(
                    "launch_run",
                    "hh.lab/launch_run/1",
                    ToolGroup::Launch,
                    "open_session",
                    "launch",
                    "Launch a run: `open_session{kind:new}` under the binding's budget slice — `run_handle` returns only after seq-0 durability.",
                    &[],
                    &[],
                    obj_schema(
                        &["definition_ref", "budget", "idempotency_key"],
                        &[
                            ("definition_ref", t_str()),
                            ("definition", t_obj()),
                            ("environment", t_obj()),
                            ("budget", t_obj()),
                            ("task", t_obj()),
                            ("idempotency_key", t_str()),
                            ("profile_binding", t_obj()),
                        ],
                    ),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "submit_input",
                    "hh.lab/submit_input/1",
                    ToolGroup::Launch,
                    "submit",
                    "control",
                    "Submit input content to a launched run's writer session (control input, idempotent under `idempotency_key`).",
                    &["run"],
                    &[("run", "session_id")],
                    obj_schema(
                        &["run", "content", "idempotency_key"],
                        &[
                            ("run", t_str()),
                            ("content", t_any()),
                            ("idempotency_key", t_str()),
                        ],
                    ),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "cancel_run",
                    "hh.lab/cancel_run/1",
                    ToolGroup::Launch,
                    "cancel",
                    "control",
                    "Cancel a launched run — `control.decision{kind: cancel}` then the open effects close per the effect model.",
                    &["run"],
                    &[("run", "session_id")],
                    obj_schema(
                        &["run"],
                        &[("run", t_str()), ("reason", t_str())],
                    ),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "respond_approval",
                    "hh.lab/respond_approval/1",
                    ToolGroup::Launch,
                    "respond_permission",
                    "control",
                    "Answer a pending approval — the `permission_request` domain, `human_principal` bindings only (every other kind answers `IllegitimateEndorsement`).",
                    &["run"],
                    &[("run", "session_id")],
                    obj_schema(
                        &["run", "permission_id", "outcome", "idempotency_key"],
                        &[
                            ("run", t_str()),
                            ("permission_id", t_str()),
                            ("outcome", t_obj()),
                            ("idempotency_key", t_str()),
                        ],
                    ),
                    Some(t_obj()),
                    None,
                ),
                // ── experiment group ─────────────────────────────────
                tool(
                    "register_experiment",
                    "hh.lab/register_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.register",
                    "write",
                    "Register an `ExperimentSpec` (refusal table verbatim) → `experiment_id`.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "expand_experiment",
                    "hh.lab/expand_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.expand",
                    "read_only",
                    "`CellPlan`/`RunPlan` preview — `expand` + `validate_batch` with no `open`.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "open_experiment",
                    "hh.lab/open_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.open_experiment",
                    "launch",
                    "Open a registered experiment → `experiment_handle`.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "experiment_status",
                    "hh.lab/experiment_status/1",
                    ToolGroup::Experiment,
                    "lab.experiment.next",
                    "read_only",
                    "The scheduler projection (`next` over the experiment ledger).",
                    &["experiment"],
                    &[("experiment", "experiment_id")],
                    obj_schema(&["experiment"], &[("experiment", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "pause_experiment",
                    "hh.lab/pause_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.pause",
                    "control",
                    "Pause an open experiment.",
                    &["experiment"],
                    &[("experiment", "experiment_id")],
                    obj_schema(&["experiment"], &[("experiment", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "resume_experiment",
                    "hh.lab/resume_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.resume",
                    "control",
                    "Resume a paused experiment.",
                    &["experiment"],
                    &[("experiment", "experiment_id")],
                    obj_schema(&["experiment"], &[("experiment", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "close_experiment",
                    "hh.lab/close_experiment/1",
                    ToolGroup::Experiment,
                    "lab.experiment.close",
                    "control",
                    "Close an experiment → `ExperimentReport`.",
                    &["experiment"],
                    &[("experiment", "experiment_id")],
                    obj_schema(&["experiment"], &[("experiment", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                // ── results group ────────────────────────────────────
                tool(
                    "query_rows",
                    "hh.lab/query_rows/1",
                    ToolGroup::Results,
                    "lab.results.query_rows",
                    "read_only",
                    "`query_rows{QuerySpec}` — a paged, watermark-stable results read.",
                    &[],
                    &[],
                    obj_schema(&["query"], &[("query", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "get_row",
                    "hh.lab/get_row/1",
                    ToolGroup::Results,
                    "lab.results.get_row",
                    "read_only",
                    "One results row by row key.",
                    &[],
                    &[],
                    obj_schema(&["row_key"], &[("row_key", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "row_history",
                    "hh.lab/row_history/1",
                    ToolGroup::Results,
                    "lab.results.row_history",
                    "read_only",
                    "A row's version history.",
                    &[],
                    &[],
                    obj_schema(&["row_key"], &[("row_key", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "cells",
                    "hh.lab/cells/1",
                    ToolGroup::Results,
                    "lab.results.cells",
                    "read_only",
                    "`cells{design}` — the cell table over the results store.",
                    &[],
                    &[],
                    obj_schema(&["design"], &[("design", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "distribution",
                    "hh.lab/distribution/1",
                    ToolGroup::Results,
                    "lab.results.distribution",
                    "read_only",
                    "A metric distribution over rows.",
                    &[],
                    &[],
                    obj_schema(&["query"], &[("query", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "results_catalogue",
                    "hh.lab/results_catalogue/1",
                    ToolGroup::Results,
                    "lab.results.catalogue",
                    "read_only",
                    "The results-store catalogue.",
                    &[],
                    &[],
                    obj_schema(&[], &[]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "verify_row",
                    "hh.lab/verify_row/1",
                    ToolGroup::Results,
                    "lab.results.verify_row",
                    "read_only",
                    "Verify a row's provenance chain.",
                    &[],
                    &[],
                    obj_schema(&["row_key"], &[("row_key", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "export_rows",
                    "hh.lab/export_rows/1",
                    ToolGroup::Results,
                    "lab.results.export_rows",
                    "read_only",
                    "`export_rows{target}` — the PublicationPolicy-gated export (`delivered` rows land on every cited run).",
                    &[],
                    &[],
                    obj_schema(&["target"], &[("target", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "leaderboard",
                    "hh.lab/leaderboard/1",
                    ToolGroup::Results,
                    "lab.leaderboard.leaderboard",
                    "read_only",
                    "A defined leaderboard's ranked rows.",
                    &[],
                    &[],
                    obj_schema(&["leaderboard"], &[("leaderboard", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                // ── assembly/registry read-only (C1 §2.3) ────────────
                tool(
                    "assembly_plan",
                    "hh.lab/assembly_plan/1",
                    ToolGroup::Read,
                    "lab.assembly.plan",
                    "read_only",
                    "Assembly plan preview (read-only).",
                    &[],
                    &[],
                    obj_schema(&["assembly"], &[("assembly", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "assembly_explain",
                    "hh.lab/assembly_explain/1",
                    ToolGroup::Read,
                    "lab.assembly.explain",
                    "read_only",
                    "Assembly explanation (read-only).",
                    &[],
                    &[],
                    obj_schema(&["assembly"], &[("assembly", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "registry_resolve",
                    "hh.lab/registry_resolve/1",
                    ToolGroup::Read,
                    "lab.registry.resolve",
                    "read_only",
                    "Resolve a registry name/ref (read-only).",
                    &[],
                    &[],
                    obj_schema(&["ref"], &[("ref", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "registry_query",
                    "hh.lab/registry_query/1",
                    ToolGroup::Read,
                    "lab.registry.query",
                    "read_only",
                    "Query the registry (read-only).",
                    &[],
                    &[],
                    obj_schema(&["query"], &[("query", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "registry_catalog",
                    "hh.lab/registry_catalog/1",
                    ToolGroup::Read,
                    "lab.registry.catalog",
                    "read_only",
                    "The registry catalogue (read-only).",
                    &[],
                    &[],
                    obj_schema(&[], &[]),
                    Some(t_obj()),
                    None,
                ),
                // ── analysis group (C2 §2.2 — `lab.analysis.*`; the
                // op's own spend charges `instrument`) ────────────────
                tool(
                    "analyze",
                    "hh.lab/analyze/1",
                    ToolGroup::Analysis,
                    "lab.analysis.analyze",
                    "read_only",
                    "`analyze{AnalysisSpec}` — summarize/compare/interaction/frontier/transfer/equivalence/rank/strata_view/diagnostics per the spec member.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "render_analysis",
                    "hh.lab/render_analysis/1",
                    ToolGroup::Analysis,
                    "lab.analysis.render",
                    "read_only",
                    "Render a comparison/analysis report to a display form.",
                    &[],
                    &[],
                    obj_schema(&["report"], &[("report", t_str()), ("format", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "diff_reports",
                    "hh.lab/diff_reports/1",
                    ToolGroup::Analysis,
                    "lab.analysis.diff_reports",
                    "read_only",
                    "Diff two content-addressed analysis reports.",
                    &[],
                    &[],
                    obj_schema(&["a", "b"], &[("a", t_str()), ("b", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "power_analysis",
                    "hh.lab/power_analysis/1",
                    ToolGroup::Analysis,
                    "lab.analysis.power",
                    "read_only",
                    "`power` — the sample-size/sensitivity analysis op.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "component_targets",
                    "hh.lab/component_targets/1",
                    ToolGroup::Analysis,
                    "lab.analysis.component_targets",
                    "read_only",
                    "Component-target attribution rows over the results store.",
                    &[],
                    &[],
                    obj_schema(&[], &[("query", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "attribution_design",
                    "hh.lab/attribution_design/1",
                    ToolGroup::Analysis,
                    "lab.analysis.attribution_design",
                    "read_only",
                    "The attribution-design analysis view.",
                    &[],
                    &[],
                    obj_schema(&["spec"], &[("spec", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                // ── write group (C2 §2.2 — `memory_write`, the
                // delegate write-gate applies) ────────────────────────
                tool(
                    "assembly_apply",
                    "hh.lab/assembly_apply/1",
                    ToolGroup::Write,
                    "lab.assembly.apply",
                    "write",
                    "Apply an assembly plan — a `memory_write{project}`; delegate callers need a covering sealed `Permission`.",
                    &[],
                    &[],
                    obj_schema(&["assembly"], &[("assembly", t_obj()), ("plan", t_obj()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "assembly_adopt",
                    "hh.lab/assembly_adopt/1",
                    ToolGroup::Write,
                    "lab.assembly.adopt",
                    "write",
                    "Adopt an applied assembly into the live registry state.",
                    &[],
                    &[],
                    obj_schema(&["assembly"], &[("assembly", t_obj()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "registry_register",
                    "hh.lab/registry_register/1",
                    ToolGroup::Write,
                    "lab.registry.register",
                    "write",
                    "Register a component/recipe record (`memory_write`; delegate-gated).",
                    &[],
                    &[],
                    obj_schema(&["record"], &[("record", t_obj()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "registry_publish",
                    "hh.lab/registry_publish/1",
                    ToolGroup::Write,
                    "lab.registry.publish",
                    "write",
                    "Publish a registered record to the organisation scope (`memory_write{organisation}`; delegate-gated).",
                    &[],
                    &[],
                    obj_schema(&["ref"], &[("ref", t_str()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "record_conformance",
                    "hh.lab/record_conformance/1",
                    ToolGroup::Write,
                    "lab.registry.record_conformance",
                    "write",
                    "Record a conformance verdict row for a registered record.",
                    &[],
                    &[],
                    obj_schema(&["record"], &[("record", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "leaderboard_define",
                    "hh.lab/leaderboard_define/1",
                    ToolGroup::Write,
                    "lab.leaderboard.define",
                    "write",
                    "Define a leaderboard (`memory_write`; delegate-gated).",
                    &[],
                    &[],
                    obj_schema(&["leaderboard"], &[("leaderboard", t_obj()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "leaderboard_publish",
                    "hh.lab/leaderboard_publish/1",
                    ToolGroup::Write,
                    "lab.leaderboard.publish",
                    "write",
                    "Publish a leaderboard definition.",
                    &[],
                    &[],
                    obj_schema(&["leaderboard"], &[("leaderboard", t_str()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "leaderboard_retract",
                    "hh.lab/leaderboard_retract/1",
                    ToolGroup::Write,
                    "lab.leaderboard.retract_entry",
                    "write",
                    "Retract one leaderboard entry.",
                    &[],
                    &[],
                    obj_schema(&["leaderboard", "entry"], &[("leaderboard", t_str()), ("entry", t_str()), ("registrar", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "grant_approver",
                    "hh.lab/grant_approver/1",
                    ToolGroup::Write,
                    "lab.permission.grant_approver",
                    "write",
                    "Grant an `ApproverGrant` (ADR-0070 D6) — a delegate may hold a scoped approval grant through this write.",
                    &[],
                    &[],
                    obj_schema(&["grant"], &[("grant", t_obj())]),
                    Some(t_obj()),
                    None,
                ),
                tool(
                    "revoke_approver",
                    "hh.lab/revoke_approver/1",
                    ToolGroup::Write,
                    "lab.permission.revoke_approver",
                    "write",
                    "Revoke an `ApproverGrant`.",
                    &[],
                    &[],
                    obj_schema(&["grant"], &[("grant", t_str())]),
                    Some(t_obj()),
                    None,
                ),
                // ── hosting group (C2 §2.2 — `serve_bundle`) ─────────
                tool(
                    "serve_bundle",
                    "hh.lab/serve_bundle/1",
                    ToolGroup::Hosting,
                    "lab.serve",
                    "launch",
                    "Serve a bundle's compiled surface to an external agent or hosting adapter — `spawn_process`-class; returns `connection_info` and mints `session_handle`.",
                    &[],
                    &[("bundle_ref", "container")],
                    obj_schema(
                        &["bundle_ref"],
                        &[("bundle_ref", t_str()), ("transport", t_str()), ("target", t_str())],
                    ),
                    Some(t_obj()),
                    None,
                ),
            ]),
        ),
        ("supply_surfaces", Json::Arr(vec![])),
        (
            "assumption_debt",
            Json::Arr(vec![
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-1")),
                    ("carrier", Json::str("tasks_carrier")),
                    (
                        "statement",
                        Json::str(
                            "The `io.modelcontextprotocol/tasks` carrier is itself assumption debt (ADR-0175 D6, reflexive): hypothesis — MCP-only callers need durable task handles and push; expiry — the extension's own deprecation clock.",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "no binding of kind `agent | service | provider_client` has used the tasks carrier (`_meta` extension declaration, `tasks/get|update|cancel`) in the last N releases.",
                        ),
                    ),
                ]),
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-6")),
                    ("carrier", Json::str("resource_carrier")),
                    (
                        "statement",
                        Json::str(
                            "The ledger-resource carrier (`hh://run/{…}/ledger|status`, `hh://results/{…}`, `resources/subscribe` + `notifications/resources/updated`) is itself assumption debt (ADR-0175 D6, reflexive): hypothesis — MCP-only callers need push over polling; a subscription is a declared sink bound to its caller binding and cancelled when the binding changes or expires.",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "no binding of kind `agent | service | provider_client` has held a live resource subscription in the last N releases.",
                        ),
                    ),
                ]),
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-2")),
                    ("carrier", Json::str("legacy_era_projection")),
                    (
                        "statement",
                        Json::str(
                            "Only the modern-era projection is served; the legacy-era profile (ADR-0097 D2 `narrowed` loss class) is the fixture server's arm — the Lab catalogue pins the modern revision and `server/discover` is always answered.",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "a legacy-era client negotiates `initialize` on the Lab server and receives the declared legacy projection with its loss record, and the dual-era conformance script passes (AC-R-2.11.3-11).",
                        ),
                    ),
                ]),
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-3")),
                    ("carrier", Json::str("caller_binding.stdio_launch")),
                    (
                        "statement",
                        Json::str(
                            "The `stdio_launch` credential binds the launcher's declared principal (`principal:test` in the shipped default); the launched_by/launch_event audit members are advisory at C1.",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "a deployed launch writes the launch_event record and the binding's audit members resolve to a durable launch row.",
                        ),
                    ),
                ]),
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-4")),
                    ("carrier", Json::str("caller_binding.oauth")),
                    (
                        "statement",
                        Json::str(
                            "OAuth bearer resolution is the `CallerAuth` seam — the H3 broker's production wiring (issuer keying, audience validation, step-up) is the mediator's; the slice resolves against a sealed binding table (the fixture authorization server's grant map).",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "a bearer minted by the production AS resolves through the broker path and the grant's `issuer_ref`/`audience` are verified, per the §7.3 H3 flow.",
                        ),
                    ),
                ]),
                Json::obj([
                    ("assumption_id", Json::str("AD-MCP-5")),
                    ("carrier", Json::str("caller_binding.mtls")),
                    (
                        "statement",
                        Json::str(
                            "The `mtls` credential kind is declared (`cert_ref`) for record completeness; no mTLS transport or resolver lands at C1 — it is the C2 arm.",
                        ),
                    ),
                    (
                        "removal_test",
                        Json::str(
                            "an mTLS-bound caller resolves a `cert_ref` through the TrustRootPolicy path and `tools/call` executes under it.",
                        ),
                    ),
                ]),
            ]),
        ),
        ("ext", Json::obj([])),
    ])
}

/// The shipped `stdio_launch` binding (the `principal:test` fixture
/// default — a deployment seals its own `caller_bindings[]`).
fn stdio_binding_json() -> Json {
    crate::binding::stdio_launch_binding("stdio", crate::binding::CallerKind::Agent).to_json()
}

/// The shipped `service` binding — a `subject_kind = client` OAuth
/// record (client credentials; R-1 pins `caller_kind ∈ {service,
/// provider_client}` for client subjects). `service` callers run
/// `unattended` — `ask → deny` — and delegate-ceilinged.
fn service_binding_json() -> Json {
    crate::binding::CallerBinding {
        binding_id: "bind-service-default".to_string(),
        credential: crate::binding::CallerCredential::OAuth {
            issuer_ref: "issuer:fixture".to_string(),
            subject_kind: crate::binding::SubjectKind::Client,
            audience: "mcp://hh-lab".to_string(),
        },
        principal_ref: "service:test".to_string(),
        caller_kind: crate::binding::CallerKind::Service,
        authority_cap: Json::obj([("class", Json::str("delegate"))]),
        permissions: vec![],
        budget_node: "pool:service".to_string(),
        pool: Json::obj([("dimensions", Json::obj([]))]),
        readers_identity: "service:test".to_string(),
        rate_policy: None,
        expires_at_ms: None,
    }
    .to_json()
}

/// The shipped `provider_client` binding — the model-provider client
/// (client credentials, delegate ceiling; OQ-398 stays open — its
/// charges land `instrument` on the surface session).
fn provider_binding_json() -> Json {
    crate::binding::CallerBinding {
        binding_id: "bind-provider-default".to_string(),
        credential: crate::binding::CallerCredential::OAuth {
            issuer_ref: "issuer:fixture".to_string(),
            subject_kind: crate::binding::SubjectKind::Client,
            audience: "mcp://hh-lab".to_string(),
        },
        principal_ref: "provider:test".to_string(),
        caller_kind: crate::binding::CallerKind::ProviderClient,
        authority_cap: Json::obj([("class", Json::str("delegate"))]),
        permissions: vec![],
        budget_node: "pool:provider".to_string(),
        pool: Json::obj([("dimensions", Json::obj([]))]),
        readers_identity: "provider:test".to_string(),
        rate_policy: None,
        expires_at_ms: None,
    }
    .to_json()
}

/// The default exposure definition, linked — the shipped `hh-lab/1`.
pub fn default_exposure() -> ExposureDef {
    let doc = default_lab_document();
    parse_exposure(&doc).expect("the shipped hh-lab/1 document links")
}
