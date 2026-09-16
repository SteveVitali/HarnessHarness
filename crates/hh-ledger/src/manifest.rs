//! The immutable **run manifest** — the payload of `lifecycle.run.created` at seq 0
//! (ADR-0026 §1). The typed fields are the §5a.1 §3 manifest vocabulary; the spec's
//! trailing `…` lands in `extra` (additive; unknown keys are preserved, never dropped —
//! CC3 — but an `ext` marker on `run_kind` is refused, CF-385).
//!
//! `open_run` validates the §5a.1 invariant row: `participant_class`,
//! `observability_level`, `configuration_id`, `configuration_version_id`, `idp`,
//! `run_kind` present — with the ADR-0183 §C carve-out that only `agent` runs carry a
//! configuration — plus the pinned-ref rule (a mutable tag is `UnresolvedRef`,
//! ADR-0137) and the lineage-anchor checks (`UnknownForkPoint` / `SourceIncomplete` /
//! `ForkPointNotCoherent`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::errors::LedgerError;
use crate::ids::is_pinned_id;

/// `run_kind ∈ {agent, experiment, surface, inbox, fleet}` (ADR-0183 §C.1; ADR-0208
/// CF-441). Only `agent` runs drive a harness and carry a configuration; the others
/// hold their own fenced writer and never enter subject aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RunKind {
    /// Drives a harness; carries `configuration_id`/`configuration_version_id`.
    Agent,
    /// An experiment-side run (ADR-0155).
    Experiment,
    /// A `serve`d tool surface run (ADR-0174).
    Surface,
    /// A goal's inbox run (ADR-0131).
    Inbox,
    /// The fleet run (ADR-0205).
    Fleet,
}

impl RunKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RunKind::Agent => "agent",
            RunKind::Experiment => "experiment",
            RunKind::Surface => "surface",
            RunKind::Inbox => "inbox",
            RunKind::Fleet => "fleet",
        }
    }

    /// Parse the canonical spelling; `ext`/unknown refused (CF-385).
    pub fn parse(s: &str) -> Option<RunKind> {
        Some(match s {
            "agent" => RunKind::Agent,
            "experiment" => RunKind::Experiment,
            "surface" => RunKind::Surface,
            "inbox" => RunKind::Inbox,
            "fleet" => RunKind::Fleet,
            _ => return None,
        })
    }

    /// Whether this kind carries a configuration (ADR-0183 §C).
    pub fn carries_configuration(self) -> bool {
        matches!(self, RunKind::Agent)
    }
}

/// `participant_class ∈ {native, hosted}` (ADR-0013): `ledger ⇔ native`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantClass {
    /// A native participant — the full typed ledger.
    Native,
    /// A hosted participant — rows enter through the same envelope via the hosting
    /// adapter at the declared `observability_level`.
    Hosted,
}

impl ParticipantClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ParticipantClass::Native => "native",
            ParticipantClass::Hosted => "hosted",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ParticipantClass> {
        match s {
            "native" => Some(ParticipantClass::Native),
            "hosted" => Some(ParticipantClass::Hosted),
            _ => None,
        }
    }
}

/// `observability_level` members — `⊆ {events, model_io, end_state, ledger}`; `ledger`
/// is entailed by `native` (CF-059) and the store stamps it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservabilityLevel {
    /// The event stream.
    Events,
    /// Model IO detail (stream deltas, attempt spans, route/cache rows).
    ModelIo,
    /// Environment end-state.
    EndState,
    /// The full ledger — entailed by `native`.
    Ledger,
}

impl ObservabilityLevel {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservabilityLevel::Events => "events",
            ObservabilityLevel::ModelIo => "model_io",
            ObservabilityLevel::EndState => "end_state",
            ObservabilityLevel::Ledger => "ledger",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ObservabilityLevel> {
        Some(match s {
            "events" => ObservabilityLevel::Events,
            "model_io" => ObservabilityLevel::ModelIo,
            "end_state" => ObservabilityLevel::EndState,
            "ledger" => ObservabilityLevel::Ledger,
            _ => return None,
        })
    }
}

/// `attendance.value` (ADR-0168 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendanceValue {
    /// A principal is reachable at a TTY.
    Interactive,
    /// Detached but a principal may re-attach.
    Async,
    /// No principal reachable — never-ask policies apply.
    Unattended,
}

impl AttendanceValue {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AttendanceValue::Interactive => "interactive",
            AttendanceValue::Async => "async",
            AttendanceValue::Unattended => "unattended",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<AttendanceValue> {
        Some(match s {
            "interactive" => AttendanceValue::Interactive,
            "async" => AttendanceValue::Async,
            "unattended" => AttendanceValue::Unattended,
            _ => return None,
        })
    }
}

/// `attendance.source` (ADR-0168 D1): `{declared, tty_inferred, forced}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendanceSource {
    /// An explicit flag.
    Declared,
    /// Inferred from stdin/stdout/stderr being terminals.
    TtyInferred,
    /// A `--no-input`-class flag.
    Forced,
}

impl AttendanceSource {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AttendanceSource::Declared => "declared",
            AttendanceSource::TtyInferred => "tty_inferred",
            AttendanceSource::Forced => "forced",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<AttendanceSource> {
        Some(match s {
            "declared" => AttendanceSource::Declared,
            "tty_inferred" => AttendanceSource::TtyInferred,
            "forced" => AttendanceSource::Forced,
            _ => return None,
        })
    }
}

/// `workspace_trust ∈ {trusted, untrusted, unknown}` — a claim from the trust store
/// (ADR-0168 D7); never a Π input at this layer (OQ-387's leaf lands at Stage 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTrust {
    /// The workspace is trusted.
    Trusted,
    /// The workspace is untrusted.
    Untrusted,
    /// The trust store has no record.
    Unknown,
}

impl WorkspaceTrust {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WorkspaceTrust::Trusted => "trusted",
            WorkspaceTrust::Untrusted => "untrusted",
            WorkspaceTrust::Unknown => "unknown",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<WorkspaceTrust> {
        Some(match s {
            "trusted" => WorkspaceTrust::Trusted,
            "untrusted" => WorkspaceTrust::Untrusted,
            "unknown" => WorkspaceTrust::Unknown,
            _ => return None,
        })
    }
}

/// `lease_ttl{writer, environment, wakeup_claim}` — milliseconds each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseTtl {
    /// The writer-lease TTL.
    pub writer_ms: u64,
    /// The environment-lease TTL.
    pub environment_ms: u64,
    /// The wakeup-claim TTL.
    pub wakeup_claim_ms: u64,
}

/// `task_ref{task_id, suite_id, split_label}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef {
    /// The task id.
    pub task_id: String,
    /// The suite id.
    pub suite_id: String,
    /// The split label (benchmark hygiene — never a feature).
    pub split_label: String,
}

/// `forked_from` / `continued_from` — `{run_id, at_seq, head_hash}` (ADR-0027 §6;
/// ADR-0131 §5). The link's `head_hash` is the anchor the new run's seq-0 `prev_hash`
/// binds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageLink {
    /// The source run.
    pub run_id: String,
    /// The source seq the link binds at.
    pub at_seq: u64,
    /// The source run's `hash` at `at_seq`.
    pub head_hash: String,
}

/// `EventRef` — the `(run_id, event_id)` cross-run causal coordinate (ADR-0027 §4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventRef {
    /// The referenced run.
    pub run_id: String,
    /// The referenced event.
    pub event_id: String,
}

/// The manifest — `{configuration_id, configuration_version_id, harness_def_ref,
/// model_profile_ref, environment_ref, budget, seed, idp, participant_class,
/// observability_level, hosting_mechanism?, capability_declaration_ref?,
/// parent_run_id?, spawn_event?, forked_from?, continued_from?, activation_no,
/// run_kind, attendance, workspace_trust, overrides_layer_id?, envelope_policy_ref?,
/// healing_policy_ref?, lease_ttl, grace_ms, task_ref?, …}` (§5a.1 §3).
///
/// The `open_run` invariant row names the six-field required set; the wider fields are
/// typed `Option`s (their presence rules land with the owning slices — ADR-0234) and
/// anything further rides in `extra`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunManifest {
    /// Seedless aggregate coordinate (agent runs only).
    pub configuration_id: Option<String>,
    /// Seeded row-key coordinate (agent runs only).
    pub configuration_version_id: Option<String>,
    /// The sealed harness definition (`semantic`/`version` id).
    pub harness_def_ref: Option<String>,
    /// The bound model profile.
    pub model_profile_ref: Option<String>,
    /// The environment record's semantic id.
    pub environment_ref: Option<String>,
    /// The environment record's version id.
    pub environment_version_id: Option<String>,
    /// The root `BudgetNode` ref.
    pub budget: Option<String>,
    /// The run seed.
    pub seed: Option<u64>,
    /// The identity profile — must be `idp/1` at this stage (ADR-0036).
    pub idp: String,
    /// `native` | `hosted`.
    pub participant_class: ParticipantClass,
    /// The declared observability subset.
    pub observability_level: BTreeSet<ObservabilityLevel>,
    /// The run discriminator.
    pub run_kind: RunKind,
    /// `activation_no ≥ 1` (ADR-0131).
    pub activation_no: u64,
    /// `{value, source}` (ADR-0168 D1).
    pub attendance: (AttendanceValue, AttendanceSource),
    /// The workspace-trust claim (ADR-0168 D7).
    pub workspace_trust: WorkspaceTrust,
    /// The hosting mechanism (hosted runs).
    pub hosting_mechanism: Option<String>,
    /// The capability declaration ref.
    pub capability_declaration_ref: Option<String>,
    /// The parent run (subagent runs).
    pub parent_run_id: Option<String>,
    /// The spawn event in the parent run.
    pub spawn_event: Option<EventRef>,
    /// A diverging fork link (fork-by-reference is Stage 2; the *field* and its anchor
    /// rule are this slice's — the envelope carries it).
    pub forked_from: Option<LineageLink>,
    /// A non-diverging continuation link.
    pub continued_from: Option<LineageLink>,
    /// The single overrides layer id (ADR-0168 D4).
    pub overrides_layer_id: Option<String>,
    /// The envelope policy ref (ADR-0132).
    pub envelope_policy_ref: Option<String>,
    /// The healing policy ref (ADR-0132).
    pub healing_policy_ref: Option<String>,
    /// `{writer, environment, wakeup_claim}`.
    pub lease_ttl: LeaseTtl,
    /// The reattach grace window (ms).
    pub grace_ms: u64,
    /// The bound task coordinate.
    pub task_ref: Option<TaskRef>,
    /// The spec's trailing `…` — additional manifest facts preserved verbatim.
    pub extra: BTreeMap<String, Json>,
}

impl RunManifest {
    /// The minimal valid manifest shape — for tests and the smallest honest run.
    pub fn minimal(run_kind: RunKind) -> RunManifest {
        RunManifest {
            configuration_id: Some("sha256:".to_string() + &"a".repeat(64)),
            configuration_version_id: Some("sha256:".to_string() + &"b".repeat(64)),
            harness_def_ref: None,
            model_profile_ref: None,
            environment_ref: None,
            environment_version_id: None,
            budget: None,
            seed: Some(0),
            idp: "idp/1".to_string(),
            participant_class: ParticipantClass::Native,
            observability_level: BTreeSet::from([ObservabilityLevel::Ledger]),
            run_kind,
            activation_no: 1,
            attendance: (AttendanceValue::Unattended, AttendanceSource::Declared),
            workspace_trust: WorkspaceTrust::Unknown,
            hosting_mechanism: None,
            capability_declaration_ref: None,
            parent_run_id: None,
            spawn_event: None,
            forked_from: None,
            continued_from: None,
            overrides_layer_id: None,
            envelope_policy_ref: None,
            healing_policy_ref: None,
            lease_ttl: LeaseTtl {
                writer_ms: 60_000,
                environment_ms: 60_000,
                wakeup_claim_ms: 60_000,
            },
            grace_ms: 0,
            task_ref: None,
            extra: BTreeMap::new(),
        }
    }

    /// Field-shape validation (the `open_run` invariant row + ADR-0183 §C). The
    /// lineage-anchor checks run in `Store::open_run` where the source run is visible.
    pub fn validate(&self) -> Result<(), LedgerError> {
        let bad = |detail: String| LedgerError::ManifestInvalid { detail };
        if self.idp != "idp/1" {
            return Err(bad(format!("idp must be idp/1, got {}", self.idp)));
        }
        if self.observability_level.is_empty() {
            return Err(bad("observability_level must be non-empty".into()));
        }
        if self.activation_no == 0 {
            return Err(bad("activation_no must be ≥ 1".into()));
        }
        if self.run_kind.carries_configuration() {
            for (field, value) in [
                ("configuration_id", &self.configuration_id),
                ("configuration_version_id", &self.configuration_version_id),
            ] {
                match value {
                    None => {
                        return Err(LedgerError::ConfigurationUnresolvable {
                            field,
                            value: "(absent)".into(),
                        })
                    }
                    Some(v) if !is_pinned_id(v) => {
                        return Err(LedgerError::ConfigurationUnresolvable {
                            field,
                            value: v.clone(),
                        })
                    }
                    _ => {}
                }
            }
        } else {
            // Non-agent runs carry no configuration/environment cells (ADR-0183 §C).
            for (field, value) in [
                ("configuration_id", &self.configuration_id),
                ("configuration_version_id", &self.configuration_version_id),
                ("environment_ref", &self.environment_ref),
                ("environment_version_id", &self.environment_version_id),
            ] {
                if value.is_some() {
                    return Err(bad(format!(
                        "{field} present on run_kind = {}",
                        self.run_kind.as_str()
                    )));
                }
            }
        }
        // Every ref field present must be a pinned identity id — a mutable tag is
        // `UnresolvedRef` (ADR-0137).
        for (field, value) in [
            ("harness_def_ref", &self.harness_def_ref),
            ("model_profile_ref", &self.model_profile_ref),
            ("environment_ref", &self.environment_ref),
            ("environment_version_id", &self.environment_version_id),
            (
                "capability_declaration_ref",
                &self.capability_declaration_ref,
            ),
            ("overrides_layer_id", &self.overrides_layer_id),
            ("envelope_policy_ref", &self.envelope_policy_ref),
            ("healing_policy_ref", &self.healing_policy_ref),
        ] {
            if let Some(v) = value {
                if !is_pinned_id(v) {
                    return Err(LedgerError::UnresolvedRef {
                        field,
                        value: v.clone(),
                    });
                }
            }
        }
        for link in [&self.forked_from, &self.continued_from]
            .into_iter()
            .flatten()
        {
            if !is_pinned_id(&link.head_hash) {
                return Err(LedgerError::UnresolvedRef {
                    field: "forked_from.head_hash",
                    value: link.head_hash.clone(),
                });
            }
        }
        Ok(())
    }

    /// The canonical payload rendering of the manifest (the seq-0 event's payload).
    pub fn to_json(&self) -> Json {
        let mut m: BTreeMap<String, Json> = self.extra.clone();
        let mut put = |k: &str, v: Json| {
            m.insert(k.to_string(), v);
        };
        put("idp", Json::str(&self.idp));
        put(
            "participant_class",
            Json::str(self.participant_class.as_str()),
        );
        put(
            "observability_level",
            Json::Arr(
                self.observability_level
                    .iter()
                    .map(|l| Json::str(l.as_str()))
                    .collect(),
            ),
        );
        put("run_kind", Json::str(self.run_kind.as_str()));
        put("activation_no", Json::Int(self.activation_no as i64));
        put(
            "attendance",
            Json::Obj(BTreeMap::from([
                ("value".to_string(), Json::str(self.attendance.0.as_str())),
                ("source".to_string(), Json::str(self.attendance.1.as_str())),
            ])),
        );
        put("workspace_trust", Json::str(self.workspace_trust.as_str()));
        put(
            "lease_ttl",
            Json::Obj(BTreeMap::from([
                (
                    "writer".to_string(),
                    Json::Int(self.lease_ttl.writer_ms as i64),
                ),
                (
                    "environment".to_string(),
                    Json::Int(self.lease_ttl.environment_ms as i64),
                ),
                (
                    "wakeup_claim".to_string(),
                    Json::Int(self.lease_ttl.wakeup_claim_ms as i64),
                ),
            ])),
        );
        put("grace_ms", Json::Int(self.grace_ms as i64));
        for (k, v) in [
            ("configuration_id", &self.configuration_id),
            ("configuration_version_id", &self.configuration_version_id),
            ("harness_def_ref", &self.harness_def_ref),
            ("model_profile_ref", &self.model_profile_ref),
            ("environment_ref", &self.environment_ref),
            ("environment_version_id", &self.environment_version_id),
            ("budget", &self.budget),
            ("hosting_mechanism", &self.hosting_mechanism),
            (
                "capability_declaration_ref",
                &self.capability_declaration_ref,
            ),
            ("parent_run_id", &self.parent_run_id),
            ("overrides_layer_id", &self.overrides_layer_id),
            ("envelope_policy_ref", &self.envelope_policy_ref),
            ("healing_policy_ref", &self.healing_policy_ref),
        ] {
            if let Some(v) = v {
                put(k, Json::str(v));
            }
        }
        if let Some(seed) = self.seed {
            put("seed", Json::Int(seed as i64));
        }
        if let Some(link) = &self.forked_from {
            put("forked_from", lineage_link_json(link));
        }
        if let Some(link) = &self.continued_from {
            put("continued_from", lineage_link_json(link));
        }
        if let Some(ev) = &self.spawn_event {
            put("spawn_event", event_ref_json(ev));
        }
        if let Some(t) = &self.task_ref {
            put(
                "task_ref",
                Json::Obj(BTreeMap::from([
                    ("task_id".to_string(), Json::str(&t.task_id)),
                    ("suite_id".to_string(), Json::str(&t.suite_id)),
                    ("split_label".to_string(), Json::str(&t.split_label)),
                ])),
            );
        }
        Json::Obj(m)
    }

    /// Decode the seq-0 payload back into a manifest (reopen path).
    pub fn from_json(j: &Json) -> Result<RunManifest, LedgerError> {
        let bad = |d: String| LedgerError::ManifestInvalid { detail: d };
        let opt_str = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
        let req_str = |k: &str| -> Result<String, LedgerError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| bad(format!("{k} missing/not a string")))
        };
        let req_int = |k: &str| -> Result<u64, LedgerError> {
            match j.get(k).and_then(Json::as_int) {
                Some(i) if i >= 0 => Ok(i as u64),
                _ => Err(bad(format!("{k} missing/not an int ≥ 0"))),
            }
        };
        let run_kind =
            RunKind::parse(&req_str("run_kind")?).ok_or_else(|| bad("run_kind unknown".into()))?;
        let participant_class = ParticipantClass::parse(&req_str("participant_class")?)
            .ok_or_else(|| bad("participant_class unknown".into()))?;
        let observability_level = match j.get("observability_level") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .and_then(ObservabilityLevel::parse)
                        .ok_or_else(|| bad("observability_level member unknown".into()))
                })
                .collect::<Result<BTreeSet<_>, _>>()?,
            _ => return Err(bad("observability_level missing/not an array".into())),
        };
        let attendance_j = j
            .get("attendance")
            .ok_or_else(|| bad("attendance missing".into()))?;
        let attendance = (
            AttendanceValue::parse(
                attendance_j
                    .get("value")
                    .and_then(Json::as_str)
                    .unwrap_or(""),
            )
            .ok_or_else(|| bad("attendance.value unknown".into()))?,
            AttendanceSource::parse(
                attendance_j
                    .get("source")
                    .and_then(Json::as_str)
                    .unwrap_or(""),
            )
            .ok_or_else(|| bad("attendance.source unknown".into()))?,
        );
        let workspace_trust = WorkspaceTrust::parse(&req_str("workspace_trust")?)
            .ok_or_else(|| bad("workspace_trust unknown".into()))?;
        let ttl_j = j
            .get("lease_ttl")
            .ok_or_else(|| bad("lease_ttl missing".into()))?;
        let ttl_i = |k: &str| -> Result<u64, LedgerError> {
            match ttl_j.get(k).and_then(Json::as_int) {
                Some(i) if i >= 0 => Ok(i as u64),
                _ => Err(bad(format!("lease_ttl.{k} missing/not an int ≥ 0"))),
            }
        };
        let link = |k: &str| -> Result<Option<LineageLink>, LedgerError> {
            match j.get(k) {
                None => Ok(None),
                Some(l) => Ok(Some(LineageLink {
                    run_id: l
                        .get("run_id")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| bad(format!("{k}.run_id missing")))?,
                    at_seq: match l.get("at_seq").and_then(Json::as_int) {
                        Some(i) if i >= 0 => i as u64,
                        _ => return Err(bad(format!("{k}.at_seq missing"))),
                    },
                    head_hash: l
                        .get("head_hash")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| bad(format!("{k}.head_hash missing")))?,
                })),
            }
        };
        let spawn_event = match j.get("spawn_event") {
            None => None,
            Some(e) => Some(EventRef {
                run_id: e
                    .get("run_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad("spawn_event.run_id missing".into()))?,
                event_id: e
                    .get("event_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad("spawn_event.event_id missing".into()))?,
            }),
        };
        let task_ref = match j.get("task_ref") {
            None => None,
            Some(t) => Some(TaskRef {
                task_id: t
                    .get("task_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad("task_ref.task_id missing".into()))?,
                suite_id: t
                    .get("suite_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad("task_ref.suite_id missing".into()))?,
                split_label: t
                    .get("split_label")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad("task_ref.split_label missing".into()))?,
            }),
        };
        // Anything not in the typed vocabulary is preserved in `extra` (CC3).
        const TYPED: &[&str] = &[
            "activation_no",
            "attendance",
            "budget",
            "capability_declaration_ref",
            "configuration_id",
            "configuration_version_id",
            "continued_from",
            "environment_ref",
            "environment_version_id",
            "envelope_policy_ref",
            "forked_from",
            "grace_ms",
            "harness_def_ref",
            "healing_policy_ref",
            "hosting_mechanism",
            "idp",
            "lease_ttl",
            "model_profile_ref",
            "observability_level",
            "overrides_layer_id",
            "parent_run_id",
            "participant_class",
            "run_kind",
            "seed",
            "spawn_event",
            "task_ref",
            "workspace_trust",
        ];
        let extra = match j {
            Json::Obj(m) => m
                .iter()
                .filter(|(k, _)| !TYPED.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            _ => return Err(bad("manifest payload must be an object".into())),
        };
        Ok(RunManifest {
            configuration_id: opt_str("configuration_id"),
            configuration_version_id: opt_str("configuration_version_id"),
            harness_def_ref: opt_str("harness_def_ref"),
            model_profile_ref: opt_str("model_profile_ref"),
            environment_ref: opt_str("environment_ref"),
            environment_version_id: opt_str("environment_version_id"),
            budget: opt_str("budget"),
            seed: j.get("seed").and_then(Json::as_int).map(|s| s as u64),
            idp: req_str("idp")?,
            participant_class,
            observability_level,
            run_kind,
            activation_no: req_int("activation_no")?,
            attendance,
            workspace_trust,
            hosting_mechanism: opt_str("hosting_mechanism"),
            capability_declaration_ref: opt_str("capability_declaration_ref"),
            parent_run_id: opt_str("parent_run_id"),
            spawn_event,
            forked_from: link("forked_from")?,
            continued_from: link("continued_from")?,
            overrides_layer_id: opt_str("overrides_layer_id"),
            envelope_policy_ref: opt_str("envelope_policy_ref"),
            healing_policy_ref: opt_str("healing_policy_ref"),
            lease_ttl: LeaseTtl {
                writer_ms: ttl_i("writer")?,
                environment_ms: ttl_i("environment")?,
                wakeup_claim_ms: ttl_i("wakeup_claim")?,
            },
            grace_ms: req_int("grace_ms")?,
            task_ref,
            extra,
        })
    }
}

fn lineage_link_json(l: &LineageLink) -> Json {
    Json::Obj(BTreeMap::from([
        ("run_id".to_string(), Json::str(&l.run_id)),
        ("at_seq".to_string(), Json::Int(l.at_seq as i64)),
        ("head_hash".to_string(), Json::str(&l.head_hash)),
    ]))
}

fn event_ref_json(e: &EventRef) -> Json {
    Json::Obj(BTreeMap::from([
        ("run_id".to_string(), Json::str(&e.run_id)),
        ("event_id".to_string(), Json::str(&e.event_id)),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_agent_manifest_validates() {
        assert!(RunManifest::minimal(RunKind::Agent).validate().is_ok());
    }

    #[test]
    fn agent_run_requires_pinned_configuration() {
        let mut m = RunManifest::minimal(RunKind::Agent);
        m.configuration_id = None;
        assert!(matches!(
            m.validate(),
            Err(LedgerError::ConfigurationUnresolvable { .. })
        ));
        let mut m = RunManifest::minimal(RunKind::Agent);
        m.configuration_version_id = Some("cfg:latest".into());
        assert!(matches!(
            m.validate(),
            Err(LedgerError::ConfigurationUnresolvable { .. })
        ));
    }

    #[test]
    fn non_agent_run_carries_no_configuration() {
        let mut m = RunManifest::minimal(RunKind::Experiment);
        m.configuration_id = None;
        m.configuration_version_id = None;
        assert!(m.validate().is_ok());
        let m = RunManifest::minimal(RunKind::Inbox);
        assert!(matches!(
            m.validate(),
            Err(LedgerError::ManifestInvalid { .. })
        ));
    }

    #[test]
    fn mutable_tag_ref_is_unresolved() {
        let mut m = RunManifest::minimal(RunKind::Agent);
        m.environment_ref = Some("registry.local/env:latest".into());
        assert!(matches!(
            m.validate(),
            Err(LedgerError::UnresolvedRef {
                field: "environment_ref",
                ..
            })
        ));
    }

    #[test]
    fn manifest_round_trips_through_canonical_json() {
        let mut m = RunManifest::minimal(RunKind::Agent);
        m.extra
            .insert("kernel_signer".into(), Json::str("sha256:signer"));
        let j = m.to_json();
        let back = RunManifest::from_json(&j).unwrap();
        assert_eq!(back.to_json(), j);
    }
}
