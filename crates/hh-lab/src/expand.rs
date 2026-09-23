//! `expand(spec) → CellPlan` — the pure, total, content-addressed expansion
//! (spec §6.3 `expand`; ADR-0154 D3–D4; S3.4a).
//!
//! Everything here is deterministic: no clocks, no store reads, no registry
//! reads. The context supplies the two resolution views expand needs —
//! `arm_config` (the per-arm `compose(layers + experiment layer) → resolve →
//! validate_assembly → seal` output, projected to the two configuration ids)
//! and `level_ineligible` (the pinned `registry_snapshot_id`'s capability view
//! — `Some(NaReason)` marks the cell `n/a{reason}`, never a dropped row,
//! T-LCD-15). Cells are `arm × task`; run plans key by the full tuple
//! (`run_plan_id = H(experiment_id ∥ arm_id ∥ configuration_version_id ∥
//! task_id ∥ replicate_index)` — scheduling and transport fields never enter
//! it).
//!
//! The `fractional_factorial` algebra (two-level `l^(k−p)` generators — the
//! OQ-361 ratified default) lives here too: generator spellings
//! `"F = A:B:…"` (the generated factor on the left, the defining word's
//! remaining letters `:`-joined on the right), the defining-contrast
//! subgroup under symmetric difference, the resolution, and the aliasing
//! table the plan carries.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::{idp_digest, idp_id};
use hh_ontology::compliance::NaReason;
use hh_ontology::eval::DesignKind;
use hh_ontology::lab::SplitLabel;
use hh_ontology::FactorKind;
use hh_wire::Json;

use crate::experiment::{
    run_plan_id, ArmSpec, CellPlan, EnvironmentDerivation, ExperimentRefusal, ExperimentSpec,
    FactorSpec, OrderKind, OrderPlan, PlanCell, RunPlan,
};
use crate::json_util::SchemaError;

// ── Expand inputs ───────────────────────────────────────────────────────────

/// `ExpandTask{task_id, split_label}` — the task-axis member `expand`
/// consumes; the caller resolves `suite.suite_ref` to the suite's
/// `TaskRecord`s (the engine's E-1 gate owns that read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandTask {
    /// The task id.
    pub task_id: String,
    /// The task's split label.
    pub split_label: SplitLabel,
}

/// The resolved arm configuration — the assembly chain's output projected to
/// ADR-0036's seedless `configuration_id` and the exact-bytes
/// `configuration_version_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmConfiguration {
    /// The seedless configuration id.
    pub configuration_id: String,
    /// The seeded exact-bytes version id.
    pub configuration_version_id: String,
}

/// The resolver view `expand` runs against — every context-parameterized
/// input arrives through a resolver so the function itself stays pure.
pub struct ExpandContext<'a> {
    /// `arm → {configuration_id, configuration_version_id}` — the sealed
    /// configuration the arm's `compose(layers + experiment layer)` produced.
    pub arm_config: &'a dyn Fn(&ArmSpec) -> Result<ArmConfiguration, ExpandError>,
    /// `level ref → n/a{reason}` when the level is ineligible under the pinned
    /// snapshot (`slot_choices` floor / absent capability). `None` = no
    /// capability view at this layer — no cell is marked.
    pub level_ineligible: Option<&'a LevelEligibility<'a>>,
}

/// `level ref → n/a{reason}` — the eligibility resolver type (T-LCD-15).
pub type LevelEligibility<'a> = dyn Fn(&str) -> Option<NaReason> + 'a;

/// `expand`'s failure sum — a register-class refusal, or a per-cell assembly
/// diagnostic (§6.3 `expand`'s failure column).
#[derive(Debug, Clone, PartialEq)]
pub enum ExpandError {
    /// A member of the closed refusal set surfaced at expand time (the spec is
    /// re-checked — an unregistered or drifted spec fails identically).
    Refusal(ExperimentRefusal),
    /// An arm's configuration could not be produced (the assembly/resolve/seal
    /// chain's diagnostic, carried verbatim).
    Assembly {
        /// The arm whose composition failed.
        arm: String,
        /// The diagnostic detail.
        detail: String,
    },
}

// ── Factor-space helpers ────────────────────────────────────────────────────

/// The *varied* factors — the axes the design varies. `task` and `replicate`
/// are never varied factors (the suite supplies the task axis;
/// `replicates_per_cell` the seed axis); `budget`/`model_snapshot`/`harness`/
/// `environment` factors are. A factor declared with a single level is a
/// *fixed* axis — `FactorSpec.levels` documents "≥ 2 for a varied factor" —
/// so single-level declarations (the exemplars' pinned model/environment
/// axes, ADR-0156 D1/D2) do not count toward a `paired` design's varied
/// factor and contribute nothing to a product/fraction.
pub fn varied_factors(spec: &ExperimentSpec) -> Vec<&FactorSpec> {
    spec.factors
        .iter()
        .filter(|f| {
            !matches!(f.kind, FactorKind::Task | FactorKind::Replicate) && f.levels.len() >= 2
        })
        .collect()
}

/// The factor/level point an arm occupies, restricted to the varied factors —
/// the design-space coordinate.
pub fn arm_point<'a>(spec: &'a ExperimentSpec, arm: &'a ArmSpec) -> BTreeMap<&'a str, &'a str> {
    let varied: BTreeSet<&str> = varied_factors(spec)
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    arm.level_assignment
        .iter()
        .filter(|(f, _)| varied.contains(f.as_str()))
        .map(|(f, l)| (f.as_str(), l.as_str()))
        .collect()
}

/// The Cartesian product of the varied factors' levels, in declaration order —
/// a `BTreeMap` per point keyed `factor → level_id` (deterministic).
pub fn full_product(spec: &ExperimentSpec) -> Vec<BTreeMap<String, String>> {
    let varied = varied_factors(spec);
    let mut points: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    for f in &varied {
        let mut next = Vec::with_capacity(points.len() * f.levels.len());
        for p in &points {
            for l in &f.levels {
                let mut q = p.clone();
                q.insert(f.name.clone(), l.level_id.clone());
                next.push(q);
            }
        }
        points = next;
    }
    points
}

/// Whether the declared arms cover `points` — every design point occupied by
/// at least one arm, every arm on a declared point, and no point occupied
/// twice *within one comparand group* (arms partition by `match_spec.mode`:
/// ADR-0156 D2 / CF-334 admit a companion arm at an already-occupied point
/// under a different match mode — the `iso_cost` arm of
/// `lab/control-strategy-family-v1` shares a design point with a
/// `matched_cap` arm, the OQ-363 run-sharing question; ADR-0213's interim
/// rule duplicates the subject runs). At least one comparand group must
/// cover every point exactly once — that group is the primary contrast.
/// For a single-mode spec this reduces to "each point exactly once".
pub fn arms_cover(spec: &ExperimentSpec, points: &[BTreeMap<String, String>]) -> bool {
    let want: BTreeSet<BTreeMap<String, String>> = points.iter().cloned().collect();
    let mut covered: BTreeSet<BTreeMap<String, String>> = BTreeSet::new();
    let mut groups: BTreeMap<String, BTreeSet<BTreeMap<String, String>>> = BTreeMap::new();
    for a in &spec.arms {
        let p: BTreeMap<String, String> = arm_point(spec, a)
            .into_iter()
            .map(|(f, l)| (f.to_string(), l.to_string()))
            .collect();
        if !want.contains(&p) {
            return false; // an arm sits off the declared design
        }
        covered.insert(p.clone());
        let mode = a
            .match_spec
            .as_ref()
            .map(|m| m.mode.as_str())
            .unwrap_or("none");
        if !groups.entry(mode.to_string()).or_default().insert(p) {
            return false; // duplicate design point within one comparand group
        }
    }
    covered == want && groups.values().any(|g| g.len() == want.len())
}

// ── Two-level fractional-factorial algebra (OQ-361 ratified default) ────────

/// A parsed generator `"F = A:B:C"` — factor `F` is generated by the product
/// of the named factors' signs (±1 coding, level index 0 ↦ +1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    /// The generated factor name (the word's own letter).
    pub factor: String,
    /// The word's remaining factors (sorted, deduplicated).
    pub word: Vec<String>,
}

impl Generator {
    /// The defining word — `{factor} ∪ word` (sorted).
    pub fn defining_word(&self) -> BTreeSet<String> {
        let mut w: BTreeSet<String> = self.word.iter().cloned().collect();
        w.insert(self.factor.clone());
        w
    }

    /// The canonical spelling (`"F = A:B:C"`).
    pub fn spelling(&self) -> String {
        format!("{}={}", self.factor, self.word.join(":"))
    }
}

/// Parse a `"F = A:B:C"`/`"F=A:B:C"` generator spelling; `None` on any other
/// shape (empty sides, an empty word member, `=` anywhere else).
pub fn parse_generator(s: &str) -> Option<Generator> {
    let (lhs, rhs) = s.split_once('=')?;
    if rhs.contains('=') {
        return None;
    }
    let factor = lhs.trim();
    if factor.is_empty() {
        return None;
    }
    let mut word: Vec<String> = rhs.split(':').map(|w| w.trim().to_string()).collect();
    if word.iter().any(|w| w.is_empty()) {
        return None;
    }
    word.sort();
    word.dedup();
    if word.iter().any(|w| w == factor) {
        return None; // the letter may not appear in its own word
    }
    Some(Generator {
        factor: factor.to_string(),
        word,
    })
}

/// The defining-contrast subgroup — the generators' defining words closed
/// under symmetric difference, plus `∅`. Sorted by (length, spelling) for a
/// deterministic rendering.
pub fn defining_subgroup(generators: &[Generator]) -> Vec<BTreeSet<String>> {
    let mut subgroup: BTreeSet<BTreeSet<String>> = BTreeSet::new();
    subgroup.insert(BTreeSet::new());
    for g in generators {
        let w = g.defining_word();
        let additions: Vec<BTreeSet<String>> = subgroup
            .iter()
            .map(|e| e.symmetric_difference(&w).cloned().collect())
            .collect();
        subgroup.extend(additions);
    }
    let mut v: Vec<BTreeSet<String>> = subgroup.into_iter().collect();
    v.sort_by_key(|a| (a.len(), effect_spelling(a)));
    v
}

/// The resolution the generators define — the minimum non-empty defining-word
/// length.
pub fn resolution_of(subgroup: &[BTreeSet<String>]) -> u32 {
    subgroup
        .iter()
        .filter(|w| !w.is_empty())
        .map(|w| w.len() as u32)
        .min()
        .unwrap_or(0)
}

/// The effect spelling — the `:`-joined sorted member names (`{"a","b"}` →
/// `"a:b"`; the same spelling `PreRegistration.interactions[]` uses).
pub fn effect_spelling(effect: &BTreeSet<String>) -> String {
    effect.iter().cloned().collect::<Vec<_>>().join(":")
}

/// Parse an effect spelling back to its factor set; `None` on an empty or
/// duplicated member.
pub fn parse_effect(s: &str) -> Option<BTreeSet<String>> {
    if s.is_empty() {
        return None;
    }
    let set: BTreeSet<String> = s.split(':').map(str::to_string).collect();
    if set.iter().any(|m| m.is_empty()) || set.len() != s.split(':').count() {
        return None;
    }
    Some(set)
}

/// `effect`'s alias class — `{effect Δ w : w ∈ subgroup}` (the effect is its
/// own alias under `w = ∅`).
pub fn alias_class(
    effect: &BTreeSet<String>,
    subgroup: &[BTreeSet<String>],
) -> BTreeSet<BTreeSet<String>> {
    subgroup
        .iter()
        .map(|w| effect.symmetric_difference(w).cloned().collect())
        .collect()
}

/// The aliasing table the plan carries —
/// `{"defining_words": […], "aliases": {"<effect>": ["<aliased>", …]}}` over
/// every main effect and two-factor interaction of the varied factors
/// (deterministic: `BTreeMap`/sorted members throughout).
pub fn aliasing_table(factors: &[&FactorSpec], subgroup: &[BTreeSet<String>]) -> Json {
    let names: Vec<String> = factors.iter().map(|f| f.name.clone()).collect();
    let mut effects: BTreeSet<BTreeSet<String>> = BTreeSet::new();
    for n in &names {
        effects.insert(BTreeSet::from([n.clone()]));
    }
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            effects.insert(BTreeSet::from([a.clone(), b.clone()]));
        }
    }
    let mut aliases = BTreeMap::new();
    for e in &effects {
        let mut others: Vec<String> = alias_class(e, subgroup)
            .into_iter()
            .filter(|a| a != e)
            .map(|a| effect_spelling(&a))
            .collect();
        others.sort();
        aliases.insert(
            effect_spelling(e),
            Json::Arr(others.iter().map(Json::str).collect()),
        );
    }
    let words: Vec<String> = subgroup
        .iter()
        .filter(|w| !w.is_empty())
        .map(effect_spelling)
        .collect();
    Json::obj([
        (
            "defining_words",
            Json::Arr(words.iter().map(Json::str).collect()),
        ),
        ("aliases", Json::Obj(aliases)),
    ])
}

/// The declared fraction's design points — the `2^(k−p)` assignments: every
/// combination over the *free* (non-generated) factors, with each generated
/// factor's level fixed by the ±1 product convention (level index =
/// XOR of the word's level indices).
pub fn fraction_points(
    spec: &ExperimentSpec,
    generators: &[Generator],
) -> Vec<BTreeMap<String, String>> {
    let varied = varied_factors(spec);
    let generated: BTreeSet<&str> = generators.iter().map(|g| g.factor.as_str()).collect();
    let free: Vec<&&FactorSpec> = varied
        .iter()
        .filter(|f| !generated.contains(f.name.as_str()))
        .collect();
    let mut points: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    for f in &free {
        let mut next = Vec::with_capacity(points.len() * f.levels.len());
        for p in &points {
            for l in &f.levels {
                let mut q = p.clone();
                q.insert(f.name.clone(), l.level_id.clone());
                next.push(q);
            }
        }
        points = next;
    }
    // Fix each generated factor by the XOR-parity convention.
    for p in &mut points {
        for g in generators {
            let Some(f) = varied.iter().find(|f| f.name == g.factor) else {
                continue;
            };
            let mut parity = 0usize;
            for w in &g.word {
                if let (Some(wf), Some(lid)) =
                    (varied.iter().find(|f| &f.name == w), p.get(w.as_str()))
                {
                    parity ^= wf
                        .levels
                        .iter()
                        .position(|l| &l.level_id == lid)
                        .unwrap_or(0);
                }
            }
            if let Some(l) = f.levels.get(parity % f.levels.len().max(1)) {
                p.insert(g.factor.clone(), l.level_id.clone());
            }
        }
    }
    points
}

// ── Ordering ────────────────────────────────────────────────────────────────

/// The block a run plan schedules in (ADR-0155 D3's `interleaved_blocked`:
/// replicate index `r` is block `r`; `serial`/`interleaved`/`random_permuted`
/// run one block).
pub fn block_of(order: &OrderPlan, kind: OrderKind, rp: &RunPlan) -> u32 {
    match kind {
        OrderKind::InterleavedBlocked => rp.replicate_index,
        _ => {
            let _ = order;
            0
        }
    }
}

/// The deterministic order key — `(block, position)`: `serial` runs in
/// declaration order (`decl_index`); `random_permuted` positions by
/// `H(permutation_seed ∥ run_plan_id)` so the order is reproducible from the
/// recorded seed alone (S-6); the `interleaved` orders produce the
/// **task-blocked, arm-interleaved** sequence (AC-R-2.10.3-4): within a
/// block, tasks group by a seeded task rank, then each replicate round
/// rotates the arms by a seeded arm rank — `(task_rank, replicate,
/// arm_rank)`, all derived under the `experiment.order.*` domains from
/// `permutation_seed`.
pub fn order_key(
    order: &OrderPlan,
    kind: OrderKind,
    rp: &RunPlan,
    decl_index: u32,
    task_id: &str,
    arm_id: &str,
) -> (u32, String) {
    let block = block_of(order, kind, rp);
    match kind {
        OrderKind::Serial => (block, format!("{decl_index:012}")),
        OrderKind::Interleaved | OrderKind::InterleavedBlocked => {
            let task_rank = derive_u64(
                "experiment.order.task",
                &format!("{}:{}", order.permutation_seed, task_id),
            );
            let arm_rank = derive_u64(
                "experiment.order.arm",
                &format!("{}:{}:{}", order.permutation_seed, task_id, arm_id),
            );
            (
                block,
                format!("{task_rank:016x}:{:08}:{arm_rank:016x}", rp.replicate_index),
            )
        }
        _ => (
            block,
            idp_digest(
                "experiment.order",
                format!("{}:{}", order.permutation_seed, rp.run_plan_id).as_bytes(),
            ),
        ),
    }
}

// ── expand ──────────────────────────────────────────────────────────────────

/// A deterministic `u64` derived under `domain` from `material` (the first
/// 16 hex digits of the `idp` digest).
fn derive_u64(domain: &str, material: &str) -> u64 {
    let hex = idp_digest(domain, material.as_bytes());
    u64::from_str_radix(&hex[..16], 16).unwrap_or(0)
}

/// `expand(spec, tasks, ctx) → CellPlan` — pure and total over a registered
/// spec (§6.3 `expand`; ADR-0154 D3–D4). Cells are `arm × task` in
/// declaration order; each eligible cell yields `replicates_per_cell` run
/// plans; an ineligible cell is planned with `na_reason` and yields no run
/// plans (planned-but-never-scheduled). The plan is content-addressed under
/// `cell_plan`.
pub fn expand(
    spec: &ExperimentSpec,
    tasks: &[ExpandTask],
    ctx: &ExpandContext<'_>,
) -> Result<CellPlan, ExpandError> {
    // The arm configurations resolve once — the sealed configuration the
    // `compose(layers + experiment layer) → … → seal` chain produced.
    let mut configs: Vec<ArmConfiguration> = Vec::with_capacity(spec.arms.len());
    for arm in &spec.arms {
        configs.push((ctx.arm_config)(arm)?);
    }

    let mut cells = Vec::new();
    let mut run_plans = Vec::new();
    for (arm, cfg) in spec.arms.iter().zip(configs.iter()) {
        // A cell is `n/a{reason}` when any assigned level is ineligible under
        // the pinned snapshot (sorted factor order ⇒ the reason is stable).
        let na = ctx.level_ineligible.and_then(|check| {
            let mut assigned: Vec<(&String, &String)> = arm.level_assignment.iter().collect();
            assigned.sort();
            assigned.iter().find_map(|(fname, lid)| {
                let level = spec
                    .factors
                    .iter()
                    .find(|f| &f.name == *fname)
                    .and_then(|f| f.levels.iter().find(|l| &l.level_id == *lid))?;
                check(&level.ref_)
            })
        });
        for task in tasks {
            let cell_id = idp_id(
                "experiment.cell",
                Json::Arr(vec![
                    Json::str(&spec.experiment_id),
                    Json::str(&arm.arm_id),
                    Json::str(&task.task_id),
                ])
                .to_canonical_string()
                .as_bytes(),
            );
            let cell = PlanCell {
                cell_id: cell_id.clone(),
                arm_id: arm.arm_id.clone(),
                configuration_id: cfg.configuration_id.clone(),
                configuration_version_id: cfg.configuration_version_id.clone(),
                task_id: task.task_id.clone(),
                split_label: task.split_label,
                na_reason: na,
            };
            if cell.na_reason.is_none() {
                for r in 0..spec.replicates_per_cell {
                    let rpid = run_plan_id(
                        &spec.experiment_id,
                        &arm.arm_id,
                        &cfg.configuration_version_id,
                        &task.task_id,
                        r,
                    );
                    let mut seed_material = BTreeMap::new();
                    if spec.seed_policy.harness_rng {
                        seed_material.insert(
                            "harness_seed".to_string(),
                            Json::Int(derive_u64("experiment.seed.harness", &rpid) as i64),
                        );
                    }
                    if spec.seed_policy.requested_sampling_seed {
                        seed_material.insert(
                            "sampling_seed".to_string(),
                            Json::Int(derive_u64("experiment.seed.sampling", &rpid) as i64),
                        );
                    }
                    run_plans.push(RunPlan {
                        run_plan_id: rpid.clone(),
                        cell_id: cell_id.clone(),
                        replicate_index: r,
                        seed_material: Json::Obj(seed_material),
                        environment_derivation: EnvironmentDerivation::FreshFromImage,
                        // ADR-0128 `cold_start`: every run is its own cache
                        // scope — the salt is content-derived from the plan id.
                        cache_scope_salt: idp_id("experiment.cache_scope", rpid.as_bytes()),
                    });
                }
            }
            cells.push(cell);
        }
    }

    let blocks = match spec.scheduling.order {
        OrderKind::InterleavedBlocked => (0..spec.replicates_per_cell)
            .map(|r| format!("block:{r}"))
            .collect(),
        OrderKind::Serial => vec!["block:serial".to_string()],
        OrderKind::Interleaved | OrderKind::RandomPermuted => {
            vec!["block:all".to_string()]
        }
    };

    let fractional = spec.design.kind == DesignKind::FractionalFactorial;
    let generators = if fractional {
        spec.design.generators.clone()
    } else {
        None
    };
    let aliasing_table = if fractional {
        let gens: Vec<Generator> = generators
            .clone()
            .unwrap_or_default()
            .iter()
            .filter_map(|g| parse_generator(g))
            .collect();
        Some(aliasing_table(
            &varied_factors(spec),
            &defining_subgroup(&gens),
        ))
    } else {
        None
    };

    let mut plan = CellPlan {
        plan_id: String::new(),
        cells,
        run_plans,
        order: OrderPlan {
            permutation_seed: spec.scheduling.permutation_seed.clone(),
            blocks,
        },
        generators,
        aliasing_table,
    };
    plan.plan_id = plan.plan_id();
    Ok(plan)
}

/// The `hh-experiment/1` schema error type re-exported for expand diagnostics.
pub type ExpandSchemaError = SchemaError;
