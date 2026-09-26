//! The benchmark/environment records (spec §5h.4; R-2.9.4⁰ᵃ; S1.24;
//! ADR-0142…0144).
//!
//! Records: [`EnvironmentFamilyRecord`], [`TaskRecord`] (with
//! [`ForeignTaskId`], [`EnvironmentSpec`], [`TaskVisible`], [`TaskHeldOut`],
//! [`TaskInstrument`], [`AdapterGraderSpec`], [`TaskValidity`],
//! [`TaskContamination`]), [`SuiteManifest`], [`SuiteValidityRecord`],
//! [`SplitAssignmentRecord`] and [`ParityReport`].
//!
//! The import-side checks land here as deterministic pure functions
//! ([`TaskRecord::validate`]): the `visible`/`held_out`/`instrument` surfaces
//! are disjoint (`SurfaceOverlap`), no held-out member appears inside the
//! environment's `seed_data`/`build_context` (`HeldOutInEnvironment`), the
//! `never_delivered` bit is honoured (`HeldOutDeliverable`), and the
//! `verifier_isolation` default rule is enforced against the family record
//! (`SharedUndeclared`). `HeldOutLeak` stays the delivery predicate in
//! `hh-verification::gate`; this module owns the static import half.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_hir::Text;
use hh_identity::idp::{identify_bytes, idp_id};
use hh_identity::kinds::RecordKind;
pub use hh_ontology::lab::EnvironmentFamilyRecord;
use hh_ontology::lab::{
    BenchmarkNetworkMode, ContaminationStratum, EnvironmentFamily, EpisodeModel, SplitLabel,
    TaskValidityState, VerifierIsolation,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::json_util::*;
use crate::model::ForeignRef;

// ── ForeignTaskId ───────────────────────────────────────────────────────────

/// `foreign{name, version, source_ref, digest_claim}` — the task's identity in
/// the external suite it was imported from. `digest_claim` is a
/// [`ForeignRef`] — a claim about a foreign digest, never a `ContentAddress`
/// (I-2).
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignTaskId {
    /// The task's name in the source suite.
    pub name: String,
    /// The source suite version.
    pub version: String,
    /// A ref to the source (URL, vendor id, import manifest).
    pub source_ref: String,
    /// The claimed source digest.
    pub digest_claim: ForeignRef,
}

impl ForeignTaskId {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("name", Json::str(&self.name)),
            ("version", Json::str(&self.version)),
            ("source_ref", Json::str(&self.source_ref)),
            ("digest_claim", self.digest_claim.to_json()),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ForeignTaskId, SchemaError> {
        const REC: &str = "ForeignTaskId";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["name", "version", "source_ref", "digest_claim"], REC)?;
        Ok(ForeignTaskId {
            name: str_at(m, "name", REC)?.to_string(),
            version: str_at(m, "version", REC)?.to_string(),
            source_ref: str_at(m, "source_ref", REC)?.to_string(),
            digest_claim: ForeignRef::from_json(member_at(m, "digest_claim", REC)?)?,
        })
    }
}

// ── EnvironmentSpec ─────────────────────────────────────────────────────────

/// `environment: EnvironmentSpec` — the task's environment declaration.
/// `seed_data`/`build_context` are the members the `HeldOutInEnvironment`
/// import check scans for held-out material.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentSpec {
    /// The environment image ref.
    pub image_ref: String,
    /// The environment image digest.
    pub image_digest: String,
    /// The runtime descriptor (backend-specific, schema-opaque at C0).
    pub runtime: Option<Json>,
    /// The network mode (ADR-0142 — `none` default).
    pub network_mode: BenchmarkNetworkMode,
    /// The environment's hard limits (schema-opaque at C0).
    pub limits: Option<Json>,
    /// Adapter refs the task may bind.
    pub adapters: Vec<String>,
    /// Data refs materialized into the environment as seed data.
    pub seed_data: Vec<String>,
    /// Refs that entered the image build context.
    pub build_context: Vec<String>,
    /// Dialect extension surface.
    pub ext: BTreeMap<String, Json>,
}

impl EnvironmentSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("image_ref".into(), Json::str(&self.image_ref));
        m.insert("image_digest".into(), Json::str(&self.image_digest));
        insert_opt(&mut m, "runtime", self.runtime.clone());
        m.insert("network_mode".into(), Json::str(self.network_mode.name()));
        insert_opt(&mut m, "limits", self.limits.clone());
        m.insert(
            "adapters".into(),
            Json::Arr(self.adapters.iter().map(Json::str).collect()),
        );
        m.insert(
            "seed_data".into(),
            Json::Arr(self.seed_data.iter().map(Json::str).collect()),
        );
        m.insert(
            "build_context".into(),
            Json::Arr(self.build_context.iter().map(Json::str).collect()),
        );
        if !self.ext.is_empty() {
            m.insert("ext".into(), Json::Obj(self.ext.clone()));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<EnvironmentSpec, SchemaError> {
        const REC: &str = "EnvironmentSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "image_ref",
                "image_digest",
                "runtime",
                "network_mode",
                "limits",
                "adapters",
                "seed_data",
                "build_context",
                "ext",
            ],
            REC,
        )?;
        let ext = match m.get("ext") {
            None | Some(Json::Null) => BTreeMap::new(),
            Some(Json::Obj(e)) => e.clone(),
            Some(_) => return Err(SchemaError::v("ext", "must be an object")),
        };
        Ok(EnvironmentSpec {
            image_ref: str_at(m, "image_ref", REC)?.to_string(),
            image_digest: str_at(m, "image_digest", REC)?.to_string(),
            runtime: m
                .get("runtime")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            network_mode: BenchmarkNetworkMode::parse(str_at(m, "network_mode", REC)?)
                .ok_or_else(|| SchemaError::v("network_mode", "unknown benchmark network mode"))?,
            limits: m
                .get("limits")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            adapters: str_vec_at(m, "adapters", REC)?,
            seed_data: str_vec_at(m, "seed_data", REC)?,
            build_context: str_vec_at(m, "build_context", REC)?,
            ext,
        })
    }
}

// ── the three task surfaces ─────────────────────────────────────────────────

/// `visible{instruction: Text, attachments[], task_metadata_scope}` — what the
/// agent may see under any run kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskVisible {
    /// The task instruction (a `Text` leaf — provenance-bearing, R-TEXT).
    pub instruction: Text,
    /// Attachment refs visible to the agent.
    pub attachments: Vec<String>,
    /// Which task metadata fields are visible (schema-opaque selector map).
    pub task_metadata_scope: Json,
}

/// `held_out{validators, fixtures, goal_state?, oracle_solution?,
/// never_delivered}` — the evaluation surface; never delivered to the agent
/// under any run kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskHeldOut {
    /// Validator refs (checker code/data).
    pub validators: Vec<String>,
    /// Held-out fixture refs.
    pub fixtures: Vec<String>,
    /// The goal-state descriptor, when the task is goal-graded.
    pub goal_state: Option<Json>,
    /// The oracle solution, when one exists.
    pub oracle_solution: Option<Json>,
    /// The importer's declaration that none of this surface is ever delivered
    /// to the agent. `false` is a contradiction — [`TaskError::HeldOutDeliverable`].
    pub never_delivered: bool,
}

/// `AdapterGraderSpec{adapter_ref, kind ∈ {result|event|trace|composite},
/// config?}` — the grading adapter binding (§5h.4).
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterGraderSpec {
    /// The adapter ref.
    pub adapter_ref: String,
    /// What the grader grades.
    pub kind: GraderKind,
    /// Adapter config (schema-opaque at C0).
    pub config: Option<Json>,
}

/// The grader kinds (§5h.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraderKind {
    /// Grades the final result.
    Result,
    /// Grades emitted events.
    Event,
    /// Grades the full trace.
    Trace,
    /// A composition of graders.
    Composite,
}

impl GraderKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            GraderKind::Result => "result",
            GraderKind::Event => "event",
            GraderKind::Trace => "trace",
            GraderKind::Composite => "composite",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<GraderKind> {
        match s {
            "result" => Some(GraderKind::Result),
            "event" => Some(GraderKind::Event),
            "trace" => Some(GraderKind::Trace),
            "composite" => Some(GraderKind::Composite),
            _ => None,
        }
    }
}

/// `TaskValidity` — the per-task audit state (§5h.4).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskValidity {
    /// The closed state sum.
    pub state: TaskValidityState,
    /// The audit that set the state.
    pub audit_ref: Option<String>,
    /// The measured noise ceiling, where audited.
    pub noise_ceiling: Option<Json>,
    /// Tasks in the same suite flagged flawed by this audit.
    pub flawed_task_ids: Vec<String>,
}

/// `contamination{first_public_at?, stratum}` (§5h.4).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskContamination {
    /// The first known public timestamp (a date string; `None` = never/unknown).
    pub first_public_at: Option<String>,
    /// The contamination stratum.
    pub stratum: ContaminationStratum,
}

/// `instrument{grader, budget_defaults, verify_budget, validity,
/// contamination, episode_model?, verifier_isolation?}` — the evaluation
/// surface's binding; never visible to the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskInstrument {
    /// The grader binding.
    pub grader: AdapterGraderSpec,
    /// Default budgets for this task (schema-opaque at C0).
    pub budget_defaults: Option<Json>,
    /// The verifier's own budget ref.
    pub verify_budget: Option<String>,
    /// The task's audit state.
    pub validity: TaskValidity,
    /// The contamination claim.
    pub contamination: TaskContamination,
    /// The episode model, when the task declares one.
    pub episode_model: Option<EpisodeModel>,
    /// The task's verifier-isolation override (`None` ⇒ the family's
    /// `verifier_isolation_default`).
    pub verifier_isolation: Option<VerifierIsolation>,
}

// ── TaskRecord ──────────────────────────────────────────────────────────────

/// `TaskRecord/1` (§5h.4; R-2.9.4⁰ᵃ) — the imported task record.
///
/// `task_id` is the **semantic-projection** id (ADR-0036):
/// `H(canonical(record minus foreign, instrument.validity,
/// instrument.contamination, provenance, ext))` — the imported-vs-annotated
/// partition: provenance claims and audit annotations never change the task's
/// identity (CF-450).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskRecord {
    /// The task's semantic id (computed — see [`TaskRecord::semantic_id`]).
    pub task_id: String,
    /// The foreign identity claims.
    pub foreign: ForeignTaskId,
    /// The task's environment family.
    pub family: EnvironmentFamily,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// The task's split label.
    pub split_label: SplitLabel,
    /// The environment declaration.
    pub environment: EnvironmentSpec,
    /// The agent-visible surface.
    pub visible: TaskVisible,
    /// The held-out evaluation surface.
    pub held_out: TaskHeldOut,
    /// The instrument surface.
    pub instrument: TaskInstrument,
    /// The record's provenance (mandatory — R-TEXT carries it for `visible.instruction`).
    pub provenance: ProvenanceRecord,
    /// Dialect extension surface.
    pub ext: BTreeMap<String, Json>,
}

/// The task import/validation refusals — typed, never a warning (§5h.4 §9;
/// AC-R-2.9.4-1 schema half).
#[derive(Debug, Clone, PartialEq)]
pub enum TaskError {
    /// A member-level schema violation.
    Schema(SchemaError),
    /// `held_out.never_delivered = false` — the importer declared the
    /// held-out surface deliverable; a contradiction.
    HeldOutDeliverable,
    /// The same ref appears on two of the three surfaces
    /// (`visible`/`held_out`/`instrument`).
    SurfaceOverlap {
        /// The duplicated ref.
        member: String,
    },
    /// A held-out member (`validators`/`fixtures`/goal/oracle refs) appears in
    /// `environment.seed_data` or `environment.build_context` — held-out
    /// material inside the environment image is a leak at import time
    /// (the static half of L1; `HeldOutLeak` stays the delivery predicate).
    HeldOutInEnvironment {
        /// The leaked member.
        member: String,
    },
    /// The task's `verifier_isolation = shared` where the family record does
    /// not declare `shared` admissible (§5h.4's default rule — `separate`
    /// wherever `submission_kind ≠ none`; `shared` only where declared).
    SharedUndeclared,
}

impl From<SchemaError> for TaskError {
    fn from(e: SchemaError) -> TaskError {
        TaskError::Schema(e)
    }
}

impl TaskRecord {
    /// The semantic-projection JSON — the canonical record minus `foreign`,
    /// `instrument.validity`, `instrument.contamination`, `provenance` and
    /// `ext` (the `task_id` basis; CF-450's imported-vs-annotated partition).
    pub fn semantic_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("family".into(), Json::str(self.family.name()));
        m.insert(
            "tags".into(),
            Json::Arr(self.tags.iter().map(Json::str).collect()),
        );
        m.insert("split_label".into(), Json::str(self.split_label.name()));
        m.insert("environment".into(), self.environment.to_json());
        let mut visible = BTreeMap::new();
        visible.insert("instruction".into(), self.visible.instruction.to_json());
        visible.insert(
            "attachments".into(),
            Json::Arr(self.visible.attachments.iter().map(Json::str).collect()),
        );
        visible.insert(
            "task_metadata_scope".into(),
            self.visible.task_metadata_scope.clone(),
        );
        m.insert("visible".into(), Json::Obj(visible));
        m.insert("held_out".into(), Self::held_out_json(&self.held_out));
        let mut instrument = BTreeMap::new();
        instrument.insert("grader".into(), Self::grader_json(&self.instrument.grader));
        if let Some(b) = &self.instrument.budget_defaults {
            instrument.insert("budget_defaults".into(), b.clone());
        }
        if let Some(v) = &self.instrument.verify_budget {
            instrument.insert("verify_budget".into(), Json::str(v));
        }
        // `validity`/`contamination` are excluded — audit annotations, never identity.
        if let Some(em) = &self.instrument.episode_model {
            instrument.insert("episode_model".into(), Json::str(em.name()));
        }
        if let Some(vi) = &self.instrument.verifier_isolation {
            instrument.insert("verifier_isolation".into(), Json::str(vi.name()));
        }
        m.insert("instrument".into(), Json::Obj(instrument));
        Json::Obj(m)
    }

    /// `task_id = H(semantic_json)` under the `task_record` domain (idp/1).
    pub fn semantic_id(&self) -> String {
        identify_bytes(
            RecordKind::TaskRecord,
            self.semantic_json().to_canonical_string().as_bytes(),
        )
    }

    fn grader_json(g: &AdapterGraderSpec) -> Json {
        let mut m = BTreeMap::new();
        m.insert("adapter_ref".into(), Json::str(&g.adapter_ref));
        m.insert("kind".into(), Json::str(g.kind.name()));
        if let Some(c) = &g.config {
            m.insert("config".into(), c.clone());
        }
        Json::Obj(m)
    }

    fn held_out_json(h: &TaskHeldOut) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "validators".into(),
            Json::Arr(h.validators.iter().map(Json::str).collect()),
        );
        m.insert(
            "fixtures".into(),
            Json::Arr(h.fixtures.iter().map(Json::str).collect()),
        );
        if let Some(g) = &h.goal_state {
            m.insert("goal_state".into(), g.clone());
        }
        if let Some(o) = &h.oracle_solution {
            m.insert("oracle_solution".into(), o.clone());
        }
        m.insert("never_delivered".into(), Json::Bool(h.never_delivered));
        Json::Obj(m)
    }

    /// The import-side checks (AC-R-2.9.4-1 schema half): disjoint surfaces;
    /// `never_delivered`; `HeldOutInEnvironment`. The family-parameterized
    /// `verifier_isolation` rule is [`TaskRecord::validate_for_family`].
    pub fn validate(&self) -> Result<(), TaskError> {
        if !self.held_out.never_delivered {
            return Err(TaskError::HeldOutDeliverable);
        }
        // Disjointness across the three surfaces — a ref may not appear on two.
        let mut seen: BTreeMap<String, &'static str> = BTreeMap::new();
        for (surface, r) in self
            .visible
            .attachments
            .iter()
            .map(|r| ("visible", r))
            .chain(
                self.held_out
                    .validators
                    .iter()
                    .chain(&self.held_out.fixtures)
                    .map(|r| ("held_out", r)),
            )
            .chain(
                self.instrument
                    .validity
                    .audit_ref
                    .iter()
                    .map(|r| ("instrument", r)),
            )
        {
            if seen.insert(r.clone(), surface).is_some() {
                return Err(TaskError::SurfaceOverlap { member: r.clone() });
            }
        }
        // HeldOutInEnvironment — held-out refs may not appear in the
        // environment's `seed_data`/`build_context` (the L1 static half).
        let held_out_refs: BTreeSet<&str> = self
            .held_out
            .validators
            .iter()
            .chain(&self.held_out.fixtures)
            .map(String::as_str)
            .collect();
        for r in self
            .environment
            .seed_data
            .iter()
            .chain(&self.environment.build_context)
        {
            if held_out_refs.contains(r.as_str()) {
                return Err(TaskError::HeldOutInEnvironment { member: r.clone() });
            }
        }
        Ok(())
    }

    /// The `verifier_isolation` rule against the family record: `shared` is
    /// admissible only where the family declares it (its
    /// `verifier_isolation_default` is `shared`); `separate` is the default
    /// wherever `submission_kind ≠ none`.
    pub fn validate_for_family(&self, family: &EnvironmentFamilyRecord) -> Result<(), TaskError> {
        self.validate()?;
        let resolved = self
            .instrument
            .verifier_isolation
            .unwrap_or(family.verifier_isolation_default);
        if resolved == VerifierIsolation::Shared && !family.declares_shared() {
            return Err(TaskError::SharedUndeclared);
        }
        Ok(())
    }

    /// The canonical JSON (the full record — the versioned form).
    pub fn to_json(&self) -> Json {
        let mut m = self.semantic_json();
        if let Json::Obj(ref mut mm) = m {
            mm.insert("task_id".into(), Json::str(&self.task_id));
            mm.insert("foreign".into(), self.foreign.to_json());
            if let Json::Obj(ref mut im) = mm
                .get_mut("instrument")
                .expect("semantic_json always emits instrument")
            {
                im.insert(
                    "validity".into(),
                    Self::validity_json(&self.instrument.validity),
                );
                im.insert(
                    "contamination".into(),
                    Self::contamination_json(&self.instrument.contamination),
                );
            }
            mm.insert("provenance".into(), self.provenance.to_json());
            if !self.ext.is_empty() {
                mm.insert("ext".into(), Json::Obj(self.ext.clone()));
            }
        }
        m
    }

    fn validity_json(v: &TaskValidity) -> Json {
        let mut m = BTreeMap::new();
        m.insert("state".into(), Json::str(v.state.name()));
        if let Some(a) = &v.audit_ref {
            m.insert("audit_ref".into(), Json::str(a));
        }
        if let Some(n) = &v.noise_ceiling {
            m.insert("noise_ceiling".into(), n.clone());
        }
        m.insert(
            "flawed_task_ids".into(),
            Json::Arr(v.flawed_task_ids.iter().map(Json::str).collect()),
        );
        Json::Obj(m)
    }

    fn contamination_json(c: &TaskContamination) -> Json {
        let mut m = BTreeMap::new();
        if let Some(t) = &c.first_public_at {
            m.insert("first_public_at".into(), Json::str(t));
        }
        m.insert("stratum".into(), Json::str(c.stratum.name()));
        Json::Obj(m)
    }

    /// Strict decode (the versioned form — all members, `task_id` verified
    /// against the semantic projection by [`TaskRecord::check_id`]).
    pub fn from_json(j: &Json) -> Result<TaskRecord, TaskError> {
        const REC: &str = "TaskRecord";
        let m = expect_obj(j, REC).map_err(TaskError::Schema)?;
        reject_unknown(
            m,
            &[
                "task_id",
                "foreign",
                "family",
                "tags",
                "split_label",
                "environment",
                "visible",
                "held_out",
                "instrument",
                "provenance",
                "ext",
            ],
            REC,
        )
        .map_err(TaskError::Schema)?;
        let visible =
            expect_obj(member_at(m, "visible", REC)?, "TaskVisible").map_err(TaskError::Schema)?;
        let held_out =
            expect_obj(member_at(m, "held_out", REC)?, "TaskHeldOut").map_err(TaskError::Schema)?;
        let instrument = expect_obj(member_at(m, "instrument", REC)?, "TaskInstrument")
            .map_err(TaskError::Schema)?;
        let grader = expect_obj(
            member_at(instrument, "grader", "TaskInstrument")?,
            "AdapterGraderSpec",
        )
        .map_err(TaskError::Schema)?;
        let validity = expect_obj(
            member_at(instrument, "validity", "TaskInstrument")?,
            "TaskValidity",
        )
        .map_err(TaskError::Schema)?;
        let contamination = expect_obj(
            member_at(instrument, "contamination", "TaskInstrument")?,
            "TaskContamination",
        )
        .map_err(TaskError::Schema)?;
        let ext = match m.get("ext") {
            None | Some(Json::Null) => BTreeMap::new(),
            Some(Json::Obj(e)) => e.clone(),
            Some(_) => {
                return Err(TaskError::Schema(SchemaError::v(
                    "ext",
                    "must be an object",
                )))
            }
        };
        let rec = TaskRecord {
            task_id: str_at(m, "task_id", REC)?.to_string(),
            foreign: ForeignTaskId::from_json(member_at(m, "foreign", REC)?)
                .map_err(TaskError::Schema)?,
            family: EnvironmentFamily::parse(str_at(m, "family", REC)?).ok_or_else(|| {
                TaskError::Schema(SchemaError::v("family", "unknown environment family"))
            })?,
            tags: str_vec_at(m, "tags", REC)?,
            split_label: SplitLabel::parse(str_at(m, "split_label", REC)?).ok_or_else(|| {
                TaskError::Schema(SchemaError::v("split_label", "unknown split label"))
            })?,
            environment: EnvironmentSpec::from_json(member_at(m, "environment", REC)?)
                .map_err(TaskError::Schema)?,
            visible: TaskVisible {
                instruction: Text::from_json(
                    member_at(visible, "instruction", "TaskVisible")?,
                    "TaskVisible.instruction",
                )
                .map_err(|e| TaskError::Schema(SchemaError::v("instruction", format!("{e:?}"))))?,
                attachments: str_vec_at(visible, "attachments", "TaskVisible")?,
                task_metadata_scope: member_at(visible, "task_metadata_scope", "TaskVisible")?
                    .clone(),
            },
            held_out: TaskHeldOut {
                validators: str_vec_at(held_out, "validators", "TaskHeldOut")?,
                fixtures: str_vec_at(held_out, "fixtures", "TaskHeldOut")?,
                goal_state: held_out
                    .get("goal_state")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                oracle_solution: held_out
                    .get("oracle_solution")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                never_delivered: bool_at(held_out, "never_delivered", "TaskHeldOut")?,
            },
            instrument: TaskInstrument {
                grader: AdapterGraderSpec {
                    adapter_ref: str_at(grader, "adapter_ref", "AdapterGraderSpec")?.to_string(),
                    kind: GraderKind::parse(str_at(grader, "kind", "AdapterGraderSpec")?)
                        .ok_or_else(|| {
                            TaskError::Schema(SchemaError::v("kind", "unknown grader kind"))
                        })?,
                    config: grader
                        .get("config")
                        .filter(|j| !matches!(j, Json::Null))
                        .cloned(),
                },
                budget_defaults: instrument
                    .get("budget_defaults")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                verify_budget: opt_str_at(instrument, "verify_budget")?.map(str::to_string),
                validity: TaskValidity {
                    state: TaskValidityState::parse(str_at(validity, "state", "TaskValidity")?)
                        .ok_or_else(|| {
                            TaskError::Schema(SchemaError::v(
                                "state",
                                "unknown task validity state",
                            ))
                        })?,
                    audit_ref: opt_str_at(validity, "audit_ref")?.map(str::to_string),
                    noise_ceiling: validity
                        .get("noise_ceiling")
                        .filter(|j| !matches!(j, Json::Null))
                        .cloned(),
                    flawed_task_ids: str_vec_at(validity, "flawed_task_ids", "TaskValidity")?,
                },
                contamination: TaskContamination {
                    first_public_at: opt_str_at(contamination, "first_public_at")?
                        .map(str::to_string),
                    stratum: ContaminationStratum::parse(str_at(
                        contamination,
                        "stratum",
                        "TaskContamination",
                    )?)
                    .ok_or_else(|| {
                        TaskError::Schema(SchemaError::v(
                            "stratum",
                            "unknown contamination stratum",
                        ))
                    })?,
                },
                episode_model: opt_str_at(instrument, "episode_model")?
                    .map(|s| {
                        EpisodeModel::parse(s).ok_or_else(|| {
                            TaskError::Schema(SchemaError::v(
                                "episode_model",
                                "unknown episode model",
                            ))
                        })
                    })
                    .transpose()?,
                verifier_isolation: opt_str_at(instrument, "verifier_isolation")?
                    .map(|s| {
                        VerifierIsolation::parse(s).ok_or_else(|| {
                            TaskError::Schema(SchemaError::v(
                                "verifier_isolation",
                                "unknown verifier isolation",
                            ))
                        })
                    })
                    .transpose()?,
            },
            provenance: ProvenanceRecord::from_json(member_at(m, "provenance", REC)?)
                .map_err(|e| TaskError::Schema(SchemaError::v("provenance", format!("{e:?}"))))?,
            ext,
        };
        Ok(rec)
    }

    /// Whether `task_id` equals the semantic-projection id (the importer's
    /// identity check — `import` computes, never trusts the submitted id).
    pub fn check_id(&self) -> bool {
        self.task_id == self.semantic_id()
    }
}

// ── SuiteValidityRecord / SuiteManifest / SplitAssignmentRecord ─────────────

/// `SuiteValidityRecord` — the suite's audit state: the L1 `audit_ref` plus
/// the L2 epoch-seeded validator dispatch (§5h.4; AC-R-2.9.4 schema half).
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteValidityRecord {
    /// The audit that certified the suite (the L1 predicate — mandatory).
    pub audit_ref: String,
    /// The audit's logical timestamp.
    pub audited_at: Option<u64>,
    /// Whether the suite has run the per-suite `epoch_seeded` validator
    /// dispatch (the L2 check's flag half).
    pub epoch_seeded: bool,
    /// The dispatch record ref (the L2 check's evidence half).
    pub dispatch_ref: Option<String>,
    /// Tasks flagged flawed by the audit.
    pub flawed_task_ids: Vec<String>,
    /// The suite noise ceiling, where measured.
    pub noise_ceiling: Option<Json>,
    /// Whether the suite is retired for headline comparisons.
    pub retired_for_headline: bool,
    /// The retirement reason.
    pub reason: Option<String>,
}

impl SuiteValidityRecord {
    /// The L2 check — a per-suite `epoch_seeded` validator dispatch: the flag
    /// and its dispatch record must both be present (§5h.4; R-2.9.4⁰ᵃ row 7).
    pub fn l2_check(&self) -> Result<(), SuiteError> {
        if !(self.epoch_seeded && self.dispatch_ref.is_some()) {
            return Err(SuiteError::MissingEpochSeededDispatch);
        }
        Ok(())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("audit_ref".into(), Json::str(&self.audit_ref));
        if let Some(t) = self.audited_at {
            m.insert("audited_at".into(), Json::Int(t as i64));
        }
        m.insert("epoch_seeded".into(), Json::Bool(self.epoch_seeded));
        if let Some(d) = &self.dispatch_ref {
            m.insert("dispatch_ref".into(), Json::str(d));
        }
        m.insert(
            "flawed_task_ids".into(),
            Json::Arr(self.flawed_task_ids.iter().map(Json::str).collect()),
        );
        if let Some(n) = &self.noise_ceiling {
            m.insert("noise_ceiling".into(), n.clone());
        }
        m.insert(
            "retired_for_headline".into(),
            Json::Bool(self.retired_for_headline),
        );
        if let Some(r) = &self.reason {
            m.insert("reason".into(), Json::str(r));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SuiteValidityRecord, SchemaError> {
        const REC: &str = "SuiteValidityRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "audit_ref",
                "audited_at",
                "epoch_seeded",
                "dispatch_ref",
                "flawed_task_ids",
                "noise_ceiling",
                "retired_for_headline",
                "reason",
            ],
            REC,
        )?;
        Ok(SuiteValidityRecord {
            audit_ref: str_at(m, "audit_ref", REC)?.to_string(),
            audited_at: opt_int_at(m, "audited_at")?.map(|v| v as u64),
            epoch_seeded: bool_at(m, "epoch_seeded", REC)?,
            dispatch_ref: opt_str_at(m, "dispatch_ref")?.map(str::to_string),
            flawed_task_ids: str_vec_at(m, "flawed_task_ids", REC)?,
            noise_ceiling: m
                .get("noise_ceiling")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            retired_for_headline: bool_at(m, "retired_for_headline", REC)?,
            reason: opt_str_at(m, "reason")?.map(str::to_string),
        })
    }
}

/// `SuiteManifest` — the imported suite's content-addressed record (§5h.4;
/// R-2.9.4⁰ᵃ row 4; ADR-0143 L3: `split_hash` over the splits map).
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteManifest {
    /// `suite_id = H(canonical(manifest))` — content-addressed.
    pub suite_id: String,
    /// The foreign suite identity claims.
    pub foreign: ForeignTaskId,
    /// The suite's primary environment family.
    pub primary_family: EnvironmentFamily,
    /// The task ids in the suite.
    pub tasks: Vec<String>,
    /// `split_map: map<task_id, SplitLabel>` — the suite's split assignments.
    pub split_map: BTreeMap<String, SplitLabel>,
    /// `split_hash` — `idp_id("split_assignment", canonical(split_map))`; it
    /// must predate any search over the suite (ADR-0143 L3).
    pub split_hash: String,
    /// The suite's audit state.
    pub validity: SuiteValidityRecord,
    /// The default contamination stratum for member tasks.
    pub contamination_default: ContaminationStratum,
    /// The adapter ref the suite's graders bind.
    pub adapter_ref: String,
    /// The import parity report, where the suite was imported with replay.
    pub parity_report: Option<crate::analysis::ParityReport>,
    /// The record's provenance (mandatory).
    pub provenance: ProvenanceRecord,
}

/// The suite-side refusals (§5h.4; typed, never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum SuiteError {
    /// A member-level schema violation.
    Schema(SchemaError),
    /// A `split_map` key names a task not in `tasks[]`.
    SplitKeyNotInTasks {
        /// The offending task id.
        task_id: String,
    },
    /// The L2 check failed — no `epoch_seeded` validator dispatch.
    MissingEpochSeededDispatch,
    /// `split_hash` does not match the `split_map` content.
    SplitHashMismatch,
    /// The parity report's comparison is not `benefit_kind = artifact_benefit`
    /// (a parity report is always an artifact-benefit comparison).
    ParityBenefitKind,
}

impl From<SchemaError> for SuiteError {
    fn from(e: SchemaError) -> SuiteError {
        SuiteError::Schema(e)
    }
}

/// `split_hash = H(canonical(split_map))` under the `split_assignment` domain
/// (ADR-0143 L3).
pub fn split_hash(split_map: &BTreeMap<String, SplitLabel>) -> String {
    let mut m = BTreeMap::new();
    for (k, v) in split_map {
        m.insert(k.clone(), Json::str(v.name()));
    }
    idp_id(
        RecordKind::SplitAssignment.domain_tag(),
        Json::Obj(m).to_canonical_string().as_bytes(),
    )
}

impl SuiteManifest {
    /// `suite_id = H(canonical(manifest minus suite_id))` under the
    /// `suite_manifest` domain.
    pub fn suite_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("suite_id");
        }
        identify_bytes(
            RecordKind::SuiteManifest,
            j.to_canonical_string().as_bytes(),
        )
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("suite_id".into(), Json::str(&self.suite_id));
        m.insert("foreign".into(), self.foreign.to_json());
        m.insert(
            "primary_family".into(),
            Json::str(self.primary_family.name()),
        );
        m.insert(
            "tasks".into(),
            Json::Arr(self.tasks.iter().map(Json::str).collect()),
        );
        m.insert(
            "split_map".into(),
            Json::Obj(
                self.split_map
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.name())))
                    .collect(),
            ),
        );
        m.insert("split_hash".into(), Json::str(&self.split_hash));
        m.insert("validity".into(), self.validity.to_json());
        m.insert(
            "contamination_default".into(),
            Json::str(self.contamination_default.name()),
        );
        m.insert("adapter_ref".into(), Json::str(&self.adapter_ref));
        if let Some(p) = &self.parity_report {
            m.insert("parity_report".into(), p.to_json());
        }
        m.insert("provenance".into(), self.provenance.to_json());
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SuiteManifest, SuiteError> {
        const REC: &str = "SuiteManifest";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "suite_id",
                "foreign",
                "primary_family",
                "tasks",
                "split_map",
                "split_hash",
                "validity",
                "contamination_default",
                "adapter_ref",
                "parity_report",
                "provenance",
            ],
            REC,
        )?;
        let mut split_map = BTreeMap::new();
        match member_at(m, "split_map", REC)? {
            Json::Obj(sm) => {
                for (k, v) in sm {
                    let l = v.as_str().and_then(SplitLabel::parse).ok_or_else(|| {
                        SuiteError::Schema(SchemaError::v(
                            "split_map",
                            format!("unknown split label for `{k}`"),
                        ))
                    })?;
                    split_map.insert(k.clone(), l);
                }
            }
            _ => {
                return Err(SuiteError::Schema(SchemaError::v(
                    "split_map",
                    "must be an object",
                )))
            }
        }
        Ok(SuiteManifest {
            suite_id: str_at(m, "suite_id", REC)?.to_string(),
            foreign: ForeignTaskId::from_json(member_at(m, "foreign", REC)?)?,
            primary_family: EnvironmentFamily::parse(str_at(m, "primary_family", REC)?)
                .ok_or_else(|| {
                    SuiteError::Schema(SchemaError::v("primary_family", "unknown family"))
                })?,
            tasks: str_vec_at(m, "tasks", REC)?,
            split_map,
            split_hash: str_at(m, "split_hash", REC)?.to_string(),
            validity: SuiteValidityRecord::from_json(member_at(m, "validity", REC)?)?,
            contamination_default: ContaminationStratum::parse(str_at(
                m,
                "contamination_default",
                REC,
            )?)
            .ok_or_else(|| {
                SuiteError::Schema(SchemaError::v(
                    "contamination_default",
                    "unknown contamination stratum",
                ))
            })?,
            adapter_ref: str_at(m, "adapter_ref", REC)?.to_string(),
            parity_report: match m.get("parity_report") {
                None | Some(Json::Null) => None,
                Some(p) => Some(crate::analysis::ParityReport::from_json(p)?),
            },
            provenance: ProvenanceRecord::from_json(member_at(m, "provenance", REC)?)
                .map_err(|e| SuiteError::Schema(SchemaError::v("provenance", format!("{e:?}"))))?,
        })
    }

    /// The suite's schema checks: `split_map` keys ⊆ `tasks[]`; `split_hash`
    /// equals the map's content hash; the L2 epoch-seeded dispatch; the parity
    /// report's benefit kind.
    pub fn validate(&self) -> Result<(), SuiteError> {
        let tasks: BTreeSet<&str> = self.tasks.iter().map(String::as_str).collect();
        for k in self.split_map.keys() {
            if !tasks.contains(k.as_str()) {
                return Err(SuiteError::SplitKeyNotInTasks { task_id: k.clone() });
            }
        }
        if self.split_hash != split_hash(&self.split_map) {
            return Err(SuiteError::SplitHashMismatch);
        }
        self.validity.l2_check()?;
        if let Some(p) = &self.parity_report {
            if p.comparison.benefit_kind != crate::analysis::BenefitKind::ArtifactBenefit {
                return Err(SuiteError::ParityBenefitKind);
            }
        }
        Ok(())
    }
}

/// `SplitAssignmentRecord{suite_id, rule, seed, splits, split_hash,
/// registered_at}` — the registered, immutable split assignment (ADR-0143
/// L3; content-addressed under `split_assignment`).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitAssignmentRecord {
    /// The suite the assignment applies to.
    pub suite_id: String,
    /// The assignment rule name (`"hash_of_task_id"` for imported suites).
    pub rule: String,
    /// The assignment seed (a deterministic string, e.g. `"0"`).
    pub seed: String,
    /// `splits: map<task_id, SplitLabel>`.
    pub splits: BTreeMap<String, SplitLabel>,
    /// `split_hash` — recomputed over `splits`; must match.
    pub split_hash: String,
    /// The registration's logical time (a `seq`, never a wall clock).
    pub registered_at: u64,
}

impl SplitAssignmentRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("suite_id".into(), Json::str(&self.suite_id));
        m.insert("rule".into(), Json::str(&self.rule));
        m.insert("seed".into(), Json::str(&self.seed));
        m.insert(
            "splits".into(),
            Json::Obj(
                self.splits
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.name())))
                    .collect(),
            ),
        );
        m.insert("split_hash".into(), Json::str(&self.split_hash));
        m.insert("registered_at".into(), Json::Int(self.registered_at as i64));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SplitAssignmentRecord, SchemaError> {
        const REC: &str = "SplitAssignmentRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "suite_id",
                "rule",
                "seed",
                "splits",
                "split_hash",
                "registered_at",
            ],
            REC,
        )?;
        let mut splits = BTreeMap::new();
        match member_at(m, "splits", REC)? {
            Json::Obj(sm) => {
                for (k, v) in sm {
                    let l = v.as_str().and_then(SplitLabel::parse).ok_or_else(|| {
                        SchemaError::v("splits", format!("unknown split label for `{k}`"))
                    })?;
                    splits.insert(k.clone(), l);
                }
            }
            _ => return Err(SchemaError::v("splits", "must be an object")),
        }
        Ok(SplitAssignmentRecord {
            suite_id: str_at(m, "suite_id", REC)?.to_string(),
            rule: str_at(m, "rule", REC)?.to_string(),
            seed: str_at(m, "seed", REC)?.to_string(),
            splits,
            split_hash: str_at(m, "split_hash", REC)?.to_string(),
            registered_at: int_at(m, "registered_at", REC)? as u64,
        })
    }

    /// The schema check: `split_hash` matches `splits`.
    pub fn validate(&self) -> Result<(), SuiteError> {
        if self.split_hash != split_hash(&self.splits) {
            return Err(SuiteError::SplitHashMismatch);
        }
        Ok(())
    }
}
