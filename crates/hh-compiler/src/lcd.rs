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

/// `LoweringLossReport{target, target_version, entries[{hir_node_id, field, class,
/// severity}], granularity_ceiling}` (§3.2.5, ADR-0021 as amended). The closed class set
/// is `{no_slot, hint_only, untyped_slot, narrowed, truncated}`; severity is
/// `{info, narrowed, lost}`. Silent coercion is forbidden — a narrowing is declared
/// `narrowed` or raised `UnexpressibleSurface` per the profile's `strict` knob.
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// The target this report is for.
    pub target: String,
    /// The target-spec version (`TargetSpec.spec_version` — a derivation-key input).
    pub target_version: String,
    /// The typed loss entries (canonical order: `hir_node_id`, then `field`).
    pub entries: Vec<LossEntry>,
    /// The comparison-granularity ceiling the artefact admits.
    pub granularity_ceiling: GranularityCeiling,
}

/// One typed loss entry — `{hir_node_id, field, class, severity}` (§3.2.5).
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// The HIR node the loss applies to (the capability's semantic id).
    pub hir_node_id: String,
    /// The dropped/narrowed field.
    pub field: String,
    /// The loss class.
    pub class: LossKind,
    /// The severity.
    pub severity: LossSeverity,
    /// The detail.
    pub detail: String,
    /// The debt record ref when the drop is a conditioned rule.
    pub debt_ref: Option<String>,
}

/// The closed loss-class set (§3.2.5; ADR-0021 as amended).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossKind {
    /// The target has no slot for the element — it is dropped, declared.
    NoSlot,
    /// The element travels as a hint only (claims, never authority).
    HintOnly,
    /// The element travels in an untyped slot (a metadata string pair).
    UntypedSlot,
    /// The element is narrowed to the target's strict subset.
    Narrowed,
    /// The element is truncated; the retained set is listed.
    Truncated,
}

impl LossKind {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            LossKind::NoSlot => "no_slot",
            LossKind::HintOnly => "hint_only",
            LossKind::UntypedSlot => "untyped_slot",
            LossKind::Narrowed => "narrowed",
            LossKind::Truncated => "truncated",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "no_slot" => LossKind::NoSlot,
            "hint_only" => LossKind::HintOnly,
            "untyped_slot" => LossKind::UntypedSlot,
            "narrowed" => LossKind::Narrowed,
            "truncated" => LossKind::Truncated,
            _ => return None,
        })
    }
}

/// The closed severity set (§3.2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossSeverity {
    /// Carried for the record; nothing a consumer must do.
    Info,
    /// The element is expressed at reduced fidelity.
    Narrowed,
    /// The element is gone from the artefact.
    Lost,
}

impl LossSeverity {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            LossSeverity::Info => "info",
            LossSeverity::Narrowed => "narrowed",
            LossSeverity::Lost => "lost",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "info" => LossSeverity::Info,
            "narrowed" => LossSeverity::Narrowed,
            "lost" => LossSeverity::Lost,
            _ => return None,
        })
    }
}

/// The comparison-granularity ceiling (§3.2.5 `granularity_ceiling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GranularityCeiling {
    /// Per-component identity survives the lowering.
    Component,
    /// Per-configuration identity survives.
    Configuration,
    /// Only product-level identity survives.
    Product,
}

impl GranularityCeiling {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            GranularityCeiling::Component => "component",
            GranularityCeiling::Configuration => "configuration",
            GranularityCeiling::Product => "product",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "component" => GranularityCeiling::Component,
            "configuration" => GranularityCeiling::Configuration,
            "product" => GranularityCeiling::Product,
            _ => return None,
        })
    }
}

/// `lcd_report(sealed, profiles, targets)` — the static composition (§3.2.7). Inputs:
/// the stage-0 `ValidationReport` (the derived results' only producer — CF-050), the
/// `LinkedGraph`, and the stage-4 loss reports (composed, never recomputed).
pub fn lcd_report(
    validation: &ValidationReport,
    linked: &LinkedGraph,
    losses: &[LoweringLossReport],
) -> LcdReport {
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
        lowering_loss: losses.to_vec(),
    }
}
