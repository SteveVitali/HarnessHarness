//! `diff` / `apply` / `invert` / `classify` (§3.1.7) — the typed `HirDiff` over canonical
//! records.
//!
//! - **diff pairing.** Nodes pair by `semantic_id` first; unmatched same-kind nodes pair by
//!   maximal canonical similarity (deterministic — ties break on canonical order). A rename,
//!   a reordered `argument_order` or a reworded `description_template` produces **only**
//!   `SurfaceEdit` ops and changes no `semantic_id` (AC-IR-05).
//! - **apply** is byte-exact: `apply(base, diff(base, target))` re-parses to `target` in
//!   canonical form; `invert` round-trips (AC-IR-06).
//! - **classification** counts `semantic | surface | provenance-only | ext` ops and computes
//!   the `authority | budget | validity | coordination` deltas and the conditioned-rule set
//!   (§3.1.7).
//! - **gates** (§3.1.7): `authority_delta = widening` is rejected when `provenance.origin =
//!   evolution` and otherwise requires `origin = human` *with attestation*; a model-origin
//!   diff touching a `flow_policy` action applies only when the edit strictly narrows it; an
//!   op touching a `conditioned_on` rule must carry or update its assumption-debt record
//!   (`ConditionedRuleIncomplete`).

use std::collections::BTreeMap;

use hh_provenance::{AuthorityClass, Origin, ProvenanceRecord};
use hh_wire::json::Json;

use crate::document::{DefinitionVersionRef, HirDocument, Node};
use crate::errors::HirError;
use crate::kinds::{EdgeKind, EntityKind};
use crate::leaves::Text;
use crate::records::{HarnessRuleRecord, KindRecord};
use crate::refs::{Ref, RunRef};

/// The document-level op target (`root`, `hir_version`).
const DOC: &str = "$doc";

// ─────────────────────────────────────────────────────────────────────────────
// Op vocabulary (§3.1.7) + tags
// ─────────────────────────────────────────────────────────────────────────────

/// The per-op tag (§3.1.7): `{plane, entity_kind, semantic}` — only `SurfaceEdit` and
/// `ExtEdit` are non-semantic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOpTag {
    /// The home plane of the touched kind (`None` for edge/doc ops).
    pub plane: Option<String>,
    /// The entity (or edge) kind the op touches.
    pub entity_kind: String,
    /// Whether the op enters the semantic projection.
    pub semantic: bool,
}

/// The closed diff-op vocabulary (§3.1.7). `ReplaceLeaf` carries the leaf's path in addition
/// to the spec's `(id, old_hash, new_hash)` so `apply` is exact under duplicate hashes; the
/// wire form records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    /// A node present in target, absent in base (carries the target node's canonical JSON).
    AddNode { node: Json, tag: DiffOpTag },
    /// A node removed in target (carries the removed node's canonical JSON so `invert`
    /// re-adds it — §3.1.7 invertibility).
    RemoveNode {
        id: String,
        node: Json,
        tag: DiffOpTag,
    },
    /// A semantic/structural field change on a paired node.
    ReplaceField {
        id: String,
        path: String,
        old: Json,
        new: Json,
        tag: DiffOpTag,
    },
    /// A `Text`/`CompiledPayload` leaf replaced (by hash).
    ReplaceLeaf {
        id: String,
        path: String,
        old_hash: String,
        new_hash: String,
        tag: DiffOpTag,
    },
    /// An edge present in target, absent in base.
    AddEdge { edge: Json, tag: DiffOpTag },
    /// An edge removed in target.
    RemoveEdge { edge: Json, tag: DiffOpTag },
    /// A `Ref` rebound (semantic_id or version coordinate changed).
    Rebind {
        id: String,
        path: String,
        old_ref: Json,
        new_ref: Json,
        tag: DiffOpTag,
    },
    /// A surface-record change — never enters `semantic_id` (AC-IR-05).
    SurfaceEdit {
        /// The profile the surface is rendered under, when known.
        profile: Option<String>,
        id: String,
        path: String,
        old: Json,
        new: Json,
        tag: DiffOpTag,
    },
    /// An `ext` map entry changed — never semantic, never authority/budget/validity.
    ExtEdit {
        id: String,
        key: String,
        old: Json,
        new: Json,
        tag: DiffOpTag,
    },
}

impl DiffOp {
    /// The op's tag.
    pub fn tag(&self) -> &DiffOpTag {
        match self {
            DiffOp::AddNode { tag, .. }
            | DiffOp::RemoveNode { tag, .. }
            | DiffOp::ReplaceField { tag, .. }
            | DiffOp::ReplaceLeaf { tag, .. }
            | DiffOp::AddEdge { tag, .. }
            | DiffOp::RemoveEdge { tag, .. }
            | DiffOp::Rebind { tag, .. }
            | DiffOp::SurfaceEdit { tag, .. }
            | DiffOp::ExtEdit { tag, .. } => tag,
        }
    }

    /// The node `id` (base-side `semantic_id`) the op touches, or [`DOC`].
    pub fn node_id(&self) -> &str {
        match self {
            DiffOp::AddNode { .. } => DOC,
            DiffOp::RemoveNode { id, .. }
            | DiffOp::ReplaceField { id, .. }
            | DiffOp::ReplaceLeaf { id, .. }
            | DiffOp::Rebind { id, .. }
            | DiffOp::SurfaceEdit { id, .. }
            | DiffOp::ExtEdit { id, .. } => id,
            DiffOp::AddEdge { .. } | DiffOp::RemoveEdge { .. } => DOC,
        }
    }

    /// Invert the op (swap sides).
    fn inverted(&self) -> DiffOp {
        match self.clone() {
            DiffOp::AddNode { node, tag } => DiffOp::RemoveNode {
                id: node
                    .get("version")
                    .and_then(|v| v.get("semantic_id"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                node,
                tag,
            },
            DiffOp::RemoveNode { node, tag, .. } => DiffOp::AddNode { node, tag },
            DiffOp::ReplaceField {
                id,
                path,
                old,
                new,
                tag,
            } => DiffOp::ReplaceField {
                id,
                path,
                old: new,
                new: old,
                tag,
            },
            DiffOp::ReplaceLeaf {
                id,
                path,
                old_hash,
                new_hash,
                tag,
            } => DiffOp::ReplaceLeaf {
                id,
                path,
                old_hash: new_hash,
                new_hash: old_hash,
                tag,
            },
            DiffOp::AddEdge { edge, tag } => DiffOp::RemoveEdge { edge, tag },
            DiffOp::RemoveEdge { edge, tag } => DiffOp::AddEdge { edge, tag },
            DiffOp::Rebind {
                id,
                path,
                old_ref,
                new_ref,
                tag,
            } => DiffOp::Rebind {
                id,
                path,
                old_ref: new_ref,
                new_ref: old_ref,
                tag,
            },
            DiffOp::SurfaceEdit {
                profile,
                id,
                path,
                old,
                new,
                tag,
            } => DiffOp::SurfaceEdit {
                profile,
                id,
                path,
                old: new,
                new: old,
                tag,
            },
            DiffOp::ExtEdit {
                id,
                key,
                old,
                new,
                tag,
            } => DiffOp::ExtEdit {
                id,
                key,
                old: new,
                new: old,
                tag,
            },
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Classification
// ─────────────────────────────────────────────────────────────────────────────

/// `authority_delta` (§3.1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityDelta {
    /// No authority-bearing change.
    None,
    /// Strictly narrower.
    Narrowing,
    /// Widening — gated on `provenance.origin` (§3.1.7).
    Widening,
}

/// `budget_delta` / `validity_delta` / `coordination_delta` (§3.1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delta {
    /// Unchanged.
    None,
    /// Strictly tighter (lower bounds, narrower window, less model autonomy).
    Tightening,
    /// Strictly looser.
    Loosening,
}

/// The diff classification (§3.1.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffClassification {
    /// Ops entering the semantic projection.
    pub semantic_ops: usize,
    /// Surface-only ops.
    pub surface_ops: usize,
    /// Provenance/version-record-only ops.
    pub provenance_only_ops: usize,
    /// `ext` ops.
    pub ext_ops: usize,
    /// The authority delta.
    pub authority_delta: AuthorityDelta,
    /// The budget delta.
    pub budget_delta: Delta,
    /// The validity delta.
    pub validity_delta: Delta,
    /// The coordination delta.
    pub coordination_delta: Delta,
    /// `semantic_id`s of conditioned rules the diff touches.
    pub touches_conditioned_rules: Vec<String>,
}

/// The derivation a diff carries (`derived-from{hypothesis, trajectories[], candidate_id?}`
/// — §3.1.7; empty only when `provenance.origin ∈ {human, migration}`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffDerivation {
    /// The hypothesis the diff instantiates.
    pub hypothesis: Option<Text>,
    /// The trajectories the diff was derived from.
    pub trajectories: Vec<RunRef>,
    /// The evolution candidate id, when applicable.
    pub candidate_id: Option<String>,
}

/// The typed `HirDiff` (§3.1.7) — itself a canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct HirDiff {
    /// The base definition ref.
    pub base: DefinitionVersionRef,
    /// The target definition ref.
    pub target: DefinitionVersionRef,
    /// The diff dialect (`HIR/1`).
    pub dialect: String,
    /// The ops (canonically sorted).
    pub ops: Vec<DiffOp>,
    /// The classification.
    pub classification: DiffClassification,
    /// The diff's own provenance — `origin` drives the widening gate.
    pub provenance: ProvenanceRecord,
    /// The `derived-from` record (empty only for `human`/`migration` origins).
    pub derivation: DiffDerivation,
}

impl HirDiff {
    /// The canonical JSON of the diff record (the codec is `schema::diff_to_json` —
    /// CC7). The form the boundary checks read — e.g. R-2.8.3's
    /// `SecretValueInDefinition` sweep over a diff in an evolution context (S1.13).
    pub fn to_json(&self) -> Json {
        crate::schema::diff_to_json(self)
    }

    /// The canonical bytes.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// diff
// ─────────────────────────────────────────────────────────────────────────────

/// `diff(base, target, provenance, derivation) → HirDiff` (§3.1.7). The gates run here:
/// a widening diff without a `human`-attested origin fails `AuthorityWidening`; a diff
/// touching a conditioned rule without carrying its debt record fails
/// `ConditionedRuleIncomplete`; a model-origin `flow_policy` edit must strictly narrow.
pub fn diff(
    base: &HirDocument,
    target: &HirDocument,
    provenance: ProvenanceRecord,
    derivation: DiffDerivation,
) -> Result<HirDiff, Vec<HirError>> {
    let mut errs = Vec::new();
    if let Err(e) = provenance.validate(None) {
        errs.push(crate::validate::map_provenance_error_pub(e, "hir diff"));
    }
    // `derived-from` is empty only for human|migration origins.
    let derived_empty = derivation.hypothesis.is_none()
        && derivation.trajectories.is_empty()
        && derivation.candidate_id.is_none()
        && provenance.derived_from.is_empty();
    if derived_empty
        && !matches!(
            provenance.origin,
            Origin::Human { .. } | Origin::Migration { .. }
        )
    {
        errs.push(HirError::SchemaViolation {
            detail: "diff provenance requires a derived-from record (non-human/migration origin)"
                .into(),
        });
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    let base_ref = definition_ref(base);
    let target_ref = definition_ref(target);
    let mut ops = Vec::new();

    // Document-level members.
    diff_doc_members(base, target, &mut ops);
    // Nodes.
    diff_nodes(base, target, &mut ops);
    // Edges.
    diff_edges(base, target, &mut ops);

    ops.sort_by_key(|op| crate::schema::diff_op_json(op).to_canonical_string());
    let classification = classify(base, target, &ops);

    // ── Gates (§3.1.7) ─────────────────────────────────────────────────────
    if classification.authority_delta == AuthorityDelta::Widening {
        let ok =
            matches!(&provenance.origin, Origin::Human { .. }) && provenance.attestation.is_some();
        if !ok {
            errs.push(HirError::AuthorityWidening {
                detail: format!(
                    "authority_delta = widening requires origin = human with attestation (got {})",
                    origin_name(&provenance.origin)
                ),
            });
        }
    }
    // A model-drafted flow-policy edit applies only when it strictly narrows.
    if matches!(
        &provenance.origin,
        Origin::Model { .. } | Origin::Evolution { .. }
    ) {
        for op in &ops {
            if let DiffOp::ReplaceField {
                path,
                old,
                new,
                tag,
                ..
            } = op
            {
                if tag.entity_kind == EntityKind::HarnessRule.name()
                    && (path.contains("flow_policy") || is_flow_policy_json(old))
                    && !json_strict_subset(new, old)
                {
                    errs.push(HirError::AuthorityWidening {
                        detail: "model/evolution-origin diff may only narrow a flow_policy".into(),
                    });
                }
            }
        }
    }
    // Conditioned-rule debt: a semantic op touching a conditioned rule must carry or
    // update its debt record.
    for rule_id in &classification.touches_conditioned_rules {
        let updates_debt = ops.iter().any(|op| {
            matches!(op, DiffOp::ReplaceField { id, path, .. } if id == rule_id && path.contains("assumption_debt"))
                || matches!(op, DiffOp::AddNode { node, .. } | DiffOp::RemoveNode { node, .. } if node_semantic_id(node).as_deref() == Some(rule_id))
        });
        if !updates_debt {
            errs.push(HirError::ConditionedRuleIncomplete {
                rule_id: rule_id.clone(),
            });
        }
    }

    if !errs.is_empty() {
        return Err(errs);
    }
    Ok(HirDiff {
        base: base_ref,
        target: target_ref,
        dialect: crate::DIALECT.to_string(),
        ops,
        classification,
        provenance,
        derivation,
    })
}

/// Same semantic coordinate, both sides pinned — a version-only re-pin.
fn is_repin(old_ref: &Json, new_ref: &Json) -> bool {
    let pinned = |j: &Json| j.get("version_id").is_some() && j.get("version_selector").is_none();
    let coordinate = |j: &Json| -> Option<(String, String)> {
        match j.get("semantic_id").and_then(Json::as_str) {
            Some(s) => Some((s.to_string(), String::new())),
            None => Some((
                j.get("class_id").and_then(Json::as_str)?.to_string(),
                j.get("variant_id").and_then(Json::as_str)?.to_string(),
            )),
        }
    };
    pinned(old_ref)
        && pinned(new_ref)
        && coordinate(old_ref).is_some()
        && coordinate(old_ref) == coordinate(new_ref)
}

fn origin_name(o: &Origin) -> &'static str {
    o.tag()
}

fn is_flow_policy_json(j: &Json) -> bool {
    j.get("flow_policy").is_some()
}

/// Strict JSON subset — `new ⊆ old` (a narrowing edit removes or shrinks, never adds).
fn json_strict_subset(new: &Json, old: &Json) -> bool {
    match (old, new) {
        (Json::Obj(o), Json::Obj(n)) => n
            .iter()
            .all(|(k, v)| o.get(k).is_some_and(|o| json_strict_subset(v, o))),
        (Json::Arr(o), Json::Arr(n)) => {
            n.len() <= o.len()
                && n.iter()
                    .zip(o.iter())
                    .all(|(n, o)| json_strict_subset(n, o))
        }
        _ => new == old,
    }
}

/// The document's definition ref (`{root semantic_id, root version_id}`).
fn definition_ref(doc: &HirDocument) -> DefinitionVersionRef {
    crate::identity::document_identity(doc).unwrap_or(DefinitionVersionRef {
        semantic_id: doc.root.semantic_id.clone(),
        version_id: match &doc.root.version {
            crate::refs::RefVersion::Pinned(v) => v.clone(),
            crate::refs::RefVersion::Selector(s) => s.clone(),
        },
    })
}

fn tag_for(kind: EntityKind, semantic: bool) -> DiffOpTag {
    DiffOpTag {
        plane: Some(kind.home().tag().to_string()),
        entity_kind: kind.name().to_string(),
        semantic,
    }
}

fn edge_tag(kind: EdgeKind) -> DiffOpTag {
    DiffOpTag {
        plane: None,
        entity_kind: kind.name().to_string(),
        semantic: true,
    }
}

fn doc_tag() -> DiffOpTag {
    DiffOpTag {
        plane: None,
        entity_kind: "document".to_string(),
        semantic: true,
    }
}

fn node_semantic_id(node_json: &Json) -> Option<String> {
    node_json
        .get("version")
        .and_then(|v| v.get("semantic_id"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| {
            crate::schema::node_from_json(node_json, "$")
                .ok()
                .map(|n| n.semantic_id())
        })
}

// ── document members ─────────────────────────────────────────────────────────

fn diff_doc_members(base: &HirDocument, target: &HirDocument, ops: &mut Vec<DiffOp>) {
    if base.root != target.root {
        ops.push(DiffOp::Rebind {
            id: DOC.into(),
            path: "root".into(),
            old_ref: base.root.to_json(),
            new_ref: target.root.to_json(),
            tag: doc_tag(),
        });
    }
    // The assembly section diffs at leaf granularity — `Rebind` for variant/entity refs,
    // `ReplaceField` for values/params/enabled/constraints (§3.3.4 `diff`; S1.9).
    match (&base.assembly, &target.assembly) {
        (Some(o), Some(n)) => diff_json_tagged(o, n, "assembly", DOC, &doc_tag(), ops),
        (o, n) if o != n => ops.push(DiffOp::ReplaceField {
            id: DOC.into(),
            path: "assembly".into(),
            old: o.clone().unwrap_or(Json::Null),
            new: n.clone().unwrap_or(Json::Null),
            tag: doc_tag(),
        }),
        _ => {}
    }
}

// ── node pairing + member diff ───────────────────────────────────────────────

fn flatten(j: &Json, path: String, out: &mut BTreeMap<String, String>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                flatten(v, format!("{path}.{k}"), out);
            }
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                flatten(v, format!("{path}[{i}]"), out);
            }
        }
        other => {
            out.insert(path, other.to_canonical_string());
        }
    }
}

/// Similarity between two node JSONs: the number of equal leaf positions in their canonical
/// flattening. Deterministic; used only for pairing (correctness never depends on it).
fn similarity(a: &Json, b: &Json) -> usize {
    let mut fa = BTreeMap::new();
    flatten(a, String::new(), &mut fa);
    let mut fb = BTreeMap::new();
    flatten(b, String::new(), &mut fb);
    fa.iter().filter(|(k, v)| fb.get(*k) == Some(*v)).count()
}

fn diff_nodes(base: &HirDocument, target: &HirDocument, ops: &mut Vec<DiffOp>) {
    // Index target nodes by semantic id.
    let mut base_by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, n) in base.nodes.iter().enumerate() {
        base_by_id.entry(n.semantic_id()).or_default().push(i);
    }
    let mut target_by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, n) in target.nodes.iter().enumerate() {
        target_by_id.entry(n.semantic_id()).or_default().push(i);
    }

    let mut paired_b: Vec<Option<usize>> = vec![None; base.nodes.len()];
    let mut paired_t: Vec<Option<usize>> = vec![None; target.nodes.len()];

    // Pass 1 — exact semantic-id pairing (zip same-id groups).
    for (sid, t_idx) in &target_by_id {
        if let Some(b_idx) = base_by_id.get(sid) {
            for (i, j) in b_idx.iter().zip(t_idx.iter()) {
                paired_b[*i] = Some(*j);
                paired_t[*j] = Some(*i);
            }
        }
    }

    // Pass 2 — same-kind maximal-similarity pairing of the remainder.
    let mut candidates: Vec<(usize, usize, usize)> = Vec::new(); // (score, b, t)
    for (i, b) in base.nodes.iter().enumerate() {
        if paired_b[i].is_some() {
            continue;
        }
        for (j, t) in target.nodes.iter().enumerate() {
            if paired_t[j].is_some() || t.kind != b.kind {
                continue;
            }
            candidates.push((similarity(&b.to_json(), &t.to_json()), i, j));
        }
    }
    // Deterministic greedy: descending score, then (b, t) order.
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (score, i, j) in candidates {
        if score == 0 || paired_b[i].is_some() || paired_t[j].is_some() {
            continue;
        }
        paired_b[i] = Some(j);
        paired_t[j] = Some(i);
    }

    for (i, b) in base.nodes.iter().enumerate() {
        match paired_b[i] {
            Some(j) => diff_node_members(b, &target.nodes[j], ops),
            None => ops.push(DiffOp::RemoveNode {
                id: b.semantic_id(),
                node: b.to_json(),
                tag: tag_for(b.kind, true),
            }),
        }
    }
    for (j, t) in target.nodes.iter().enumerate() {
        if paired_t[j].is_none() {
            ops.push(DiffOp::AddNode {
                node: t.to_json(),
                tag: tag_for(t.kind, true),
            });
        }
    }
}

fn diff_node_members(b: &Node, t: &Node, ops: &mut Vec<DiffOp>) {
    let id = b.semantic_id();
    let bj = b.to_json();
    let tj = t.to_json();
    for member in ["kind", "dialect", "provenance", "version", "assembly"] {
        let old = bj.get(member).cloned().unwrap_or(Json::Null);
        let new = tj.get(member).cloned().unwrap_or(Json::Null);
        if old != new {
            ops.push(DiffOp::ReplaceField {
                id: id.clone(),
                path: member.to_string(),
                old,
                new,
                tag: tag_for(b.kind, true),
            });
        }
    }
    diff_json(
        bj.get("semantic").unwrap_or(&Json::Null),
        tj.get("semantic").unwrap_or(&Json::Null),
        "semantic",
        &id,
        b.kind,
        ops,
    );
    diff_surface(bj.get("surface"), tj.get("surface"), &id, b.kind, ops);
    diff_ext(bj.get("ext"), tj.get("ext"), &id, b.kind, ops);
}

fn diff_surface(
    old: Option<&Json>,
    new: Option<&Json>,
    id: &str,
    kind: EntityKind,
    ops: &mut Vec<DiffOp>,
) {
    if old == new {
        return;
    }
    let profile = None; // profile conditioning metadata is filled by callers that know it
    match (old, new) {
        (Some(o), Some(n)) => diff_surface_json(o, n, "surface", id, kind, profile, ops),
        (o, n) => ops.push(DiffOp::SurfaceEdit {
            profile,
            id: id.to_string(),
            path: "surface".into(),
            old: o.cloned().unwrap_or(Json::Null),
            new: n.cloned().unwrap_or(Json::Null),
            tag: tag_for(kind, false),
        }),
    }
}

fn diff_surface_json(
    old: &Json,
    new: &Json,
    path: &str,
    id: &str,
    kind: EntityKind,
    profile: Option<String>,
    ops: &mut Vec<DiffOp>,
) {
    match (old, new) {
        (Json::Obj(o), Json::Obj(n)) => {
            for k in o
                .keys()
                .chain(n.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                match (o.get(k), n.get(k)) {
                    (Some(ov), Some(nv)) => diff_surface_json(
                        ov,
                        nv,
                        &format!("{path}{}", seg_key(k)),
                        id,
                        kind,
                        profile.clone(),
                        ops,
                    ),
                    (ov, nv) => ops.push(DiffOp::SurfaceEdit {
                        profile: profile.clone(),
                        id: id.to_string(),
                        path: format!("{path}{}", seg_key(k)),
                        old: ov.cloned().unwrap_or(Json::Null),
                        new: nv.cloned().unwrap_or(Json::Null),
                        tag: tag_for(kind, false),
                    }),
                }
            }
        }
        (Json::Arr(o), Json::Arr(n)) => {
            for i in 0..o.len().max(n.len()) {
                match (o.get(i), n.get(i)) {
                    (Some(ov), Some(nv)) => diff_surface_json(
                        ov,
                        nv,
                        &format!("{path}[{i}]"),
                        id,
                        kind,
                        profile.clone(),
                        ops,
                    ),
                    (ov, nv) => ops.push(DiffOp::SurfaceEdit {
                        profile: profile.clone(),
                        id: id.to_string(),
                        path: format!("{path}[{i}]"),
                        old: ov.cloned().unwrap_or(Json::Null),
                        new: nv.cloned().unwrap_or(Json::Null),
                        tag: tag_for(kind, false),
                    }),
                }
            }
        }
        _ => ops.push(DiffOp::SurfaceEdit {
            profile,
            id: id.to_string(),
            path: path.to_string(),
            old: old.clone(),
            new: new.clone(),
            tag: tag_for(kind, false),
        }),
    }
}

fn diff_ext(
    old: Option<&Json>,
    new: Option<&Json>,
    id: &str,
    kind: EntityKind,
    ops: &mut Vec<DiffOp>,
) {
    let empty = Json::Obj(BTreeMap::new());
    let o = old.unwrap_or(&empty);
    let n = new.unwrap_or(&empty);
    if let (Json::Obj(om), Json::Obj(nm)) = (o, n) {
        for k in om
            .keys()
            .chain(nm.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            match (om.get(k), nm.get(k)) {
                (Some(ov), Some(nv)) if ov == nv => {}
                (ov, nv) => ops.push(DiffOp::ExtEdit {
                    id: id.to_string(),
                    key: k.clone(),
                    old: ov.cloned().unwrap_or(Json::Null),
                    new: nv.cloned().unwrap_or(Json::Null),
                    tag: tag_for(kind, false),
                }),
            }
        }
    }
}

/// Recursive semantic-member diff — emits `ReplaceField`/`Rebind`/`ReplaceLeaf` at leaf
/// granularity.
fn diff_json(
    old: &Json,
    new: &Json,
    path: &str,
    id: &str,
    kind: EntityKind,
    ops: &mut Vec<DiffOp>,
) {
    diff_json_tagged(old, new, path, id, &tag_for(kind, true), ops)
}

/// The same walk over an explicit tag (the document-level assembly section uses the
/// document tag).
fn diff_json_tagged(
    old: &Json,
    new: &Json,
    path: &str,
    id: &str,
    tag: &DiffOpTag,
    ops: &mut Vec<DiffOp>,
) {
    if old == new {
        return;
    }
    // A `Ref` — `{semantic_id, version_id|version_selector}` — or a `ComponentVariantRef` —
    // `{class_id, variant_id, version_id|version_selector}` (§3.3.2) — diffs as a `Rebind`.
    let has_version =
        |j: &Json| j.get("version_id").is_some() || j.get("version_selector").is_some();
    let is_ref = |j: &Json| {
        has_version(j)
            && (j.get("semantic_id").is_some()
                || (j.get("class_id").is_some() && j.get("variant_id").is_some()))
    };
    if is_ref(old) && is_ref(new) {
        ops.push(DiffOp::Rebind {
            id: id.to_string(),
            path: path.to_string(),
            old_ref: old.clone(),
            new_ref: new.clone(),
            tag: tag.clone(),
        });
        return;
    }
    // A `Text`/`CompiledPayload` leaf — `content_hash`/`bytes_hash` changed → `ReplaceLeaf`.
    fn leaf_hash(j: &Json) -> Option<&str> {
        j.get("content_hash")
            .or_else(|| j.get("bytes_hash"))
            .and_then(Json::as_str)
    }
    if let (Some(oh), Some(nh)) = (leaf_hash(old), leaf_hash(new)) {
        if oh != nh {
            ops.push(DiffOp::ReplaceLeaf {
                id: id.to_string(),
                path: path.to_string(),
                old_hash: oh.to_string(),
                new_hash: nh.to_string(),
                tag: tag.clone(),
            });
        }
        // Remaining leaf members diff as fields (owner, authority, language, …).
        if let (Json::Obj(om), Json::Obj(nm)) = (old, new) {
            for k in om
                .keys()
                .chain(nm.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                if k == "content_hash" || k == "bytes_hash" {
                    continue;
                }
                match (om.get(k), nm.get(k)) {
                    (Some(ov), Some(nv)) => {
                        diff_json_tagged(ov, nv, &format!("{path}{}", seg_key(k)), id, tag, ops)
                    }
                    (ov, nv) => ops.push(DiffOp::ReplaceField {
                        id: id.to_string(),
                        path: format!("{path}{}", seg_key(k)),
                        old: ov.cloned().unwrap_or(Json::Null),
                        new: nv.cloned().unwrap_or(Json::Null),
                        tag: tag.clone(),
                    }),
                }
            }
        }
        return;
    }
    match (old, new) {
        (Json::Obj(om), Json::Obj(nm)) => {
            for k in om
                .keys()
                .chain(nm.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                match (om.get(k), nm.get(k)) {
                    (Some(ov), Some(nv)) => {
                        diff_json_tagged(ov, nv, &format!("{path}{}", seg_key(k)), id, tag, ops)
                    }
                    (ov, nv) => ops.push(DiffOp::ReplaceField {
                        id: id.to_string(),
                        path: format!("{path}{}", seg_key(k)),
                        old: ov.cloned().unwrap_or(Json::Null),
                        new: nv.cloned().unwrap_or(Json::Null),
                        tag: tag.clone(),
                    }),
                }
            }
        }
        (Json::Arr(oa), Json::Arr(na)) => {
            for i in 0..oa.len().max(na.len()) {
                match (oa.get(i), na.get(i)) {
                    (Some(ov), Some(nv)) => {
                        diff_json_tagged(ov, nv, &format!("{path}[{i}]"), id, tag, ops)
                    }
                    (ov, nv) => ops.push(DiffOp::ReplaceField {
                        id: id.to_string(),
                        path: format!("{path}[{i}]"),
                        old: ov.cloned().unwrap_or(Json::Null),
                        new: nv.cloned().unwrap_or(Json::Null),
                        tag: tag.clone(),
                    }),
                }
            }
        }
        _ => ops.push(DiffOp::ReplaceField {
            id: id.to_string(),
            path: path.to_string(),
            old: old.clone(),
            new: new.clone(),
            tag: tag.clone(),
        }),
    }
}

// ── edges ────────────────────────────────────────────────────────────────────

fn diff_edges(base: &HirDocument, target: &HirDocument, ops: &mut Vec<DiffOp>) {
    let mut target_edges: Vec<Json> = target.edges.iter().map(|e| e.to_json()).collect();
    for e in &base.edges {
        let ej = e.to_json();
        match target_edges.iter().position(|t| *t == ej) {
            Some(p) => {
                target_edges.remove(p);
            }
            None => ops.push(DiffOp::RemoveEdge {
                edge: ej,
                tag: edge_tag(e.kind),
            }),
        }
    }
    for t in target_edges {
        // The kind for the tag — parse is infallible on a well-formed edge JSON.
        let kind = t
            .get("kind")
            .and_then(Json::as_str)
            .and_then(|k| EdgeKind::parse(k).ok())
            .unwrap_or(EdgeKind::DependsOn);
        ops.push(DiffOp::AddEdge {
            edge: t,
            tag: edge_tag(kind),
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// classify
// ─────────────────────────────────────────────────────────────────────────────

/// `classify(base, target, ops)` — the §3.1.7 classification over a diff's ops.
/// `classify_pair(base, target) → DiffClassification` — the gate-free
/// reading of `diff`: the same op computation and classification fold,
/// without the provenance derivation or the §3.1.7 gates. The boundary's
/// I-1 check needs the *classification* of an override-applied pair
/// before deciding whether to refuse (`AuthorityWideningRequiresHuman`)
/// — `diff` itself *enforces* the gate, so a caller that must classify
/// first uses this.
pub fn classify_pair(base: &HirDocument, target: &HirDocument) -> DiffClassification {
    let mut ops = Vec::new();
    diff_doc_members(base, target, &mut ops);
    diff_nodes(base, target, &mut ops);
    diff_edges(base, target, &mut ops);
    ops.sort_by_key(|op| crate::schema::diff_op_json(op).to_canonical_string());
    classify(base, target, &ops)
}

pub fn classify(base: &HirDocument, target: &HirDocument, ops: &[DiffOp]) -> DiffClassification {
    let mut c = DiffClassification {
        semantic_ops: 0,
        surface_ops: 0,
        provenance_only_ops: 0,
        ext_ops: 0,
        authority_delta: AuthorityDelta::None,
        budget_delta: Delta::None,
        validity_delta: Delta::None,
        coordination_delta: Delta::None,
        touches_conditioned_rules: Vec::new(),
    };

    // Edge add/remove pairs that differ only in provenance/version are provenance-only.
    let adds: Vec<&Json> = ops
        .iter()
        .filter_map(|op| match op {
            DiffOp::AddEdge { edge, .. } => Some(edge),
            _ => None,
        })
        .collect();
    let removes: Vec<&Json> = ops
        .iter()
        .filter_map(|op| match op {
            DiffOp::RemoveEdge { edge, .. } => Some(edge),
            _ => None,
        })
        .collect();
    let prov_only_pair = |a: &Json, r: &Json| {
        let mut am = a.clone();
        let mut rm = r.clone();
        for j in [&mut am, &mut rm] {
            if let Json::Obj(m) = j {
                m.remove("provenance");
                m.remove("version");
            }
        }
        am == rm
    };
    let mut prov_only_edges: Vec<&Json> = Vec::new();

    for op in ops {
        match op {
            DiffOp::SurfaceEdit { .. } => c.surface_ops += 1,
            DiffOp::ExtEdit { .. } => c.ext_ops += 1,
            DiffOp::RemoveEdge { edge, .. } => {
                if adds.iter().any(|a| prov_only_pair(a, edge)) {
                    prov_only_edges.push(edge);
                    c.provenance_only_ops += 1;
                } else {
                    c.semantic_ops += 1;
                }
            }
            DiffOp::AddEdge { edge, .. } => {
                if removes.iter().any(|r| prov_only_pair(edge, r)) {
                    c.provenance_only_ops += 1;
                } else {
                    c.semantic_ops += 1;
                }
            }
            DiffOp::ReplaceField { path, .. } => {
                if path.starts_with("provenance") || path.starts_with("version") {
                    c.provenance_only_ops += 1;
                } else {
                    c.semantic_ops += 1;
                }
            }
            DiffOp::Rebind {
                path,
                old_ref,
                new_ref,
                ..
            } => {
                // A **re-pin** — both sides pinned, same semantic coordinate (a `Ref`'s
                // `semantic_id`; a `ComponentVariantRef`'s `{class_id, variant_id}` name) — is
                // a version-record consequence of sealing, not a semantic edit
                // (refs-by-semantic_id, §3.1.2; S1.9/ADR-0240). A rebind to another
                // semantic coordinate is semantic.
                if path.starts_with("provenance")
                    || path.starts_with("version")
                    || is_repin(old_ref, new_ref)
                {
                    c.provenance_only_ops += 1;
                } else {
                    c.semantic_ops += 1;
                }
            }
            _ => c.semantic_ops += 1,
        }
    }

    classify_deltas(base, target, ops, &mut c);

    // Conditioned rules touched.
    let mut touched = std::collections::BTreeSet::new();
    for op in ops {
        let id = op.node_id().to_string();
        for doc in [base, target] {
            if let Some(n) = doc.node(&id) {
                if let KindRecord::HarnessRule(HarnessRuleRecord {
                    conditioned_on: Some(_),
                    rule_id,
                    ..
                }) = &n.semantic
                {
                    let _ = rule_id;
                    touched.insert(n.semantic_id());
                }
            }
        }
        // For add/remove ops the node JSON itself may be a conditioned rule.
        if let DiffOp::AddNode { node, .. } | DiffOp::RemoveNode { node, .. } = op {
            if node.get("kind").and_then(Json::as_str) == Some("harness_rule")
                && node
                    .get("semantic")
                    .and_then(|s| s.get("conditioned_on"))
                    .is_some()
            {
                if let Some(sid) = node_semantic_id(node) {
                    touched.insert(sid);
                }
            }
        }
    }
    c.touches_conditioned_rules = touched.into_iter().collect();
    c
}

fn combine_authority(a: AuthorityDelta, b: AuthorityDelta) -> AuthorityDelta {
    // Widening dominates (conservative).
    match (a, b) {
        (AuthorityDelta::Widening, _) | (_, AuthorityDelta::Widening) => AuthorityDelta::Widening,
        (AuthorityDelta::Narrowing, _) | (_, AuthorityDelta::Narrowing) => {
            AuthorityDelta::Narrowing
        }
        _ => AuthorityDelta::None,
    }
}

fn combine_delta(a: Delta, b: Delta) -> Delta {
    match (a, b) {
        (Delta::Loosening, _) | (_, Delta::Loosening) => Delta::Loosening,
        (Delta::Tightening, _) | (_, Delta::Tightening) => Delta::Tightening,
        _ => Delta::None,
    }
}

fn classify_deltas(
    base: &HirDocument,
    target: &HirDocument,
    ops: &[DiffOp],
    c: &mut DiffClassification,
) {
    for op in ops {
        match op {
            DiffOp::ReplaceField {
                path,
                old,
                new,
                tag,
                ..
            } => {
                // Provenance authority.
                if path == "provenance.authority" {
                    c.authority_delta = combine_authority(
                        c.authority_delta,
                        authority_dir(
                            old.as_str().and_then(AuthorityClass::parse),
                            new.as_str().and_then(AuthorityClass::parse),
                        ),
                    );
                }
                if tag.entity_kind == EntityKind::Budget.name() && path.contains("dimensions") {
                    c.budget_delta = combine_delta(c.budget_delta, budget_dir(old, new));
                }
                if path.contains("validity") {
                    c.validity_delta = combine_delta(c.validity_delta, validity_dir(old, new));
                }
                if path.contains("control_boundary") {
                    c.coordination_delta =
                        combine_delta(c.coordination_delta, boundary_dir(old, new));
                }
                if path.contains("grants") || path.contains("issuer") {
                    c.authority_delta = combine_authority(c.authority_delta, grants_dir(old, new));
                }
            }
            DiffOp::Rebind {
                path,
                old_ref,
                new_ref,
                ..
            } => {
                // A rebind of a `budget`-named field: compare the resolved budgets.
                if path.ends_with("budget") || path.ends_with("bound") {
                    c.budget_delta = combine_delta(
                        c.budget_delta,
                        budget_rebind_dir(base, target, old_ref, new_ref),
                    );
                }
            }
            DiffOp::AddEdge { edge, .. } => match edge.get("kind").and_then(Json::as_str) {
                Some("authorizes") => {
                    c.authority_delta =
                        combine_authority(c.authority_delta, AuthorityDelta::Widening)
                }
                Some("delegated-to") => {
                    c.authority_delta =
                        combine_authority(c.authority_delta, AuthorityDelta::Widening)
                }
                _ => {}
            },
            DiffOp::RemoveEdge { edge, .. } => match edge.get("kind").and_then(Json::as_str) {
                Some("authorizes") | Some("delegated-to") => {
                    c.authority_delta =
                        combine_authority(c.authority_delta, AuthorityDelta::Narrowing)
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn authority_dir(old: Option<AuthorityClass>, new: Option<AuthorityClass>) -> AuthorityDelta {
    match (old, new) {
        (Some(o), Some(n)) if n > o => AuthorityDelta::Widening,
        (Some(o), Some(n)) if n < o => AuthorityDelta::Narrowing,
        (None, Some(_)) => AuthorityDelta::Widening,
        (Some(_), None) => AuthorityDelta::Narrowing,
        _ => AuthorityDelta::None,
    }
}

/// Grant direction: parse `{effect{domain}, scope, constraints{time?, count?}}` JSONs and
/// compare coverage — a strictly-larger grant widens.
fn grants_dir(old: &Json, new: &Json) -> AuthorityDelta {
    let cov = |g: &Json| -> (String, String, Option<u64>, Option<u64>) {
        (
            g.get("effect")
                .and_then(|e| e.get("domain"))
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            g.get("scope")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            g.get("constraints")
                .and_then(|c| c.get("time"))
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            g.get("constraints")
                .and_then(|c| c.get("count"))
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
        )
    };
    match (old, new) {
        (Json::Arr(o), Json::Arr(n)) => {
            if n.len() > o.len() {
                AuthorityDelta::Widening
            } else if n.len() < o.len() {
                AuthorityDelta::Narrowing
            } else {
                AuthorityDelta::None
            }
        }
        _ => {
            let (od, os, ot, oc) = cov(old);
            let (nd, ns, nt, nc) = cov(new);
            let scope_wider = scope_covers_str(&ns, &os) && ns != os;
            let scope_narrower = scope_covers_str(&os, &ns) && os != ns;
            if nd != od {
                return AuthorityDelta::Widening;
            }
            if scope_wider || (nt.is_none() && ot.is_some()) || (nc.is_none() && oc.is_some()) {
                AuthorityDelta::Widening
            } else if scope_narrower
                || (nt.is_some() && ot.is_none())
                || (nc.is_some() && oc.is_none())
            {
                AuthorityDelta::Narrowing
            } else {
                AuthorityDelta::None
            }
        }
    }
}

fn scope_covers_str(pattern: &str, scope: &str) -> bool {
    pattern == "*"
        || pattern == scope
        || pattern
            .strip_suffix('*')
            .is_some_and(|p| scope.starts_with(p))
}

fn budget_dir(old: &Json, new: &Json) -> Delta {
    // Compare `DimensionBound` JSONs `{hard?, soft?}` or a dimensions map of them.
    let bound = |j: &Json| -> (Option<i64>, Option<i64>) {
        (
            j.get("hard").and_then(Json::as_int),
            j.get("soft").and_then(Json::as_int),
        )
    };
    match (old, new) {
        (Json::Obj(om), Json::Obj(nm)) if om.keys().any(|k| nm.contains_key(k)) => {
            let mut d = Delta::None;
            for k in om
                .keys()
                .chain(nm.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                match (om.get(k), nm.get(k)) {
                    (Some(o), Some(n)) => {
                        let (oh, os) = bound(o);
                        let (nh, ns) = bound(n);
                        if nh > oh || ns > os {
                            d = combine_delta(d, Delta::Loosening);
                        } else if nh < oh || ns < os {
                            d = combine_delta(d, Delta::Tightening);
                        }
                    }
                    (Some(_), None) => d = combine_delta(d, Delta::Tightening),
                    (None, Some(_)) => d = combine_delta(d, Delta::Loosening),
                    _ => {}
                }
            }
            d
        }
        _ => {
            // A bound leaf (`…/dimensions/<dim>/hard|soft`) diffs as a
            // bare scalar pair — the cap *is* the value, so a higher
            // number loosens and a lower one tightens.
            if let (Some(ov), Some(nv)) = (old.as_int(), new.as_int()) {
                if nv > ov {
                    return Delta::Loosening;
                }
                if nv < ov {
                    return Delta::Tightening;
                }
                return Delta::None;
            }
            let (oh, os) = bound(old);
            let (nh, ns) = bound(new);
            if nh > oh || ns > os {
                Delta::Loosening
            } else if nh < oh || ns < os {
                Delta::Tightening
            } else {
                Delta::None
            }
        }
    }
}

fn budget_rebind_dir(
    base: &HirDocument,
    target: &HirDocument,
    old_ref: &Json,
    new_ref: &Json,
) -> Delta {
    let resolve = |doc: &HirDocument, r: &Json| {
        Ref::from_json(r, "$")
            .ok()
            .and_then(|r| doc.node(&r.semantic_id).cloned())
            .and_then(|n| match n.semantic {
                KindRecord::Budget(b) => Some(b),
                _ => None,
            })
    };
    match (resolve(base, old_ref), resolve(target, new_ref)) {
        (Some(o), Some(n)) => {
            // Loosening if any new bound exceeds the old.
            let mut d = Delta::None;
            for (k, nb) in &n.dimensions {
                match o.dimensions.get(k) {
                    Some(ob) if !nb.within(ob) => d = combine_delta(d, Delta::Loosening),
                    Some(_) => {}
                    None => d = combine_delta(d, Delta::Loosening),
                }
            }
            if d == Delta::None && n.dimensions != o.dimensions {
                Delta::Tightening
            } else {
                d
            }
        }
        _ => Delta::None,
    }
}

fn validity_dir(old: &Json, new: &Json) -> Delta {
    // `{from, until?|condition?}` — a wider window (later `until` / dropped bound) loosens.
    let until = |j: &Json| j.get("until").and_then(Json::as_int);
    match (old, new) {
        (Json::Obj(_), Json::Obj(_)) => {
            let ou = until(old);
            let nu = until(new);
            if nu > ou || (nu.is_none() && ou.is_some() && new.get("condition").is_none()) {
                Delta::Loosening
            } else if nu < ou || (nu.is_some() && ou.is_none()) {
                Delta::Tightening
            } else {
                Delta::None
            }
        }
        _ => Delta::None,
    }
}

fn boundary_dir(old: &Json, new: &Json) -> Delta {
    // Owner autonomy rank: model = 2, code = 1, human = 0 — more autonomy loosens.
    let rank = |s: &str| match s {
        "model" => 2,
        "code" => 1,
        "human" => 0,
        _ => 1,
    };
    let mut d = Delta::None;
    if let (Json::Obj(om), Json::Obj(nm)) = (
        old.get("assignments")
            .cloned()
            .unwrap_or(Json::Obj(BTreeMap::new())),
        new.get("assignments")
            .cloned()
            .unwrap_or(Json::Obj(BTreeMap::new())),
    ) {
        for k in om
            .keys()
            .chain(nm.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let o = om.get(k).and_then(Json::as_str).map(rank).unwrap_or(1);
            let n = nm.get(k).and_then(Json::as_str).map(rank).unwrap_or(1);
            if n > o {
                d = combine_delta(d, Delta::Loosening);
            } else if n < o {
                d = combine_delta(d, Delta::Tightening);
            }
        }
    }
    d
}

// ─────────────────────────────────────────────────────────────────────────────
// apply / invert
// ─────────────────────────────────────────────────────────────────────────────

/// `apply(base, diff) → target` (§3.1.7) — byte-exact over canonical records: the applied
/// document re-parses and canonicalizes to `target`'s canonical bytes.
pub fn apply(base: &HirDocument, diff: &HirDiff) -> Result<HirDocument, Vec<HirError>> {
    let mut doc = base.to_json();
    let mut nodes: Vec<Json> = match doc.get("nodes") {
        Some(Json::Arr(v)) => v.clone(),
        _ => Vec::new(),
    };
    let mut edges: Vec<Json> = match doc.get("edges") {
        Some(Json::Arr(v)) => v.clone(),
        _ => Vec::new(),
    };

    // Resolve op ids → node positions once (ids are base-side `semantic_id`s).
    let mut id_at: Vec<String> = nodes
        .iter()
        .map(|n| node_semantic_id(n).unwrap_or_default())
        .collect();
    let mut errs = Vec::new();

    for op in &diff.ops {
        match op {
            DiffOp::AddNode { node, .. } => {
                nodes.push(node.clone());
                id_at.push(node_semantic_id(node).unwrap_or_default());
            }
            DiffOp::RemoveNode { node, .. } => {
                if let Some(p) = nodes.iter().position(|n| *n == *node) {
                    nodes.remove(p);
                    id_at.remove(p);
                }
            }
            DiffOp::AddEdge { edge, .. } => edges.push(edge.clone()),
            DiffOp::RemoveEdge { edge, .. } => {
                if let Some(p) = edges.iter().position(|e| *e == *edge) {
                    edges.remove(p);
                }
            }
            DiffOp::ReplaceField {
                id, path, old, new, ..
            } => match target_of(&mut doc, &mut nodes, &id_at, id) {
                Some(target) => {
                    if let Err(e) = set_path(target, path, old, new) {
                        errs.push(e);
                    }
                }
                None => errs.push(unresolved_id(id)),
            },
            DiffOp::Rebind {
                id,
                path,
                old_ref,
                new_ref,
                ..
            } => match target_of(&mut doc, &mut nodes, &id_at, id) {
                Some(target) => {
                    if let Err(e) = set_path(target, path, old_ref, new_ref) {
                        errs.push(e);
                    }
                }
                None => errs.push(unresolved_id(id)),
            },
            DiffOp::ReplaceLeaf {
                id,
                path,
                old_hash,
                new_hash,
                ..
            } => match target_of(&mut doc, &mut nodes, &id_at, id) {
                Some(target) => {
                    if let Err(e) = set_leaf_hash(target, path, old_hash, new_hash) {
                        errs.push(e);
                    }
                }
                None => errs.push(unresolved_id(id)),
            },
            DiffOp::SurfaceEdit {
                id, path, old, new, ..
            } => match target_of(&mut doc, &mut nodes, &id_at, id) {
                Some(target) => {
                    if let Err(e) = set_path(target, path, old, new) {
                        errs.push(e);
                    }
                }
                None => errs.push(unresolved_id(id)),
            },
            DiffOp::ExtEdit {
                id, key, old, new, ..
            } => match target_of(&mut doc, &mut nodes, &id_at, id) {
                Some(target) => {
                    if let Json::Obj(_) = target {
                        let ext = get_or_insert_obj(target, "ext");
                        if let Json::Obj(m) = ext {
                            let cur = m.get(key).cloned().unwrap_or(Json::Null);
                            if cur != *old {
                                errs.push(stale(path_of(key), old, &cur));
                            } else if *new == Json::Null {
                                m.remove(key);
                            } else {
                                m.insert(key.clone(), new.clone());
                            }
                        }
                    }
                }
                None => errs.push(unresolved_id(id)),
            },
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    if let Json::Obj(m) = &mut doc {
        m.insert("nodes".into(), Json::Arr(nodes));
        m.insert("edges".into(), Json::Arr(edges));
    }
    crate::schema::document_from_json(&doc).map_err(|e| vec![e])
}

/// `invert(diff) → diff⁻¹` — `apply(target, invert(diff(base, target)))` re-parses to `base`
/// in canonical form (§3.1.7). The classification is inverted by symmetry.
pub fn invert(diff: &HirDiff) -> HirDiff {
    let mut ops: Vec<DiffOp> = diff.ops.iter().map(DiffOp::inverted).collect();
    ops.sort_by_key(|op| crate::schema::diff_op_json(op).to_canonical_string());
    let c = &diff.classification;
    HirDiff {
        base: diff.target.clone(),
        target: diff.base.clone(),
        dialect: diff.dialect.clone(),
        ops,
        classification: DiffClassification {
            semantic_ops: c.semantic_ops,
            surface_ops: c.surface_ops,
            provenance_only_ops: c.provenance_only_ops,
            ext_ops: c.ext_ops,
            authority_delta: match c.authority_delta {
                AuthorityDelta::Widening => AuthorityDelta::Narrowing,
                AuthorityDelta::Narrowing => AuthorityDelta::Widening,
                AuthorityDelta::None => AuthorityDelta::None,
            },
            budget_delta: match c.budget_delta {
                Delta::Loosening => Delta::Tightening,
                Delta::Tightening => Delta::Loosening,
                Delta::None => Delta::None,
            },
            validity_delta: match c.validity_delta {
                Delta::Loosening => Delta::Tightening,
                Delta::Tightening => Delta::Loosening,
                Delta::None => Delta::None,
            },
            coordination_delta: match c.coordination_delta {
                Delta::Loosening => Delta::Tightening,
                Delta::Tightening => Delta::Loosening,
                Delta::None => Delta::None,
            },
            touches_conditioned_rules: c.touches_conditioned_rules.clone(),
        },
        provenance: diff.provenance.clone(),
        derivation: diff.derivation.clone(),
    }
}

// ── apply internals ──────────────────────────────────────────────────────────

fn path_of(key: &str) -> String {
    format!("ext{}", seg_key(key))
}

fn stale(path: String, expected: &Json, found: &Json) -> HirError {
    HirError::UnresolvedRef {
        detail: format!(
            "stale diff op at {path}: expected {}, found {}",
            expected.to_canonical_string(),
            found.to_canonical_string()
        ),
    }
}

fn unresolved_id(id: &str) -> HirError {
    HirError::UnresolvedRef {
        detail: format!("diff op targets unknown node {id}"),
    }
}

/// The JSON value an op targets: the document for [`DOC`], else the node whose base-side
/// semantic id is `id` (`None` → the op fails `UnresolvedRef`).
fn target_of<'a>(
    doc: &'a mut Json,
    nodes: &'a mut [Json],
    id_at: &[String],
    id: &str,
) -> Option<&'a mut Json> {
    if id == DOC {
        return Some(doc);
    }
    id_at.iter().position(|s| s == id).map(|p| &mut nodes[p])
}

fn get_or_insert_obj<'a>(j: &'a mut Json, key: &str) -> &'a mut Json {
    if let Json::Obj(m) = j {
        if !matches!(m.get(key), Some(Json::Obj(_))) {
            m.insert(key.to_string(), Json::Obj(BTreeMap::new()));
        }
        m.get_mut(key).unwrap()
    } else {
        j
    }
}

/// A path segment — `.key` if `k` is plain (`[A-Za-z0-9_-]+`), else `["escaped key"]`
/// so keys containing `.`, `[`, `]` (budget dimension names like `tokens.total`) stay
/// unambiguous (§3.1.7 paths are canonical-JSON addresses).
fn seg_key(k: &str) -> String {
    if k.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        format!(".{k}")
    } else {
        let esc = k.replace('\\', "\\\\").replace('"', "\\\"");
        format!("[\"{esc}\"]")
    }
}

/// Parse `a.b[2].c` / `a["tokens.total"].hard` into segments.
fn parse_path(path: &str) -> Vec<PathSeg> {
    let b: Vec<char> = path.chars().collect();
    let mut segs = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '.' => {
                if !cur.is_empty() {
                    segs.push(PathSeg::Key(std::mem::take(&mut cur)));
                }
                i += 1;
            }
            '[' => {
                if !cur.is_empty() {
                    segs.push(PathSeg::Key(std::mem::take(&mut cur)));
                }
                i += 1;
                if b.get(i) == Some(&'"') {
                    i += 1;
                    let mut k = String::new();
                    while i < b.len() && b[i] != '"' {
                        if b[i] == '\\' && i + 1 < b.len() {
                            i += 1;
                        }
                        k.push(b[i]);
                        i += 1;
                    }
                    i += 1; // closing quote
                    if b.get(i) == Some(&']') {
                        i += 1;
                    }
                    segs.push(PathSeg::Key(k));
                } else {
                    let mut n = String::new();
                    while i < b.len() && b[i] != ']' {
                        n.push(b[i]);
                        i += 1;
                    }
                    if b.get(i) == Some(&']') {
                        i += 1;
                    }
                    if let Ok(v) = n.parse() {
                        segs.push(PathSeg::Idx(v));
                    }
                }
            }
            c => {
                cur.push(c);
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        segs.push(PathSeg::Key(cur));
    }
    segs
}

#[derive(Debug)]
enum PathSeg {
    Key(String),
    Idx(usize),
}

fn navigate_mut<'a>(j: &'a mut Json, segs: &[PathSeg]) -> Option<&'a mut Json> {
    let mut cur = j;
    for s in segs {
        cur = match s {
            PathSeg::Key(k) => match cur {
                Json::Obj(m) => m.get_mut(k.as_str())?,
                _ => return None,
            },
            PathSeg::Idx(i) => match cur {
                Json::Arr(v) => v.get_mut(*i)?,
                _ => return None,
            },
        };
    }
    Some(cur)
}

fn set_path(root: &mut Json, path: &str, old: &Json, new: &Json) -> Result<(), HirError> {
    let segs = parse_path(path);
    let Some((last, parents)) = segs.split_last() else {
        return Err(HirError::SchemaViolation {
            detail: "empty op path".into(),
        });
    };
    let parent = navigate_mut(root, parents).ok_or_else(|| HirError::UnresolvedRef {
        detail: format!("diff op path {path} does not resolve"),
    })?;
    match (parent, last) {
        (Json::Obj(m), PathSeg::Key(k)) => {
            let cur = m.get(k.as_str()).cloned().unwrap_or(Json::Null);
            if cur != *old {
                return Err(stale(path.to_string(), old, &cur));
            }
            if *new == Json::Null {
                m.remove(k.as_str());
            } else {
                m.insert(k.clone(), new.clone());
            }
            Ok(())
        }
        (Json::Arr(v), PathSeg::Idx(i)) => {
            let cur = v.get(*i).cloned().unwrap_or(Json::Null);
            if *i == v.len() && *old == Json::Null {
                v.push(new.clone());
                return Ok(());
            }
            if cur != *old {
                return Err(stale(path.to_string(), old, &cur));
            }
            if *new == Json::Null {
                v.remove(*i);
            } else {
                v[*i] = new.clone();
            }
            Ok(())
        }
        _ => Err(HirError::UnresolvedRef {
            detail: format!("diff op path {path} does not resolve"),
        }),
    }
}

fn set_leaf_hash(
    root: &mut Json,
    path: &str,
    old_hash: &str,
    new_hash: &str,
) -> Result<(), HirError> {
    let segs = parse_path(path);
    let leaf = navigate_mut(root, &segs).ok_or_else(|| HirError::UnresolvedRef {
        detail: format!("ReplaceLeaf path {path} does not resolve"),
    })?;
    for key in ["content_hash", "bytes_hash"] {
        if let Some(v) = leaf.get(key).and_then(Json::as_str) {
            if v == old_hash {
                if let Json::Obj(m) = leaf {
                    m.insert(key.to_string(), Json::str(new_hash));
                }
                return Ok(());
            }
        }
    }
    Err(stale(path.to_string(), &Json::str(old_hash), leaf))
}
