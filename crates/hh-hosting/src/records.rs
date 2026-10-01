//! The `ParticipantRecord` / `AdapterRecord` body schemas (registry kinds
//! `participant`/`adapter`; §6.6 §3; ADR-0166 D2/D3; R-2.10.6⁰) and the
//! `hh.hosting/1` extension block (`HostingExt`).
//!
//! Layering (CC7): this crate owns the schemas; `hh-registry` stores the
//! canonical bodies verbatim behind the `kind` tag gate (the same layering as
//! `environment_record`). `version_identity` on a participant record is the
//! derived coordinate `idp("hh.hosting.participant", canonical{declaration,
//! participant_version})` — `from_json` re-derives and refuses a mismatch, so
//! an authored coordinate can never drift from the declaration it names.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use hh_budget::errors::EnforcementLevel;
use hh_hir::records::AssumptionDebtRecord;
use hh_identity::idp::idp_id;
use hh_ontology::participant::{
    CapabilityVerdict, HostingMechanism, Observability, ParticipantClass, ParticipantDescriptor,
};
use hh_wire::Json;

use crate::events::EventChannel;

/// The registered `hh.hosting/1` ext-block member name (ADR-0166 §2.5).
pub const HOSTING_EXT_KEY: &str = "hh.hosting/1";

/// `process_placement` — where the hosted process runs (§6.6 ext).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessPlacement {
    /// `in_environment` — inside the Lab-provisioned environment.
    InEnvironment,
    /// `lab_host` — on the Lab host (out of the environment).
    LabHost,
    /// `remote_service` — a remote service the Lab does not host.
    RemoteService,
}

impl ProcessPlacement {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProcessPlacement::InEnvironment => "in_environment",
            ProcessPlacement::LabHost => "lab_host",
            ProcessPlacement::RemoteService => "remote_service",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ProcessPlacement> {
        Some(match s {
            "in_environment" => ProcessPlacement::InEnvironment,
            "lab_host" => ProcessPlacement::LabHost,
            "remote_service" => ProcessPlacement::RemoteService,
            _ => return None,
        })
    }
}

/// `model_io_intercept` — how the adapter intercepts model I/O (§6.6 ext).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelIoIntercept {
    /// `base_url` — a proxy base URL.
    BaseUrl,
    /// `client_patch` — a client-side patch.
    ClientPatch,
    /// `none` — not intercepted.
    None,
    /// `unknown` — undeclared (never coerced — T-LCD-07).
    Unknown,
}

impl ModelIoIntercept {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelIoIntercept::BaseUrl => "base_url",
            ModelIoIntercept::ClientPatch => "client_patch",
            ModelIoIntercept::None => "none",
            ModelIoIntercept::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ModelIoIntercept> {
        Some(match s {
            "base_url" => ModelIoIntercept::BaseUrl,
            "client_patch" => ModelIoIntercept::ClientPatch,
            "none" => ModelIoIntercept::None,
            "unknown" => ModelIoIntercept::Unknown,
            _ => return None,
        })
    }
}

/// A `budget_enforcement` claim value — `enforced | advisory | unenforceable`
/// per dimension (§6.6 ext; the [`EnforcementLevel`] vocabulary — CC1).
pub type EnforcementClaim = EnforcementLevel;

/// The `hh.hosting/1` extension block on a `CapabilityDeclarationRecord`
/// (ADR-0166 §2.5 — the registered members). Every member is optional
/// (`absent` is honest — never defaulted); unknown members preserve verbatim
/// in `extra` (additive evolution, I-3's record-level twin). `ext` never
/// decides authority, budget or validity (ADR-0015).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HostingExt {
    /// `credential_supply` — the channels credentials may flow through.
    pub credential_supply: Option<Vec<String>>,
    /// `receipts` — tool-effect receipts.
    pub receipts: Option<CapabilityVerdict>,
    /// `account_exact` — exact account reporting.
    pub account_exact: Option<CapabilityVerdict>,
    /// `elicitation` — the Lab may elicit mid-run.
    pub elicitation: Option<CapabilityVerdict>,
    /// `model_io_intercept` — the interception mode.
    pub model_io_intercept: Option<ModelIoIntercept>,
    /// `process_placement` — where the hosted process runs.
    pub process_placement: Option<ProcessPlacement>,
    /// `tool_supply_channel` — how tools reach the participant.
    pub tool_supply_channel: Option<String>,
    /// `context_supply_mode` — how context is delivered.
    pub context_supply_mode: Option<String>,
    /// `coordinates` — the declared coordinates map (`{model?, …}`).
    pub coordinates: Option<Json>,
    /// `budget_enforcement` — `dimension → enforcement claim` (derived at
    /// Stage 4, never participant-authored into `enforced` — the block
    /// *declares* what the adapter claims; the Lab derives what it enforces).
    pub budget_enforcement: Option<BTreeMap<String, EnforcementClaim>>,
    /// `abi_versions` — the `hh-hosting/n` majors the participant speaks.
    pub abi_versions: Option<Vec<String>>,
    /// `event_channels` — the event channels the adapter exposes.
    pub event_channels: Option<BTreeSet<EventChannel>>,
    /// `usage_mapping` — how usage reports map to cost rows.
    pub usage_mapping: Option<String>,
    /// Unknown members — preserved verbatim (I-3's record twin).
    pub extra: BTreeMap<String, Json>,
}

/// Record-schema failures — typed, never a string (R2).
#[derive(Debug, Clone, PartialEq)]
pub enum RecordError {
    /// A required member is absent or mistyped.
    Member {
        /// The member path.
        path: String,
        /// What was expected.
        detail: String,
    },
    /// A closed-vocabulary member carried an unknown spelling.
    UnknownSpelling {
        /// The member path.
        path: String,
        /// The spelling found.
        value: String,
    },
    /// `descriptor.class` is not `hosted` on a participant record.
    ClassNotHosted,
    /// `version_identity` does not equal the derived coordinate.
    VersionIdentityMismatch,
    /// The `debt` member failed `AssumptionDebtRecord/1` decode.
    DebtSchema {
        /// The decode detail.
        detail: String,
    },
    /// A hosted `OpaqueProcess` declaration must never carry
    /// `hosting_mechanism = none` (CF-351 — `none` is native-only).
    MechanismNoneOnHosted,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::Member { path, detail } => write!(f, "Member({path}: {detail})"),
            RecordError::UnknownSpelling { path, value } => {
                write!(f, "UnknownSpelling({path}: {value})")
            }
            RecordError::ClassNotHosted => write!(f, "ClassNotHosted"),
            RecordError::VersionIdentityMismatch => write!(f, "VersionIdentityMismatch"),
            RecordError::DebtSchema { detail } => write!(f, "DebtSchema({detail})"),
            RecordError::MechanismNoneOnHosted => write!(f, "MechanismNoneOnHosted"),
        }
    }
}

impl std::error::Error for RecordError {}

fn bad(path: &str, detail: &str) -> RecordError {
    RecordError::Member {
        path: path.to_string(),
        detail: detail.to_string(),
    }
}

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, RecordError> {
    j.get(k).ok_or_else(|| RecordError::Member {
        path: format!("{path}.{k}"),
        detail: "missing".to_string(),
    })
}

fn req_str(j: &Json, k: &str, path: &str) -> Result<String, RecordError> {
    match req(j, k, path)? {
        Json::Str(s) => Ok(s.clone()),
        _ => Err(bad(&format!("{path}.{k}"), "expected string")),
    }
}

fn opt_str_vec(j: &Json, k: &str) -> Option<Vec<String>> {
    match j.get(k) {
        Some(Json::Arr(items)) => Some(
            items
                .iter()
                .filter_map(|i| i.as_str().map(String::from))
                .collect(),
        ),
        _ => None,
    }
}

impl HostingExt {
    /// The canonical JSON form (unknown members round-trip through `extra`).
    pub fn to_json(&self) -> Json {
        let mut m = self.extra.clone();
        if let Some(v) = &self.credential_supply {
            m.insert(
                "credential_supply".into(),
                Json::Arr(v.iter().map(Json::str).collect()),
            );
        }
        if let Some(v) = &self.receipts {
            m.insert("receipts".into(), Json::str(v.as_str()));
        }
        if let Some(v) = &self.account_exact {
            m.insert("account_exact".into(), Json::str(v.as_str()));
        }
        if let Some(v) = &self.elicitation {
            m.insert("elicitation".into(), Json::str(v.as_str()));
        }
        if let Some(v) = &self.model_io_intercept {
            m.insert("model_io_intercept".into(), Json::str(v.as_str()));
        }
        if let Some(v) = &self.process_placement {
            m.insert("process_placement".into(), Json::str(v.as_str()));
        }
        if let Some(v) = &self.tool_supply_channel {
            m.insert("tool_supply_channel".into(), Json::str(v));
        }
        if let Some(v) = &self.context_supply_mode {
            m.insert("context_supply_mode".into(), Json::str(v));
        }
        if let Some(v) = &self.coordinates {
            m.insert("coordinates".into(), v.clone());
        }
        if let Some(v) = &self.budget_enforcement {
            m.insert(
                "budget_enforcement".into(),
                Json::Obj(
                    v.iter()
                        .map(|(k, l)| (k.clone(), Json::str(l.as_str())))
                        .collect(),
                ),
            );
        }
        if let Some(v) = &self.abi_versions {
            m.insert(
                "abi_versions".into(),
                Json::Arr(v.iter().map(Json::str).collect()),
            );
        }
        if let Some(v) = &self.event_channels {
            m.insert(
                "event_channels".into(),
                Json::Arr(v.iter().map(|c| Json::str(c.as_str())).collect()),
            );
        }
        if let Some(v) = &self.usage_mapping {
            m.insert("usage_mapping".into(), Json::str(v));
        }
        Json::Obj(m)
    }

    /// Strict decode — known members typed; unknown members preserved in
    /// `extra` (an unknown member is never a decode error — additive
    /// evolution). A *known* member with a bad spelling refuses.
    pub fn from_json(j: &Json, path: &str) -> Result<HostingExt, RecordError> {
        let Json::Obj(m) = j else {
            return Err(bad(path, "expected object"));
        };
        let verdict = |k: &str| -> Result<Option<CapabilityVerdict>, RecordError> {
            match m.get(k) {
                Some(Json::Str(s)) => CapabilityVerdict::parse(s).map(Some).ok_or_else(|| {
                    RecordError::UnknownSpelling {
                        path: format!("{path}.{k}"),
                        value: s.clone(),
                    }
                }),
                Some(_) => Err(bad(&format!("{path}.{k}"), "expected verdict string")),
                None => Ok(None),
            }
        };
        let mut budget_enforcement = None;
        if let Some(Json::Obj(be)) = m.get("budget_enforcement") {
            let mut map = BTreeMap::new();
            for (k, v) in be {
                match v.as_str().and_then(EnforcementLevel::parse) {
                    Some(l) => {
                        map.insert(k.clone(), l);
                    }
                    None => {
                        return Err(RecordError::UnknownSpelling {
                            path: format!("{path}.budget_enforcement.{k}"),
                            value: v.as_str().unwrap_or("").to_string(),
                        })
                    }
                }
            }
            budget_enforcement = Some(map);
        }
        let mut event_channels = None;
        if let Some(Json::Arr(items)) = m.get("event_channels") {
            let mut set = BTreeSet::new();
            for i in items {
                let s = i
                    .as_str()
                    .ok_or_else(|| bad(&format!("{path}.event_channels"), "expected string"))?;
                set.insert(
                    EventChannel::parse(s).map_err(|v| RecordError::UnknownSpelling {
                        path: format!("{path}.event_channels"),
                        value: v,
                    })?,
                );
            }
            event_channels = Some(set);
        }
        const KNOWN: &[&str] = &[
            "credential_supply",
            "receipts",
            "account_exact",
            "elicitation",
            "model_io_intercept",
            "process_placement",
            "tool_supply_channel",
            "context_supply_mode",
            "coordinates",
            "budget_enforcement",
            "abi_versions",
            "event_channels",
            "usage_mapping",
        ];
        let extra: BTreeMap<String, Json> = m
            .iter()
            .filter(|(k, _)| !KNOWN.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(HostingExt {
            credential_supply: opt_str_vec(j, "credential_supply"),
            receipts: verdict("receipts")?,
            account_exact: verdict("account_exact")?,
            elicitation: verdict("elicitation")?,
            model_io_intercept: match m.get("model_io_intercept").and_then(Json::as_str) {
                Some(s) => Some(ModelIoIntercept::parse(s).ok_or_else(|| {
                    RecordError::UnknownSpelling {
                        path: format!("{path}.model_io_intercept"),
                        value: s.to_string(),
                    }
                })?),
                None => None,
            },
            process_placement: match m.get("process_placement").and_then(Json::as_str) {
                Some(s) => Some(ProcessPlacement::parse(s).ok_or_else(|| {
                    RecordError::UnknownSpelling {
                        path: format!("{path}.process_placement"),
                        value: s.to_string(),
                    }
                })?),
                None => None,
            },
            tool_supply_channel: m
                .get("tool_supply_channel")
                .and_then(Json::as_str)
                .map(String::from),
            context_supply_mode: m
                .get("context_supply_mode")
                .and_then(Json::as_str)
                .map(String::from),
            coordinates: m.get("coordinates").cloned(),
            budget_enforcement,
            abi_versions: opt_str_vec(j, "abi_versions"),
            event_channels,
            usage_mapping: m
                .get("usage_mapping")
                .and_then(Json::as_str)
                .map(String::from),
            extra,
        })
    }
}

/// The canonical `participant` record body (§6.6 §3 — the descriptor + the
/// capability declaration + `ext hh.hosting/1`). `version_identity` is
/// derived (`idp("hh.hosting.participant", canonical{declaration,
/// participant_version})`) — the registry mints `version_id` on the envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticipantRecord {
    /// The participant id (a label — identity is the version coordinate).
    pub participant_id: String,
    /// The participant's own version string (the product version).
    pub participant_version: String,
    /// The derived `version_identity` (`H(declaration ∥ participant_version)`
    /// — re-derived on decode; a mismatch refuses).
    pub version_identity: String,
    /// The participant descriptor — `class` must be `hosted` and
    /// `hosting_mechanism` never `none` (CF-351).
    pub descriptor: ParticipantDescriptor,
    /// The `CapabilityDeclarationRecord` fields (`dimension → value` — the
    /// ADR-0018 tri-state surface; values stay `Json` because several
    /// dimensions carry structured values).
    pub capability_declaration: BTreeMap<String, Json>,
    /// The `hh.hosting/1` extension block.
    pub hosting_ext: HostingExt,
    /// Other `ext` blocks — preserved verbatim.
    pub ext: BTreeMap<String, Json>,
}

/// The `hh.hosting.participant` idp domain tag for `version_identity`.
pub const PARTICIPANT_VERSION_DOMAIN: &str = "hh.hosting.participant";

impl ParticipantRecord {
    /// The derived `version_identity` — `H(declaration ∥ participant_version)`
    /// over the canonical declaration bytes (§6.6 §3).
    pub fn derive_version_identity(
        capability_declaration: &BTreeMap<String, Json>,
        participant_version: &str,
    ) -> String {
        idp_id(
            PARTICIPANT_VERSION_DOMAIN,
            Json::obj([
                (
                    "declaration",
                    Json::Obj(
                        capability_declaration
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    ),
                ),
                ("participant_version", Json::str(participant_version)),
            ])
            .to_canonical_string()
            .as_bytes(),
        )
    }

    /// Construct with the derived `version_identity`.
    pub fn new(
        participant_id: &str,
        participant_version: &str,
        descriptor: ParticipantDescriptor,
        capability_declaration: BTreeMap<String, Json>,
        hosting_ext: HostingExt,
        ext: BTreeMap<String, Json>,
    ) -> Result<ParticipantRecord, RecordError> {
        if descriptor.class != ParticipantClass::Hosted {
            return Err(RecordError::ClassNotHosted);
        }
        if descriptor.hosting_mechanism == HostingMechanism::None {
            return Err(RecordError::MechanismNoneOnHosted);
        }
        Ok(ParticipantRecord {
            participant_id: participant_id.to_string(),
            participant_version: participant_version.to_string(),
            version_identity: Self::derive_version_identity(
                &capability_declaration,
                participant_version,
            ),
            descriptor,
            capability_declaration,
            hosting_ext,
            ext,
        })
    }

    /// The canonical body (`{kind: "participant", …}` — the `kind` tag the
    /// registry's structural gate checks).
    pub fn to_json(&self) -> Json {
        let d = &self.descriptor;
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str("participant"));
        m.insert("participant_id".into(), Json::str(&self.participant_id));
        m.insert(
            "participant_version".into(),
            Json::str(&self.participant_version),
        );
        m.insert("version_identity".into(), Json::str(&self.version_identity));
        m.insert(
            "descriptor".into(),
            Json::obj([
                ("class", Json::str(d.class.as_str())),
                ("hosting_mechanism", Json::str(d.hosting_mechanism.as_str())),
                (
                    "observability_level",
                    Json::Arr(
                        d.observability_level
                            .iter()
                            .map(|o| Json::str(o.as_str()))
                            .collect(),
                    ),
                ),
                (
                    "capability_vector",
                    Json::Obj(
                        d.capability_vector
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                            .collect(),
                    ),
                ),
            ]),
        );
        m.insert(
            "capability_declaration".into(),
            Json::Obj(
                self.capability_declaration
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        let mut ext = self.ext.clone();
        ext.insert(HOSTING_EXT_KEY.to_string(), self.hosting_ext.to_json());
        m.insert("ext".into(), Json::Obj(ext));
        Json::Obj(m)
    }

    /// Strict decode — re-derives `version_identity` and refuses a mismatch
    /// (an authored coordinate can never drift from the declaration).
    pub fn from_json(j: &Json) -> Result<ParticipantRecord, RecordError> {
        let path = "participant";
        match j.get("kind").and_then(Json::as_str) {
            Some("participant") => {}
            Some(t) => {
                return Err(bad(
                    "participant.kind",
                    &format!("expected \"participant\", found {t}"),
                ))
            }
            None => return Err(bad("participant.kind", "missing")),
        }
        let dj = req(j, "descriptor", path)?;
        let class =
            ParticipantClass::parse(&req_str(dj, "class", "descriptor")?).ok_or_else(|| {
                RecordError::UnknownSpelling {
                    path: "descriptor.class".into(),
                    value: String::new(),
                }
            })?;
        let mechanism = HostingMechanism::parse(&req_str(dj, "hosting_mechanism", "descriptor")?)
            .ok_or_else(|| RecordError::UnknownSpelling {
            path: "descriptor.hosting_mechanism".into(),
            value: String::new(),
        })?;
        let mut observability = BTreeSet::new();
        if let Some(Json::Arr(items)) = dj.get("observability_level") {
            for i in items {
                let s = i
                    .as_str()
                    .ok_or_else(|| bad("descriptor.observability_level", "expected string"))?;
                observability.insert(Observability::parse(s).ok_or_else(|| {
                    RecordError::UnknownSpelling {
                        path: "descriptor.observability_level".into(),
                        value: s.to_string(),
                    }
                })?);
            }
        }
        let mut capability_vector = BTreeMap::new();
        if let Some(Json::Obj(cv)) = dj.get("capability_vector") {
            for (k, v) in cv {
                let s = v.as_str().ok_or_else(|| {
                    bad("descriptor.capability_vector", "expected verdict string")
                })?;
                capability_vector.insert(
                    k.clone(),
                    CapabilityVerdict::parse(s).ok_or_else(|| RecordError::UnknownSpelling {
                        path: format!("descriptor.capability_vector.{k}"),
                        value: s.to_string(),
                    })?,
                );
            }
        }
        // The §2.7 rules are the one validation (CC1 — `describe`, never a
        // second implementation): class declared, `ledger` is native-entailed,
        // `none` mechanism is native-only.
        let descriptor =
            hh_ontology::participant::describe(hh_ontology::participant::RawDescriptor {
                class: Some(class),
                hosting_mechanism: mechanism,
                observability_level: observability,
                capability_vector,
            })
            .map_err(|e| RecordError::Member {
                path: "descriptor".into(),
                detail: format!("{e:?}"),
            })?;
        if descriptor.class != ParticipantClass::Hosted {
            return Err(RecordError::ClassNotHosted);
        }
        if descriptor.hosting_mechanism == HostingMechanism::None {
            return Err(RecordError::MechanismNoneOnHosted);
        }
        let capability_declaration = match req(j, "capability_declaration", path)? {
            Json::Obj(m) => m.clone(),
            _ => return Err(bad("participant.capability_declaration", "expected object")),
        };
        let participant_version = req_str(j, "participant_version", path)?;
        let version_identity = req_str(j, "version_identity", path)?;
        if version_identity
            != Self::derive_version_identity(&capability_declaration, &participant_version)
        {
            return Err(RecordError::VersionIdentityMismatch);
        }
        let ext_obj = match j.get("ext") {
            Some(Json::Obj(e)) => e.clone(),
            _ => BTreeMap::new(),
        };
        let hosting_ext = match ext_obj.get(HOSTING_EXT_KEY) {
            Some(e) => HostingExt::from_json(e, "participant.ext.hh.hosting/1")?,
            None => HostingExt::default(),
        };
        let ext: BTreeMap<String, Json> = ext_obj
            .into_iter()
            .filter(|(k, _)| k != HOSTING_EXT_KEY)
            .collect();
        Ok(ParticipantRecord {
            participant_id: req_str(j, "participant_id", path)?,
            participant_version,
            version_identity,
            descriptor,
            capability_declaration,
            hosting_ext,
            ext,
        })
    }
}

/// The canonical `adapter` record body (§6.6 §3 — `{adapter_id, version_id,
/// hosting_mechanism, participant_selector, declaration_defaults,
/// placement_supported, lowering_table_ref, loss_report_ref, debt}`). The debt
/// is a complete `AssumptionDebtRecord` — `link` refuses an adapter without
/// one (T-LCD-05 reflexive; the record is complete-by-schema, the *service*
/// enforces completeness semantics at attach).
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterRecord {
    /// The adapter id.
    pub adapter_id: String,
    /// The adapter's declared version string (the registry mints the record's
    /// `version_id` on the envelope — this member is the adapter's own
    /// version coordinate, like a participant's `participant_version`).
    pub version_id: String,
    /// The mechanism the adapter binds (`session_abi` at C0; never `none`).
    pub hosting_mechanism: HostingMechanism,
    /// `participant_selector` — the claim the adapter's debt hypothesis is
    /// bound to (opaque `Json` — selector grammar is a service concern).
    pub participant_selector: Json,
    /// `declaration_defaults` — the capability states the adapter assumes
    /// participants matching the selector satisfy (`dimension → value`).
    pub declaration_defaults: BTreeMap<String, Json>,
    /// `placement_supported` — the placements the adapter claims (I-6: the
    /// adapter refuses to attach a mechanism/placement it does not claim).
    pub placement_supported: BTreeSet<ProcessPlacement>,
    /// The pinned lowering-table ref (content address / version_id).
    pub lowering_table_ref: Option<String>,
    /// The pinned published loss-report ref.
    pub loss_report_ref: Option<String>,
    /// The adapter's `AssumptionDebtRecord` (complete — hypothesis, evidence,
    /// owner, expiry, removal test).
    pub debt: AssumptionDebtRecord,
    /// `ext` members — preserved, never deciding (CC3).
    pub ext: BTreeMap<String, Json>,
}

impl AdapterRecord {
    /// I-6 — whether the record claims `(mechanism, placement)` (an adapter
    /// refuses to attach what it does not claim).
    pub fn claims(&self, mechanism: HostingMechanism, placement: ProcessPlacement) -> bool {
        self.hosting_mechanism == mechanism && self.placement_supported.contains(&placement)
    }

    /// The canonical body (`{kind: "adapter", …}`).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str("adapter"));
        m.insert("adapter_id".into(), Json::str(&self.adapter_id));
        m.insert("version_id".into(), Json::str(&self.version_id));
        m.insert(
            "hosting_mechanism".into(),
            Json::str(self.hosting_mechanism.as_str()),
        );
        m.insert(
            "participant_selector".into(),
            self.participant_selector.clone(),
        );
        m.insert(
            "declaration_defaults".into(),
            Json::Obj(
                self.declaration_defaults
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        m.insert(
            "placement_supported".into(),
            Json::Arr(
                self.placement_supported
                    .iter()
                    .map(|p| Json::str(p.as_str()))
                    .collect(),
            ),
        );
        if let Some(r) = &self.lowering_table_ref {
            m.insert("lowering_table_ref".into(), Json::str(r));
        }
        if let Some(r) = &self.loss_report_ref {
            m.insert("loss_report_ref".into(), Json::str(r));
        }
        m.insert("debt".into(), hh_hir::debt_json(&self.debt, false));
        m.insert(
            "ext".into(),
            Json::Obj(
                self.ext
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        Json::Obj(m)
    }

    /// Strict decode — the `debt` member decodes through the canonical
    /// `AssumptionDebtRecord/1` codec (an adapter with a malformed debt record
    /// never registers — T-LCD-05 reflexive).
    pub fn from_json(j: &Json) -> Result<AdapterRecord, RecordError> {
        let path = "adapter";
        match j.get("kind").and_then(Json::as_str) {
            Some("adapter") => {}
            Some(t) => {
                return Err(bad(
                    "adapter.kind",
                    &format!("expected \"adapter\", found {t}"),
                ))
            }
            None => return Err(bad("adapter.kind", "missing")),
        }
        let mechanism = HostingMechanism::parse(&req_str(j, "hosting_mechanism", path)?)
            .ok_or_else(|| RecordError::UnknownSpelling {
                path: "adapter.hosting_mechanism".into(),
                value: String::new(),
            })?;
        if mechanism == HostingMechanism::None {
            return Err(RecordError::MechanismNoneOnHosted);
        }
        let mut placement_supported = BTreeSet::new();
        for i in match req(j, "placement_supported", path)? {
            Json::Arr(items) => items,
            _ => return Err(bad("adapter.placement_supported", "expected array")),
        } {
            let s = i
                .as_str()
                .ok_or_else(|| bad("adapter.placement_supported", "expected string"))?;
            placement_supported.insert(ProcessPlacement::parse(s).ok_or_else(|| {
                RecordError::UnknownSpelling {
                    path: "adapter.placement_supported".into(),
                    value: s.to_string(),
                }
            })?);
        }
        let debt = hh_hir::debt_from_json(req(j, "debt", path)?, "adapter.debt").map_err(|e| {
            RecordError::DebtSchema {
                detail: format!("{e:?}"),
            }
        })?;
        let declaration_defaults = match req(j, "declaration_defaults", path)? {
            Json::Obj(m) => m.clone(),
            _ => return Err(bad("adapter.declaration_defaults", "expected object")),
        };
        Ok(AdapterRecord {
            adapter_id: req_str(j, "adapter_id", path)?,
            version_id: req_str(j, "version_id", path)?,
            hosting_mechanism: mechanism,
            participant_selector: req(j, "participant_selector", path)?.clone(),
            declaration_defaults,
            placement_supported,
            lowering_table_ref: j
                .get("lowering_table_ref")
                .and_then(Json::as_str)
                .map(String::from),
            loss_report_ref: j
                .get("loss_report_ref")
                .and_then(Json::as_str)
                .map(String::from),
            debt,
            ext: match j.get("ext") {
                Some(Json::Obj(m)) => m.clone(),
                _ => BTreeMap::new(),
            },
        })
    }
}
