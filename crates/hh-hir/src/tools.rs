//! The tool-plane vocabulary shared by the registry, the compiler, the monitor
//! and the exposure slice (spec §5d; ADR-0087…0095): the `ScopeBinding` record
//! and `scope_kind` closed sum, the scoped-domain table the scope-totality
//! invariant reads, the run-time `ExposureMode` sum, the `discover_surfaces`
//! capability declaration, and the record-level half of the V-E1 invariant
//! battery (the document-level half lives in [`crate::validate`]).
//!
//! One schema source (CC7): `hh_monitor::args` re-exports [`ScopeKind`],
//! [`ScopeBinding`] and [`scope_bindings`] from here — no second reader of the
//! `scope_bindings` member exists.

use std::collections::BTreeSet;

use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::errors::HirError;
use crate::kinds::{EffectDomain, ToolEffects, World};
use crate::leaves::Text;
use crate::records::{Resources, ScopeBindings, ToolCapabilityRecord};

// ── ScopeBinding / ScopeKind (§5d.1 §3; ADR-0087 D3) ─────────────────────────

/// `scope_kind ∈ {fs_path, host, recipient, spend_amount, secret_ref,
/// process_target, memory_scope, resource_key}` (§05d; ADR-0087 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// A filesystem path.
    FsPath,
    /// A network host.
    Host,
    /// A human recipient.
    Recipient,
    /// A spend amount.
    SpendAmount,
    /// A secret reference.
    SecretRef,
    /// A process target.
    ProcessTarget,
    /// A memory persistence scope.
    MemoryScope,
    /// A `share`-mode resource key.
    ResourceKey,
}

impl ScopeKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::FsPath => "fs_path",
            ScopeKind::Host => "host",
            ScopeKind::Recipient => "recipient",
            ScopeKind::SpendAmount => "spend_amount",
            ScopeKind::SecretRef => "secret_ref",
            ScopeKind::ProcessTarget => "process_target",
            ScopeKind::MemoryScope => "memory_scope",
            ScopeKind::ResourceKey => "resource_key",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<ScopeKind> {
        match s {
            "fs_path" => Some(ScopeKind::FsPath),
            "host" => Some(ScopeKind::Host),
            "recipient" => Some(ScopeKind::Recipient),
            "spend_amount" => Some(ScopeKind::SpendAmount),
            "secret_ref" => Some(ScopeKind::SecretRef),
            "process_target" => Some(ScopeKind::ProcessTarget),
            "memory_scope" => Some(ScopeKind::MemoryScope),
            "resource_key" => Some(ScopeKind::ResourceKey),
            _ => None,
        }
    }
}

/// `ScopeBinding = {param_path, scope_kind, canonicalization?}` (§5d.1 §3;
/// ADR-0087 D3). The record is read from the capability's `scope_bindings`
/// JSON member — a `ScopeBinding` value is `{param_path, scope_kind,
/// canonicalization?}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeBinding {
    /// The canonical parameter path the binding selects (`a.b` dotted).
    pub param_path: String,
    /// The closed scope kind.
    pub scope_kind: ScopeKind,
    /// The declared canonicalization (e.g. `path_canonical`, `host_lower`) —
    /// the transform's name is recorded, its application is R-2.8.4's.
    pub canonicalization: Option<String>,
}

/// Parse a `ScopeBindings` record into the binding list. `Bindings(json)` is
/// `[{param_path, scope_kind, canonicalization?}]` (a single-binding object is
/// admitted); `Unknown` ⇒ `None` (the `scope_bindings_unknown` member — a call
/// then matches `*` grants only, ADR-0087 D3).
///
/// A malformed member (missing `param_path`, a `scope_kind` outside the closed
/// sum) is `Err` — never silently dropped (V-E1-2/CC3).
pub fn scope_bindings(s: &ScopeBindings) -> Result<Option<Vec<ScopeBinding>>, HirError> {
    match s {
        ScopeBindings::Unknown => Ok(None),
        ScopeBindings::Bindings(j) => {
            let rows = match j {
                Json::Arr(rows) => rows.clone(),
                Json::Null => Vec::new(),
                other => vec![other.clone()], // a single-binding record
            };
            let mut out = Vec::new();
            for (i, r) in rows.iter().enumerate() {
                let path = r.get("param_path").and_then(Json::as_str).ok_or_else(|| {
                    HirError::SchemaViolation {
                        detail: format!("scope_bindings[{i}].param_path missing"),
                    }
                })?;
                let kind = r
                    .get("scope_kind")
                    .and_then(Json::as_str)
                    .ok_or_else(|| HirError::SchemaViolation {
                        detail: format!("scope_bindings[{i}].scope_kind missing"),
                    })
                    .and_then(|s| {
                        ScopeKind::parse(s).ok_or_else(|| HirError::UnknownKind {
                            kind: format!("scope_kind {s}"),
                        })
                    })?;
                out.push(ScopeBinding {
                    param_path: path.to_string(),
                    scope_kind: kind,
                    canonicalization: r
                        .get("canonicalization")
                        .and_then(Json::as_str)
                        .map(String::from),
                });
            }
            Ok(Some(out))
        }
    }
}

/// The scoped-domain table (§5d.1 §3 scope totality; ADR-0087 D3): the
/// `ScopeKind` an effect domain's `scope_bindings` row must carry. `None` = the
/// domain is not scope-bearing at C0 (`permission_request`, `model_call`).
/// `resource_key` scopes `share`-mode resource leases, not an effect domain.
///
/// (Owned decision — see the S1.17 ADR: the spec names the eight `scope_kind`s
/// but pins no domain→kind table; this is the minimal faithful mapping.)
pub fn domain_scope_kind(d: EffectDomain) -> Option<ScopeKind> {
    match d {
        EffectDomain::FsRead | EffectDomain::FsWrite => Some(ScopeKind::FsPath),
        EffectDomain::NetEgress => Some(ScopeKind::Host),
        EffectDomain::MessageHuman => Some(ScopeKind::Recipient),
        EffectDomain::Spend => Some(ScopeKind::SpendAmount),
        EffectDomain::SecretAccess => Some(ScopeKind::SecretRef),
        EffectDomain::Exec | EffectDomain::SpawnProcess => Some(ScopeKind::ProcessTarget),
        EffectDomain::MemoryWrite => Some(ScopeKind::MemoryScope),
        EffectDomain::PermissionRequest | EffectDomain::ModelCall => None,
    }
}

// ── The run-time exposure-mode sum (§5d.3 §3; ADR-0093 D1) ───────────────────

/// `ExposureMode` (run-time; closed kernel sum — `direct | indexed | deferred |
/// code_mode | hidden`): how a compiled surface is presented per model call.
/// **Distinct** from the compile-time `ExposureMode` of §5d.2
/// (`primitive | split | composite | freeform | code_mode | shim` — how a
/// capability set is *rendered*) and from the authored `ToolSurface.
/// exposure_mode` member (`primitive | composite_member | hidden`).
///
/// `hidden` is never delivered, indexed or callable; `code_mode` is C2
/// extension behaviour (callable only from an executed program — never a
/// `check_callable`-callable plan mode at C0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExposureMode {
    /// The full compiled definition is delivered.
    Direct,
    /// Only the typed `IndexForm` is delivered (`handle_only`).
    Indexed,
    /// Nothing delivered; reachable only via discovery.
    Deferred,
    /// Callable only from an executed program (C2).
    CodeMode,
    /// Never delivered, indexed or callable.
    Hidden,
}

impl ExposureMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ExposureMode::Direct => "direct",
            ExposureMode::Indexed => "indexed",
            ExposureMode::Deferred => "deferred",
            ExposureMode::CodeMode => "code_mode",
            ExposureMode::Hidden => "hidden",
        }
    }

    /// Every member, in declaration order.
    pub const ALL: [ExposureMode; 5] = [
        ExposureMode::Direct,
        ExposureMode::Indexed,
        ExposureMode::Deferred,
        ExposureMode::CodeMode,
        ExposureMode::Hidden,
    ];

    /// Parse the closed sum; unknown spellings are refused (never coerced).
    pub fn parse(s: &str) -> Option<ExposureMode> {
        ExposureMode::ALL.iter().copied().find(|m| m.as_str() == s)
    }
}

/// The authored `ToolSurface.exposure_mode` member read at `bind` (OQ-240
/// interim ruling — see the S1.17 ADR): `{mode ∈ {primitive, composite_member,
/// hidden}, pinned?, admitted_modes?: [ExposureMode]}`. Defaults:
/// `mode = primitive`, `pinned = false`, `admitted_modes = {direct}`.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredExposure {
    /// The authored rendering kind (`hidden` removes the surface from every
    /// plan, index, discovery result and catalogue export — AC-R-2.5.3-12).
    pub hidden: bool,
    /// `pinned` — definition/kernel-set, immutable to policy (I-NARROW);
    /// pinned surfaces stay `direct` and cannot be evicted.
    pub pinned: bool,
    /// The definition-side admitted run-time modes (narrow-only at plan time).
    pub admitted: BTreeSet<ExposureMode>,
}

/// Parse the authored `exposure_mode` member. Absent member ⇒ the default
/// (`{direct}`, not pinned, not hidden). An `admitted_modes` spelling outside
/// the run-time sum is a schema violation (closed sums only — V-E1-2).
pub fn authored_exposure(j: Option<&Json>) -> Result<AuthoredExposure, HirError> {
    let mut out = AuthoredExposure {
        hidden: false,
        pinned: false,
        admitted: [ExposureMode::Direct].into_iter().collect(),
    };
    let Some(j) = j else { return Ok(out) };
    // `Json::Null` is the schema's unset marker for structured slots — absent.
    if matches!(j, Json::Null) {
        return Ok(out);
    }
    let Json::Obj(_) = j else {
        return Err(HirError::SchemaViolation {
            detail: "exposure_mode must be an object".into(),
        });
    };
    match j.get("mode").and_then(Json::as_str) {
        None | Some("primitive") => {}
        Some("hidden") => out.hidden = true,
        Some("composite_member") => {}
        Some(other) => {
            return Err(HirError::UnknownKind {
                kind: format!("exposure_mode.mode {other}"),
            })
        }
    }
    if let Some(p) = j.get("pinned") {
        match p {
            Json::Bool(b) => out.pinned = *b,
            _ => {
                return Err(HirError::SchemaViolation {
                    detail: "exposure_mode.pinned must be a bool".into(),
                })
            }
        }
    }
    if let Some(Json::Arr(modes)) = j.get("admitted_modes") {
        out.admitted = BTreeSet::new();
        for m in modes {
            let s = m.as_str().ok_or_else(|| HirError::SchemaViolation {
                detail: "exposure_mode.admitted_modes members must be strings".into(),
            })?;
            let mode = ExposureMode::parse(s).ok_or_else(|| HirError::UnknownKind {
                kind: format!("admitted_modes {s}"),
            })?;
            out.admitted.insert(mode);
        }
    }
    Ok(out)
}

// ── param_path ∈ input_schema (V-E1-4) ───────────────────────────────────────

/// `param_path` resolves in `input_schema` (V-E1-4 → `UnmappedParameter`). A
/// dotted path descends `properties`; a schema carrying no `properties`
/// constraint but admitting additional members (`additionalProperties` /
/// `unevaluatedProperties` truthy, a non-object schema, or a combinator that
/// cannot be disproved statically — `anyOf`/`oneOf`/`allOf`/`$ref`) is
/// permissive for the unresolved tail: the check is fail-closed on *declared
/// closed* shapes and honest about undecidable ones (documented at S1.17 — an
/// unprovable path under an open schema is admitted, a path outside a closed
/// `properties` set is `UnmappedParameter`).
pub fn param_path_exists(schema: &Json, path: &str) -> bool {
    let mut cur = schema;
    for seg in path.split('.') {
        // Combinators we cannot disprove statically are permissive.
        if cur.get("anyOf").is_some()
            || cur.get("oneOf").is_some()
            || cur.get("allOf").is_some()
            || cur.get("$ref").is_some()
        {
            return true;
        }
        match cur.get("properties").and_then(|p| p.get(seg)) {
            Some(next) => cur = next,
            None => {
                // No declared property for this segment — admissible only when
                // the schema admits additional members.
                let additional = matches!(
                    cur.get("additionalProperties"),
                    Some(Json::Bool(true)) | Some(Json::Obj(_))
                ) || matches!(
                    cur.get("unevaluatedProperties"),
                    Some(Json::Bool(true)) | Some(Json::Obj(_))
                );
                let unconstrained = cur.get("properties").is_none()
                    && !matches!(cur.get("type"), Some(t) if t.as_str() == Some("object"));
                return additional || unconstrained;
            }
        }
    }
    true
}

// ── V-E1 record checks (§5d.1 §3; ADR-0087 D6) ───────────────────────────────

/// The lifted source kinds — declarations are `unverified` (ADR-0034 P7) and
/// the record registers `quarantined` (ADR-0088 amendment; CF-210).
pub fn lifted_source_kind(source: &Json) -> Option<&'static str> {
    match source.get("kind").and_then(Json::as_str) {
        Some("mcp_listing") => Some("mcp_listing"),
        Some("participant_supplied") => Some("participant_supplied"),
        _ => None,
    }
}

/// The capability's declared source kind, if carried.
pub fn source_kind(source: &Json) -> Option<String> {
    source.get("kind").and_then(Json::as_str).map(String::from)
}

/// Whether the record is the kernel's `discover_surfaces` capability (R-2.5.3
/// §3 — the resolver adds it to any sealed definition that admits `deferred`).
/// The C0 marker is `exposure_hint.discovery = true` (declared on the hint —
/// an input to run-time selection, excluded from `semantic_id`).
pub fn is_discovery_capability(rec: &ToolCapabilityRecord) -> bool {
    rec.exposure_hint.get("discovery") == Some(&Json::Bool(true))
}

/// Whether the capability's declared effects gate it at `seal` without a
/// `flow_contract` (§5g.2 §3; AC-R-2.8.2-1): `world = open` on any declared
/// effect, or a domain in `{net_egress, message_human, fs_read, memory_write}`.
/// `pure` capabilities and closed-world effects outside the gated domains
/// seal contractless — the gate covers the flows C2 must see (egress,
/// human-directed messaging, untrusted reads, shared-memory writes).
pub fn capability_needs_flow_contract(rec: &ToolCapabilityRecord) -> bool {
    if let ToolEffects::Declared(set) = &rec.effects {
        for e in set {
            let gated_domain = matches!(
                e.domain,
                EffectDomain::NetEgress
                    | EffectDomain::MessageHuman
                    | EffectDomain::FsRead
                    | EffectDomain::MemoryWrite
            );
            let open = e
                .attributes
                .as_ref()
                .map(|a| a.world == World::Open)
                .unwrap_or(false);
            if gated_domain || open {
                return true;
            }
        }
    }
    false
}

/// The record-level V-E1 battery (§5d.1 §3; ADR-0087 D6) — checks 1, 3, 4, 7, 8
/// (`authority` is the node's `provenance.authority` — conferred, never read
/// from content). The document-level checks live in [`crate::validate`]: V-E1-2
/// (closed sums — the codec refuses unknown members), V-E1-5 (T-LCD-01
/// model-identity statics), V-E1-6 (`Text` provenance), V-E1-9 (identity
/// projection — `exposure_hint`/`cost_model.measured_ref` excluded),
/// V-E1-10 (`procedure`-source derived-effects equality).
///
/// Every failure is a typed `HirError` — collected, never a warning.
pub fn validate_capability(rec: &ToolCapabilityRecord, authority: AuthorityClass) -> Vec<HirError> {
    let mut errs = Vec::new();

    // V-E1-1: non-empty effects or `pure`.
    if let ToolEffects::Declared(set) = &rec.effects {
        if set.is_empty() {
            errs.push(HirError::EmptyEffectSet);
        }
    }

    // V-E1-3/4: scope totality + `param_path ∈ input_schema`.
    match scope_bindings(&rec.scope_bindings) {
        Err(e) => errs.push(e),
        Ok(None) => {
            // `scope_bindings_unknown` — declared unknown covers every scoped
            // domain (coverable only by unscoped `*` grants).
        }
        Ok(Some(rows)) => {
            for b in &rows {
                if !param_path_exists(&rec.input_schema, &b.param_path) {
                    errs.push(HirError::UnmappedParameter {
                        param_path: b.param_path.clone(),
                    });
                }
            }
            if let ToolEffects::Declared(set) = &rec.effects {
                for e in set {
                    if let Some(kind) = domain_scope_kind(e.domain) {
                        if !rows.iter().any(|b| b.scope_kind == kind) {
                            errs.push(HirError::UnscopedParameter {
                                domain: e.domain.name().to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // V-E1-7: lifted ⇒ `unverified` (a `pin` attestation raises it — the raised
    // record is a *new version*; a lifted declaration never mints above
    // `unverified` at register).
    if let Some(kind) = lifted_source_kind(&rec.source) {
        if authority != AuthorityClass::Unverified {
            errs.push(HirError::LiftedDeclarationsNotUnverified {
                source_kind: kind.to_string(),
            });
        }
    }

    // V-E1-8: `exact` cost only with `measured_ref` (ADR-0089 D1).
    if let Some(cm) = &rec.cost_model {
        let has_measured = cm.get("measured_ref").is_some();
        if let Some(Json::Obj(declared)) = cm.get("declared") {
            for (dim, est) in declared {
                if est.get("confidence").and_then(Json::as_str) == Some("exact") && !has_measured {
                    errs.push(HirError::ExactCostUnmeasured {
                        dimension: dim.clone(),
                    });
                }
            }
        }
    }

    // V-E1-3 resources shape sanity: `declared ⇒ keys` (tri-state honesty).
    if let Resources::Declared(keys) = &rec.resources {
        let _ = keys; // declared-with-keys is the honest form; no rule to add.
    }

    // R-2.8.2 (§5g.2 §3): a declared `flow_contract` must be a well-formed
    // `FlowContract` — `hh_provenance::flow` owns the shape (CC7), so a
    // malformed member is `SchemaViolation` here; and every parameter the
    // contract names (`recipient_params`, `content_params`, rule-selector
    // `params`, `forall_param` bounds) must resolve in `input_schema`
    // (`UnmappedParameter` — the same V-E1-4 refusal).
    if let Some(f) = &rec.flow_contract {
        match hh_provenance::flow::FlowContract::from_json(f) {
            Err(e) => errs.push(HirError::SchemaViolation {
                detail: format!("flow_contract: {}", e.detail),
            }),
            Ok(contract) => {
                let mut check_param = |p: &String| {
                    if !param_path_exists(&rec.input_schema, p) {
                        errs.push(HirError::UnmappedParameter {
                            param_path: p.clone(),
                        });
                    }
                };
                for p in contract
                    .recipient_params
                    .iter()
                    .chain(contract.content_params.iter())
                {
                    check_param(p);
                }
                for r in &contract.rules {
                    for p in &r.selector.params {
                        check_param(p);
                    }
                }
            }
        }
    }

    errs
}

// ── discover_surfaces (§5d.3 §3; ADR-0094 D1) ────────────────────────────────

/// The `DiscoveryQuery` schema (the `discover_surfaces` input):
/// `{form ∈ {regex, natural_language, structured{namespace?, effect_filter?,
/// tags?, source?}}, text, limit?}`.
pub fn discovery_query_schema() -> Json {
    Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([
                (
                    "form",
                    Json::obj([
                        ("type", Json::str("string")),
                        (
                            "enum",
                            Json::Arr(vec![
                                Json::str("regex"),
                                Json::str("natural_language"),
                                Json::str("structured"),
                            ]),
                        ),
                    ]),
                ),
                ("text", Json::obj([("type", Json::str("string"))])),
                ("limit", Json::obj([("type", Json::str("integer"))])),
                (
                    "structured",
                    Json::obj([
                        ("type", Json::str("object")),
                        (
                            "properties",
                            Json::obj([
                                ("namespace", Json::obj([("type", Json::str("string"))])),
                                ("effect_filter", Json::obj([("type", Json::str("string"))])),
                                (
                                    "tags",
                                    Json::obj([
                                        ("type", Json::str("array")),
                                        ("items", Json::obj([("type", Json::str("string"))])),
                                    ]),
                                ),
                                ("source", Json::obj([("type", Json::str("string"))])),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "required",
            Json::Arr(vec![Json::str("form"), Json::str("text")]),
        ),
    ])
}

/// The `DiscoveryResult` schema (the `discover_surfaces` output):
/// `{hits[{surface_id, rank, score?}], truncated, executed_by, cost}`.
pub fn discovery_result_schema() -> Json {
    Json::obj([
        ("type", Json::str("object")),
        (
            "properties",
            Json::obj([
                (
                    "hits",
                    Json::obj([
                        ("type", Json::str("array")),
                        (
                            "items",
                            Json::obj([
                                ("type", Json::str("object")),
                                (
                                    "properties",
                                    Json::obj([
                                        ("surface_id", Json::obj([("type", Json::str("string"))])),
                                        ("rank", Json::obj([("type", Json::str("integer"))])),
                                        ("score", Json::Null),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
                ("truncated", Json::obj([("type", Json::str("boolean"))])),
                ("executed_by", Json::obj([("type", Json::str("string"))])),
                ("cost", Json::Null),
            ]),
        ),
    ])
}

/// The `discover_surfaces` capability record (§5d.3 §3; ADR-0094 D1):
/// `effects = pure`, `input_schema = DiscoveryQuery`, `output_schema =
/// DiscoveryResult`, `exposure_hint = {discovery: true, default: direct}` —
/// the resolver adds it to any sealed definition that admits `deferred`, and
/// `select_surfaces`/`validate` require it `direct` whenever a plan defers
/// (I-DISCOVERY). `purpose` is supplied by the caller (a `Text` leaf carries
/// its provenance — never defaulted, CC2).
pub fn discover_surfaces_capability(purpose: Text) -> ToolCapabilityRecord {
    ToolCapabilityRecord {
        purpose,
        input_schema: discovery_query_schema(),
        output_schema: Some(discovery_result_schema()),
        effects: ToolEffects::Pure,
        preconditions: Vec::new(),
        scope_bindings: ScopeBindings::Bindings(Json::Arr(vec![])),
        resources: Resources::NoneDeclared,
        observation_contract: Json::obj([
            ("content_class", Json::str("public")),
            ("streaming", Json::Bool(false)),
            ("error_classes", Json::Arr(vec![])),
        ]),
        cost_model: None,
        execution_requirement: Json::obj([("environment_class", Json::str("kernel_internal"))]),
        source: Json::obj([("kind", Json::str("kernel"))]),
        exposure_hint: Json::obj([
            ("discovery", Json::Bool(true)),
            ("default", Json::str("direct")),
        ]),
        postconditions: Vec::new(),
        flow_contract: None,
        action_patterns: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::{
        EffectAttributes, EffectClass, Mutability, RepeatSafety, Reversibility, World,
    };

    fn attrs() -> EffectAttributes {
        EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Closed,
            reversibility: Reversibility::Compensable,
        }
    }

    fn purpose() -> Text {
        Text::new(
            "does a thing",
            "test:owner",
            hh_provenance::ProvenanceRecord::minted(
                hh_provenance::Origin::human("test:author", hh_provenance::HumanRole::Author),
                hh_provenance::PersistenceScope::Definition,
                0,
            ),
        )
    }

    fn cap(effects: ToolEffects, scope: ScopeBindings) -> ToolCapabilityRecord {
        ToolCapabilityRecord {
            purpose: purpose(),
            input_schema: Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([
                        ("path", Json::obj([("type", Json::str("string"))])),
                        ("host", Json::obj([("type", Json::str("string"))])),
                    ]),
                ),
            ]),
            output_schema: None,
            effects,
            preconditions: Vec::new(),
            scope_bindings: scope,
            resources: Resources::NoneDeclared,
            observation_contract: Json::obj([("error_classes", Json::Arr(vec![]))]),
            cost_model: None,
            execution_requirement: Json::obj([("environment_class", Json::str("kernel_internal"))]),
            source: Json::obj([
                ("kind", Json::str("native_variant")),
                ("ref", Json::str("v")),
            ]),
            exposure_hint: Json::obj([("default", Json::str("direct"))]),
            postconditions: Vec::new(),
            flow_contract: None,
            action_patterns: Vec::new(),
        }
    }

    fn binding(path: &str, kind: &str) -> Json {
        Json::obj([
            ("param_path", Json::str(path)),
            ("scope_kind", Json::str(kind)),
        ])
    }

    #[test]
    fn v_e1_1_empty_declared_effects_fail() {
        let rec = cap(
            ToolEffects::Declared(BTreeSet::new()),
            ScopeBindings::Unknown,
        );
        let errs = validate_capability(&rec, AuthorityClass::Definition);
        assert!(errs.contains(&HirError::EmptyEffectSet));
        // `pure` is the honest empty.
        let rec = cap(ToolEffects::Pure, ScopeBindings::Unknown);
        assert!(validate_capability(&rec, AuthorityClass::Definition).is_empty());
    }

    #[test]
    fn v_e1_3_scoped_domain_needs_binding_or_unknown() {
        let effects = ToolEffects::Declared(
            [EffectClass {
                domain: EffectDomain::FsWrite,
                attributes: Some(attrs()),
            }]
            .into_iter()
            .collect(),
        );
        // No binding, not unknown → UnscopedParameter.
        let rec = cap(effects.clone(), ScopeBindings::Bindings(Json::Arr(vec![])));
        assert!(
            validate_capability(&rec, AuthorityClass::Definition).contains(
                &HirError::UnscopedParameter {
                    domain: "fs_write".into()
                }
            )
        );
        // `scope_bindings_unknown` covers.
        let rec = cap(effects.clone(), ScopeBindings::Unknown);
        assert!(validate_capability(&rec, AuthorityClass::Definition).is_empty());
        // A matching binding covers.
        let rec = cap(
            effects,
            ScopeBindings::Bindings(Json::Arr(vec![binding("path", "fs_path")])),
        );
        assert!(validate_capability(&rec, AuthorityClass::Definition).is_empty());
    }

    #[test]
    fn v_e1_4_param_path_outside_schema_is_unmapped() {
        let effects = ToolEffects::Declared(
            [EffectClass {
                domain: EffectDomain::FsWrite,
                attributes: Some(attrs()),
            }]
            .into_iter()
            .collect(),
        );
        let rec = cap(
            effects,
            ScopeBindings::Bindings(Json::Arr(vec![binding("missing", "fs_path")])),
        );
        assert!(
            validate_capability(&rec, AuthorityClass::Definition).contains(
                &HirError::UnmappedParameter {
                    param_path: "missing".into()
                }
            )
        );
    }

    #[test]
    fn v_e1_7_lifted_must_be_unverified() {
        let mut rec = cap(ToolEffects::Pure, ScopeBindings::Unknown);
        rec.source = Json::obj([("kind", Json::str("mcp_listing"))]);
        assert!(validate_capability(&rec, AuthorityClass::Unverified).is_empty());
        let errs = validate_capability(&rec, AuthorityClass::External);
        assert!(errs
            .iter()
            .any(|e| matches!(e, HirError::LiftedDeclarationsNotUnverified { .. })));
    }

    #[test]
    fn v_e1_8_exact_cost_needs_measured_ref() {
        let mut rec = cap(ToolEffects::Pure, ScopeBindings::Unknown);
        rec.cost_model = Some(Json::obj([(
            "declared",
            Json::obj([(
                "time.wall_ms",
                Json::obj([("confidence", Json::str("exact")), ("value", Json::Int(5))]),
            )]),
        )]));
        let errs = validate_capability(&rec, AuthorityClass::Definition);
        assert!(errs
            .iter()
            .any(|e| matches!(e, HirError::ExactCostUnmeasured { .. })));
        // measured_ref discharges it.
        if let Some(Json::Obj(m)) = &mut rec.cost_model {
            m.insert("measured_ref".into(), Json::str("sha256:abc"));
        }
        assert!(validate_capability(&rec, AuthorityClass::Definition).is_empty());
    }

    #[test]
    fn exposure_mode_sum_is_closed() {
        for m in ExposureMode::ALL {
            assert_eq!(ExposureMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(ExposureMode::parse("expanded"), None);
    }
}
