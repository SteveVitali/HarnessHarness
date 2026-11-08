//! `fp_seal` — the first-pass outcome seal (manifest 284/R6.30, Stage
//! 1; spec §10.1.4 firewall + REQ-XH-121; contributing REQ-XH-114).
//!
//! REQ-XH-121 seals the outcome fields of first-pass agent-participant
//! slots in W2 and W4 — slot verdict, reward, `ctrf` counts, the
//! `executed{resolved|unresolved}` disposition members, verifier
//! output and every derivative — until FP-RESULTS (R6.48). Every gate
//! readout, the W4 checkpoint readout, committed summaries,
//! operator-console views and the site's internal profile carry cost
//! fields only and show outcome fields as
//! `withheld{sealed_until_results}`.
//!
//! This module makes the seal mechanical:
//!
//! - [`seal_view`] projects a record into the sealed view a declared
//!   consumer reads — outcome members are rendered the closed
//!   `withheld{sealed_until_results}` member (R6.8's
//!   [`crate::disposition::WithheldReason::SealedUntilResults`]), cost
//!   and infrastructure members pass through, and a record carrying an
//!   unmaskable outcome member refuses typed.
//! - [`check_unsealed`] admits the declared unsealed classes as typed
//!   members — W1 re-proof slots with their packet and discordance
//!   register, SM-* smoke records, the PS0 and HH-native lane probes,
//!   and PP0 AG3/AG5/AG6/AG8 evidence (exit state, receipts, egress
//!   rows, protocol errors — never the task verdict). A PP0 readout
//!   carrying a task verdict refuses.
//! - [`dev_projection`] yields the development-partition projection
//!   with verdict, reward and verifier fields *removed* (REQ-XH-114 /
//!   REQ-XH-121, Q-X52) — the projection R6.42 applies to the
//!   committed partition before H development sees it. Removed, never
//!   masked: the dev projection carries no withheld marker, it simply
//!   lacks the members.
//!
//! The firewall stays a refusal, not a filter (R6.29's
//! [`crate::continuation::check_cost_only`] is the same predicate):
//! infrastructure members (`inconclusive{infra:*}`, engine class
//! `infrastructure_failure`) are never outcome fields, so holds
//! (REQ-XH-124, REQ-XH-127) may read them while a verdict read
//! refuses. The seal opens only at FP-RESULTS; nothing here opens it.

use hharness_wire::json::Json;

use crate::continuation::OUTCOME_FIELDS;
use crate::disposition::WithheldReason;
use crate::json_ext::JsonExt;

/// `hh-fp-sealed-view/1` — the sealed projection every gate readout,
/// the W4 checkpoint readout, committed summaries, operator-console
/// views and the site's internal profile consume.
pub const SEALED_VIEW_SCHEMA: &str = "hh-fp-sealed-view/1";
/// `hh-fp-dev-projection/1` — the development-partition projection
/// (verdict, reward and verifier fields removed).
pub const DEV_PROJECTION_SCHEMA: &str = "hh-fp-dev-projection/1";

/// The sealed marker — R6.8's closed-set member, verbatim.
pub fn seal_marker() -> String {
    format!(
        "withheld{{{}}}",
        WithheldReason::SealedUntilResults.as_str()
    )
}

// ── consumers and record classes ─────────────────────────────────────

/// The sealed-view consumers REQ-XH-121 names — a closed set; any
/// other consumer spelling refuses [`SealError::UndeclaredConsumer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewKind {
    /// A gate readout (PROV-*, FP-S, FP-E, G-L1, PROV-Wc).
    GateReadout,
    /// The W4 round-boundary checkpoint readout (REQ-XH-112).
    CheckpointReadout,
    /// A committed summary.
    CommittedSummary,
    /// An operator-console view.
    OperatorConsole,
    /// The site's internal profile (through R6.5's generator — never a
    /// second site path).
    SiteInternal,
}

impl ViewKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::GateReadout => "gate_readout",
            ViewKind::CheckpointReadout => "checkpoint_readout",
            ViewKind::CommittedSummary => "committed_summary",
            ViewKind::OperatorConsole => "operator_console",
            ViewKind::SiteInternal => "site_internal",
        }
    }

    /// Every member — the closed set.
    pub const ALL: [ViewKind; 5] = [
        ViewKind::GateReadout,
        ViewKind::CheckpointReadout,
        ViewKind::CommittedSummary,
        ViewKind::OperatorConsole,
        ViewKind::SiteInternal,
    ];

    /// The strict parse; `None` outside the set.
    pub fn parse(s: &str) -> Option<ViewKind> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

/// The record classes the seal treats — a closed set. `AgentSlot` is
/// the sealed class (W2/W4 agent-participant slots and their
/// derivatives); the rest are the declared unsealed classes of
/// REQ-XH-121 (admission or machinery evidence, never claim data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SealClass {
    /// A W2/W4 agent-participant slot — sealed until FP-RESULTS.
    AgentSlot,
    /// A W1 oracle re-proof slot, its packet or its discordance
    /// register (REQ-XB-125, REQ-XH-116) — unsealed.
    Reproof,
    /// An SM-* $0 smoke-battery record (REQ-XC-324) — unsealed.
    Smoke,
    /// A PS0 or HH-native lane-probe record — unsealed.
    Probe,
    /// PP0 AG3/AG5/AG6/AG8 evidence — unsealed *as machinery
    /// evidence*: exit state, receipts, egress rows and protocol
    /// errors. A task verdict inside it refuses.
    Pp0AgEvidence,
}

impl SealClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SealClass::AgentSlot => "agent_slot",
            SealClass::Reproof => "reproof",
            SealClass::Smoke => "smoke",
            SealClass::Probe => "probe",
            SealClass::Pp0AgEvidence => "pp0_ag_evidence",
        }
    }

    /// Every member — the closed set.
    pub const ALL: [SealClass; 5] = [
        SealClass::AgentSlot,
        SealClass::Reproof,
        SealClass::Smoke,
        SealClass::Probe,
        SealClass::Pp0AgEvidence,
    ];

    /// The strict parse; `None` outside the set.
    pub fn parse(s: &str) -> Option<SealClass> {
        Self::ALL.iter().find(|c| c.as_str() == s).copied()
    }
}

// ── refusals ─────────────────────────────────────────────────────────

/// A typed seal refusal (X-07: the firewall is mechanical, never a
/// silent filter).
#[derive(Debug, Clone, PartialEq)]
pub enum SealError {
    /// An outcome member reached a sealed consumer unmasked.
    OutcomeField {
        /// The dotted path of the refused member.
        path: String,
    },
    /// A consumer or class spelling outside the closed set.
    UndeclaredConsumer {
        /// The rejected spelling.
        spelling: String,
    },
    /// A record class outside [`SealClass::ALL`].
    UndeclaredClass {
        /// The rejected spelling.
        spelling: String,
    },
    /// A shape failure on strict decode.
    Schema {
        /// The verbatim detail.
        detail: String,
    },
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::OutcomeField { path } => write!(
                f,
                "refused{{outcome_field}}: {path} — sealed consumers carry cost fields only (REQ-XH-121)"
            ),
            SealError::UndeclaredConsumer { spelling } => {
                write!(f, "refused{{seal_consumer}}: {spelling} is undeclared")
            }
            SealError::UndeclaredClass { spelling } => {
                write!(f, "refused{{seal_class}}: {spelling} is undeclared")
            }
            SealError::Schema { detail } => write!(f, "{SEALED_VIEW_SCHEMA}: {detail}"),
        }
    }
}
impl std::error::Error for SealError {}

fn schema(detail: impl Into<String>) -> SealError {
    SealError::Schema {
        detail: detail.into(),
    }
}

// ── the seal ─────────────────────────────────────────────────────────

/// `true` when `k` names an outcome member under the REQ-XH-121 sealed
/// set — R6.29's [`OUTCOME_FIELDS`] plus the derivative spellings
/// (`success`, `classification`, `pass`, `grade` members carrying a
/// task outcome).
fn is_outcome_member(k: &str) -> bool {
    OUTCOME_FIELDS.contains(&k)
        || matches!(
            k,
            "success" | "classification" | "pass" | "passed" | "reward_ppm" | "reward_raw"
        )
        || k.starts_with("reward")
        || k.starts_with("verifier")
        || k.contains("outcome")
}

/// `true` when a disposition string is an outcome member —
/// `executed{resolved|unresolved}`. `inconclusive{infra:*}`,
/// `withheld{…}` and `failed{…}` are not outcome fields (REQ-XH-121's
/// carve-out: holds read the infrastructure members).
fn disposition_is_outcome(s: &str) -> bool {
    s.starts_with("executed{")
}

/// The members a sealed view shows instead of an outcome member — the
/// closed `withheld{sealed_until_results}` marker.
fn masked() -> Json {
    Json::str(seal_marker())
}

/// `seal_view(record, view)` — project `record` into the sealed view
/// `view` reads. Every outcome member becomes
/// `withheld{sealed_until_results}` at its position; cost,
/// infrastructure, identity and declared members pass through
/// unchanged. A member that *is* an outcome value but cannot be
/// masked in place (a disposition `executed{…}`) masks at its
/// `value` member — the envelope stays readable to holds.
///
/// The returned object is stamped `{schema, view, record}`: the
/// projection is a new record, never a mutation of its input.
pub fn seal_view(record: &Json, view: ViewKind) -> Result<Json, SealError> {
    let projected = seal_node(record, "")?;
    Ok(Json::obj([
        ("schema", Json::str(SEALED_VIEW_SCHEMA)),
        ("view", Json::str(view.as_str())),
        ("record", projected),
    ]))
}

fn seal_node(j: &Json, path: &str) -> Result<Json, SealError> {
    match j {
        Json::Obj(m) => {
            let mut out: Vec<(String, Json)> = Vec::with_capacity(m.len());
            for (k, v) in m {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if is_outcome_member(k) {
                    out.push((k.clone(), masked()));
                    continue;
                }
                if k == "value" {
                    if let Some(s) = v.as_str() {
                        if disposition_is_outcome(s) {
                            out.push((k.clone(), masked()));
                            continue;
                        }
                    }
                }
                out.push((k.clone(), seal_node(v, &p)?));
            }
            Ok(Json::Obj(out.into_iter().collect()))
        }
        Json::Arr(a) => {
            let mut out = Vec::with_capacity(a.len());
            for (i, v) in a.iter().enumerate() {
                out.push(seal_node(v, &format!("{path}[{i}]"))?);
            }
            Ok(Json::Arr(out))
        }
        _ => Ok(j.clone()),
    }
}

/// `check_sealed(j)` — the schema check a sealed consumer runs: an
/// outcome member *unmasked* — its value is not the closed
/// `withheld{sealed_until_results}` marker — refuses typed, at any
/// depth. A member carrying the marker is the REQ-XH-121 rendering
/// and passes. This is the view-side twin of R6.29's
/// [`crate::continuation::check_cost_only`] (which refuses the member outright for
/// *inputs*); one firewall, two doors (CC1).
pub fn check_sealed(j: &Json) -> Result<(), SealError> {
    check_no_outcome(j, "")
}

fn check_no_outcome(j: &Json, path: &str) -> Result<(), SealError> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                let masked_member = v.as_str() == Some(seal_marker().as_str());
                if is_outcome_member(k) && !masked_member {
                    return Err(SealError::OutcomeField { path: p });
                }
                if k == "value" {
                    if let Some(s) = v.as_str() {
                        if disposition_is_outcome(s) {
                            return Err(SealError::OutcomeField { path: p });
                        }
                    }
                }
                check_no_outcome(v, &p)?;
            }
            Ok(())
        }
        Json::Arr(a) => {
            for (i, v) in a.iter().enumerate() {
                check_no_outcome(v, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ── the declared unsealed classes ────────────────────────────────────

/// `check_unsealed(record, class)` — admit `record` as a member of the
/// declared unsealed class `class`. The class must be one of the
/// REQ-XH-121 carve-outs — an [`SealClass::AgentSlot`] here refuses
/// (agent slots go through [`seal_view`]), and a PP0 evidence record
/// carrying a task verdict refuses (AG evidence reads exit state,
/// receipts, egress rows and protocol errors, never the verdict).
pub fn check_unsealed(record: &Json, class: SealClass) -> Result<(), SealError> {
    match class {
        SealClass::AgentSlot => Err(SealError::Schema {
            detail: "agent slots are the sealed class — project them, never read raw".to_string(),
        }),
        SealClass::Pp0AgEvidence => {
            // AG evidence = exit state, receipts, egress rows,
            // protocol errors — never the task verdict.
            check_no_outcome(record, "")
        }
        SealClass::Reproof | SealClass::Smoke | SealClass::Probe => {
            // Admission/machinery evidence: outcome members are
            // readable by declaration (REQ-XH-121's carve-out). The
            // record must still be an object.
            if record.as_obj().is_none() {
                return Err(schema("unsealed record must be an object"));
            }
            Ok(())
        }
    }
}

// ── the development-partition projection ─────────────────────────────

/// The members [`dev_projection`] removes — verdict, reward and
/// verifier fields and their derivatives (REQ-XH-121: "a projection
/// with verdict, reward and verifier fields removed"). Removal, not
/// masking: H development never sees that a field existed.
fn is_dev_stripped(k: &str) -> bool {
    OUTCOME_FIELDS.contains(&k)
        || k.starts_with("reward")
        || k.starts_with("verifier")
        || matches!(k, "success" | "classification" | "pass" | "passed")
        || k.contains("outcome")
}

/// `dev_projection(record)` — the development-partition projection:
/// `record` minus every verdict, reward and verifier member
/// recursively; a disposition `executed{…}` drops the disposition
/// member's `value` (an `inconclusive{infra:*}` disposition stays —
/// infrastructure is not an outcome field). Stamped
/// `{schema: hh-fp-dev-projection/1, record}`.
pub fn dev_projection(record: &Json) -> Json {
    Json::obj([
        ("schema", Json::str(DEV_PROJECTION_SCHEMA)),
        ("record", dev_strip(record)),
    ])
}

fn dev_strip(j: &Json) -> Json {
    match j {
        Json::Obj(m) => {
            let mut out: Vec<(String, Json)> = Vec::new();
            for (k, v) in m {
                if is_dev_stripped(k) {
                    continue;
                }
                if k == "value" {
                    if let Some(s) = v.as_str() {
                        if disposition_is_outcome(s) {
                            continue;
                        }
                    }
                }
                out.push((k.clone(), dev_strip(v)));
            }
            Json::Obj(out.into_iter().collect())
        }
        Json::Arr(a) => Json::Arr(a.iter().map(dev_strip).collect()),
        _ => j.clone(),
    }
}

/// `check_dev_projection(j)` — the schema check R6.42's consumer
/// runs: a verdict, reward or verifier member anywhere, or a
/// disposition `executed{…}`, refuses typed. The dev projection
/// carries *no* member of the sealed set — removed, not masked.
pub fn check_dev_projection(j: &Json) -> Result<(), SealError> {
    check_no_outcome(j, "")
}
