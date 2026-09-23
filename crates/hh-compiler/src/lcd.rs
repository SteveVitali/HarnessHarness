//! `lcd_report` (§3.2.6) — the static composition over `validate_assembly`'s derived
//! results (CF-050: the report *composes* them, never recomputes them) plus the compile's
//! conditioned-rule inventory, the per-profile diff-fields set, and the lowering-loss
//! surface (empty at Stage 1 — `lower_target` lands at S3.2).

use std::collections::BTreeMap;

use hh_assembly::ValidationReport;

use crate::link::{ConditionedRule, LinkedGraph};
use crate::profile::profile_coordinate;

/// `LcdReport` — the bundle's static LCD section (§3.2.6).
#[derive(Debug, Clone, PartialEq)]
pub struct LcdReport {
    /// 7-L2 findings (`C-LCD-4` subjects) — composed from `validate_assembly`'s derived
    /// results.
    pub benchmark_conditioned_rules: Vec<String>,
    /// 7-T10 identity stability — composed, not recomputed.
    pub identity_stability: Option<bool>,
    /// 7-L4 hosting edges (`[]` at C0).
    pub hosting_edges: Vec<String>,
    /// 7-O opacity `{opaque, total}` — composed from the derived opacity summary.
    pub opacity: Option<(usize, usize)>,
    /// Every conditioned rule across the three homes (profile / definition / variant) —
    /// with the debt status each carries.
    pub conditioned_rules: Vec<ConditionedRule>,
    /// `per_profile_diff_fields` — the member set each chain profile changes vs its
    /// parent (profile-owned fields ∪ metered `ext` use).
    pub per_profile_diff_fields: BTreeMap<String, Vec<String>>,
    /// `lowering_loss` — per-target `LoweringLossReport`s; `[]` at Stage 1 (stage 4 lands
    /// at S3.2; the report is *present and empty*, never fabricated).
    pub lowering_loss: Vec<LoweringLossReport>,
}

/// `LoweringLossReport` — a typed per-target loss record (§3.2.8; ADR-0021's typed
/// losses: `degraded | synthesized | dropped-with-debt`). No variant is produced at
/// Stage 1 — the type is declared now so the report schema is stable (CC8).
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// The target this report is for.
    pub target: String,
    /// The losses.
    pub losses: Vec<LossEntry>,
}

/// One typed loss entry.
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// The surface/element the loss applies to.
    pub subject: String,
    /// `degraded | synthesized | dropped-with-debt`.
    pub kind: LossKind,
    /// The detail.
    pub detail: String,
    /// The debt record ref when `kind = dropped-with-debt`.
    pub debt_ref: Option<String>,
}

/// The closed loss kinds (ADR-0021).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossKind {
    /// The target expresses the element at reduced fidelity.
    Degraded,
    /// The element is synthesized (constructed by the lowering, not in the source).
    Synthesized,
    /// The element is dropped; the debt record names it.
    DroppedWithDebt,
}

impl LossKind {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            LossKind::Degraded => "degraded",
            LossKind::Synthesized => "synthesized",
            LossKind::DroppedWithDebt => "dropped-with-debt",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "degraded" => LossKind::Degraded,
            "synthesized" => LossKind::Synthesized,
            "dropped-with-debt" => LossKind::DroppedWithDebt,
            _ => return None,
        })
    }
}

/// `lcd_report(sealed, profiles, targets)` — the static composition (§3.2.7). Inputs:
/// the stage-0 `ValidationReport` (the derived results' only producer — CF-050) and the
/// `LinkedGraph`.
pub fn lcd_report(validation: &ValidationReport, linked: &LinkedGraph) -> LcdReport {
    let derived = &validation.derived;
    // per-profile diff fields — each chain member vs its parent.
    let mut per_profile_diff_fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let chain = &linked.profile.chain;
    for (i, p) in chain.iter().enumerate() {
        let diffs = if i == 0 {
            // The base's "diff" is its whole declared field set (owned fields ∪ metered
            // ext use).
            let mut v: Vec<String> = crate::profile::owned_fields(p).into_iter().collect();
            v.extend(p.ext.keys().map(|k| format!("ext/{k}")));
            v
        } else {
            crate::profile::member_diff(&chain[i - 1], p)
        };
        per_profile_diff_fields.insert(profile_coordinate(p), diffs);
    }
    LcdReport {
        benchmark_conditioned_rules: derived.benchmark_conditioned_rules.clone(),
        identity_stability: derived.identity_stability,
        hosting_edges: derived.hosting_edges.clone(),
        opacity: derived
            .opacity
            .as_ref()
            .map(|o| (o.opaque_ratio_num, o.total)),
        conditioned_rules: linked.conditioned_rules.clone(),
        per_profile_diff_fields,
        lowering_loss: Vec::new(),
    }
}
