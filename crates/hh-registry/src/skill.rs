//! `lift_skill` — ADR-0086's lift direction (R-2.4.5⁰, C0/Stage-2):
//! `lift_skill(ExtensionRecord{kind: skill}, SkillTree) → {Procedure, Text[],
//! Artifact[], CompiledPayload[], LoweringLossReport}`.
//!
//! The mapping (ADR-0086 D1):
//!
//! - `name` → the profile `index`/`source` back-reference + the procedure
//!   surface (`semantic_id` is the identity — `NameCollision` is `resolve`'s
//!   job, never last-wins);
//! - `description` → `index.description` (a `Text` at the extension's
//!   `text_authority`, default `external` — R-TEXT);
//! - the body → **one** `Instruction(Text)` step;
//! - `allowed-tools` → `declared_claims` rows (`AllowedTools`) on the lift
//!   result — **claims, never grants**: `allowed_capabilities` stays empty and
//!   only an `authorizes` edge at `seal` can cover an invoked capability
//!   (otherwise `EffectUncovered`);
//! - `compatibility` → `applicability_notes`; a typed `requires_env` member →
//!   `env_requires` predicates;
//! - `scripts/*` → `CompiledPayload{declared_interface{effects:{exec}}}`
//!   behind `Opaque` steps (`subprocess_confined` target);
//! - `references/*`/`assets/*` → content-addressed `Artifact` nodes
//!   (`produced_by` the procedure — read through `fs_read` scoped to the
//!   extension's content, never a workspace widening);
//! - vendor `PathTrigger`s → `path_glob` `Predicate`s + `retrieval_hints`;
//! - `metadata` keys → `ext["<ns>/metadata"]`; `metadata.version` → a
//!   `version_label` claim (CF-087);
//! - render purity is `validate`'s gate: a body carrying the declared
//!   render-time exec marker (`` !` ``<cmd>`` ` ``) fails
//!   [`hh_hir::validate`] with `RenderTimeExecution` — the lift produces the
//!   doc faithfully, the gate refuses it.
//!
//! The registry never performs I/O: the caller supplies the read tree
//! (`SkillTree`) — CC5.

use std::collections::BTreeMap;

use hh_hir::document::Node;
use hh_hir::kinds::{EffectClass, EffectDomain, EntityKind};
use hh_hir::leaves::{CompiledPayload, DeclaredInterface, Text};
use hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY;
use hh_hir::records::{
    ArtifactRecord, CompileHint, KindRecord, ProcedureRecord, ProcedureStep, ProcedureSurface,
    SurfaceRecord,
};
use hh_hir::refs::Ref;
use hh_provenance::authority::PersistenceScope;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

use crate::extension::{DeclaredClaim, DeclaredClaimKind, ExtensionKind, ExtensionRecord};

/// A file inside the skill tree (`scripts/`, `references/`, `assets/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    /// The tree-relative path (`scripts/run.sh`).
    pub path: String,
    /// The file bytes.
    pub bytes: Vec<u8>,
    /// The media type (the caller's sniff — `text/x-shellscript` etc.).
    pub media_type: String,
}

/// A typed vendor trigger field (lifted where typed — ADR-0086 D1): the Agent
/// Skills `PathTrigger` form becomes a `path_glob` `Predicate`; keyword/task
/// triggers become `index.retrieval_hints` (never evaluated by the kernel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillTrigger {
    /// A path-glob trigger (`path_glob` predicate).
    PathGlob {
        /// The glob.
        pattern: String,
    },
    /// A keyword trigger (retrieval hint only).
    Keyword {
        /// The term.
        term: String,
    },
    /// A task-description trigger (retrieval hint only).
    Task {
        /// The description.
        description: String,
    },
}

/// The read skill tree — `SKILL.md` split into the typed members plus the
/// bundled directories. The caller performs the I/O (CC5).
#[derive(Debug, Clone, Default)]
pub struct SkillTree {
    /// `name`.
    pub name: String,
    /// `description`.
    pub description: String,
    /// `license`.
    pub license: Option<String>,
    /// `compatibility` (an applicability note; a `requires_env:[…]` member
    /// additionally lifts to `env_requires` predicates).
    pub compatibility: Option<String>,
    /// `requires_env` — typed environment requirements (lifted to
    /// `env_requires` predicates).
    pub requires_env: Vec<String>,
    /// `metadata` (string→string; `version` becomes a `version_label` claim).
    pub metadata: BTreeMap<String, String>,
    /// `allowed-tools` — claims only (never `allowed_capabilities`).
    pub allowed_tools: Vec<String>,
    /// The body markdown — the one `Instruction(Text)` step.
    pub body: String,
    /// `scripts/*`.
    pub scripts: Vec<SkillFile>,
    /// `references/*`.
    pub references: Vec<SkillFile>,
    /// `assets/*`.
    pub assets: Vec<SkillFile>,
    /// Typed vendor trigger fields.
    pub triggers: Vec<SkillTrigger>,
    /// The tree path inside the extension payload.
    pub tree_path: Option<String>,
}

/// The declared `LoweringLossReport` (ADR-0086/0021): every carried member is
/// `mapped`, every dropped member is `lost` with a reason — nothing vanishes
/// silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoweringLossReport {
    /// `field → where it landed`.
    pub mapped: Vec<String>,
    /// `(field, reason)` — declared losses.
    pub lost: Vec<(String, String)>,
    /// Notes (e.g. an authority cap applied at lift).
    pub notes: Vec<String>,
}

/// The `lift_skill` output — the members the caller composes into a
/// definition document.
#[derive(Debug)]
pub struct SkillLift {
    /// The `Procedure` node (one `Instruction` step + an `Opaque` step per
    /// script; `allowed_capabilities` empty until `seal`).
    pub procedure: Node,
    /// The `Artifact` nodes for `references/*`/`assets/*`.
    pub artifacts: Vec<Node>,
    /// The `CompiledPayload`s the `Opaque` steps carry (also inside the
    /// procedure — returned for accounting).
    pub payloads: Vec<CompiledPayload>,
    /// The `Text` leaves the lift produced (description, body, notes — also
    /// inside the members).
    pub texts: Vec<Text>,
    /// The claims (`allowed-tools`, `version_label`) — claims, never grants.
    pub declared_claims: Vec<DeclaredClaim>,
    /// The loss report.
    pub loss: LoweringLossReport,
}

/// `lift_skill` failures.
#[derive(Debug, Clone, PartialEq)]
pub enum LiftError {
    /// The extension is not `kind: skill`.
    WrongKind {
        /// The actual kind spelling.
        kind: String,
    },
}

impl std::fmt::Display for LiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiftError::WrongKind { kind } => write!(f, "LiftError::WrongKind: {kind}"),
        }
    }
}
impl std::error::Error for LiftError {}

/// `lift_skill(ext, tree, at)` — ADR-0086 D1. `at` is the caller's clock (the
/// lift is pure — same inputs, same output).
pub fn lift_skill(
    ext: &ExtensionRecord,
    tree: &SkillTree,
    at: u64,
) -> Result<SkillLift, LiftError> {
    if ext.kind != ExtensionKind::Skill {
        return Err(LiftError::WrongKind {
            kind: ext.kind.as_str().to_string(),
        });
    }
    let mut loss = LoweringLossReport::default();
    let ext_origin = Origin::Tool {
        capability: format!("extension:{}", ext.name),
        invocation_ref: ext.content.id(),
        inner_source: None,
    };
    let prov = || ProvenanceRecord::minted(ext_origin.clone(), PersistenceScope::Run, at);
    let text_authority = ext.trust.text_authority;
    if text_authority > hh_provenance::authority::AuthorityClass::External {
        loss.notes.push(format!(
            "text_authority {ta} capped at external at lift — re-endorse at seal",
            ta = text_authority.as_str()
        ));
    }

    // ── leaves ───────────────────────────────────────────────────────────
    let mut texts = Vec::new();
    let description = Text::new(tree.description.clone(), ext.name.clone(), prov());
    let body = Text::new(tree.body.clone(), ext.name.clone(), prov());
    texts.push(description.clone());
    texts.push(body.clone());

    // ── preconditions (typed vendor fields) ──────────────────────────────
    let mut preconditions: Vec<Json> = Vec::new();
    let mut retrieval_hints: Vec<Json> = Vec::new();
    for t in &tree.triggers {
        match t {
            SkillTrigger::PathGlob { pattern } => {
                preconditions.push(Json::obj([
                    ("kind", Json::str("path_glob")),
                    ("pattern", Json::str(pattern.clone())),
                ]));
                retrieval_hints.push(Json::str(format!("path:{pattern}")));
            }
            SkillTrigger::Keyword { term } => {
                retrieval_hints.push(Json::str(format!("keyword:{term}")));
            }
            SkillTrigger::Task { description } => {
                retrieval_hints.push(Json::str(format!("task:{description}")));
            }
        }
    }
    for key in &tree.requires_env {
        preconditions.push(Json::obj([
            ("kind", Json::str("env_requires")),
            ("key", Json::str(key.clone())),
        ]));
    }

    // ── scripts → CompiledPayloads behind Opaque steps ───────────────────
    let mut steps = vec![ProcedureStep::Instruction(body.clone())];
    let mut payloads = Vec::new();
    for f in &tree.scripts {
        let addr = hh_identity::idp::address(&f.bytes, f.media_type.clone());
        let payload = CompiledPayload {
            format_tag: "script".into(),
            bytes_hash: addr.id(),
            declared_interface: Some(DeclaredInterface {
                inputs: Json::obj([("argv", Json::str(f.path.clone()))]),
                outputs: Json::Null,
                effects: {
                    let mut s = std::collections::BTreeSet::new();
                    s.insert(EffectClass::domain_only(EffectDomain::Exec));
                    s
                },
                deterministic: false,
                target: "subprocess_confined".into(),
            }),
            owner: ext.name.clone(),
            provenance: prov(),
        };
        payloads.push(payload.clone());
        steps.push(ProcedureStep::Opaque(payload));
    }
    if !tree.scripts.is_empty() {
        loss.mapped
            .push("scripts/* → CompiledPayload{exec} Opaque steps".into());
    }

    // ── applicability notes + claims ─────────────────────────────────────
    let mut notes: Vec<Json> = Vec::new();
    if let Some(c) = &tree.compatibility {
        notes.push(Json::obj([("content", Json::str(c.clone()))]));
        loss.mapped
            .push("compatibility → applicability_notes".into());
    }
    if let Some(l) = &tree.license {
        notes.push(Json::obj([("content", Json::str(format!("license: {l}")))]));
    }
    let mut declared_claims: Vec<DeclaredClaim> = Vec::new();
    for tool in &tree.allowed_tools {
        declared_claims.push(DeclaredClaim {
            kind: DeclaredClaimKind::AllowedTools,
            value: Json::str(tool.clone()),
        });
    }
    if !tree.allowed_tools.is_empty() {
        loss.mapped
            .push("allowed-tools → declared_claims (never grants)".into());
    }
    if let Some(v) = tree.metadata.get("version") {
        declared_claims.push(DeclaredClaim {
            kind: DeclaredClaimKind::Other,
            value: Json::obj([("version_label", Json::str(v.clone()))]),
        });
    }

    // ── the profile ext block (ProcedureProfile/1) ───────────────────────
    let ns = ext.name.split('/').next().unwrap_or("local");
    let mut metadata = BTreeMap::new();
    for (k, v) in &tree.metadata {
        if k != "version" {
            metadata.insert(k.clone(), Json::str(v.clone()));
        }
    }
    let profile = vec![
        (
            "index",
            Json::obj([
                ("retrieval_hints", Json::Arr(retrieval_hints)),
                ("tags", Json::Arr(vec![Json::str(tree.name.clone())])),
            ]),
        ),
        ("applicability_notes", Json::Arr(notes)),
        ("failure_classes", Json::Arr(vec![])),
        (
            "source",
            Json::obj([
                ("extension_ref", Json::str(ext.name.clone())),
                (
                    "tree_path",
                    tree.tree_path
                        .as_ref()
                        .map(|p| Json::str(p.clone()))
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
    ];
    let mut ext_map = BTreeMap::new();
    ext_map.insert(
        PROCEDURE_PROFILE_EXT_KEY.to_string(),
        Json::Obj(
            profile
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
    );
    if !metadata.is_empty() {
        ext_map.insert(format!("{ns}/metadata"), Json::Obj(metadata));
    }

    // ── the Procedure node ───────────────────────────────────────────────
    let record = ProcedureRecord {
        preconditions: Json::Arr(preconditions),
        steps,
        expected_evidence: Json::Null,
        allowed_capabilities: Vec::new(), // claims → grants only at `seal`
        failure_handlers: Json::Arr(vec![]),
    };
    let mut procedure = Node::new(EntityKind::Procedure, KindRecord::Procedure(record), prov());
    procedure.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::Instruction,
    }));
    procedure.ext = ext_map;
    let proc_sid = procedure.semantic_id();

    // ── references/* + assets/* → content-addressed Artifacts ────────────
    let mut artifacts = Vec::new();
    for f in tree.references.iter().chain(tree.assets.iter()) {
        let addr = hh_identity::idp::address(&f.bytes, f.media_type.clone());
        let mut a = Node::new(
            EntityKind::Artifact,
            KindRecord::Artifact(ArtifactRecord {
                content_hash: addr.id(),
                media_type: f.media_type.clone(),
                size: f.bytes.len() as u64,
                storage_ref: addr.id(),
                produced_by: Ref::selected(proc_sid.clone(), "lifted"),
            }),
            prov(),
        );
        a.ext
            .insert(format!("{ns}/path"), Json::str(f.path.clone()));
        artifacts.push(a);
    }
    if !tree.references.is_empty() || !tree.assets.is_empty() {
        loss.mapped
            .push("references/* + assets/* → content-addressed Artifacts (fs_read scope)".into());
    }
    loss.mapped.push("name → index.source + surface".into());
    loss.mapped
        .push("description → index.description (external)".into());
    loss.mapped.push("body → one Instruction(Text) step".into());

    Ok(SkillLift {
        procedure,
        artifacts,
        payloads,
        texts,
        declared_claims,
        loss,
    })
}
