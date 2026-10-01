//! `AssemblySource` (§6.1 data model; ADR-0147 D1) — the *one* record an author
//! writes: `{dialect, root_kind, base?, document?, layers[], overrides?,
//! imports?, hosted?, ext}`. The record is **never sealable** — it is an input
//! to `desugar`, which rewrites every authoring convenience into the ratified
//! `hir/1` constructs (D1–D5) before `compose`/`resolve` see them (A-2).
//!
//! Two members go beyond the minimal row the spec table lists, both owned by
//! this ticket's ruling (recorded in the run ledger + ADR):
//!
//! - `document` — the authored document scaffold `{root, nodes[], edges[]}`
//!   (canonical `hir/1` node/edge JSON, decoded by `hh_hir::wire`'s codecs —
//!   CC7). Required for a native source with no `base`; ignored content-wise
//!   for the assembly section (the composed layers own it).
//! - `hosted` — the typed home for the §6.1 §2.5 meta-harness members
//!   (`executor{harness, model?, auth[]}`, `tools`, `instructions`,
//!   `policies`, `budget`, `permissions`, `hosting_mechanism`,
//!   `declared_capabilities`, `participant_version`, `params`,
//!   `observability`) — legal only under `root_kind = hosted` (D5).

use std::collections::BTreeMap;

use hh_wire::json::Json;

/// The one source dialect (`hir/1` — the assembly grammar's dialect).
pub const SOURCE_DIALECT: &str = "hir/1";

/// `root_kind ∈ {native, hosted}` (§6.1; D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// A native process — every component first-class.
    Native,
    /// A hosted (opaque) participant — `AgentProcess.hosted = OpaqueProcess`.
    Hosted,
}

impl RootKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RootKind::Native => "native",
            RootKind::Hosted => "hosted",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<RootKind> {
        match s {
            "native" => Some(RootKind::Native),
            "hosted" => Some(RootKind::Hosted),
            _ => None,
        }
    }
}

/// One authored layer: `{provenance: LayerProvenance, fragment: <partial
/// Assembly JSON>}`. The fragment is carried as canonical JSON and decoded
/// member-wise by `hh_assembly::Assembly::from_json` at desugar — the kernel
/// codec is the one decoder (CC7; a malformed fragment is a `C-LOAD-*`
/// diagnostic, never a string error).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLayer {
    /// The layer's provenance (`source_kind`, `id`, `version`, `precedence`).
    pub provenance: hh_assembly::grammar::LayerProvenance,
    /// The authored fragment (a partial `Assembly`).
    pub fragment: Json,
}

/// The authored document scaffold: `{root: <semantic_id>, nodes: [<canonical
/// node JSON>], edges: [<canonical edge JSON>]}`. `root` names a node's
/// `semantic_id` — the document-level `Ref` is minted as
/// `selected(root, "latest")` and pinned by `resolve`/`seal` (S-2).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceDocument {
    /// The root node's `semantic_id`.
    pub root: String,
    /// Canonical `hir/1` node records.
    pub nodes: Vec<Json>,
    /// Canonical `hir/1` edge records.
    pub edges: Vec<Json>,
}

/// `imports[]` entry (D3): `{path_or_ref, as ∈ {context_item, procedure,
/// text}, content?, name?, frontmatter_schema?}`. `content` carries the
/// imported bytes inline — the service performs no IO; a caller reading from
/// a path inlines the file's text here. `path_or_ref` is recorded as the
/// import's `source_system` label on the minted `Text` leaves (provenance,
/// never a load).
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// The import coordinate (`path` or registry ref) — a provenance label.
    pub path_or_ref: String,
    /// `context_item | procedure | text`.
    pub as_: ImportKind,
    /// The inlined content (required at C0 — the desugar is pure).
    pub content: Option<String>,
    /// The entity's local name (the `entities[]` key / node label).
    pub name: Option<String>,
    /// A typed frontmatter constraint (`{"require": [keys], "types": {…}}`),
    /// enforced at desugar over `content`'s `---`-delimited header.
    pub frontmatter_schema: Option<Json>,
}

/// `Import.as` — the closed sum (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// A `ContextItem` entity.
    ContextItem,
    /// A `Procedure` entity.
    Procedure,
    /// A bare `Text` leaf (materialises as a `ContextItem` carrying it).
    Text,
}

impl ImportKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ImportKind::ContextItem => "context_item",
            ImportKind::Procedure => "procedure",
            ImportKind::Text => "text",
        }
    }

    /// Parse the closed sum (the `Procedure(skill)` spelling is accepted as
    /// `procedure` — the skill-tree lift is ADR-0086's, the import's `as`
    /// value only selects the entity kind).
    pub fn parse(s: &str) -> Option<ImportKind> {
        match s {
            "context_item" | "ContextItem" => Some(ImportKind::ContextItem),
            "procedure" | "Procedure" | "skill" | "Procedure(skill)" => Some(ImportKind::Procedure),
            "text" | "Text" => Some(ImportKind::Text),
            _ => None,
        }
    }
}

/// `executor.model` (§6.1 §2.5): `bound([model_set…])` — an M-set the
/// instrument binds — or `self_selected` (the participant chooses).
#[derive(Debug, Clone, PartialEq)]
pub enum ModelBinding {
    /// The participant self-selects its model.
    SelfSelected,
    /// A bound model set (participant coordinates).
    Bound(Vec<String>),
}

/// The `hosted` member (D5; §6.1 §2.5) — the meta-harness authoring block.
/// Every member desugars into an `OpaqueProcess` field or a supplied entity;
/// nothing here introduces a new IR type (A-2).
#[derive(Debug, Clone, PartialEq)]
pub struct HostedSpec {
    /// `executor.harness` — the participant coordinate (a `registry:` name
    /// selector or a pinned coordinate; pinned by `resolve`).
    pub harness: String,
    /// `executor.model` — `bound | self_selected`.
    pub model: Option<ModelBinding>,
    /// `auth` — `SecretRef` channel names (`$secret:<name>`); a value where a
    /// channel belongs is `C-SEC-1` at resolve.
    pub auth: Vec<String>,
    /// `tools` — `ToolCapability` declarations the instrument supplies
    /// (`supplies.tools`). Each entry is a kind-member record body plus an
    /// optional `name` (the node's authored `semantic_id` hint).
    pub tools: Vec<HostedSupply>,
    /// `instructions`/`prompt` — `ContextItem` records (`supplies.context`);
    /// `content` text mints `Text{authority ≤ external}` leaves.
    pub instructions: Vec<HostedSupply>,
    /// `policies` — `HarnessRule`/`Permission` node bodies.
    pub policies: Vec<HostedSupply>,
    /// `budget` — the `Budget` record body (required dimensions map).
    pub budget: Option<Json>,
    /// `permissions` — the `Permission` record body.
    pub permissions: Option<Json>,
    /// `hosting_mechanism` — `session_abi | model_boundary_intercept |
    /// container_installed` (`none` is refused — CF-351).
    pub hosting_mechanism: Option<String>,
    /// `declared_capabilities` — the fifteen-field tri-state map
    /// (`{streaming: supported, …}`); absent members read `unknown`.
    pub declared_capabilities: Option<Json>,
    /// `participant_version` — the participant's own version coordinate.
    pub participant_version: Option<String>,
    /// `params` — participant params (structured).
    pub params: Option<Json>,
    /// `observability` — declared observability levels.
    pub observability: Vec<String>,
    /// `parameters` — `ParameterSpec` map merged into the assembly
    /// (`sweepable` admissible only over `supported` coordinates — the
    /// capability check rides the parameter-space stage).
    pub parameters: BTreeMap<String, Json>,
    /// `values` — authored parameter values.
    pub values: BTreeMap<String, Json>,
}

/// A named kind-member record inside `hosted`: `{name?, kind?, record}` —
/// `kind` is required only on `policies` (`HarnessRule` | `Permission`);
/// `tools` fix `ToolCapability`, `instructions` fix `ContextItem`.
#[derive(Debug, Clone, PartialEq)]
pub struct HostedSupply {
    /// The entity's local name (becomes the node's authored `semantic_id`).
    pub name: Option<String>,
    /// The entity kind (policies only — `HarnessRule` | `Permission`).
    pub kind: Option<String>,
    /// The kind-member record body (canonical JSON).
    pub record: Json,
}

/// The `AssemblySource` record (§6.1; ADR-0147 D1). **Never sealable** — it
/// exists only as `desugar`'s input; `seal` never sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct AssemblySource {
    /// `dialect` — must be `hir/1`.
    pub dialect: String,
    /// `root_kind` — `native | hosted`.
    pub root_kind: RootKind,
    /// `base` — a `DefinitionRef` (`version:<vid>` or `ns/name[@label]`); D1
    /// makes it the lowest-precedence `packaged-default` layer.
    pub base: Option<String>,
    /// `document` — the authored scaffold `{root, nodes, edges}`.
    pub document: Option<SourceDocument>,
    /// `layers` — the authored layer stack.
    pub layers: Vec<SourceLayer>,
    /// `overrides` — the ADR-0025 spellings (`path=v`, `+path=v`, `++path=v`,
    /// `~path`, `path=v1,v2`, `class_id=variant_id`); D2 materialises exactly
    /// one `experiment` layer.
    pub overrides: Vec<String>,
    /// `overrides` member presence — `[]` still materialises the layer
    /// (D2 "always materialised": the member, not the content, decides).
    pub overrides_present: bool,
    /// `imports` — D3.
    pub imports: Vec<Import>,
    /// `hosted` — D5; legal only under `root_kind = hosted`.
    pub hosted: Option<HostedSpec>,
    /// `ext` — the only place unknown keys may live.
    pub ext: BTreeMap<String, Json>,
}

impl AssemblySource {
    /// The canonical JSON of the source (the codec's write direction — CC7:
    /// `hh-lab` owns the `AssemblySource` schema).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("dialect".into(), Json::str(&self.dialect));
        m.insert("root_kind".into(), Json::str(self.root_kind.as_str()));
        if let Some(b) = &self.base {
            m.insert("base".into(), Json::str(b.clone()));
        }
        if let Some(d) = &self.document {
            m.insert(
                "document".into(),
                Json::obj([
                    ("root", Json::str(d.root.clone())),
                    ("nodes", Json::Arr(d.nodes.clone())),
                    ("edges", Json::Arr(d.edges.clone())),
                ]),
            );
        }
        m.insert(
            "layers".into(),
            Json::Arr(
                self.layers
                    .iter()
                    .map(|l| {
                        Json::obj([
                            (
                                "provenance",
                                hh_assembly::grammar::layer_json(&l.provenance),
                            ),
                            ("fragment", l.fragment.clone()),
                        ])
                    })
                    .collect(),
            ),
        );
        if self.overrides_present {
            m.insert(
                "overrides".into(),
                Json::Arr(self.overrides.iter().map(Json::str).collect()),
            );
        }
        if !self.imports.is_empty() {
            m.insert(
                "imports".into(),
                Json::Arr(self.imports.iter().map(import_json).collect()),
            );
        }
        if let Some(h) = &self.hosted {
            m.insert("hosted".into(), hosted_json(h));
        }
        m.insert("ext".into(), Json::Obj(self.ext.clone()));
        Json::Obj(m)
    }

    /// Parse an `AssemblySource` (strict member set — unknown keys are
    /// `C-LOAD-3`-class errors the caller surfaces as diagnostics).
    pub fn from_json(j: &Json, path: &str) -> Result<AssemblySource, String> {
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(format!("{path}: an AssemblySource must be an object")),
        };
        const KNOWN: &[&str] = &[
            "dialect",
            "root_kind",
            "base",
            "document",
            "layers",
            "overrides",
            "imports",
            "hosted",
            "ext",
        ];
        for k in m.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(format!("{path}.{k}: unknown AssemblySource member"));
            }
        }
        let dialect = match m.get("dialect") {
            Some(Json::Str(s)) => s.clone(),
            Some(_) => return Err(format!("{path}.dialect must be a string")),
            None => SOURCE_DIALECT.to_string(),
        };
        let root_kind = match m.get("root_kind") {
            Some(Json::Str(s)) => RootKind::parse(s)
                .ok_or_else(|| format!("{path}.root_kind: unknown spelling `{s}`"))?,
            Some(_) => return Err(format!("{path}.root_kind must be a string")),
            None => RootKind::Native,
        };
        let base = match m.get("base") {
            Some(Json::Str(s)) => Some(s.clone()),
            Some(_) => return Err(format!("{path}.base must be a DefinitionRef string")),
            None => None,
        };
        let document = match m.get("document") {
            Some(Json::Obj(dm)) => {
                for k in dm.keys() {
                    if !matches!(k.as_str(), "root" | "nodes" | "edges") {
                        return Err(format!("{path}.document.{k}: unknown member"));
                    }
                }
                let root = dm
                    .get("root")
                    .and_then(Json::as_str)
                    .ok_or_else(|| format!("{path}.document.root is required"))?
                    .to_string();
                let nodes = match dm.get("nodes") {
                    Some(Json::Arr(items)) => items.clone(),
                    Some(_) => return Err(format!("{path}.document.nodes must be a list")),
                    None => Vec::new(),
                };
                let edges = match dm.get("edges") {
                    Some(Json::Arr(items)) => items.clone(),
                    Some(_) => return Err(format!("{path}.document.edges must be a list")),
                    None => Vec::new(),
                };
                Some(SourceDocument { root, nodes, edges })
            }
            Some(_) => return Err(format!("{path}.document must be an object")),
            None => None,
        };
        let mut layers = Vec::new();
        match m.get("layers") {
            Some(Json::Arr(items)) => {
                for (i, lj) in items.iter().enumerate() {
                    let lp = format!("{path}.layers[{i}]");
                    let lm = match lj {
                        Json::Obj(lm) => lm,
                        _ => return Err(format!("{lp}: a layer must be an object")),
                    };
                    for k in lm.keys() {
                        if !matches!(k.as_str(), "provenance" | "fragment") {
                            return Err(format!("{lp}.{k}: unknown member"));
                        }
                    }
                    let provenance = hh_assembly::grammar::layer_from_json(
                        lm.get("provenance")
                            .ok_or_else(|| format!("{lp}.provenance is required"))?,
                        &format!("{lp}.provenance"),
                    )?;
                    let fragment = lm
                        .get("fragment")
                        .cloned()
                        .ok_or_else(|| format!("{lp}.fragment is required"))?;
                    layers.push(SourceLayer {
                        provenance,
                        fragment,
                    });
                }
            }
            Some(_) => return Err(format!("{path}.layers must be a list")),
            None => {}
        }
        let (overrides, overrides_present) = match m.get("overrides") {
            Some(Json::Arr(items)) => {
                let mut out = Vec::new();
                for (i, o) in items.iter().enumerate() {
                    out.push(
                        o.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| format!("{path}.overrides[{i}] must be a string"))?,
                    );
                }
                (out, true)
            }
            Some(_) => return Err(format!("{path}.overrides must be a list of spellings")),
            None => (Vec::new(), false),
        };
        let mut imports = Vec::new();
        match m.get("imports") {
            Some(Json::Arr(items)) => {
                for (i, ij) in items.iter().enumerate() {
                    imports.push(import_from_json(ij, &format!("{path}.imports[{i}]"))?);
                }
            }
            Some(_) => return Err(format!("{path}.imports must be a list")),
            None => {}
        }
        let hosted = match m.get("hosted") {
            Some(hj) => Some(hosted_from_json(hj, &format!("{path}.hosted"))?),
            None => None,
        };
        let ext = match m.get("ext") {
            Some(Json::Obj(xm)) => xm.clone(),
            Some(_) => return Err(format!("{path}.ext must be an object")),
            None => BTreeMap::new(),
        };
        Ok(AssemblySource {
            dialect,
            root_kind,
            base,
            document,
            layers,
            overrides,
            overrides_present,
            imports,
            hosted,
            ext,
        })
    }

    /// The canonical bytes of the source.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }
}

pub(crate) fn import_json(i: &Import) -> Json {
    let mut pairs = vec![
        ("path_or_ref", Json::str(i.path_or_ref.clone())),
        ("as", Json::str(i.as_.as_str())),
    ];
    if let Some(c) = &i.content {
        pairs.push(("content", Json::str(c.clone())));
    }
    if let Some(n) = &i.name {
        pairs.push(("name", Json::str(n.clone())));
    }
    if let Some(f) = &i.frontmatter_schema {
        pairs.push(("frontmatter_schema", f.clone()));
    }
    Json::obj(pairs)
}

fn import_from_json(j: &Json, path: &str) -> Result<Import, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path}: an Import must be an object")),
    };
    for k in m.keys() {
        if !matches!(
            k.as_str(),
            "path_or_ref" | "as" | "content" | "name" | "frontmatter_schema"
        ) {
            return Err(format!("{path}.{k}: unknown Import member"));
        }
    }
    let path_or_ref = m
        .get("path_or_ref")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.path_or_ref is required"))?
        .to_string();
    let as_ = m
        .get("as")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.as is required"))
        .and_then(|s| {
            ImportKind::parse(s).ok_or_else(|| format!("{path}.as: unknown spelling `{s}`"))
        })?;
    let content = match m.get("content") {
        Some(Json::Str(s)) => Some(s.clone()),
        Some(_) => return Err(format!("{path}.content must be a string")),
        None => None,
    };
    let name = match m.get("name") {
        Some(Json::Str(s)) => Some(s.clone()),
        Some(_) => return Err(format!("{path}.name must be a string")),
        None => None,
    };
    let frontmatter_schema = m.get("frontmatter_schema").cloned();
    Ok(Import {
        path_or_ref,
        as_,
        content,
        name,
        frontmatter_schema,
    })
}

fn hosted_supply_json(s: &HostedSupply) -> Json {
    let mut pairs = Vec::new();
    if let Some(n) = &s.name {
        pairs.push(("name", Json::str(n.clone())));
    }
    if let Some(k) = &s.kind {
        pairs.push(("kind", Json::str(k.clone())));
    }
    pairs.push(("record", s.record.clone()));
    Json::obj(pairs)
}

fn hosted_supply_from_json(j: &Json, path: &str) -> Result<HostedSupply, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path}: a hosted supply entry must be an object")),
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "name" | "kind" | "record") {
            return Err(format!("{path}.{k}: unknown member"));
        }
    }
    Ok(HostedSupply {
        name: m.get("name").and_then(Json::as_str).map(str::to_string),
        kind: m.get("kind").and_then(Json::as_str).map(str::to_string),
        record: m
            .get("record")
            .cloned()
            .ok_or_else(|| format!("{path}.record is required"))?,
    })
}

fn hosted_json(h: &HostedSpec) -> Json {
    let model = h.model.as_ref().map(|mb| match mb {
        ModelBinding::SelfSelected => Json::str("self_selected"),
        ModelBinding::Bound(ms) => Json::obj([(
            "bound",
            Json::Arr(ms.iter().map(|s| Json::str(s.clone())).collect()),
        )]),
    });
    let mut exec = vec![("harness", Json::str(h.harness.clone()))];
    if let Some(mj) = model {
        exec.push(("model", mj));
    }
    exec.push((
        "auth",
        Json::Arr(h.auth.iter().map(|s| Json::str(s.clone())).collect()),
    ));
    let mut m = BTreeMap::new();
    m.insert("executor".into(), Json::obj(exec));
    let supplies = |v: &[HostedSupply]| Json::Arr(v.iter().map(hosted_supply_json).collect());
    if !h.tools.is_empty() {
        m.insert("tools".into(), supplies(&h.tools));
    }
    if !h.instructions.is_empty() {
        m.insert("instructions".into(), supplies(&h.instructions));
    }
    if !h.policies.is_empty() {
        m.insert("policies".into(), supplies(&h.policies));
    }
    if let Some(b) = &h.budget {
        m.insert("budget".into(), b.clone());
    }
    if let Some(p) = &h.permissions {
        m.insert("permissions".into(), p.clone());
    }
    if let Some(hm) = &h.hosting_mechanism {
        m.insert("hosting_mechanism".into(), Json::str(hm.clone()));
    }
    if let Some(dc) = &h.declared_capabilities {
        m.insert("declared_capabilities".into(), dc.clone());
    }
    if let Some(pv) = &h.participant_version {
        m.insert("participant_version".into(), Json::str(pv.clone()));
    }
    if let Some(p) = &h.params {
        m.insert("params".into(), p.clone());
    }
    if !h.observability.is_empty() {
        m.insert(
            "observability".into(),
            Json::Arr(
                h.observability
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        );
    }
    if !h.parameters.is_empty() {
        m.insert("parameters".into(), Json::Obj(h.parameters.clone()));
    }
    if !h.values.is_empty() {
        m.insert("values".into(), Json::Obj(h.values.clone()));
    }
    Json::Obj(m)
}

fn hosted_from_json(j: &Json, path: &str) -> Result<HostedSpec, String> {
    let m = match j {
        Json::Obj(m) => m,
        _ => return Err(format!("{path}: `hosted` must be an object")),
    };
    for k in m.keys() {
        if !matches!(
            k.as_str(),
            "executor"
                | "tools"
                | "instructions"
                | "policies"
                | "budget"
                | "permissions"
                | "hosting_mechanism"
                | "declared_capabilities"
                | "participant_version"
                | "params"
                | "observability"
                | "parameters"
                | "values"
        ) {
            return Err(format!("{path}.{k}: unknown `hosted` member"));
        }
    }
    let exec = m
        .get("executor")
        .and_then(|e| match e {
            Json::Obj(em) => Some(em),
            _ => None,
        })
        .ok_or_else(|| format!("{path}.executor is required"))?;
    for k in exec.keys() {
        if !matches!(k.as_str(), "harness" | "model" | "auth") {
            return Err(format!("{path}.executor.{k}: unknown member"));
        }
    }
    let harness = exec
        .get("harness")
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{path}.executor.harness is required"))?
        .to_string();
    let model = match exec.get("model") {
        Some(Json::Str(s)) if s == "self_selected" => Some(ModelBinding::SelfSelected),
        Some(Json::Obj(mm)) => match mm.get("bound") {
            Some(Json::Arr(items)) => Some(ModelBinding::Bound(
                items
                    .iter()
                    .map(|i| {
                        i.as_str().map(str::to_string).ok_or_else(|| {
                            format!("{path}.executor.model.bound members must be strings")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => return Err(format!("{path}.executor.model: `bound` must be a list")),
        },
        Some(_) => {
            return Err(format!(
                "{path}.executor.model must be `self_selected` or `{{bound: […]}}`"
            ))
        }
        None => None,
    };
    let auth = match exec.get("auth") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{path}.executor.auth members must be strings"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(format!("{path}.executor.auth must be a list")),
        None => Vec::new(),
    };
    let supplies = |key: &str| -> Result<Vec<HostedSupply>, String> {
        match m.get(key) {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, s)| hosted_supply_from_json(s, &format!("{path}.{key}[{i}]")))
                .collect(),
            Some(_) => Err(format!("{path}.{key} must be a list")),
            None => Ok(Vec::new()),
        }
    };
    let str_list = |key: &str| -> Result<Vec<String>, String> {
        match m.get(key) {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{path}.{key} members must be strings"))
                })
                .collect(),
            Some(_) => Err(format!("{path}.{key} must be a list")),
            None => Ok(Vec::new()),
        }
    };
    let json_map = |key: &str| -> Result<BTreeMap<String, Json>, String> {
        match m.get(key) {
            Some(Json::Obj(mm)) => Ok(mm.clone()),
            Some(_) => Err(format!("{path}.{key} must be an object")),
            None => Ok(BTreeMap::new()),
        }
    };
    Ok(HostedSpec {
        harness,
        model,
        auth,
        tools: supplies("tools")?,
        instructions: supplies("instructions")?,
        policies: supplies("policies")?,
        budget: m.get("budget").cloned(),
        permissions: m.get("permissions").cloned(),
        hosting_mechanism: m
            .get("hosting_mechanism")
            .and_then(Json::as_str)
            .map(str::to_string),
        declared_capabilities: m.get("declared_capabilities").cloned(),
        participant_version: m
            .get("participant_version")
            .and_then(Json::as_str)
            .map(str::to_string),
        params: m.get("params").cloned(),
        observability: str_list("observability")?,
        parameters: json_map("parameters")?,
        values: json_map("values")?,
    })
}
