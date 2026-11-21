//! `p4x` — the R7.2 exploration pre-registration + frozen selection
//! machinery (manifest 327; spec §8.4/§8.5/§8.7; design/37 parts 3 & 5.3):
//!
//! - **the declared X0/X1 design** (REQ-XD-206; AC-8.4-1): the X1 batch
//!   validates as a Lab `one_factor_at_a_time` `ExperimentSpec` (the base
//!   point plus single-level perturbations), built from the committed x2
//!   `sweep-registry/1` + `hh-p4-levels/1` records — never a second arm
//!   list. X0 and X1 never pool with each other or with confirmation.
//! - **the declared stop-rule set** (REQ-XD-210; AC-8.4-3): budget,
//!   infrastructure, integrity and catalog identity only — an
//!   outcome-based stop or a level drop refuses typed.
//! - **the exploration-packet labels** (REQ-XD-207; AC-8.4-2): every
//!   figure is labelled `exploratory`; a confirmatory label on an
//!   exploration figure refuses.
//! - **the frozen selection rule** (REQ-XD-211; AC-8.5-1): gates (a)–(f),
//!   score Δ̂ > 0, one level per factor, m_max = 2, tie-breaks within
//!   1/(|D_A|·k), the pre-declared `combine` flag and `no_candidate`.
//!   The pre-registration's `selection_rule` member is byte-equal to
//!   [`frozen_rule_json`] — the machinery decodes and executes that
//!   member, never a second copy.
//! - **the deterministic selection packet** (REQ-XD-212; AC-8.5-2):
//!   `hh-p4-selection/1` is a pure function of the committed X1 records
//!   + the frozen rule; its input manifest refuses held-out and
//!     phase-2/3 outcome records; its hash predates the confirmation
//!     pre-registration's declared seq.
//! - **the selection label** (REQ-XD-213; AC-8.5-3): a selected
//!   configuration's exploration estimate displays only with its
//!   selection label + simulated expected optimism; the confirmatory
//!   estimate is the claim.
//! - **the combination leg** (REQ-XD-214; AC-8.5-4): pre-declared only,
//!   confirmation only, union-path declared-delta check, labelled
//!   "combination; interaction not estimated".

use std::collections::{BTreeMap, BTreeSet};

use hharness_ontology::config::Ref;
use hharness_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hharness_ontology::lab::SplitLabel;
use hharness_ontology::participant::{Granularity, ParticipantClass};
use hharness_ontology::FactorKind;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SpecContext, SuiteBinding, ValidationStrategy,
};
use crate::sweep::{self, SweepRefusal};

fn as_obj(j: &Json) -> Option<&BTreeMap<String, Json>> {
    match j {
        Json::Obj(m) => Some(m),
        _ => None,
    }
}
fn as_arr(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

/// `hh-p4-x-prereg/1` — the frozen exploration pre-registration.
pub const X_PREREG_SCHEMA: &str = "hh-p4-x-prereg/1";

/// `hh-p4-selection/1` — the deterministic selection packet (R7.4 mints
/// the real one over committed X1 records; R7.2 lands the machinery).
pub const SELECTION_PACKET_SCHEMA: &str = "hh-p4-selection/1";

/// The recorded_at the pass's records carry (declared constant — the
/// emitters are deterministic, never clock-read).
pub const RECORDED_AT: &str = "2026-11-20";

// ── REQ-XD-206 — the declared X0/X1 design ───────────────────────────────────

/// The level → factor map over the frozen registry's Tier-1 set (§8.1;
/// the factor order is each level's registry order — V8/V9 share the
/// compaction axis). `(level_id, factor_name, base_label, conditional)`
/// — `conditional` marks the knob an X1 activation measurement bears
/// on at selection gate (c).
pub const X1_FACTOR_TABLE: &[(&str, &str, &str, bool)] = &[
    ("V1", "effort", "high", false),
    ("V2", "effort", "high", false),
    ("V3", "exec_timeout", "30000", true),
    ("V4", "obs_renderer", "10000/5000/5000", true),
    ("V5", "reasoning_replay", "keep", false),
    ("V6", "instance_template", "verbatim", false),
    ("V7", "tool_surface", "bash_only", true),
    ("V8", "compaction", "none@1048576", true),
    ("V9", "compaction", "none@1048576", true),
];

/// The factor vocabulary's declared axes in registry order (the first
/// level of each factor fixes the order).
pub const X1_FACTOR_ORDER: &[&str] = &[
    "effort",
    "exec_timeout",
    "obs_renderer",
    "reasoning_replay",
    "instance_template",
    "tool_surface",
    "compaction",
];

/// One admitted level's declaration for the X1 spec — `factor_name`,
/// registry `level_id`, its sealed `level_ref` and display `label`.
#[derive(Debug, Clone)]
pub struct XLevel {
    /// The factor axis.
    pub factor: &'static str,
    /// The registry level id (`V1`…).
    pub level_id: String,
    /// The sealed `level_ref` (`sha256:…` — the `LevelSpec.ref_` and the
    /// arm's pinned `artifact_ref`).
    pub level_ref: String,
    /// The registry level name (`eff-low`…).
    pub label: String,
    /// `true` for a conditional knob (gate (c) reads its measured
    /// activation).
    pub conditional: bool,
    /// The level's registry order (tie-break leg 3).
    pub registry_order: u64,
}

/// The design surface the committed x2 registry admits: per factor the
/// base level (`level_id "B"`, the declared sweep baseline's ref) plus
/// the admitted perturbed levels; withheld levels contribute their
/// factor's base level only (a fixed axis, honestly held at baseline).
#[derive(Debug, Clone)]
pub struct XDesign {
    /// The spec factors (`FactorSpec`) — varied axes carry ≥ 2 levels.
    pub factors: Vec<FactorSpec>,
    /// The design-level declarations (`Design.factors`) — the mirror
    /// vocabulary the `hh-experiment/1` document carries.
    pub design_factors: Vec<FactorDeclaration>,
    /// The baseline document's sealed ref (every base level's ref).
    pub baseline_ref: String,
    /// The admitted levels in registry order.
    pub admitted: Vec<XLevel>,
    /// The withheld levels in registry order (`level_id`, disposition).
    pub withheld: Vec<(String, String)>,
}

/// `x_design(reg, levels_rec)` — derive the X1 design surface from the
/// committed frozen `sweep-registry/1` (`reg`) and `hh-p4-levels/1`
/// (`levels_rec`) records. A level carrying `withheld` contributes its
/// factor's base axis only; an admitted level must carry `sealed_ref`
/// in the registry and a `level_ref` in the levels record (a missing
/// member is a typed refusal, never a fabricated ref).
pub fn x_design(reg: &Json, levels_rec: &Json) -> Result<XDesign, SweepRefusal> {
    let baseline_ref = levels_rec
        .get("baseline_ref")
        .and_then(Json::as_str)
        .ok_or_else(|| SweepRefusal::MissingMember {
            member: "levels.baseline_ref".to_string(),
        })?
        .to_string();
    let level_ref_of = |level_id: &str| -> Option<String> {
        levels_rec
            .get("levels")
            .and_then(as_arr)
            .and_then(|a| {
                a.iter()
                    .find(|l| l.get("level_id").and_then(Json::as_str) == Some(level_id))
            })
            .and_then(|l| l.get("level_ref"))
            .and_then(Json::as_str)
            .map(str::to_string)
    };
    let entries =
        reg.get("levels")
            .and_then(as_arr)
            .ok_or_else(|| SweepRefusal::MissingMember {
                member: "registry.levels".to_string(),
            })?;
    let mut admitted: Vec<XLevel> = Vec::new();
    let mut withheld: Vec<(String, String)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let m = as_obj(e).ok_or_else(|| SweepRefusal::MissingMember {
            member: format!("registry.levels[{i}]"),
        })?;
        let level_id = m
            .get("level_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let Some(&(_, factor, _base_label, conditional)) =
            X1_FACTOR_TABLE.iter().find(|(id, _, _, _)| *id == level_id)
        else {
            return Err(SweepRefusal::MissingMember {
                member: format!("factor axis for level `{level_id}`"),
            });
        };
        if let Some(w) = m.get("withheld").and_then(Json::as_str) {
            withheld.push((level_id, w.to_string()));
            continue;
        }
        let sealed_ref = m.get("sealed_ref").and_then(Json::as_str).ok_or_else(|| {
            SweepRefusal::MissingMember {
                member: format!("registry.levels[{i}].sealed_ref — an admitted level is sealed"),
            }
        })?;
        // The registry's sealed_ref is authoritative; the levels
        // record's level_ref must agree (one seal, never two).
        let level_ref = level_ref_of(&level_id).ok_or_else(|| SweepRefusal::MissingMember {
            member: format!("levels record carries no `{level_id}`"),
        })?;
        if level_ref != sealed_ref {
            return Err(SweepRefusal::DeclarationMismatch {
                detail: format!(
                    "{level_id}: levels.level_ref {level_ref} != registry.sealed_ref {sealed_ref}"
                ),
            });
        }
        let label = m
            .get("level_name")
            .and_then(Json::as_str)
            .unwrap_or(&level_id)
            .to_string();
        admitted.push(XLevel {
            factor,
            level_id,
            level_ref,
            label,
            conditional,
            registry_order: i as u64,
        });
    }
    // The factor list — each axis's base level plus its admitted
    // perturbations in registry order; a fully withheld axis stays
    // declared with its base level only (a fixed axis, honestly held).
    let mut factors = Vec::new();
    let mut design_factors = Vec::new();
    for &factor in X1_FACTOR_ORDER {
        let base_label = X1_FACTOR_TABLE
            .iter()
            .find(|(_, f, _, _)| *f == factor)
            .map(|(_, _, b, _)| *b)
            .unwrap_or("baseline");
        let mut levels = vec![LevelSpec {
            level_id: "B".to_string(),
            ref_: baseline_ref.clone(),
            overrides: None,
            label: format!("baseline ({base_label})"),
            class: ParticipantClass::Native,
            non_portable: false,
        }];
        let mut decl_levels = vec![FactorLevel {
            id: "B".to_string(),
            content_ref: baseline_ref.clone(),
            label: format!("baseline ({base_label})"),
        }];
        for lvl in admitted.iter().filter(|l| l.factor == factor) {
            levels.push(LevelSpec {
                level_id: lvl.level_id.clone(),
                ref_: lvl.level_ref.clone(),
                overrides: None,
                label: lvl.label.clone(),
                class: ParticipantClass::Native,
                non_portable: false,
            });
            decl_levels.push(FactorLevel {
                id: lvl.level_id.clone(),
                content_ref: lvl.level_ref.clone(),
                label: lvl.label.clone(),
            });
        }
        factors.push(FactorSpec {
            name: factor.to_string(),
            kind: FactorKind::Harness,
            granularity: Some(Granularity::ComponentLevel),
            role: None,
            levels,
        });
        design_factors.push(FactorDeclaration {
            name: factor.to_string(),
            kind: FactorKind::Harness,
            granularity: Some(Granularity::ComponentLevel),
            levels: decl_levels,
            role: None,
        });
    }
    Ok(XDesign {
        factors,
        design_factors,
        baseline_ref,
        admitted,
        withheld,
    })
}

/// The declared logical seq the design records carry (the transaction
/// `seq`, never a wall clock — the exploration cohort's registration
/// order: X0's smoke spec registers before X1's batch).
pub const X0_REGISTERED_AT: u64 = 1;
/// X1 registers after X0.
pub const X1_REGISTERED_AT: u64 = 2;

/// `x_arms(design)` — the OFAT arm set derived from the design surface:
/// the base arm plus one single-level perturbation arm per admitted
/// level, in factor-then-registry order. Every arm assigns every varied
/// factor (base levels elsewhere) — `arms_cover`'s expected point set.
pub fn x_arms(design: &XDesign) -> Vec<ArmSpec> {
    let varied: Vec<&FactorSpec> = design
        .factors
        .iter()
        .filter(|f| f.levels.len() >= 2)
        .collect();
    let base_assignment: BTreeMap<String, String> = varied
        .iter()
        .map(|f| (f.name.clone(), f.levels[0].level_id.clone()))
        .collect();
    let arm = |arm_id: &str, assignment: BTreeMap<String, String>, artifact: &str| {
        ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis:
            "exploratory OFAT screen — a single-level perturbation vs the frozen baseline (descriptive)"
                .to_string(),
        level_assignment: assignment,
        eval_budget: "budget:p4-x-slot".to_string(),
        search_budget: None,
        inference_budget: None,
        match_spec: None,
        artifact_ref: Ref::new(format!("p4-level-{arm_id}"), artifact),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    }
    };
    let mut arms = vec![arm(
        "baseline",
        base_assignment.clone(),
        &design.baseline_ref,
    )];
    for lvl in &design.admitted {
        let mut a = base_assignment.clone();
        a.insert(lvl.factor.to_string(), lvl.level_id.clone());
        arms.push(arm(&lvl.level_id, a, &lvl.level_ref));
    }
    arms
}

/// `x_spec(tag, design, tasks, k, seed, held_out_ref)` — build and
/// validate the declared exploration `ExperimentSpec` (kind
/// `exploratory`, `design.kind = one_factor_at_a_time`) through the
/// Lab's own `register` refusal set — never a re-implemented check.
/// The spec-level `pre_registration` stays absent: the phase-4
/// pre-registration is the committed `hh-p4-x-prereg/1` record itself
/// (exploratory kind; the design member carries the mandatory
/// design-level record).
#[allow(clippy::too_many_arguments)] // the declared design's honest member set
pub fn x_spec(
    tag: &str,
    design: &XDesign,
    suite_ref: &str,
    k: u32,
    seed: &str,
    held_out_ref: &str,
    task_split_hash: &str,
    registered_at: u64,
) -> Result<ExperimentSpec, SweepRefusal> {
    let seed_policy = SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: false,
    };
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Exploratory,
        design: Design {
            id: format!("{tag}-ofat"),
            kind: DesignKind::OneFactorAtATime,
            factors: design.design_factors.clone(),
            blocking: vec!["task".to_string()],
            replicates_per_cell: k,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy.clone(),
            held_out_split_ref: Some(held_out_ref.to_string()),
            pre_registration: PreRegistration {
                registered_at,
                hypothesis:
                    "OFAT exploration screens large config-knob effects (descriptive; mean half-widths ≈ ±.14 — large effects only)"
                        .to_string(),
                primary_metrics: vec!["resolved".to_string()],
                equivalence_margin: None,
                min_n: k,
                analysis_plan_ref: "docs/design/37_sweep-method.md#part-3".to_string(),
                task_split_hash: task_split_hash.to_string(),
                interactions: Vec::new(),
                dead_weight_purpose: false,
            },
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: design.factors.clone(),
        arms: x_arms(design),
        suite: SuiteBinding {
            suite_ref: suite_ref.to_string(),
            split_labels_used: vec![SplitLabel::Dev],
            split_assignment_ref: None,
        },
        replicates_per_cell: k,
        seed_policy,
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: Vec::new(),
            order: OrderKind::RandomPermuted,
            permutation_seed: seed.to_string(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 1_000,
                multiplier_ppm: 2_000_000,
                max_ms: 60_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: format!("budget:{tag}"),
            instrument: format!("budget:{tag}-instrument"),
        },
        bundle_policy: BundlePolicy::Named {
            name: tag.to_string(),
        },
        ext: BTreeMap::from([
            (
                "schedule".to_string(),
                Json::str("slot_schedule_randomization — seeded, 5 rounds"),
            ),
            (
                "exploration_label".to_string(),
                Json::str(EXPLORATION_LABEL),
            ),
        ]),
    };
    spec.experiment_id = spec.experiment_id();
    spec.register(&SpecContext::member_level()).map_err(|e| {
        SweepRefusal::UnexpressibleSurface {
            detail: format!("{tag}: the declared design fails `register` — {e:?}"),
        }
    })?;
    Ok(spec)
}

/// `check_pool_disjoint(cohorts)` — REQ-XD-206's pooling refusal: an
/// estimand drawing records from two distinct cohorts refuses
/// (X0 into X1, or exploration into confirmation).
pub fn check_pool_disjoint(cohorts: &[&str]) -> Result<(), SweepRefusal> {
    let set: BTreeSet<&str> = cohorts.iter().copied().collect();
    if set.len() > 1 {
        return Err(SweepRefusal::PoolAcrossCohorts {
            detail: format!(
                "a pooled estimand spans cohorts {} — X0 and X1 never pool; exploration and confirmation never pool",
                set.into_iter().collect::<Vec<_>>().join(" + ")
            ),
        });
    }
    Ok(())
}

// ── REQ-XD-210 — the declared stop-rule set ─────────────────────────────────

/// The §8.4 stop classes — budget, infrastructure, integrity, catalog
/// identity only.
pub const STOP_CLASSES: [&str; 4] = ["budget", "infrastructure", "integrity", "catalog_identity"];

/// The members that make a stop rule outcome-based or level-dropping —
/// exploration never stops on an observed outcome and never drops a
/// level.
pub const STOP_FORBIDDEN_MEMBERS: &[&str] = &[
    "on_outcome",
    "outcome",
    "score",
    "verdict",
    "delta",
    "delta_hat",
    "min_gain",
    "early_stop",
    "stop_on_gain",
    "futility",
    "drop_level",
    "level_drop",
    "drop",
    "prune",
    "prune_levels",
    "level_drop_threshold",
];

/// `check_stop_rule(rule)` — one declared stop rule: `class` must be a
/// member of the §8.4 set; an outcome member or a level-dropping member
/// refuses typed.
pub fn check_stop_rule(rule: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(m) = as_obj(rule) else {
        out.push(SweepRefusal::MissingMember {
            member: "stop_rule".to_string(),
        });
        return out;
    };
    match m.get("class").and_then(Json::as_str) {
        Some(c) if STOP_CLASSES.contains(&c) => {}
        Some(c) => out.push(SweepRefusal::StopClass {
            detail: format!(
                "`{c}` — the declared set is budget, infrastructure, integrity, catalog_identity only"
            ),
        }),
        None => out.push(SweepRefusal::MissingMember {
            member: "stop_rule.class".to_string(),
        }),
    }
    for k in m.keys() {
        let kl = k.to_ascii_lowercase();
        if kl.contains("drop") || kl.contains("prune") {
            out.push(SweepRefusal::LevelDrop { detail: k.clone() });
        } else if STOP_FORBIDDEN_MEMBERS.contains(&kl.as_str()) || crate::p4::is_outcome_field(&kl)
        {
            out.push(SweepRefusal::OutcomeStop { detail: k.clone() });
        }
    }
    out
}

/// `check_stop_set(rules)` — the declared set covers exactly the four
/// §8.4 classes (each at least once, never an undeclared class).
pub fn check_stop_set(rules: Option<&Json>) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    match rules.and_then(as_arr) {
        Some(rs) => {
            for r in rs.iter() {
                for e in check_stop_rule(r) {
                    out.push(e);
                }
                if let Some(c) = as_obj(r)
                    .and_then(|m| m.get("class"))
                    .and_then(Json::as_str)
                {
                    seen.insert(c.to_string());
                }
            }
            for c in STOP_CLASSES {
                if !seen.contains(c) {
                    out.push(SweepRefusal::MissingMember {
                        member: format!("stop rule class `{c}` is not declared"),
                    });
                }
            }
        }
        None => out.push(SweepRefusal::MissingMember {
            member: "stop_rules".to_string(),
        }),
    }
    out
}

/// The declared stop-rule set as the pre-registration carries it —
/// the four §8.4 classes, verbatim.
pub fn declared_stop_rules() -> Json {
    Json::Arr(
        [
            ("budget", "the proxy root cap / the declared window ceiling"),
            (
                "infrastructure",
                "repeated infrastructure failures at the declared bounds; reattempts within §6.8's bounds",
            ),
            (
                "integrity",
                "a confirmed answer-key or tamper veto stops everything at once",
            ),
            (
                "catalog_identity",
                "a catalog-identity change between exploration and confirmation is recorded; a mid-batch catalog change stops the batch",
            ),
        ]
        .iter()
        .map(|(class, detail)| {
            Json::obj([("class", Json::str(*class)), ("detail", Json::str(*detail))])
        })
        .collect(),
    )
}

// ── REQ-XD-207 — the exploration-packet label machinery ─────────────────────

/// The label every exploration figure carries.
pub const EXPLORATION_LABEL: &str = "exploratory";

/// A member that marks a figure (a labelled estimate surface).
pub const FIGURE_MEMBERS: &[&str] = &[
    "delta_hat",
    "delta_ppm",
    "estimate",
    "interval",
    "bootstrap",
    "fm_interval",
    "bh_q",
    "activation",
    "fidelity",
    "completion_envelope",
    "cost",
    "costs",
    "figure",
];

/// `is_confirmatory_label(s)` — a confirmatory spelling on an
/// exploration figure refuses (confirm/claim/confirmed family).
pub fn is_confirmatory_label(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.contains("confirm") || l.contains("claim") || l.contains("headline")
}

/// `is_exploratory_label(s)` — the required spelling family.
pub fn is_exploratory_label(s: &str) -> bool {
    s.to_ascii_lowercase().contains("exploratory")
}

/// `check_exploration_labels(packet)` — REQ-XD-207: the packet's own
/// label is `exploratory`; every object carrying a figure member
/// carries an exploratory label; a confirmatory spelling anywhere on a
/// figure refuses. Returns every refusal collected.
pub fn check_exploration_labels(packet: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    match packet.get("label").and_then(Json::as_str) {
        Some(l) if is_exploratory_label(l) => {}
        Some(l) if is_confirmatory_label(l) => out.push(SweepRefusal::ConfirmatoryLabel {
            detail: format!("packet.label `{l}` — no exploration figure is a claim"),
        }),
        Some(l) => out.push(SweepRefusal::MissingLabel {
            member: format!("packet.label `{l}` is not `exploratory`"),
        }),
        None => out.push(SweepRefusal::MissingLabel {
            member: "packet.label".to_string(),
        }),
    }
    fn walk(j: &Json, path: &str, is_root: bool, out: &mut Vec<SweepRefusal>) {
        if let Some(m) = as_obj(j) {
            // The root's own label is handled by the packet-level check
            // above; nested figures re-check theirs.
            if !is_root {
                let figure_like = FIGURE_MEMBERS.iter().any(|k| m.contains_key(*k));
                match m.get("label").and_then(Json::as_str) {
                    Some(l) if is_confirmatory_label(l) => {
                        out.push(SweepRefusal::ConfirmatoryLabel {
                            detail: format!("{path}.label `{l}`"),
                        })
                    }
                    Some(_) => {}
                    None if figure_like => out.push(SweepRefusal::MissingLabel {
                        member: format!("{path}.label"),
                    }),
                    None => {}
                }
            }
            for (k, v) in m {
                walk(v, &format!("{path}.{k}"), false, out);
            }
        }
        if let Json::Arr(a) = j {
            for (i, v) in a.iter().enumerate() {
                walk(v, &format!("{path}[{i}]"), false, out);
            }
        }
    }
    walk(packet, "packet", true, &mut out);
    out
}

// ── REQ-XD-211 — the frozen selection rule ──────────────────────────────────

/// The §8.5 frozen selection rule, verbatim.
pub const SELECTION_RULE_TEXT: &str = "1. Eligibility. A level is eligible iff all of these hold: (a) declared-delta and liveness passed; (b) fidelity: intended = served on 100 % of receipts; (c) measured X1 activation ≥ 0.25 for conditional knobs; (d) unresolved slots ≤ 10 % of planned; (e) no confirmed answer-key or tamper veto; (f) mean $/trial ≤ 2.0 × the baseline's. 2. Score. Δ̂ > 0 qualifies. 3. One level per factor. 4. Count. m_max = 2. 5. Ties. Within 1/(|D_A|·k), by lower $/trial, then registry order. 6. Combination. An optional pre-declared combination C₁₂, confirmation-only. 7. No candidate. withheld{no_candidate}, with the publishable statement \"no explored level had a positive exploration estimate under the frozen rule\". 8. Determinism. The selection packet is a pure function of the committed X1 records.";

/// The publishable `no_candidate` statement.
pub const NO_CANDIDATE_STATEMENT: &str =
    "no explored level had a positive exploration estimate under the frozen rule";

/// The declared `combine` flag — the pre-declared C₁₂ combination is
/// admitted by the frozen rule (confirmation-only, REQ-XD-214's leg).
pub const COMBINE_DECLARED: bool = true;

/// The selection gate floors (§8.5): measured X1 activation ≥ 0.25,
/// unresolved ≤ 10 % of planned, mean $/trial ≤ 2.0 × baseline's.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionRule {
    /// Gate (c) floor — conditional knobs' measured activation (ppm).
    pub activation_floor_ppm: u64,
    /// Gate (d) ceiling — unresolved slots / planned (ppm).
    pub unresolved_ceiling_ppm: u64,
    /// Gate (f) ceiling — level mean $/trial as a multiple of the
    /// baseline's (ppm of 1.0 — 2_000_000 = ×2.0).
    pub cost_multiple_ppm: u64,
    /// The family bound.
    pub m_max: u64,
    /// The pre-declared `combine` flag.
    pub combine: bool,
}

/// `frozen_selection_rule()` — the rule the machinery executes: the
/// §8.5 thresholds, m_max = 2, `combine` as the pre-registration
/// declares it.
pub fn frozen_selection_rule() -> SelectionRule {
    SelectionRule {
        activation_floor_ppm: 250_000,
        unresolved_ceiling_ppm: 100_000,
        cost_multiple_ppm: 2_000_000,
        m_max: 2,
        combine: COMBINE_DECLARED,
    }
}

/// `frozen_rule_json()` — the byte-pinned rule member: the §8.5 text
/// verbatim plus the machine parameters the executor decodes. The
/// pre-registration's `selection_rule` member is this object; a changed
/// byte is a new identity.
pub fn frozen_rule_json() -> Json {
    let r = frozen_selection_rule();
    Json::obj([
        ("id", Json::str("p4-selection-rule/1")),
        ("text", Json::str(SELECTION_RULE_TEXT)),
        (
            "gates",
            Json::obj([
                ("a", Json::str("declared-delta and liveness passed")),
                (
                    "b",
                    Json::str("fidelity: intended = served on 100 % of receipts"),
                ),
                (
                    "c",
                    Json::obj([
                        (
                            "text",
                            Json::str("measured X1 activation ≥ 0.25 for conditional knobs"),
                        ),
                        ("floor_ppm", Json::Int(r.activation_floor_ppm as i64)),
                    ]),
                ),
                (
                    "d",
                    Json::obj([
                        ("text", Json::str("unresolved slots ≤ 10 % of planned")),
                        ("ceiling_ppm", Json::Int(r.unresolved_ceiling_ppm as i64)),
                    ]),
                ),
                ("e", Json::str("no confirmed answer-key or tamper veto")),
                (
                    "f",
                    Json::obj([
                        ("text", Json::str("mean $/trial ≤ 2.0 × the baseline's")),
                        ("multiple_ppm", Json::Int(r.cost_multiple_ppm as i64)),
                    ]),
                ),
            ]),
        ),
        ("score", Json::str("Δ̂ > 0 qualifies")),
        ("one_level_per_factor", Json::Bool(true)),
        ("m_max", Json::Int(r.m_max as i64)),
        (
            "ties",
            Json::obj([
                (
                    "text",
                    Json::str("within 1/(|D_A|·k), by lower $/trial, then registry order"),
                ),
                ("epsilon", Json::str("1/(|D_A|·k)")),
            ]),
        ),
        (
            "combination",
            Json::obj([
                ("combine", Json::Bool(r.combine)),
                ("member", Json::str("C₁₂")),
                ("scope", Json::str("confirmation-only")),
                ("label", Json::str(COMBINATION_LABEL)),
            ]),
        ),
        (
            "no_candidate",
            Json::obj([
                ("disposition", Json::str("withheld{no_candidate}")),
                ("publishable", Json::str(NO_CANDIDATE_STATEMENT)),
            ]),
        ),
        (
            "determinism",
            Json::str("the selection packet is a pure function of the committed X1 records"),
        ),
        (
            "expected_optimism",
            Json::obj([
                ("exploration_estimate", Json::str(OPTIMISM_LABEL)),
                (
                    "confirmation_bias",
                    Json::str("|bias| ≤ .003 in simulation"),
                ),
            ]),
        ),
    ])
}

/// `rule_from_json(j)` — decode the machine parameters out of a rule
/// member. The executor takes the rule the *record* carries, decoded
/// through this one path.
pub fn rule_from_json(j: &Json) -> Result<SelectionRule, SweepRefusal> {
    let missing = |member: &str| SweepRefusal::MissingMember {
        member: format!("selection_rule.{member}"),
    };
    let m = as_obj(j).ok_or_else(|| missing(""))?;
    let gate = |g: &str, k: &str| -> Result<u64, SweepRefusal> {
        m.get("gates")
            .and_then(|ggs| ggs.get(g))
            .and_then(|gg| gg.get(k))
            .and_then(Json::as_int)
            .map(|v| v as u64)
            .ok_or_else(|| missing(&format!("gates.{g}.{k}")))
    };
    Ok(SelectionRule {
        activation_floor_ppm: gate("c", "floor_ppm")?,
        unresolved_ceiling_ppm: gate("d", "ceiling_ppm")?,
        cost_multiple_ppm: gate("f", "multiple_ppm")?,
        m_max: m
            .get("m_max")
            .and_then(Json::as_int)
            .map(|v| v as u64)
            .ok_or_else(|| missing("m_max"))?,
        combine: match m.get("combination").and_then(|c| c.get("combine")) {
            Some(Json::Bool(b)) => *b,
            _ => return Err(missing("combination.combine")),
        },
    })
}

/// `check_frozen_rule(member)` — REQ-XD-211's byte-equal leg: the
/// pre-registration's `selection_rule` member is the frozen rule the
/// machinery executes — canonical-byte equal, never a paraphrase.
pub fn check_frozen_rule(member: &Json) -> Result<(), SweepRefusal> {
    if member.to_canonical_string() == frozen_rule_json().to_canonical_string() {
        Ok(())
    } else {
        Err(SweepRefusal::FrozenRuleMismatch {
            detail: "the record's selection_rule is not byte-equal to the §8.5 frozen rule"
                .to_string(),
        })
    }
}

/// `frozen_rule_sha256()` — the rule's content hash.
pub fn frozen_rule_sha256() -> String {
    sha256_hex(frozen_rule_json().to_canonical_string().as_bytes())
}

/// One level's X1 evidence the frozen rule consumes — every gate's
/// measured surface plus the equal-task Δ̂ (ppm, joint population).
#[derive(Debug, Clone)]
pub struct LevelInput {
    /// The registry level id.
    pub level_id: String,
    /// The factor axis.
    pub factor: String,
    /// The level's registry order (tie leg 3).
    pub registry_order: u64,
    /// Gate (a): the declared-delta check passed.
    pub declared_delta_passed: bool,
    /// Gate (a): the liveness check passed.
    pub liveness_passed: bool,
    /// Gate (b): intended = served on 100 % of receipts.
    pub fidelity_ok: bool,
    /// The knob is conditional (gate (c) applies).
    pub conditional: bool,
    /// Gate (c): the measured X1 activation (ppm); `None` = unmeasured
    /// (`insufficient_evidence` — a conditional knob without a
    /// measurement is not a pass, never a fabricated floor verdict).
    pub activation_ppm: Option<u64>,
    /// Gate (d): unresolved slots.
    pub unresolved_slots: u64,
    /// Gate (d): planned slots.
    pub planned_slots: u64,
    /// Gate (e): a confirmed answer-key or tamper veto on the level.
    pub veto: bool,
    /// Gate (f): the level's mean $/trial (micro-USD).
    pub cost_micros: u64,
    /// The equal-task Δ̂ vs the baseline (ppm, joint population).
    pub delta_ppm: i64,
}

/// The gate letters in order.
pub const GATE_LETTERS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

/// `gate_table_row(input, baseline_cost, rule)` — the level's (a)–(f)
/// gate evaluation as a record row: each gate's verdict plus the note
/// that bears it (`insufficient_evidence` is a recorded verdict, never
/// a pass).
pub fn gate_table_row(input: &LevelInput, baseline_cost: u64, rule: &SelectionRule) -> Json {
    let a = input.declared_delta_passed && input.liveness_passed;
    let c_note;
    let c = if !input.conditional {
        c_note = "always-active knob — gate applies to conditional knobs only";
        true
    } else {
        match input.activation_ppm {
            Some(p) if p >= rule.activation_floor_ppm => {
                c_note = "measured activation meets the floor";
                true
            }
            Some(_) => {
                c_note = "measured activation below the 0.25 floor";
                false
            }
            None => {
                c_note = "unmeasured{insufficient_evidence} — no X1 activation measurement; never a pass";
                false
            }
        }
    };
    let d = input.unresolved_slots * 1_000_000 <= input.planned_slots * rule.unresolved_ceiling_ppm;
    let f = input.cost_micros * 1_000_000 <= baseline_cost * rule.cost_multiple_ppm;
    let gates = [
        ("a", a),
        ("b", input.fidelity_ok),
        ("c", c),
        ("d", d),
        ("e", !input.veto),
        ("f", f),
    ];
    let eligible = gates.iter().all(|(_, ok)| *ok);
    Json::obj([
        ("level_id", Json::str(&input.level_id)),
        ("factor", Json::str(&input.factor)),
        ("registry_order", Json::Int(input.registry_order as i64)),
        (
            "gates",
            Json::obj([
                (
                    "a",
                    Json::obj([
                        ("pass", Json::Bool(a)),
                        (
                            "detail",
                            Json::str(if a {
                                "declared-delta and liveness passed"
                            } else {
                                "declared-delta or liveness did not pass"
                            }),
                        ),
                    ]),
                ),
                (
                    "b",
                    Json::obj([
                        ("pass", Json::Bool(input.fidelity_ok)),
                        (
                            "detail",
                            Json::str(if input.fidelity_ok {
                                "intended = served on 100 % of receipts"
                            } else {
                                "a receipt's served value differs from the declaration"
                            }),
                        ),
                    ]),
                ),
                (
                    "c",
                    Json::obj([("pass", Json::Bool(c)), ("detail", Json::str(c_note))]),
                ),
                (
                    "d",
                    Json::obj([
                        ("pass", Json::Bool(d)),
                        (
                            "detail",
                            Json::str(if d {
                                "unresolved slots within 10 % of planned"
                            } else {
                                "unresolved slots exceed 10 % of planned"
                            }),
                        ),
                    ]),
                ),
                (
                    "e",
                    Json::obj([
                        ("pass", Json::Bool(!input.veto)),
                        (
                            "detail",
                            Json::str(if input.veto {
                                "a confirmed answer-key or tamper veto stands"
                            } else {
                                "no confirmed answer-key or tamper veto"
                            }),
                        ),
                    ]),
                ),
                (
                    "f",
                    Json::obj([
                        ("pass", Json::Bool(f)),
                        (
                            "detail",
                            Json::str(if f {
                                "mean $/trial within 2.0 × the baseline's"
                            } else {
                                "mean $/trial exceeds 2.0 × the baseline's"
                            }),
                        ),
                    ]),
                ),
            ]),
        ),
        ("eligible", Json::Bool(eligible)),
        ("delta_ppm", Json::Int(input.delta_ppm)),
        ("qualified", Json::Bool(eligible && input.delta_ppm > 0)),
    ])
}

/// `select_cmp(a, b, tie_trials)` — the §8.5 ordering: higher Δ̂ first;
/// within 1/(|D_A|·k) (exact — `|a−b|·(|D_A|·k) ≤ 10⁶` over ppm), by
/// lower $/trial, then registry order. `tie_trials` = |D_A|·k.
pub fn select_cmp(a: &LevelInput, b: &LevelInput, tie_trials: u64) -> std::cmp::Ordering {
    let d = (a.delta_ppm - b.delta_ppm).unsigned_abs() as u128;
    if d * (tie_trials as u128) <= 1_000_000u128 {
        // Within the resolution — lower $/trial, then registry order.
        a.cost_micros
            .cmp(&b.cost_micros)
            .then(a.registry_order.cmp(&b.registry_order))
    } else {
        b.delta_ppm.cmp(&a.delta_ppm)
    }
}

/// `select(rule, inputs, baseline_cost, tie_trials)` — the §8.5 rule
/// applied mechanically: eligible = every gate; qualified = Δ̂ > 0;
/// one level per factor (the higher one under `select_cmp`); the top
/// `m_max`; `combine` adds C₁₂ when two selected levels sit on
/// different factors; an empty qualified set is `withheld{no_candidate}`
/// with the publishable statement.
pub fn select(
    rule: &SelectionRule,
    inputs: &[LevelInput],
    baseline_cost: u64,
    tie_trials: u64,
) -> Json {
    let rows: Vec<Json> = inputs
        .iter()
        .map(|i| gate_table_row(i, baseline_cost, rule))
        .collect();
    let mut qualified: Vec<&LevelInput> = inputs
        .iter()
        .zip(rows.iter())
        .filter(|(_, r)| r.get("qualified") == Some(&Json::Bool(true)))
        .map(|(i, _)| i)
        .collect();
    // One level per factor — the higher under the §8.5 ordering.
    let mut best: BTreeMap<&str, &LevelInput> = BTreeMap::new();
    for i in qualified.drain(..) {
        best.entry(i.factor.as_str())
            .and_modify(|cur| {
                if select_cmp(i, cur, tie_trials) == std::cmp::Ordering::Less {
                    *cur = i;
                }
            })
            .or_insert(i);
    }
    let mut ranked: Vec<&LevelInput> = best.into_values().collect();
    ranked.sort_by(|a, b| select_cmp(a, b, tie_trials));
    let selected: Vec<&LevelInput> = ranked.into_iter().take(rule.m_max as usize).collect();
    let gate_table = Json::Arr(rows);
    if selected.is_empty() {
        return Json::obj([
            ("disposition", Json::str("withheld{no_candidate}")),
            ("gate_table", gate_table),
            ("publishable", Json::str(NO_CANDIDATE_STATEMENT)),
        ]);
    }
    let members: Vec<Json> = selected
        .iter()
        .enumerate()
        .map(|(r, i)| {
            Json::obj([
                ("member", Json::str(&i.level_id)),
                ("factor", Json::str(&i.factor)),
                ("rank", Json::Int((r + 1) as i64)),
                ("delta_ppm", Json::Int(i.delta_ppm)),
                (
                    "exploration_label",
                    Json::str("selected from the same data; estimate is exploratory"),
                ),
            ])
        })
        .collect();
    let factors: BTreeSet<&str> = selected.iter().map(|i| i.factor.as_str()).collect();
    let mut outcome = vec![
        (
            "disposition",
            Json::str(format!("selected{{m={}}}", selected.len())),
        ),
        ("members", Json::Arr(members)),
        ("gate_table", gate_table),
    ];
    if rule.combine && selected.len() == 2 && factors.len() == 2 {
        outcome.push((
            "combination",
            Json::obj([
                ("member", Json::str("C₁₂")),
                (
                    "levels",
                    Json::Arr(
                        selected
                            .iter()
                            .map(|i| Json::str(&i.level_id))
                            .collect(),
                    ),
                ),
                (
                    "label",
                    Json::str(COMBINATION_LABEL),
                ),
                (
                    "exploration_estimate",
                    Json::str("none — no exploration estimate; the additivity assumption is untested by OFAT"),
                ),
                ("scope", Json::str("confirmation-only")),
            ]),
        ));
    }
    Json::Obj(BTreeMap::from_iter(
        outcome.into_iter().map(|(k, v)| (k.to_string(), v)),
    ))
}

// ── REQ-XD-212 — the deterministic selection packet ─────────────────────────

/// `check_selection_inputs(manifest)` — the input manifest lists only
/// committed X1 records: each entry carries `relpath`, `sha256`,
/// `cohort`; a held-out entry refuses `held_out_input`, a phase-2/3 or
/// other non-X1 outcome entry refuses `foreign_outcome_input` (X-07 —
/// enforced, never promised).
pub fn check_selection_inputs(manifest: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(entries) = as_arr(manifest) else {
        out.push(SweepRefusal::MissingMember {
            member: "input_manifest".to_string(),
        });
        return out;
    };
    for (i, e) in entries.iter().enumerate() {
        let Some(m) = as_obj(e) else {
            out.push(SweepRefusal::MissingMember {
                member: format!("input_manifest[{i}]"),
            });
            continue;
        };
        for need in ["relpath", "sha256", "cohort"] {
            if m.get(need).and_then(Json::as_str).is_none() {
                out.push(SweepRefusal::MissingMember {
                    member: format!("input_manifest[{i}].{need}"),
                });
            }
        }
        match m.get("cohort").and_then(Json::as_str) {
            Some("x1") => {}
            Some(c) => {
                let cl = c.to_ascii_lowercase().replace(['-', ' '], "_");
                if cl.contains("held_out") || cl.contains("heldout") {
                    out.push(SweepRefusal::HeldOutInput {
                        detail: format!("input_manifest[{i}] cohort `{c}` is held-out"),
                    });
                } else {
                    out.push(SweepRefusal::ForeignOutcomeInput {
                        detail: format!(
                            "input_manifest[{i}] cohort `{c}` — the selection reads committed X1 records only"
                        ),
                    });
                }
            }
            None => {}
        }
    }
    out
}

/// `selection_packet(inputs, inputs_manifest, rule_sha, outcome, seq)` —
/// mint `hh-p4-selection/1` as a pure function of the committed X1
/// records (the manifest of hashes) and the frozen rule: byte-identical
/// recompute (REQ-XD-212). `seq` is the record's declared logical seq —
/// the confirmation pre-registration's seq must be strictly later.
pub fn selection_packet(
    input_manifest: Json,
    rule_sha256: &str,
    outcome: Json,
    recorded_seq: u64,
) -> Json {
    Json::obj([
        ("schema", Json::str(SELECTION_PACKET_SCHEMA)),
        ("recorded_seq", Json::Int(recorded_seq as i64)),
        ("input_manifest", input_manifest),
        ("selection_rule_sha256", Json::str(rule_sha256)),
        ("outcome", outcome),
        (
            "predates",
            Json::str("the confirmation pre-registration — this packet's hash is committed first (REQ-XD-212)"),
        ),
        (
            "determinism",
            Json::str("pure function of the committed X1 records and the frozen rule — byte-identical recompute"),
        ),
    ])
}

/// `check_predates_confirmation(selection, confirmation)` — the packet
/// hash predates the confirmation pre-registration: the confirmation
/// cites this packet's hash, and its declared `recorded_seq` is
/// strictly later (REQ-XD-212; §8.7's order).
pub fn check_predates_confirmation(
    selection: &Json,
    confirmation: &Json,
) -> Result<(), SweepRefusal> {
    let sel_seq = selection
        .get("recorded_seq")
        .and_then(Json::as_int)
        .ok_or_else(|| SweepRefusal::MissingMember {
            member: "selection.recorded_seq".to_string(),
        })?;
    let conf_seq = confirmation
        .get("recorded_seq")
        .and_then(Json::as_int)
        .ok_or_else(|| SweepRefusal::MissingMember {
            member: "confirmation.recorded_seq".to_string(),
        })?;
    if sel_seq >= conf_seq {
        return Err(SweepRefusal::HashOrdering {
            detail: format!(
                "selection seq {sel_seq} is not before confirmation seq {conf_seq} — the packet hash must predate the confirmation pre-registration"
            ),
        });
    }
    let packet_sha = selection
        .get("packet_sha256")
        .and_then(Json::as_str)
        .unwrap_or("");
    match confirmation.get("selection_packet_sha256").and_then(Json::as_str) {
        Some(cited) if cited == packet_sha => Ok(()),
        Some(cited) => Err(SweepRefusal::HashOrdering {
            detail: format!(
                "confirmation cites selection_packet_sha256 {cited} — the packet's committed hash is {packet_sha}"
            ),
        }),
        None => Err(SweepRefusal::MissingMember {
            member: "confirmation.selection_packet_sha256".to_string(),
        }),
    }
}

// ── REQ-XD-213 — the selection label + simulated optimism ───────────────────

/// The selection label a selected configuration's exploration estimate
/// carries.
pub const SELECTION_LABEL: &str = "selected from the same data";

/// The simulated expected-optimism display (design/37: ≈ +.03 to +.10).
pub const OPTIMISM_LABEL: &str = "expected optimism ≈ +.03 to +.10 (design simulation)";

/// The full display label, verbatim.
pub const SELECTION_DISPLAY_LABEL: &str =
    "selected from the same data; expected optimism ≈ +.03 to +.10 (design simulation)";

/// `check_selection_display(display)` — REQ-XD-213: a selected
/// configuration's exploration estimate displays only with its
/// selection label + the simulated expected optimism; the claim cites
/// the confirmatory estimate. `display` shape:
/// `{level_id, exploration:{delta_ppm, selection_label,
/// expected_optimism}, claim:{estimate_source}}`.
pub fn check_selection_display(display: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(m) = as_obj(display) else {
        out.push(SweepRefusal::MissingMember {
            member: "display".to_string(),
        });
        return out;
    };
    match m.get("exploration") {
        Some(ex) => {
            let em = as_obj(ex);
            let label = em
                .and_then(|e| e.get("selection_label"))
                .and_then(Json::as_str);
            match label {
                Some(l) if l.contains(SELECTION_LABEL) => {}
                Some(_) | None => out.push(SweepRefusal::SelectionLabelAbsent {
                    detail: "exploration.selection_label absent or not the declared label"
                        .to_string(),
                }),
            }
            let opt = em
                .and_then(|e| e.get("expected_optimism"))
                .and_then(Json::as_str);
            match opt {
                Some(o) if o.contains("+.03") && o.contains("+.10") => {}
                Some(_) | None => out.push(SweepRefusal::SelectionLabelAbsent {
                    detail: "exploration.expected_optimism absent — the simulated ≈ +.03 to +.10 range is required".to_string(),
                }),
            }
        }
        None => out.push(SweepRefusal::MissingMember {
            member: "display.exploration".to_string(),
        }),
    }
    match m
        .get("claim")
        .and_then(|c| c.get("estimate_source"))
        .and_then(Json::as_str)
    {
        Some("confirmatory") => {}
        Some(s) => out.push(SweepRefusal::ClaimCitesExploration {
            detail: format!("claim.estimate_source `{s}` — the confirmatory estimate is the claim"),
        }),
        None => out.push(SweepRefusal::MissingMember {
            member: "display.claim.estimate_source".to_string(),
        }),
    }
    out
}

// ── REQ-XD-214 — the combination leg ────────────────────────────────────────

/// The mandatory combination label.
pub const COMBINATION_LABEL: &str = "combination; interaction not estimated";

/// `check_combination(decl)` — a combined configuration is admissible
/// only when pre-declared (`predeclared: true` — the frozen rule's
/// `combine` flag declared it), only in confirmation (`phase` must be
/// `"confirmation"`), with a union-path declared-delta check
/// (`declared_delta` = the union of the two member levels' declared
/// typed paths — `member_paths` is the two levels' own declared sets),
/// labelled "combination; interaction not estimated". Returns the
/// admission record on pass, else the refusals.
pub fn check_combination(decl: &Json) -> Result<Json, Vec<SweepRefusal>> {
    let mut out = Vec::new();
    let Some(m) = as_obj(decl) else {
        return Err(vec![SweepRefusal::MissingMember {
            member: "combination".to_string(),
        }]);
    };
    if m.get("predeclared") != Some(&Json::Bool(true)) {
        out.push(SweepRefusal::UndeclaredCombination {
            detail: "the combination was not pre-declared in the frozen rule's `combine` flag"
                .to_string(),
        });
    }
    match m.get("phase").and_then(Json::as_str) {
        Some("confirmation") => {}
        Some(p) => out.push(SweepRefusal::CombinationContext {
            detail: format!("phase `{p}` — a combined configuration is confirmation-only"),
        }),
        None => out.push(SweepRefusal::MissingMember {
            member: "combination.phase".to_string(),
        }),
    }
    // The union-path declared-delta check: the combination's declared
    // delta is exactly the union of its member levels' declared typed
    // paths — no third surface.
    let declared: BTreeSet<String> = m
        .get("declared_delta")
        .and_then(as_arr)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let union: BTreeSet<String> = m
        .get("member_paths")
        .and_then(as_arr)
        .map(|a| {
            a.iter()
                .filter_map(as_arr)
                .flatten()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if declared.is_empty() || declared != union {
        out.push(SweepRefusal::UndeclaredCombination {
            detail: format!(
                "declared_delta {declared:?} != the member levels' declared-path union {union:?} — the union-path check fails"
            ),
        });
    }
    match m.get("label").and_then(Json::as_str) {
        Some(COMBINATION_LABEL) => {}
        Some(l) => out.push(SweepRefusal::MissingLabel {
            member: format!("combination.label `{l}` != \"{COMBINATION_LABEL}\""),
        }),
        None => out.push(SweepRefusal::MissingLabel {
            member: "combination.label".to_string(),
        }),
    }
    if out.is_empty() {
        Ok(Json::obj([
            ("admitted", Json::Bool(true)),
            ("label", Json::str(COMBINATION_LABEL)),
            (
                "note",
                Json::str("pre-declared, confirmation-only; the union-path declared-delta check passed — the interaction is not estimated"),
            ),
        ]))
    } else {
        Err(out)
    }
}

// ── the `hh-p4-x-prereg/1` record ───────────────────────────────────────────

/// `band_a_dev_tasks(selection)` — the X1 manifest: the band-A `dev`
/// members of the committed `task-selection/1` record in declared
/// cell order (the split's own within-cell order), plus the X0 set —
/// the first declared dev task of each band-A difficulty cell (the
/// stratum-cover smoke, deterministic).
pub fn band_a_dev_tasks(selection: &Json) -> Result<(Vec<String>, Vec<String>), SweepRefusal> {
    let cells =
        selection
            .get("cells")
            .and_then(as_arr)
            .ok_or_else(|| SweepRefusal::MissingMember {
                member: "task-selection.cells".to_string(),
            })?;
    let mut x1 = Vec::new();
    let mut x0 = Vec::new();
    for c in cells {
        let cm = as_obj(c).ok_or_else(|| SweepRefusal::MissingMember {
            member: "task-selection cell".to_string(),
        })?;
        if cm.get("band").and_then(Json::as_str) != Some("A") {
            continue;
        }
        let dev = cm
            .get("dev")
            .and_then(as_arr)
            .ok_or_else(|| SweepRefusal::MissingMember {
                member: "task-selection cell.dev".to_string(),
            })?;
        let ids: Vec<String> = dev
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect();
        if let Some(first) = ids.first() {
            x0.push(first.clone());
        }
        x1.extend(ids);
    }
    if x1.is_empty() || x0.is_empty() {
        return Err(SweepRefusal::MissingMember {
            member: "band-A dev cells in the task-selection record".to_string(),
        });
    }
    Ok((x1, x0))
}

/// `x_prereg_record(args)` — build the frozen `hh-p4-x-prereg/1` from
/// the committed inputs: the x2 `sweep-registry/1` + `hh-p4-levels/1`
/// (registry hash, withheld set, sealed refs), `hh-p4-w-rule/1` (W
/// verbatim), `hh-p4-admissibility/1` (floor + forecast dispositions),
/// `task-selection/1` (the band-A dev manifest). The declared envelope
/// is what GATE-G35 presents — slots, arms, levels, ceiling estimate
/// and task source all recorded honestly.
#[allow(clippy::too_many_arguments)] // the committed inputs it consumes
pub fn x_prereg_record(
    reg: &Json,
    levels_rec: &Json,
    w_rule_rec: &Json,
    admissibility: &Json,
    task_selection: &Json,
    registry_relpath: &str,
    w_rule_relpath: &str,
    task_relpath: &str,
) -> Result<Json, SweepRefusal> {
    let registry_sha = reg
        .get("frozen_sha256")
        .and_then(Json::as_str)
        .ok_or_else(|| SweepRefusal::MissingMember {
            member: "registry.frozen_sha256".to_string(),
        })?
        .to_string();
    let identity = reg
        .get("exploration_identity")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let w_outcome = w_rule_rec
        .get("derivation")
        .and_then(|d| d.get("outcome"))
        .and_then(Json::as_str)
        .ok_or_else(|| SweepRefusal::MissingMember {
            member: "w_rule.derivation.outcome".to_string(),
        })?
        .to_string();
    let design = x_design(reg, levels_rec)?;
    let (x1_tasks, x0_tasks) = band_a_dev_tasks(task_selection)?;
    let split_hash = sha256_hex(
        Json::Arr(x1_tasks.iter().map(Json::str).collect())
            .to_canonical_string()
            .as_bytes(),
    );
    let held_out_ref = format!("{task_relpath}#band-a-held-out");
    let x0_spec = x_spec(
        "p4-x0",
        &design,
        &format!("{task_relpath}#band-a-dev-x0"),
        1,
        "p4-x0-schedule-r7.2-2026-11-20",
        &held_out_ref,
        &format!("sha256:{split_hash}"),
        X0_REGISTERED_AT,
    )?;
    let x1_spec = x_spec(
        "p4-x1",
        &design,
        &format!("{task_relpath}#band-a-dev"),
        5,
        "p4-x1-schedule-r7.2-2026-11-20",
        &held_out_ref,
        &format!("sha256:{split_hash}"),
        X1_REGISTERED_AT,
    )?;
    let n_arms = design.admitted.len() as i64 + 1;
    let x0_slots = n_arms * x0_tasks.len() as i64;
    let x1_slots = n_arms * x1_tasks.len() as i64 * 5;
    let withheld_json: Vec<Json> = design
        .withheld
        .iter()
        .map(|(id, w)| {
            Json::obj([
                ("level_id", Json::str(id)),
                ("disposition", Json::str(w)),
                ("note", Json::str("declared, never sealed — the level stays in the registry with its forecast")),
            ])
        })
        .collect();
    let arm_ids: Vec<Json> = std::iter::once(Json::str("baseline"))
        .chain(design.admitted.iter().map(|l| Json::str(&l.level_id)))
        .collect();
    let spec_json = |s: &ExperimentSpec, arms: i64, slots: i64, tasks: usize, k: i64| {
        let sj = s.to_json();
        Json::obj([
            ("experiment_id", Json::str(&s.experiment_id)),
            ("experiment", sj),
            ("tasks", Json::Int(tasks as i64)),
            ("replicates", Json::Int(k)),
            ("arms", Json::Int(arms)),
            ("slots", Json::Int(slots)),
        ])
    };
    let effort_pin = admissibility
        .get("effort_pin")
        .and_then(|e| e.get("declared_baseline"))
        .and_then(Json::as_str)
        .unwrap_or("high")
        .to_string();
    let rec = Json::obj([
        ("schema", Json::str(X_PREREG_SCHEMA)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("327_R7.2")),
        (
            "predates",
            Json::str("every X0/X1 subject and GATE-G35 — this hash is minted offline before any window (hash-before-subjects)"),
        ),
        (
            "spend",
            Json::str("USD $0.00 — registration authorizes no spend (REQ-XD-221: phase-4 spend is authorized only at G-P4-X and G-P4-C)"),
        ),
        (
            "baseline",
            Json::obj([
                ("document", Json::str("declared sweep baseline (fixture, pass 2)")),
                ("baseline_ref", Json::str(&design.baseline_ref)),
                (
                    "label",
                    Json::str("the phase-4 baseline — H frozen by G-L1 for the exact version swept; the exact G-L1 pass id is cited at GATE-G35 (REQ-XD-220)"),
                ),
                ("effort_pin", Json::str(&effort_pin)),
                (
                    "note",
                    Json::str("the production mini HIR document is E2-T9's unlanded Stage 2 — the registry's declared baseline is what the sealed levels derive from"),
                ),
            ]),
        ),
        (
            "registry",
            Json::obj([
                ("path", Json::str(registry_relpath)),
                ("sha256", Json::str(&registry_sha)),
                ("exploration_identity", Json::str(&identity)),
            ]),
        ),
        (
            "w",
            Json::obj([
                ("record", Json::str(w_rule_relpath)),
                ("outcome", Json::str(&w_outcome)),
                (
                    "verbatim",
                    Json::str("F(32768) < 0.25 on the committed corpus — both composite levels withheld{inert_at_admissible_window}; no admissible W is fabricated"),
                ),
            ]),
        ),
        ("withheld", Json::Arr(withheld_json)),
        (
            "tasks",
            Json::obj([
                (
                    "source",
                    Json::str(format!("{task_relpath} — band-A dev cells of the frozen task-selection/1 record")),
                ),
                (
                    "x0_rule",
                    Json::str("the first declared dev task of each band-A difficulty cell (stratum cover; deterministic)"),
                ),
                ("x0", Json::Arr(x0_tasks.iter().map(Json::str).collect())),
                ("x1", Json::Arr(x1_tasks.iter().map(Json::str).collect())),
            ]),
        ),
        (
            "design",
            Json::obj([
                ("x0", {
                    let mut s = spec_json(&x0_spec, n_arms, x0_slots, x0_tasks.len(), 1);
                    if let Json::Obj(ref mut m) = s {
                        m.insert("kind".into(), Json::str("smoke — all executable arms, one replicate"));
                        m.insert("pooling".into(), Json::str("never pooled with X1 or confirmation"));
                    }
                    s
                }),
                ("x1", {
                    let mut s = spec_json(&x1_spec, n_arms, x1_slots, x1_tasks.len(), 5);
                    if let Json::Obj(ref mut m) = s {
                        m.insert("kind".into(), Json::str("one_factor_at_a_time"));
                        m.insert("band".into(), Json::str("band-A development tasks"));
                        m.insert("rounds".into(), Json::Int(5));
                        m.insert("schedule".into(), Json::str("slot_schedule_randomization — seeded"));
                        m.insert("seed".into(), Json::str("p4-x1-schedule-r7.2-2026-11-20"));
                        m.insert("pooling".into(), Json::str("never pooled with X0 or confirmation"));
                        m.insert("arm_ids".into(), Json::Arr(arm_ids));
                    }
                    s
                }),
            ]),
        ),
        (
            "envelope",
            Json::obj([
                ("x0_slots", Json::Int(x0_slots)),
                ("x1_slots", Json::Int(x1_slots)),
                ("nominal_x1_slots", Json::Int(550)),
                (
                    "nominal_note",
                    Json::str("§8.4's 550-slot nominal assumes all nine levels admitted; the W-rule withhold leaves 8 executable arms (baseline + V1–V7)"),
                ),
                (
                    "cost_usd",
                    Json::obj([
                        ("model_spend_range", Json::str("≈ $28–$187 (§8.4 nominal, at $0.05–$0.34 per trial)")),
                        ("planning_ceiling_usd", Json::str("≈ $337 (§8.4 nominal envelope)")),
                        ("executable_note", Json::str("the executable slots are inside the declared nominal ceiling — the withheld levels never run")),
                    ]),
                ),
            ]),
        ),
        (
            "estimators",
            Json::obj([
                ("delta_hat", Json::str("per-level equal-task Δ̂ vs the baseline, joint population — descriptive")),
                ("intervals", Json::str("descriptive task bootstrap + FM intervals")),
                ("bh_q", Json::str("Benjamini–Hochberg q over the level set — descriptive")),
                ("secondary", Json::str("activation, fidelity, completion envelope, costs — descriptive")),
                ("label", Json::str("exploratory — every figure carries it; no exploration figure carries a confirmatory label (REQ-XD-207)")),
            ]),
        ),
        (
            "stop_rules",
            Json::obj([
                (
                    "classes",
                    Json::Arr(STOP_CLASSES.iter().map(|c| Json::str(*c)).collect()),
                ),
                ("rules", declared_stop_rules()),
                (
                    "verbatim",
                    Json::str("budget, infrastructure, integrity and catalog identity only — no outcome-based stopping or level dropping (REQ-XD-210)"),
                ),
            ]),
        ),
        ("selection_rule", frozen_rule_json()),
        ("selection_rule_sha256", Json::str(frozen_rule_sha256())),
        (
            "selection_packet",
            Json::obj([
                ("schema", Json::str(SELECTION_PACKET_SCHEMA)),
                ("minted_by", Json::str("R7.4 — over committed X1 records after GATE-G35")),
                ("input_manifest", Json::str("committed X1 records only — held-out and phase-2/3 outcome records refuse typed (REQ-XD-212)")),
                ("determinism", Json::str("pure function — byte-identical recompute")),
                ("hash_order", Json::str("the packet hash predates the confirmation pre-registration")),
            ]),
        ),
        (
            "labels",
            Json::obj([
                ("exploration", Json::str("exploratory — every exploration figure")),
                ("selection", Json::str(SELECTION_DISPLAY_LABEL)),
                ("combination", Json::str(COMBINATION_LABEL)),
            ]),
        ),
        (
            "expected_outcome",
            Json::str("stated before data: for config-knob effects of about 5 points, P(confirm) is 0.13–0.26; the likely honest headline is exploratory rankings plus \"no selected configuration was confirmed different from the baseline at Holm α = .05 on 37 tasks\" (§8.6)"),
        ),
        (
            "integrity",
            Json::obj([
                ("scan", Json::str("answer-scan/1 on every scored trial")),
                ("review", Json::str("HUMAN flagged-pass review per D1 F13 before each results freeze")),
            ]),
        ),
        (
            "combine_declaration",
            Json::obj([
                ("combine", Json::Bool(COMBINE_DECLARED)),
                ("member", Json::str("C₁₂ — applies the two selected levels from different factors together")),
                ("admissibility", Json::str("confirmation only; union-path declared-delta check; labelled \"combination; interaction not estimated\" (REQ-XD-214)")),
                ("note", Json::str("no exploration estimate — the additivity assumption is untested by OFAT")),
            ]),
        ),
        (
            "cohort",
            Json::str("x2 (REQ-XD-220: exploration requires a G-L1 pass for the exact H swept, follows the recorded L2 verdict, never runs concurrently with L2; any H change starts a new phase-4 cohort)"),
        ),
    ]);
    let refusals = check_x_prereg(&rec);
    if !refusals.is_empty() {
        return Err(SweepRefusal::TierViolation {
            detail: format!("the minted pre-registration fails its own validator: {refusals:?}"),
        });
    }
    Ok(rec)
}

/// `check_x_prereg(rec)` — the `hh-p4-x-prereg/1` validator: schema +
/// required members, the withheld set carries typed dispositions, the
/// stop set is exactly the §8.4 classes, the `selection_rule` member is
/// byte-equal to the frozen rule, both declared specs re-validate
/// through `register` as `one_factor_at_a_time`, the envelope math
/// agrees, and the labels carry the declared spellings.
pub fn check_x_prereg(rec: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(m) = as_obj(rec) else {
        out.push(SweepRefusal::MissingMember {
            member: "prereg".to_string(),
        });
        return out;
    };
    if m.get("schema").and_then(Json::as_str) != Some(X_PREREG_SCHEMA) {
        out.push(SweepRefusal::MissingMember {
            member: "schema = hh-p4-x-prereg/1".to_string(),
        });
    }
    for need in [
        "recorded_at",
        "ticket",
        "predates",
        "spend",
        "baseline",
        "registry",
        "w",
        "withheld",
        "tasks",
        "design",
        "envelope",
        "estimators",
        "stop_rules",
        "selection_rule",
        "selection_rule_sha256",
        "labels",
        "expected_outcome",
        "integrity",
        "combine_declaration",
        "cohort",
    ] {
        if m.get(need).is_none() || matches!(m.get(need), Some(Json::Null)) {
            out.push(SweepRefusal::MissingMember {
                member: format!("prereg.{need}"),
            });
        }
    }
    // The frozen rule — byte-equal to what the machinery executes.
    if let Some(r) = m.get("selection_rule") {
        if let Err(e) = check_frozen_rule(r) {
            out.push(e);
        }
        match rule_from_json(r) {
            Ok(rule) if rule != frozen_selection_rule() => {
                out.push(SweepRefusal::FrozenRuleMismatch {
                    detail: "the decoded parameters differ from the frozen rule".to_string(),
                })
            }
            Ok(_) => {}
            Err(e) => out.push(e),
        }
    }
    // The declared specs re-validate as `one_factor_at_a_time`.
    for leg in ["x0", "x1"] {
        if let Some(spec_j) = m
            .get("design")
            .and_then(|d| d.get(leg))
            .and_then(|d| d.get("experiment"))
        {
            match ExperimentSpec::from_json(spec_j) {
                Ok(spec) => {
                    if spec.design.kind != DesignKind::OneFactorAtATime {
                        out.push(SweepRefusal::TierViolation {
                            detail: format!("design.{leg} is not one_factor_at_a_time"),
                        });
                    }
                    if let Err(e) = spec.register(&SpecContext::member_level()) {
                        out.push(SweepRefusal::UnexpressibleSurface {
                            detail: format!("design.{leg} fails register: {e:?}"),
                        });
                    }
                }
                Err(e) => out.push(SweepRefusal::MissingMember {
                    member: format!("design.{leg}.experiment undecodable: {e:?}"),
                }),
            }
        }
    }
    // The stop set is exactly the §8.4 classes.
    if let Some(rules) = m.get("stop_rules").and_then(|s| s.get("rules")) {
        out.extend(check_stop_set(Some(rules)));
    }
    // The withheld set carries typed dispositions.
    if let Some(ws) = m.get("withheld").and_then(as_arr) {
        for (i, w) in ws.iter().enumerate() {
            let disp = as_obj(w)
                .and_then(|w| w.get("disposition"))
                .and_then(Json::as_str)
                .unwrap_or("");
            if !disp.starts_with("withheld{") {
                out.push(SweepRefusal::MissingMember {
                    member: format!("withheld[{i}].disposition is not typed"),
                });
            }
        }
    }
    // The labels carry the declared spellings.
    if let Some(l) = m.get("labels") {
        match l.get("selection").and_then(Json::as_str) {
            Some(SELECTION_DISPLAY_LABEL) => {}
            _ => out.push(SweepRefusal::MissingLabel {
                member: "labels.selection".to_string(),
            }),
        }
        match l.get("combination").and_then(Json::as_str) {
            Some(COMBINATION_LABEL) => {}
            _ => out.push(SweepRefusal::MissingLabel {
                member: "labels.combination".to_string(),
            }),
        }
    }
    // The envelope's slot math is honest: slots = arms × tasks × k.
    if let Some(e) = m.get("envelope") {
        for (slots_key, leg) in [("x0_slots", "x0"), ("x1_slots", "x1")] {
            if let (Some(decl), Some(dslots)) = (
                e.get(slots_key).and_then(Json::as_int),
                m.get("design")
                    .and_then(|d| d.get(leg))
                    .and_then(|d| d.get("slots"))
                    .and_then(Json::as_int),
            ) {
                if decl != dslots {
                    out.push(SweepRefusal::TierViolation {
                        detail: format!(
                            "envelope.{slots_key} {decl} != design.{leg}.slots {dslots}"
                        ),
                    });
                }
            }
        }
    }
    out
}

/// `registry_cites(reg, prereg)` — the pre-registration's
/// `registry.sha256` must equal the frozen registry record's own
/// `frozen_sha256` (the hash is cited, never re-typed).
pub fn check_registry_cite(reg: &Json, prereg: &Json) -> Result<(), SweepRefusal> {
    let want = reg
        .get("frozen_sha256")
        .and_then(Json::as_str)
        .unwrap_or("");
    let have = prereg
        .get("registry")
        .and_then(|r| r.get("sha256"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if want.is_empty() || have != want {
        return Err(SweepRefusal::DeclarationMismatch {
            detail: format!("prereg registry.sha256 `{have}` != frozen_sha256 `{want}`"),
        });
    }
    Ok(())
}

/// `sweep::check_merge` re-export for the pooling tests' convenience —
/// cross-identity data never merge (REQ-XD-202; consumed, not re-owned).
pub use sweep::check_merge;
