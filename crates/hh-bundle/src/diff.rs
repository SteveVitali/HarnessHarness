//! `diff(bundle_a, bundle_b) → BundleDiff` — the Lab's "what differs
//! between two arms" read (§5h.3 §2; ADR-0139 D6; AC-R-2.9.3-12). Pure
//! over the two decoded bundles: member-role deltas per section, the
//! `configuration_delta` (model/profile/environment/budget/seed), the
//! `sameness` L0–L4 verdict over the definition refs (the one identity
//! scheme, ADR-0037 D3), the `varied_factor[]` derivation for arm
//! bundles, and `level_delta`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::kinds::RecordKind;
use hh_identity::refs::VersionedRef;
use hh_identity::sameness;
use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::levels::roles;
use crate::manifest::BundleManifest;

/// `MemberDelta{role, a, b, kind}` — `kind ∈ {added, removed, changed,
/// surface_only}` (§5h.3 §2).
#[derive(Debug, Clone, PartialEq)]
pub struct MemberDelta {
    /// The member role.
    pub role: String,
    /// The member address in bundle a (`None` = absent).
    pub a: Option<String>,
    /// The member address in bundle b (`None` = absent).
    pub b: Option<String>,
    /// `added | removed | changed | surface_only`.
    pub kind: String,
}

impl MemberDelta {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("role", Json::str(self.role.clone())),
            ("a", self.a.clone().map(Json::str).unwrap_or(Json::Null)),
            ("b", self.b.clone().map(Json::str).unwrap_or(Json::Null)),
            ("kind", Json::str(self.kind.clone())),
        ])
    }
}

/// `BundleDiff{sections, definition_diff?, sameness, configuration_delta,
/// level_delta, varied_factor[], verdict}` (§5h.3 §2/§3).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleDiff {
    /// `version_id`s of the inputs.
    pub a: String,
    /// `version_id`s of the inputs.
    pub b: String,
    /// `section → MemberDelta[]` (member-role level).
    pub sections: BTreeMap<String, Vec<MemberDelta>>,
    /// The definition-section diff summary (`HirDiff` is the IR plane's;
    /// the bundle layer carries the ref pair + the sameness verdict).
    pub definition_diff: Option<Json>,
    /// The L0–L4 `Sameness` record over the two `definition` sections.
    pub sameness: Json,
    /// `{model, profile, environment, budget, seed}` — per-key `equal`
    /// or `{a, b}` deltas.
    pub configuration_delta: Json,
    /// `{claimed: {a, b}, max_supported: {a, b}}`.
    pub level_delta: Json,
    /// The varied factors — for arm/experiment bundles, the factors whose
    /// `level_assignment` differs between the two arms' `results.arm`
    /// documents (AC-R-2.9.3-12).
    pub varied_factors: Vec<String>,
    /// `identical` (same `version_id`) | `equivalent` (same subject +
    /// results + definition refs) | `divergent`.
    pub verdict: String,
}

impl BundleDiff {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-bundle-diff/1")),
            ("a", Json::str(self.a.clone())),
            ("b", Json::str(self.b.clone())),
            (
                "sections",
                Json::Obj(
                    self.sections
                        .iter()
                        .map(|(s, ds)| {
                            (
                                s.clone(),
                                Json::Arr(ds.iter().map(|d| d.to_json()).collect()),
                            )
                        })
                        .collect(),
                ),
            ),
            (
                "definition_diff",
                self.definition_diff.clone().unwrap_or(Json::Null),
            ),
            ("sameness", self.sameness.clone()),
            ("configuration_delta", self.configuration_delta.clone()),
            ("level_delta", self.level_delta.clone()),
            (
                "varied_factor",
                Json::Arr(self.varied_factors.iter().map(Json::str).collect()),
            ),
            ("verdict", Json::str(self.verdict.clone())),
        ])
    }
}

/// The section a member role belongs to (the diff's `sections` key).
fn role_section(role: &str) -> String {
    if role.starts_with("ledger_page:") || role.starts_with("ledger_tree:") {
        return "traces".to_string();
    }
    if let Some(rest) = role.strip_prefix("contains:") {
        let _ = rest;
        return "composition".to_string();
    }
    if let Some(rest) = role.strip_prefix("row:") {
        let _ = rest;
        return "results".to_string();
    }
    if role.starts_with("extension:") {
        return "resolved_dependencies".to_string();
    }
    match role {
        r if r == roles::SUBJECT => "subject".into(),
        r if r == roles::DEFINITION => "definition".into(),
        r if r == roles::ENVIRONMENT => "environment".into(),
        r if r == roles::COMPILED_BUNDLE => "definition".into(),
        r if r == roles::MODEL => "model".into(),
        r if r == roles::NONDETERMINISM => "nondeterminism".into(),
        r if r == roles::INSTRUMENT => "instrument".into(),
        r if r == roles::CONFIGURATION => "configuration".into(),
        r if r == roles::RESOLVED_DEPENDENCIES => "resolved_dependencies".into(),
        r if r == roles::RESULTS => "results".into(),
        other => other.to_string(),
    }
}

/// `role → (address, size, media)` over a manifest.
fn role_map(m: &BundleManifest) -> BTreeMap<String, (String, u64, String)> {
    m.members
        .iter()
        .map(|r| {
            (
                r.role.clone(),
                (r.address.clone(), r.size, r.media_type.clone()),
            )
        })
        .collect()
}

/// Decode a member JSON doc from a decoded bundle (`None` when absent or
/// not JSON).
fn member_doc(decoded: &Decoded, addr: &str) -> Option<Json> {
    decoded
        .members
        .get(addr)
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| hh_wire::json::parse(t).ok())
}

/// The arm's `level_assignment` — off the decoded `results.arm` member
/// doc (arm bundles), or the manifest's `results.arm` literal.
fn arm_levels(m: &BundleManifest, decoded: &Decoded) -> BTreeMap<String, String> {
    let arm_doc = m
        .results
        .get("arm")
        .and_then(Json::as_str)
        .and_then(|addr| member_doc(decoded, addr))
        .unwrap_or_else(|| m.results.get("arm").cloned().unwrap_or(Json::Null));
    let mut out = BTreeMap::new();
    if let Some(Json::Obj(la)) = arm_doc.get("level_assignment") {
        for (f, l) in la {
            if let Some(s) = l.as_str() {
                out.insert(f.clone(), s.to_string());
            }
        }
    }
    out
}

/// `diff(a, b)` — the pure comparison (§5h.3 §2).
pub fn bundle_diff(a: &Decoded, b: &Decoded) -> BundleDiff {
    let ma = &a.manifest;
    let mb = &b.manifest;

    // Member deltas per section.
    let ra = role_map(ma);
    let rb = role_map(mb);
    let mut roles_all: BTreeSet<String> = ra.keys().cloned().collect();
    roles_all.extend(rb.keys().cloned());
    let mut sections: BTreeMap<String, Vec<MemberDelta>> = BTreeMap::new();
    for role in roles_all {
        let (xa, xb) = (ra.get(&role), rb.get(&role));
        let (kind, va, vb) = match (xa, xb) {
            (Some((a_addr, a_size, a_media)), Some((b_addr, b_size, b_media))) => {
                if a_addr == b_addr && (a_size, a_media) == (b_size, b_media) {
                    continue; // identical member — no delta row.
                } else if a_addr == b_addr {
                    ("surface_only", Some(a_addr.clone()), Some(b_addr.clone()))
                } else {
                    ("changed", Some(a_addr.clone()), Some(b_addr.clone()))
                }
            }
            (Some((a_addr, _, _)), None) => ("removed", Some(a_addr.clone()), None),
            (None, Some((b_addr, _, _))) => ("added", None, Some(b_addr.clone())),
            (None, None) => continue,
        };
        sections
            .entry(role_section(&role))
            .or_default()
            .push(MemberDelta {
                role: role.clone(),
                a: va,
                b: vb,
                kind: kind.to_string(),
            });
    }

    // Definition diff + sameness (the L0–L4 ladder over the one identity
    // scheme — `derived-from` lineage counts as same-lineage).
    let def_a_vid = ma
        .definition
        .get("version_id")
        .and_then(Json::as_str)
        .unwrap_or("");
    let def_b_vid = mb
        .definition
        .get("version_id")
        .and_then(Json::as_str)
        .unwrap_or("");
    let def_a_sem = ma
        .definition
        .get("semantic_id")
        .and_then(Json::as_str)
        .unwrap_or("");
    let def_b_sem = mb
        .definition
        .get("semantic_id")
        .and_then(Json::as_str)
        .unwrap_or("");
    let same_lineage = {
        let chain_of = |m: &BundleManifest| -> BTreeSet<String> {
            let mut s = BTreeSet::new();
            if let Some(df) = m.composition.get("derived_from") {
                if let Some(id) = df.get("bundle_id").and_then(Json::as_str) {
                    s.insert(id.to_string());
                }
            }
            if let Some(Json::Arr(cs)) = m.composition.get("contains") {
                for c in cs {
                    if let Some(id) = c.get("bundle_id").and_then(Json::as_str) {
                        s.insert(id.to_string());
                    }
                }
            }
            s
        };
        let ca = chain_of(ma);
        let cb = chain_of(mb);
        ca.contains(&mb.version_id)
            || cb.contains(&ma.version_id)
            || !def_a_sem.is_empty() && def_a_sem == def_b_sem
    };
    // The diff's refs are `definition` coordinates — provenance is the
    // bundle boundary's own marker (never read from content).
    let prov = || hh_provenance::ProvenanceRecord::kernel("hh-bundle:diff", 0);
    let vref = |vid: &str, sem: &str| {
        let mut r = VersionedRef::pinned(RecordKind::SealedDefinition, vid, prov());
        if !sem.is_empty() {
            r.semantic_id = Some(sem.to_string());
        }
        r
    };
    let sam = sameness::sameness(
        &vref(def_a_vid, def_a_sem),
        &vref(def_b_vid, def_b_sem),
        same_lineage,
        None,
    );
    let sameness_json = Json::obj([
        ("level", Json::str(format!("{:?}", sam.level))),
        ("used_classification", Json::Bool(sam.used_classification)),
    ]);
    let definition_diff = Some(Json::obj([
        ("version_id_a", Json::str(def_a_vid)),
        ("version_id_b", Json::str(def_b_vid)),
        ("semantic_id_a", Json::str(def_a_sem)),
        ("semantic_id_b", Json::str(def_b_sem)),
    ]));

    // configuration_delta — per-key `equal` or the `{a, b}` pair.
    let cfg_keys = ["model", "profile", "environment", "budget", "seed"];
    let mut cfg_delta = BTreeMap::new();
    for k in cfg_keys {
        let va = ma.configuration.get(k).cloned().unwrap_or(Json::Null);
        let vb = mb.configuration.get(k).cloned().unwrap_or(Json::Null);
        cfg_delta.insert(
            k.to_string(),
            if va == vb {
                Json::str("equal")
            } else {
                Json::obj([("a", va), ("b", vb)])
            },
        );
    }
    // `model` compares the `model` section too (snapshots live there).
    if ma.model != mb.model {
        cfg_delta.insert(
            "model".to_string(),
            Json::obj([("a", ma.model.clone()), ("b", mb.model.clone())]),
        );
    }

    // level_delta — the declared claims.
    let lvl = |m: &BundleManifest| {
        Json::obj([
            (
                "claimed",
                m.reproducibility
                    .get("claimed_level")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
            (
                "max_supported",
                m.reproducibility
                    .get("max_supported_level")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
        ])
    };

    // varied_factor[] — the level-assignment diff between arm bundles.
    let la = arm_levels(ma, a);
    let lb = arm_levels(mb, b);
    let mut varied: Vec<String> = Vec::new();
    let mut factors: BTreeSet<&String> = la.keys().collect();
    factors.extend(lb.keys());
    for f in factors {
        if la.get(f) != lb.get(f) {
            varied.push(f.clone());
        }
    }

    let same_subject = ma.subject.run_ids == mb.subject.run_ids;
    let same_results = ma.results.get("rows") == mb.results.get("rows");
    let verdict = if ma.version_id == mb.version_id {
        "identical"
    } else if same_subject && same_results && def_a_vid == def_b_vid {
        "equivalent"
    } else {
        "divergent"
    };

    BundleDiff {
        a: ma.version_id.clone(),
        b: mb.version_id.clone(),
        sections,
        definition_diff,
        sameness: sameness_json,
        configuration_delta: Json::Obj(cfg_delta),
        level_delta: Json::obj([("a", lvl(ma)), ("b", lvl(mb))]),
        varied_factors: varied,
        verdict: verdict.to_string(),
    }
}
