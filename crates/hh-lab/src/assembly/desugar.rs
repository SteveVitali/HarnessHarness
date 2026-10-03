//! `desugar` (§6.1 §2.2; ADR-0147 D1) — the pure, total, order-defined rewrite
//! from `AssemblySource` to the ratified `hir/1` constructs:
//!
//! - **D1** `base` → a lowest-precedence `packaged-default` layer
//!   (`id = base.version_id`) carrying the base's authored assembly members
//!   (`layers[]`/`resolved`/the service's `ext` bookkeeping stripped — a base
//!   is a *layer*, never a shortcut around `resolve`).
//! - **D2** `overrides` → exactly one `experiment` layer
//!   (`id = H(idp ∥ "override-layer" ∥ canonical(overrides))`), materialised
//!   whenever the `overrides` member is present — even `[]` (OQ-076). The
//!   layer's *effect* applies to the composed assembly after `compose` runs:
//!   the grammar has no tombstone, so `~path` is a desugar-level deletion
//!   attributed to the experiment layer (`source_layer` on the resulting
//!   `C-CLASS-2` is the experiment id), never a new merge primitive.
//! - **D3** `imports` → `entities` entries on one synthesised `user` layer +
//!   document nodes for `context_item`/`procedure` imports (`Text{authority ≤
//!   external, origin = import}`; the pointer rule: content is data, never
//!   code).
//! - **D4** selectors/deny/`head` live only inside layers: a `document` node's
//!   `native.slots` may not carry selector bindings (`C-LOAD-1` at desugar).
//! - **D5** `root_kind = hosted` → synthesised `AgentProcess{hosted:
//!   OpaqueProcess}` + `Budget`/`Permission`/supply nodes; `slots` /
//!   `profile_binding` anywhere in the source are `C-CLASS-6`.

use std::collections::{BTreeMap, BTreeSet};

use hh_assembly::catalog::ClassCatalog;
use hh_assembly::compose::Layer;
use hh_assembly::diagnostics::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use hh_assembly::grammar::{Assembly, EntityBinding, LayerProvenance, LayerSourceKind};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::leaves::Text;
use hh_hir::records::{
    AgentProcessBody, AgentProcessRecord, BudgetRecord, CapabilityDeclarationRecord,
    ContextItemRecord, KindRecord, MemoryRecord, OpaqueProcess, PermissionRecord, ProcedureRecord,
    ProcedureStep, Supplies, Validity,
};
use hh_hir::refs::{ComponentVariantRef, Ref, RefVersion};
use hh_identity::idp::idp_id;
use hh_identity::names::ResolveMode;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope, ProvenanceRecord};
use hh_registry::records::RegistryRecord;
use hh_registry::store::{RegistryStore, ResolveInput, ResolveRequest};
use hh_wire::json::Json;

use crate::assembly::source::{
    AssemblySource, HostedSpec, Import, ImportKind, ModelBinding, RootKind, SOURCE_DIALECT,
};

/// The extension key the desugared assembly carries for `explain` and S-7
/// (the desugared layer stack — provenance **and** fragments — plus the
/// derivation key). `ext` never enters `semantic_id` (ExtEdit is
/// non-semantic, §3.1.7), so the record is attribution-free for identity
/// (ADR-0025 excludes `layers[]`; this member rides the same rule).
pub const LAB_EXT_KEY: &str = "hh.lab/assembly";

/// A parsed `overrides[]` entry (ADR-0025 grammar).
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideOp {
    /// `path=v` — set.
    Set { path: String, value: Json },
    /// `+path=v` — append to the list at `path`.
    Append { path: String, value: Json },
    /// `++path=v` — append-or-override (at assembly scope: an append that is
    /// permitted to reshape the bound value's shape).
    AppendOrOverride { path: String, value: Json },
    /// `~path` — delete.
    Delete { path: String },
    /// `path=v1,v2` — a choice (multi-bind on a slot; an array on a value).
    Choice { path: String, values: Vec<Json> },
    /// `class_id=variant_id` — the group override on `slots`.
    Bind {
        class_id: String,
        variant_id: String,
    },
}

/// The D2 materialisation — one `experiment` layer + the ops it applies.
#[derive(Debug, Clone)]
pub struct ExperimentLayer {
    /// The layer provenance (`id = H(idp ∥ "override-layer" ∥ canonical)`).
    pub provenance: LayerProvenance,
    /// The parsed ops (applied post-`compose`).
    pub ops: Vec<OverrideOp>,
    /// The delta fragment — the record of what the layer sets/deletes, for
    /// the derivation key and `explain` (`~` targets are listed under
    /// `ext["hh.lab/deleted"]`; this fragment never enters `compose` — its
    /// effect is the desugar transform).
    pub fragment: Assembly,
    /// The `~` targets (assembly paths).
    pub tombstones: Vec<String>,
}

/// `desugar`'s output — the document scaffold (assembly member unset) plus
/// the layer stack `compose` consumes and the experiment materialisation the
/// service applies after compose.
pub struct Desugared {
    /// The scaffold document (`assembly` unset — `compose` owns it).
    pub doc: HirDocument,
    /// The compose input: `[base?] + authored + [imports?]`.
    pub layers: Vec<Layer>,
    /// The D2 materialisation (present when `overrides` was authored).
    pub experiment: Option<ExperimentLayer>,
    /// Desugar-stage diagnostics (never fail-fast — the scaffold may be
    /// partial; downstream stages still run).
    pub diags: Vec<AssemblyDiagnostic>,
}

fn ddiag(
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
        stage: Stage::Desugar,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0147".to_string(),
    }
}

fn dwarn(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    let mut d = ddiag(code, path, subject, detail, remedy, kernel);
    d.severity = Severity::Warning;
    d
}

/// Resolve a `DefinitionRef` spelling to a registered sealed definition:
/// `version:<vid>`, a bare `<sha…>` version id, or `ns/name[@label]`.
pub fn base_definition(
    base: &str,
    registry: &RegistryStore,
    snapshot_id: Option<&str>,
) -> Result<hh_hir::document::SealedDefinition, String> {
    let input = if let Some(v) = base.strip_prefix("version:") {
        ResolveInput::Version(v.to_string())
    } else if base.contains('/') {
        let (nm, label) = match base.split_once('@') {
            Some((n, l)) => (n.to_string(), Some(l.to_string())),
            None => (base.to_string(), None),
        };
        let (ns, name) = nm
            .split_once('/')
            .ok_or_else(|| format!("base `{base}` is not `ns/name`"))?;
        ResolveInput::Selector {
            namespace: ns.to_string(),
            name: name.to_string(),
            label,
            snapshot_id: snapshot_id.map(str::to_string),
        }
    } else {
        ResolveInput::Version(base.to_string())
    };
    let resolved = registry
        .resolve(&input, ResolveMode::Audit, &ResolveRequest::default())
        .map_err(|e| format!("base `{base}` does not resolve: {e:?}"))?;
    match &resolved.record {
        RegistryRecord::SealedDefinition(s) => Ok(s.clone()),
        other => Err(format!(
            "base `{base}` resolved to kind {:?} — a `sealed_definition` is required",
            other.kind()
        )),
    }
}

/// D1 — the base's authored assembly members as a `packaged-default` fragment
/// (`layers`/`resolved`/`ext` stripped: the derived members are this
/// assembly's to compute, never the base's).
fn base_fragment(
    s: &hh_hir::document::SealedDefinition,
    kernel: &ProvenanceRecord,
) -> (Assembly, Vec<AssemblyDiagnostic>) {
    let mut diags = Vec::new();
    match &s.document.assembly {
        Some(j) => match Assembly::from_json(j, "/assembly", kernel, &mut diags) {
            Some(mut a) => {
                a.layers = None;
                a.resolved = None;
                a.ext = BTreeMap::new();
                (a, diags)
            }
            None => (Assembly::empty(), diags),
        },
        None => (Assembly::empty(), diags),
    }
}

/// The import-minted provenance — `origin = import` at definition scope with
/// a **pin attestation** over the imported bytes (`subject_hash` is the
/// content address the service pins — the importer's endorsement that makes
/// import content sealable; §8.1 #3's import/migration rule). The minted
/// authority is `unverified` (≤ external — R-TEXT; never widened); seal's
/// pin-basis endorsement confers `definition` authority.
fn import_prov(path_or_ref: &str, content: &[u8], seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted_attested(
        Origin::Import {
            source_system: path_or_ref.to_string(),
            mapping_version: "hh-assembly-source/1".to_string(),
        },
        PersistenceScope::Definition,
        seq,
        hh_provenance::Attestation {
            kind: hh_provenance::AttestationKind::Pin,
            subject_hash: hh_identity::idp::idp_id("hh.import", content),
            anchor: hh_provenance::AttestationAnchor::Signer("hh-lab/assembly-source".to_string()),
            verified_by: "kernel:hh-lab/assembly".to_string(),
            verified_at: seq,
        },
    )
}

/// Parse a `---`-delimited frontmatter header (`key: value` lines; the body
/// after the second `---`). Returns `(frontmatter map, body)`.
fn split_frontmatter(content: &str) -> (BTreeMap<String, Json>, String) {
    let mut fm = BTreeMap::new();
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (fm, content.to_string());
    }
    let rest = &trimmed[3..];
    match rest.find("\n---") {
        Some(end) => {
            let header = &rest[..end];
            let body = &rest[end + 4..];
            for line in header.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    fm.insert(k.trim().to_string(), Json::str(v.trim()));
                }
            }
            (fm, body.to_string())
        }
        None => (fm, content.to_string()),
    }
}

/// Enforce `frontmatter_schema` (`{require: [key…], types: {key: "string"|
/// "int"|"bool"}}`) over a parsed header; returns diagnostics.
fn check_frontmatter(
    fm: &BTreeMap<String, Json>,
    schema: &Json,
    subject: &str,
    kernel: &ProvenanceRecord,
) -> Vec<AssemblyDiagnostic> {
    let mut diags = Vec::new();
    if let Some(Json::Arr(reqs)) = schema.get("require") {
        for r in reqs {
            if let Some(k) = r.as_str() {
                if !fm.contains_key(k) {
                    diags.push(ddiag(
                        Code::LoadParse,
                        "/imports/frontmatter",
                        subject,
                        &format!("import `{subject}`'s frontmatter lacks required key `{k}`"),
                        "add the frontmatter key the schema requires",
                        kernel,
                    ));
                }
            }
        }
    }
    if let Some(Json::Obj(types)) = schema.get("types") {
        for (k, t) in types {
            if let (Some(v), Some(ts)) = (fm.get(k), t.as_str()) {
                let ok = match ts {
                    "int" => {
                        matches!(v, Json::Int(_))
                            || v.as_str().is_some_and(|s| s.parse::<i64>().is_ok())
                    }
                    "bool" => {
                        matches!(v, Json::Bool(_))
                            || matches!(v.as_str(), Some("true") | Some("false"))
                    }
                    _ => true,
                };
                if !ok {
                    diags.push(ddiag(
                        Code::LoadParse,
                        "/imports/frontmatter",
                        subject,
                        &format!("import `{subject}`'s frontmatter key `{k}` is not a `{ts}`"),
                        "fix the frontmatter value's type",
                        kernel,
                    ));
                }
            }
        }
    }
    diags
}

/// D3 — one import → `(entities entry, document nodes)`. `content` must be
/// inlined (`Option::None` is a `C-LOAD-1` — the desugar is pure, no IO).
fn desugar_import(
    imp: &Import,
    seq: u64,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) -> (String, EntityBinding, Vec<Node>) {
    let name = imp
        .name
        .clone()
        .unwrap_or_else(|| format!("import:{}", imp.path_or_ref.replace(['/', '\\'], "_")));
    let content = match &imp.content {
        Some(c) => c.clone(),
        None => {
            diags.push(ddiag(
                Code::LoadParse,
                "/imports",
                &name,
                &format!(
                    "import `{name}` carries no `content` — the service performs no IO; inline the imported bytes"
                ),
                "inline the file's text under `content` (the caller owns path resolution)",
                kernel,
            ));
            String::new()
        }
    };
    let prov = import_prov(&imp.path_or_ref, content.as_bytes(), seq);
    let (fm, body) = split_frontmatter(&content);
    if let Some(schema) = &imp.frontmatter_schema {
        diags.extend(check_frontmatter(&fm, schema, &name, kernel));
    }
    let fm_json = Json::Obj(fm.into_iter().collect());
    let text = Text::new(body.clone(), "import", prov.clone());
    let entity_kind = match imp.as_ {
        ImportKind::ContextItem | ImportKind::Text => "context_item",
        ImportKind::Procedure => "procedure",
    };
    // The entities entry — `Inline{kind, record}` (data; never a sealable
    // node by itself — §3.3.2).
    let binding = EntityBinding::Inline {
        kind: entity_kind.to_string(),
        record: Json::obj([
            ("content", text.to_json()),
            ("frontmatter", fm_json.clone()),
            ("source", Json::str(imp.path_or_ref.clone())),
        ]),
    };
    // The document nodes — a `ContextItem` (or `Procedure`) wrapping the
    // content `Memory` so `supplies.context`/`procedures` refs have a graph
    // target.
    let mem_id = format!("{name}.content");
    let nodes = match imp.as_ {
        ImportKind::ContextItem | ImportKind::Text => {
            let mem = {
                let mut n = Node::new(
                    EntityKind::Memory,
                    KindRecord::Memory(MemoryRecord {
                        content: text.clone(),
                        validity: Validity::open_from(0),
                        authority: AuthorityClass::Unverified,
                        confidence: Json::Null,
                        source_trajectories: vec![],
                        scope: PersistenceScope::Definition,
                    }),
                    prov.clone(),
                );
                n.version.semantic_id = Some(mem_id.clone());
                n
            };
            let mut ci = Node::new(
                EntityKind::ContextItem,
                KindRecord::ContextItem(ContextItemRecord {
                    payload: Ref::selected(mem_id.clone(), "latest"),
                    authority: AuthorityClass::Unverified,
                    validity: Validity::open_from(0),
                    placement_policy: None,
                    priority: 0,
                    delivery_id: format!("{name}.delivery"),
                    activation_observable: Json::Null,
                }),
                prov.clone(),
            );
            ci.version.semantic_id = Some(name.clone());
            vec![mem, ci]
        }
        ImportKind::Procedure => {
            let mut p = Node::new(
                EntityKind::Procedure,
                KindRecord::Procedure(ProcedureRecord {
                    preconditions: Json::Null,
                    steps: vec![ProcedureStep::Instruction(text.clone())],
                    expected_evidence: Json::Null,
                    allowed_capabilities: vec![],
                    failure_handlers: Json::Null,
                }),
                prov.clone(),
            );
            p.version.semantic_id = Some(name.clone());
            vec![p]
        }
    };
    (name, binding, nodes)
}

/// Parse one override spelling (ADR-0025). `catalog` resolves
/// `class_id=variant_id` group overrides.
fn parse_override(raw: &str, catalog: &dyn ClassCatalog) -> Result<OverrideOp, String> {
    if let Some(p) = raw.strip_prefix("~~") {
        return Err(format!("`{raw}` — `~~` is not a production (use `~{p}`)"));
    }
    if let Some(p) = raw.strip_prefix('~') {
        if p.is_empty() {
            return Err("`~` without a path".into());
        }
        return Ok(OverrideOp::Delete {
            path: p.to_string(),
        });
    }
    if let Some(rest) = raw.strip_prefix("++") {
        let (p, v) = rest
            .split_once('=')
            .ok_or_else(|| format!("`{raw}` — `++path=v` needs `=`"))?;
        return Ok(OverrideOp::AppendOrOverride {
            path: p.to_string(),
            value: parse_value(v),
        });
    }
    if let Some(rest) = raw.strip_prefix('+') {
        let (p, v) = rest
            .split_once('=')
            .ok_or_else(|| format!("`{raw}` — `+path=v` needs `=`"))?;
        return Ok(OverrideOp::Append {
            path: p.to_string(),
            value: parse_value(v),
        });
    }
    let (lhs, rhs) = raw
        .split_once('=')
        .ok_or_else(|| format!("`{raw}` — an override needs `=`"))?;
    // `class_id=variant_id` — the group override on slots (the lhs names a
    // catalog class id or slot key and carries no `.`/`/` path separator).
    let is_class = !lhs.contains('.')
        && !lhs.contains('/')
        && (catalog.class(lhs).is_some()
            || catalog
                .class_ids()
                .iter()
                .any(|c| catalog.class(c).map(|r| r.slot_key == lhs).unwrap_or(false)));
    if is_class {
        return Ok(OverrideOp::Bind {
            class_id: lhs.to_string(),
            variant_id: rhs.to_string(),
        });
    }
    if rhs.contains(',') {
        return Ok(OverrideOp::Choice {
            path: lhs.to_string(),
            values: rhs.split(',').map(parse_value).collect(),
        });
    }
    Ok(OverrideOp::Set {
        path: lhs.to_string(),
        value: parse_value(rhs),
    })
}

/// `v` → `Json` (canonical scalars/objects parse; a bare word is a string).
fn parse_value(v: &str) -> Json {
    match v {
        "true" => Json::Bool(true),
        "false" => Json::Bool(false),
        "null" => Json::Null,
        _ => {
            if let Ok(i) = v.parse::<i64>() {
                Json::Int(i)
            } else if v.starts_with('{') || v.starts_with('[') || v.starts_with('"') {
                hh_wire::canonical::parse_canonical(v.as_bytes()).unwrap_or(Json::Str(v.into()))
            } else {
                Json::Str(v.into())
            }
        }
    }
}

/// The `experiment` layer id — `H(idp ∥ "override-layer" ∥
/// canonical(overrides))` (ADR-0147 D2; the established `hh-embed`
/// spelling).
pub fn overrides_layer_id(overrides: &[String]) -> String {
    let mut sorted: Vec<String> = overrides.to_vec();
    sorted.sort();
    let canon =
        Json::Arr(sorted.iter().map(|s| Json::str(s.clone())).collect()).to_canonical_string();
    idp_id("override-layer", canon.as_bytes())
}

/// Materialise the D2 layer — parse every spelling (all errors collected).
fn materialise_experiment(
    source: &AssemblySource,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) -> Option<ExperimentLayer> {
    if !source.overrides_present {
        return None;
    }
    let id = overrides_layer_id(&source.overrides);
    let prov = LayerProvenance {
        source_kind: LayerSourceKind::Experiment,
        id: id.clone(),
        version: "1".to_string(),
        precedence: i64::MAX, // the experiment layer is always topmost
    };
    let mut ops = Vec::new();
    let mut tombstones = Vec::new();
    let mut fragment = Assembly::empty();
    let mut deleted = Vec::new();
    for raw in &source.overrides {
        match parse_override(raw, catalog) {
            Ok(op) => {
                if let OverrideOp::Delete { path } = &op {
                    tombstones.push(path.clone());
                    deleted.push(Json::str(path.clone()));
                }
                ops.push(op);
            }
            Err(e) => diags.push(ddiag(
                Code::LoadParse,
                "/overrides",
                raw,
                &format!("override `{raw}` fails the ADR-0025 grammar: {e}"),
                "fix the override spelling",
                kernel,
            )),
        }
    }
    if !deleted.is_empty() {
        fragment
            .ext
            .insert("hh.lab/deleted".into(), Json::Arr(deleted));
    }
    Some(ExperimentLayer {
        provenance: prov,
        ops,
        fragment,
        tombstones,
    })
}

/// Apply the materialised overrides to the composed assembly (the D2
/// transform — post-`compose`, attributed to the experiment layer). Returns
/// the touched paths (for `plan.layer_map`/`explain`).
pub fn apply_experiment(
    a: &mut Assembly,
    exp: &ExperimentLayer,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) -> Vec<String> {
    let mut touched = Vec::new();
    for op in &exp.ops {
        match op {
            OverrideOp::Bind {
                class_id,
                variant_id,
            } => {
                // Group override: the lhs may be a class id or a slot key.
                a.slots.insert(
                    class_id.clone(),
                    hh_hir::records::SlotBindings::One(hh_hir::records::SlotBinding::of(
                        ComponentVariantRef::selected(class_id, variant_id, "latest"),
                    )),
                );
                touched.push(format!("slots.{class_id}"));
            }
            OverrideOp::Set { path, value }
            | OverrideOp::Append { path, value }
            | OverrideOp::AppendOrOverride { path, value } => {
                let append = matches!(
                    op,
                    OverrideOp::Append { .. } | OverrideOp::AppendOrOverride { .. }
                );
                apply_path(a, path, value, append, &exp.provenance.id, kernel, diags);
                touched.push(path.clone());
            }
            OverrideOp::Choice { path, values } => {
                apply_path(
                    a,
                    path,
                    &Json::Arr(values.clone()),
                    false,
                    &exp.provenance.id,
                    kernel,
                    diags,
                );
                touched.push(path.clone());
            }
            OverrideOp::Delete { path } => {
                apply_delete(a, path, &exp.provenance.id, kernel, diags);
                touched.push(path.clone());
            }
        }
    }
    touched
}

/// Set/append `value` at an assembly path (`slots.<c>[.enabled|.params.<k>]`,
/// `values.<p>[.sub]`, `parameters.<p>`, `entities.<id>`,
/// `profile_binding`, `ext.<k>`).
fn apply_path(
    a: &mut Assembly,
    path: &str,
    value: &Json,
    append: bool,
    layer: &str,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) {
    let mut parts = path.splitn(2, '.');
    let head = parts.next().unwrap_or("");
    let tail = parts.next();
    match head {
        "slots" => {
            let (slot, sub) = match tail {
                Some(t) => {
                    let mut it = t.splitn(2, '.');
                    (it.next().unwrap_or(""), it.next())
                }
                None => ("", None),
            };
            if slot.is_empty() {
                diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    path,
                    "a `slots.<class>` override needs a class name",
                    "spell `slots.<class>` or `class_id=variant_id`",
                    kernel,
                ));
                return;
            }
            match sub {
                None => {
                    // `slots.<c>=<variant>` — bind (one or many by choice/array).
                    let bindings = match value {
                        Json::Arr(items) => items
                            .iter()
                            .filter_map(Json::as_str)
                            .map(|v| {
                                hh_hir::records::SlotBinding::of(ComponentVariantRef::selected(
                                    slot, v, "latest",
                                ))
                            })
                            .collect::<Vec<_>>(),
                        Json::Str(v) => vec![hh_hir::records::SlotBinding::of(
                            ComponentVariantRef::selected(slot, v, "latest"),
                        )],
                        _ => vec![],
                    };
                    if bindings.is_empty() {
                        diags.push(ddiag(
                            Code::LoadParse,
                            path,
                            slot,
                            "a `slots.<c>` override value must be a variant name or list",
                            "spell `slots.<c>=<variant>` or `slots.<c>=<v1>,<v2>`",
                            kernel,
                        ));
                        return;
                    }
                    if append {
                        match a.slots.get_mut(slot) {
                            Some(hh_hir::records::SlotBindings::Many(v)) => v.extend(bindings),
                            Some(hh_hir::records::SlotBindings::One(b)) => {
                                let mut v = vec![b.clone()];
                                v.extend(bindings);
                                a.slots.insert(
                                    slot.to_string(),
                                    hh_hir::records::SlotBindings::Many(v),
                                );
                            }
                            None => {
                                a.slots.insert(
                                    slot.to_string(),
                                    hh_hir::records::SlotBindings::Many(bindings),
                                );
                            }
                        }
                    } else {
                        a.slots.insert(
                            slot.to_string(),
                            if bindings.len() == 1 {
                                hh_hir::records::SlotBindings::One(
                                    bindings.into_iter().next().unwrap(),
                                )
                            } else {
                                hh_hir::records::SlotBindings::Many(bindings)
                            },
                        );
                    }
                }
                Some("enabled") => {
                    let flag = matches!(value, Json::Bool(true));
                    match a.slots.get_mut(slot) {
                        Some(hh_hir::records::SlotBindings::One(b)) => b.enabled = flag,
                        Some(hh_hir::records::SlotBindings::Many(v)) => {
                            for b in v {
                                b.enabled = flag;
                            }
                        }
                        None => diags.push(ddiag(
                            Code::LoadParse,
                            path,
                            slot,
                            "`enabled` toggles an existing binding — none is bound",
                            "bind the slot before toggling `enabled`",
                            kernel,
                        )),
                    }
                }
                Some(rest) if rest.starts_with("params.") => {
                    let key = rest.trim_start_matches("params.");
                    match a.slots.get_mut(slot) {
                        Some(hh_hir::records::SlotBindings::One(b)) => {
                            b.params.insert(key.to_string(), value.clone());
                        }
                        Some(hh_hir::records::SlotBindings::Many(v)) => {
                            if let Some(b) = v.first_mut() {
                                b.params.insert(key.to_string(), value.clone());
                            }
                        }
                        None => diags.push(ddiag(
                            Code::LoadParse,
                            path,
                            slot,
                            "`params` target an existing binding — none is bound",
                            "bind the slot before setting a param",
                            kernel,
                        )),
                    }
                }
                Some(other) => diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    slot,
                    &format!("unknown slot override member `{other}`"),
                    "use `slots.<c>[.enabled|.params.<k>]`",
                    kernel,
                )),
            }
        }
        "values" => {
            let p = tail.unwrap_or("");
            if p.is_empty() {
                diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    path,
                    "a `values.<p>` override needs a parameter id",
                    "spell `values.<p>=<v>`",
                    kernel,
                ));
                return;
            }
            // S-8 guard — overrides may not add slots, and only named
            // parameter space members are values.
            let mut it = p.splitn(2, '.');
            let key = it.next().unwrap_or("");
            if let Some(sub) = it.next() {
                let mut target = a.values.get(key).cloned().unwrap_or(Json::Null);
                set_json_pointer(&mut target, sub, value.clone());
                a.values.insert(key.to_string(), target);
            } else {
                a.values.insert(key.to_string(), value.clone());
            }
        }
        "parameters" => {
            let p = tail.unwrap_or("");
            match hh_assembly::grammar::parameter_spec_from_json(
                value,
                &format!("/assembly/parameters/{p}"),
            ) {
                Ok(ps) => {
                    a.parameters.insert(p.to_string(), ps);
                }
                Err(e) => diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    p,
                    &format!("parameter override is not a ParameterSpec: {e}"),
                    "author the parameter as a ParameterSpec record",
                    kernel,
                )),
            }
        }
        "entities" => {
            let id = tail.unwrap_or("");
            match hh_assembly::grammar::entity_binding_from_json(value, &format!("/assembly/entities/{id}")) {
                Ok(eb) => {
                    a.entities.insert(id.to_string(), eb);
                }
                Err(e) => diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    id,
                    &format!("entity override is not a `Ref | Entity`: {e}"),
                    "author the entity as `Ref | {kind, record}`",
                    kernel,
                )),
            }
        }
        "profile_binding" => {
            match value.as_str() {
                Some("unbound") => a.profile_binding = hh_assembly::grammar::ProfileBinding::Unbound,
                _ => {
                    a.profile_binding = hh_assembly::grammar::ProfileBinding::Constraint(value.clone())
                }
            }
        }
        "constraints" => {
            match hh_assembly::grammar::constraint_from_json(value, "/assembly/constraints[-]") {
                Ok(mut c) => {
                    c.source = Some(LayerProvenance {
                        source_kind: LayerSourceKind::Experiment,
                        id: layer.to_string(),
                        version: "1".into(),
                        precedence: i64::MAX,
                    });
                    a.constraints.push(c);
                }
                Err(e) => diags.push(ddiag(
                    Code::LoadParse,
                    path,
                    "constraints",
                    &format!("constraint override fails the grammar: {e}"),
                    "author the constraint as `{kind, subject}`",
                    kernel,
                )),
            }
        }
        "ext" => {
            if let Some(k) = tail {
                a.ext.insert(k.to_string(), value.clone());
            }
        }
        _ => diags.push(ddiag(
            Code::LoadParse,
            path,
            path,
            &format!(
                "override path `{path}` names no assembly member (slots|values|parameters|entities|profile_binding|constraints|ext)"
            ),
            "name an assembly member",
            kernel,
        )),
    }
}

/// `~path` — the desugar-level deletion (the experiment layer's tombstone).
fn apply_delete(
    a: &mut Assembly,
    path: &str,
    layer: &str,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) {
    let mut parts = path.splitn(2, '.');
    let head = parts.next().unwrap_or("");
    let tail = parts.next();
    let removed = match head {
        "slots" => tail.map(|t| a.slots.remove(t).is_some()).unwrap_or(false),
        "values" => tail.map(|t| a.values.remove(t).is_some()).unwrap_or(false),
        "parameters" => tail
            .map(|t| a.parameters.remove(t).is_some())
            .unwrap_or(false),
        "entities" => tail
            .map(|t| a.entities.remove(t).is_some())
            .unwrap_or(false),
        "profile_binding" => {
            let had = !matches!(
                a.profile_binding,
                hh_assembly::grammar::ProfileBinding::Unbound
            );
            a.profile_binding = hh_assembly::grammar::ProfileBinding::Unbound;
            had
        }
        "ext" => tail.map(|t| a.ext.remove(t).is_some()).unwrap_or(false),
        _ => false,
    };
    if !removed {
        diags.push(dwarn(
            Code::RefDenyListNoop,
            path,
            path,
            &format!("`~{path}` removed nothing (the member was absent; layer `{layer}`)"),
            "check the path — a delete of an absent member is a no-op",
            kernel,
        ));
    }
}

/// A minimal `a.b.c` JSON-pointer set (objects only; arrays by index).
fn set_json_pointer(target: &mut Json, pointer: &str, value: Json) {
    let mut parts = pointer.split('.');
    if let Some(first) = parts.next() {
        let rest = parts.collect::<Vec<_>>().join(".");
        if let Json::Obj(m) = target {
            let entry = m.entry(first.to_string()).or_insert(Json::Null);
            if rest.is_empty() {
                *entry = value;
            } else {
                set_json_pointer(entry, &rest, value);
            }
        }
    }
}

/// Synthesise the hosted scaffold (D5): the `AgentProcess{hosted}` root plus
/// `Budget`/`Permission`/`HarnessRule` (accounting) nodes and the `supplies`
/// entities. `document` members add nodes/edges on top.
///
/// `hosted.parameters`/`hosted.values` return as the scaffold layer's members
/// (`params → ParameterSpec`, `affects ⊆ {configuration, product}`, `sweepable`
/// admissibility over the declared capability vector — R-2.10.1²/ADR-0013 (4));
/// the stage-3 check (`C-PARAM-6`) refuses the rest.
fn hosted_scaffold(
    h: &HostedSpec,
    registrar: &ProvenanceRecord,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
    seq: &mut u64,
) -> (
    HirDocument,
    BTreeMap<String, hh_assembly::grammar::ParameterSpec>,
    BTreeMap<String, Json>,
) {
    let next = |s: &mut u64| {
        *s += 1;
        *s
    };
    let mk_prov = |s: &mut u64| {
        ProvenanceRecord::minted(
            registrar.origin.clone(),
            PersistenceScope::Definition,
            next(s),
        )
    };
    let mut nodes = Vec::new();
    let root_sid = "hosted:root".to_string();

    // The accounting HarnessRule (budget.accounting refs a node — a
    // declared default rule keeps the member pinned inside the document).
    let rule_id = format!("{root_sid}/accounting");
    let mut rule = Node::new(
        EntityKind::HarnessRule,
        KindRecord::HarnessRule(hh_hir::records::HarnessRuleRecord {
            rule_id: rule_id.clone(),
            trigger: Json::Null,
            action: hh_hir::records::RuleAction::RequestApproval(Json::Null),
            scope: Json::Null,
            conditioned_on: None,
            assumption_debt: None,
        }),
        mk_prov(seq),
    );
    rule.version.semantic_id = Some(rule_id.clone());
    nodes.push(rule);

    // Budget — authored body or the empty-dimension default.
    let budget_id = format!("{root_sid}/budget");
    let budget_rec = match &h.budget {
        Some(j) => {
            match hh_hir::wire::semantic_record_from_json(EntityKind::Budget, j, "/hosted/budget") {
                Ok(r) => r,
                Err(e) => {
                    diags.push(ddiag(
                        Code::LoadParse,
                        "/hosted/budget",
                        "budget",
                        &format!("`hosted.budget` is not a `Budget` record: {e:?}"),
                        "author the Budget record body",
                        kernel,
                    ));
                    KindRecord::Budget(BudgetRecord {
                        dimensions: BTreeMap::new(),
                        scope: "*".into(),
                        parent: None,
                        accounting: Ref::selected(&rule_id, "latest"),
                    })
                }
            }
        }
        None => KindRecord::Budget(BudgetRecord {
            dimensions: BTreeMap::new(),
            scope: "*".into(),
            parent: None,
            accounting: Ref::selected(&rule_id, "latest"),
        }),
    };
    let mut budget = Node::new(EntityKind::Budget, budget_rec, mk_prov(seq));
    budget.version.semantic_id = Some(budget_id.clone());
    nodes.push(budget);

    // Permission — authored body or the empty-grants default.
    let perm_id = format!("{root_sid}/permissions");
    let perm_rec = match &h.permissions {
        Some(j) => match hh_hir::wire::semantic_record_from_json(
            EntityKind::Permission,
            j,
            "/hosted/permissions",
        ) {
            Ok(r) => r,
            Err(e) => {
                diags.push(ddiag(
                    Code::LoadParse,
                    "/hosted/permissions",
                    "permissions",
                    &format!("`hosted.permissions` is not a `Permission` record: {e:?}"),
                    "author the Permission record body",
                    kernel,
                ));
                KindRecord::Permission(PermissionRecord {
                    holder: Ref::selected(&root_sid, "latest"),
                    grants: vec![],
                    issuer: hh_hir::records::Issuer {
                        authority: AuthorityClass::Kernel,
                        reference: "hh-lab/assembly".into(),
                    },
                    validity: Validity::open_from(0),
                    revocation: None,
                })
            }
        },
        None => KindRecord::Permission(PermissionRecord {
            holder: Ref::selected(&root_sid, "latest"),
            grants: vec![],
            issuer: hh_hir::records::Issuer {
                authority: AuthorityClass::Kernel,
                reference: "hh-lab/assembly".into(),
            },
            validity: Validity::open_from(0),
            revocation: None,
        }),
    };
    let mut perm = Node::new(EntityKind::Permission, perm_rec, mk_prov(seq));
    perm.version.semantic_id = Some(perm_id.clone());
    nodes.push(perm);

    // Supplies: tools → ToolCapability nodes; instructions → ContextItem
    // (+Memory) nodes; policies → HarnessRule/Permission nodes.
    let mut supplies = Supplies::default();
    for (i, t) in h.tools.iter().enumerate() {
        let sid = t
            .name
            .clone()
            .unwrap_or_else(|| format!("{root_sid}/tool/{i}"));
        match hh_hir::wire::semantic_record_from_json(
            EntityKind::ToolCapability,
            &t.record,
            &format!("/hosted/tools/{i}"),
        ) {
            Ok(rec) => {
                let mut n = Node::new(EntityKind::ToolCapability, rec, mk_prov(seq));
                n.version.semantic_id = Some(sid.clone());
                nodes.push(n);
                supplies.tools.push(Ref::selected(sid, "latest"));
            }
            Err(e) => diags.push(ddiag(
                Code::LoadParse,
                &format!("/hosted/tools/{i}"),
                &sid,
                &format!("tool record is not a `ToolCapability`: {e:?}"),
                "author the ToolCapability record body",
                kernel,
            )),
        }
    }
    for (i, ins) in h.instructions.iter().enumerate() {
        let sid = ins
            .name
            .clone()
            .unwrap_or_else(|| format!("{root_sid}/context/{i}"));
        // `{content: "…"}` shorthand → a Memory payload + ContextItem
        // (authority ≤ external — the meta-harness instruction text is
        // imported content, never definition-authored).
        let content_text = ins
            .record
            .get("content")
            .and_then(Json::as_str)
            .map(str::to_string);
        let prov = mk_prov(seq);
        if let Some(body) = content_text {
            let mem_id = format!("{sid}/content");
            let mut mem = Node::new(
                EntityKind::Memory,
                KindRecord::Memory(MemoryRecord {
                    content: Text::new(body, "import", prov.clone()),
                    validity: Validity::open_from(0),
                    authority: AuthorityClass::Unverified,
                    confidence: Json::Null,
                    source_trajectories: vec![],
                    scope: PersistenceScope::Definition,
                }),
                prov.clone(),
            );
            mem.version.semantic_id = Some(mem_id.clone());
            nodes.push(mem);
            let mut ci = Node::new(
                EntityKind::ContextItem,
                KindRecord::ContextItem(ContextItemRecord {
                    payload: Ref::selected(mem_id, "latest"),
                    authority: AuthorityClass::Unverified,
                    validity: Validity::open_from(0),
                    placement_policy: None,
                    priority: 0,
                    delivery_id: format!("{sid}.delivery"),
                    activation_observable: Json::Null,
                }),
                prov,
            );
            ci.version.semantic_id = Some(sid.clone());
            nodes.push(ci);
            supplies.context.push(Ref::selected(sid, "latest"));
        } else {
            match hh_hir::wire::semantic_record_from_json(
                EntityKind::ContextItem,
                &ins.record,
                &format!("/hosted/instructions/{i}"),
            ) {
                Ok(rec) => {
                    let mut n = Node::new(EntityKind::ContextItem, rec, prov);
                    n.version.semantic_id = Some(sid.clone());
                    nodes.push(n);
                    supplies.context.push(Ref::selected(sid, "latest"));
                }
                Err(e) => diags.push(ddiag(
                    Code::LoadParse,
                    &format!("/hosted/instructions/{i}"),
                    &sid,
                    &format!("instruction record is not a `ContextItem`: {e:?}"),
                    "author `{content}` or a ContextItem record body",
                    kernel,
                )),
            }
        }
    }
    for (i, p) in h.policies.iter().enumerate() {
        let sid = p
            .name
            .clone()
            .unwrap_or_else(|| format!("{root_sid}/policy/{i}"));
        let kind = match p.kind.as_deref() {
            Some("HarnessRule") | Some("harness_rule") | None => EntityKind::HarnessRule,
            Some("Permission") | Some("permission") => EntityKind::Permission,
            Some(other) => {
                diags.push(ddiag(
                    Code::LoadParse,
                    &format!("/hosted/policies/{i}"),
                    &sid,
                    &format!("policy kind `{other}` is not HarnessRule|Permission"),
                    "policies are HarnessRule or Permission records",
                    kernel,
                ));
                continue;
            }
        };
        match hh_hir::wire::semantic_record_from_json(
            kind,
            &p.record,
            &format!("/hosted/policies/{i}"),
        ) {
            Ok(rec) => {
                let mut n = Node::new(kind, rec, mk_prov(seq));
                n.version.semantic_id = Some(sid);
                nodes.push(n);
            }
            Err(e) => diags.push(ddiag(
                Code::LoadParse,
                &format!("/hosted/policies/{i}"),
                &sid,
                &format!("policy record fails its kind schema: {e:?}"),
                "author the record body for the declared kind",
                kernel,
            )),
        }
    }

    // executor.model — `self_selected` (the default) records no binding;
    // `bound` carries the participant-side model coordinates in `params`.
    let mut params = h.params.clone();
    let auth_json = Json::Arr(
        h.auth
            .iter()
            .map(|s| Json::str(format!("$secret:{}", s.trim_start_matches("$secret:"))))
            .collect(),
    );
    let model_json = match &h.model {
        Some(ModelBinding::SelfSelected) | None => Json::str("self_selected"),
        Some(ModelBinding::Bound(ms)) => Json::obj([(
            "bound",
            Json::Arr(ms.iter().map(|s| Json::str(s.clone())).collect()),
        )]),
    };
    let mut p = params.take().unwrap_or(Json::obj([]));
    if let Json::Obj(pm) = &mut p {
        pm.insert("model_binding".into(), model_json);
        pm.insert("auth".into(), auth_json);
    }
    params = Some(p);

    let mechanism = h
        .hosting_mechanism
        .as_deref()
        .map(hh_ontology::participant::HostingMechanism::parse)
        .unwrap_or(Some(hh_ontology::participant::HostingMechanism::SessionAbi));
    let mechanism = match mechanism {
        Some(m) => m,
        None => {
            diags.push(ddiag(
                Code::LoadParse,
                "/hosted/hosting_mechanism",
                "hosting_mechanism",
                "unknown hosting mechanism spelling",
                "use session_abi | model_boundary_intercept | container_installed",
                kernel,
            ));
            hh_ontology::participant::HostingMechanism::SessionAbi
        }
    };
    if matches!(mechanism, hh_ontology::participant::HostingMechanism::None) {
        diags.push(ddiag(
            Code::LoadParse,
            "/hosted/hosting_mechanism",
            "hosting_mechanism",
            "`none` is a native descriptor — a hosted body must name a mechanism (CF-351)",
            "declare the hosting mechanism",
            kernel,
        ));
    }

    let declared_caps = match &h.declared_capabilities {
        Some(j) => decode_capability_declaration(j, kernel, diags),
        None => CapabilityDeclarationRecord::all_unknown(),
    };
    let observability: BTreeSet<hh_ontology::participant::Observability> = h
        .observability
        .iter()
        .filter_map(|s| hh_ontology::participant::Observability::parse(s))
        .collect();

    let mut root = Node::new(
        EntityKind::AgentProcess,
        KindRecord::AgentProcess(AgentProcessRecord {
            body: AgentProcessBody::Hosted(OpaqueProcess {
                participant_ref: h.harness.clone(),
                declared_capabilities: declared_caps,
                observability_levels: observability,
                participant_version: h
                    .participant_version
                    .clone()
                    .unwrap_or_else(|| idp_id("participant-version", h.harness.as_bytes())),
                version_identity: None,
                hosting_mechanism: mechanism,
                supplies,
                budget: Ref::selected(budget_id, "latest"),
                permissions: Ref::selected(perm_id, "latest"),
                params,
            }),
        }),
        mk_prov(seq),
    );
    root.version.semantic_id = Some(root_sid.clone());

    nodes.push(root);
    let mut doc = HirDocument::new(Ref::selected(root_sid, "latest"));
    doc.nodes = nodes;

    // `hosted.parameters`/`hosted.values` — the declared space and bound values
    // ride a scaffold layer below the authored layers (R-2.10.1²); members fail
    // the `ParameterSpec` schema as `C-LOAD-2`, the hosted admissibility as
    // `C-PARAM-6` at stage 3.
    let mut params_space = BTreeMap::new();
    for (p, j) in &h.parameters {
        match hh_assembly::grammar::parameter_spec_from_json(j, &format!("/hosted/parameters/{p}"))
        {
            Ok(ps) => {
                params_space.insert(p.clone(), ps);
            }
            Err(e) => diags.push(ddiag(
                Code::LoadParse,
                &format!("/hosted/parameters/{p}"),
                p,
                &format!("hosted parameter `{p}` is not a `ParameterSpec` record: {e}"),
                "author the parameter as `ParameterSpec{type, domain?, default?, required, sweepable, affects[], budget_relevant}`",
                kernel,
            )),
        }
    }
    (doc, params_space, h.values.clone())
}

/// Decode the `declared_capabilities` tri-state map.
fn decode_capability_declaration(
    j: &Json,
    kernel: &ProvenanceRecord,
    diags: &mut Vec<AssemblyDiagnostic>,
) -> CapabilityDeclarationRecord {
    let mut d = CapabilityDeclarationRecord::all_unknown();
    let state = |v: &Json| match v.as_str() {
        Some("supported") => Some(hh_hir::records::CapabilityState::Supported),
        Some("unsupported") => Some(hh_hir::records::CapabilityState::Unsupported),
        Some("unknown") => Some(hh_hir::records::CapabilityState::Unknown),
        _ => None,
    };
    if let Json::Obj(m) = j {
        for (k, v) in m {
            let Some(s) = state(v) else {
                diags.push(ddiag(
                    Code::LoadParse,
                    "/hosted/declared_capabilities",
                    k,
                    &format!("capability `{k}` is not supported|unsupported|unknown"),
                    "use the tri-state spellings",
                    kernel,
                ));
                continue;
            };
            match k.as_str() {
                "streaming" => d.streaming = s,
                "interrupt" => d.interrupt = s,
                "steer" => d.steer = s,
                "live_queue" => d.live_queue = s,
                "resume" => d.resume = s,
                "fork" => d.fork = s,
                "compaction" => d.compaction = s,
                "images" => d.images = s,
                "subagents" => d.subagents = s,
                "permission_surface" => d.permission_surface = s,
                "instruction_delivery" => d.instruction_delivery = s,
                "model_family" => d.model_family = s,
                "effort_vocabulary" => d.effort_vocabulary = s,
                "trajectory_export" => d.trajectory_export = s,
                "native_config" => d.native_config = s,
                _ => diags.push(ddiag(
                    Code::LoadParse,
                    "/hosted/declared_capabilities",
                    k,
                    &format!("unknown capability field `{k}`"),
                    "name one of the fifteen declared fields",
                    kernel,
                )),
            }
        }
    }
    d
}

/// `desugar(source) → Desugared` (§6.1 §2.2) — pure and total over the typed
/// source: every failure is a `Desugar`-stage diagnostic, never a panic or a
/// string error (V-1).
pub fn desugar(
    source: &AssemblySource,
    registry: &RegistryStore,
    snapshot_id: Option<&str>,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
    registrar: &ProvenanceRecord,
) -> Desugared {
    let mut diags = Vec::new();
    let mut seq: u64 = 0;

    // Dialect gate — the source's dialect is the assembly grammar's.
    if source.dialect != SOURCE_DIALECT {
        diags.push(ddiag(
            Code::LoadDialect,
            "/dialect",
            &source.dialect,
            &format!(
                "source dialect `{}` is not `{SOURCE_DIALECT}`",
                source.dialect
            ),
            "author `dialect` as `hir/1`",
            kernel,
        ));
    }
    if source.root_kind == RootKind::Native && source.hosted.is_some() {
        diags.push(ddiag(
            Code::LoadParse,
            "/hosted",
            "hosted",
            "`hosted` member on a `root_kind = native` source",
            "remove `hosted` or set `root_kind = hosted`",
            kernel,
        ));
    }
    if source.root_kind == RootKind::Hosted && source.hosted.is_none() {
        diags.push(ddiag(
            Code::LoadParse,
            "/hosted",
            "hosted",
            "`root_kind = hosted` requires the `hosted` block (the OpaqueProcess inputs)",
            "author `hosted{executor{harness, …}, …}`",
            kernel,
        ));
    }

    // D5 — slots/profile_binding are refused anywhere on a hosted source
    // (C-CLASS-6). The check scans the *decoded* fragments so a nested
    // authoring can't smuggle a slot in as opaque JSON.
    if source.root_kind == RootKind::Hosted {
        for (i, l) in source.layers.iter().enumerate() {
            let mut frag_diags = Vec::new();
            if let Some(f) = Assembly::from_json(
                &l.fragment,
                &format!("/layers/{i}/fragment"),
                kernel,
                &mut frag_diags,
            ) {
                if !f.slots.is_empty()
                    || !matches!(
                        f.profile_binding,
                        hh_assembly::grammar::ProfileBinding::Unbound
                    )
                {
                    diags.push(ddiag(
                        Code::ClassSlotsOnHosted,
                        &format!("/layers/{i}/fragment"),
                        &l.provenance.id,
                        "a hosted definition carries no `slots`/`profile_binding` — the OpaqueProcess boundary supplies them",
                        "remove slots/profile_binding from the layer (D5)",
                        kernel,
                    ));
                }
            }
            diags.extend(frag_diags);
        }
        for raw in &source.overrides {
            if raw.contains("slots.") || raw.starts_with("profile_binding") {
                diags.push(ddiag(
                    Code::ClassSlotsOnHosted,
                    "/overrides",
                    raw,
                    "a hosted definition carries no `slots`/`profile_binding` (D5)",
                    "remove the slot/profile override",
                    kernel,
                ));
            }
        }
    }

    // D1 — the base layer (lowest precedence).
    let mut layers: Vec<Layer> = Vec::new();
    let mut base_doc: Option<hh_hir::document::SealedDefinition> = None;
    if let Some(b) = &source.base {
        match base_definition(b, registry, snapshot_id) {
            Ok(sealed) => {
                let (frag, mut fdiags) = base_fragment(&sealed, kernel);
                // Re-tag decode diags to the desugar stage's read of the
                // base (they are C-LOAD-* already).
                diags.append(&mut fdiags);
                let min_prec = source
                    .layers
                    .iter()
                    .map(|l| l.provenance.precedence)
                    .min()
                    .unwrap_or(0);
                layers.push(Layer {
                    provenance: LayerProvenance {
                        source_kind: LayerSourceKind::PackagedDefault,
                        id: sealed.definition_ref.version_id.clone(),
                        version: "1".into(),
                        precedence: min_prec.saturating_sub(1).min(i64::MAX - 2),
                    },
                    fragment: frag,
                });
                base_doc = Some(sealed);
            }
            Err(e) => diags.push(ddiag(
                Code::RefUnresolved,
                "/base",
                b,
                &e,
                "name a registered sealed definition (version:<vid> or ns/name[@label])",
                kernel,
            )),
        }
    }

    // Authored layers — fragments decode member-wise (C-LOAD-* diags).
    for (i, l) in source.layers.iter().enumerate() {
        let mut fdiags = Vec::new();
        if let Some(frag) = Assembly::from_json(
            &l.fragment,
            &format!("/layers/{i}/fragment"),
            kernel,
            &mut fdiags,
        ) {
            layers.push(Layer {
                provenance: l.provenance.clone(),
                fragment: frag,
            });
        }
        diags.append(&mut fdiags);
    }

    // D3 — imports → one `user` layer's entities + document nodes.
    let mut import_nodes = Vec::new();
    if !source.imports.is_empty() {
        let mut frag = Assembly::empty();
        let canon = Json::Arr(
            source
                .imports
                .iter()
                .map(super::source::import_json)
                .collect(),
        )
        .to_canonical_string();
        for imp in &source.imports {
            let (name, binding, nodes) = desugar_import(imp, seq, kernel, &mut diags);
            frag.entities.insert(name, binding);
            import_nodes.extend(nodes);
        }
        let max_prec = layers
            .iter()
            .map(|l| l.provenance.precedence)
            .max()
            .unwrap_or(0);
        layers.push(Layer {
            provenance: LayerProvenance {
                source_kind: LayerSourceKind::User,
                id: idp_id("imports", canon.as_bytes()),
                version: "1".into(),
                precedence: (max_prec + 1).min(i64::MAX - 2),
            },
            fragment: frag,
        });
    }

    // The scaffold — hosted synthesises (D5); native takes `document` or the
    // base's sealed body.
    let mut doc = match source.root_kind {
        RootKind::Hosted => {
            let (d, params_space, values) = hosted_scaffold(
                source.hosted.as_ref().expect("checked above"),
                registrar,
                kernel,
                &mut diags,
                &mut seq,
            );
            // The scaffold's `parameters`/`values` ride a synthesised layer
            // below the authored ones (the `base` layer's slot) so authored
            // `values`/`parameters` overrides win on precedence (R-2.10.1²).
            if !params_space.is_empty() || !values.is_empty() {
                let min_prec = source
                    .layers
                    .iter()
                    .map(|l| l.provenance.precedence)
                    .min()
                    .unwrap_or(0);
                let mut frag = Assembly::empty();
                frag.parameters = params_space;
                frag.values = values;
                layers.push(Layer {
                    provenance: LayerProvenance {
                        source_kind: LayerSourceKind::PackagedDefault,
                        id: "hosted:scaffold".into(),
                        version: "1".into(),
                        precedence: min_prec.saturating_sub(1).min(i64::MAX - 2),
                    },
                    fragment: frag,
                });
            }
            d
        }
        RootKind::Native => match &source.document {
            Some(sd) => {
                let dj = Json::obj([
                    ("hir_version", Json::str("HIR/1")),
                    (
                        "root",
                        Json::obj([
                            ("semantic_id", Json::str(sd.root.clone())),
                            ("version_selector", Json::str("latest")),
                        ]),
                    ),
                    ("nodes", Json::Arr(sd.nodes.clone())),
                    ("edges", Json::Arr(sd.edges.clone())),
                ]);
                match hh_hir::wire::document_from_json(&dj) {
                    Ok(d) => d,
                    Err(e) => {
                        diags.push(ddiag(
                            Code::LoadParse,
                            "/document",
                            "document",
                            &format!("the authored document fails the `hir/1` schema: {e:?}"),
                            "author canonical `hir/1` node/edge records",
                            kernel,
                        ));
                        HirDocument::new(Ref::selected(sd.root.clone(), "latest"))
                    }
                }
            }
            None => match &base_doc {
                Some(b) => {
                    let mut d = b.document.clone();
                    d.assembly = None;
                    d
                }
                None => {
                    diags.push(ddiag(
                        Code::LoadParse,
                        "/document",
                        "document",
                        "a native source with no `base` must carry `document` (nodes/edges/root)",
                        "author `document` or name a `base`",
                        kernel,
                    ));
                    HirDocument::new(Ref::selected("", "latest"))
                }
            },
        },
    };
    doc.nodes.extend(import_nodes);

    // D4 — a document node's `native.slots` may not carry selector bindings
    // (selectors live inside layers, pinned under one snapshot at resolve).
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(ap) = &n.semantic {
            if let AgentProcessBody::Native(np) = &ap.body {
                let has_selector = np.slots.values().any(|bs| match bs {
                    hh_hir::records::SlotBindings::One(b) => {
                        matches!(b.variant.version, RefVersion::Selector(_))
                    }
                    hh_hir::records::SlotBindings::Many(v) => v
                        .iter()
                        .any(|b| matches!(b.variant.version, RefVersion::Selector(_))),
                });
                if has_selector {
                    diags.push(ddiag(
                        Code::LoadParse,
                        "/document/nodes",
                        &n.semantic_id(),
                        "a document node's `native.slots` carries a selector — selectors are legal only inside layers (D4)",
                        "bind slots through `layers[]`/`overrides`, not the document body",
                        kernel,
                    ));
                }
            }
        }
    }

    let experiment = materialise_experiment(source, catalog, kernel, &mut diags);

    Desugared {
        doc,
        layers,
        experiment,
        diags,
    }
}

/// The canonical derivation-key input — `canonical(desugared layers ∥
/// snapshot ∥ class_catalog.version ∥ assembler.version)` where a layer
/// encodes `{provenance, fragment}`.
pub fn derivation_key(
    layers: &[Layer],
    experiment: Option<&ExperimentLayer>,
    snapshot_id: &str,
    catalog: &dyn ClassCatalog,
    assembler_version: &str,
) -> String {
    let layer_json = |l: &LayerProvenance, f: &Assembly| {
        Json::obj([
            ("provenance", hh_assembly::grammar::layer_json(l)),
            ("fragment", f.to_json()),
        ])
    };
    let mut ls: Vec<Json> = layers
        .iter()
        .map(|l| layer_json(&l.provenance, &l.fragment))
        .collect();
    if let Some(e) = experiment {
        ls.push(layer_json(&e.provenance, &e.fragment));
    }
    // `class_catalog.version` — the catalog is a view; its version is the
    // content hash of its class set.
    let catalog_version = {
        let mut ids = catalog.class_ids();
        ids.sort();
        let canon =
            Json::Arr(ids.iter().map(|i| Json::str(i.clone())).collect()).to_canonical_string();
        idp_id("class_catalog", canon.as_bytes())
    };
    let canon = Json::obj([
        ("layers", Json::Arr(ls)),
        ("snapshot", Json::str(snapshot_id)),
        ("class_catalog_version", Json::str(catalog_version)),
        ("assembler_version", Json::str(assembler_version)),
    ])
    .to_canonical_string();
    idp_id("assemble", canon.as_bytes())
}

/// The `hh.lab/assembly` ext member — the desugared layer stack's
/// **provenance** plus the derivation key and the authored override/tombstone
/// spellings, carried inside the sealed definition so `explain`/`plan`
/// attribution is self-contained (S-7's `layers[]` record). Fragments are
/// never embedded: they carry authored `version_selector`s, and a sealed form
/// may not contain an unresolved selector — the provenance is the record;
/// `derivation_key` is the fragments' content address.
pub fn lab_ext(
    layers: &[Layer],
    experiment: Option<&ExperimentLayer>,
    derivation_key: &str,
) -> Json {
    let mut ls: Vec<Json> = layers
        .iter()
        .map(|l| hh_assembly::grammar::layer_json(&l.provenance))
        .collect();
    let mut overrides = Json::Null;
    let mut tombstones = Json::Null;
    if let Some(e) = experiment {
        ls.push(hh_assembly::grammar::layer_json(&e.provenance));
        overrides = Json::Arr(e.ops.iter().map(|o| Json::str(format!("{o:?}"))).collect());
        tombstones = Json::Arr(e.tombstones.iter().map(|t| Json::str(t.clone())).collect());
    }
    Json::obj([
        ("derivation_key", Json::str(derivation_key)),
        ("layers", Json::Arr(ls)),
        ("overrides", overrides),
        ("tombstones", tombstones),
    ])
}
