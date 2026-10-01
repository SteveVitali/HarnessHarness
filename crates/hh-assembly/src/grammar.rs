//! The typed assembly grammar (§3.3.2; ADR-0023). A Harness Definition is one `hir/1`
//! document whose `assembly` member carries the configuration-and-composition record:
//! `Assembly{dialect, profile_binding, slots, parameters, values, entities, constraints,
//! layers?, resolved?, ext}`. The section is **data** — no host-language content; code
//! enters only through `CompiledPayload` leaves or registry-resolved variant pins
//! (Q-L1-09). `ext` is the only place unknown keys may live; any other unknown key is a
//! `C-LOAD-3` error.
//!
//! The typed model sits in `hh-assembly`; the *document* carries the section as canonical
//! `Json` (`HirDocument.assembly`) and `seal` admits it only when no `version_selector` or
//! `$param:`/`$entity:` binding form remains (§3.1.3 "only resolved documents are
//! sealable"). The slot records themselves are the *same* `SlotBinding`/`SlotBindings`
//! the `native.slots` member carries — one shape, one codec (`hh_hir::wire`, CC7).

use std::collections::BTreeMap;

use hh_hir::records::SlotBindings;
use hh_hir::refs::{ProfileRef, Ref};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::diagnostics::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};

/// The one assembly grammar dialect (`hir/1`'s assembly section — CC8: growth is an
/// additive bump).
pub const ASSEMBLY_DIALECT: &str = "HIR/1";

/// The `$param:<id>` binding-form prefix (§3.3.2 — resolved by `resolve`, never sealed).
pub const PARAM_MARKER: &str = "$param:";
/// The `$entity:<id>` binding-form prefix (§3.3.2).
pub const ENTITY_MARKER: &str = "$entity:";
/// The `$secret:<name>` channel-name prefix (§3.3.4 — secrets resolve to channel names;
/// the marker *survives* `resolve` — it names a channel, never a value).
pub const SECRET_MARKER: &str = "$secret:";

/// `profile_binding ∈ {unbound | ProfileRef | ProfileConstraint}` (§3.3.2). `unbound` is
/// the default — the profile is a *configuration coordinate* `link` binds (T-LCD-04);
/// a pinned `ProfileRef` makes the definition `non_portable = true` (OQ-078).
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileBinding {
    /// The definition constrains or pins no profile.
    Unbound,
    /// A pinned (or selector-carrying, authored) `ProfileRef` — `non_portable`.
    Pinned(ProfileRef),
    /// A `ProfileConstraint` — the §5b-owned shape (C1/Stage 5); carried opaquely here.
    Constraint(Json),
}

impl ProfileBinding {
    /// `non_portable` — a pinned profile binding (OQ-078).
    pub fn is_non_portable(&self) -> bool {
        matches!(self, ProfileBinding::Pinned(p) if p.pinned)
    }
}

/// `ParameterSpec.required ∈ {required, optional, prompt}` (§3.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamRequirement {
    /// The definition must carry a value (or a `default`).
    Required,
    /// Optional.
    Optional,
    /// The runner prompts at bind time.
    Prompt,
}

impl ParamRequirement {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ParamRequirement::Required => "required",
            ParamRequirement::Optional => "optional",
            ParamRequirement::Prompt => "prompt",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<ParamRequirement> {
        match s {
            "required" => Some(ParamRequirement::Required),
            "optional" => Some(ParamRequirement::Optional),
            "prompt" => Some(ParamRequirement::Prompt),
            _ => None,
        }
    }
}

/// `ParameterSpec.type` — the closed type sum (§3.3.2): `int | real | bool | enum |
/// string | duration | token_count | ref<class_id> | ref<entity_kind>`. `enum`'s
/// `domain` carries the allowed values. `ref<…>` types name a parameter whose value is
/// a reference — to a component class's bound variant (`ClassRef`) or a document entity
/// of a kind (`EntityRef`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamType {
    /// `int`.
    Int,
    /// `real`.
    Real,
    /// `bool`.
    Bool,
    /// `enum` — `domain` is the allowed-value list.
    Enum,
    /// `string`.
    Str,
    /// `duration` (canonical duration payload).
    Duration,
    /// `token_count`.
    TokenCount,
    /// `ref<class_id>` — a component-class reference.
    ClassRef(String),
    /// `ref<entity_kind>` — an entity-kind reference.
    EntityRef(String),
}

impl ParamType {
    /// The canonical spelling (`ref<class_id>` / `ref<entity:kind>` — `ref<x>` parses as
    /// a class ref per the spec's `ref<class_id>` notation; entity refs spell the
    /// `entity:` qualifier, ADR-0240).
    pub fn as_str(&self) -> String {
        match self {
            ParamType::Int => "int".into(),
            ParamType::Real => "real".into(),
            ParamType::Bool => "bool".into(),
            ParamType::Enum => "enum".into(),
            ParamType::Str => "string".into(),
            ParamType::Duration => "duration".into(),
            ParamType::TokenCount => "token_count".into(),
            ParamType::ClassRef(c) => format!("ref<{c}>"),
            ParamType::EntityRef(k) => format!("ref<entity:{k}>"),
        }
    }

    /// Parse the closed sum (`ref<x>` / `ref<class:x>` → class ref; `ref<entity:x>` →
    /// entity ref).
    pub fn parse(s: &str) -> Option<ParamType> {
        match s {
            "int" => return Some(ParamType::Int),
            "real" => return Some(ParamType::Real),
            "bool" => return Some(ParamType::Bool),
            "enum" => return Some(ParamType::Enum),
            "string" => return Some(ParamType::Str),
            "duration" => return Some(ParamType::Duration),
            "token_count" => return Some(ParamType::TokenCount),
            _ => {}
        }
        let inner = s.strip_prefix("ref<")?.strip_suffix('>')?;
        if let Some(e) = inner.strip_prefix("entity:") {
            return Some(ParamType::EntityRef(e.to_string()));
        }
        if let Some(c) = inner.strip_prefix("class:") {
            return Some(ParamType::ClassRef(c.to_string()));
        }
        Some(ParamType::ClassRef(inner.to_string()))
    }
}

/// `ParameterSpec{type, domain, default?, required, unit?, sweepable, affects[],
/// budget_relevant}` (§3.3.2) — the declared parameter space, never inferred from code.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterSpec {
    /// The value type (closed sum).
    pub param_type: ParamType,
    /// The sweepable domain (`[…]` or `{min,max}`); mandatory when `sweepable` or `enum`.
    pub domain: Option<Json>,
    /// The default value, when declared.
    pub default: Option<Json>,
    /// `required | optional | prompt`.
    pub required: ParamRequirement,
    /// The unit, when declared.
    pub unit: Option<String>,
    /// Whether the sweep engine may enumerate this parameter.
    pub sweepable: bool,
    /// Declaration fields this parameter affects (causal annotation).
    pub affects: Vec<String>,
    /// Whether the parameter moves budget consumption (`C-PARAM-5` guards a default-only
    /// binding).
    pub budget_relevant: bool,
}

/// `Constraint.kind` — the closed set (§3.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// `requires` — `subject.of` bound/valued requires `subject.needs` bound/valued.
    Requires,
    /// `conflicts` — `subject.of` and `subject.with` may not both hold.
    Conflicts,
    /// `implies` — `subject.of` holding implies `subject.needs` holds.
    Implies,
    /// `range` — `subject.of`'s value within `{min, max}`.
    Range,
    /// `authority_cap` — the authority ceiling over `subject.of` (monotone, §3.3.3).
    AuthorityCap,
}

impl ConstraintKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConstraintKind::Requires => "requires",
            ConstraintKind::Conflicts => "conflicts",
            ConstraintKind::Implies => "implies",
            ConstraintKind::Range => "range",
            ConstraintKind::AuthorityCap => "authority_cap",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<ConstraintKind> {
        match s {
            "requires" => Some(ConstraintKind::Requires),
            "conflicts" => Some(ConstraintKind::Conflicts),
            "implies" => Some(ConstraintKind::Implies),
            "range" => Some(ConstraintKind::Range),
            "authority_cap" => Some(ConstraintKind::AuthorityCap),
            _ => None,
        }
    }
}

/// `LayerProvenance{source_kind, id, version, precedence}` (§3.3.3) — the layer that
/// authored a member (the `source_layer` of diagnostics, and a `Constraint`'s `source`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerProvenance {
    /// The layer's source kind (closed sum).
    pub source_kind: LayerSourceKind,
    /// The layer id (the `layers[]` entry key; `source_layer` on diagnostics).
    pub id: String,
    /// The layer version.
    pub version: String,
    /// The precedence (higher wins where the merge policy allows).
    pub precedence: i64,
}

/// `LayerProvenance.source_kind ∈ {packaged-default, organisation, user, project,
/// experiment, session}` (§3.3.3). The experiment layer is an ordinary layer so
/// attribution can name the override (OQ-076).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSourceKind {
    /// `packaged-default`.
    PackagedDefault,
    /// `organisation`.
    Organisation,
    /// `user`.
    User,
    /// `project`.
    Project,
    /// `experiment`.
    Experiment,
    /// `session`.
    Session,
}

impl LayerSourceKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LayerSourceKind::PackagedDefault => "packaged-default",
            LayerSourceKind::Organisation => "organisation",
            LayerSourceKind::User => "user",
            LayerSourceKind::Project => "project",
            LayerSourceKind::Experiment => "experiment",
            LayerSourceKind::Session => "session",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<LayerSourceKind> {
        match s {
            "packaged-default" => Some(LayerSourceKind::PackagedDefault),
            "organisation" => Some(LayerSourceKind::Organisation),
            "user" => Some(LayerSourceKind::User),
            "project" => Some(LayerSourceKind::Project),
            "experiment" => Some(LayerSourceKind::Experiment),
            "session" => Some(LayerSourceKind::Session),
            _ => None,
        }
    }
}

/// `Constraint{kind, subject, source}` (§3.3.2). `subject` is the structured operand
/// record — `{of, needs?, with?, min?, max?, ceiling?}` spellings per kind (ADR-0240);
/// `source` is the authoring layer's provenance (mandatory on composed documents —
/// `C-COMP-3` when absent where layers exist).
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    /// The constraint kind.
    pub kind: ConstraintKind,
    /// The structured operand record.
    pub subject: Json,
    /// The authoring layer's provenance.
    pub source: Option<LayerProvenance>,
}

/// An `entities` member value — `Ref | Entity` (§3.3.2): a §3.1 entity referenced by
/// slots and values. At C0/Stage 1 an `Entity` is an inline declaration
/// `{kind: <entity_kind>, record: <payload>}` — carried as data; `resolve` pins only the
/// `Ref` form (ADR-0240).
#[derive(Debug, Clone, PartialEq)]
pub enum EntityBinding {
    /// A `Ref` — usually document-internal (`semantic_id` → a node).
    Ref(Ref),
    /// An inline entity declaration (`kind` + payload).
    Inline {
        /// The declared entity kind.
        kind: String,
        /// The entity payload (kind-member records; opaque here).
        record: Json,
    },
}

/// The `resolved` member `resolve` writes — `{registry_snapshot_id, resolved_at}` —
/// never authored (§3.3.6: the sealed definition records the snapshot it resolved
/// against).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInfo {
    /// The registry snapshot the resolve was confined to.
    pub registry_snapshot_id: String,
    /// The resolve timestamp (ms).
    pub resolved_at: u64,
}

/// The extension merge policy (§5g.5 §3; S1.23): how two sources' candidates
/// combine under one name. `exact_only` is the default — a ref binds exactly
/// one pinned record and a second candidate for the same name is a collision;
/// `disjoint` declares that the sources bind disjoint namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergePolicy {
    /// Exact-only merge (the default).
    #[default]
    ExactOnly,
    /// Declared disjoint namespaces.
    Disjoint,
}

/// The `extensions` member (§5g.5 §3; S1.23): `{sources, refs, merge_policy}` —
/// the **declared** extension surface. Sources are declared, never implicit
/// (CF-079 — there is no silent home/project directory scan); every ref's
/// locator scheme must be covered by a declared source (L1); refs pin at
/// `resolve` (`locator.resolved`/`fetched_at`, `content`, `extension_id`); a
/// selector or missing pin reaching a sealed form fails `UnpinnedInSealedForm`
/// (L4; AC-R-2.8.5-1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionBlock {
    /// The declared sources the refs' locators resolve under.
    pub sources: Vec<hh_registry::extension::DeclaredSource>,
    /// The extension references (selector-bearing before `resolve`, pinned after).
    pub refs: Vec<hh_registry::extension::ExtensionRef>,
    /// The declared merge policy (`exact_only` by default).
    pub merge_policy: MergePolicy,
}

/// The typed `Assembly` section (§3.3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Assembly {
    /// The grammar dialect (`HIR/1`).
    pub dialect: String,
    /// `profile_binding ∈ {unbound | ProfileRef | ProfileConstraint}`.
    pub profile_binding: ProfileBinding,
    /// `slots: map<slot_name, SlotBinding | [SlotBinding]>` — the §3.3.2 record shared
    /// with `native.slots` (`resolve` materialises it onto the root `native` process).
    pub slots: BTreeMap<String, SlotBindings>,
    /// `parameters: map<param_id, ParameterSpec>` — the declared space.
    pub parameters: BTreeMap<String, ParameterSpec>,
    /// `values: map<param_id, Value>` — the definition's point in the space.
    pub values: BTreeMap<String, Json>,
    /// `entities: map<entity_id, Ref | Entity>`.
    pub entities: BTreeMap<String, EntityBinding>,
    /// `constraints: [Constraint]`.
    pub constraints: Vec<Constraint>,
    /// `layers?: [LayerProvenance]` — present on composed documents only.
    pub layers: Option<Vec<LayerProvenance>>,
    /// `extensions?: {sources, refs, merge_policy}` — the declared extension
    /// surface (§5g.5 §3; S1.23): declared sources only (no implicit scanning,
    /// CF-079), refs pinned at `resolve`, `UnpinnedInSealedForm` at `seal` (L4).
    pub extensions: Option<ExtensionBlock>,
    /// `resolved?` — the member `resolve` writes (never authored).
    pub resolved: Option<ResolvedInfo>,
    /// `ext` — the only place unknown keys may live.
    pub ext: BTreeMap<String, Json>,
}

impl Assembly {
    /// An empty section at the `HIR/1` dialect, profile `unbound`.
    pub fn empty() -> Assembly {
        Assembly {
            dialect: ASSEMBLY_DIALECT.into(),
            profile_binding: ProfileBinding::Unbound,
            slots: BTreeMap::new(),
            parameters: BTreeMap::new(),
            values: BTreeMap::new(),
            entities: BTreeMap::new(),
            constraints: Vec::new(),
            layers: None,
            extensions: None,
            resolved: None,
            ext: BTreeMap::new(),
        }
    }

    /// Whether the section is in *resolved* form — no `version_selector`, no
    /// `$param:`/`$entity:` binding form anywhere (`$secret:` channel names survive
    /// resolve — they are names, never values), and every extension ref pinned
    /// (§5g.5 L4 — an unpinned ref is not a resolved section). Mirrors `seal`'s
    /// admission check.
    pub fn is_resolved(&self) -> bool {
        unresolved_in_json(&self.to_json(), "assembly").is_none()
            && self
                .extensions
                .as_ref()
                .map(|e| e.refs.iter().all(|r| r.is_pinned()))
                .unwrap_or(true)
    }

    /// The canonical JSON encoding (the member `HirDocument.assembly` carries).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("dialect".into(), Json::str(&self.dialect));
        m.insert(
            "profile_binding".into(),
            profile_binding_json(&self.profile_binding),
        );
        m.insert("slots".into(), hh_hir::wire::slots_json(&self.slots, false));
        m.insert(
            "parameters".into(),
            Json::Obj(
                self.parameters
                    .iter()
                    .map(|(k, v)| (k.clone(), parameter_spec_json(v)))
                    .collect(),
            ),
        );
        m.insert("values".into(), Json::Obj(self.values.clone()));
        m.insert(
            "entities".into(),
            Json::Obj(
                self.entities
                    .iter()
                    .map(|(k, v)| (k.clone(), entity_binding_json(v)))
                    .collect(),
            ),
        );
        m.insert(
            "constraints".into(),
            Json::Arr(self.constraints.iter().map(constraint_json).collect()),
        );
        if let Some(layers) = &self.layers {
            m.insert(
                "layers".into(),
                Json::Arr(layers.iter().map(layer_json).collect()),
            );
        }
        if let Some(e) = &self.extensions {
            m.insert("extensions".into(), extension_block_json(e));
        }
        if let Some(r) = &self.resolved {
            m.insert(
                "resolved".into(),
                Json::obj([
                    ("registry_snapshot_id", Json::str(&r.registry_snapshot_id)),
                    ("resolved_at", Json::Int(r.resolved_at as i64)),
                ]),
            );
        }
        m.insert("ext".into(), Json::Obj(self.ext.clone()));
        Json::Obj(m)
    }

    /// Member-wise decode — **never fail-fast**: each undecodable member produces a
    /// `C-LOAD-*` diagnostic and falls back to the member default so later stages still
    /// see the rest (§3.3.8 "every stage runs and reports"). Unknown non-`ext` keys are
    /// `C-LOAD-3`. `kernel` mints the `detail: Text{owner = kernel}` leaves.
    pub fn from_json(
        j: &Json,
        path: &str,
        kernel: &ProvenanceRecord,
        diags: &mut Vec<AssemblyDiagnostic>,
    ) -> Option<Assembly> {
        let m = match j {
            Json::Obj(m) => m,
            _ => {
                diags.push(diag(
                    Code::LoadParse,
                    path,
                    "assembly",
                    "assembly member must be an object",
                    "author the assembly section as an object",
                    kernel,
                ));
                return None;
            }
        };
        const KNOWN: &[&str] = &[
            "dialect",
            "profile_binding",
            "slots",
            "parameters",
            "values",
            "entities",
            "constraints",
            "layers",
            "extensions",
            "resolved",
            "ext",
        ];
        for k in m.keys() {
            if !KNOWN.contains(&k.as_str()) {
                diags.push(diag(
                    Code::LoadUnknownKey,
                    &format!("{path}/{k}"),
                    k,
                    "unknown assembly member — only `ext` may carry undeclared keys",
                    "move the member under `ext` or remove it",
                    kernel,
                ));
            }
        }
        let mut a = Assembly::empty();
        if let Some(d) = m.get("dialect") {
            match d.as_str() {
                Some(s) => a.dialect = s.to_string(),
                None => diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/dialect"),
                    "dialect",
                    "`dialect` must be a string",
                    "author `dialect` as the grammar dialect string",
                    kernel,
                )),
            }
        }
        if let Some(p) = m.get("profile_binding") {
            match profile_binding_from_json(p) {
                Ok(pb) => a.profile_binding = pb,
                Err(detail) => diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/profile_binding"),
                    "profile_binding",
                    &detail,
                    "author `unbound`, a `ProfileRef`, or `{\"constraint\": …}`",
                    kernel,
                )),
            }
        }
        if let Some(s) = m.get("slots") {
            match hh_hir::wire::slots_from_json(s, &format!("{path}/slots")) {
                Ok(slots) => a.slots = slots,
                Err(e) => diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/slots"),
                    "slots",
                    &format!("slots member fails the §3.3.2 grammar: {e:?}"),
                    "author `slots` as map<slot_name, SlotBinding | [SlotBinding]>",
                    kernel,
                )),
            }
        }
        if let Some(p) = m.get("parameters") {
            if let Json::Obj(pm) = p {
                for (id, spec) in pm {
                    match parameter_spec_from_json(spec, &format!("{path}/parameters/{id}")) {
                        Ok(ps) => {
                            a.parameters.insert(id.clone(), ps);
                        }
                        Err(detail) => diags.push(diag(
                            Code::LoadParse,
                            &format!("{path}/parameters/{id}"),
                            id,
                            &detail,
                            "author the parameter as a `ParameterSpec` record",
                            kernel,
                        )),
                    }
                }
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/parameters"),
                    "parameters",
                    "`parameters` must be an object",
                    "author `parameters` as map<param_id, ParameterSpec>",
                    kernel,
                ));
            }
        }
        if let Some(v) = m.get("values") {
            if let Json::Obj(vm) = v {
                a.values = vm.clone();
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/values"),
                    "values",
                    "`values` must be an object",
                    "author `values` as map<param_id, Value>",
                    kernel,
                ));
            }
        }
        if let Some(e) = m.get("entities") {
            if let Json::Obj(em) = e {
                for (id, eb) in em {
                    match entity_binding_from_json(eb, &format!("{path}/entities/{id}")) {
                        Ok(b) => {
                            a.entities.insert(id.clone(), b);
                        }
                        Err(detail) => diags.push(diag(
                            Code::LoadParse,
                            &format!("{path}/entities/{id}"),
                            id,
                            &detail,
                            "author the entity as a `Ref` or `{kind, record}`",
                            kernel,
                        )),
                    }
                }
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/entities"),
                    "entities",
                    "`entities` must be an object",
                    "author `entities` as map<entity_id, Ref | Entity>",
                    kernel,
                ));
            }
        }
        if let Some(c) = m.get("constraints") {
            if let Json::Arr(items) = c {
                for (i, cj) in items.iter().enumerate() {
                    match constraint_from_json(cj, &format!("{path}/constraints/{i}")) {
                        Ok(cn) => a.constraints.push(cn),
                        Err(detail) => diags.push(diag(
                            Code::LoadParse,
                            &format!("{path}/constraints/{i}"),
                            &format!("constraints[{i}]"),
                            &detail,
                            "author the constraint as `{kind, subject, source}`",
                            kernel,
                        )),
                    }
                }
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/constraints"),
                    "constraints",
                    "`constraints` must be a list",
                    "author `constraints` as [Constraint]",
                    kernel,
                ));
            }
        }
        if let Some(l) = m.get("layers") {
            if let Json::Arr(items) = l {
                let mut layers = Vec::new();
                for (i, lj) in items.iter().enumerate() {
                    match layer_from_json(lj, &format!("{path}/layers/{i}")) {
                        Ok(lp) => layers.push(lp),
                        Err(detail) => diags.push(diag(
                            Code::LoadParse,
                            &format!("{path}/layers/{i}"),
                            &format!("layers[{i}]"),
                            &detail,
                            "author the layer as `{source_kind, id, version, precedence}`",
                            kernel,
                        )),
                    }
                }
                a.layers = Some(layers);
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/layers"),
                    "layers",
                    "`layers` must be a list",
                    "author `layers` as [LayerProvenance]",
                    kernel,
                ));
            }
        }
        if let Some(e) = m.get("extensions") {
            match extension_block_from_json(e, &format!("{path}/extensions")) {
                Ok(eb) => a.extensions = Some(eb),
                Err(detail) => diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/extensions"),
                    "extensions",
                    &detail,
                    "author `extensions` as `{sources: [DeclaredSource], refs: [ExtensionRef], merge_policy}`",
                    kernel,
                )),
            }
        }
        if let Some(r) = m.get("resolved") {
            match resolved_from_json(r, &format!("{path}/resolved")) {
                Ok(ri) => a.resolved = Some(ri),
                Err(detail) => diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/resolved"),
                    "resolved",
                    &detail,
                    "`resolved` is written by `resolve`, never authored",
                    kernel,
                )),
            }
        }
        if let Some(x) = m.get("ext") {
            if let Json::Obj(xm) = x {
                a.ext = xm.clone();
            } else {
                diags.push(diag(
                    Code::LoadParse,
                    &format!("{path}/ext"),
                    "ext",
                    "`ext` must be an object",
                    "author `ext` as an object of extension members",
                    kernel,
                ));
            }
        }
        Some(a)
    }
}

// ── member codecs ────────────────────────────────────────────────────────────

fn profile_binding_json(p: &ProfileBinding) -> Json {
    match p {
        ProfileBinding::Unbound => Json::str("unbound"),
        ProfileBinding::Pinned(pr) => Json::obj([
            ("profile_ref", Json::str(&pr.profile)),
            ("pinned", Json::Bool(pr.pinned)),
        ]),
        ProfileBinding::Constraint(c) => Json::obj([("constraint", c.clone())]),
    }
}

fn profile_binding_from_json(j: &Json) -> Result<ProfileBinding, String> {
    match j {
        Json::Str(s) if s == "unbound" => Ok(ProfileBinding::Unbound),
        Json::Str(s) => Err(format!("unknown profile_binding spelling `{s}`")),
        Json::Obj(m) => {
            if let Some(c) = m.get("constraint") {
                return Ok(ProfileBinding::Constraint(c.clone()));
            }
            let profile = m
                .get("profile_ref")
                .and_then(Json::as_str)
                .ok_or("`profile_binding` object must carry `profile_ref` or `constraint`")?;
            let pinned = matches!(m.get("pinned"), Some(Json::Bool(true)));
            Ok(ProfileBinding::Pinned(ProfileRef {
                profile: profile.to_string(),
                pinned,
            }))
        }
        _ => Err("`profile_binding` must be a string or object".into()),
    }
}

pub(crate) fn parameter_spec_json(p: &ParameterSpec) -> Json {
    let mut m = BTreeMap::new();
    m.insert("type".into(), Json::str(p.param_type.as_str()));
    m.insert("required".into(), Json::str(p.required.as_str()));
    m.insert("sweepable".into(), Json::Bool(p.sweepable));
    m.insert("budget_relevant".into(), Json::Bool(p.budget_relevant));
    m.insert(
        "affects".into(),
        Json::Arr(p.affects.iter().map(Json::str).collect()),
    );
    if let Some(d) = &p.domain {
        m.insert("domain".into(), d.clone());
    }
    if let Some(d) = &p.default {
        m.insert("default".into(), d.clone());
    }
    if let Some(u) = &p.unit {
        m.insert("unit".into(), Json::str(u));
    }
    Json::Obj(m)
}

pub fn parameter_spec_from_json(j: &Json, path: &str) -> Result<ParameterSpec, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path} must be an object")),
    };
    for k in m.keys() {
        if !matches!(
            k.as_str(),
            "type"
                | "domain"
                | "default"
                | "required"
                | "unit"
                | "sweepable"
                | "affects"
                | "budget_relevant"
        ) {
            return Err(format!("{path}.{k}: unknown ParameterSpec member"));
        }
    }
    let ty = m
        .get("type")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.type is required"))?;
    let param_type =
        ParamType::parse(ty).ok_or_else(|| format!("{path}.type: unknown type `{ty}`"))?;
    let required = match m.get("required") {
        None => ParamRequirement::Optional,
        Some(Json::Str(s)) => ParamRequirement::parse(s)
            .ok_or_else(|| format!("{path}.required: unknown spelling `{s}`"))?,
        Some(_) => return Err(format!("{path}.required must be a string")),
    };
    let affects = match m.get("affects") {
        None => Vec::new(),
        Some(Json::Arr(items)) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{path}.affects members must be strings"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(format!("{path}.affects must be a list")),
    };
    let unit = match m.get("unit") {
        None => None,
        Some(Json::Str(s)) => Some(s.clone()),
        Some(_) => return Err(format!("{path}.unit must be a string")),
    };
    let bool_member = |name: &str| -> Result<bool, String> {
        match m.get(name) {
            None => Ok(false),
            Some(Json::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("{path}.{name} must be a boolean")),
        }
    };
    Ok(ParameterSpec {
        param_type,
        domain: m.get("domain").cloned(),
        default: m.get("default").cloned(),
        required,
        unit,
        sweepable: bool_member("sweepable")?,
        affects,
        budget_relevant: bool_member("budget_relevant")?,
    })
}

fn entity_binding_json(e: &EntityBinding) -> Json {
    match e {
        EntityBinding::Ref(r) => r.to_json(),
        EntityBinding::Inline { kind, record } => {
            Json::obj([("kind", Json::str(kind)), ("record", record.clone())])
        }
    }
}

pub fn entity_binding_from_json(j: &Json, path: &str) -> Result<EntityBinding, String> {
    if let Json::Obj(m) = j {
        if m.contains_key("semantic_id") {
            return Ref::from_json(j, path)
                .map(EntityBinding::Ref)
                .map_err(|e| format!("{path}: {e:?}"));
        }
        if let (Some(Json::Str(kind)), Some(record)) = (m.get("kind"), m.get("record")) {
            return Ok(EntityBinding::Inline {
                kind: kind.clone(),
                record: record.clone(),
            });
        }
    }
    Err(format!("{path} must be a `Ref` or `{{kind, record}}`"))
}

fn constraint_json(c: &Constraint) -> Json {
    let mut m = BTreeMap::new();
    m.insert("kind".into(), Json::str(c.kind.as_str()));
    m.insert("subject".into(), c.subject.clone());
    if let Some(s) = &c.source {
        m.insert("source".into(), layer_json(s));
    }
    Json::Obj(m)
}

pub fn constraint_from_json(j: &Json, path: &str) -> Result<Constraint, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path} must be an object")),
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "kind" | "subject" | "source") {
            return Err(format!("{path}.{k}: unknown Constraint member"));
        }
    }
    let kind = m
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.kind is required"))
        .and_then(|s| {
            ConstraintKind::parse(s).ok_or_else(|| format!("{path}.kind: unknown spelling `{s}`"))
        })?;
    let subject = m
        .get("subject")
        .cloned()
        .ok_or_else(|| format!("{path}.subject is required"))?;
    let source = match m.get("source") {
        None => None,
        Some(s) => Some(layer_from_json(s, &format!("{path}.source"))?),
    };
    Ok(Constraint {
        kind,
        subject,
        source,
    })
}

/// The canonical JSON of one `LayerProvenance` (`{source_kind, id, version,
/// precedence}` — §3.3.3; exported for `hh-lab`'s `AssemblySource` codec — CC7).
pub fn layer_json(l: &LayerProvenance) -> Json {
    Json::obj([
        ("source_kind", Json::str(l.source_kind.as_str())),
        ("id", Json::str(&l.id)),
        ("version", Json::str(&l.version)),
        ("precedence", Json::Int(l.precedence)),
    ])
}

/// Parse one `LayerProvenance` (the codec's read direction — strict member set).
pub fn layer_from_json(j: &Json, path: &str) -> Result<LayerProvenance, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path} must be an object")),
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "source_kind" | "id" | "version" | "precedence") {
            return Err(format!("{path}.{k}: unknown LayerProvenance member"));
        }
    }
    let source_kind = m
        .get("source_kind")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.source_kind is required"))
        .and_then(|s| {
            LayerSourceKind::parse(s)
                .ok_or_else(|| format!("{path}.source_kind: unknown spelling `{s}`"))
        })?;
    let id = m
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.id is required"))?;
    let version = m
        .get("version")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.version is required"))?;
    let precedence = match m.get("precedence") {
        Some(Json::Int(p)) => *p,
        _ => return Err(format!("{path}.precedence must be an integer")),
    };
    Ok(LayerProvenance {
        source_kind,
        id: id.to_string(),
        version: version.to_string(),
        precedence,
    })
}

fn resolved_from_json(j: &Json, path: &str) -> Result<ResolvedInfo, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path} must be an object")),
    };
    let snap = m
        .get("registry_snapshot_id")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.registry_snapshot_id is required"))?;
    let at = match m.get("resolved_at") {
        Some(Json::Int(ms)) => *ms as u64,
        _ => return Err(format!("{path}.resolved_at must be an integer")),
    };
    Ok(ResolvedInfo {
        registry_snapshot_id: snap.to_string(),
        resolved_at: at,
    })
}

fn extension_block_json(e: &ExtensionBlock) -> Json {
    Json::obj([
        (
            "sources",
            Json::Arr(
                e.sources
                    .iter()
                    .map(hh_registry::extension::declared_source_json)
                    .collect(),
            ),
        ),
        (
            "refs",
            Json::Arr(
                e.refs
                    .iter()
                    .map(hh_registry::extension::extension_ref_json)
                    .collect(),
            ),
        ),
        (
            "merge_policy",
            Json::str(match e.merge_policy {
                MergePolicy::ExactOnly => "exact_only",
                MergePolicy::Disjoint => "disjoint",
            }),
        ),
    ])
}

fn extension_block_from_json(j: &Json, path: &str) -> Result<ExtensionBlock, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path} must be an object")),
    };
    let mut sources = Vec::new();
    match m.get("sources") {
        Some(Json::Arr(items)) => {
            for (i, s) in items.iter().enumerate() {
                let sp = format!("{path}/sources/{i}");
                sources.push(
                    hh_registry::extension::declared_source_from_json(s, &sp)
                        .map_err(|e| format!("{sp}: {e:?}"))?,
                );
            }
        }
        Some(_) => return Err(format!("{path}.sources must be a list")),
        None => {}
    }
    let mut refs = Vec::new();
    match m.get("refs") {
        Some(Json::Arr(items)) => {
            for (i, r) in items.iter().enumerate() {
                let rp = format!("{path}/refs/{i}");
                refs.push(
                    hh_registry::extension::extension_ref_from_json(r, &rp)
                        .map_err(|e| format!("{rp}: {e:?}"))?,
                );
            }
        }
        Some(_) => return Err(format!("{path}.refs must be a list")),
        None => {}
    }
    let merge_policy = match m.get("merge_policy").and_then(Json::as_str) {
        None | Some("exact_only") => MergePolicy::ExactOnly,
        Some("disjoint") => MergePolicy::Disjoint,
        Some(other) => return Err(format!("{path}.merge_policy: unknown spelling `{other}`")),
    };
    Ok(ExtensionBlock {
        sources,
        refs,
        merge_policy,
    })
}

/// The `seal`-mirroring unresolved walk over canonical JSON: a `version_selector`
/// member or a `$param:`/`$entity:` string is unresolved; `$secret:` channel names
/// survive (§3.3.4). Returns the first offending path.
pub fn unresolved_in_json(j: &Json, path: &str) -> Option<String> {
    match j {
        Json::Obj(m) => {
            if m.contains_key("version_selector") {
                return Some(format!("{path}: version_selector"));
            }
            for (k, v) in m {
                if let Some(p) = unresolved_in_json(v, &format!("{path}/{k}")) {
                    return Some(p);
                }
            }
            None
        }
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| unresolved_in_json(v, &format!("{path}/{i}"))),
        Json::Str(s) if s.starts_with(PARAM_MARKER) || s.starts_with(ENTITY_MARKER) => {
            Some(format!("{path}: {s}"))
        }
        _ => None,
    }
}

/// Every `$param:<id>` / `$entity:<id>` / `$secret:<name>` marker under a JSON value —
/// `(path, marker)` pairs, deterministic order.
pub fn markers_in_json(j: &Json, path: &str, out: &mut Vec<(String, String)>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                markers_in_json(v, &format!("{path}/{k}"), out);
            }
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                markers_in_json(v, &format!("{path}[{i}]"), out);
            }
        }
        Json::Str(s)
            if s.starts_with(PARAM_MARKER)
                || s.starts_with(ENTITY_MARKER)
                || s.starts_with(SECRET_MARKER) =>
        {
            out.push((path.to_string(), s.clone()));
        }
        _ => {}
    }
}

/// A shorthand for building an `AssemblyDiagnostic` at `Stage::Validate(1)` (load-time
/// grammar failures).
fn diag(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity: Severity::Error,
        path: path.to_string(),
        source_layer: None,
        subject: subject.to_string(),
        stage: Stage::Validate(1),
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0148".into(),
    }
}
