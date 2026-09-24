//! R-2.2.4⁰ᵇ — the replay driver's ledger half: `ReplayValidityReport`,
//! the nondeterminism-recording fold (ADR-0135 §2), the
//! `lifecycle.replay.started/finished` emitters, and the `reconstruct`
//! driver (rebuild views + resolve the environment binding; nothing is
//! re-executed — ADR-0028 §1).
//!
//! The `deterministic` driver half lives in `hh-control::replay` (the
//! control loop is the thing re-driven); this module owns the *validity
//! verdict* — a pure fold over the branch record, the source prefix and
//! the caller-supplied probe evidence — plus the report's canonical form.
//!
//! Verdict ladder (§5a.4):
//!
//! * `deterministic` — the variant declares `deterministic_replay`, every
//!   consumed source is `confined_to_events`, the fork point is coherent,
//!   no fingerprint mismatch, and the recorded `control.decision` /
//!   `context.assembled.view_hash` sequences were reproduced (the driver
//!   supplies the reproduction verdict — this fold computes the record's
//!   eligibility).
//! * `re_executed` — coherent and snapshot-coupled, but some source is
//!   unrecorded or the variant does not declare deterministic replay.
//! * `degraded` — world movement, fingerprint mismatch, contradictory
//!   probes, or uncovered environment reads. Never hidden: `degraded`
//!   names its `reasons` and is excluded from artifact-benefit rows.
//! * `invalid` — incoherent fork point, non-read-only execution on a
//!   `trace_only` branch, replay divergence, or an intervention recorded
//!   before the fork point.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::branch::{BranchRecord, EnvBinding};
use crate::errors::LedgerError;
use crate::event::EventEnvelope;
use crate::store::Store;
use crate::views::{View, ViewKind};

/// `replay(run|branch, driver_mode ∈ {reconstruct, deterministic})` — the
/// requested driver mode (§5a.4 op table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayDriverMode {
    /// Rebuild views + the environment binding; nothing re-executes.
    Reconstruct,
    /// Feed recorded responses/observations/decisions to the declaring
    /// variant; no effect dispatches.
    Deterministic,
}

impl ReplayDriverMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayDriverMode::Reconstruct => "reconstruct",
            ReplayDriverMode::Deterministic => "deterministic",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ReplayDriverMode> {
        match s {
            "reconstruct" => Some(ReplayDriverMode::Reconstruct),
            "deterministic" => Some(ReplayDriverMode::Deterministic),
            _ => None,
        }
    }
}

/// `ReplayValidityReport.mode` — the four-valued verdict (§5a.4; ADR-0135).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidityMode {
    /// Exactly reproducible — the declaration, confinement and the
    /// reproduction check all hold.
    Deterministic,
    /// Coherent and snapshot-coupled but re-executed: some source is
    /// unrecorded or the variant does not declare deterministic replay.
    ReExecuted,
    /// Reproduced with caveats — world movement, fingerprint mismatch,
    /// contradictory probes or uncovered environment reads; the reasons
    /// name each one. Never hidden, never counted as an artifact benefit.
    Degraded,
    /// Not a valid replay — incoherent fork point, non-read-only
    /// execution on a `trace_only` branch, divergence, or an
    /// intervention recorded before the fork point.
    Invalid,
}

impl ValidityMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ValidityMode::Deterministic => "deterministic",
            ValidityMode::ReExecuted => "re_executed",
            ValidityMode::Degraded => "degraded",
            ValidityMode::Invalid => "invalid",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ValidityMode> {
        match s {
            "deterministic" => Some(ValidityMode::Deterministic),
            "re_executed" => Some(ValidityMode::ReExecuted),
            "degraded" => Some(ValidityMode::Degraded),
            "invalid" => Some(ValidityMode::Invalid),
            _ => None,
        }
    }
}

/// `SourceKind` — the closed nondeterminism-source list (ADR-0135 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceKind {
    /// Provider sampling — the `model.call.*` record (request/response
    /// refs, parsed calls, snapshot, fingerprint).
    ProviderSampling,
    /// Wall clock — `control.clock.read` rows.
    WallClock,
    /// Recorded randomness — `control.random.read` rows / `manifest.seed`.
    Randomness,
    /// Network — `context.observation.recorded` coverage of egress.
    Network,
    /// Filesystem order — the kernel-derived fs-diff capture.
    FilesystemOrder,
    /// Concurrency — the single-writer/lease discipline record.
    Concurrency,
    /// Environment state — the bound env + snapshot/probe record.
    EnvironmentState,
    /// Human input — `security.permission.decided{decider: human}` (the
    /// recorded decision is *substituted* at replay — `substituted = true`).
    HumanInput,
    /// External service — `context.observation.recorded`/host-effect rows.
    ExternalService,
}

impl SourceKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::ProviderSampling => "provider_sampling",
            SourceKind::WallClock => "wall_clock",
            SourceKind::Randomness => "randomness",
            SourceKind::Network => "network",
            SourceKind::FilesystemOrder => "filesystem_order",
            SourceKind::Concurrency => "concurrency",
            SourceKind::EnvironmentState => "environment_state",
            SourceKind::HumanInput => "human_input",
            SourceKind::ExternalService => "external_service",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<SourceKind> {
        Some(match s {
            "provider_sampling" => SourceKind::ProviderSampling,
            "wall_clock" => SourceKind::WallClock,
            "randomness" => SourceKind::Randomness,
            "network" => SourceKind::Network,
            "filesystem_order" => SourceKind::FilesystemOrder,
            "concurrency" => SourceKind::Concurrency,
            "environment_state" => SourceKind::EnvironmentState,
            "human_input" => SourceKind::HumanInput,
            "external_service" => SourceKind::ExternalService,
            _ => return None,
        })
    }
}

/// `sources[]` — one record per consumed source: whether every read is
/// event-confined (recorded) and whether replay substitutes the recorded
/// value (`decider = human` rows, ADR-0135 §1).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRecord {
    /// The source.
    pub source: SourceKind,
    /// Every consumed read is recorded (event-confined).
    pub confined_to_events: bool,
    /// The recorded value is substituted at replay (human decisions).
    pub substituted: bool,
}

impl SourceRecord {
    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("source", Json::str(self.source.as_str())),
            ("confined_to_events", Json::Bool(self.confined_to_events)),
            ("substituted", Json::Bool(self.substituted)),
        ])
    }

    /// Fold a member back.
    pub fn from_json(j: &Json) -> Option<SourceRecord> {
        Some(SourceRecord {
            source: SourceKind::parse(j.get("source")?.as_str()?)?,
            confined_to_events: matches!(j.get("confined_to_events"), Some(Json::Bool(true))),
            substituted: matches!(j.get("substituted"), Some(Json::Bool(true))),
        })
    }
}

/// `external_dependencies[]` — one probe verdict per recorded external
/// dependency (the driver probes; the verdict is evidence, never content).
#[derive(Debug, Clone, PartialEq)]
pub struct DependencyProbe {
    /// The dependency (service/capability spelling).
    pub service: String,
    /// `agree | contradict | unreachable` — a `contradict` degrades.
    pub probed_verdict: String,
}

impl DependencyProbe {
    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("service", Json::str(&self.service)),
            ("probed_verdict", Json::str(&self.probed_verdict)),
        ])
    }
}

/// `environment{…}` — the binding, coverage and uncaptured reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnvironmentReport {
    /// The bound snapshot (env = snapshot), when present.
    pub snapshot_ref: Option<String>,
    /// `live | snapshot | trace_only | none` — the branch's env binding
    /// (a plain run's is `live`).
    pub binding: String,
    /// The run's declared `observability_level`.
    pub coverage: Vec<String>,
    /// Sources the environment consumed but the record does not capture.
    pub uncaptured: Vec<String>,
}

/// `model{…}` — the snapshot + fingerprint evidence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelReport {
    /// The recorded model snapshot, when present.
    pub snapshot_ref: Option<String>,
    /// `Some(false)` is a fingerprint mismatch — `degraded`.
    pub fingerprint_match: Option<bool>,
}

/// `ReplayValidityReport` — the §5a.4 record (canonical JSON; the content
/// address is the `report_ref` `lifecycle.replay.finished` carries).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayValidityReport {
    /// The run the replay covered (the branch for a branch replay).
    pub run_id: String,
    /// The branch id, when the target is a branch.
    pub branch_id: Option<String>,
    /// The fork point the replay anchored at, when branch-scoped.
    pub fork_point: Option<u64>,
    /// The verdict.
    pub mode: ValidityMode,
    /// The recorded variant's `deterministic_replay` declaration.
    pub variant_declares_deterministic_replay: bool,
    /// One record per consumed source.
    pub sources: Vec<SourceRecord>,
    /// The environment evidence.
    pub environment: EnvironmentReport,
    /// The model evidence.
    pub model: ModelReport,
    /// The external-dependency probe verdicts.
    pub external_dependencies: Vec<DependencyProbe>,
    /// Sources the trace–environment coupling covers (ADR-0135 §3).
    pub coupled_sources: Vec<String>,
    /// The coupling agreement (`Some(false)` ⇒ degraded).
    pub coupling_agreement: Option<bool>,
    /// Every reason the verdict is not stronger (closed spellings).
    pub reasons: Vec<String>,
}

impl ReplayValidityReport {
    /// The canonical member form — the `report_ref` blob content.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("run_id".into(), Json::str(&self.run_id));
        if let Some(b) = &self.branch_id {
            m.insert("branch_id".into(), Json::str(b));
        }
        if let Some(fp) = self.fork_point {
            m.insert("fork_point".into(), Json::Int(fp as i64));
        }
        m.insert("mode".into(), Json::str(self.mode.as_str()));
        m.insert(
            "variant_declares_deterministic_replay".into(),
            Json::Bool(self.variant_declares_deterministic_replay),
        );
        m.insert(
            "sources".into(),
            Json::Arr(self.sources.iter().map(SourceRecord::to_json).collect()),
        );
        let mut env = BTreeMap::new();
        env.insert("binding".into(), Json::str(&self.environment.binding));
        env.insert(
            "coverage".into(),
            Json::Arr(
                self.environment
                    .coverage
                    .iter()
                    .map(|c| Json::str(c.clone()))
                    .collect(),
            ),
        );
        env.insert(
            "uncaptured".into(),
            Json::Arr(
                self.environment
                    .uncaptured
                    .iter()
                    .map(|c| Json::str(c.clone()))
                    .collect(),
            ),
        );
        if let Some(s) = &self.environment.snapshot_ref {
            env.insert("snapshot_ref".into(), Json::str(s));
        }
        m.insert("environment".into(), Json::Obj(env));
        let mut model = BTreeMap::new();
        if let Some(s) = &self.model.snapshot_ref {
            model.insert("snapshot_ref".into(), Json::str(s));
        }
        if let Some(fm) = self.model.fingerprint_match {
            model.insert("fingerprint_match".into(), Json::Bool(fm));
        }
        m.insert("model".into(), Json::Obj(model));
        m.insert(
            "external_dependencies".into(),
            Json::Arr(
                self.external_dependencies
                    .iter()
                    .map(DependencyProbe::to_json)
                    .collect(),
            ),
        );
        m.insert(
            "coupled_sources".into(),
            Json::Arr(
                self.coupled_sources
                    .iter()
                    .map(|c| Json::str(c.clone()))
                    .collect(),
            ),
        );
        if let Some(ca) = self.coupling_agreement {
            m.insert("coupling_agreement".into(), Json::Bool(ca));
        }
        m.insert(
            "reasons".into(),
            Json::Arr(self.reasons.iter().map(|r| Json::str(r.clone())).collect()),
        );
        Json::Obj(m)
    }

    /// `report_ref` — the content address of the canonical document
    /// (deterministic across rebuilds — AC-R-2.2.4-11).
    pub fn content_ref(&self) -> String {
        let bytes = self.to_json().to_canonical_string();
        hh_identity::idp::address(bytes.as_bytes(), "application/hh.replay-validity+json").id()
    }

    /// Fold a report back out of its canonical document (the rebuild path —
    /// AC-R-2.2.4-11's "rebuilds from the ledger alone").
    pub fn from_json(j: &Json) -> Option<ReplayValidityReport> {
        let env = j.get("environment")?;
        let model = j.get("model")?;
        Some(ReplayValidityReport {
            run_id: j.get("run_id")?.as_str()?.to_string(),
            branch_id: j
                .get("branch_id")
                .and_then(Json::as_str)
                .map(str::to_string),
            fork_point: j.get("fork_point").and_then(Json::as_int).map(|i| i as u64),
            mode: ValidityMode::parse(j.get("mode")?.as_str()?)?,
            variant_declares_deterministic_replay: matches!(
                j.get("variant_declares_deterministic_replay"),
                Some(Json::Bool(true))
            ),
            sources: arr_of(j.get("sources"))
                .iter()
                .map(SourceRecord::from_json)
                .collect::<Option<Vec<_>>>()?,
            environment: EnvironmentReport {
                snapshot_ref: env
                    .get("snapshot_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                binding: env.get("binding")?.as_str()?.to_string(),
                coverage: env
                    .get("coverage")
                    .map(|j| arr_of(Some(j)))
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                uncaptured: env
                    .get("uncaptured")
                    .map(|j| arr_of(Some(j)))
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            model: ModelReport {
                snapshot_ref: model
                    .get("snapshot_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                fingerprint_match: model
                    .get("fingerprint_match")
                    .and_then(|j| bool_of(Some(j))),
            },
            external_dependencies: j
                .get("external_dependencies")
                .map(|j| arr_of(Some(j)))
                .map(|a| {
                    a.iter()
                        .filter_map(|d| {
                            Some(DependencyProbe {
                                service: d.get("service")?.as_str()?.to_string(),
                                probed_verdict: d.get("probed_verdict")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            coupled_sources: j
                .get("coupled_sources")
                .map(|j| arr_of(Some(j)))
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            coupling_agreement: j.get("coupling_agreement").and_then(|j| bool_of(Some(j))),
            reasons: j
                .get("reasons")
                .map(|j| arr_of(Some(j)))
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// The evidence the record alone cannot carry — the caller (the replay
/// driver at the boundary) supplies it.
#[derive(Debug, Clone, Default)]
pub struct ValidityInput {
    /// The recorded variant's `deterministic_replay` declaration (read off
    /// the sealed definition by the boundary — the ledger never re-reads
    /// content, CC2).
    pub variant_declares: bool,
    /// The model fingerprint evidence — `Some(false)` is a mismatch
    /// (`degraded`); `None` = no fingerprint claim was recorded.
    pub fingerprint_match: Option<bool>,
    /// The external-dependency probe verdicts the driver ran.
    pub probes: Vec<DependencyProbe>,
    /// Sources the caller knows the environment consumed but the record
    /// does not capture (uncaptured reads degrade the verdict).
    pub uncaptured: Vec<String>,
    /// The reproduction outcome — `Some((at_seq, expected, got))` when the
    /// deterministic drive diverged (`invalid`); `Some`-absent + the
    /// driver reports reproduction ⇒ `deterministic` is claimable.
    pub diverged: Option<(u64, String, String)>,
    /// Whether the drive reproduced the recorded `control.decision` /
    /// `view_hash` sequences (only meaningful for `deterministic` driver
    /// mode — a `reconstruct` replay never claims `deterministic`).
    pub reproduced: Option<bool>,
}

/// `Json::Arr` items — the wire `Json` has no `as_arr`; match it here.
fn arr_of(j: Option<&Json>) -> Vec<Json> {
    match j {
        Some(Json::Arr(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// `Json::Bool` — the wire `Json` has no `as_bool`; match it here.
fn bool_of(j: Option<&Json>) -> Option<bool> {
    match j {
        Some(Json::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// `validity(store, run_id, input)` — the pure fold (§5a.4; ADR-0135 §3):
/// the record's confinement/coupling/coherence assessment, combined with
/// the caller's probe/fingerprint/divergence evidence into the verdict.
/// Rebuildable — two calls over the same prefix return equal
/// `content_ref`s (AC-R-2.2.4-11).
pub fn validity(
    store: &Store,
    run_id: &str,
    input: &ValidityInput,
) -> Result<ReplayValidityReport, LedgerError> {
    let events = store.envelopes(run_id)?;
    let manifest = store.manifest(run_id)?.clone();
    let folds = store.effect_folds(run_id)?;

    let mut reasons: Vec<String> = Vec::new();
    let mut uncaptured: BTreeSet<String> = input.uncaptured.iter().cloned().collect();

    // ── branch record + coherence ────────────────────────────────────────
    let branch = branch_record(store, run_id)?;
    let mut branch_invalid = false;
    let mut coupling_agreement: Option<bool> = None;
    let mut coupled_sources: Vec<String> = Vec::new();
    if let Some(b) = &branch {
        if store.check_fork_point(&b.source_run_id, b.at_seq).is_err() {
            branch_invalid = true;
            reasons.push("fork_point_not_coherent".to_string());
        }
        // `trace_only` ⇒ read-only: a committed non-read-only effect in the
        // branch's own suffix is an `invalid` replay target (§5a.4).
        if b.env == EnvBinding::TraceOnly {
            for (effect_id, fold) in &folds {
                if fold.phase.past_commit() && !fold.risk_class.is_read_only() {
                    branch_invalid = true;
                    reasons.push(format!(
                        "trace_only_effect:{effect_id}: non-read-only effect executed"
                    ));
                }
            }
        }
        // Atomic trace–environment coupling (ADR-0135 §3): `env = snapshot`
        // couples `environment_state` when the snapshot resolves;
        // `trace_only` couples nothing mutable.
        match b.env {
            EnvBinding::Snapshot => match &b.snapshot_ref {
                Some(sref) => match store.blob_status(sref) {
                    crate::audit::BlobStatus::Present => {
                        coupled_sources.push(SourceKind::EnvironmentState.as_str().to_string());
                        coupling_agreement = Some(true);
                    }
                    _ => {
                        coupling_agreement = Some(false);
                        reasons.push("snapshot_unresolvable".to_string());
                    }
                },
                None => {
                    coupling_agreement = Some(false);
                    reasons.push("snapshot_missing".to_string());
                }
            },
            EnvBinding::TraceOnly => {
                coupling_agreement = Some(true);
            }
            _ => {}
        }
    }

    // ── sources — confinement per ADR-0135 §2 ────────────────────────────
    let has_class = |c: &str| events.iter().any(|e| e.class == c);
    let mut sources: Vec<SourceRecord> = Vec::new();
    // provider_sampling — confined when every completed call carries the
    // recorded outcome the deterministic driver feeds (response_ref +
    // parsed `calls`; the `calls` member is the S3.6 recording-rule
    // extension — older runs without it are re_executed, not silently
    // claimed deterministic).
    let completed_calls: Vec<&EventEnvelope> = events
        .iter()
        .filter(|e| e.class == "model.call.completed")
        .collect();
    let model_confined = completed_calls
        .iter()
        .all(|e| e.payload.get("calls").is_some() && e.payload.get("response_ref").is_some())
        && (!completed_calls.is_empty() || !has_class("model.call.requested"));
    let model_present = has_class("model.call.requested");
    if model_present {
        sources.push(SourceRecord {
            source: SourceKind::ProviderSampling,
            confined_to_events: model_confined,
            substituted: false,
        });
        if !model_confined {
            reasons.push("model_io_unrecorded".to_string());
        }
    }
    // wall_clock / randomness — the declaration binds the recording rule:
    // a declaring variant's reads land `control.clock.read` /
    // `control.random.read`; a non-declaring variant's reads were served
    // ephemerally (unrecorded ⇒ unconfined when any reads are possible —
    // we cannot prove none happened, so absence of the declaration is the
    // honest signal).
    for (kind, class) in [
        (SourceKind::WallClock, "control.clock.read"),
        (SourceKind::Randomness, "control.random.read"),
    ] {
        let recorded = has_class(class);
        let confined = input.variant_declares || recorded;
        if !confined || recorded {
            sources.push(SourceRecord {
                source: kind,
                confined_to_events: confined,
                substituted: false,
            });
            if !confined {
                uncaptured.insert(kind.as_str().to_string());
            }
        }
    }
    // human_input — every `decided{decider: human}` is substituted at
    // replay (ADR-0135 §1); confined by construction (the row exists).
    if events.iter().any(|e| {
        e.class == "security.permission.decided"
            && e.payload.get("decider") == Some(&Json::str("human"))
    }) {
        sources.push(SourceRecord {
            source: SourceKind::HumanInput,
            confined_to_events: true,
            substituted: true,
        });
    }
    // environment_state — consumed when effects ran; captured when the
    // run has a bound environment record (env rows or a branch snapshot).
    let ran_effects = folds.iter().any(|(_, f)| f.phase.past_commit());
    let env_recorded = has_class("action.environment.bound")
        || has_class("action.environment.attached")
        || branch
            .as_ref()
            .map(|b| b.snapshot_ref.is_some())
            .unwrap_or(false);
    if ran_effects {
        sources.push(SourceRecord {
            source: SourceKind::EnvironmentState,
            confined_to_events: env_recorded,
            substituted: false,
        });
        if !env_recorded {
            uncaptured.insert(SourceKind::EnvironmentState.as_str().to_string());
            reasons.push("environment_state_uncaptured".to_string());
        }
    }
    // network / external_service — confined when every committed effect
    // carrying a network/external domain has a `context.observation.
    // recorded` row (the observation record is the captured source).
    let observations: BTreeSet<String> = events
        .iter()
        .filter(|e| e.class == "context.observation.recorded")
        .filter_map(|e| {
            e.payload
                .get("effect_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    let mut net_unconfined = false;
    for (effect_id, fold) in &folds {
        if fold.phase.past_commit()
            && fold.risk_class.scope == hh_ontology::risk::RiskScope::External
            && !observations.contains(effect_id)
        {
            net_unconfined = true;
            uncaptured.insert(SourceKind::Network.as_str().to_string());
            uncaptured.insert(SourceKind::ExternalService.as_str().to_string());
        }
    }
    if net_unconfined {
        sources.push(SourceRecord {
            source: SourceKind::Network,
            confined_to_events: false,
            substituted: false,
        });
        sources.push(SourceRecord {
            source: SourceKind::ExternalService,
            confined_to_events: false,
            substituted: false,
        });
        reasons.push("external_observation_unrecorded".to_string());
    } else if !observations.is_empty() {
        sources.push(SourceRecord {
            source: SourceKind::Network,
            confined_to_events: true,
            substituted: false,
        });
        sources.push(SourceRecord {
            source: SourceKind::ExternalService,
            confined_to_events: true,
            substituted: false,
        });
    }
    // filesystem_order — the kernel-derived fs diff capture is
    // deterministic (ordering is never executor-supplied): confined.
    // concurrency — single-writer lease discipline confines it at C0.
    sources.push(SourceRecord {
        source: SourceKind::FilesystemOrder,
        confined_to_events: true,
        substituted: false,
    });
    sources.push(SourceRecord {
        source: SourceKind::Concurrency,
        confined_to_events: true,
        substituted: false,
    });

    // ── verdict ──────────────────────────────────────────────────────────
    let probes_contradict = input
        .probes
        .iter()
        .any(|p| p.probed_verdict == "contradict");
    let fingerprint_mismatch = input.fingerprint_match == Some(false);
    let coupling_failed = input
        .probes
        .iter()
        .any(|p| p.probed_verdict == "contradict")
        || coupling_agreement == Some(false);

    let all_confined = sources.iter().all(|s| s.confined_to_events);
    let mode = if branch_invalid || input.diverged.is_some() {
        if input.diverged.is_some() {
            reasons.push("replay_diverged".to_string());
        }
        ValidityMode::Invalid
    } else if fingerprint_mismatch || probes_contradict || coupling_failed {
        if fingerprint_mismatch {
            reasons.push("fingerprint_mismatch".to_string());
        }
        if probes_contradict {
            reasons.push("contradicting_probe".to_string());
        }
        if coupling_agreement == Some(false) {
            reasons.push("coupling_disagreement".to_string());
        }
        ValidityMode::Degraded
    } else if !uncaptured.is_empty() {
        reasons.push("uncaptured_sources".to_string());
        ValidityMode::Degraded
    } else if input.variant_declares && all_confined {
        // `deterministic` is claimable only when the drive reproduced —
        // a `reconstruct` run or a not-yet-driven check reports the
        // record's eligibility as `re_executed`-eligible, never the
        // reproduction claim it cannot evidence (ADR-0135 §3 honesty).
        match input.reproduced {
            Some(true) => ValidityMode::Deterministic,
            Some(false) => {
                reasons.push("replay_diverged".to_string());
                ValidityMode::Invalid
            }
            None => ValidityMode::ReExecuted,
        }
    } else {
        if !input.variant_declares {
            reasons.push("variant_not_declaring".to_string());
        }
        if !all_confined {
            reasons.push("unconfined_sources".to_string());
        }
        ValidityMode::ReExecuted
    };

    Ok(ReplayValidityReport {
        run_id: run_id.to_string(),
        branch_id: branch.as_ref().map(|b| b.branch_id.clone()),
        fork_point: branch.as_ref().map(|b| b.at_seq),
        mode,
        variant_declares_deterministic_replay: input.variant_declares,
        sources,
        environment: EnvironmentReport {
            snapshot_ref: branch.as_ref().and_then(|b| b.snapshot_ref.clone()),
            binding: branch
                .as_ref()
                .map(|b| b.env.as_str().to_string())
                .unwrap_or_else(|| "live".to_string()),
            coverage: manifest
                .observability_level
                .iter()
                .map(|o| o.as_str().to_string())
                .collect(),
            uncaptured: uncaptured.iter().cloned().collect(),
        },
        model: ModelReport {
            snapshot_ref: arr_of(manifest.extra.get("model_snapshots"))
                .first()
                .and_then(|v| v.as_str().map(str::to_string)),
            fingerprint_match: input.fingerprint_match,
        },
        external_dependencies: input.probes.clone(),
        coupled_sources,
        coupling_agreement,
        reasons,
    })
}

/// The branch record the run's own `lifecycle.run.forked` row carries —
/// `None` for a non-branch run.
pub fn branch_record(store: &Store, run_id: &str) -> Result<Option<BranchRecord>, LedgerError> {
    let events = store.envelopes(run_id)?;
    Ok(events
        .iter()
        .find(|e| e.class == "lifecycle.run.forked")
        .and_then(|e| BranchRecord::from_payload(run_id, &e.payload)))
}

/// `lifecycle.replay.started{driver_mode, target_run, until?}` — kernel
/// bookkeeping on the target run (the branch for a branch replay).
pub fn replay_started(
    store: &mut Store,
    target_run: &str,
    driver_mode: ReplayDriverMode,
    until: Option<u64>,
) -> Result<EventEnvelope, LedgerError> {
    let mut m = BTreeMap::new();
    m.insert("driver_mode".into(), Json::str(driver_mode.as_str()));
    m.insert("target_run".into(), Json::str(target_run));
    if let Some(u) = until {
        m.insert("until".into(), Json::Int(u as i64));
    }
    store.commit_kernel_row(
        target_run,
        "lifecycle.replay.started",
        Json::Obj(m),
        vec![],
        vec![],
    )
}

/// `lifecycle.replay.finished{driver_mode, mode, report_ref}` — the report
/// blob is stored content-addressed and the row names it (`report_ref`).
pub fn replay_finished(
    store: &mut Store,
    target_run: &str,
    driver_mode: ReplayDriverMode,
    report: &ReplayValidityReport,
) -> Result<EventEnvelope, LedgerError> {
    let bytes = report.to_json().to_canonical_string();
    let report_ref = store
        .put_blob(bytes.as_bytes(), "application/hh.replay-validity+json")?
        .id();
    let mut m = BTreeMap::new();
    m.insert("driver_mode".into(), Json::str(driver_mode.as_str()));
    m.insert("mode".into(), Json::str(report.mode.as_str()));
    m.insert("report_ref".into(), Json::str(&report_ref));
    store.commit_kernel_row(
        target_run,
        "lifecycle.replay.finished",
        Json::Obj(m),
        vec![],
        vec![],
    )
}

/// `Reconstructed` — the `reconstruct` driver's product: the rebuilt
/// views (each carries `view_hash` — AC-R-2.2.4-11's rebuild check) and
/// the resolved environment binding.
#[derive(Debug, Clone)]
pub struct Reconstructed {
    /// The rebuilt views (`context_view`, `run_summary`, `effect_ledger`,
    /// `checkpoint`, `branch_tree`, `effects_by_key`).
    pub views: Vec<View>,
    /// The resolved env snapshot ref (`env = snapshot` branches).
    pub env_snapshot_ref: Option<String>,
    /// The `context.assembled` view_hash sequence the rebuild reproduced —
    /// one entry per recorded assembly row, in order (empty when the
    /// assembler did not record `view_hash` members).
    pub view_hash_sequence: Vec<String>,
}

/// `reconstruct` — rebuild the views + resolve the environment binding
/// over `run`'s prefix (`until` bounds it); nothing re-executes
/// (ADR-0028 §1). The rebuilt `view_hash` set is the rebuildability
/// evidence; `view_hash_sequence` carries the recorded
/// `context.assembled` hashes the caller compares.
pub fn reconstruct(
    store: &Store,
    run_id: &str,
    until: Option<u64>,
) -> Result<Reconstructed, LedgerError> {
    let events = store.envelopes(run_id)?;
    let tip = events.last().map(|e| e.seq);
    let until = until.or(tip);
    let kinds = [
        ViewKind::ContextView,
        ViewKind::RunSummary,
        ViewKind::EffectLedger,
        ViewKind::Checkpoint,
        ViewKind::BranchTree,
        ViewKind::EffectsByKey,
    ];
    let mut views = Vec::new();
    for k in kinds {
        views.push(store.project(run_id, k, until)?);
    }
    let branch = branch_record(store, run_id)?;
    let env_snapshot_ref = match branch.as_ref().map(|b| b.env) {
        Some(EnvBinding::Snapshot) => match branch.as_ref().and_then(|b| b.snapshot_ref.clone()) {
            Some(sref) => match store.blob_status(&sref) {
                crate::audit::BlobStatus::Present => Some(sref),
                status => {
                    return Err(LedgerError::SnapshotUnavailable {
                        kind: "fs_tree".to_string(),
                        detail: format!("snapshot {sref} is {status:?}"),
                    })
                }
            },
            None => {
                return Err(LedgerError::SnapshotUnavailable {
                    kind: "fs_tree".to_string(),
                    detail: "env = snapshot binding carries no snapshot_ref".to_string(),
                })
            }
        },
        _ => None,
    };
    let view_hash_sequence = events
        .iter()
        .filter(|e| e.class == "context.assembled" && until.map(|u| e.seq <= u).unwrap_or(true))
        .filter_map(|e| {
            e.payload
                .get("view_hash")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    Ok(Reconstructed {
        views,
        env_snapshot_ref,
        view_hash_sequence,
    })
}

/// `effects_by_key` — the §5a.2 C0 dedup projection is
/// [`crate::views::effects_by_key_view`] (`ViewKind::EffectsByKey`); this
/// helper resolves one key against the live fold — the dispatch-time
/// consult (ADR-0102 §10).
///
/// Returns `(prior_effect_id, phase, outcome)` for the effect that
/// recorded `key`, if any.
pub fn key_lookup(
    store: &Store,
    run_id: &str,
    key: &str,
) -> Result<
    Option<(
        String,
        crate::effect::EffectPhase,
        Option<crate::effect::ObservedOutcome>,
    )>,
    LedgerError,
> {
    for (effect_id, fold) in store.effect_folds(run_id)? {
        if fold.idempotency_key.as_deref() == Some(key) {
            return Ok(Some((effect_id, fold.phase, fold.outcome)));
        }
    }
    Ok(None)
}

/// The stored observation a `commit`-replay serves — the recorded
/// `observed` payload for `effect_id`'s latest applied attempt, or `None`.
pub fn stored_observation(
    store: &Store,
    run_id: &str,
    effect_id: &str,
) -> Result<Option<Json>, LedgerError> {
    let events = store.envelopes(run_id)?;
    Ok(events
        .iter()
        .rev()
        .find(|e| {
            e.class == "action.effect.observed" && e.scope.effect_id.as_deref() == Some(effect_id)
        })
        .map(|e| e.payload.clone()))
}
