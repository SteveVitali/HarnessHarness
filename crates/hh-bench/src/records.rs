//! The `hh-bench` records — the adapter lifecycle's value types
//! (R-2.9.4⁰ᵇ; ADR-0142/0143/0144; spec §5h.4). The task/suite schemas live
//! in `hh_lab::bench` (CC7 — one schema source); this module carries only
//! the *runtime* records the out-of-process contract needs.

use std::collections::BTreeMap;

use hh_lab::bench::{EnvironmentSpec, TaskRecord};
use hh_ontology::lab::{BenchmarkNetworkMode, EnvironmentFamily};
use hh_wire::Json;

/// A `BenchTask` is the imported `TaskRecord` (the schema half is
/// `hh_lab::bench::TaskRecord`; the name here marks the adapter's view).
pub type BenchTask = TaskRecord;

/// `environment_handle/1` — the materialized-environment handle an adapter
/// returns from `materialize`/`expose`. A handle is a *reference*, never a
/// capability leak: it names the directory/transport and the family's
/// declared network mode; the participant never sees `grade` or
/// `collect_submission` routes.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentHandle {
    /// The handle id (`env_handle/<idp>` over the materialization record).
    pub handle_id: String,
    /// The task the environment serves.
    pub task_id: String,
    /// The environment family.
    pub family: EnvironmentFamily,
    /// The backing root (a fixture adapter's directory; a real adapter's
    /// transport ref — opaque to the harness, never to the adapter).
    pub root: String,
    /// The declared network mode (L4 egress containment — `none` default;
    /// the adapter records what it enforced, never what it intends).
    pub network_mode: BenchmarkNetworkMode,
    /// The image digest the environment materialized from (`None` = the
    /// task's digest was unresolved at materialize — recorded, never
    /// invented).
    pub resolved_image_digest: Option<String>,
    /// Which surface this handle exposes (`search | held_out | instrument` —
    /// the held-out/instrument surfaces exist only on verifier handles).
    pub surface: Surface,
}

/// The task surface a handle exposes (§5h.4's three surfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Surface {
    /// The agent-visible surface (`visible`).
    Search,
    /// The held-out surface — verifier handles only.
    HeldOut,
    /// The instrument surface — grader handles only.
    Instrument,
}

impl Surface {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Search => "search",
            Surface::HeldOut => "held_out",
            Surface::Instrument => "instrument",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<Surface> {
        [Surface::Search, Surface::HeldOut, Surface::Instrument]
            .into_iter()
            .find(|x| x.as_str() == s)
    }
}

/// `exposed_task/1` — what `expose` delivers: the task's `visible` surface
/// bound to a participant profile, plus the environment handle the
/// participant may drive. Held-out/instrument members never appear here.
#[derive(Debug, Clone, PartialEq)]
pub struct ExposedTask {
    /// The task id.
    pub task_id: String,
    /// The instruction text (the `visible.instruction` content).
    pub instruction: String,
    /// The visible attachment refs.
    pub attachments: Vec<String>,
    /// The metadata scope the task declares visible.
    pub metadata_scope: Json,
    /// The environment handle the participant may drive.
    pub environment: EnvironmentHandle,
    /// The participant profile the exposure was bound to (profile-blind
    /// *except* at `expose` — the binding is recorded, never inferred).
    pub profile_ref: String,
    /// The foreign fields preserved from the source task (`ext` + any
    /// member with no typed home — T-LCD-11, byte-for-byte).
    pub foreign_preserved: BTreeMap<String, Json>,
}

/// `submission/1` — `collect_submission`'s output: the participant's
/// terminal artifact plus the apply evidence. A submission is what `grade`
/// consumes; `collect_submission` is an instrument-plane op the participant
/// can never invoke (the adapter's op routing enforces it).
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    /// The submission id (`idp/1` over the canonical record).
    pub submission_id: String,
    /// The task it answers.
    pub task_id: String,
    /// The artifact refs the participant produced (content addresses).
    pub artifact_refs: Vec<String>,
    /// The terminal payload bytes (hex in canonical JSON).
    pub payload_hex: String,
    /// Whether the submission applied cleanly to the environment (a failed
    /// apply is a *scored failure*, never an infrastructure failure — §5h.4).
    pub applied: bool,
    /// The apply-failure detail, when `applied = false`.
    pub apply_error: Option<String>,
}

impl Submission {
    /// `submission_id = H(canonical(minus submission_id))` under
    /// `bench.submission`.
    pub fn compute_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("submission_id");
        }
        hh_identity::idp_id("bench.submission", j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m: BTreeMap<String, Json> = BTreeMap::new();
        m.insert("schema".into(), Json::str("submission/1"));
        m.insert("submission_id".into(), Json::str(&self.submission_id));
        m.insert("task_id".into(), Json::str(&self.task_id));
        m.insert(
            "artifact_refs".to_string(),
            Json::Arr(self.artifact_refs.iter().map(Json::str).collect()),
        );
        m.insert("payload_hex".to_string(), Json::str(&self.payload_hex));
        m.insert("applied".to_string(), Json::Bool(self.applied));
        if let Some(e) = &self.apply_error {
            m.insert("apply_error".to_string(), Json::str(e));
        }
        Json::Obj(m)
    }
}

impl EnvironmentHandle {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m: BTreeMap<String, Json> = BTreeMap::new();
        m.insert("schema".into(), Json::str("environment_handle/1"));
        m.insert("handle_id".into(), Json::str(&self.handle_id));
        m.insert("task_id".into(), Json::str(&self.task_id));
        m.insert("family".to_string(), Json::str(self.family.name()));
        m.insert("root".to_string(), Json::str(&self.root));
        m.insert(
            "network_mode".to_string(),
            Json::str(self.network_mode.name()),
        );
        if let Some(d) = &self.resolved_image_digest {
            m.insert("resolved_image_digest".to_string(), Json::str(d));
        }
        m.insert("surface".to_string(), Json::str(self.surface.as_str()));
        Json::Obj(m)
    }
}

impl ExposedTask {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m: BTreeMap<String, Json> = BTreeMap::new();
        m.insert("schema".into(), Json::str("exposed_task/1"));
        m.insert("task_id".to_string(), Json::str(&self.task_id));
        m.insert("instruction".to_string(), Json::str(&self.instruction));
        m.insert(
            "attachments".to_string(),
            Json::Arr(self.attachments.iter().map(Json::str).collect()),
        );
        m.insert("metadata_scope".to_string(), self.metadata_scope.clone());
        m.insert("environment".to_string(), self.environment.to_json());
        m.insert("profile_ref".to_string(), Json::str(&self.profile_ref));
        if !self.foreign_preserved.is_empty() {
            m.insert(
                "foreign_preserved".to_string(),
                Json::Obj(self.foreign_preserved.clone()),
            );
        }
        Json::Obj(m)
    }
}

/// The task's environment declaration re-exported (the adapter consumes it
/// at `materialize`).
pub type EnvSpec = EnvironmentSpec;
