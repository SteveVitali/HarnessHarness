//! `benchset` — the hermetic Stage-3 `benchmarkSet` loader
//! (ticket S3.12b; GATE-G2 gap G2-1; spec §10.3/§5h.4; R-2.9.4⁰ᵇ).
//!
//! The committed corpus at `fixtures/benchset/stage3_v1/` is the Stage-3
//! reference suite: four strata (A `suite.tb2` / `coding_terminal` /
//! `fresh_temporal`; C `suite.swebench` / `contaminated_public` +
//! `retired_for_headline` — the SWE-bench-class control of ADR-0146 D3;
//! D `suite.taubench` / `structured_tool`; E `suite.agentdojo` /
//! `adversarial_security`), each task carrying its recorded `model_io`
//! transcript and the original runner's recorded verdicts
//! (`original_runs` — the parity compare's `original` arm, ≥ 3 per task
//! on the suite's declared `parity_subset`).
//!
//! Loading is *integration, never authoring* (N13; ADR-0006): the loader
//! reads committed canonical facts and derives the `TaskRecord`s,
//! `SuiteManifest` (split hash recomputed, L2 epoch-seeded dispatch
//! recorded), `SplitAssignmentRecord` and the bound `FixtureAdapter` —
//! `manifest.validate()`/`record.validate()`/`split_assignment.validate()`
//! all run at load, so a corrupted corpus refuses rather than degrades.
//!
//! Corpus layout:
//! ```text
//! benchset.json                     — {schema: benchset/1, benchset_id, suites{key → suite_id}}
//! <suite_id>/suite.json             — benchset_suite/1 decl
//! <suite_id>/tasks/<name>.json      — benchset_task/1 decl (visible/held-out facts + recorded runs)
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hh_identity::idp_digest;
use hh_lab::bench::ForeignTaskId;
use hh_lab::bench::{split_hash, SplitAssignmentRecord, SuiteManifest, SuiteValidityRecord};
use hh_lab::model::ForeignRef;
use hh_ontology::lab::{
    ContaminationStratum, EnvironmentFamily, EpisodeModel, HandleCapability, SplitLabel, Support,
    VerifierIsolation,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::adapters::{fixture_task, FixtureAdapter};
use crate::records::BenchTask;

/// The committed corpus id — `LEDGER.md`'s `benchmarkSet` names this.
pub const BENCHSET_ID: &str = "benchset.stage3.v1";

/// The corpus directory inside `hh-bench`.
pub const BENCHSET_DIR: &str = "fixtures/benchset/stage3_v1";

/// The committed corpus root (`<hh-bench>/fixtures/benchset/stage3_v1`).
pub fn default_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BENCHSET_DIR)
}

/// Loader failures — typed, never a warning (a malformed corpus is a
/// refusal, not a partial suite).
#[derive(Debug, PartialEq)]
pub enum BenchsetError {
    /// A corpus file could not be read.
    Io {
        /// The path.
        path: String,
        /// The OS error.
        detail: String,
    },
    /// A corpus file failed schema shape (missing/wrong-typed member).
    Malformed {
        /// The path.
        path: String,
        /// What failed.
        detail: String,
    },
    /// An unlisted member — the corpus schema is closed.
    UnknownMember {
        /// The path.
        path: String,
        /// The member name.
        member: String,
    },
    /// The corpus contradicts itself (model_io never produces `expected`,
    /// a `parity_subset` name without ≥ 3 recorded runs, a split member
    /// absent from `tasks`, …).
    Inconsistent {
        /// The path.
        path: String,
        /// What diverged.
        detail: String,
    },
    /// A `TaskRecord` failed `TaskRecord::validate`.
    Task(hh_lab::bench::TaskError),
    /// A `SuiteManifest`/`SplitAssignmentRecord` failed its checks.
    Suite(hh_lab::bench::SuiteError),
    /// A shared codec refused.
    Schema(hh_lab::json_util::SchemaError),
}

impl std::fmt::Display for BenchsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchsetError::Io { path, detail } => write!(f, "io({path}): {detail}"),
            BenchsetError::Malformed { path, detail } => {
                write!(f, "malformed({path}): {detail}")
            }
            BenchsetError::UnknownMember { path, member } => {
                write!(f, "unknown_member({path}): {member}")
            }
            BenchsetError::Inconsistent { path, detail } => {
                write!(f, "inconsistent({path}): {detail}")
            }
            BenchsetError::Task(e) => write!(f, "task: {e:?}"),
            BenchsetError::Suite(e) => write!(f, "suite: {e:?}"),
            BenchsetError::Schema(e) => write!(f, "schema: {e:?}"),
        }
    }
}

impl std::error::Error for BenchsetError {}

impl From<hh_lab::bench::TaskError> for BenchsetError {
    fn from(e: hh_lab::bench::TaskError) -> Self {
        BenchsetError::Task(e)
    }
}
impl From<hh_lab::bench::SuiteError> for BenchsetError {
    fn from(e: hh_lab::bench::SuiteError) -> Self {
        BenchsetError::Suite(e)
    }
}
impl From<hh_lab::json_util::SchemaError> for BenchsetError {
    fn from(e: hh_lab::json_util::SchemaError) -> Self {
        BenchsetError::Schema(e)
    }
}

/// One recorded model turn — the surface call the recorded `model_io`
/// emitted (`{surface, args}`; the replay drives them verbatim through the
/// real driver, the gate applies them to the materialized env).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    /// The surface id (`fs.write`, `hh.submit`, …).
    pub surface: String,
    /// The call's argument record.
    pub args: Json,
}

/// One recorded original-runner verdict — the foreign runner's committed
/// row for this task/replicate (the parity compare's `original` arm input).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedRun {
    /// The recorded `task_success` reward (ppm).
    pub reward_ppm: i64,
    /// The recorded `model_calls` consumption.
    pub model_calls: i64,
}

/// A corpus task — the imported `TaskRecord` plus the two recorded
/// surfaces the corpus carries (`model_io` + `original_runs`).
#[derive(Debug, Clone)]
pub struct BenchsetTask {
    /// The task's committed name (corpus-local key).
    pub name: String,
    /// The imported record + expected submission bytes.
    pub fixture: crate::adapters::FixtureTask,
    /// The recorded `model_io` transcript (replayed verbatim).
    pub model_io: Vec<RecordedCall>,
    /// The original runner's recorded verdicts (the parity `original` arm).
    pub original_runs: Vec<RecordedRun>,
}

impl BenchsetTask {
    /// The task record.
    pub fn record(&self) -> &BenchTask {
        &self.fixture.record
    }
    /// The expected submission bytes (`held_out.oracle_solution`'s mirror).
    pub fn expected(&self) -> &[u8] {
        &self.fixture.expected
    }
}

/// One loaded suite — its `SuiteManifest`, split assignment, bound
/// `FixtureAdapter`, and task rows.
#[derive(Debug)]
pub struct BenchSuite {
    /// The stratum key (`a` | `c` | `d` | `e`).
    pub key: String,
    /// The derived `SuiteManifest` (`validate()`d at load).
    pub manifest: SuiteManifest,
    /// The derived `SplitAssignmentRecord` (`validate()`d at load).
    pub split_assignment: SplitAssignmentRecord,
    /// The adapter bound to this suite's family/tasks.
    pub adapter: FixtureAdapter,
    /// The task rows (committed `name` order — deterministic).
    pub tasks: Vec<BenchsetTask>,
    /// The declared parity subset (task *names*; AC-R-2.9.4-3's
    /// pre-registered subset — every member carries ≥ 3 recorded
    /// original-runner verdicts).
    pub parity_subset: Vec<String>,
}

impl BenchSuite {
    /// The suite id (`manifest.suite_id`).
    pub fn suite_id(&self) -> &str {
        &self.manifest.suite_id
    }
    /// The suite's environment family.
    pub fn family(&self) -> EnvironmentFamily {
        self.manifest.primary_family
    }
    /// The contamination stratum the suite declares.
    pub fn stratum(&self) -> ContaminationStratum {
        self.manifest.contamination_default
    }
    /// Whether the suite is retired for headline (AC-R-2.9.4-11's label).
    pub fn retired_for_headline(&self) -> bool {
        self.manifest.validity.retired_for_headline
    }
    /// Task rows on a split label (the expand-time `suite_tasks` input).
    pub fn tasks_on(&self, split: SplitLabel) -> Vec<&BenchsetTask> {
        self.tasks
            .iter()
            .filter(|t| t.record().split_label == split)
            .collect()
    }
    /// The task row by committed name.
    pub fn task_named(&self, name: &str) -> Option<&BenchsetTask> {
        self.tasks.iter().find(|t| t.name == name)
    }
    /// The task row by semantic `task_id`.
    pub fn task(&self, task_id: &str) -> Option<&BenchsetTask> {
        self.tasks.iter().find(|t| t.record().task_id == task_id)
    }
}

/// The loaded `benchmarkSet` — the committed corpus's in-memory form.
#[derive(Debug)]
pub struct Benchset {
    /// The corpus id (`benchset.stage3.v1`).
    pub id: String,
    /// The directory the corpus was read from.
    pub dir: PathBuf,
    /// `stratum key → suite` (deterministic order).
    pub suites: BTreeMap<String, BenchSuite>,
}

// ── strict-JSON helpers ─────────────────────────────────────────────────

fn members<'a>(
    j: &'a Json,
    allowed: &[&str],
    path: &str,
) -> Result<&'a BTreeMap<String, Json>, BenchsetError> {
    let m = match j {
        Json::Obj(m) => m,
        _ => {
            return Err(BenchsetError::Malformed {
                path: path.into(),
                detail: "not an object".into(),
            })
        }
    };
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(BenchsetError::UnknownMember {
                path: path.into(),
                member: k.clone(),
            });
        }
    }
    Ok(m)
}

fn req<'a>(m: &'a BTreeMap<String, Json>, k: &str, path: &str) -> Result<&'a Json, BenchsetError> {
    m.get(k).ok_or_else(|| BenchsetError::Malformed {
        path: path.into(),
        detail: format!("missing `{k}`"),
    })
}

fn req_str(m: &BTreeMap<String, Json>, k: &str, path: &str) -> Result<String, BenchsetError> {
    req(m, k, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| BenchsetError::Malformed {
            path: path.into(),
            detail: format!("`{k}` not a string"),
        })
}

fn req_int(m: &BTreeMap<String, Json>, k: &str, path: &str) -> Result<i64, BenchsetError> {
    req(m, k, path)?
        .as_int()
        .ok_or_else(|| BenchsetError::Malformed {
            path: path.into(),
            detail: format!("`{k}` not an int"),
        })
}

fn read_json(path: &Path) -> Result<Json, BenchsetError> {
    let bytes = std::fs::read(path).map_err(|e| BenchsetError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let text = String::from_utf8(bytes).map_err(|e| BenchsetError::Malformed {
        path: path.display().to_string(),
        detail: format!("utf8: {e}"),
    })?;
    hh_wire::parse(&text).map_err(|e| BenchsetError::Malformed {
        path: path.display().to_string(),
        detail: format!("json: {e}"),
    })
}

fn kernel_prov(tag: &str) -> ProvenanceRecord {
    ProvenanceRecord::kernel(format!("hh-bench/benchset/{tag}"), 0)
}

// ── task load ────────────────────────────────────────────────────────────

fn load_task(
    suite_dir: &Path,
    suite_id: &str,
    adapter_id: &str,
    family: EnvironmentFamily,
    stratum: ContaminationStratum,
    isolation: Option<VerifierIsolation>,
    episode: Option<EpisodeModel>,
    name: &str,
) -> Result<BenchsetTask, BenchsetError> {
    let path = suite_dir.join("tasks").join(format!("{name}.json"));
    let ps = path.display().to_string();
    let j = read_json(&path)?;
    let m = members(
        &j,
        &[
            "schema",
            "name",
            "split",
            "instruction",
            "expected",
            "model_io",
            "original_runs",
        ],
        &ps,
    )?;
    if req_str(m, "schema", &ps)? != "benchset_task/1" {
        return Err(BenchsetError::Malformed {
            path: ps,
            detail: "schema ≠ benchset_task/1".into(),
        });
    }
    if req_str(m, "name", &ps)? != name {
        return Err(BenchsetError::Inconsistent {
            path: ps,
            detail: format!("name member ≠ file name {name}"),
        });
    }
    let split =
        SplitLabel::parse(&req_str(m, "split", &ps)?).ok_or_else(|| BenchsetError::Malformed {
            path: ps.clone(),
            detail: "unknown split label".into(),
        })?;
    let instruction = req_str(m, "instruction", &ps)?;
    let expected = req_str(m, "expected", &ps)?.into_bytes();

    let model_io: Vec<RecordedCall> = match req(m, "model_io", &ps)? {
        Json::Arr(turns) => turns
            .iter()
            .map(|t| {
                let tm = members(t, &["surface", "args"], &ps)?;
                Ok(RecordedCall {
                    surface: req_str(tm, "surface", &ps)?,
                    args: req(tm, "args", &ps)?.clone(),
                })
            })
            .collect::<Result<_, BenchsetError>>()?,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "model_io not an array".into(),
            })
        }
    };

    // The transcript must produce the submission the corpus declares —
    // an `fs.write` to `submission.json` carrying exactly `expected`
    // followed by an `hh.submit` (the recorded run completes).
    let wrote_expected = model_io.iter().any(|c| {
        c.surface == "fs.write"
            && c.args.get("path").and_then(Json::as_str) == Some("submission.json")
            && c.args
                .get("text")
                .and_then(Json::as_str)
                .map(|s| s.as_bytes() == expected.as_slice())
                .unwrap_or(false)
    });
    if !wrote_expected {
        return Err(BenchsetError::Inconsistent {
            path: ps,
            detail: "model_io never writes `expected` to submission.json".into(),
        });
    }
    if model_io.last().map(|c| c.surface.as_str()) != Some("hh.submit") {
        return Err(BenchsetError::Inconsistent {
            path: ps,
            detail: "model_io does not end in hh.submit".into(),
        });
    }

    let original_runs: Vec<RecordedRun> = match req(m, "original_runs", &ps)? {
        Json::Arr(rs) => rs
            .iter()
            .map(|r| {
                let rm = members(r, &["reward_ppm", "model_calls"], &ps)?;
                Ok(RecordedRun {
                    reward_ppm: req_int(rm, "reward_ppm", &ps)?,
                    model_calls: req_int(rm, "model_calls", &ps)?,
                })
            })
            .collect::<Result<_, BenchsetError>>()?,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "original_runs not an array".into(),
            })
        }
    };

    let fixture = fixture_task(
        adapter_id,
        family,
        &format!("{suite_id}/{name}"),
        split,
        stratum,
        &instruction,
        &expected,
        isolation,
        episode,
    );
    fixture.record.validate()?;
    Ok(BenchsetTask {
        name: name.into(),
        fixture,
        model_io,
        original_runs,
    })
}

// ── suite load ───────────────────────────────────────────────────────────

fn load_suite(dir: &Path, key: &str, suite_id: &str) -> Result<BenchSuite, BenchsetError> {
    let suite_dir = dir.join(suite_id);
    let spath = suite_dir.join("suite.json");
    let ps = spath.display().to_string();
    let j = read_json(&spath)?;
    let m = members(
        &j,
        &[
            "schema",
            "suite_id",
            "adapter_id",
            "family",
            "contamination_stratum",
            "retired_for_headline",
            "verifier_isolation",
            "episode_model",
            "requires",
            "flawed_task_ids",
            "parity_subset",
            "tasks",
        ],
        &ps,
    )?;
    if req_str(m, "schema", &ps)? != "benchset_suite/1" {
        return Err(BenchsetError::Malformed {
            path: ps,
            detail: "schema ≠ benchset_suite/1".into(),
        });
    }
    if req_str(m, "suite_id", &ps)? != suite_id {
        return Err(BenchsetError::Inconsistent {
            path: ps,
            detail: format!("suite_id member ≠ directory {suite_id}"),
        });
    }
    let adapter_id = req_str(m, "adapter_id", &ps)?;
    let family = EnvironmentFamily::parse(&req_str(m, "family", &ps)?).ok_or_else(|| {
        BenchsetError::Malformed {
            path: ps.clone(),
            detail: "unknown family".into(),
        }
    })?;
    let stratum = ContaminationStratum::parse(&req_str(m, "contamination_stratum", &ps)?)
        .ok_or_else(|| BenchsetError::Malformed {
            path: ps.clone(),
            detail: "unknown contamination_stratum".into(),
        })?;
    let retired = match req(m, "retired_for_headline", &ps)? {
        Json::Bool(b) => *b,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "retired_for_headline not a bool".into(),
            })
        }
    };
    let isolation = match m.get("verifier_isolation") {
        None | Some(Json::Null) => None,
        Some(Json::Str(s)) => {
            Some(
                VerifierIsolation::parse(s).ok_or_else(|| BenchsetError::Malformed {
                    path: ps.clone(),
                    detail: "unknown verifier_isolation".into(),
                })?,
            )
        }
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "verifier_isolation not a string".into(),
            })
        }
    };
    let episode = match m.get("episode_model") {
        None | Some(Json::Null) => None,
        Some(Json::Str(s)) => {
            Some(
                EpisodeModel::parse(s).ok_or_else(|| BenchsetError::Malformed {
                    path: ps.clone(),
                    detail: "unknown episode_model".into(),
                })?,
            )
        }
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "episode_model not a string".into(),
            })
        }
    };
    let mut requires = BTreeMap::new();
    match req(m, "requires", &ps)? {
        Json::Obj(rm) => {
            for (k, v) in rm {
                let cap = HandleCapability::parse(k).ok_or_else(|| BenchsetError::Malformed {
                    path: ps.clone(),
                    detail: format!("unknown handle capability {k}"),
                })?;
                let sup = v.as_str().and_then(Support::parse).ok_or_else(|| {
                    BenchsetError::Malformed {
                        path: ps.clone(),
                        detail: format!("unknown support level for {k}"),
                    }
                })?;
                requires.insert(cap, sup);
            }
        }
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "requires not an object".into(),
            })
        }
    }
    let flawed: Vec<String> = match m.get("flawed_task_ids") {
        None => vec![],
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| BenchsetError::Malformed {
                        path: ps.clone(),
                        detail: "flawed_task_ids member not a string".into(),
                    })
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "flawed_task_ids not an array".into(),
            })
        }
    };
    let parity_subset: Vec<String> = match m.get("parity_subset") {
        None => vec![],
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| BenchsetError::Malformed {
                        path: ps.clone(),
                        detail: "parity_subset member not a string".into(),
                    })
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "parity_subset not an array".into(),
            })
        }
    };
    let task_names: Vec<String> = match req(m, "tasks", &ps)? {
        Json::Arr(a) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| BenchsetError::Malformed {
                        path: ps.clone(),
                        detail: "tasks member not a string".into(),
                    })
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "tasks not an array".into(),
            })
        }
    };

    let mut tasks = Vec::with_capacity(task_names.len());
    for name in &task_names {
        tasks.push(load_task(
            &suite_dir,
            suite_id,
            &adapter_id,
            family,
            stratum,
            isolation,
            episode,
            name,
        )?);
    }

    // AC-R-2.9.4-3's n ≥ 3 per task on the pre-registered parity subset —
    // a subset member without three recorded original-runner verdicts
    // refuses at load, never at the comparison.
    for name in &parity_subset {
        let t =
            tasks
                .iter()
                .find(|t| &t.name == name)
                .ok_or_else(|| BenchsetError::Inconsistent {
                    path: ps.clone(),
                    detail: format!("parity_subset member {name} not in tasks[]"),
                })?;
        if t.original_runs.len() < 3 {
            return Err(BenchsetError::Inconsistent {
                path: ps.clone(),
                detail: format!("parity_subset member {name} has < 3 original_runs"),
            });
        }
    }

    let task_ids: Vec<String> = tasks.iter().map(|t| t.record().task_id.clone()).collect();
    // `flawed_task_ids` names suite members (semantic ids); resolve the
    // committed names through the loaded rows.
    let flawed_ids: Vec<String> = flawed
        .iter()
        .map(|name| {
            tasks
                .iter()
                .find(|t| &t.name == name || t.record().task_id == *name)
                .map(|t| t.record().task_id.clone())
                .ok_or_else(|| BenchsetError::Inconsistent {
                    path: ps.clone(),
                    detail: format!("flawed_task_ids member {name} not in tasks[]"),
                })
        })
        .collect::<Result<_, _>>()?;
    let split_map: BTreeMap<String, SplitLabel> = tasks
        .iter()
        .map(|t| (t.record().task_id.clone(), t.record().split_label))
        .collect();

    let mut manifest = SuiteManifest {
        suite_id: String::new(),
        foreign: ForeignTaskId {
            name: suite_id.into(),
            version: BENCHSET_ID.into(),
            source_ref: format!("fixture://benchset/{BENCHSET_ID}/{suite_id}"),
            digest_claim: ForeignRef {
                system: "fixture".into(),
                digest: idp_digest("bench.suite", suite_id.as_bytes()),
                label: Some(format!("{BENCHSET_ID}:{suite_id}")),
                provenance: kernel_prov(suite_id),
            },
        },
        primary_family: family,
        tasks: task_ids,
        split_map,
        split_hash: String::new(),
        validity: SuiteValidityRecord {
            audit_ref: format!("audit://{BENCHSET_ID}/{suite_id}"),
            audited_at: Some(0),
            epoch_seeded: true,
            dispatch_ref: Some(format!("dispatch://{BENCHSET_ID}/{suite_id}")),
            flawed_task_ids: flawed_ids,
            noise_ceiling: None,
            retired_for_headline: retired,
            reason: retired.then(|| format!("{BENCHSET_ID} control stratum")),
        },
        contamination_default: stratum,
        adapter_ref: adapter_id.clone(),
        parity_report: None,
        provenance: kernel_prov(suite_id),
    };
    manifest.split_hash = split_hash(&manifest.split_map);
    manifest.suite_id = manifest.suite_id();
    manifest.validate()?;

    let split_assignment = SplitAssignmentRecord {
        suite_id: manifest.suite_id.clone(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: manifest.split_map.clone(),
        split_hash: manifest.split_hash.clone(),
        registered_at: 0,
    };
    split_assignment.validate()?;

    let adapter = FixtureAdapter::from_parts(
        // `from_parts` needs a 'static id — intern the four corpus ids.
        match adapter_id.as_str() {
            "adapter_a" => "adapter_a",
            "adapter_c" => "adapter_c",
            "adapter_d" => "adapter_d",
            "adapter_e" => "adapter_e",
            other => {
                return Err(BenchsetError::Malformed {
                    path: ps,
                    detail: format!("unknown adapter id {other}"),
                })
            }
        },
        family,
        requires,
        tasks.iter().map(|t| t.fixture.clone()).collect(),
    );

    Ok(BenchSuite {
        key: key.into(),
        manifest,
        split_assignment,
        adapter,
        tasks,
        parity_subset,
    })
}

impl Benchset {
    /// Load the committed corpus at `default_dir()`.
    pub fn load_default() -> Result<Benchset, BenchsetError> {
        Self::load(&default_dir())
    }

    /// Load a corpus directory: `benchset.json` + one `<suite_id>/` dir
    /// per declared stratum. Everything validates at load — a corrupt or
    /// internally inconsistent corpus refuses.
    pub fn load(dir: &Path) -> Result<Benchset, BenchsetError> {
        let bpath = dir.join("benchset.json");
        let ps = bpath.display().to_string();
        let j = read_json(&bpath)?;
        let m = members(&j, &["schema", "benchset_id", "suites"], &ps)?;
        if req_str(m, "schema", &ps)? != "benchset/1" {
            return Err(BenchsetError::Malformed {
                path: ps,
                detail: "schema ≠ benchset/1".into(),
            });
        }
        let id = req_str(m, "benchset_id", &ps)?;
        if id != BENCHSET_ID {
            return Err(BenchsetError::Inconsistent {
                path: ps,
                detail: format!("benchset_id {id} ≠ {BENCHSET_ID}"),
            });
        }
        let mut suites = BTreeMap::new();
        match req(m, "suites", &ps)? {
            Json::Obj(sm) => {
                for (key, v) in sm {
                    let suite_id = v.as_str().ok_or_else(|| BenchsetError::Malformed {
                        path: ps.clone(),
                        detail: format!("suites.{key} not a string"),
                    })?;
                    suites.insert(key.clone(), load_suite(dir, key, suite_id)?);
                }
            }
            _ => {
                return Err(BenchsetError::Malformed {
                    path: ps,
                    detail: "suites not an object".into(),
                })
            }
        }
        if suites.len() != 4 {
            return Err(BenchsetError::Inconsistent {
                path: ps,
                detail: "the Stage-3 set declares exactly four strata (a/c/d/e)".into(),
            });
        }
        Ok(Benchset {
            id,
            dir: dir.to_path_buf(),
            suites,
        })
    }

    /// The suite bound to a stratum key (`a` | `c` | `d` | `e`).
    pub fn suite(&self, key: &str) -> Option<&BenchSuite> {
        self.suites.get(key)
    }
}
