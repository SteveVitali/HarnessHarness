//! The **Lab vocabulary closed sums** (R-2.9.4⁰ᵃ, §5h.4; R-2.10.3⁰ᵃ, §6.3): the typed
//! enumerations shared across the measurement/Lab records — environment families, split
//! labels, contamination strata, the declared `handle.*` capability set, submission kinds,
//! verifier isolation, episode models, task validity states, and the environment network
//! mode. Every sum is closed: an unknown spelling is a typed refusal, never a
//! forward-compat guess (CC3 — nothing unaccounted).
//!
//! These live in `hh-ontology` (not `hh-lab`) because they are shared vocabulary: the
//! ledger manifest (`TaskRef.split_label`), the compliance metric declarations
//! (`applies_to_families`), the HIR debt scope (`task_classes`), and the Lab records all
//! reference the same closed sets — one schema source (CC7).

/// `EnvironmentFamily/1` — the closed environment-family sum (ADR-0142 D6): a suite's
/// tasks belong to one family. A new family is a dialect decision (the sum is closed);
/// a family the ontology does not declare is an `unknown-family` schema error, never a
/// forward-compat open arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnvironmentFamily {
    /// `coding_terminal` — the agent drives a shell/filesystem environment.
    CodingTerminal,
    /// `browser_computer` — browser/desktop control tasks.
    BrowserComputer,
    /// `search_research` — retrieval-grounded research tasks.
    SearchResearch,
    /// `structured_tool` — API/tool-call completion tasks.
    StructuredTool,
    /// `persistent_multi_episode` — multi-episode tasks whose state persists
    /// across agent sessions (long-term memory / learning suites).
    PersistentMultiEpisode,
    /// `adversarial_security` — adversarial/security evaluation tasks.
    AdversarialSecurity,
    /// `long_running_recovery` — long-horizon tasks with injected
    /// failures/recovery probes.
    LongRunningRecovery,
}

impl EnvironmentFamily {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EnvironmentFamily::CodingTerminal => "coding_terminal",
            EnvironmentFamily::BrowserComputer => "browser_computer",
            EnvironmentFamily::SearchResearch => "search_research",
            EnvironmentFamily::StructuredTool => "structured_tool",
            EnvironmentFamily::PersistentMultiEpisode => "persistent_multi_episode",
            EnvironmentFamily::AdversarialSecurity => "adversarial_security",
            EnvironmentFamily::LongRunningRecovery => "long_running_recovery",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EnvironmentFamily> {
        match s {
            "coding_terminal" => Some(EnvironmentFamily::CodingTerminal),
            "browser_computer" => Some(EnvironmentFamily::BrowserComputer),
            "search_research" => Some(EnvironmentFamily::SearchResearch),
            "structured_tool" => Some(EnvironmentFamily::StructuredTool),
            "persistent_multi_episode" => Some(EnvironmentFamily::PersistentMultiEpisode),
            "adversarial_security" => Some(EnvironmentFamily::AdversarialSecurity),
            "long_running_recovery" => Some(EnvironmentFamily::LongRunningRecovery),
            _ => None,
        }
    }

    /// Every declared family.
    pub const ALL: [EnvironmentFamily; 7] = [
        EnvironmentFamily::CodingTerminal,
        EnvironmentFamily::BrowserComputer,
        EnvironmentFamily::SearchResearch,
        EnvironmentFamily::StructuredTool,
        EnvironmentFamily::PersistentMultiEpisode,
        EnvironmentFamily::AdversarialSecurity,
        EnvironmentFamily::LongRunningRecovery,
    ];
}

/// `SplitLabel/1` — the closed split set (§5h.4 §3): the label a task/suite declares
/// about which partition of evaluation it belongs to. `search` is usable by context
/// search; `held_out`/`private`/`control` are never usable for search and never
/// enter an agent's context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SplitLabel {
    /// `dev` — development split; usable for calibration and iteration.
    Dev,
    /// `search` — the split search/config-selection may consult.
    Search,
    /// `held_out` — the primary unbiased estimate split; never in context.
    HeldOut,
    /// `public` — publicly scored/leaderboard split.
    Public,
    /// `private` — private leaderboard split; never in context.
    Private,
    /// `control` — canary/positive-control split.
    Control,
}

impl SplitLabel {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            SplitLabel::Dev => "dev",
            SplitLabel::Search => "search",
            SplitLabel::HeldOut => "held_out",
            SplitLabel::Public => "public",
            SplitLabel::Private => "private",
            SplitLabel::Control => "control",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<SplitLabel> {
        match s {
            "dev" => Some(SplitLabel::Dev),
            "search" => Some(SplitLabel::Search),
            "held_out" => Some(SplitLabel::HeldOut),
            "public" => Some(SplitLabel::Public),
            "private" => Some(SplitLabel::Private),
            "control" => Some(SplitLabel::Control),
            _ => None,
        }
    }

    /// Whether the split is admissible for context-search use (the
    /// `search`/`dev` splits; held-out/private/control never are).
    pub fn search_admissible(self) -> bool {
        matches!(self, SplitLabel::Dev | SplitLabel::Search)
    }
}

/// `ContaminationStratum` — the closed contamination set (§5h.4 §5): the
/// manifest-level contamination report is stratified by the training-cutoff claim
/// and a per-stratum exposure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContaminationStratum {
    /// `private_held_out` — private held-out material; lowest contamination.
    PrivateHeldOut,
    /// `fresh_temporal` — material created after the model's training cutoff.
    FreshTemporal,
    /// `public_dated` — dated public material (contamination depends on the
    /// model's declared cutoff).
    PublicDated,
    /// `contaminated_public` — public material known to appear in training
    /// corpora (used for discrimination checks, not headline estimates).
    ContaminatedPublic,
    /// `unknown` — the stratum could not be determined.
    Unknown,
}

impl ContaminationStratum {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ContaminationStratum::PrivateHeldOut => "private_held_out",
            ContaminationStratum::FreshTemporal => "fresh_temporal",
            ContaminationStratum::PublicDated => "public_dated",
            ContaminationStratum::ContaminatedPublic => "contaminated_public",
            ContaminationStratum::Unknown => "unknown",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ContaminationStratum> {
        match s {
            "private_held_out" => Some(ContaminationStratum::PrivateHeldOut),
            "fresh_temporal" => Some(ContaminationStratum::FreshTemporal),
            "public_dated" => Some(ContaminationStratum::PublicDated),
            "contaminated_public" => Some(ContaminationStratum::ContaminatedPublic),
            "unknown" => Some(ContaminationStratum::Unknown),
            _ => None,
        }
    }
}

/// `HandleCapability` — the closed handle-capability name set (§5h.4 §3): an
/// `EnvironmentFamilyRecord.requires` member declares the capabilities a
/// participant must surface for the family (e.g. `handle.bash`,
/// `handle.image_diff`). Members are names in this closed set, never open
/// strings — the ontology's `capability` home checks declared handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandleCapability {
    /// `handle.bash` — a shell handle.
    Bash,
    /// `handle.file` — a filesystem handle.
    File,
    /// `handle.browser` — a browser-control handle.
    Browser,
    /// `handle.computer` — a desktop-control handle.
    Computer,
    /// `handle.network` — a network handle.
    Network,
    /// `handle.clock` — a wall-clock handle.
    Clock,
    /// `handle.image` — an image-handle capability (screenshot/inspect).
    Image,
    /// `handle.image_diff` — an image-diff capability.
    ImageDiff,
    /// `handle.long_running` — long-running process handles.
    LongRunning,
    /// `handle.recovery` — recovery/fault-injection handles.
    Recovery,
}

impl HandleCapability {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            HandleCapability::Bash => "handle.bash",
            HandleCapability::File => "handle.file",
            HandleCapability::Browser => "handle.browser",
            HandleCapability::Computer => "handle.computer",
            HandleCapability::Network => "handle.network",
            HandleCapability::Clock => "handle.clock",
            HandleCapability::Image => "handle.image",
            HandleCapability::ImageDiff => "handle.image_diff",
            HandleCapability::LongRunning => "handle.long_running",
            HandleCapability::Recovery => "handle.recovery",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<HandleCapability> {
        match s {
            "handle.bash" => Some(HandleCapability::Bash),
            "handle.file" => Some(HandleCapability::File),
            "handle.browser" => Some(HandleCapability::Browser),
            "handle.computer" => Some(HandleCapability::Computer),
            "handle.network" => Some(HandleCapability::Network),
            "handle.clock" => Some(HandleCapability::Clock),
            "handle.image" => Some(HandleCapability::Image),
            "handle.image_diff" => Some(HandleCapability::ImageDiff),
            "handle.long_running" => Some(HandleCapability::LongRunning),
            "handle.recovery" => Some(HandleCapability::Recovery),
            _ => None,
        }
    }
}

/// `Support/1` — the tri-state capability declaration (§5h.4 §3): a
/// `requires` member's value. An open vocabulary would allow
/// `supported_when_convenient` — the closed sum does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Support {
    /// `required` — the participant must surface the capability.
    Required,
    /// `supported` — the participant may surface it.
    Supported,
    /// `unsupported` — the participant may not surface it for the family.
    Unsupported,
}

impl Support {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            Support::Required => "required",
            Support::Supported => "supported",
            Support::Unsupported => "unsupported",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Support> {
        match s {
            "required" => Some(Support::Required),
            "supported" => Some(Support::Supported),
            "unsupported" => Some(Support::Unsupported),
            _ => None,
        }
    }
}

/// `SubmissionKind/1` — how a participant submits answers for the family
/// (§5h.4 §3). The `verifier_isolation` default rule keys off it:
/// `separate` wherever `submission_kind ≠ none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubmissionKind {
    /// `none` — no formal submission channel (state judged in place).
    None,
    /// `patch` — a diff/patch submission.
    Patch,
    /// `answer` — a final textual/structured answer.
    Answer,
    /// `session` — a session-recording submission.
    Session,
    /// `trace` — an action-trace submission.
    Trace,
    /// `artifact` — a binary/file artifact submission.
    Artifact,
}

impl SubmissionKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            SubmissionKind::None => "none",
            SubmissionKind::Patch => "patch",
            SubmissionKind::Answer => "answer",
            SubmissionKind::Session => "session",
            SubmissionKind::Trace => "trace",
            SubmissionKind::Artifact => "artifact",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<SubmissionKind> {
        match s {
            "none" => Some(SubmissionKind::None),
            "patch" => Some(SubmissionKind::Patch),
            "answer" => Some(SubmissionKind::Answer),
            "session" => Some(SubmissionKind::Session),
            "trace" => Some(SubmissionKind::Trace),
            "artifact" => Some(SubmissionKind::Artifact),
            _ => None,
        }
    }
}

/// `VerifierIsolation/1` — the closed verifier-isolation set (§5h.4 §3):
/// where the verifier runs relative to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerifierIsolation {
    /// `separate` — the verifier runs outside the agent's reach.
    Separate,
    /// `shared` — the verifier shares the agent's plane (admissible only
    /// where the family declares it).
    Shared,
    /// `hosted` — the verifier runs in the hosted verifier plane.
    Hosted,
}

impl VerifierIsolation {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            VerifierIsolation::Separate => "separate",
            VerifierIsolation::Shared => "shared",
            VerifierIsolation::Hosted => "hosted",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<VerifierIsolation> {
        match s {
            "separate" => Some(VerifierIsolation::Separate),
            "shared" => Some(VerifierIsolation::Shared),
            "hosted" => Some(VerifierIsolation::Hosted),
            _ => None,
        }
    }
}

/// `EpisodeModel/1` — the closed episode-model set (§5h.4 §3): whether a task's
/// environment is fresh-per-episode or persists state across episodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EpisodeModel {
    /// `single_episode` — one episode, fresh environment.
    SingleEpisode,
    /// `fresh_per_episode` — multiple episodes, each a fresh environment.
    FreshPerEpisode,
    /// `persistent` — state persists across episodes.
    Persistent,
}

impl EpisodeModel {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EpisodeModel::SingleEpisode => "single_episode",
            EpisodeModel::FreshPerEpisode => "fresh_per_episode",
            EpisodeModel::Persistent => "persistent",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EpisodeModel> {
        match s {
            "single_episode" => Some(EpisodeModel::SingleEpisode),
            "fresh_per_episode" => Some(EpisodeModel::FreshPerEpisode),
            "persistent" => Some(EpisodeModel::Persistent),
            _ => None,
        }
    }
}

/// `TaskValidityState/1` — the closed validity-state set (§5h.4 §3): an
/// audit's verdict on a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskValidityState {
    /// `valid` — the task is admissible for headline estimates.
    Valid,
    /// `flawed` — the task is flawed; recorded (never silently dropped) and
    /// excluded from the headline set.
    Flawed,
    /// `retired` — the task is retired from the suite.
    Retired,
    /// `under_review` — the task's validity is being adjudicated.
    UnderReview,
}

impl TaskValidityState {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            TaskValidityState::Valid => "valid",
            TaskValidityState::Flawed => "flawed",
            TaskValidityState::Retired => "retired",
            TaskValidityState::UnderReview => "under_review",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<TaskValidityState> {
        match s {
            "valid" => Some(TaskValidityState::Valid),
            "flawed" => Some(TaskValidityState::Flawed),
            "retired" => Some(TaskValidityState::Retired),
            "under_review" => Some(TaskValidityState::UnderReview),
            _ => None,
        }
    }
}

/// `BenchmarkNetworkMode/1` — the closed environment-network-mode set
/// (§5h.4 §3): the network posture a task's environment declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BenchmarkNetworkMode {
    /// `none` — no network access.
    None,
    /// `simulated` — a simulated/sandboxed network.
    Simulated,
    /// `restricted` — an allow-listed restricted network.
    Restricted,
    /// `open` — an open network.
    Open,
}

impl BenchmarkNetworkMode {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            BenchmarkNetworkMode::None => "none",
            BenchmarkNetworkMode::Simulated => "simulated",
            BenchmarkNetworkMode::Restricted => "restricted",
            BenchmarkNetworkMode::Open => "open",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<BenchmarkNetworkMode> {
        match s {
            "none" => Some(BenchmarkNetworkMode::None),
            "simulated" => Some(BenchmarkNetworkMode::Simulated),
            "restricted" => Some(BenchmarkNetworkMode::Restricted),
            "open" => Some(BenchmarkNetworkMode::Open),
            _ => None,
        }
    }
}

// ── EnvironmentFamilyRecord ─────────────────────────────────────────────────

/// `EnvironmentFamilyRecord{family_id, version, requires, oracle_classes_available,
/// submission_kind, verifier_isolation_default, process_metrics[],
/// fault_profiles_admissible[], perturbation_profiles_admissible[],
/// episode_model, provenance}` — the registered `environment_family` record
/// body (§5h.4; R-2.9.4⁰ᵃ row 2; ADR-0142). The ontology owns the schema the
/// way it owns `MetricDeclaration` (CC7); `hh-registry` carries the record
/// variant.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentFamilyRecord {
    /// The closed family id.
    pub family_id: EnvironmentFamily,
    /// The record's version (data at C0).
    pub version: String,
    /// `requires: HandleRequirements` — `map<HandleCapability, Support>`; every
    /// key is a closed-set member.
    pub requires: std::collections::BTreeMap<HandleCapability, Support>,
    /// The oracle classes available in this family.
    pub oracle_classes_available: Vec<String>,
    /// What the suite submits (per-family constant).
    pub submission_kind: SubmissionKind,
    /// The family's verifier-isolation default.
    pub verifier_isolation_default: VerifierIsolation,
    /// The process metrics the family emits.
    pub process_metrics: Vec<String>,
    /// The fault profiles admissible in this family.
    pub fault_profiles_admissible: Vec<String>,
    /// The perturbation profiles admissible in this family.
    pub perturbation_profiles_admissible: Vec<String>,
    /// The family's episode model.
    pub episode_model: EpisodeModel,
    /// The record's provenance (mandatory).
    pub provenance: hh_provenance::ProvenanceRecord,
}

/// `EnvironmentFamilyRecord` schema errors (typed, never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum FamilyError {
    /// A member-level schema violation.
    SchemaViolation {
        /// The member that failed.
        member: String,
        /// The failure detail.
        detail: String,
    },
    /// `submission_kind ≠ none` while `oracle_classes_available` is empty —
    /// a family that accepts submissions must declare a verifier.
    MissingOracle,
}

impl EnvironmentFamilyRecord {
    /// Whether the family declares `shared` verifier isolation admissible —
    /// the declaration a task's `verifier_isolation = shared` requires
    /// (§5h.4's default rule: `separate` wherever `submission_kind ≠ none`;
    /// `shared` admissible only where the family declares it).
    pub fn declares_shared(&self) -> bool {
        self.verifier_isolation_default == VerifierIsolation::Shared
    }

    /// The record's schema checks (the `register` admission gate's half).
    pub fn validate(&self) -> Result<(), FamilyError> {
        if self.submission_kind != SubmissionKind::None && self.oracle_classes_available.is_empty()
        {
            return Err(FamilyError::MissingOracle);
        }
        Ok(())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> hh_wire::Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert(
            "family_id".into(),
            hh_wire::Json::str(self.family_id.name()),
        );
        m.insert("version".into(), hh_wire::Json::str(&self.version));
        m.insert(
            "requires".into(),
            hh_wire::Json::Obj(
                self.requires
                    .iter()
                    .map(|(k, v)| (k.name().to_string(), hh_wire::Json::str(v.name())))
                    .collect(),
            ),
        );
        m.insert(
            "oracle_classes_available".into(),
            hh_wire::Json::Arr(
                self.oracle_classes_available
                    .iter()
                    .map(hh_wire::Json::str)
                    .collect(),
            ),
        );
        m.insert(
            "submission_kind".into(),
            hh_wire::Json::str(self.submission_kind.name()),
        );
        m.insert(
            "verifier_isolation_default".into(),
            hh_wire::Json::str(self.verifier_isolation_default.name()),
        );
        m.insert(
            "process_metrics".into(),
            hh_wire::Json::Arr(
                self.process_metrics
                    .iter()
                    .map(hh_wire::Json::str)
                    .collect(),
            ),
        );
        m.insert(
            "fault_profiles_admissible".into(),
            hh_wire::Json::Arr(
                self.fault_profiles_admissible
                    .iter()
                    .map(hh_wire::Json::str)
                    .collect(),
            ),
        );
        m.insert(
            "perturbation_profiles_admissible".into(),
            hh_wire::Json::Arr(
                self.perturbation_profiles_admissible
                    .iter()
                    .map(hh_wire::Json::str)
                    .collect(),
            ),
        );
        m.insert(
            "episode_model".into(),
            hh_wire::Json::str(self.episode_model.name()),
        );
        m.insert("provenance".into(), self.provenance.to_json());
        hh_wire::Json::Obj(m)
    }

    /// Strict decode — unknown members, spellings and member types refuse.
    pub fn from_json(j: &hh_wire::Json) -> Result<EnvironmentFamilyRecord, FamilyError> {
        const REC: &str = "EnvironmentFamilyRecord";
        let m = match j {
            hh_wire::Json::Obj(m) => m,
            _ => {
                return Err(FamilyError::SchemaViolation {
                    member: REC.to_string(),
                    detail: "must be a JSON object".to_string(),
                })
            }
        };
        for k in m.keys() {
            if ![
                "family_id",
                "version",
                "requires",
                "oracle_classes_available",
                "submission_kind",
                "verifier_isolation_default",
                "process_metrics",
                "fault_profiles_admissible",
                "perturbation_profiles_admissible",
                "episode_model",
                "provenance",
            ]
            .contains(&k.as_str())
            {
                return Err(FamilyError::SchemaViolation {
                    member: k.clone(),
                    detail: format!("unknown member of {REC}"),
                });
            }
        }
        let str_at = |member: &str| -> Result<&str, FamilyError> {
            m.get(member)
                .and_then(hh_wire::Json::as_str)
                .ok_or_else(|| FamilyError::SchemaViolation {
                    member: member.to_string(),
                    detail: format!("{REC}.{member} must be a string"),
                })
        };
        let str_vec_at = |member: &str| -> Result<Vec<String>, FamilyError> {
            match m.get(member) {
                Some(hh_wire::Json::Arr(a)) => a
                    .iter()
                    .map(|j| {
                        j.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| FamilyError::SchemaViolation {
                                member: member.to_string(),
                                detail: format!("{REC}.{member}[] must be strings"),
                            })
                    })
                    .collect(),
                _ => Err(FamilyError::SchemaViolation {
                    member: member.to_string(),
                    detail: format!("{REC}.{member} must be an array"),
                }),
            }
        };
        let mut requires = std::collections::BTreeMap::new();
        match m.get("requires") {
            Some(hh_wire::Json::Obj(rm)) => {
                for (k, v) in rm {
                    let cap =
                        HandleCapability::parse(k).ok_or_else(|| FamilyError::SchemaViolation {
                            member: "requires".to_string(),
                            detail: format!("unknown handle capability `{k}`"),
                        })?;
                    let sup = v.as_str().and_then(Support::parse).ok_or_else(|| {
                        FamilyError::SchemaViolation {
                            member: "requires".to_string(),
                            detail: format!("unknown support value for `{k}`"),
                        }
                    })?;
                    requires.insert(cap, sup);
                }
            }
            _ => {
                return Err(FamilyError::SchemaViolation {
                    member: "requires".to_string(),
                    detail: "must be an object".to_string(),
                })
            }
        }
        Ok(EnvironmentFamilyRecord {
            family_id: EnvironmentFamily::parse(str_at("family_id")?).ok_or_else(|| {
                FamilyError::SchemaViolation {
                    member: "family_id".to_string(),
                    detail: "unknown environment family".to_string(),
                }
            })?,
            version: str_at("version")?.to_string(),
            requires,
            oracle_classes_available: str_vec_at("oracle_classes_available")?,
            submission_kind: SubmissionKind::parse(str_at("submission_kind")?).ok_or_else(
                || FamilyError::SchemaViolation {
                    member: "submission_kind".to_string(),
                    detail: "unknown submission kind".to_string(),
                },
            )?,
            verifier_isolation_default: VerifierIsolation::parse(str_at(
                "verifier_isolation_default",
            )?)
            .ok_or_else(|| FamilyError::SchemaViolation {
                member: "verifier_isolation_default".to_string(),
                detail: "unknown verifier isolation".to_string(),
            })?,
            process_metrics: str_vec_at("process_metrics")?,
            fault_profiles_admissible: str_vec_at("fault_profiles_admissible")?,
            perturbation_profiles_admissible: str_vec_at("perturbation_profiles_admissible")?,
            episode_model: EpisodeModel::parse(str_at("episode_model")?).ok_or_else(|| {
                FamilyError::SchemaViolation {
                    member: "episode_model".to_string(),
                    detail: "unknown episode model".to_string(),
                }
            })?,
            provenance: hh_provenance::ProvenanceRecord::from_json(
                m.get("provenance")
                    .ok_or_else(|| FamilyError::SchemaViolation {
                        member: "provenance".to_string(),
                        detail: "missing member".to_string(),
                    })?,
            )
            .map_err(|e| FamilyError::SchemaViolation {
                member: "provenance".to_string(),
                detail: format!("{e:?}"),
            })?,
        })
    }
}
