//! The assumption-debt validation machinery (R-2.9.6⁰ᵃ, §5h.6 §4): `DebtHomes/1`
//! membership + per-home completeness (`validate_for_home`), and
//! `validate_removal_test` — the static/dry-run instantiation check wired into
//! `seal`/`register`. The refusal vocabulary is typed ([`DebtError`]); every
//! violation is a typed refusal, never a warning (CC3).
//!
//! Context-parameterized design: the checks a `seal`/`register` call can honor
//! depend on what the caller can resolve. [`RemovalTestContext`] carries the
//! resolvers — an absent resolver *skips* the check (the member-level checks —
//! payload instantiation, `beneficiaries ⊆ scope`, `missing_snapshot_scope`,
//! `InsufficientRunway`, `OwnerUnreachable` — always run); a resolver that
//! reports "absent" produces the corresponding `UnexecutableRemovalTest` reason.
//! Full template/probe resolution lands with the Stage-3 experiment registry.

use std::collections::BTreeSet;

use hh_ontology::debt::{
    debt_home, required_fields, DebtHome, DebtPolicy, DebtScope, ExpiryKind, RemovalTestKind,
    UnexecutableReason,
};

use crate::leaves::Text;
use crate::records::AssumptionDebtRecord;
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

/// `RetirementRecord{removal_test_report_ref, verdict_ref, decided_by,
/// rationale}` — the human-sealed discharge record a `retired` debt carries
/// (§5h.6 §3; `supersedes{reason: expiry}` chains reference it).
#[derive(Debug, Clone, PartialEq)]
pub struct RetirementRecord {
    /// The removal-test report the retirement rests on.
    pub removal_test_report_ref: String,
    /// The `RemovalVerdict` ref.
    pub verdict_ref: String,
    /// Who retired it — human provenance (retirement requires human sealing;
    /// never an automated deletion).
    pub decided_by: ProvenanceRecord,
    /// The rationale (prose — `Text`, provenance-bearing).
    pub rationale: Text,
}

impl RetirementRecord {
    /// The canonical JSON.
    pub fn to_json(&self, semantic: bool) -> Json {
        Json::obj([
            (
                "removal_test_report_ref",
                Json::str(self.removal_test_report_ref.clone()),
            ),
            ("verdict_ref", Json::str(self.verdict_ref.clone())),
            ("decided_by", self.decided_by.to_json()),
            (
                "rationale",
                crate::schema::text_json(&self.rationale, semantic),
            ),
        ])
    }
}

/// The debt-validation refusal vocabulary (§5h.6 §4): every refusal is a
/// typed variant, never a string match.
#[derive(Debug, Clone, PartialEq)]
pub enum DebtError {
    /// `UnknownDebtHome` — a debt record on a `(record_kind, field)` pair the
    /// `DebtHomes/1` inventory does not list.
    UnknownDebtHome {
        /// The record kind that carried the debt.
        record_kind: String,
        /// The field that carried it.
        field: String,
    },
    /// `MissingField` — a field the home's `required_fields` demands is absent.
    MissingField {
        /// The home row.
        home: &'static str,
        /// The missing field.
        field: String,
    },
    /// `EnumOutsideClosedSet` — a `debt_class`/`deficiency_class`/`status`/
    /// kind spelling outside its closed sum (decode-time refusal).
    EnumOutsideClosedSet {
        /// The field.
        field: String,
        /// The offending value.
        value: String,
    },
    /// `UnexecutableRemovalTest{reason}` — the removal test cannot
    /// instantiate at seal/register.
    UnexecutableRemovalTest {
        /// The closed reason.
        reason: UnexecutableReason,
        /// Human-readable detail.
        detail: String,
    },
    /// `InsufficientRunway` — `expiry.params.until − created_at` is below
    /// `DebtPolicy.min_runway`.
    InsufficientRunway {
        /// The declared runway (ms).
        runway_ms: u64,
        /// The policy minimum (ms).
        min_ms: u64,
    },
    /// `OwnerUnreachable` — the owner is not reachable through any declared
    /// notice sink.
    OwnerUnreachable {
        /// The owner id.
        owner_id: String,
    },
}

/// The view of a resolved experiment template `validate_removal_test` needs —
/// the minimal projection a `retirement_experiment`'s `template_ref` resolves
/// to.
#[derive(Debug, Clone)]
pub struct TemplateView {
    /// The resolved experiment ref.
    pub experiment_ref: String,
    /// Whether the template's arms carry a `match_spec` (a retirement
    /// experiment is a matched-budget design — absent → `MissingMatchSpec`).
    pub has_match_spec: bool,
}

/// The resolution context `validate_removal_test` runs against — the
/// resolvers the calling validation point (seal/register/manager) can honor.
/// An absent resolver *skips* the check; a resolver that reports absence
/// produces the corresponding `UnexecutableRemovalTest` reason.
/// The `resolve_template` resolver signature — `ref → view` (the `Option`
/// payload mirrors "known/unknown" at the calling validation point).
pub type TemplateResolver<'a> = dyn Fn(&str) -> Option<TemplateView> + 'a;

#[derive(Default)]
pub struct RemovalTestContext<'a> {
    /// The `DebtPolicy` in force.
    pub policy: Option<&'a DebtPolicy>,
    /// `now` (transaction time) for `InsufficientRunway`.
    pub now_ms: u64,
    /// Resolve an experiment `template_ref` — `None` resolver → the check is
    /// skipped (no template registry at this validation point); a resolver
    /// returning `None` → `UnresolvedTemplate`.
    pub resolve_template: Option<&'a TemplateResolver<'a>>,
    /// Whether a probe ref resolves — same skip/refuse contract.
    pub probe_ref_known: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether a `zero_uses` scope resolves.
    pub zero_use_scope_resolved: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether the split assignment the test needs exists —
    /// `Some(false)` → `MissingSplitAssignment`.
    pub split_assigned: Option<bool>,
    /// Whether every artifact the test needs is sealed —
    /// `Some(false)` → `UnsealedArtifact`.
    pub artifact_sealed: Option<bool>,
    /// The rule ids the candidate diff removes (for `NotARetirementDiff`) —
    /// `Some(set)` must equal `{rule_id}`.
    pub diff_removed_rules: Option<&'a BTreeSet<String>>,
    /// The declared notice-sink ids (`OwnerUnreachable` checks
    /// `owner.reach_via` against them).
    pub declared_sinks: Option<&'a BTreeSet<String>>,
}

impl RemovalTestContext<'_> {
    /// The default member-level context (no resolvers — the seal/register
    /// path's Stage-1 honest wiring: member-level checks only).
    pub fn member_level() -> RemovalTestContext<'static> {
        RemovalTestContext {
            policy: None,
            now_ms: 0,
            resolve_template: None,
            probe_ref_known: None,
            zero_use_scope_resolved: None,
            split_assigned: None,
            artifact_sealed: None,
            diff_removed_rules: None,
            declared_sinks: None,
        }
    }
}

/// `validate_removal_test` — the static/dry-run instantiation check
/// (§5h.6 §4). Runs the member-level checks always; context-parameterized
/// checks run where the caller supplies the resolver.
pub fn validate_removal_test(
    record: &AssumptionDebtRecord,
    home: &DebtHome,
    ctx: &RemovalTestContext<'_>,
) -> Result<(), DebtError> {
    // `missing_snapshot_scope` — an evolution-origin record or an explicitly
    // `model_conditioned` debt must carry `scope.model_selectors` (the G4
    // conditioned-on rule; AC-R-2.9.6-2). The home's `needs_model_scope`
    // marks where the scope is *expected*; the refusal fires on the record's
    // own conditioning.
    let needs_scope = record.debt_class == Some(hh_ontology::debt::DebtClass::ModelConditioned)
        || record
            .created_by
            .as_ref()
            .map(|p| matches!(p.origin, hh_provenance::Origin::Evolution { .. }))
            .unwrap_or(false);
    if needs_scope {
        let has_scope = record
            .scope
            .as_ref()
            .map(|s| !s.model_selectors.is_empty())
            .unwrap_or(false);
        if !has_scope {
            return Err(DebtError::UnexecutableRemovalTest {
                reason: UnexecutableReason::MissingSnapshotScope,
                detail: format!(
                    "{}.{} is model-conditioned/evolution-origin but carries no \
                     scope.model_selectors",
                    home.record_kind, home.field
                ),
            });
        }
    }
    let test = match &record.removal_test {
        Some(t) => t,
        None => return Ok(()), // `removal_test_ref` alone is the ratified member;
                               // the typed `removal_test` is additive.
    };
    // Member-level instantiation — the kind's mandatory payload.
    if !test.instantiates() {
        return Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::MissingPayload,
            detail: format!(
                "removal_test.kind {} lacks its mandatory payload member",
                test.kind.name()
            ),
        });
    }
    // `beneficiaries ⊆ scope` — every claimed `(factor, level)` beneficiary
    // must be covered by the record's scope.
    if !test.beneficiaries.is_empty() {
        let scope = record.scope.clone().unwrap_or_default();
        for b in &test.beneficiaries {
            if !beneficiary_covered(&scope, b) {
                return Err(DebtError::UnexecutableRemovalTest {
                    reason: UnexecutableReason::BeneficiariesOutsideScope,
                    detail: format!("beneficiary {b} is not covered by scope"),
                });
            }
        }
    }
    // Context checks — run where the caller supplies the resolver.
    match test.kind {
        RemovalTestKind::RetirementExperiment => {
            if let Some(resolve) = ctx.resolve_template {
                let template = test.template_ref.as_deref().and_then(resolve);
                let template = template.ok_or_else(|| DebtError::UnexecutableRemovalTest {
                    reason: UnexecutableReason::UnresolvedTemplate,
                    detail: format!(
                        "template_ref {} does not resolve to an experiment template",
                        test.template_ref.as_deref().unwrap_or("<absent>")
                    ),
                })?;
                if !template.has_match_spec {
                    return Err(DebtError::UnexecutableRemovalTest {
                        reason: UnexecutableReason::MissingMatchSpec,
                        detail: format!(
                            "template {} arms carry no match_spec",
                            template.experiment_ref
                        ),
                    });
                }
            }
            if let Some(diff) = ctx.diff_removed_rules {
                let expected: BTreeSet<String> = [record.rule_id.clone()].into_iter().collect();
                if *diff != expected {
                    return Err(DebtError::UnexecutableRemovalTest {
                        reason: UnexecutableReason::NotARetirementDiff,
                        detail: format!(
                            "the diff removes {diff:?}, not exactly the conditioned rule {}",
                            record.rule_id
                        ),
                    });
                }
            }
            if let Some(false) = ctx.split_assigned {
                return Err(DebtError::UnexecutableRemovalTest {
                    reason: UnexecutableReason::MissingSplitAssignment,
                    detail: "the retirement experiment's suite has no split assignment".into(),
                });
            }
        }
        RemovalTestKind::ProbeRun | RemovalTestKind::ModelProbe | RemovalTestKind::ChallengeRun => {
            if let Some(known) = ctx.probe_ref_known {
                for r in &test.probe_refs {
                    if !known(r) {
                        return Err(DebtError::UnexecutableRemovalTest {
                            reason: UnexecutableReason::UnresolvedProbeRef,
                            detail: format!("probe ref {r} does not resolve"),
                        });
                    }
                }
            }
        }
        RemovalTestKind::ZeroUses => {
            if let Some(resolved) = ctx.zero_use_scope_resolved {
                let scope_ref = test.scope_ref.as_deref().unwrap_or("");
                if !resolved(scope_ref) {
                    return Err(DebtError::UnexecutableRemovalTest {
                        reason: UnexecutableReason::UnresolvedZeroUseScope,
                        detail: format!("zero-use scope {scope_ref} does not resolve"),
                    });
                }
            }
        }
        _ => {}
    }
    if let Some(false) = ctx.artifact_sealed {
        return Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::UnsealedArtifact,
            detail: "an artifact the removal test needs is unsealed".into(),
        });
    }
    // `InsufficientRunway` — `until − created_at < policy.min_runway` where a
    // `date`-bounded expiry names `until` and the record names `created_at`.
    if let (Some(policy), Some(expiry)) = (ctx.policy, &record.expiry) {
        if expiry.condition == ExpiryKind::Date {
            if let (Some(until), Some(created)) = (expiry.params.until, record.created_at) {
                let runway = until.saturating_sub(created);
                if runway < policy.min_runway_ms {
                    return Err(DebtError::InsufficientRunway {
                        runway_ms: runway,
                        min_ms: policy.min_runway_ms,
                    });
                }
            }
        }
    }
    // `OwnerUnreachable` — the owner is not reachable through any declared
    // sink (the check fires only where the caller declares the sink set and
    // the record declares `reach_via` members).
    if let Some(sinks) = ctx.declared_sinks {
        if !record.owner.reach_via.is_empty()
            && !record.owner.reach_via.iter().any(|s| sinks.contains(s))
        {
            return Err(DebtError::OwnerUnreachable {
                owner_id: record.owner.id.clone(),
            });
        }
    }
    Ok(())
}

/// Whether `beneficiary` is covered by the record's scope
/// (`model_snapshot:<id>`/family prefix, `task:<class>`, `role:<role>`).
fn beneficiary_covered(scope: &DebtScope, beneficiary: &str) -> bool {
    scope.covers_beneficiary(beneficiary)
}

/// `validate_for_home` — the `DebtHomes/1` membership, per-home completeness,
/// and removal-test instantiation check (`UnknownDebtHome` for an unlisted
/// `(record_kind, field)`; `MissingField` per `required_fields`; the
/// `validate_removal_test` battery).
pub fn validate_for_home(
    record: &AssumptionDebtRecord,
    record_kind: &str,
    field: &str,
    policy: &DebtPolicy,
    ctx: &RemovalTestContext<'_>,
) -> Result<(), DebtError> {
    let home = debt_home(record_kind, field).ok_or_else(|| DebtError::UnknownDebtHome {
        record_kind: record_kind.to_string(),
        field: field.to_string(),
    })?;
    for f in required_fields(home, policy) {
        if !record.has_field(&f) {
            return Err(DebtError::MissingField {
                home: home.record_kind,
                field: f,
            });
        }
    }
    validate_removal_test(record, home, ctx)
}
