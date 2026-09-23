//! `diff_view` (§6.1 §2.4; ADR-0149) — `AssemblyDiff = project(HirDiff)`, a
//! total, injective bucketing of every `DiffOp` into the assembly-level
//! vocabulary, keyed by assembly path, carrying `classification` verbatim and
//! `sameness ∈ L0..L4`, rendered Terraform-class. **T-1**
//! `flatten(project(d)) = d.ops` as a multiset; the projection is never
//! stored.

use std::collections::BTreeMap;

use hh_hir::diff::{DiffClassification, DiffOp};
use hh_identity::sameness::SamenessLevel;
use hh_wire::json::Json;

/// `L0..L4` spellings (hh-identity keeps the enum bare — the label is a view).
pub fn sameness_name(s: SamenessLevel) -> &'static str {
    match s {
        SamenessLevel::L0 => "L0",
        SamenessLevel::L1 => "L1",
        SamenessLevel::L2 => "L2",
        SamenessLevel::L3 => "L3",
        SamenessLevel::L4 => "L4",
    }
}

/// The closed bucket vocabulary (§6.1 §2.4; `participant_version_change` is
/// the T-4 hosted-only addition — a claim, ADR-0037 rule (c)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssemblyBucket {
    /// A slot's bound variant changed.
    SlotRebind,
    /// `enabled` flipped on one binding (T-5: exactly one op).
    SlotToggle,
    /// A bound variant's `params` member changed.
    SlotParamChange,
    /// `values.<p>` changed.
    ValueChange,
    /// `parameters.*` (the parameter space) changed.
    SpaceChange,
    /// An `entities` entry appeared.
    EntityAdd,
    /// An `entities` entry disappeared.
    EntityRemove,
    /// An `entities` entry's content changed.
    EntityChange,
    /// A `hir/1` edge appeared/disappeared/changed.
    EdgeChange,
    /// `constraints`/`authority_cap` changed.
    CapChange,
    /// `profile_binding` changed.
    ProfileConstraintChange,
    /// Hosted only: `participant_version` (a claim) changed.
    ParticipantVersionChange,
    /// Surface-record change only (never enters `semantic_id`).
    SurfaceOnly,
    /// An `ext` member changed.
    ExtOnly,
    /// Provenance/version-record change only.
    ProvenanceOnly,
}

impl AssemblyBucket {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            AssemblyBucket::SlotRebind => "slot_rebind",
            AssemblyBucket::SlotToggle => "slot_toggle",
            AssemblyBucket::SlotParamChange => "slot_param_change",
            AssemblyBucket::ValueChange => "value_change",
            AssemblyBucket::SpaceChange => "space_change",
            AssemblyBucket::EntityAdd => "entity_add",
            AssemblyBucket::EntityRemove => "entity_remove",
            AssemblyBucket::EntityChange => "entity_change",
            AssemblyBucket::EdgeChange => "edge_change",
            AssemblyBucket::CapChange => "cap_change",
            AssemblyBucket::ProfileConstraintChange => "profile_constraint_change",
            AssemblyBucket::ParticipantVersionChange => "participant_version_change",
            AssemblyBucket::SurfaceOnly => "surface_only",
            AssemblyBucket::ExtOnly => "ext_only",
            AssemblyBucket::ProvenanceOnly => "provenance_only",
        }
    }

    /// Parse the spelling back.
    pub fn parse(s: &str) -> Option<AssemblyBucket> {
        Some(match s {
            "slot_rebind" => AssemblyBucket::SlotRebind,
            "slot_toggle" => AssemblyBucket::SlotToggle,
            "slot_param_change" => AssemblyBucket::SlotParamChange,
            "value_change" => AssemblyBucket::ValueChange,
            "space_change" => AssemblyBucket::SpaceChange,
            "entity_add" => AssemblyBucket::EntityAdd,
            "entity_remove" => AssemblyBucket::EntityRemove,
            "entity_change" => AssemblyBucket::EntityChange,
            "edge_change" => AssemblyBucket::EdgeChange,
            "cap_change" => AssemblyBucket::CapChange,
            "profile_constraint_change" => AssemblyBucket::ProfileConstraintChange,
            "participant_version_change" => AssemblyBucket::ParticipantVersionChange,
            "surface_only" => AssemblyBucket::SurfaceOnly,
            "ext_only" => AssemblyBucket::ExtOnly,
            "provenance_only" => AssemblyBucket::ProvenanceOnly,
            _ => return None,
        })
    }
}

/// One projected op — the bucket, the assembly-level path, and the original
/// `DiffOp` (flatten is the identity on ops: T-1).
#[derive(Debug, Clone)]
pub struct AssemblyDiffOp {
    /// The bucket.
    pub bucket: AssemblyBucket,
    /// The assembly-level path the bucket keys on (`assembly.slots.<c>` →
    /// `slots.<c>`; node paths keep their `nodes.<sid>` form).
    pub path: String,
    /// The render glyph (`+`, `-`, `~`, `-/+`, `≈`, `·`).
    pub glyph: char,
    /// The underlying op.
    pub op: DiffOp,
}

/// `AssemblyDiff` — the projected view (§6.1 §2.4; derived, never stored).
#[derive(Debug, Clone)]
pub struct AssemblyDiff {
    /// The bucketed ops (same multiset as the source `HirDiff.ops` — T-1).
    pub ops: Vec<AssemblyDiffOp>,
    /// The `HirDiff` classification, verbatim.
    pub classification: DiffClassification,
    /// `sameness ∈ L0..L4` when the pair's refs are known (ADR-0037).
    pub sameness: Option<SamenessLevel>,
}

fn op_path(op: &DiffOp) -> (String, String) {
    // (raw op path, node/edge id)
    match op {
        DiffOp::AddNode { node, .. } => (
            format!("nodes.{}", node_semantic_id(node)),
            node_semantic_id(node),
        ),
        DiffOp::RemoveNode { id, .. } => (format!("nodes.{id}"), id.clone()),
        DiffOp::ReplaceField { id, path, .. }
        | DiffOp::ReplaceLeaf { id, path, .. }
        | DiffOp::Rebind { id, path, .. }
        | DiffOp::SurfaceEdit { id, path, .. } => (path.clone(), id.clone()),
        DiffOp::AddEdge { edge, .. } | DiffOp::RemoveEdge { edge, .. } => {
            (format!("edges.{}", edge_key(edge)), edge_key(edge))
        }
        DiffOp::ExtEdit { id, key, .. } => (format!("ext.{key}"), id.clone()),
    }
}

fn node_semantic_id(node: &Json) -> String {
    node.get("version")
        .and_then(|v| v.get("semantic_id"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

fn edge_key(edge: &Json) -> String {
    edge.get("id")
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| edge.to_canonical_string())
}

/// Bucket one op — the projection is total (every op lands somewhere; the
/// fallback is `entity_change`, never a dropped op).
fn bucket_of(op: &DiffOp, path: &str) -> AssemblyBucket {
    match op {
        DiffOp::SurfaceEdit { .. } => return AssemblyBucket::SurfaceOnly,
        DiffOp::ExtEdit { .. } => return AssemblyBucket::ExtOnly,
        DiffOp::AddEdge { .. } | DiffOp::RemoveEdge { .. } => return AssemblyBucket::EdgeChange,
        _ => {}
    }
    if let Some(rest) = path.strip_prefix("assembly.") {
        if let Some(s) = rest.strip_prefix("slots.") {
            if s.contains("enabled") {
                return AssemblyBucket::SlotToggle;
            }
            if s.contains("params") {
                return AssemblyBucket::SlotParamChange;
            }
            return AssemblyBucket::SlotRebind;
        }
        if rest.starts_with("values.") {
            return AssemblyBucket::ValueChange;
        }
        if rest.starts_with("parameters.") {
            return AssemblyBucket::SpaceChange;
        }
        if let Some(e) = rest.strip_prefix("entities.") {
            let _ = e;
            return match op {
                DiffOp::ReplaceField { old, new, .. } => {
                    if matches!(old, Json::Null) {
                        AssemblyBucket::EntityAdd
                    } else if matches!(new, Json::Null) {
                        AssemblyBucket::EntityRemove
                    } else {
                        AssemblyBucket::EntityChange
                    }
                }
                _ => AssemblyBucket::EntityChange,
            };
        }
        if rest.starts_with("constraints") {
            return AssemblyBucket::CapChange;
        }
        if rest.starts_with("profile_binding") {
            return AssemblyBucket::ProfileConstraintChange;
        }
        if rest.starts_with("ext.") {
            return AssemblyBucket::ExtOnly;
        }
        if rest.starts_with("layers") || rest.starts_with("resolved") {
            return AssemblyBucket::ProvenanceOnly;
        }
        return AssemblyBucket::ValueChange;
    }
    // Node-level ops on a hosted body's claim field (T-4).
    if path.contains("participant_version") || path.contains("version_identity") {
        return AssemblyBucket::ParticipantVersionChange;
    }
    match op {
        DiffOp::AddNode { .. } => AssemblyBucket::EntityAdd,
        DiffOp::RemoveNode { .. } => AssemblyBucket::EntityRemove,
        DiffOp::Rebind { .. } => {
            if path.contains("supplies") || path.contains("budget") || path.contains("permissions")
            {
                AssemblyBucket::EntityChange
            } else {
                AssemblyBucket::SlotRebind
            }
        }
        _ => {
            if path.starts_with("provenance")
                || path.starts_with("version")
                || path == "hir_version"
            {
                AssemblyBucket::ProvenanceOnly
            } else if path.contains("authority_cap") || path.contains("constraints") {
                AssemblyBucket::CapChange
            } else {
                AssemblyBucket::EntityChange
            }
        }
    }
}

fn glyph_of(op: &DiffOp) -> char {
    match op {
        DiffOp::AddNode { .. } | DiffOp::AddEdge { .. } => '+',
        DiffOp::RemoveNode { .. } | DiffOp::RemoveEdge { .. } => '-',
        DiffOp::Rebind { .. } | DiffOp::ReplaceField { .. } | DiffOp::ReplaceLeaf { .. } => '~',
        DiffOp::SurfaceEdit { .. } => '≈',
        DiffOp::ExtEdit { .. } => '·',
    }
}

/// `project(ops, classification, sameness) → AssemblyDiff` (T-1: the bucketed
/// ops are the same multiset — `flatten` returns them verbatim).
pub fn project(
    ops: Vec<DiffOp>,
    classification: DiffClassification,
    sameness: Option<SamenessLevel>,
) -> AssemblyDiff {
    let ops = ops
        .into_iter()
        .map(|op| {
            let (path, _id) = op_path(&op);
            AssemblyDiffOp {
                bucket: bucket_of(&op, &path),
                path,
                glyph: glyph_of(&op),
                op,
            }
        })
        .collect();
    AssemblyDiff {
        ops,
        classification,
        sameness,
    }
}

/// `flatten(project(d)) = d.ops` (T-1).
pub fn flatten(d: &AssemblyDiff) -> Vec<DiffOp> {
    d.ops.iter().map(|o| o.op.clone()).collect()
}

/// Terraform-class render — `+` add, `-` remove, `~` change, `≈` surface-only,
/// `·` provenance/ext-only — one line per op, sorted by path (deterministic).
pub fn render(d: &AssemblyDiff) -> String {
    let mut lines: Vec<String> = d
        .ops
        .iter()
        .map(|o| format!("{} {} ({})", o.glyph, o.path, o.bucket.name()))
        .collect();
    lines.sort();
    let mut out = String::new();
    if let Some(s) = d.sameness {
        out.push_str(&format!("sameness: {}\n", sameness_name(s)));
    }
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out
}

/// The diff's canonical JSON view (for `explain`/`plan` wire output — the
/// projection itself is never stored as a registry record).
pub fn diff_json(d: &AssemblyDiff) -> Json {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for o in &d.ops {
        *counts.entry(o.bucket.name().to_string()).or_insert(0) += 1;
    }
    Json::obj([
        (
            "ops",
            Json::Arr(
                d.ops
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("bucket", Json::str(o.bucket.name())),
                            ("path", Json::str(o.path.clone())),
                            ("glyph", Json::str(o.glyph.to_string())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "bucket_counts",
            Json::Obj(counts.into_iter().map(|(k, v)| (k, Json::Int(v))).collect()),
        ),
        (
            "sameness",
            d.sameness
                .map(|s| Json::str(sameness_name(s)))
                .unwrap_or(Json::Null),
        ),
        ("render", Json::str(render(d))),
    ])
}
