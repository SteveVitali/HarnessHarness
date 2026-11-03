//! `recompute` — the §6.3 **Recompute** recipient procedure through the
//! native owner (`hh-analysis`; decomposition row 26 — the R5.15
//! population contract and R5.23 receipt are consumed, never
//! re-derived). Records-in/records-out like [`assemble_receipt`]: the
//! caller supplies the frozen evidence side (`RecomputePlan`) plus the
//! rebuilt [`AnalysisInput`]; nothing is read from a producer store and
//! no `hh recompute` command exists (§6.3 — printed recipes stay
//! `not_run{recipe_unimplemented}` until their own owner admits them).
//!
//! The legs are the §6.3 row verbatim: `verify_inputs` (withheld
//! members refuse; pin drift refuses — no rerun under altered
//! selector/price/weight/family/draw pins; `generated_from` and
//! `report_id` recompute exactly) → `population` (frozen grade
//! selection + the all-planned reconcile through
//! [`assemble_receipt`]; non-`closed` is a verify-first refusal, never
//! a compare drift) → `rerun` ([`analyze`]; kernel refusals propagate
//! verbatim — one vocabulary) → `compare` (identities,
//! membership/order, counts/missingness and categorical verdicts
//! exact; estimate scalars ride the frozen tolerance
//! `abs(new−reference) ≤ 1e−10 × max(1,abs(reference))` at full
//! precision before formatting; a threshold/verdict change fails even
//! inside tolerance). `reproduced` only when every leg passed and the
//! drift set is empty — arithmetic agreement never validates hidden
//! history/oracle and confers no R level (§6.3).

use hharness_experiment::view::ExperimentView;
use hharness_lab::analysis::AnalysisSpec;
use hharness_wire::Json;

use crate::error::AnalysisError;
use crate::kernel::{analyze, project_runs, AnalysisInput};
use crate::receipt::{assemble_receipt, AnalysisDecl, DeclaredReceipt};
use crate::report::{generated_from_for, report_id_for};

/// The derived recompute record's schema label — evidence about a
/// procedure (like `closure-report/1`), never a level or a store.
pub const RECOMPUTE_SCHEMA: &str = "analysis-recompute/1";

/// The portable scalar tolerance — frozen before execution (§6.3): a
/// constant, never a parameter — no wider tolerance, no recipe-side
/// formula fork (AC-2).
const SCALAR_TOL_REL: f64 = 1e-10;

/// The frozen rule over raw values, never formatted strings. The
/// admitted wire encoding is integer-scaled — at ppm magnitudes any
/// unequal integers are outside tolerance; the rule binds at the
/// extremes it was admitted for.
pub fn within_tolerance(new: i64, reference: i64) -> bool {
    (new as f64 - reference as f64).abs() <= SCALAR_TOL_REL * (reference as f64).abs().max(1.0)
}

/// The closed exact-compare member set — pins, counts, ordinals,
/// thresholds, declared levels: a member inside it never rides the
/// tolerance (a threshold change fails even inside tolerance — §6.3).
/// Estimate members (`*_ppm` posteriors, interval endpoints,
/// error/delta records) are outside it by name.
fn exact_member(name: &str) -> bool {
    name.ends_with("_count")
        || name.ends_with("_size")
        || matches!(
            name,
            "seed"
                | "draws"
                | "requested"
                | "used"
                | "level"
                | "level_ppm"
                | "tol_ppm"
                | "confidence_ppm"
                | "tail_ppm"
                | "c"
                | "n"
                | "s"
                | "f"
                | "u"
                | "a"
                | "b"
                | "count"
                | "ordinal"
                | "replicate_index"
                | "attempt_no"
                | "activation_no"
                | "n_rows"
                | "n_tasks"
                | "family_size"
        )
}

/// The verify-stage typed refusals (CC9 — the boundary renders
/// `AnalysisError::Recompute` verbatim).
#[derive(Debug, Clone, PartialEq)]
pub enum RecomputeError {
    /// A required declared pin or the reference body is absent —
    /// missing inputs are unavailable/refused, never implied (§6.3).
    Withheld { member: String },
    /// The reference is present but is not a decodable
    /// `analysis_report_body/1`.
    MalformedReference { detail: String },
    /// A pin member that must be an integer is not — a nonfinite
    /// input refuses.
    NonFinite { member: String },
    /// A declared pin disagrees with the frozen reference or the
    /// rebuilt input — wrong selector/price/weight/family/draw; the
    /// rerun never proceeds under altered pins. `declared` is the
    /// asserted pin value, `derived` the counterpart it was checked
    /// against.
    WrongPin {
        member: String,
        declared: String,
        derived: String,
    },
    /// The declared receipt did not reconcile — frozen selection /
    /// planned-slot closure is a verify-first predicate, not a drift.
    SelectionNotClosed { codes: Vec<String> },
    /// `population.declared.analysis` and `plan.declared` name two
    /// different analysis declarations.
    DeclaredAnalysisDrift { detail: String },
}

impl std::fmt::Display for RecomputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Withheld { member } => write!(f, "withheld recompute input: {member}"),
            Self::MalformedReference { detail } => write!(f, "malformed reference body: {detail}"),
            Self::NonFinite { member } => {
                write!(f, "nonfinite recompute pin: {member} must be an integer")
            }
            Self::WrongPin {
                member,
                declared,
                derived,
            } => {
                write!(
                    f,
                    "wrong recompute pin {member}: declared {declared} vs rebuilt {derived}"
                )
            }
            Self::SelectionNotClosed { codes } => {
                write!(
                    f,
                    "frozen selection/population not closed: {}",
                    codes.join(", ")
                )
            }
            Self::DeclaredAnalysisDrift { detail } => {
                write!(f, "declared analysis drift: {detail}")
            }
        }
    }
}

impl std::error::Error for RecomputeError {}

/// One compare-stage drift row — `kind` ∈ `identity | order | count |
/// verdict | threshold | scalar`; `declared`/`derived` are canonical
/// renderings of the frozen and recomputed members.
#[derive(Debug, Clone, PartialEq)]
pub struct RecomputeDrift {
    /// The member path (`finite_manifest_contrast.interval.level_ppm`).
    pub member: String,
    /// The drift class.
    pub kind: &'static str,
    /// The frozen reference member.
    pub declared: String,
    /// The recomputed member.
    pub derived: String,
}

/// One procedure leg's record (`verify_inputs` | `population` |
/// `rerun` | `compare`; `verdict` ∈ `pass | closed | not_supplied |
/// identical | drifted`).
#[derive(Debug, Clone, PartialEq)]
pub struct RecomputeLeg {
    /// The leg name.
    pub name: &'static str,
    /// The leg verdict.
    pub verdict: &'static str,
    /// Leg detail.
    pub detail: Vec<String>,
}

/// The recompute report — `reproduced` only when every leg passed and
/// no drift recorded. Agreement is arithmetic, never a hidden-history
/// or R-level claim.
#[derive(Debug, Clone)]
pub struct RecomputeReport {
    /// The four legs in procedure order.
    pub legs: Vec<RecomputeLeg>,
    /// Every recorded drift (empty under `reproduced`).
    pub drifts: Vec<RecomputeDrift>,
    /// `reproduced | not_reproduced`.
    pub verdict: &'static str,
    /// The recomputed report id.
    pub report_id: String,
    /// The frozen reference's report id.
    pub reference_report_id: String,
    /// The recomputed `analysis_report_body/1` (the recipient's own
    /// produced record).
    pub body: Json,
}

impl RecomputeReport {
    /// Canonical JSON — the `analysis-recompute/1` derived record.
    pub fn to_json(&self) -> Json {
        let legs = self.legs.iter().map(|l| {
            Json::obj([
                ("name", Json::str(l.name)),
                ("verdict", Json::str(l.verdict)),
                (
                    "detail",
                    Json::Arr(l.detail.iter().map(Json::str).collect()),
                ),
            ])
        });
        let drifts = self.drifts.iter().map(|d| {
            Json::obj([
                ("member", Json::str(&d.member)),
                ("kind", Json::str(d.kind)),
                ("declared", Json::str(&d.declared)),
                ("derived", Json::str(&d.derived)),
            ])
        });
        Json::obj([
            ("schema", Json::str(RECOMPUTE_SCHEMA)),
            ("legs", Json::Arr(legs.collect())),
            ("drifts", Json::Arr(drifts.collect())),
            ("verdict", Json::str(self.verdict)),
            ("report_id", Json::str(&self.report_id)),
            ("reference_report_id", Json::str(&self.reference_report_id)),
            ("body", self.body.clone()),
        ])
    }
}

/// The frozen population/selection side — when supplied, the
/// all-planned reconcile runs before the rerun over runs/tasks/pins
/// folded from the same rebuilt input (never a second run set).
#[derive(Debug)]
pub struct PopulationPins<'a> {
    /// The bound experiment's folded view.
    pub view: &'a ExperimentView,
    /// The binary metric ref the population fold reads.
    pub metric_ref: &'a str,
    /// The declared receipt (`analysis` must equal `plan.declared`).
    pub declared: &'a DeclaredReceipt,
}

/// The frozen recompute plan — the evidence side the rebuilt input is
/// verified against (records-in).
#[derive(Debug)]
pub struct RecomputePlan<'a> {
    /// The pinned `AnalysisSpec` (content-addressed through
    /// `declared.spec_ref`).
    pub spec: &'a AnalysisSpec,
    /// The declared analysis pins (`DeclaredReceipt::analysis`).
    pub declared: &'a AnalysisDecl,
    /// The frozen `analysis_report_body/1` — `None`/`Null` is withheld.
    pub reference: Option<&'a Json>,
    /// The frozen selection/population pins (`None` records
    /// `not_supplied`; the run-set identity still binds through
    /// `generated_from`).
    pub population: Option<PopulationPins<'a>>,
}

/// The leaf name of a member path (`a.b[3].c` → `c`).
fn leaf(path: &str) -> &str {
    path.rsplit(['.', '[', ']'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
}

fn render(j: &Json) -> String {
    j.to_canonical_string()
}

fn drift(out: &mut Vec<RecomputeDrift>, member: String, kind: &'static str, d: &Json, n: &Json) {
    out.push(RecomputeDrift {
        member,
        kind,
        declared: render(d),
        derived: render(n),
    });
}

/// The recursive body walk — §6.3's comparison rules verbatim.
fn compare_json(path: &str, reference: &Json, derived: &Json, out: &mut Vec<RecomputeDrift>) {
    match (reference, derived) {
        (Json::Obj(a), Json::Obj(b)) => {
            for (k, v) in a {
                match b.get(k) {
                    None => drift(out, format!("{path}.{k}"), "identity", v, &Json::Null),
                    Some(dv) => compare_json(&format!("{path}.{k}"), v, dv, out),
                }
            }
            for (k, v) in b {
                if !a.contains_key(k) {
                    drift(out, format!("{path}.{k}"), "identity", &Json::Null, v);
                }
            }
        }
        (Json::Arr(a), Json::Arr(b)) => {
            if a.len() != b.len() {
                drift(
                    out,
                    path.into(),
                    "count",
                    &Json::Int(a.len() as i64),
                    &Json::Int(b.len() as i64),
                );
            } else if a != b {
                // Membership-equal but reordered is an `order` drift —
                // never silently re-sorted into agreement (§6.3).
                let mut sa: Vec<String> = a.iter().map(render).collect();
                let mut sb: Vec<String> = b.iter().map(render).collect();
                sa.sort();
                sb.sort();
                if sa == sb {
                    drift(out, path.into(), "order", reference, derived);
                    return;
                }
            }
            for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                compare_json(&format!("{path}[{i}]"), x, y, out);
            }
        }
        (Json::Int(x), Json::Int(y)) => {
            if x != y && exact_member(leaf(path)) {
                drift(out, path.into(), "threshold", reference, derived);
            } else if x != y && !within_tolerance(*y, *x) {
                drift(out, path.into(), "scalar", reference, derived);
            }
        }
        (Json::Str(_), Json::Str(_))
        | (Json::Bool(_), Json::Bool(_))
        | (Json::Null, Json::Null) => {
            if reference != derived {
                drift(out, path.into(), "verdict", reference, derived);
            }
        }
        _ => drift(out, path.into(), "identity", reference, derived),
    }
}

/// `presented ⊆ expected` member-wise for the pin stage — every member
/// of `presented` must resolve and equal inside `expected`.
fn pin_equal(prefix: &str, presented: &Json, expected: &Json) -> Result<(), RecomputeError> {
    if let Json::Obj(pm) = presented {
        for (k, v) in pm {
            let member = format!("{prefix}.{k}");
            match expected.get(k) {
                None => return Err(RecomputeError::Withheld { member }),
                Some(ev) => pin_equal(&member, v, ev)?,
            }
        }
        return Ok(());
    }
    if presented == expected {
        return Ok(());
    }
    Err(RecomputeError::WrongPin {
        member: prefix.into(),
        declared: render(expected),
        derived: render(presented),
    })
}

/// The frozen weights member — `weights` (object) then `weights_ref`
/// (string); `None` when the kind section carries neither.
fn frozen_weights(section: Option<&Json>) -> Option<Json> {
    section?
        .get("weights")
        .cloned()
        .or_else(|| section?.get("weights_ref").cloned())
}

/// `decl.weights` equals the frozen member under the admitted
/// spellings (bare value, `{weights_ref|rule|weights: <value>}`).
fn weights_equal(declared: &Json, frozen: &Json) -> bool {
    if declared == frozen {
        return true;
    }
    if let Json::Obj(dm) = declared {
        return ["weights_ref", "rule", "weights"]
            .iter()
            .filter_map(|k| dm.get(*k))
            .any(|v| v == frozen);
    }
    false
}

/// Leg 1 — `verify_inputs`: completeness, then pin equality against
/// the frozen reference and the rebuilt input.
fn verify_inputs<'p>(
    plan: &'p RecomputePlan<'_>,
    input: &AnalysisInput<'_>,
) -> Result<&'p Json, AnalysisError> {
    let decl = plan.declared;
    let withheld = |member: &str| -> AnalysisError {
        RecomputeError::Withheld {
            member: member.into(),
        }
        .into()
    };
    let wrong = |member: &str, declared: String, derived: String| -> AnalysisError {
        RecomputeError::WrongPin {
            member: member.into(),
            declared,
            derived,
        }
        .into()
    };
    let reference = plan
        .reference
        .filter(|r| !matches!(r, Json::Null))
        .ok_or_else(|| withheld("reference_body"))?;
    if reference.get("schema").and_then(Json::as_str) != Some("analysis_report_body/1") {
        return Err(RecomputeError::MalformedReference {
            detail: "schema is not analysis_report_body/1".into(),
        }
        .into());
    }
    for member in ["generated_from", "report_id", "kind"] {
        if reference.get(member).is_none() {
            return Err(RecomputeError::MalformedReference {
                detail: format!("missing required member {member}"),
            }
            .into());
        }
    }
    let ref_gf = reference.get("generated_from").expect("checked");
    let ref_id = reference
        .get("report_id")
        .and_then(Json::as_str)
        .expect("checked");

    // Declared completeness — the receipt's MISSING_PIN members plus
    // the spec ref the selector binds through.
    if decl.prices == Json::Null {
        return Err(withheld("prices"));
    }
    if decl.method == Json::Null {
        return Err(withheld("method"));
    }
    if decl.weights == Json::Null {
        return Err(withheld("weights"));
    }
    let spec_ref = decl.spec_ref.as_ref().ok_or_else(|| withheld("spec_ref"))?;

    // Selector / kind identity (exact — the spec is content-addressed).
    if *spec_ref != plan.spec.spec_id() {
        return Err(wrong("spec_ref", spec_ref.clone(), plan.spec.spec_id()));
    }
    let ref_kind = reference
        .get("kind")
        .and_then(Json::as_str)
        .expect("checked");
    if ref_kind != plan.spec.kind {
        return Err(wrong("kind", ref_kind.into(), plan.spec.kind.clone()));
    }

    // Prices — each declared member against the rebuilt input.
    let prices = match &decl.prices {
        Json::Obj(m) => m,
        _ => {
            return Err(RecomputeError::MalformedReference {
                detail: "decl.prices is not an object".into(),
            }
            .into())
        }
    };
    let ptv = prices
        .get("price_table_version")
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| withheld("prices.price_table_version"))?;
    if ptv != input.price_table_version {
        return Err(wrong(
            "price_table_version",
            ptv.into(),
            input.price_table_version.into(),
        ));
    }
    if let Some(v) = prices.get("confidence_ppm") {
        let v = v.as_int().ok_or(RecomputeError::NonFinite {
            member: "prices.confidence_ppm".into(),
        })?;
        if v != input.confidence_ppm {
            return Err(wrong(
                "confidence_ppm",
                v.to_string(),
                input.confidence_ppm.to_string(),
            ));
        }
    }
    if let Some(v) = prices.get("metric_registry_version") {
        let v = v.as_str().ok_or(RecomputeError::NonFinite {
            member: "prices.metric_registry_version".into(),
        })?;
        if v != input.metric_registry_version {
            return Err(wrong(
                "metric_registry_version",
                v.into(),
                input.metric_registry_version.into(),
            ));
        }
    }

    // Method — the declared record's required members present, then
    // each against the frozen kind section's `metadata` member-wise
    // (estimator/version/prior/keys: the family pins).
    let section = reference.get(&plan.spec.kind);
    let metadata = section
        .and_then(|s| s.get("metadata"))
        .ok_or_else(|| withheld(&format!("{}.metadata", plan.spec.kind)))?;
    for m in ["estimator", "version", "keys"] {
        let absent = decl
            .method
            .get(m)
            .map(|v| v.as_str().map(str::is_empty).unwrap_or(false))
            .unwrap_or(true);
        if absent {
            return Err(withheld(&format!("method.{m}")));
        }
    }
    pin_equal("method", &decl.method, metadata).map_err(AnalysisError::from)?;

    // Weights — the frozen section member under admitted spellings.
    let frozen = frozen_weights(section).ok_or_else(|| withheld("weights"))?;
    if !weights_equal(&decl.weights, &frozen) {
        return Err(wrong("weights", render(&decl.weights), render(&frozen)));
    }

    // Report binding + draw pins.
    if let Some(r) = &decl.report_ref {
        if r != ref_id {
            return Err(wrong("report_ref", r.clone(), ref_id.into()));
        }
    }
    if decl.seed != input.seed {
        return Err(wrong("seed", decl.seed.to_string(), input.seed.to_string()));
    }
    if decl.draws != input.resampling_draws {
        return Err(wrong(
            "draws",
            decl.draws.to_string(),
            input.resampling_draws.to_string(),
        ));
    }

    // Input-set identity, then the content-addressed report id — the
    // symmetric compare walk (declared = frozen, derived = rebuilt);
    // the first drift is the refused pin.
    let gf = generated_from_for(input);
    let mut d = Vec::new();
    compare_json("generated_from", ref_gf, &gf, &mut d);
    if let Some(x) = d.into_iter().next() {
        return Err(wrong(&x.member, x.declared, x.derived));
    }
    let recomputed = report_id_for(plan.spec, &gf);
    if recomputed != ref_id {
        return Err(wrong("report_id", ref_id.into(), recomputed));
    }
    Ok(reference)
}

/// `recompute_analysis(plan, input)` — the §6.3 Recompute procedure:
/// verify inputs, reconcile the frozen selection/population, rerun the
/// pinned analysis through the one kernel, compare under the frozen
/// exact/tolerance rules.
pub fn recompute_analysis(
    plan: &RecomputePlan<'_>,
    input: &AnalysisInput<'_>,
) -> Result<RecomputeReport, AnalysisError> {
    let mut legs: Vec<RecomputeLeg> = Vec::new();
    let reference = verify_inputs(plan, input)?;
    legs.push(RecomputeLeg {
        name: "verify_inputs",
        verdict: "pass",
        detail: Vec::new(),
    });

    match &plan.population {
        Some(pop) => {
            if pop.declared.analysis != *plan.declared {
                return Err(RecomputeError::DeclaredAnalysisDrift {
                    detail: "population.declared.analysis != plan.declared".into(),
                }
                .into());
            }
            let (runs, _not_run) = project_runs(input)?;
            let receipt = assemble_receipt(
                pop.view,
                &runs,
                input.tasks,
                pop.metric_ref,
                &input.watermark_set.runs,
                pop.declared,
            )?;
            if receipt.verdict != "closed" {
                return Err(RecomputeError::SelectionNotClosed {
                    codes: receipt
                        .failures
                        .iter()
                        .map(|f| f.code.to_string())
                        .collect(),
                }
                .into());
            }
            legs.push(RecomputeLeg {
                name: "population",
                verdict: "closed",
                detail: Vec::new(),
            });
        }
        None => legs.push(RecomputeLeg {
            name: "population",
            verdict: "not_supplied",
            detail: Vec::new(),
        }),
    }

    let outcome = analyze(plan.spec, input)?;
    legs.push(RecomputeLeg {
        name: "rerun",
        verdict: "pass",
        detail: Vec::new(),
    });

    let mut drifts: Vec<RecomputeDrift> = Vec::new();
    compare_json("", reference, &outcome.body, &mut drifts);
    legs.push(RecomputeLeg {
        name: "compare",
        verdict: if drifts.is_empty() {
            "identical"
        } else {
            "drifted"
        },
        detail: vec![format!("{} drift(s)", drifts.len())],
    });

    let verdict = if drifts.is_empty() {
        "reproduced"
    } else {
        "not_reproduced"
    };
    Ok(RecomputeReport {
        legs,
        drifts,
        verdict,
        report_id: outcome.report_id,
        reference_report_id: reference
            .get("report_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .into(),
        body: outcome.body,
    })
}
