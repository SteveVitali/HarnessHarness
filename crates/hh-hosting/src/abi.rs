//! The `hh-hosting/1` handshake and verb vocabularies (spec §6.6 §2.1;
//! R-2.10.6; S4.5a; ADR-0164).
//!
//! * `describe` advertises `abi_versions[]`. `attach` picks the **highest
//!   mutually supported major** — the Lab speaks `hh-hosting/1`. A participant
//!   advertising **no** ABI versions attaches at `hh-hosting/1` with every
//!   declaration `unknown` (never refused — the declaration is the honest
//!   record). An unknown **major** refuses `AbiVersionUnsupported`; an unknown
//!   **minor** attaches with the unknown ext fields preserved verbatim.
//! * The verb lists are closed: baseline verbs every participant answers, the
//!   capability-declared verb surface, and the *excluded* verbs the boundary
//!   refuses outright — **there is no raw pass-through** (§6.6 §2.1; vendor
//!   methods are capability-declared + debt-backed, never passthrough).
//! * Protocol negotiation (the session protocol the adapter speaks) is
//!   orthogonal to this negotiation — a session-ABI version never appears in
//!   `abi_versions`.

use std::collections::BTreeMap;
use std::fmt;

use hh_wire::Json;

/// The ABI version the Lab speaks.
pub const ABI_VERSION: &str = "hh-hosting/1";

/// The Lab-supported major.
pub const SUPPORTED_MAJOR: u64 = 1;

/// A `hh-hosting/<major>[.<minor>]` spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiVersion {
    /// The protocol major.
    pub major: u64,
    /// The declared minor (`0` when absent).
    pub minor: u64,
}

impl AbiVersion {
    /// Parse a `hh-hosting/n[.m]` spelling (`None` — never coerced).
    pub fn parse(s: &str) -> Option<AbiVersion> {
        let rest = s.strip_prefix("hh-hosting/")?;
        let (major, minor) = match rest.split_once('.') {
            Some((ma, mi)) => (ma.parse().ok()?, mi.parse().unwrap_or(0)),
            None => (rest.parse().ok()?, 0),
        };
        Some(AbiVersion { major, minor })
    }

    /// The canonical spelling.
    pub fn as_string(self) -> String {
        if self.minor == 0 {
            format!("hh-hosting/{}", self.major)
        } else {
            format!("hh-hosting/{}.{}", self.major, self.minor)
        }
    }
}

/// The negotiated attach — `version` is the mutually supported spelling and
/// `declarations_unknown` marks the no-advertisement path (§6.6: a participant
/// that advertises no ABI versions attaches at `hh-hosting/1` with every
/// declaration `unknown`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    /// The attached ABI version.
    pub version: AbiVersion,
    /// `true` when the participant advertised no ABI versions — every
    /// declaration starts `unknown`.
    pub declarations_unknown: bool,
}

/// The §6.6 handshake: pick the highest mutually supported major. `advertised`
/// is the participant's `abi_versions[]` (`ext hh.hosting/1`). An empty list
/// attaches at `hh-hosting/1` with `declarations_unknown`; an advertised major
/// the Lab does not speak refuses `AbiVersionUnsupported` (never silently
/// downgraded to a version the Lab cannot honor); an unknown minor attaches —
/// the fields the Lab does not understand preserve verbatim under `extra`.
pub fn negotiate(advertised: &[String]) -> Result<Negotiated, HostingError> {
    if advertised.is_empty() {
        return Ok(Negotiated {
            version: AbiVersion {
                major: SUPPORTED_MAJOR,
                minor: 0,
            },
            declarations_unknown: true,
        });
    }
    // Highest mutually supported: the best advertised spelling under the
    // Lab's major — majors the Lab does not speak are *not* candidates; if
    // the only advertised majors are foreign, the attach refuses.
    let mut best: Option<AbiVersion> = None;
    for s in advertised {
        match AbiVersion::parse(s) {
            Some(v) if v.major == SUPPORTED_MAJOR => {
                if best.map(|b| v.minor > b.minor).unwrap_or(true) {
                    best = Some(v);
                }
            }
            Some(_) => {}
            None => {
                return Err(HostingError::AbiVersionUnsupported {
                    spelling: s.clone(),
                })
            }
        }
    }
    match best {
        Some(v) => Ok(Negotiated {
            version: v,
            declarations_unknown: false,
        }),
        None => Err(HostingError::AbiVersionUnsupported {
            spelling: advertised.join(","),
        }),
    }
}

/// The baseline verbs — every participant answers these (§6.6 §2.1).
pub const BASELINE_VERBS: &[&str] = &[
    "describe",
    "open",
    "submit",
    "cancel",
    "resume",
    "close",
    "stream_events",
];

/// The Lab-side verb (`probe` is a Lab/driver verb, never participant-facing).
pub const LAB_VERBS: &[&str] = &["probe"];

/// The participant-facing upcalls — the participant calls *into* the Lab on
/// these (policy-first, mediated).
pub const UPCALLS: &[&str] = &["request_permission", "report_usage", "elicit"];

/// The capability-declared verbs — admissible only when the reconciled
/// `capability_vector` says `supported` for the dimension the verb gates on.
/// `set_coordinate` gates per-coordinate (`coordinate_<name>`).
pub const CAPABILITY_DECLARED_VERBS: &[&str] =
    &["set_coordinate", "steer", "account", "export", "elicit"];

/// The dimension a capability-declared verb gates on (`set_coordinate` is
/// per-coordinate — the caller appends `coordinate_<name>`).
pub fn verb_dimension(verb: &str) -> Option<String> {
    Some(match verb {
        "steer" => "steer".to_string(),
        "account" => "account_exact".to_string(),
        "export" => "trajectory_export".to_string(),
        "elicit" => "elicitation".to_string(),
        _ => return None,
    })
}

/// The excluded verbs — the closed §6.6 §2.1 list the boundary refuses
/// outright. No raw pass-through exists: a method the ABI does not name is
/// `ExcludedVerb` (vendor methods are capability-declared + debt-backed,
/// never passthrough).
pub const EXCLUDED_VERBS: &[&str] = &[
    "set_policy",
    "deliver_credential",
    "fork",
    "inject_tool",
    "read_ledger",
    "checkpoint",
    "authenticate",
    "install_extension",
    "inspect_artifacts",
    "spawn_subagent",
    "set_control_strategy",
    "compact",
    "grant",
    "set_budget",
];

/// `true` for every verb spelling the ABI admits — baseline, capability-
/// declared, the Lab verb, and the upcalls. `false` (→ `ExcludedVerb` when it
/// names a known excluded verb, else `UnknownVerb`) for everything else.
pub fn is_admitted_verb(v: &str) -> bool {
    BASELINE_VERBS.contains(&v)
        || CAPABILITY_DECLARED_VERBS.contains(&v)
        || LAB_VERBS.contains(&v)
        || UPCALLS.contains(&v)
        || v.starts_with("coordinate_")
}

/// `HostingError` — the service's typed error sum (R2; the boundary maps each
/// to a diagnostic, never a string).
#[derive(Debug, Clone, PartialEq)]
pub enum HostingError {
    /// An advertised ABI major the Lab does not speak.
    AbiVersionUnsupported {
        /// The refused spelling(s).
        spelling: String,
    },
    /// A verb the participant's capability vector does not declare
    /// `supported` (or a baseline-verb misuse — e.g. `resume` with an
    /// undeclared mode).
    CapabilityNotSupported {
        /// The verb.
        verb: String,
        /// The dimension the verb gates on.
        dimension: String,
    },
    /// A spelling on the excluded-verbs list — refused outright.
    ExcludedVerb {
        /// The verb.
        verb: String,
    },
    /// A method spelling the ABI does not name at all — not an excluded-verb
    /// spelling, still refused (there is no raw pass-through).
    UnknownVerb {
        /// The spelling.
        verb: String,
    },
    /// The session is not in a state the verb requires.
    SessionState {
        /// The session.
        session: String,
        /// What was wrong.
        detail: String,
    },
    /// The participant record is not admissible for attach (non-hosted,
    /// mechanism `none`, or a `descriptor` rule the record must satisfy).
    ParticipantInadmissible {
        /// Why.
        detail: String,
    },
    /// The adapter refused the attach or the adapter record's claims/debt
    /// are incomplete (I-6).
    AdapterRefused {
        /// Why.
        detail: String,
    },
    /// A policy row refused the participant's effect (`deny` — the
    /// participant observes the refusal as a protocol failure, never a
    /// policy disclosure).
    PolicyDenied {
        /// The refused action.
        detail: String,
    },
    /// A `permission_surface = none` participant attempted to raise a
    /// permission surface, or a permission request arrived on a participant
    /// the declaration says cannot ask.
    PermissionSurfaceAbsent {
        /// The session.
        session: String,
    },
    /// A credential channel the declaration does not name, or a credential
    /// value outside a declared `credential_supply` channel.
    CredentialChannelUndeclared {
        /// The channel attempted.
        channel: String,
    },
    /// `budget_enforcement` derivation: the mechanism×placement cannot
    /// enforce the floor (e.g. `time.wall_ms` on a mechanism with no
    /// Lab-owned clock channel).
    BudgetUnderivable {
        /// The dimension that failed.
        dimension: String,
        /// Why.
        detail: String,
    },
    /// A hard ceiling was hit at a boundary decision point — the run ends
    /// `budget_exhausted{dimension}` after `session.closed`.
    BudgetExhausted {
        /// The budget id.
        budget_id: String,
        /// The dimension.
        dimension: String,
    },
    /// A hosted event failed I-1…I-5 validation at ingest.
    InvalidEvent {
        /// The `HostedError`.
        detail: crate::events::HostedError,
    },
    /// The transport/adapter wire failed (in-process: the participant's
    /// session-protocol error, carried verbatim).
    Transport {
        /// The error.
        detail: String,
    },
    /// The verb's params failed their member-minimum check.
    SchemaViolation {
        /// The member path.
        path: String,
        /// What was expected.
        detail: String,
    },
}

impl fmt::Display for HostingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostingError::AbiVersionUnsupported { spelling } => {
                write!(f, "AbiVersionUnsupported({spelling})")
            }
            HostingError::CapabilityNotSupported { verb, dimension } => {
                write!(f, "CapabilityNotSupported({verb} on {dimension})")
            }
            HostingError::ExcludedVerb { verb } => write!(f, "ExcludedVerb({verb})"),
            HostingError::UnknownVerb { verb } => write!(f, "UnknownVerb({verb})"),
            HostingError::SessionState { session, detail } => {
                write!(f, "SessionState({session}: {detail})")
            }
            HostingError::ParticipantInadmissible { detail } => {
                write!(f, "ParticipantInadmissible({detail})")
            }
            HostingError::AdapterRefused { detail } => {
                write!(f, "AdapterRefused({detail})")
            }
            HostingError::PolicyDenied { detail } => write!(f, "PolicyDenied({detail})"),
            HostingError::PermissionSurfaceAbsent { session } => {
                write!(f, "PermissionSurfaceAbsent({session})")
            }
            HostingError::CredentialChannelUndeclared { channel } => {
                write!(f, "CredentialChannelUndeclared({channel})")
            }
            HostingError::BudgetUnderivable { dimension, detail } => {
                write!(f, "BudgetUnderivable({dimension}: {detail})")
            }
            HostingError::BudgetExhausted {
                budget_id,
                dimension,
            } => {
                write!(f, "BudgetExhausted({budget_id}.{dimension})")
            }
            HostingError::InvalidEvent { detail } => write!(f, "InvalidEvent({detail})"),
            HostingError::Transport { detail } => write!(f, "Transport({detail})"),
            HostingError::SchemaViolation { path, detail } => {
                write!(f, "SchemaViolation({path}: {detail})")
            }
        }
    }
}

impl std::error::Error for HostingError {}

impl From<crate::events::HostedError> for HostingError {
    fn from(e: crate::events::HostedError) -> HostingError {
        HostingError::InvalidEvent { detail: e }
    }
}

/// `open`'s `HostedRunSpec` — what the Lab hands the participant (§6.6 §2.1):
/// `definition_ref`, `params` (the arm params), `placement`,
/// `connection_info` (a Json the participant reads — **never a handle**),
/// `context_items[]`, `budget_view` (advisory — delivered only as a
/// `ContextItem` when `instruction_delivery` is `supported`; D1: budgets
/// never enter the participant's authority loop), `credentials` (channel
/// ids only — values flow on `credential_supply` channels, never inline).
#[derive(Debug, Clone, PartialEq)]
pub struct HostedRunSpec {
    /// The sealed definition the run executes (`version_id`/`content_ref`).
    pub definition_ref: String,
    /// The arm params (opaque `Json` — the definition reads them).
    pub params: Json,
    /// Where the process runs.
    pub placement: crate::records::ProcessPlacement,
    /// How to reach the environment — a `Json` connection record, never a
    /// capability handle (handle-keyed members refuse at decode).
    pub connection_info: Json,
    /// Lab-supplied context items (delivered per `context_supply_mode`).
    pub context_items: Vec<Json>,
    /// Advisory budget view — a `ContextItem` delivered only when
    /// `instruction_delivery` is `supported` (CF-073).
    pub budget_view: Option<Json>,
    /// Declared `credential_supply` channels the run may use (channel ids —
    /// never values).
    pub credential_channels: Vec<String>,
    /// The participant's resume cursor, when this `open` continues a prior
    /// session (the `resume` verb mints it — `open` never invents one).
    pub resume_cursor: Option<ResumeCursor>,
}

/// A `ResumeCursor` — the session-continuation token the Lab mints (§6.6
/// §2.4): `{session_ref, seq, at, mode}` — `cold` re-attaches the same
/// `session_ref` after a participant restart; `warm` continues on a live
/// transport; `none` starts clean.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumeCursor {
    /// The session being resumed.
    pub session_ref: String,
    /// The dense-seq watermark the resume starts after.
    pub seq: u64,
    /// The resume mode (`cold`/`warm`/`none` — declared on the capability
    /// vector as `resume_cold`/`resume_warm`).
    pub mode: String,
    /// Ext members — preserved, never deciding.
    pub ext: BTreeMap<String, Json>,
}

/// The participant's end state at `close` — the kernel's own snapshot
/// reading (§6.6 §2.2 `session.closed`): `{session_ref, reason, turns, usage?,
/// artifacts?, stop_reason}` — the Lab snapshot, never the participant's
/// self-report.
#[derive(Debug, Clone, PartialEq)]
pub struct EndState {
    /// The session.
    pub session_ref: String,
    /// The close reason spelling (`completed`/`cancelled`/`budget_exhausted`/
    /// `failed`/`abandoned` — lifted to `StopReason` at `lift`).
    pub reason: String,
    /// The turns the session ran.
    pub turns: u64,
    /// The session usage, when the participant reported any.
    pub usage: Option<Json>,
    /// The artifacts the session produced (refs).
    pub artifacts: Vec<Json>,
    /// The lifted `StopReason` Json, when a terminal fired.
    pub stop_reason: Option<Json>,
}

/// The `open` result — `{session_ref, capability_vector, end_state: none}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    /// The Lab-minted session ref.
    pub session_ref: String,
    /// The reconciled capability vector the session opened under.
    pub capability_vector: BTreeMap<String, Json>,
    /// The ABI version the handshake attached at.
    pub abi_version: String,
}

/// `connection_info` must never carry a handle — `handle`/`*_handle` members
/// refuse (§6.6 placement rule; `inject::refuse_handle_keys` precedent).
pub fn refuse_handle_keys(j: &Json, path: &str) -> Result<(), HostingError> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if k == "handle" || k.ends_with("_handle") || k == "handles" {
                    return Err(HostingError::SchemaViolation {
                        path: format!("{path}.{k}"),
                        detail: "a participant receives connection_info, never a handle"
                            .to_string(),
                    });
                }
                refuse_handle_keys(v, &format!("{path}.{k}"))?;
            }
            Ok(())
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                refuse_handle_keys(v, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_advertised_versions_attaches_at_v1_with_unknowns() {
        let n = negotiate(&[]).expect("empty attach");
        assert_eq!(n.version.major, 1);
        assert!(n.declarations_unknown);
    }

    #[test]
    fn unknown_major_refuses_and_v1_attaches() {
        assert!(matches!(
            negotiate(&["hh-hosting/2".to_string()]),
            Err(HostingError::AbiVersionUnsupported { .. })
        ));
        let n = negotiate(&["hh-hosting/1".to_string(), "hh-hosting/2".to_string()])
            .expect("a supported major exists");
        assert_eq!(n.version.major, 1);
        assert!(!n.declarations_unknown);
        // An unparseable spelling refuses — never coerced.
        assert!(matches!(
            negotiate(&["acp/1".to_string()]),
            Err(HostingError::AbiVersionUnsupported { .. })
        ));
    }

    #[test]
    fn unknown_minor_attaches_at_the_highest_mutual() {
        let n = negotiate(&["hh-hosting/1.3".to_string(), "hh-hosting/1.1".to_string()])
            .expect("minor attaches");
        assert_eq!(n.version.minor, 3);
    }

    #[test]
    fn the_verb_lists_are_closed_and_disjoint() {
        for v in BASELINE_VERBS
            .iter()
            .chain(LAB_VERBS)
            .chain(UPCALLS)
            .chain(CAPABILITY_DECLARED_VERBS)
        {
            assert!(
                !EXCLUDED_VERBS.contains(v),
                "admitted verb {v} on the excluded list"
            );
            assert!(is_admitted_verb(v));
        }
        for v in EXCLUDED_VERBS {
            assert!(!is_admitted_verb(v), "excluded {v} admitted");
        }
        assert!(!is_admitted_verb("vendor_method"));
        assert!(is_admitted_verb("coordinate_model"));
    }

    #[test]
    fn connection_info_never_carries_a_handle() {
        let ok = Json::obj([("socket", Json::str("/tmp/s")), ("env", Json::obj([]))]);
        assert!(refuse_handle_keys(&ok, "connection_info").is_ok());
        let bad = Json::obj([("tool_handle", Json::str("h:1"))]);
        assert!(matches!(
            refuse_handle_keys(&bad, "connection_info"),
            Err(HostingError::SchemaViolation { .. })
        ));
        let nested = Json::obj([(
            "inner",
            Json::obj([("handles", Json::Arr(vec![Json::str("h")]))]),
        )]);
        assert!(refuse_handle_keys(&nested, "connection_info").is_err());
    }
}
