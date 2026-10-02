//! `CandidateState` — the §05h candidate state machine (S6.1a;
//! AC-R-2.9.5-1). The closed sum plus the legality table the campaign
//! driver consults before every `transitioned` mint: the durable row is
//! the transition's *record*; this table is the only place a transition's
//! legality is decided (CC1).
//!
//! ```text
//! proposed → classified → hypothesized → screened → searched
//!   → validated → transferred → security_checked → sealed_candidate
//!   → canary → active → expiring → {retired | revalidated → active}
//! terminals: rejected{stage,code,report_ref?} | withdrawn{by}
//!            | reverted{from ∈ {canary, active}, reason}
//! ```

use hh_wire::json::Json;
use std::collections::BTreeMap;

/// The state spellings — the `to`/`from` member vocabulary of
/// `measurement.evolution.candidate.transitioned`.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateState {
    /// Intake — the proposal is registered in lineage.
    Proposed,
    /// The S1 classification gates passed (change-class + locality +
    /// apply/invert).
    Classified,
    /// A falsifiable hypothesis is bound (S2).
    Hypothesized,
    /// The targeted-counterexample screen passed (S3).
    Screened,
    /// The matched-total search/eval stage passed (S4).
    Searched,
    /// The held-out `artifact_benefit` + retention + veto gates passed
    /// (S5).
    Validated,
    /// Transfer + compatibility evidence recorded (S6).
    Transferred,
    /// The security invariance check passed (S7).
    SecurityChecked,
    /// The human seal + complete acceptance report landed (S8).
    SealedCandidate,
    /// The shadow canary is running (S9).
    Canary,
    /// Rolled out — the candidate is the serving definition (S9).
    Active,
    /// A retirement trigger fired (S10).
    Expiring,
    /// The removal test kept the candidate — return to service.
    Revalidated,
    /// The removal test retired the candidate (terminal).
    Retired,
    /// A gate refused the candidate (terminal) — `{stage, code,
    /// report_ref?}`.
    Rejected {
        /// The stage that refused (S0…S10/structural).
        stage: String,
        /// The typed refusal code.
        code: String,
        /// The refusal report's content ref, when one exists.
        report_ref: Option<String>,
    },
    /// The proposer withdrew (terminal).
    Withdrawn {
        /// Who withdrew.
        by: String,
    },
    /// A canary/active candidate reverted to its last human-sealed
    /// ancestor (terminal for this candidate).
    Reverted {
        /// The state reverted from (`canary` | `active`).
        from: String,
        /// The reason.
        reason: String,
    },
}

impl CandidateState {
    /// The canonical `to`/`from` spelling on the durable row.
    pub fn name(&self) -> &'static str {
        match self {
            CandidateState::Proposed => "proposed",
            CandidateState::Classified => "classified",
            CandidateState::Hypothesized => "hypothesized",
            CandidateState::Screened => "screened",
            CandidateState::Searched => "searched",
            CandidateState::Validated => "validated",
            CandidateState::Transferred => "transferred",
            CandidateState::SecurityChecked => "security_checked",
            CandidateState::SealedCandidate => "sealed_candidate",
            CandidateState::Canary => "canary",
            CandidateState::Active => "active",
            CandidateState::Expiring => "expiring",
            CandidateState::Revalidated => "revalidated",
            CandidateState::Retired => "retired",
            CandidateState::Rejected { .. } => "rejected",
            CandidateState::Withdrawn { .. } => "withdrawn",
            CandidateState::Reverted { .. } => "reverted",
        }
    }

    /// Whether the state is terminal for the candidate.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CandidateState::Rejected { .. }
                | CandidateState::Withdrawn { .. }
                | CandidateState::Reverted { .. }
                | CandidateState::Retired
        )
    }

    /// Parse the `to`/`from` spelling (terminal detail is read off the
    /// row's own `code`/`report_ref`/`by`/`reason` members).
    pub fn parse(s: &str) -> Option<&'static str> {
        match s {
            "proposed" => Some("proposed"),
            "classified" => Some("classified"),
            "hypothesized" => Some("hypothesized"),
            "screened" => Some("screened"),
            "searched" => Some("searched"),
            "validated" => Some("validated"),
            "transferred" => Some("transferred"),
            "security_checked" => Some("security_checked"),
            "sealed_candidate" => Some("sealed_candidate"),
            "canary" => Some("canary"),
            "active" => Some("active"),
            "expiring" => Some("expiring"),
            "revalidated" => Some("revalidated"),
            "retired" => Some("retired"),
            "rejected" => Some("rejected"),
            "withdrawn" => Some("withdrawn"),
            "reverted" => Some("reverted"),
            _ => None,
        }
    }
}

/// `allowed(from, to)` — the legality table. `to` is the state spelling
/// (`rejected`/`withdrawn`/`reverted` are the terminal legs).
pub fn allowed(from: &CandidateState, to: &str) -> bool {
    let to = match CandidateState::parse(to) {
        Some(t) => t,
        None => return false,
    };
    if from.is_terminal() {
        return false; // nothing leaves a terminal state
    }
    match to {
        "rejected" => true, // a gate may refuse from any live state
        "withdrawn" => !matches!(from, CandidateState::Canary | CandidateState::Active),
        "reverted" => matches!(from, CandidateState::Canary | CandidateState::Active),
        _ => matches!(
            (from, to),
            (CandidateState::Proposed, "classified")
                | (CandidateState::Classified, "hypothesized")
                | (CandidateState::Hypothesized, "screened")
                | (CandidateState::Screened, "searched")
                | (CandidateState::Searched, "validated")
                | (CandidateState::Validated, "transferred")
                | (CandidateState::Transferred, "security_checked")
                | (CandidateState::SecurityChecked, "sealed_candidate")
                | (CandidateState::SealedCandidate, "canary")
                | (CandidateState::Canary, "active")
                | (CandidateState::Active, "expiring")
                | (CandidateState::Expiring, "retired")
                | (CandidateState::Expiring, "revalidated")
                | (CandidateState::Revalidated, "active")
        ),
    }
}

/// One durable transition as folded — the member set of a
/// `candidate.transitioned` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// The transition's `from` spelling (`""` on the intake row).
    pub from: String,
    /// The `to` spelling.
    pub to: String,
    /// The stage that minted the row (S0…S10/intake).
    pub stage: String,
    /// The refusal `code` (rejected rows only).
    pub code: Option<String>,
    /// The report/evidence ref the transition cites.
    pub report_ref: Option<String>,
    /// The bound hypothesis ref (hypothesized rows).
    pub hypothesis_ref: Option<String>,
    /// The corpus evidence refs (hypothesized rows).
    pub evidence_refs: Vec<String>,
    /// The slot the candidate claimed (proposed rows).
    pub slot: Option<String>,
    /// `by` (withdrawn) / revert detail carrier.
    pub by: Option<String>,
    /// `reason` (rejected/reverted).
    pub reason: Option<String>,
    /// The event that minted the transition.
    pub event_id: String,
}

impl Transition {
    /// Decode the payload members the fold consumes (tolerant — the fold
    /// is over durable rows, all members optional).
    pub fn from_event(e: &hh_ledger::event::EventEnvelope) -> Transition {
        let p = &e.payload;
        let s = |k: &str| p.get(k).and_then(Json::as_str).map(str::to_string);
        Transition {
            from: s("from").unwrap_or_default(),
            to: s("to").unwrap_or_default(),
            stage: s("stage").unwrap_or_default(),
            code: s("code"),
            report_ref: s("report_ref"),
            hypothesis_ref: s("hypothesis_ref"),
            evidence_refs: match p.get("evidence_refs") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            },
            slot: s("slot"),
            by: s("by"),
            reason: s("reason"),
            event_id: e.event_id.clone(),
        }
    }

    /// The row's own `{from, to, stage, …}` member map (sans event_id).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("from".into(), Json::str(&self.from));
        m.insert("to".into(), Json::str(&self.to));
        m.insert("stage".into(), Json::str(&self.stage));
        for (k, v) in [
            ("code", &self.code),
            ("report_ref", &self.report_ref),
            ("hypothesis_ref", &self.hypothesis_ref),
            ("slot", &self.slot),
            ("by", &self.by),
            ("reason", &self.reason),
        ] {
            if let Some(s) = v {
                m.insert(k.into(), Json::str(s));
            }
        }
        if !self.evidence_refs.is_empty() {
            m.insert(
                "evidence_refs".into(),
                Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
            );
        }
        m.insert("event_id".into(), Json::str(&self.event_id));
        Json::Obj(m)
    }
}
