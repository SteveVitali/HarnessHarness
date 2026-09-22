//! The Stage-3 fixture adapters A/C/D/E (R-2.9.4⁰ᵇ; spec §5h.4).
//!
//! The lettered variants are one `benchmark_adapter` component bound to a
//! family each — `adapter_a` (coding_terminal), `adapter_c`
//! (search_research), `adapter_d` (structured_tool), `adapter_e`
//! (persistent_multi_episode). Each serves a **fixture** suite of imported
//! `TaskRecord`s — the adapter integrates a suite; the suite's tasks are
//! data, never authored into the adapter (the "integrate, never author"
//! line is enforced by the records: a `TaskRecord` carries its foreign
//! identity + provenance, and the adapter only routes surfaces).
//!
//! The process boundary is `hh-bench-adapter`: every op here is the
//! in-process body the binary wraps — identical bytes in, identical bytes
//! out.

use std::collections::BTreeMap;
use std::path::Path;

use hh_lab::bench::{
    AdapterGraderSpec, EnvironmentSpec, ForeignTaskId, GraderKind, TaskContamination, TaskHeldOut,
    TaskInstrument, TaskRecord, TaskValidity, TaskVisible,
};
use hh_lab::model::ForeignRef;
use hh_ontology::lab::{
    BenchmarkNetworkMode, ContaminationStratum, EnvironmentFamily, SplitLabel, TaskValidityState,
    VerifierIsolation,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::adapter::{AdapterError, BenchmarkAdapter};
use crate::env::FsEnv;
use crate::grade::{grade, GradeError, GradeRequest, GradeResult};
use crate::infra::InfraSignal;
use crate::records::{BenchTask, EnvironmentHandle, ExposedTask, Submission, Surface};

/// A fixture suite task: the imported record plus the oracle answer the
/// One fixture suite row — the imported `TaskRecord` plus the expected
/// submission payload (the fixture oracle the verifier checks against —
/// `held_out.oracle_solution`'s bytes).
pub struct FixtureTask {
    /// The imported record.
    pub record: TaskRecord,
    /// The expected submission payload (the fixture oracle).
    pub expected: Vec<u8>,
}

/// `FixtureAdapter` — one adapter letter bound to one family.
pub struct FixtureAdapter {
    /// `adapter_a` | `adapter_c` | `adapter_d` | `adapter_e`.
    adapter_id: &'static str,
    /// The served family.
    family: EnvironmentFamily,
    /// The family's `HandleRequirements` (`EnvironmentFamilyRecord.requires`
    /// — the `materialize` gate, AC-R-2.9.4-12).
    requires: BTreeMap<hh_ontology::lab::HandleCapability, hh_ontology::lab::Support>,
    /// The fixture suite (`task_id` order — deterministic).
    tasks: BTreeMap<String, FixtureTask>,
}

#[allow(clippy::too_many_arguments)] // the fixture row's members
/// Build one fixture `TaskRecord` — the shared constructor the lettered
/// adapters and the test fixtures use (the record's `task_id` is computed
/// by `semantic_id`, never caller-chosen).
pub fn fixture_task(
    adapter_id: &str,
    family: EnvironmentFamily,
    name: &str,
    split: SplitLabel,
    stratum: ContaminationStratum,
    instruction: &str,
    expected: &[u8],
    isolation: Option<VerifierIsolation>,
    episode_model: Option<hh_ontology::lab::EpisodeModel>,
) -> FixtureTask {
    let prov = || ProvenanceRecord::kernel(format!("hh-bench/{adapter_id}"), 0);
    let mut rec = TaskRecord {
        task_id: String::new(),
        foreign: ForeignTaskId {
            name: name.into(),
            version: "fixture-1".into(),
            source_ref: format!("fixture://{adapter_id}"),
            digest_claim: ForeignRef {
                system: "fixture".into(),
                digest: hh_identity::idp_digest("bench.foreign", expected),
                label: Some(format!("{adapter_id}:{name}")),
                provenance: prov(),
            },
        },
        family,
        tags: vec!["fixture".into()],
        split_label: split,
        environment: EnvironmentSpec {
            image_ref: format!("fixture/{adapter_id}/{name}"),
            image_digest: hh_identity::idp_digest("bench.image", name.as_bytes()),
            runtime: None,
            network_mode: BenchmarkNetworkMode::None,
            limits: None,
            adapters: vec![adapter_id.into()],
            seed_data: vec![],
            build_context: vec![],
            ext: BTreeMap::new(),
        },
        visible: TaskVisible {
            instruction: hh_hir::Text::new(instruction, format!("suite/{adapter_id}"), prov()),
            attachments: vec![],
            task_metadata_scope: Json::obj([("fields", Json::Arr(vec![]))]),
        },
        held_out: TaskHeldOut {
            validators: vec![format!("{adapter_id}.verifier")],
            fixtures: vec![],
            goal_state: None,
            oracle_solution: Some(Json::str(hex(expected))),
            never_delivered: true,
        },
        instrument: TaskInstrument {
            grader: AdapterGraderSpec {
                adapter_ref: adapter_id.into(),
                kind: GraderKind::Result,
                config: None,
            },
            budget_defaults: None,
            verify_budget: None,
            validity: TaskValidity {
                state: TaskValidityState::Valid,
                audit_ref: None,
                noise_ceiling: None,
                flawed_task_ids: vec![],
            },
            contamination: TaskContamination {
                first_public_at: None,
                stratum,
            },
            episode_model,
            verifier_isolation: isolation,
        },
        provenance: prov(),
        ext: BTreeMap::new(),
    };
    rec.task_id = rec.semantic_id();
    FixtureTask {
        record: rec,
        expected: expected.to_vec(),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl FixtureAdapter {
    /// `adapter_a` — the `coding_terminal` fixture suite.
    pub fn adapter_a() -> FixtureAdapter {
        let mut tasks = BTreeMap::new();
        for (name, split, instruction, expected) in [
            (
                "a-hello",
                SplitLabel::Search,
                "Write the string `hello` to submission.json.",
                b"{\"answer\":\"hello\"}".as_slice(),
            ),
            (
                "a-sum",
                SplitLabel::HeldOut,
                "Write the sum of 2 and 3 to submission.json.",
                b"{\"answer\":5}".as_slice(),
            ),
        ] {
            let t = fixture_task(
                "adapter_a",
                EnvironmentFamily::CodingTerminal,
                name,
                split,
                ContaminationStratum::PrivateHeldOut,
                instruction,
                expected,
                None,
                None,
            );
            tasks.insert(t.record.task_id.clone(), t);
        }
        FixtureAdapter {
            adapter_id: "adapter_a",
            family: EnvironmentFamily::CodingTerminal,
            requires: [
                (
                    hh_ontology::lab::HandleCapability::Bash,
                    hh_ontology::lab::Support::Required,
                ),
                (
                    hh_ontology::lab::HandleCapability::File,
                    hh_ontology::lab::Support::Required,
                ),
            ]
            .into_iter()
            .collect(),
            tasks,
        }
    }

    /// `adapter_c` — the `search_research` fixture suite.
    pub fn adapter_c() -> FixtureAdapter {
        let mut tasks = BTreeMap::new();
        for (name, split, instruction, expected) in [
            (
                "c-capital",
                SplitLabel::Search,
                "Research: write the capital of France to submission.json.",
                b"{\"answer\":\"paris\"}".as_slice(),
            ),
            (
                "c-element",
                SplitLabel::HeldOut,
                "Research: write the chemical symbol for gold to submission.json.",
                b"{\"answer\":\"au\"}".as_slice(),
            ),
        ] {
            let t = fixture_task(
                "adapter_c",
                EnvironmentFamily::SearchResearch,
                name,
                split,
                ContaminationStratum::PublicDated,
                instruction,
                expected,
                None,
                None,
            );
            tasks.insert(t.record.task_id.clone(), t);
        }
        FixtureAdapter {
            adapter_id: "adapter_c",
            family: EnvironmentFamily::SearchResearch,
            requires: [(
                hh_ontology::lab::HandleCapability::File,
                hh_ontology::lab::Support::Required,
            )]
            .into_iter()
            .collect(),
            tasks,
        }
    }

    /// `adapter_d` — the `structured_tool` fixture suite.
    pub fn adapter_d() -> FixtureAdapter {
        let mut tasks = BTreeMap::new();
        for (name, split, instruction, expected) in [
            (
                "d-tool",
                SplitLabel::Search,
                "Call the declared tool shape; write {\"answer\":true} to submission.json.",
                b"{\"answer\":true}".as_slice(),
            ),
            (
                "d-args",
                SplitLabel::HeldOut,
                "Complete the tool call with the declared args; write {\"answer\":\"ok\"}.",
                b"{\"answer\":\"ok\"}".as_slice(),
            ),
        ] {
            let t = fixture_task(
                "adapter_d",
                EnvironmentFamily::StructuredTool,
                name,
                split,
                ContaminationStratum::PrivateHeldOut,
                instruction,
                expected,
                Some(VerifierIsolation::Separate),
                None,
            );
            tasks.insert(t.record.task_id.clone(), t);
        }
        FixtureAdapter {
            adapter_id: "adapter_d",
            family: EnvironmentFamily::StructuredTool,
            // `structured_tool` needs no container — the fixture env's
            // kernel-internal file handle suffices (AC-R-2.9.4-12's second
            // half).
            requires: [(
                hh_ontology::lab::HandleCapability::File,
                hh_ontology::lab::Support::Required,
            )]
            .into_iter()
            .collect(),
            tasks,
        }
    }

    /// `adapter_e` — the `persistent_multi_episode` fixture suite.
    pub fn adapter_e() -> FixtureAdapter {
        let mut tasks = BTreeMap::new();
        for (name, split, instruction, expected) in [
            (
                "e-persist",
                SplitLabel::Search,
                "Across episodes: write the remembered token {\"answer\":\"token-1\"}.",
                b"{\"answer\":\"token-1\"}".as_slice(),
            ),
            (
                "e-recall",
                SplitLabel::HeldOut,
                "Across episodes: write the remembered token {\"answer\":\"token-2\"}.",
                b"{\"answer\":\"token-2\"}".as_slice(),
            ),
        ] {
            let t = fixture_task(
                "adapter_e",
                EnvironmentFamily::PersistentMultiEpisode,
                name,
                split,
                ContaminationStratum::FreshTemporal,
                instruction,
                expected,
                Some(VerifierIsolation::Separate),
                Some(hh_ontology::lab::EpisodeModel::Persistent),
            );
            tasks.insert(t.record.task_id.clone(), t);
        }
        FixtureAdapter {
            adapter_id: "adapter_e",
            family: EnvironmentFamily::PersistentMultiEpisode,
            requires: [
                (
                    hh_ontology::lab::HandleCapability::File,
                    hh_ontology::lab::Support::Required,
                ),
                (
                    hh_ontology::lab::HandleCapability::LongRunning,
                    hh_ontology::lab::Support::Required,
                ),
            ]
            .into_iter()
            .collect(),
            tasks,
        }
    }

    /// The letter → adapter dispatch.
    pub fn by_id(adapter_id: &str) -> Option<FixtureAdapter> {
        match adapter_id {
            "adapter_a" => Some(Self::adapter_a()),
            "adapter_c" => Some(Self::adapter_c()),
            "adapter_d" => Some(Self::adapter_d()),
            "adapter_e" => Some(Self::adapter_e()),
            _ => None,
        }
    }

    fn fixture(&self, task_id: &str) -> Result<&FixtureTask, AdapterError> {
        self.tasks
            .get(task_id)
            .ok_or_else(|| AdapterError::UnknownTask(task_id.into()))
    }

    /// A fixture adapter from parts — the test/conformance constructor
    /// (e.g. the AC-R-2.9.4-6 tag-only-image suite).
    pub fn from_parts(
        adapter_id: &'static str,
        family: EnvironmentFamily,
        requires: BTreeMap<hh_ontology::lab::HandleCapability, hh_ontology::lab::Support>,
        tasks: Vec<FixtureTask>,
    ) -> FixtureAdapter {
        FixtureAdapter {
            adapter_id,
            family,
            requires,
            tasks: tasks
                .into_iter()
                .map(|t| (t.record.task_id.clone(), t))
                .collect(),
        }
    }
}

impl BenchmarkAdapter for FixtureAdapter {
    fn adapter_id(&self) -> &'static str {
        self.adapter_id
    }

    fn family(&self) -> EnvironmentFamily {
        self.family
    }

    fn task_ids(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }

    fn task(&self, task_id: &str) -> Result<BenchTask, AdapterError> {
        Ok(self.fixture(task_id)?.record.clone())
    }

    fn materialize(
        &self,
        task_id: &str,
        root: &str,
        verifier: bool,
    ) -> Result<EnvironmentHandle, AdapterError> {
        let task = self.fixture(task_id)?;
        // AC-R-2.9.4-12: the family's `requires` gate — a `required`
        // capability the environment declares `unsupported`/`unknown`
        // refuses `FamilyUnsupported{missing}` before any bytes land.
        crate::adapter::check_handle_requirements(&self.requires, &FsEnv::declared_support())?;
        let dir = Path::new(root).join(if verifier { "verifier" } else { "participant" });
        let _env = FsEnv::materialize(&dir, &task.record.environment)
            .map_err(|e| AdapterError::EnvironmentUnusable(e.to_string()))?;
        // Stage the held-out surface into the verifier root — the only place
        // it ever lands (never the participant root).
        if verifier {
            // Stage held-out bytes at the verifier env's root (the verifier's
            // own working directory — never the participant root).
            std::fs::write(dir.join("expected.bin"), &task.expected)
                .map_err(|e| AdapterError::EnvironmentUnusable(e.to_string()))?;
        }
        let mut h = EnvironmentHandle {
            handle_id: String::new(),
            task_id: task_id.into(),
            family: self.family,
            root: dir.to_string_lossy().into_owned(),
            network_mode: task.record.environment.network_mode,
            // AC-R-2.9.4-6: only a pinned digest populates the handle; a
            // tag-only task records `None` (and lands in `unpinned[]`).
            resolved_image_digest: crate::adapter::is_pinned_image_digest(
                &task.record.environment.image_digest,
            )
            .then(|| task.record.environment.image_digest.clone()),
            surface: if verifier {
                Surface::Instrument
            } else {
                Surface::Search
            },
        };
        h.handle_id = hh_identity::idp_id(
            "bench.env_handle",
            h.to_json().to_canonical_string().as_bytes(),
        );
        Ok(h)
    }

    fn expose(
        &self,
        env: &EnvironmentHandle,
        profile_ref: &str,
    ) -> Result<ExposedTask, AdapterError> {
        if env.surface != Surface::Search {
            // `expose` on a verifier/instrument handle is the held-out
            // boundary crossing — refuse outright.
            return Err(AdapterError::HeldOutBoundaryViolation(format!(
                "expose on a {:?} handle",
                env.surface
            )));
        }
        let task = self.fixture(&env.task_id)?;
        let fs = FsEnv {
            root: Path::new(&env.root).to_path_buf(),
            verifier_root: Path::new(&env.root).join("verifier"),
        };
        let instruction = task
            .record
            .visible
            .instruction
            .content
            .clone()
            .unwrap_or_default();
        fs.expose(&instruction, &[])
            .map_err(|e| AdapterError::EnvironmentUnusable(e.to_string()))?;
        Ok(ExposedTask {
            task_id: env.task_id.clone(),
            instruction,
            attachments: task.record.visible.attachments.clone(),
            metadata_scope: task.record.visible.task_metadata_scope.clone(),
            environment: env.clone(),
            profile_ref: profile_ref.into(),
            foreign_preserved: task.record.ext.clone(),
        })
    }

    fn collect_submission(&self, env: &EnvironmentHandle) -> Result<Submission, AdapterError> {
        if env.surface != Surface::Search {
            return Err(AdapterError::InstrumentOpFromParticipant(
                crate::adapter::AdapterOp::CollectSubmission,
            ));
        }
        let fs = FsEnv {
            root: Path::new(&env.root).to_path_buf(),
            verifier_root: Path::new(&env.root).join("verifier"),
        };
        let payload = fs
            .collect_submission()
            .map_err(|e| AdapterError::EnvironmentUnusable(e.to_string()))?;
        let mut s = match payload {
            Some(bytes) => {
                let applied = hh_wire::parse(&String::from_utf8_lossy(&bytes)).is_ok();
                Submission {
                    submission_id: String::new(),
                    task_id: env.task_id.clone(),
                    artifact_refs: vec![hh_identity::idp_id("bench.submission_bytes", &bytes)],
                    payload_hex: hex(&bytes),
                    applied,
                    apply_error: if applied {
                        None
                    } else {
                        Some("payload is not canonical JSON".into())
                    },
                }
            }
            None => Submission {
                submission_id: String::new(),
                task_id: env.task_id.clone(),
                artifact_refs: vec![],
                payload_hex: String::new(),
                applied: false,
                apply_error: Some("no submission.json produced".into()),
            },
        };
        s.submission_id = s.compute_id();
        Ok(s)
    }

    fn grade(&self, req: &GradeRequest) -> Result<GradeResult, GradeError> {
        // The fixture verifier: byte-exact the submission payload against the
        // staged expected answer, emit `{"reward": ppm}` on its stdout —
        // then the shared `grade()` pipeline does typed parsing + infra
        // classification. The process separation is `hh-bench-adapter`'s;
        // the bytes are identical either side of it.
        let task = self
            .fixture(&req.task.task_id)
            .map_err(|_| GradeError::GraderUnavailable("unknown task".into()))?;
        let actual = req
            .submission
            .payload_hex
            .as_str()
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.iter().collect::<String>())
            .filter_map(|h| u8::from_str_radix(&h, 16).ok())
            .collect::<Vec<u8>>();
        let reward = if req.submission.applied && actual == task.expected {
            1_000_000i64
        } else {
            0
        };
        let stdout = format!("{{\"reward\":{reward}}}");
        let signal = InfraSignal {
            verifier_exit: Some(0),
            verifier_stderr: "",
            verifier_timed_out: false,
            environment_error: None,
            submission_produced: req.submission.applied || !req.submission.payload_hex.is_empty(),
        };
        grade(req, &stdout, &signal)
    }
}

/// The adapter registry — `adapter_id → adapter`.
pub fn registry() -> BTreeMap<&'static str, FixtureAdapter> {
    [
        ("adapter_a", FixtureAdapter::adapter_a()),
        ("adapter_c", FixtureAdapter::adapter_c()),
        ("adapter_d", FixtureAdapter::adapter_d()),
        ("adapter_e", FixtureAdapter::adapter_e()),
    ]
    .into_iter()
    .collect()
}

/// The unused-import guard for the held-out hex helper (kept for the
/// import-side path — `unhex` decodes `oracle_solution` members).
#[allow(dead_code)]
fn held_out_bytes(t: &FixtureTask) -> Option<Vec<u8>> {
    t.record
        .held_out
        .oracle_solution
        .as_ref()
        .and_then(|j| j.as_str().map(str::to_string))
        .and_then(|s| unhex(&s))
}
