//! `sweep` — the OFAT sweep glue E2-T10 names (manifest 304/R6.43;
//! spec §8.1–§8.2; design/37 Part 1/§2.3; design/38 §6.1–§6.3):
//!
//! - **`sweep-registry/1`** — the frozen factor/level registry record
//!   (REQ-XD-201/202; alias REQ-XE-014): every level declares its
//!   factor, typed paths, baseline and level values, tier, exercising
//!   condition, activation metric, forecast source, cost prior,
//!   dependency tickets and the upstream locator or `"HH-only"`
//!   (non-upstream constants marked). Tier 1 is exactly V1–V9 of
//!   §8.1; Tier 2 carries each listed candidate with its reason. The
//!   registry freezes with its sha256 — the hash is the exploration
//!   identity; a level added or changed after the freeze is a new
//!   identity and its data never merge (REQ-XD-202's merge refusal).
//! - **level materialization** (REQ-XE-124): each OFAT perturbation is
//!   one override spelling; `hh_assembly::enumerate` produces the
//!   point, `resolve` re-pins and seals it, and the sealed
//!   `version_id` is `LevelSpec.ref_`. Override spellings are kept for
//!   attribution only — there is no runtime override interpreter; a
//!   level opened by override instead of by ref refuses typed.
//! - **the declared-delta and liveness check** (REQ-XE-125; aliases
//!   REQ-XD-125, REQ-XD-203): before any paid sweep slot, the variant
//!   document differs from the frozen baseline in exactly one
//!   registered slot, its request diff is confined to declared paths
//!   and non-empty on the factor's exercising branch, and
//!   executor-level knobs additionally pass the executed scripted leg.
//!   Otherwise the variant is refused — `inert{factor,branch}`,
//!   `unexpressible{…}` or `no_admission_path{…}`, never a silent
//!   downgrade. A swept variant carries no parity claim.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hharness_assembly::catalog::ClassCatalog;
use hharness_assembly::grammar::{Assembly, ParamRequirement, ParamType, ParameterSpec};
use hharness_assembly::resolve::{resolve, ResolveEnv};
use hharness_assembly::space::{enumerate, space, Override, SweepDesign};
use hharness_assembly::Stage1Catalog;
use hharness_hir::document::{HirDocument, Node, SealedDefinition};
use hharness_hir::kinds::EntityKind;
use hharness_hir::leaves::Text;
use hharness_hir::records::*;
use hharness_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hharness_identity::names::ResolveMode;
use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_registry::kinds::{Cardinality, Placement};
use hharness_registry::records::{
    AppliesTo, ClassRecord, ContractOperation, Implementation, RegistryRecord, VariantRecord,
};
use hharness_registry::store::RegistryStore;
use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

use crate::experiment::LevelSpec;
use hharness_ontology::participant::ParticipantClass;

/// The object map, when this is an object (Json destructures, never
/// guesses).
fn as_obj(j: &Json) -> Option<&BTreeMap<String, Json>> {
    match j {
        Json::Obj(m) => Some(m),
        _ => None,
    }
}

/// The items, when this is an array.
fn as_arr(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

/// `sweep-registry/1` — the frozen factor/level registry record.
pub const SWEEP_REGISTRY_SCHEMA: &str = "sweep-registry/1";

/// The REQ-XH-115 label a swept variant carries wherever it surfaces.
pub const SWEEP_VARIANT_LABEL: &str =
    "single pre-registered contrast; no exploration; local to the baseline configuration";

/// R6.33's committed `v5_declaration` sha256 — the sealed V5 level's
/// declaration must reproduce the canonical sub-object byte-equal
/// (the AC-3 refusal test; never edit the pre-registration to fit).
pub const V5_DECLARATION_SHA256: &str =
    "e950043fb986461e939839c8a1c892c512813e4aff7933fe106033cd6ab1ef0a";

/// The §8.1 Tier-1 level ids — the registry's Tier 1 is exactly these.
pub const TIER1_LEVEL_IDS: [&str; 9] = ["V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8", "V9"];

/// The §8.1 Tier-2 candidate names — registered, not run, each with
/// its recorded reason.
pub const TIER2_CANDIDATES: [&str; 7] = [
    "step-limit",
    "format-error-cap",
    "truncation-off",
    "context-policy",
    "interaction-mode-shim",
    "explicit-temperature",
    "cost-limit",
];

// ── the typed refusals ───────────────────────────────────────────────────────

/// `SweepRefusal` — every sweep check refuses typed, never silently.
#[derive(Debug, Clone, PartialEq)]
pub enum SweepRefusal {
    /// A required registry member is absent.
    MissingMember {
        /// The member path.
        member: String,
    },
    /// A member outside the closed registry vocabulary.
    UnknownMember {
        /// The member path.
        member: String,
    },
    /// A level names neither an upstream locator nor `"HH-only"`.
    MissingUpstream {
        /// The level id.
        level_id: String,
    },
    /// A declared constant is not marked non-upstream.
    UnmarkedNonUpstream {
        /// The level id and member.
        detail: String,
    },
    /// The tier table violates REQ-XD-202 (Tier 1 exactly V1–V9; Tier 2
    /// each with a reason).
    TierViolation {
        /// What violated.
        detail: String,
    },
    /// The declared params cannot express the experiment's delta —
    /// `enumerate` rejected the point or resolve/seal refused
    /// (`UnexpressibleSurface`, never a silent downgrade).
    UnexpressibleSurface {
        /// The level id and what refused.
        detail: String,
    },
    /// A level was opened by override spelling instead of by sealed
    /// `ref_` — attribution is never an interpreter (REQ-XE-124).
    OpenedByOverride {
        /// The level id.
        level_id: String,
    },
    /// The doc-level delta touches a path outside the declared typed
    /// paths, or a second slot.
    UndeclaredDelta {
        /// The offending path.
        path: String,
    },
    /// The variant is declarable but its diff is empty on the
    /// factor's exercising branch, or its executed leg failed —
    /// refused `inert{factor,branch}` (REQ-XE-125).
    Inert {
        /// The factor id.
        factor_id: String,
        /// The exercising branch.
        branch: String,
    },
    /// A level is declarable but has no live admission path (the
    /// required liveness evidence kind is absent at arm time).
    NoAdmissionPath {
        /// The level id and what is missing.
        detail: String,
    },
    /// The V5 declaration does not equal R6.33's canonical
    /// declaration (sha256 mismatch — never edited to fit).
    DeclarationMismatch {
        /// What mismatched.
        detail: String,
    },
    /// Data attempted to merge across two exploration identities
    /// (a level added or changed after the freeze — REQ-XD-202).
    MergeAcrossIdentities {
        /// The two identities.
        detail: String,
    },
}

impl std::fmt::Display for SweepRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SweepRefusal::MissingMember { member } => {
                write!(f, "refused{{missing_member:{member}}}")
            }
            SweepRefusal::UnknownMember { member } => {
                write!(f, "refused{{unknown_member:{member}}}")
            }
            SweepRefusal::MissingUpstream { level_id } => {
                write!(f, "refused{{missing_upstream:{level_id}}}")
            }
            SweepRefusal::UnmarkedNonUpstream { detail } => {
                write!(f, "refused{{unmarked_non_upstream:{detail}}}")
            }
            SweepRefusal::TierViolation { detail } => {
                write!(f, "refused{{tier:{detail}}}")
            }
            SweepRefusal::UnexpressibleSurface { detail } => {
                write!(f, "unexpressible{{{detail}}}")
            }
            SweepRefusal::OpenedByOverride { level_id } => {
                write!(f, "refused{{opened_by_override:{level_id}}}")
            }
            SweepRefusal::UndeclaredDelta { path } => {
                write!(f, "refused{{undeclared_delta:{path}}}")
            }
            SweepRefusal::Inert { factor_id, branch } => {
                write!(f, "inert{{{factor_id},{branch}}}")
            }
            SweepRefusal::NoAdmissionPath { detail } => {
                write!(f, "refused{{no_admission_path:{detail}}}")
            }
            SweepRefusal::DeclarationMismatch { detail } => {
                write!(f, "refused{{declaration_mismatch:{detail}}}")
            }
            SweepRefusal::MergeAcrossIdentities { detail } => {
                write!(f, "refused{{merge_across_identities:{detail}}}")
            }
        }
    }
}
impl std::error::Error for SweepRefusal {}

// ── `sweep-registry/1` ───────────────────────────────────────────────────────

/// The required members of one registry level entry.
pub const LEVEL_REQUIRED: &[&str] = &[
    "level_id",
    "level_name",
    "factor_id",
    "typed_paths",
    "baseline",
    "level",
    "tier",
    "exercising_condition",
    "activation_metric",
    "forecast_source",
    "cost_prior",
    "dependency_tickets",
    "upstream_locator",
    "non_upstream_constants",
];

/// The optional members of one registry level entry.
pub const LEVEL_OPTIONAL: &[&str] = &[
    "constants",
    "liveness",
    "override_spellings",
    "sealed_ref",
    "declaration",
    "declaration_sha256",
    "reason",
    "admission_route",
];

/// The exercising branch of a level's liveness declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Branch {
    /// The knob acts on every request.
    Always,
    /// The knob acts only from the second request on (e.g. replayed
    /// reasoning — nothing to replay at turn 1).
    AfterTurn(u64),
    /// The knob acts only when its declared condition occurs (the
    /// request-diff leg cannot prove it — the forecast gate decides).
    Conditional,
}

impl Branch {
    /// The canonical spelling.
    pub fn as_str(&self) -> String {
        match self {
            Branch::Always => "always".to_string(),
            Branch::AfterTurn(t) => format!("after_turn:{t}"),
            Branch::Conditional => "conditional".to_string(),
        }
    }

    /// `true` when `turn` (1-based) is inside the branch.
    pub fn contains(&self, turn: usize) -> bool {
        match self {
            Branch::Always => true,
            Branch::AfterTurn(t) => (turn as u64) > *t,
            Branch::Conditional => true,
        }
    }
}

/// The liveness evidence kind a level needs (REQ-XE-125).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivenessKind {
    /// A renderer/lane knob — the L1.1 request diff proves it.
    RequestDiff,
    /// An executor-level knob — L1.1's observation replay produces no
    /// request diff; the executed scripted-model leg proves it.
    ExecutorLeg,
}

/// One level's liveness declaration — the declared request paths the
/// diff is confined to, the exercising branch, and the kind of
/// evidence that admits it.
#[derive(Debug, Clone, PartialEq)]
pub struct LivenessDecl {
    /// The evidence kind.
    pub kind: LivenessKind,
    /// Declared wire-side diff paths (`*` is a path-segment wildcard).
    pub declared_request_paths: Vec<String>,
    /// The exercising branch.
    pub exercising_branch: Branch,
}

/// `check_registry(reg)` — the `sweep-registry/1` validator
/// (REQ-XD-201/202; AC-8.1-1/2): strict member-set decode, every
/// required member present, each level cites an upstream locator or
/// `"HH-only"`, non-upstream constants marked, Tier 1 exactly V1–V9,
/// Tier 2 each carrying its reason. Returns every refusal collected.
pub fn check_registry(reg: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(m) = as_obj(reg) else {
        out.push(SweepRefusal::MissingMember {
            member: "registry".to_string(),
        });
        return out;
    };
    if m.get("schema").and_then(Json::as_str) != Some(SWEEP_REGISTRY_SCHEMA) {
        out.push(SweepRefusal::MissingMember {
            member: "schema = sweep-registry/1".to_string(),
        });
    }
    for k in m.keys() {
        if ![
            "schema",
            "levels",
            "tier2",
            "frozen_sha256",
            "exploration_identity",
            "note",
            "parity",
            "label",
            "recorded_at",
            "baseline",
        ]
        .contains(&k.as_str())
        {
            out.push(SweepRefusal::UnknownMember {
                member: format!("registry.{k}"),
            });
        }
    }
    let mut tier1: BTreeSet<String> = BTreeSet::new();
    match m.get("levels").and_then(as_arr) {
        Some(levels) => {
            for (i, l) in levels.iter().enumerate() {
                check_level(l, i, &mut tier1, &mut out);
            }
        }
        None => out.push(SweepRefusal::MissingMember {
            member: "levels".to_string(),
        }),
    }
    let want: BTreeSet<String> = TIER1_LEVEL_IDS.iter().map(|s| s.to_string()).collect();
    if tier1 != want {
        out.push(SweepRefusal::TierViolation {
            detail: format!("Tier 1 is {tier1:?} — required exactly V1–V9 of §8.1"),
        });
    }
    // Tier 2 — each listed candidate with its reason (REQ-XD-202).
    let mut t2_names: BTreeSet<String> = BTreeSet::new();
    match m.get("tier2").and_then(as_arr) {
        Some(t2) => {
            for (i, l) in t2.iter().enumerate() {
                let Some(lm) = as_obj(l) else {
                    out.push(SweepRefusal::MissingMember {
                        member: format!("tier2[{i}]"),
                    });
                    continue;
                };
                let id = lm
                    .get("level_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    out.push(SweepRefusal::MissingMember {
                        member: format!("tier2[{i}].level_id"),
                    });
                }
                t2_names.insert(id.clone());
                for k in ["factor_id", "reason", "upstream_locator"] {
                    if lm.get(k).and_then(Json::as_str).map(|s| !s.is_empty()) != Some(true) {
                        out.push(SweepRefusal::MissingMember {
                            member: format!("tier2[{i}].{k}"),
                        });
                    }
                }
                if lm.get("reason").and_then(Json::as_str) == Some("") {
                    out.push(SweepRefusal::TierViolation {
                        detail: format!("tier2[{i}] {id} carries no reason"),
                    });
                }
            }
        }
        None => out.push(SweepRefusal::MissingMember {
            member: "tier2".to_string(),
        }),
    }
    let want2: BTreeSet<String> = TIER2_CANDIDATES.iter().map(|s| s.to_string()).collect();
    if t2_names != want2 {
        out.push(SweepRefusal::TierViolation {
            detail: format!(
                "Tier 2 is {t2_names:?} — required each §8.1 candidate with its reason"
            ),
        });
    }
    out
}

/// One Tier-1 level's member check.
fn check_level(l: &Json, i: usize, tier1: &mut BTreeSet<String>, out: &mut Vec<SweepRefusal>) {
    let Some(lm) = as_obj(l) else {
        out.push(SweepRefusal::MissingMember {
            member: format!("levels[{i}]"),
        });
        return;
    };
    for k in lm.keys() {
        if !LEVEL_REQUIRED.contains(&k.as_str()) && !LEVEL_OPTIONAL.contains(&k.as_str()) {
            out.push(SweepRefusal::UnknownMember {
                member: format!("levels[{i}].{k}"),
            });
        }
    }
    for k in LEVEL_REQUIRED {
        if lm.get(*k).is_none() || matches!(lm.get(*k), Some(Json::Null)) {
            out.push(SweepRefusal::MissingMember {
                member: format!("levels[{i}].{k}"),
            });
        }
    }
    let level_id = lm
        .get("level_id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    if lm.get("tier").and_then(Json::as_int) == Some(1) {
        tier1.insert(level_id.clone());
    }
    // The upstream citation: a locator or "HH-only" (REQ-XD-201).
    match lm.get("upstream_locator").and_then(Json::as_str) {
        Some("HH-only") => {}
        Some(u) if u.starts_with("upstream:") || u.starts_with("hh:") => {}
        _ => out.push(SweepRefusal::MissingUpstream {
            level_id: level_id.clone(),
        }),
    }
    // Non-upstream constants are marked: every `constants` member must
    // appear in `non_upstream_constants`.
    let marked: BTreeSet<String> = lm
        .get("non_upstream_constants")
        .and_then(as_arr)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(cm) = lm.get("constants").and_then(as_obj) {
        for k in cm.keys() {
            if !marked.contains(k) {
                out.push(SweepRefusal::UnmarkedNonUpstream {
                    detail: format!("{level_id}.constants.{k}"),
                });
            }
        }
    }
}

/// `freeze_registry(reg)` — the frozen record: the registry canonical
/// bytes with `frozen_sha256` + `exploration_identity` stamped
/// (REQ-XD-201's "frozen with its hash").
pub fn freeze_registry(reg: &Json) -> Json {
    let mut m = as_obj(reg).cloned().unwrap_or_default();
    m.remove("frozen_sha256");
    m.remove("exploration_identity");
    let frozen = Json::Obj(m.clone());
    let sha = sha256_hex(frozen.to_canonical_string().as_bytes());
    m.insert("frozen_sha256".to_string(), Json::str(&sha));
    m.insert(
        "exploration_identity".to_string(),
        Json::str(exploration_identity(&sha)),
    );
    Json::Obj(m)
}

/// The exploration identity a frozen registry binds — the freeze hash
/// itself names the identity (X1 for the first-pass freeze).
pub fn exploration_identity(frozen_sha256: &str) -> String {
    format!("x1:sha256:{frozen_sha256}")
}

/// `registry_sha256(reg)` — the freeze digest over the registry's
/// canonical bytes excluding the stamped members.
pub fn registry_sha256(reg: &Json) -> String {
    let mut m = as_obj(reg).cloned().unwrap_or_default();
    m.remove("frozen_sha256");
    m.remove("exploration_identity");
    sha256_hex(Json::Obj(m).to_canonical_string().as_bytes())
}

/// `check_merge(a_identity, b_identity)` — REQ-XD-202's merge refusal:
/// data recorded under two exploration identities never merge.
pub fn check_merge(a: &str, b: &str) -> Result<(), SweepRefusal> {
    if a == b {
        Ok(())
    } else {
        Err(SweepRefusal::MergeAcrossIdentities {
            detail: format!("{a} vs {b} — a level changed after the freeze is a new identity"),
        })
    }
}

/// `check_v5_declaration(decl)` — the sealed V5 level's declaration
/// must reproduce R6.33's canonical sub-object byte-equal (sha256
/// match, AC-3; never edit the pre-registration to fit).
pub fn check_v5_declaration(decl: &Json) -> Result<(), SweepRefusal> {
    let sha = sha256_hex(decl.to_canonical_string().as_bytes());
    if sha != V5_DECLARATION_SHA256 {
        return Err(SweepRefusal::DeclarationMismatch {
            detail: format!("sha256 {sha} != R6.33's {V5_DECLARATION_SHA256}"),
        });
    }
    Ok(())
}

// ── level materialization (REQ-XE-124) ───────────────────────────────────────

/// A level sealed through enumerate → resolve → seal.
#[derive(Debug)]
pub struct SealedLevel {
    /// The level id.
    pub level_id: String,
    /// The override spellings (attribution only — never an
    /// interpreter).
    pub spellings: Vec<String>,
    /// The sealed document.
    pub sealed: SealedDefinition,
    /// `ref_` — the sealed `version_id` (digest-frozen, never a
    /// mutable string).
    pub version_id: String,
    /// The `LevelSpec` the Lab design binds — `ref_` is the sealed
    /// `version_id`; `overrides` carries the spellings for
    /// attribution only.
    pub level_spec: LevelSpec,
}

/// `materialize_level(space, level_id, factor_id, spellings, env)` —
/// the first production consumer of `enumerate` (REQ-XE-124): each
/// OFAT perturbation is one override spelling; the point passes
/// `validate_assembly` inside `enumerate` (a failure routes to
/// `rejected`, never silently); it is then resolved and sealed and
/// its `version_id` is `LevelSpec.ref_`.
pub fn materialize_level(
    sp: &hharness_assembly::space::ParameterSpace,
    level_id: &str,
    spellings: &[&str],
    env: &mut ResolveEnv<'_>,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
) -> Result<SealedLevel, SweepRefusal> {
    let overrides: Result<Vec<Override>, SweepRefusal> = spellings
        .iter()
        .map(|s| {
            Override::parse(*s).map_err(|e| SweepRefusal::UnexpressibleSurface {
                detail: format!("{level_id}: spelling `{s}`: {e}"),
            })
        })
        .collect();
    let overrides = overrides?;
    let out = enumerate(
        sp,
        &SweepDesign::OverrideList(overrides),
        catalog,
        None,
        kernel,
    );
    let Some(point) = out.points.into_iter().next() else {
        let why = out
            .rejected
            .first()
            .map(|r| {
                r.validation
                    .diagnostics
                    .first()
                    .map(|d| format!("{:?}: {}", d.code, d.subject))
                    .unwrap_or_else(|| "rejected".to_string())
            })
            .unwrap_or_else(|| "no point".to_string());
        return Err(SweepRefusal::UnexpressibleSurface {
            detail: format!("{level_id}: the declared params cannot express the delta ({why})"),
        });
    };
    let sealed =
        resolve(&point.document, env).map_err(|diags| SweepRefusal::UnexpressibleSurface {
            detail: format!(
                "{level_id}: resolve refused: {}",
                diags
                    .iter()
                    .map(|d| format!("{:?}", d.code))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        })?;
    let version_id = sealed.definition_ref.version_id.clone();
    let level_spec = LevelSpec {
        level_id: level_id.to_string(),
        ref_: version_id.clone(),
        overrides: Some(Json::Arr(
            point.overrides.iter().map(|s| Json::str(s)).collect(),
        )),
        label: SWEEP_VARIANT_LABEL.to_string(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    Ok(SealedLevel {
        level_id: level_id.to_string(),
        spellings: point.overrides.clone(),
        sealed,
        version_id,
        level_spec,
    })
}

/// How a level is opened — by its sealed ref or by override spelling.
#[derive(Debug, Clone, PartialEq)]
pub enum LevelOpen {
    /// `ByRef(version_id)` — the only admitted opening.
    ByRef(String),
    /// `ByOverrides(spellings)` — attribution is never an interpreter.
    ByOverrides(Vec<String>),
}

/// `check_level_open(spec, opened)` — the REQ-XE-124 identity check:
/// a level opened by override instead of by ref refuses typed; a ref
/// other than the sealed `version_id` refuses.
pub fn check_level_open(spec: &LevelSpec, opened: &LevelOpen) -> Result<(), SweepRefusal> {
    match opened {
        LevelOpen::ByRef(r) if *r == spec.ref_ => Ok(()),
        LevelOpen::ByRef(r) => Err(SweepRefusal::UndeclaredDelta {
            path: format!("ref {r} != sealed {}", spec.ref_),
        }),
        LevelOpen::ByOverrides(_) => Err(SweepRefusal::OpenedByOverride {
            level_id: spec.level_id.clone(),
        }),
    }
}

// ── the declared-delta and liveness check (REQ-XE-125) ───────────────────────

/// `assembly_delta(base_doc, level_doc)` — the changed leaf paths
/// under `/assembly` between the baseline and the variant (the
/// doc-level delta REQ-XE-125's "exactly one registered slot" reads).
pub fn assembly_delta(base: &Json, level: &Json) -> Vec<String> {
    let mut out = Vec::new();
    json_diff(base, level, "/assembly", &mut out);
    out
}

/// Recursive leaf diff — every path whose leaf value differs.
pub fn json_diff(a: &Json, b: &Json, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Json::Obj(ma), Json::Obj(mb)) => {
            for (k, v) in ma {
                match mb.get(k) {
                    Some(vb) => json_diff(v, vb, &format!("{path}/{k}"), out),
                    None => out.push(format!("{path}/{k}")),
                }
            }
            for (k, _) in mb {
                if !ma.contains_key(k) {
                    out.push(format!("{path}/{k}"));
                }
            }
        }
        (Json::Arr(xa), Json::Arr(xb)) if xa.len() == xb.len() => {
            for (i, (x, y)) in xa.iter().zip(xb.iter()).enumerate() {
                json_diff(x, y, &format!("{path}[{i}]"), out);
            }
        }
        _ => {
            if a != b {
                out.push(path.to_string());
            }
        }
    }
}

/// Normalize a delta path — drop the `SlotBindings` wrapper segment
/// (`one` / `many[…]`) so a declared typed path compares member-for-
/// member.
fn norm_delta(p: &str) -> String {
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    // `slots/<class>/<one|many[i]>/<field>/…` — the binding-wrapper
    // segment sits at index 2.
    let out: Vec<&str> = segs
        .iter()
        .enumerate()
        .filter(|(i, seg)| {
            !(*i == 2 && segs[0] == "slots" && (**seg == "one" || seg.starts_with("many[")))
        })
        .map(|(_, s)| *s)
        .collect();
    out.join("/")
}

/// Normalize a declared typed path — `.` is the member-descent
/// operator (a param name's nested keys, e.g.
/// `transcript_render.assistant_replay.reasoning`).
fn norm_declared(d: &str) -> String {
    d.replace('.', "/")
}

/// `check_doc_delta(declared_typed_paths, delta)` — exactly one
/// registered slot differs and every changed path is inside the
/// declared typed paths (REQ-XE-125's doc-level confinement).
pub fn check_doc_delta(
    declared_typed_paths: &[String],
    delta: &[String],
) -> Result<(), SweepRefusal> {
    let declared: Vec<String> = declared_typed_paths
        .iter()
        .map(|d| norm_declared(d))
        .collect();
    let mut slots: BTreeSet<String> = BTreeSet::new();
    for p in delta {
        // `resolved` bookkeeping rides with the re-resolve — never a
        // declared delta member.
        if p.starts_with("/assembly/resolved") {
            continue;
        }
        let rel = p
            .strip_prefix("/assembly/")
            .map(norm_delta)
            .unwrap_or_else(|| p.clone());
        let covered = declared
            .iter()
            .any(|d| rel.starts_with(d.as_str()) || d.starts_with(rel.as_str()));
        if !covered {
            return Err(SweepRefusal::UndeclaredDelta { path: p.clone() });
        }
        if let Some(rest) = rel.strip_prefix("slots/") {
            slots.insert(rest.split('/').next().unwrap_or("").to_string());
        }
    }
    if slots.len() > 1 {
        return Err(SweepRefusal::UndeclaredDelta {
            path: format!("{} slots differ — exactly one registered slot", slots.len()),
        });
    }
    Ok(())
}

/// A wire diff path pattern match — `*` is a single-segment
/// wildcard, `[]` indexes match any element.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let norm = |s: &str| s.replace('[', "/[").replace("[]", "/*");
    let np = norm(pattern);
    let p: Vec<&str> = np.split('/').filter(|s| !s.is_empty()).collect();
    let nb = norm(path);
    let a: Vec<&str> = nb.split('/').filter(|s| !s.is_empty()).collect();
    p.len() == a.len()
        && p.iter()
            .zip(a.iter())
            .all(|(pp, aa)| *pp == "*" || pp == aa)
}

/// `request_diff(baseline_bodies, variant_bodies)` — the per-request
/// leaf diff between the canonical upstream-bound request bodies the
/// L1.1 drives captured (the §8.2 declared-delta input).
pub fn request_diff(baseline: &[Vec<u8>], variant: &[Vec<u8>]) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    for (i, (ba, va)) in baseline.iter().zip(variant.iter()).enumerate() {
        let (Ok(pa), Ok(pb)) = (
            parse(&String::from_utf8_lossy(ba)),
            parse(&String::from_utf8_lossy(va)),
        ) else {
            if ba != va {
                out.push((i + 1, vec!["bytes".to_string()]));
            }
            continue;
        };
        let mut paths = Vec::new();
        json_diff(&pa, &pb, "", &mut paths);
        if !paths.is_empty() {
            out.push((i + 1, paths));
        }
    }
    if baseline.len() != variant.len() {
        out.push((
            baseline.len().max(variant.len()),
            vec!["request_count".to_string()],
        ));
    }
    out
}

/// `check_request_diff(liveness, factor_id, diff)` — the wire-side
/// leg: every observed diff path confined to the declared paths, and
/// the diff non-empty on the exercising branch — else `inert`.
pub fn check_request_diff(
    liveness: &LivenessDecl,
    factor_id: &str,
    diff: &[(usize, Vec<String>)],
) -> Result<(), SweepRefusal> {
    for (_, paths) in diff {
        for p in paths {
            if !liveness
                .declared_request_paths
                .iter()
                .any(|d| path_matches(d, p))
            {
                return Err(SweepRefusal::UndeclaredDelta { path: p.clone() });
            }
        }
    }
    let live = diff
        .iter()
        .any(|(turn, paths)| liveness.exercising_branch.contains(*turn) && !paths.is_empty());
    if !live {
        return Err(SweepRefusal::Inert {
            factor_id: factor_id.to_string(),
            branch: liveness.exercising_branch.as_str(),
        });
    }
    Ok(())
}

/// `check_executed_leg(baseline_rc, level_rc)` — the executor-level
/// leg (REQ-XE-125): `cmd-120` distinguishes rc −1 (the baseline's
/// declared timeout kill) from rc 0 (the level's window).
pub fn check_executed_leg(
    factor_id: &str,
    baseline_rc: i64,
    level_rc: i64,
) -> Result<(), SweepRefusal> {
    if baseline_rc == -1 && level_rc == 0 {
        Ok(())
    } else {
        Err(SweepRefusal::Inert {
            factor_id: factor_id.to_string(),
            branch: format!("executed_leg:baseline_rc={baseline_rc},level_rc={level_rc}"),
        })
    }
}

/// The liveness evidence a level's check consumes — the kind the
/// level's `liveness.kind` declares.
#[derive(Debug)]
pub enum LivenessEvidence<'a> {
    /// The L1.1 request diff (renderer/lane knobs).
    RequestDiff(&'a [(usize, Vec<String>)]),
    /// The executed scripted-model leg's return codes.
    ExecutedLeg {
        /// The baseline's rc (the declared timeout must kill: −1).
        baseline_rc: i64,
        /// The level's rc (the widened window completes: 0).
        level_rc: i64,
    },
    /// No evidence produced — a declarable level with no live
    /// admission path refuses typed at arm time.
    Absent,
}

/// `check_liveness(decl, factor_id, evidence)` — the REQ-XE-125 gate
/// dispatcher: the evidence kind must match the level's declared
/// liveness kind (a level declarable but lacking its admission path
/// refuses `no_admission_path`, never silently admitted).
pub fn check_liveness(
    decl: &LivenessDecl,
    factor_id: &str,
    level_id: &str,
    evidence: &LivenessEvidence<'_>,
) -> Result<(), SweepRefusal> {
    match (decl.kind, evidence) {
        (LivenessKind::RequestDiff, LivenessEvidence::RequestDiff(d)) => {
            check_request_diff(decl, factor_id, d)
        }
        (
            LivenessKind::ExecutorLeg,
            LivenessEvidence::ExecutedLeg {
                baseline_rc,
                level_rc,
            },
        ) => check_executed_leg(factor_id, *baseline_rc, *level_rc),
        (LivenessKind::RequestDiff, LivenessEvidence::ExecutedLeg { .. })
        | (LivenessKind::ExecutorLeg, LivenessEvidence::RequestDiff(_)) => {
            Err(SweepRefusal::NoAdmissionPath {
                detail: format!(
                    "{level_id}: evidence kind does not match the declared liveness kind"
                ),
            })
        }
        (_, LivenessEvidence::Absent) => Err(SweepRefusal::NoAdmissionPath {
            detail: format!("{level_id}: no liveness evidence at arm time"),
        }),
    }
}

// ── the declared sweep baseline + fixture store ──────────────────────────────

/// The declared sweep-baseline document — a minimal mini-shaped
/// authored document carrying the first-pass factors' declared
/// parameter (`reasoning_replay`, sweepable, feeding
/// `slots/model_port/params/transcript_render.assistant_replay
/// .reasoning` through the `$param:` binding). The production mini
/// document lands under E2-T9; this is the declared sweep fixture,
/// honest as such — never a stand-in claim.
pub fn sweep_baseline_doc() -> HirDocument {
    let prov = |seq: u64| {
        ProvenanceRecord::minted(
            Origin::human("sweep:author", HumanRole::Author),
            PersistenceScope::Definition,
            seq,
        )
    };
    let sel = |sid: &str| Ref::selected(sid, "latest");
    let sid = |mut n: Node, id: &str| {
        n.version.semantic_id = Some(id.to_string());
        n
    };
    let mut doc = HirDocument::new(sel("sweep:agent"));
    doc.nodes.push(sid(
        Node::new(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "sweep:rule.rule".to_string(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            prov(1),
        ),
        "sweep:rule",
    ));
    doc.nodes.push(sid(
        Node::new(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1_000_000),
                        soft: None,
                    },
                )]),
                scope: "*".to_string(),
                parent: None,
                accounting: sel("sweep:rule"),
            }),
            prov(2),
        ),
        "sweep:budget",
    ));
    doc.nodes.push(sid(
        Node::new(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("sweep:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "sweep:issuer".to_string(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            prov(3),
        ),
        "sweep:perm",
    ));
    // The assembly section — the declared parameter space (never
    // inferred): `reasoning_replay` sweepable ∈ {verbatim, drop},
    // feeding the model_port slot's transcript_render leaf.
    let mut a = Assembly::empty();
    a.parameters.insert(
        "reasoning_replay".to_string(),
        ParameterSpec {
            param_type: ParamType::Str,
            domain: Some(Json::Arr(vec![Json::str("verbatim"), Json::str("drop")])),
            default: Some(Json::str("verbatim")),
            required: ParamRequirement::Optional,
            unit: None,
            sweepable: true,
            affects: vec![],
            budget_relevant: false,
        },
    );
    a.values
        .insert("reasoning_replay".to_string(), Json::str("verbatim"));
    let mut mp = SlotBinding::of(ComponentVariantRef::selected(
        "model_port",
        "hh/openai-chat",
        "latest",
    ));
    mp.params.insert(
        "transcript_render".to_string(),
        Json::obj([(
            "assistant_replay",
            Json::obj([("reasoning", Json::str("$param:reasoning_replay"))]),
        )]),
    );
    a.slots
        .insert("model_port".to_string(), SlotBindings::One(mp));
    for class_id in ["control_strategy", "context_policy"] {
        let (variant, _): (&str, ()) = (
            match class_id {
                "control_strategy" => "hh/round_robin",
                _ => "hh/full_window",
            },
            (),
        );
        a.slots.insert(
            class_id.to_string(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
                class_id, variant, "latest",
            ))),
        );
    }
    doc.nodes.push(sid(
        Node::new(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("sweep:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".to_string(),
                        pinned: true,
                    },
                    slots: a.slots.clone(),
                    control_boundary: Default::default(),
                    budget: sel("sweep:budget"),
                    permissions: sel("sweep:perm"),
                    environment: EnvironmentRef {
                        environment: "env:sweep".to_string(),
                    },
                }),
            }),
            prov(4),
        ),
        "sweep:agent",
    ));
    doc.assembly = Some(a.to_json());
    doc
}

/// The seeded sweep store — the declared class records and one
/// variant per bound class (`model_port` → `hh/openai-chat`,
/// `control_strategy` → `hh/round_robin`, `context_policy` →
/// `hh/full_window`), published deterministically. Fixture-scope,
/// `provenance: synthetic` in spirit — declared, never live.
pub fn sweep_seeded_store(dir: &Path) -> RegistryStore {
    let kernel = ProvenanceRecord::kernel("sweep:registry", 0);
    let prov = |seq: u64| {
        ProvenanceRecord::minted(
            Origin::human("sweep:author", HumanRole::Author),
            PersistenceScope::Definition,
            seq,
        )
    };
    let mut store = RegistryStore::open(dir, &kernel).expect("sweep store opens");
    for (class_id, name) in [
        ("model_port", "openai-chat"),
        ("control_strategy", "round_robin"),
        ("context_policy", "full_window"),
    ] {
        let c = store
            .register(
                RegistryRecord::Class(sweep_class_record(class_id)),
                &kernel,
                None,
            )
            .expect("class registers");
        let v = store
            .register(
                RegistryRecord::Variant(sweep_variant_record(&c.version_id, &format!("hh/{name}"))),
                &kernel,
                None,
            )
            .expect("variant registers");
        store
            .publish("hh", name, &v.version_id, None, None, &kernel)
            .expect("publish");
        let _ = prov;
    }
    store
}

fn sweep_class_record(id: &str) -> ClassRecord {
    ClassRecord {
        class_id: id.to_string(),
        contract: vec![ContractOperation {
            name: "run".to_string(),
            inputs: Json::Null,
            outputs: Json::Null,
            invariants: vec!["i".to_string()],
            failure_modes: vec!["refuse".to_string()],
        }],
        cardinality: Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            ("additionalProperties", Json::Bool(false)),
            ("properties", Json::obj([("deterministic", Json::Null)])),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec![],
        slot_key: id.to_string(),
        tier: "C0".to_string(),
        depends_on: vec![],
    }
}

fn sweep_variant_record(class_ref: &str, variant_id: &str) -> VariantRecord {
    VariantRecord {
        variant_id: variant_id.to_string(),
        class_ref: class_ref.to_string(),
        contract_range: "1.0".to_string(),
        version_label: Some("1.0.0".to_string()),
        param_schema: BTreeMap::new(),
        implementation: Implementation {
            content: hharness_identity::idp::address(
                format!("impl-{variant_id}").as_bytes(),
                "application/octet-stream",
            ),
            placement: Placement::InProcess,
            host_requirements: Json::Null,
        },
        capability_declaration: BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]),
        conditioned_rules: vec![],
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: Text::new(
            format!("v {variant_id}"),
            "sweep:owner",
            ProvenanceRecord::minted(
                Origin::human("sweep:author", HumanRole::Author),
                PersistenceScope::Definition,
                0,
            ),
        ),
        dialect_range: "registry/1".to_string(),
    }
}

/// `materialize_v5(work_dir)` — the one sealed level of the first
/// pass: V5 `rsn-drop` over the declared sweep baseline, through the
/// REQ-XE-124 chain. Returns the baseline's `version_id`, the level's
/// sealed definition (canonical JSON), the `LevelSpec` and the
/// recorded spelling. Deterministic — the seeded store, the authored
/// baseline and the `resolved_at` stamp are fixed.
pub fn materialize_v5(work_dir: &Path) -> Result<V5Materialization, SweepRefusal> {
    let kernel = ProvenanceRecord::kernel("sweep:seal", 0);
    let mut store = sweep_seeded_store(&work_dir.join("registry"));
    let authored = sweep_baseline_doc();
    let catalog = Stage1Catalog::stage1();
    let mut env = ResolveEnv {
        registry: &mut store,
        catalog: &catalog,
        snapshot_id: None,
        mode: ResolveMode::Audit,
        registrar: kernel.clone(),
        resolved_at: 1_763_000_000_000,
        notices: None,
    };
    // The frozen baseline — resolve + seal once.
    let base = resolve(&authored, &mut env).map_err(|d| SweepRefusal::UnexpressibleSurface {
        detail: format!("baseline resolve refused: {}", d.len()),
    })?;
    let baseline_version_id = base.definition_ref.version_id.clone();
    // The declared space is read off the *authored* document — the
    // `$param:` markers still bind the sweepable parameter into the
    // slot params (a resolved doc's markers are consumed).
    let sp = space(&authored, &kernel).map_err(|e| SweepRefusal::UnexpressibleSurface {
        detail: format!("space: {e}"),
    })?;
    let level = materialize_level(
        &sp,
        "V5",
        &["reasoning_replay=drop"],
        &mut env,
        &catalog,
        &kernel,
    )?;
    Ok(V5Materialization {
        baseline_version_id,
        level_version_id: level.version_id.clone(),
        sealed_level_json: hharness_hir::wire::sealed_definition_json(&level.sealed),
        spellings: level.spellings.clone(),
        level_spec: level.level_spec,
        baseline_json: hharness_hir::wire::sealed_definition_json(&base),
    })
}

/// The V5 materialization result.
#[derive(Debug)]
pub struct V5Materialization {
    /// The frozen baseline's `version_id`.
    pub baseline_version_id: String,
    /// The sealed V5 level's `version_id` — `LevelSpec.ref_`.
    pub level_version_id: String,
    /// The sealed level document (canonical JSON).
    pub sealed_level_json: Json,
    /// The recorded override spellings (attribution only).
    pub spellings: Vec<String>,
    /// The `LevelSpec` (ref_ = the sealed version_id).
    pub level_spec: LevelSpec,
    /// The sealed baseline document (canonical JSON).
    pub baseline_json: Json,
}

/// `sealed_definition_json` — the canonical export spelling.
pub fn sealed_json(s: &SealedDefinition) -> Json {
    hharness_hir::wire::sealed_definition_json(s)
}

/// The V5 liveness declaration — the §8.1 row: request-diff kind,
/// `reasoning_content` the declared wire path, always after turn 1.
pub fn v5_liveness() -> LivenessDecl {
    LivenessDecl {
        kind: LivenessKind::RequestDiff,
        declared_request_paths: vec!["messages/*/reasoning_content".to_string()],
        exercising_branch: Branch::AfterTurn(1),
    }
}

/// The `cmd-120` executor-level liveness declaration — the executed
/// leg's evidence kind.
pub fn v3_liveness() -> LivenessDecl {
    LivenessDecl {
        kind: LivenessKind::ExecutorLeg,
        declared_request_paths: Vec::new(),
        exercising_branch: Branch::Conditional,
    }
}

/// The V5 `sweep-registry/1` level entry (the one sealed level the
/// first pass declares).
pub fn v5_level_entry(sealed_ref: &str, declaration: &Json, declaration_sha256: &str) -> Json {
    Json::obj([
        ("level_id", Json::str("V5")),
        ("level_name", Json::str("rsn-drop")),
        (
            "factor_id",
            Json::str("transcript_render.assistant_replay.reasoning"),
        ),
        (
            "typed_paths",
            Json::Arr(vec![
                Json::str("slots/model_port/params/transcript_render.assistant_replay.reasoning"),
                Json::str("values/reasoning_replay"),
            ]),
        ),
        ("baseline", Json::str("verbatim")),
        ("level", Json::str("drop")),
        ("tier", Json::Int(1)),
        (
            "exercising_condition",
            Json::str("always after turn 1 (reasoning always present)"),
        ),
        (
            "activation_metric",
            Json::str("share of requests with a dropped non-empty reasoning member"),
        ),
        (
            "forecast_source",
            Json::str("non-outcome fields: reasoning presence on upstream-bound requests"),
        ),
        ("cost_prior", Json::str("↓ input tokens")),
        ("dependency_tickets", Json::Arr(vec![Json::str("E2-T1")])),
        ("upstream_locator", Json::str("HH-only")),
        ("non_upstream_constants", Json::Arr(Vec::new())),
        ("sealed_ref", Json::str(sealed_ref)),
        (
            "override_spellings",
            Json::Arr(vec![Json::str("reasoning_replay=drop")]),
        ),
        (
            "liveness",
            Json::obj([
                ("kind", Json::str("request_diff")),
                (
                    "declared_request_paths",
                    Json::Arr(vec![Json::str("messages/*/reasoning_content")]),
                ),
                (
                    "exercising_branch",
                    Json::obj([("kind", Json::str("after_turn")), ("turn", Json::Int(1))]),
                ),
            ]),
        ),
        ("declaration", declaration.clone()),
        ("declaration_sha256", Json::str(declaration_sha256)),
    ])
}

/// Build the `sweep-registry/1` record — Tier 1 V1–V9 of §8.1 and the
/// Tier-2 candidates each with its reason; V5 carries `sealed_ref`.
/// `v5` is the V5 level entry (see `v5_level_entry`).
pub fn sweep_registry_record(v5: Json) -> Json {
    let lvl = |id: &str,
               name: &str,
               factor: &str,
               typed: &[&str],
               baseline: &str,
               level: &str,
               exercising: &str,
               metric: &str,
               forecast: &str,
               cost: &str,
               deps: &[&str],
               upstream: &str,
               non_up: &[&str]| {
        Json::obj([
            ("level_id", Json::str(id)),
            ("level_name", Json::str(name)),
            ("factor_id", Json::str(factor)),
            (
                "typed_paths",
                Json::Arr(typed.iter().map(|s| Json::str(*s)).collect()),
            ),
            ("baseline", Json::str(baseline)),
            ("level", Json::str(level)),
            ("tier", Json::Int(1)),
            ("exercising_condition", Json::str(exercising)),
            ("activation_metric", Json::str(metric)),
            ("forecast_source", Json::str(forecast)),
            ("cost_prior", Json::str(cost)),
            (
                "dependency_tickets",
                Json::Arr(deps.iter().map(|s| Json::str(*s)).collect()),
            ),
            ("upstream_locator", Json::str(upstream)),
            (
                "non_upstream_constants",
                Json::Arr(non_up.iter().map(|s| Json::str(*s)).collect()),
            ),
        ])
    };
    Json::obj([
        ("schema", Json::str(SWEEP_REGISTRY_SCHEMA)),
        (
            "label",
            Json::str(SWEEP_VARIANT_LABEL),
        ),
        (
            "parity",
            Json::str("none — a swept variant carries no parity claim against the original (REQ-XE-125)"),
        ),
        (
            "note",
            Json::str("first-pass scope (FP-6): V1–V9 + Tier 2 declared; only V5 is sealed — exploration, selection and the other seals are later passes"),
        ),
        ("recorded_at", Json::str("2026-11-11")),
        (
            "baseline",
            Json::obj([
                ("document", Json::str("declared sweep baseline (fixture)")),
                ("note", Json::str("the first-pass mini HIR document is E2-T9's unlanded Stage 2 — the registry's levels declare the typed paths; V5 seals over this declared baseline")),
            ]),
        ),
        (
            "levels",
            Json::Arr(vec![
                lvl("V1", "eff-low", "reasoning effort · declared lane parameter, proxy pin",
                    &["lane/effort_pin"], "high", "low",
                    "always", "fidelity: applied = declared on 100% of calls",
                    "non-outcome fields: per-call reasoning tokens",
                    "↓", &["proxy pin", "effort home (Q-X44 / AR-26)"], "upstream:hf-mini-swe-agent",
                    &[]),
                lvl("V2", "eff-max", "reasoning effort · declared lane parameter, proxy pin",
                    &["lane/effort_pin"], "high", "max",
                    "always", "fidelity: applied = declared on 100% of calls",
                    "non-outcome fields: per-call reasoning tokens",
                    "↑ ≈ ×1.3 per trial", &["proxy pin", "effort home (Q-X44 / AR-26)"],
                    "upstream:hf-mini-swe-agent", &[]),
                lvl("V3", "cmd-120", "exec timeout exec_defaults.timeout_ms",
                    &["ToolCapability.exec_defaults.timeout_ms"], "30000", "120000",
                    "conditional: ≥ 1 command > 30 s",
                    "share of first subjects with ≥ 1 command > 30 s",
                    "non-outcome fields: per-command durations",
                    "wall ↑", &["E2-T6"], "HH-only", &[]),
                lvl("V4", "obs-2.5k", "observation renderer json_head_tail@1",
                    &["result_renderer.threshold", "result_renderer.head", "result_renderer.tail"],
                    "10000/5000/5000", "2500/1250/1250",
                    "conditional: counterfactual rendering difference",
                    "exact counterfactual rendering difference",
                    "non-outcome fields: raw observation lengths",
                    "↓ input tokens", &["E2-T4"], "HH-only", &[]),
                v5,
                lvl("V6", "prm-wf", "instance template alternative leaf",
                    &["Text:instance_template"], "verbatim",
                    "workflow guidance removed, submit instruction and command rules kept",
                    "always (wire-visible)", "wire-visible",
                    "non-outcome fields: request system/user lengths",
                    "≈ 0", &["E2-T3"], "HH-only", &[]),
                lvl("V7", "tool-edit", "tool surface",
                    &["tools[]", "description_template"], "bash only", "bash + edit_file",
                    "wire-visible; behavioural ≥ 1 edit_file call",
                    "share of first subjects with ≥ 1 edit_file call",
                    "non-outcome fields: request tool schema",
                    "small ↑ (schema tokens)", &["edit_file capability (Q-X44: withheld unless funded)"],
                    "HH-only", &[]),
                lvl("V8", "cmp-ctr@W", "compaction_strategy + declared window_cap",
                    &["slots/compaction_strategy", "window_cap"], "none @ catalog window",
                    "clear_tool_results @ W", "conditional under W",
                    "§4.2 activation counts under W",
                    "non-outcome fields: per-call upstream input tokens",
                    "↓ on long trajectories", &["E2-T7"], "HH-only", &[]),
                lvl("V9", "cmp-sum@W", "compaction_strategy + declared window_cap + summarizer profile",
                    &["slots/compaction_strategy", "window_cap"], "none",
                    "hh/summarize-rolling@1 @ W", "conditional under W",
                    "§4.2 activation counts under W, plus compaction-role call overhead",
                    "non-outcome fields: per-call upstream input tokens",
                    "±", &["E2-T7", "summarizer profile on the proxy lane"], "HH-only", &[]),
            ]),
        ),
        (
            "tier2",
            Json::Arr(vec![
                t2("step-limit", "Budget.dimensions.model_calls.hard ∞ → 50",
                    "HF mean is 31.8 calls/trial — a cap of 50 binds only the tail; mostly a cost lever",
                    "a cost-oriented later exploration, once an admitted cost method exists"),
                t2("format-error-cap", "format-error cap 3 → 1 / 5",
                    "exercised only by streaks, probably rare with native tool calls",
                    "admit if the phase-2 pilot's mini receipts show a streak rate ≥ the activation floor"),
                t2("truncation-off", "observation truncation off / 20 000",
                    "exercised only by outputs > 10 000 chars — rarer than > 2 500",
                    "as above, using the observation-length census"),
                t2("context-policy", "context policy (recency_plus_pins, priority_eviction)",
                    "probably inert on the wire today — the transcript reads the compaction rows, not the assembled plan (E2 F13)",
                    "after G21 lands; the REQ-XE-125 liveness check must pass"),
                t2("interaction-mode-shim", "interaction mode shim (text actions)",
                    "needs G14, part of Terminus-2 stage two (E2-T12; Q-X42)",
                    "after the stage-two block"),
                t2("explicit-temperature", "explicit temperature",
                    "baseline sends none; provider default and honoured-under-thinking unknown; explicit 1.0 may be distributionally inert while wire-visible",
                    "after the S0 smoke (Q-X23) records acceptance and a stated rationale"),
                t2("cost-limit", "cost limit (Budget.spend.hard)",
                    "duplicates the proxy's per-trial ceiling",
                    "not proposed"),
            ]),
        ),
    ])
}

fn t2(id: &str, factor: &str, reason: &str, route: &str) -> Json {
    Json::obj([
        ("level_id", Json::str(id)),
        ("factor_id", Json::str(factor)),
        ("reason", Json::str(reason)),
        ("admission_route", Json::str(route)),
        ("upstream_locator", Json::str("HH-only")),
        ("tier", Json::Int(2)),
    ])
}
