//! `propose_retirement` + the retirement gate (S6.1b; §5h.6 §2;
//! ADR-0197 D6/D7; AC-R-2.9.6-4): an evolution-origin removal arrives as
//! a *proposal* — the gate demands a `pass` verdict on the debt, a diff
//! that removes exactly the debt's rule and widens nothing — and returns
//! a `RetirementProposal` the caller deposits; actual retirement is the
//! human-sealed `hh_lab::debt::retire` half (`retire` in `manager.rs`).
//! Proposal and deployment are structurally separated: a proposal carries
//! provenance and is auditable, and it never mutates a status.

use std::collections::BTreeSet;

use hh_ontology::debt::{RemovalVerdict, Verdict};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::errors::{DebtManagerError, Refusal};

/// `ProposalDiff` — the caller's projection of the proposal's `HirDiff`
/// (records-in: the manager reads no diff store; the evolution pipeline
/// projects its diff into this shape). The gate is conservative: a
/// retirement diff removes *exactly* the debt's rule and widens nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalDiff {
    /// The rule ids the diff removes (`RemoveNode` ops' `rule_id`
    /// projection — exactly `{record.rule_id}` for a retirement).
    pub removed_rules: BTreeSet<String>,
    /// Whether the diff widens any authority handle (`true` ⇒ refusal —
    /// a retirement never widens).
    pub widens_authority: bool,
    /// Whether the diff loosens any budget node (`true` ⇒ refusal).
    pub loosens_budget: bool,
    /// The diff ref (`diff:<id>` — provenance of the proposal).
    pub diff_ref: String,
}

/// `RetirementProposal` — the proposal record `propose_retirement`
/// returns (records-out; the caller deposits it through LabDocs/routes it
/// to the human gate). `state` is always `proposed` — there is no
/// deployment path through the manager.
#[derive(Debug, Clone, PartialEq)]
pub struct RetirementProposal {
    /// The debt the proposal retires.
    pub debt_ref: String,
    /// The rule the diff removes (== the debt's `rule_id`).
    pub rule_id: String,
    /// The proposal diff ref.
    pub diff_ref: String,
    /// The `pass` verdict the proposal rests on (`report_ref` — the
    /// evidence the human seal cites).
    pub evidence_report_ref: String,
    /// The proposer's provenance (`origin = evolution` is the common case —
    /// recorded, never silently upgraded).
    pub proposed_by: ProvenanceRecord,
    /// The proposal state — always `proposed`.
    pub state: &'static str,
}

impl RetirementProposal {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("debt_ref", Json::str(&self.debt_ref)),
            ("rule_id", Json::str(&self.rule_id)),
            ("diff_ref", Json::str(&self.diff_ref)),
            ("evidence_report_ref", Json::str(&self.evidence_report_ref)),
            ("proposed_by", self.proposed_by.to_json()),
            ("state", Json::str(self.state)),
        ])
    }
}

/// `propose_retirement(debt_ref, rule_id, diff, verdicts, proposed_by) →
/// RetirementProposal` — the gate (§5h.6 §2): `RetirementNotEvidenced`
/// without a `pass` verdict on the debt; `NotARetirementDiff` when the
/// diff removes anything but exactly `rule_id` or widens
/// authority/loosens a budget. The admitted outcome is a *proposal* —
/// the human-sealed `retire` is the only `→ retired` path.
pub fn propose_retirement(
    debt_ref: &str,
    rule_id: &str,
    diff: &ProposalDiff,
    verdicts: &[RemovalVerdict],
    proposed_by: ProvenanceRecord,
) -> Result<RetirementProposal, DebtManagerError> {
    let verdict = verdicts
        .iter()
        .find(|v| v.debt_ref == debt_ref && v.verdict == Verdict::Pass)
        .ok_or_else(|| Refusal::RetirementNotEvidenced {
            debt_ref: debt_ref.to_string(),
        })?;
    if diff.widens_authority {
        return Err(Refusal::NotARetirementDiff {
            detail: "the proposal diff widens an authority handle".to_string(),
        }
        .into());
    }
    if diff.loosens_budget {
        return Err(Refusal::NotARetirementDiff {
            detail: "the proposal diff loosens a budget node".to_string(),
        }
        .into());
    }
    let expected: BTreeSet<String> = [rule_id.to_string()].into_iter().collect();
    if diff.removed_rules != expected {
        return Err(Refusal::NotARetirementDiff {
            detail: format!(
                "the diff removes {:?}, not exactly {{{rule_id}}}",
                diff.removed_rules
            ),
        }
        .into());
    }
    Ok(RetirementProposal {
        debt_ref: debt_ref.to_string(),
        rule_id: rule_id.to_string(),
        diff_ref: diff.diff_ref.clone(),
        evidence_report_ref: verdict.report_ref.clone(),
        proposed_by,
        state: "proposed",
    })
}
