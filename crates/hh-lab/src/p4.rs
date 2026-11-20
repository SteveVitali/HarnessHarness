//! `p4` — the second-pass phase-4 level-admissibility machinery
//! (manifest 326/R7.1; spec §8.1–§8.3; design/37 §2.4; design/38):
//!
//! - **the pass's `sweep-registry/1`** — a NEW exploration identity
//!   (`x2:`; level changes after R6.43's x1 freeze never merge,
//!   REQ-XD-202). R6.43's validator/freeze/merge machinery is
//!   consumed, never re-owned. Every Tier-1 level is declared with
//!   its sealed `sealed_ref` + `override_spellings`; Tier 2 keeps its
//!   reasons plus its admission verdict.
//! - **tier-2 admission** (REQ-XD-225; AC-8.1-3): a candidate enters
//!   only a *later* exploration identity, after its dependency lands
//!   and its forecast activation meets the floor.
//! - **the effort lane** (REQ-XD-204; AC-8.3-1): a declared lane
//!   parameter of the level document (`effort_pin` →
//!   `slots/model_port/params/effort`), proxy-pinned, requested and
//!   applied values recorded per call; any call whose applied value
//!   differs from the declaration makes the level ineligible —
//!   `ineligible{mis_pinned:…}`, never a silent pass.
//! - **the W rule** (REQ-XD-205 carrying REQ-XE-127; AC-8.3-2): F(w)
//!   is the share of committed baseline trials whose peak upstream
//!   input tokens exceed w, read from **non-outcome token records
//!   only**. W is the largest candidate ≥ 32 768 with F(W) ≥ 0.5; if
//!   F(32 768) < 0.25 both composite levels are
//!   `withheld{inert_at_admissible_window}`. Task-conditioned caps
//!   and input padding refuse typed.
//! - **the §4.2-generic activation fold** (REQ-XD-208; AC-8.2-2):
//!   per-level activation incl. exact counterfactual activation for
//!   deterministic renderer/executor knobs. `hh-activation/1` is
//!   unchanged — this fold lives beside it.
//! - **the inert forecast** (REQ-XD-209; AC-8.2-3): pre-spend, from
//!   non-outcome baseline records; a level below the 0.25 floor is
//!   `withheld{inert_predicted}`, retained in the registry with its
//!   forecast. The input manifest refuses outcome fields.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hharness_assembly::grammar::{Assembly, ParamRequirement, ParamType, ParameterSpec};
use hharness_assembly::resolve::{resolve, ResolveEnv};
use hharness_assembly::space::space;
use hharness_assembly::Stage1Catalog;
use hharness_hir::records::{AgentProcessBody, KindRecord};
use hharness_hir::refs::ComponentVariantRef;
use hharness_hir::{SlotBinding, SlotBindings};
use hharness_identity::names::ResolveMode;
use hharness_provenance::ProvenanceRecord;
use hharness_registry::records::RegistryRecord;
use hharness_registry::store::RegistryStore;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::sweep::{
    self, materialize_level, sealed_json, SealedLevel, SweepRefusal, TIER1_LEVEL_IDS,
    TIER2_CANDIDATES,
};

fn as_obj(j: &Json) -> Option<&BTreeMap<String, Json>> {
    match j {
        Json::Obj(m) => Some(m),
        _ => None,
    }
}

/// The second pass's exploration tag — `x2:sha256:<freeze hash>`
/// names the new identity (REQ-XD-202; x1's data never merge).
pub const P4_TAG: &str = "x2";

// ── REQ-XD-205 + REQ-XE-127 — the W rule ─────────────────────────────────────

/// The §8.3 candidate window caps.
pub const W_CANDIDATES: [u64; 5] = [32_768, 49_152, 65_536, 98_304, 131_072];

/// W is never below 32 768.
pub const W_FLOOR: u64 = 32_768;

/// The outcome-field vocabulary the W-rule and forecast input
/// manifests refuse (X-07 — non-outcome token records only). A field
/// whose name (or last `.`-segment) matches refuses.
pub const OUTCOME_FIELDS: &[&str] = &[
    "reward", "verdict", "outcome", "resolved", "resolve", "score", "pass", "passed", "success",
    "solved", "answer", "grade", "result", "label", "winner",
];

/// `true` when `field` names an outcome member (exact or last
/// `.`-segment match — `usage.reward` refuses as surely as `reward`).
pub fn is_outcome_field(field: &str) -> bool {
    let f = field.trim().to_ascii_lowercase();
    let last = f.rsplit('.').next().unwrap_or(&f);
    OUTCOME_FIELDS.contains(&f.as_str()) || OUTCOME_FIELDS.contains(&last)
}

/// `check_non_outcome_fields(fields)` — an input manifest (or a
/// record's member set) names only non-outcome fields; any outcome
/// member refuses typed (REQ-XD-205/209's input-manifest refusal).
pub fn check_non_outcome_fields<'a, I>(fields: I) -> Result<(), SweepRefusal>
where
    I: IntoIterator<Item = &'a str>,
{
    for f in fields {
        if is_outcome_field(f) {
            return Err(SweepRefusal::OutcomeField {
                field: f.to_string(),
            });
        }
    }
    Ok(())
}

/// `check_record_non_outcome(rec)` — the per-record leg: a token
/// record carrying an outcome member refuses on read (the manifest's
/// declared field list is not the only guard).
pub fn check_record_non_outcome(rec: &Json) -> Result<(), SweepRefusal> {
    if let Some(m) = as_obj(rec) {
        for k in m.keys() {
            if is_outcome_field(k) {
                return Err(SweepRefusal::OutcomeField { field: k.clone() });
            }
        }
    }
    Ok(())
}

/// `f_over(peaks, w) → (exceed, total)` — the exact count of trials
/// whose peak upstream input tokens exceed `w` (F(w) = exceed/total;
/// exact rational, never a float).
pub fn f_over(peaks: &[u64], w: u64) -> (u64, u64) {
    (
        peaks.iter().filter(|p| **p > w).count() as u64,
        peaks.len() as u64,
    )
}

/// The W-rule outcome over a committed peak-token distribution.
#[derive(Debug, Clone, PartialEq)]
pub enum WOutcome {
    /// The largest admissible cap meeting the target F(W) ≥ 0.5.
    Chosen {
        /// The frozen window cap (uniform over every task/arm).
        w: u64,
    },
    /// F(32 768) < 0.25 — both composite levels are withheld
    /// (`withheld{inert_at_admissible_window}`).
    WithheldInertAtAdmissibleWindow,
    /// The input corpus carried no trials — `pending{records}`,
    /// never a fabricated F.
    PendingRecords,
}

impl WOutcome {
    /// The recorded disposition spelling.
    pub fn spelling(&self) -> String {
        match self {
            WOutcome::Chosen { w } => format!("chosen{{w={w}}}"),
            WOutcome::WithheldInertAtAdmissibleWindow => {
                "withheld{inert_at_admissible_window}".to_string()
            }
            WOutcome::PendingRecords => "pending{records}".to_string(),
        }
    }
}

/// The full derivation record — F per candidate (exact num/den), the
/// outcome and the rule it applies.
#[derive(Debug, Clone)]
pub struct WRule {
    /// `(cap, exceed, total)` per candidate.
    pub table: Vec<(u64, u64, u64)>,
    /// The outcome.
    pub outcome: WOutcome,
}

/// `w_rule(peaks)` — the §8.3 rule over the committed peak-token
/// distribution: W is the largest candidate ≥ 32 768 with F(W) ≥ 0.5;
/// if F(32 768) < 0.25 both composite levels withhold. Exact rational
/// comparisons — `exceed*2 ≥ total`, `exceed*4 < total`.
pub fn w_rule(peaks: &[u64]) -> WRule {
    if peaks.is_empty() {
        return WRule {
            table: W_CANDIDATES.iter().map(|w| (*w, 0, 0)).collect(),
            outcome: WOutcome::PendingRecords,
        };
    }
    let table: Vec<(u64, u64, u64)> = W_CANDIDATES
        .iter()
        .map(|w| {
            let (e, t) = f_over(peaks, *w);
            (*w, e, t)
        })
        .collect();
    // The withhold leg: F(32 768) < 0.25 ⟺ exceed*4 < total.
    let (e0, t0) = f_over(peaks, W_FLOOR);
    if e0 * 4 < t0 {
        return WRule {
            table,
            outcome: WOutcome::WithheldInertAtAdmissibleWindow,
        };
    }
    // The largest candidate with F(W) ≥ 0.5 ⟺ exceed*2 ≥ total.
    let chosen = table
        .iter()
        .rev()
        .find(|(_, e, t)| e * 2 >= *t)
        .map(|(w, _, _)| *w);
    WRule {
        table,
        outcome: match chosen {
            Some(w) => WOutcome::Chosen { w },
            // No candidate meets the target but the floor passed: the
            // floor cap stands (W is never below 32 768).
            None => WOutcome::Chosen { w: W_FLOOR },
        },
    }
}

/// The `w-rule` member of the derivation record (exact num/den +
/// ppm for readability — the rational is authoritative).
pub fn w_rule_json(wr: &WRule) -> Json {
    Json::obj([
        ("schema", Json::str("hh-p4-w-rule/1")),
        (
            "rule",
            Json::str(
                "W is the largest value in {32768, 49152, 65536, 98304, 131072} with F(W) ≥ 0.5; W ≥ 32768; if F(32768) < 0.25 both composite levels are withheld{inert_at_admissible_window}; W applies identically to every task and replicate",
            ),
        ),
        (
            "f",
            Json::Arr(
                wr.table
                    .iter()
                    .map(|(w, e, t)| {
                        Json::obj([
                            ("w", Json::Int(*w as i64)),
                            ("exceed", Json::Int(*e as i64)),
                            ("total", Json::Int(*t as i64)),
                            (
                                "ppm",
                                Json::Int(if *t == 0 {
                                    0
                                } else {
                                    (e * 1_000_000 / t) as i64
                                }),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("outcome", Json::str(wr.outcome.spelling())),
    ])
}

/// `check_w_decl(decl)` — the declared composite applies one W to
/// every task and replicate: a task-/replicate-conditioned cap or an
/// input-padding member refuses typed (REQ-XD-205 carrying
/// REQ-XE-127).
pub fn check_w_decl(decl: &Json) -> Vec<SweepRefusal> {
    let mut out = Vec::new();
    let Some(m) = as_obj(decl) else {
        out.push(SweepRefusal::MissingMember {
            member: "w_decl".to_string(),
        });
        return out;
    };
    for k in m.keys() {
        let kl = k.to_ascii_lowercase();
        if kl.contains("task") || kl.contains("replicate") || kl.contains("condition") {
            out.push(SweepRefusal::TaskConditionedCap {
                detail: format!("member `{k}` conditions the cap"),
            });
        }
        if kl.contains("pad") {
            out.push(SweepRefusal::InputPadding { detail: k.clone() });
        }
    }
    for need in ["strategy", "window_cap"] {
        if m.get(need).is_none() {
            out.push(SweepRefusal::MissingMember {
                member: format!("w_decl.{need}"),
            });
        }
    }
    match m.get("window_cap").and_then(Json::as_int) {
        Some(w) if w < W_FLOOR as i64 => out.push(SweepRefusal::TaskConditionedCap {
            detail: format!("window_cap {w} < the {W_FLOOR} floor"),
        }),
        Some(_) => {}
        None => out.push(SweepRefusal::MissingMember {
            member: "w_decl.window_cap".to_string(),
        }),
    }
    out
}

// ── REQ-XD-204 — the effort lane, proxy pin, applied-vs-declared ─────────────

/// One call's recorded pin evidence — the proxy pin surface's
/// `mutations.effort` triple (requested, applied, path) plus the call
/// id (R6.39's `apply_policy` record shape, consumed not re-owned).
#[derive(Debug, Clone)]
pub struct LaneCall {
    /// The call id.
    pub call_id: String,
    /// The value the request carried (None when the pin inserted it).
    pub requested: Option<String>,
    /// The value the proxy applied.
    pub applied: String,
    /// The surface effort path (`reasoning_effort` /
    /// `output_config.effort`).
    pub path: String,
}

/// `check_effort_fidelity(level_id, declared, calls)` — REQ-XD-204:
/// the level document's declared lane parameter is proxy-pinned;
/// requested and applied values are recorded per call; any call whose
/// applied value differs from the declaration makes the level
/// ineligible for selection — `ineligible{mis_pinned:…}`.
pub fn check_effort_fidelity(
    level_id: &str,
    declared: &str,
    calls: &[LaneCall],
) -> Result<Json, SweepRefusal> {
    for c in calls {
        if c.applied != declared {
            return Err(SweepRefusal::MisPinned {
                detail: format!(
                    "{level_id}: call {} applied `{}` != declared `{}` (requested {})",
                    c.call_id,
                    c.applied,
                    declared,
                    c.requested.as_deref().unwrap_or("∅"),
                ),
            });
        }
    }
    Ok(Json::obj([
        ("level_id", Json::str(level_id)),
        ("declared", Json::str(declared)),
        ("fidelity", Json::str("applied = declared on 100% of calls")),
        (
            "calls",
            Json::Arr(
                calls
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("call_id", Json::str(&c.call_id)),
                            (
                                "requested",
                                c.requested.as_deref().map(Json::str).unwrap_or(Json::Null),
                            ),
                            ("applied", Json::str(&c.applied)),
                            ("path", Json::str(&c.path)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]))
}

// ── REQ-XD-208 — the §4.2-generic activation fold ────────────────────────────

/// The activation metric kinds of §8.1 — a §4.2-generic fold, beside
/// `hh-activation/1` (whose frozen leg vocabulary is unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMetric {
    /// The knob acts on every request — activation 1.
    Always,
    /// Wire-visible on every request — activation 1.
    WireVisible,
    /// Share of first subjects the knob touches (conditional).
    ShareOfFirstSubjects,
    /// Exact counterfactual activation — a deterministic renderer or
    /// executor knob: replay the baseline evidence under the level
    /// and count the changed units exactly (no sampling error).
    ExactCounterfactual,
}

impl ActivationMetric {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            ActivationMetric::Always => "always",
            ActivationMetric::WireVisible => "wire_visible",
            ActivationMetric::ShareOfFirstSubjects => "share_of_first_subjects",
            ActivationMetric::ExactCounterfactual => "exact_counterfactual",
        }
    }
}

/// The evidence a fold consumes — counts only, never outcome data.
#[derive(Debug, Clone, Copy)]
pub struct ActivationEvidence {
    /// Units the knob exercised (calls rendered differently, commands
    /// over the timeout, requests carrying reasoning).
    pub exercised: u64,
    /// Units observed.
    pub total: u64,
}

/// `fold_activation(level_id, metric, ev)` — the per-level activation
/// as an exact rational (`num/den`; ppm for readability). An
/// `Always`/`WireVisible` fold is 1; a conditional share is
/// exercised/total; `ExactCounterfactual` is the exact changed/total
/// of a deterministic knob — the counterfactual recomputes the
/// renderer/executor output, it does not sample it.
pub fn fold_activation(level_id: &str, metric: ActivationMetric, ev: ActivationEvidence) -> Json {
    let (num, den, exact) = match metric {
        ActivationMetric::Always | ActivationMetric::WireVisible => (1u64, 1u64, true),
        ActivationMetric::ShareOfFirstSubjects => (ev.exercised, ev.total, false),
        ActivationMetric::ExactCounterfactual => (ev.exercised, ev.total, true),
    };
    Json::obj([
        ("level_id", Json::str(level_id)),
        ("metric", Json::str(metric.as_str())),
        ("activation_num", Json::Int(num as i64)),
        ("activation_den", Json::Int(den as i64)),
        (
            "activation_ppm",
            Json::Int(if den == 0 {
                0
            } else {
                (num * 1_000_000 / den) as i64
            }),
        ),
        ("exact", Json::Bool(exact)),
        (
            "note",
            Json::str(if exact {
                "deterministic counterfactual — recomputed, never sampled"
            } else {
                "share over the recorded evidence"
            }),
        ),
    ])
}

/// The activation ppm of a fold record (0 when the denominator is 0).
pub fn activation_ppm(fold: &Json) -> u64 {
    fold.get("activation_ppm")
        .and_then(Json::as_int)
        .unwrap_or(0) as u64
}

// ── REQ-XD-209 — the pre-spend inert forecast ────────────────────────────────

/// The proposed activation floor (design/37 §2.4: 0.25).
pub const ACTIVATION_FLOOR_PPM: u64 = 250_000;

/// `forecast_disposition(ppm)` — a level below the floor is
/// `withheld{inert_predicted}` before X1 spend, retained in the
/// registry with its forecast (REQ-XD-209).
pub fn forecast_disposition(activation_ppm: u64) -> &'static str {
    if activation_ppm >= ACTIVATION_FLOOR_PPM {
        "admitted{forecast_ok}"
    } else {
        "withheld{inert_predicted}"
    }
}

/// One level's forecast entry — the fold plus its disposition. A
/// negative activation ppm is an *unmeasured* fold (the corpus
/// cannot bear the knob's evidence) — recorded honestly, never
/// folded into a floor verdict.
pub fn forecast_entry(level_id: &str, fold: Json) -> Json {
    let raw = fold
        .get("activation_ppm")
        .and_then(Json::as_int)
        .unwrap_or(-1);
    let disposition = if raw < 0 {
        "unmeasured{evidence_absent}".to_string()
    } else {
        forecast_disposition(raw as u64).to_string()
    };
    Json::obj([
        ("level_id", Json::str(level_id)),
        ("fold", fold),
        ("floor_ppm", Json::Int(ACTIVATION_FLOOR_PPM as i64)),
        ("disposition", Json::str(disposition)),
    ])
}

// ── REQ-XD-225 — Tier-2 admission ────────────────────────────────────────────

/// `check_tier2_admission(level_id, dependency_landed, forecast_ppm)`
/// — a Tier-2 candidate enters only a *later* exploration identity,
/// after its dependency lands and its forecast activation meets the
/// floor. This pass's identity (`x2`) never admits a Tier-2 entry —
/// the verdict records the later identity or the refusal reason.
pub fn check_tier2_admission(
    level_id: &str,
    dependency_landed: bool,
    forecast_ppm: Option<u64>,
) -> Json {
    let (verdict, reason): (&str, String) = if !dependency_landed {
        (
            "refused{dependency_not_landed}",
            "the candidate's dependency has not landed — it may only enter a later exploration identity after it does".to_string(),
        )
    } else {
        match forecast_ppm {
            Some(ppm) if ppm >= ACTIVATION_FLOOR_PPM => (
                "admits{later_identity}",
                "dependency landed and the forecast meets the activation floor — admission is to a LATER identity, never this one".to_string(),
            ),
            Some(ppm) => (
                "refused{forecast_below_floor}",
                format!("forecast activation {ppm} ppm < the {ACTIVATION_FLOOR_PPM} ppm floor"),
            ),
            None => (
                "refused{no_forecast}",
                "no activation forecast on record — admission needs a measured forecast".to_string(),
            ),
        }
    };
    Json::obj([
        ("level_id", Json::str(level_id)),
        ("tier", Json::Int(2)),
        ("dependency_landed", Json::Bool(dependency_landed)),
        (
            "forecast_ppm",
            forecast_ppm
                .map(|p| Json::Int(p as i64))
                .unwrap_or(Json::Null),
        ),
        ("this_identity", Json::str("never admitted")),
        ("verdict", Json::str(verdict)),
        ("reason", Json::str(&reason)),
    ])
}

/// The verdict member of an admission record.
pub fn admission_verdict(rec: &Json) -> Option<String> {
    rec.get("verdict")
        .and_then(Json::as_str)
        .map(str::to_string)
}

// ── the pass's baseline + sealing (enumerate→seal consumed from R6.43) ───────

/// One Tier-1 level's pass-2 seal declaration: the override spellings
/// (attribution) and the concrete declared typed paths the doc-delta
/// check enforces.
#[derive(Debug, Clone)]
pub struct P4LevelDecl {
    /// The level id.
    pub level_id: &'static str,
    /// The level name.
    pub level_name: &'static str,
    /// The factor id.
    pub factor_id: &'static str,
    /// The override spellings (each OFAT perturbation; composites
    /// carry one spelling per member — S + W, tools + template).
    pub spellings: &'static [&'static str],
    /// The concrete declared typed paths (assembly spelling).
    pub typed_paths: &'static [&'static str],
    /// `false` while the level is withheld (no sealed ref this pass —
    /// honest absence, never a placeholder seal).
    pub seal: bool,
}

/// The pass-2 level spellings. V8/V9 are the §8.3 composites — they
/// seal only under a declared W; with the W-rule's withhold their
/// spellings stay declared, never sealed.
pub fn p4_level_decls() -> Vec<P4LevelDecl> {
    vec![
        P4LevelDecl {
            level_id: "V1",
            level_name: "eff-low",
            factor_id: "reasoning effort · declared lane parameter, proxy pin",
            spellings: &["effort_pin=low"],
            typed_paths: &[
                "lane/effort_pin",
                "slots/model_port/params/effort",
                "values/effort_pin",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V2",
            level_name: "eff-max",
            factor_id: "reasoning effort · declared lane parameter, proxy pin",
            spellings: &["effort_pin=max"],
            typed_paths: &[
                "lane/effort_pin",
                "slots/model_port/params/effort",
                "values/effort_pin",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V3",
            level_name: "cmd-120",
            factor_id: "exec timeout exec_defaults.timeout_ms",
            spellings: &["cmd_timeout_ms=120000"],
            typed_paths: &[
                "ToolCapability.exec_defaults.timeout_ms",
                "values/cmd_timeout_ms",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V4",
            level_name: "obs-2.5k",
            factor_id: "observation renderer json_head_tail@1",
            spellings: &["obs_threshold=2500", "obs_head=1250", "obs_tail=1250"],
            typed_paths: &[
                "result_renderer.threshold",
                "result_renderer.head",
                "result_renderer.tail",
                "values/obs_threshold",
                "values/obs_head",
                "values/obs_tail",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V5",
            level_name: "rsn-drop",
            factor_id: "transcript_render.assistant_replay.reasoning",
            spellings: &["reasoning_replay=drop"],
            typed_paths: &[
                "slots/model_port/params/transcript_render.assistant_replay.reasoning",
                "values/reasoning_replay",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V6",
            level_name: "prm-wf",
            factor_id: "instance template alternative leaf",
            spellings: &["instance_template=slim_workflow"],
            typed_paths: &[
                "Text:instance_template",
                "slots/model_port/params/instance_template",
                "values/instance_template",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V7",
            level_name: "tool-edit",
            factor_id: "tool surface",
            spellings: &["tool_set=bash_edit", "tool_desc=bash_edit"],
            typed_paths: &[
                "tools[]",
                "description_template",
                "values/tool_set",
                "values/tool_desc",
            ],
            seal: true,
        },
        P4LevelDecl {
            level_id: "V8",
            level_name: "cmp-ctr@W",
            factor_id: "compaction_strategy + declared window_cap",
            spellings: &["cmp_strategy=clear_tool_results", "window_cap=32768"],
            typed_paths: &[
                "slots/compaction_strategy",
                "window_cap",
                "values/cmp_strategy",
                "values/window_cap",
            ],
            seal: false,
        },
        P4LevelDecl {
            level_id: "V9",
            level_name: "cmp-sum@W",
            factor_id: "compaction_strategy + declared window_cap + summarizer profile",
            spellings: &["cmp_strategy=summarize_rolling@1", "window_cap=32768"],
            typed_paths: &[
                "slots/compaction_strategy",
                "window_cap",
                "summarizer_profile",
                "values/cmp_strategy",
                "values/window_cap",
            ],
            seal: false,
        },
    ]
}

/// The pass-2 declared sweep baseline — the same declared fixture
/// family as R6.43's (the first-pass mini HIR document is still
/// E2-T9's unlanded Stage 2; this doc declares the second pass's
/// sweepable parameter surface honestly): every Tier-1 factor's
/// declared parameter is sweepable and binds its typed path through
/// the `$param:` marker.
pub fn sweep_baseline_doc_p4() -> hharness_hir::document::HirDocument {
    let mut doc = sweep::sweep_baseline_doc();
    let kernel = ProvenanceRecord::kernel("sweep:p4", 0);
    let mut a = Assembly::from_json(
        doc.assembly.as_ref().expect("baseline carries assembly"),
        "/assembly",
        &kernel,
        &mut Vec::new(),
    )
    .expect("baseline assembly decodes");
    let str_enum = |domain: &[&str]| ParameterSpec {
        param_type: ParamType::Enum,
        domain: Some(Json::Arr(domain.iter().map(|s| Json::str(*s)).collect())),
        default: Some(Json::str(domain[0])),
        required: ParamRequirement::Optional,
        unit: None,
        sweepable: true,
        affects: vec![],
        budget_relevant: false,
    };
    let int_enum = |domain: &[i64]| ParameterSpec {
        param_type: ParamType::Int,
        domain: Some(Json::Arr(domain.iter().map(|v| Json::Int(*v)).collect())),
        default: Some(Json::Int(domain[0])),
        required: ParamRequirement::Optional,
        unit: None,
        sweepable: true,
        affects: vec![],
        budget_relevant: false,
    };
    // V1/V2 — the declared lane parameter of the level document
    // (REQ-XD-204's effort home), proxy-pinned per call.
    a.parameters
        .insert("effort_pin".to_string(), str_enum(&["high", "low", "max"]));
    a.values.insert("effort_pin".to_string(), Json::str("high"));
    // V3 — the declared exec timeout (R6.39's `exec_defaults` path).
    a.parameters
        .insert("cmd_timeout_ms".to_string(), int_enum(&[30_000, 120_000]));
    a.values
        .insert("cmd_timeout_ms".to_string(), Json::Int(30_000));
    // V4 — the observation renderer's three declared knobs.
    a.parameters
        .insert("obs_threshold".to_string(), int_enum(&[10_000, 2_500]));
    a.values
        .insert("obs_threshold".to_string(), Json::Int(10_000));
    a.parameters
        .insert("obs_head".to_string(), int_enum(&[5_000, 1_250]));
    a.values.insert("obs_head".to_string(), Json::Int(5_000));
    a.parameters
        .insert("obs_tail".to_string(), int_enum(&[5_000, 1_250]));
    a.values.insert("obs_tail".to_string(), Json::Int(5_000));
    // V6 — the instance template's alternative leaf.
    a.parameters.insert(
        "instance_template".to_string(),
        str_enum(&["verbatim", "slim_workflow"]),
    );
    a.values
        .insert("instance_template".to_string(), Json::str("verbatim"));
    // V8/V9 — the declared composite: strategy + window_cap. The
    // catalog window is 1 048 576; W is never below 32 768.
    a.parameters.insert(
        "cmp_strategy".to_string(),
        str_enum(&["none", "clear_tool_results", "summarize_rolling@1"]),
    );
    a.values
        .insert("cmp_strategy".to_string(), Json::str("none"));
    a.parameters.insert(
        "window_cap".to_string(),
        int_enum(&[1_048_576, 32_768, 49_152, 65_536, 98_304, 131_072]),
    );
    a.values
        .insert("window_cap".to_string(), Json::Int(1_048_576));
    // The model_port slot gains the declared lane + template leaves.
    if let Some(SlotBindings::One(mp)) = a.slots.get_mut("model_port") {
        mp.params
            .insert("effort".to_string(), Json::str("$param:effort_pin"));
        mp.params.insert(
            "instance_template".to_string(),
            Json::str("$param:instance_template"),
        );
    }
    // V7 — the tool surface, declared at `values` (the fixture has no
    // registered tool-surface class; `tool_set` + `tool_desc` carry
    // the bash → bash + edit_file delta honestly).
    a.parameters.insert(
        "tool_set".to_string(),
        str_enum(&["bash_only", "bash_edit"]),
    );
    a.values
        .insert("tool_set".to_string(), Json::str("bash_only"));
    a.parameters.insert(
        "tool_desc".to_string(),
        str_enum(&["bash_only", "bash_edit"]),
    );
    a.values
        .insert("tool_desc".to_string(), Json::str("bash_only"));
    // The compaction composite — `ordered-many` cardinality, none at
    // the catalog window (S + W is the declared delta).
    let mut cs = SlotBinding::of(ComponentVariantRef::selected(
        "compaction_strategy",
        "hh/compactor",
        "latest",
    ));
    cs.params
        .insert("strategy".to_string(), Json::str("$param:cmp_strategy"));
    cs.params
        .insert("window_cap".to_string(), Json::str("$param:window_cap"));
    a.slots.insert(
        "compaction_strategy".to_string(),
        SlotBindings::Many(vec![cs]),
    );
    doc.assembly = Some(a.to_json());
    // The root native's slot mirror rides the same bindings (a
    // disagreeing double binding is C-COMP-2).
    for n in &mut doc.nodes {
        if let KindRecord::AgentProcess(ap) = &mut n.semantic {
            if let AgentProcessBody::Native(nat) = &mut ap.body {
                nat.slots = a.slots.clone();
            }
        }
    }
    doc
}

/// The pass-2 seeded store — the first-pass classes plus
/// `tool_surface` (`hh/tool_exec`) and `compaction_strategy`
/// (`hh/compactor`).
pub fn sweep_seeded_store_p4(dir: &Path) -> RegistryStore {
    let kernel = ProvenanceRecord::kernel("sweep:registry", 0);
    let mut store = RegistryStore::open(dir, &kernel).expect("p4 sweep store opens");
    for (class_id, name) in [
        ("model_port", "openai-chat"),
        ("control_strategy", "round_robin"),
        ("context_policy", "full_window"),
        ("compaction_strategy", "compactor"),
    ] {
        let c = store
            .register(
                RegistryRecord::Class(sweep_class_record_pub(class_id)),
                &kernel,
                None,
            )
            .expect("class registers");
        let v = store
            .register(
                RegistryRecord::Variant(sweep_variant_record_pub(
                    &c.version_id,
                    &format!("hh/{name}"),
                )),
                &kernel,
                None,
            )
            .expect("variant registers");
        store
            .publish("hh", name, &v.version_id, None, None, &kernel)
            .expect("publish");
    }
    store
}

// Thin re-exports of sweep.rs's private fixture record builders —
// the same record shape, one implementation (CC1); never a second
// class/variant author.
fn sweep_class_record_pub(id: &str) -> hharness_registry::records::ClassRecord {
    sweep::class_record_for_p4(id)
}
fn sweep_variant_record_pub(
    class_ref: &str,
    variant_id: &str,
) -> hharness_registry::records::VariantRecord {
    sweep::variant_record_for_p4(class_ref, variant_id)
}

/// One sealed level plus its doc-delta evidence.
#[derive(Debug)]
pub struct P4SealedLevel {
    /// The sealed level.
    pub sealed: SealedLevel,
    /// The declared-typed-path-confined delta vs the baseline.
    pub delta: Vec<String>,
}

/// The pass-2 materialization result.
#[derive(Debug)]
pub struct P4Materialization {
    /// The frozen baseline's `version_id`.
    pub baseline_version_id: String,
    /// The sealed baseline document (canonical JSON).
    pub baseline_json: Json,
    /// The levels sealed this pass (the declared-decls with
    /// `seal: true`); withheld levels never fabricate a ref.
    pub levels: Vec<P4SealedLevel>,
}

/// `seal_p4_levels(work_dir)` — enumerate → resolve → seal over the
/// pass-2 baseline for every declarable Tier-1 level, each with its
/// declared-delta check (one registered slot, confined to declared
/// typed paths). Withheld levels (`seal: false`) are skipped honestly.
pub fn seal_p4_levels(work_dir: &Path) -> Result<P4Materialization, SweepRefusal> {
    let kernel = ProvenanceRecord::kernel("sweep:seal", 0);
    let mut store = sweep_seeded_store_p4(&work_dir.join("registry"));
    let authored = sweep_baseline_doc_p4();
    let catalog = Stage1Catalog::stage1();
    let mut env = ResolveEnv {
        registry: &mut store,
        catalog: &catalog,
        snapshot_id: None,
        mode: ResolveMode::Audit,
        registrar: kernel.clone(),
        resolved_at: 1_763_000_000_000,
        notices: None,
    };
    let base = resolve(&authored, &mut env).map_err(|d| SweepRefusal::UnexpressibleSurface {
        detail: format!("p4 baseline resolve refused: {}", d.len()),
    })?;
    let baseline_json = sealed_json(&base);
    let base_asm = baseline_json
        .get("document")
        .and_then(|d| d.get("assembly"))
        .cloned()
        .unwrap_or(Json::Null);
    let sp = space(&authored, &kernel).map_err(|e| SweepRefusal::UnexpressibleSurface {
        detail: format!("space: {e}"),
    })?;
    let mut levels = Vec::new();
    for decl in p4_level_decls().iter().filter(|d| d.seal) {
        // A one-spelling level takes enumerate's arm; a conjunction
        // of spellings (V4's renderer knobs, V7's tools + template,
        // V8/V9's S + W) takes the composite path — same
        // validate → resolve → seal machinery.
        let lvl = if decl.spellings.len() == 1 {
            materialize_level(
                &sp,
                decl.level_id,
                decl.spellings,
                &mut env,
                &catalog,
                &kernel,
            )?
        } else {
            sweep::materialize_level_composite(
                &sp,
                decl.level_id,
                decl.spellings,
                &mut env,
                &catalog,
                &kernel,
            )?
        };
        let lvl_asm = sealed_json(&lvl.sealed)
            .get("document")
            .and_then(|d| d.get("assembly"))
            .cloned()
            .unwrap_or(Json::Null);
        let delta = sweep::assembly_delta(&base_asm, &lvl_asm);
        let declared: Vec<String> = decl.typed_paths.iter().map(|s| s.to_string()).collect();
        sweep::check_doc_delta(&declared, &delta)?;
        if delta.is_empty() {
            return Err(SweepRefusal::Inert {
                factor_id: decl.factor_id.to_string(),
                branch: "document".to_string(),
            });
        }
        levels.push(P4SealedLevel { sealed: lvl, delta });
    }
    Ok(P4Materialization {
        baseline_version_id: base.definition_ref.version_id.clone(),
        baseline_json,
        levels,
    })
}

// ── the pass's `sweep-registry/1` record ─────────────────────────────────────

/// The per-level registry text — bundled to keep `lvl_entry` small.
struct LvlRow<'a> {
    baseline: &'a str,
    level: &'a str,
    exercising: &'a str,
    metric: &'a str,
    forecast: &'a str,
    cost: &'a str,
    deps: &'a [&'a str],
    upstream: &'a str,
}

fn lvl_entry(
    decl: &P4LevelDecl,
    row: &LvlRow<'_>,
    sealed_ref: Option<&str>,
    extra: Vec<(String, Json)>,
) -> Json {
    let mut m = BTreeMap::from([
        ("level_id".to_string(), Json::str(decl.level_id)),
        ("level_name".to_string(), Json::str(decl.level_name)),
        ("factor_id".to_string(), Json::str(decl.factor_id)),
        (
            "typed_paths".to_string(),
            Json::Arr(decl.typed_paths.iter().map(|s| Json::str(*s)).collect()),
        ),
        ("baseline".to_string(), Json::str(row.baseline)),
        ("level".to_string(), Json::str(row.level)),
        ("tier".to_string(), Json::Int(1)),
        (
            "exercising_condition".to_string(),
            Json::str(row.exercising),
        ),
        ("activation_metric".to_string(), Json::str(row.metric)),
        ("forecast_source".to_string(), Json::str(row.forecast)),
        ("cost_prior".to_string(), Json::str(row.cost)),
        (
            "dependency_tickets".to_string(),
            Json::Arr(row.deps.iter().map(|s| Json::str(*s)).collect()),
        ),
        ("upstream_locator".to_string(), Json::str(row.upstream)),
        ("non_upstream_constants".to_string(), Json::Arr(Vec::new())),
        (
            "override_spellings".to_string(),
            Json::Arr(decl.spellings.iter().map(|s| Json::str(*s)).collect()),
        ),
    ]);
    if let Some(r) = sealed_ref {
        m.insert("sealed_ref".to_string(), Json::str(r));
    }
    for (k, v) in extra {
        m.insert(k, v);
    }
    Json::Obj(m)
}

/// `sweep_registry_record_p4(mat, v5_entry_extra, level_extra)` —
/// the pass's registry: every Tier-1 level declared per §8.1 with its
/// sealed_ref (V8/V9 carry their `withheld` disposition — declared,
/// never sealed), Tier 2 kept verbatim with its admission verdicts.
/// `level_extra` maps level_id → additional members (`forecast`,
/// `withheld`, `declaration`, `declaration_sha256`).
pub fn sweep_registry_record_p4(
    mat: &P4Materialization,
    level_extra: &BTreeMap<String, Vec<(String, Json)>>,
    admissibility: Json,
) -> Json {
    let decls = p4_level_decls();
    let sealed_ref_of = |id: &str| -> Option<String> {
        mat.levels
            .iter()
            .find(|l| l.sealed.level_id == id)
            .map(|l| l.sealed.version_id.clone())
    };
    let extra_of =
        |id: &str| -> Vec<(String, Json)> { level_extra.get(id).cloned().unwrap_or_default() };
    let entries: Vec<(usize, Json)> = vec![
        (
            0,
            lvl_entry(
                &decls[0],
                &LvlRow {
                    baseline: "high",
                    level: "low",
                    exercising: "always",
                    metric: "fidelity: applied = declared on 100% of calls",
                    forecast: "non-outcome fields: per-call reasoning tokens",
                    cost: "↓",
                    deps: &["proxy pin", "effort home (Q-X44 / AR-26)"],
                    upstream: "upstream:hf-mini-swe-agent",
                },
                sealed_ref_of("V1").as_deref(),
                extra_of("V1"),
            ),
        ),
        (
            1,
            lvl_entry(
                &decls[1],
                &LvlRow {
                    baseline: "high",
                    level: "max",
                    exercising: "always",
                    metric: "fidelity: applied = declared on 100% of calls",
                    forecast: "non-outcome fields: per-call reasoning tokens",
                    cost: "↑ ≈ ×1.3 per trial",
                    deps: &["proxy pin", "effort home (Q-X44 / AR-26)"],
                    upstream: "upstream:hf-mini-swe-agent",
                },
                sealed_ref_of("V2").as_deref(),
                extra_of("V2"),
            ),
        ),
        (
            2,
            lvl_entry(
                &decls[2],
                &LvlRow {
                    baseline: "30000",
                    level: "120000",
                    exercising: "conditional: ≥ 1 command > 30 s",
                    metric: "share of first subjects with ≥ 1 command > 30 s",
                    forecast: "non-outcome fields: per-command durations",
                    cost: "wall ↑",
                    deps: &["E2-T6"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V3").as_deref(),
                extra_of("V3"),
            ),
        ),
        (
            3,
            lvl_entry(
                &decls[3],
                &LvlRow {
                    baseline: "10000/5000/5000",
                    level: "2500/1250/1250",
                    exercising: "conditional: counterfactual rendering difference",
                    metric: "exact counterfactual rendering difference",
                    forecast: "non-outcome fields: raw observation lengths",
                    cost: "↓ input tokens",
                    deps: &["E2-T4"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V4").as_deref(),
                extra_of("V4"),
            ),
        ),
        (
            4,
            lvl_entry(
                &decls[4],
                &LvlRow {
                    baseline: "verbatim",
                    level: "drop",
                    exercising: "always after turn 1 (reasoning always present)",
                    metric: "share of requests with a dropped non-empty reasoning member",
                    forecast: "non-outcome fields: reasoning presence on upstream-bound requests",
                    cost: "↓ input tokens",
                    deps: &["E2-T1"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V5").as_deref(),
                extra_of("V5"),
            ),
        ),
        (
            5,
            lvl_entry(
                &decls[5],
                &LvlRow {
                    baseline: "verbatim",
                    level: "workflow guidance removed, submit instruction and command rules kept",
                    exercising: "always (wire-visible)",
                    metric: "wire-visible",
                    forecast: "non-outcome fields: request system/user lengths",
                    cost: "≈ 0",
                    deps: &["E2-T3"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V6").as_deref(),
                extra_of("V6"),
            ),
        ),
        (
            6,
            lvl_entry(
                &decls[6],
                &LvlRow {
                    baseline: "bash only",
                    level: "bash + edit_file",
                    exercising: "wire-visible; behavioural ≥ 1 edit_file call",
                    metric: "share of first subjects with ≥ 1 edit_file call",
                    forecast: "non-outcome fields: request tool schema",
                    cost: "small ↑ (schema tokens)",
                    deps: &["edit_file capability (Q-X44: withheld unless funded)"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V7").as_deref(),
                extra_of("V7"),
            ),
        ),
        (
            7,
            lvl_entry(
                &decls[7],
                &LvlRow {
                    baseline: "none @ catalog window",
                    level: "clear_tool_results @ W",
                    exercising: "conditional under W",
                    metric: "§4.2 activation counts under W",
                    forecast: "non-outcome fields: per-call upstream input tokens",
                    cost: "↓ on long trajectories",
                    deps: &["E2-T7"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V8").as_deref(),
                extra_of("V8"),
            ),
        ),
        (
            8,
            lvl_entry(
                &decls[8],
                &LvlRow {
                    baseline: "none",
                    level: "hh/summarize-rolling@1 @ W",
                    exercising: "conditional under W",
                    metric: "§4.2 activation counts under W, plus compaction-role call overhead",
                    forecast: "non-outcome fields: per-call upstream input tokens",
                    cost: "±",
                    deps: &["E2-T7", "summarizer profile on the proxy lane"],
                    upstream: "HH-only",
                },
                sealed_ref_of("V9").as_deref(),
                extra_of("V9"),
            ),
        ),
    ];
    let mut levels: Vec<Json> = entries.into_iter().map(|(_, j)| j).collect();
    levels.shrink_to_fit();
    Json::obj([
        ("schema", Json::str(sweep::SWEEP_REGISTRY_SCHEMA)),
        ("label", Json::str(sweep::SWEEP_VARIANT_LABEL)),
        (
            "parity",
            Json::str(
                "none — a swept variant carries no parity claim against the original (REQ-XE-125)",
            ),
        ),
        (
            "note",
            Json::str(
                "second-pass scope (R7.1): V1–V9 + Tier 2 declared; V1–V7 sealed through R6.43's enumerate→seal; V8/V9 declared with their W-rule disposition — a level changed after this freeze is a new identity (REQ-XD-202)",
            ),
        ),
        ("recorded_at", Json::str("2026-11-20")),
        (
            "baseline",
            Json::obj([
                ("document", Json::str("declared sweep baseline (fixture, pass 2)")),
                (
                    "note",
                    Json::str(
                        "the production mini HIR document is E2-T9's unlanded Stage 2 — the registry's levels declare the typed paths; every sealed level derives from this declared baseline",
                    ),
                ),
            ]),
        ),
        ("admissibility", admissibility),
        ("levels", Json::Arr(levels)),
        (
            "tier2",
            Json::Arr(vec![
                sweep::t2_for_p4(
                    "step-limit",
                    "Budget.dimensions.model_calls.hard ∞ → 50",
                    "HF mean is 31.8 calls/trial — a cap of 50 binds only the tail; mostly a cost lever",
                    "a cost-oriented later exploration, once an admitted cost method exists",
                ),
                sweep::t2_for_p4(
                    "format-error-cap",
                    "format-error cap 3 → 1 / 5",
                    "exercised only by streaks, probably rare with native tool calls",
                    "admit if the phase-2 pilot's mini receipts show a streak rate ≥ the activation floor",
                ),
                sweep::t2_for_p4(
                    "truncation-off",
                    "observation truncation off / 20 000",
                    "exercised only by outputs > 10 000 chars — rarer than > 2 500",
                    "as above, using the observation-length census",
                ),
                sweep::t2_for_p4(
                    "context-policy",
                    "context policy (recency_plus_pins, priority_eviction)",
                    "probably inert on the wire today — the transcript reads the compaction rows, not the assembled plan (E2 F13)",
                    "after G21 lands; the REQ-XE-125 liveness check must pass",
                ),
                sweep::t2_for_p4(
                    "interaction-mode-shim",
                    "interaction mode shim (text actions)",
                    "needs G14, part of Terminus-2 stage two (E2-T12; Q-X42)",
                    "after the stage-two block",
                ),
                sweep::t2_for_p4(
                    "explicit-temperature",
                    "explicit temperature",
                    "baseline sends none; provider default and honoured-under-thinking unknown; explicit 1.0 may be distributionally inert while wire-visible",
                    "after the S0 smoke (Q-X23) records acceptance and a stated rationale",
                ),
                sweep::t2_for_p4(
                    "cost-limit",
                    "cost limit (Budget.spend.hard)",
                    "duplicates the proxy's per-trial ceiling",
                    "not proposed",
                ),
            ]),
        ),
    ])
}

/// The id sets this pass's validator shares with R6.43's.
pub fn p4_id_sets() -> (BTreeSet<String>, BTreeSet<String>) {
    (
        TIER1_LEVEL_IDS.iter().map(|s| s.to_string()).collect(),
        TIER2_CANDIDATES.iter().map(|s| s.to_string()).collect(),
    )
}

/// `registry_hash_for_prereg(reg)` — the frozen hash R7.2's
/// exploration pre-registration cites (§8.7 order: the registry hash
/// enters the pre-registration before X0 subjects).
pub fn registry_hash_for_prereg(frozen: &Json) -> Option<String> {
    frozen
        .get("frozen_sha256")
        .and_then(Json::as_str)
        .map(str::to_string)
}

/// The derivation/admissibility digest of a committed record set —
/// the emitter's audit member (record sha256s, canonical bytes).
pub fn record_sha256(rec: &Json) -> String {
    sha256_hex(rec.to_canonical_string().as_bytes())
}
