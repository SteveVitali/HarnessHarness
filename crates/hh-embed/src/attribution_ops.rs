//! `lab.attribution.*` — the designed causal-attribution boundary
//! (S6.3b; §5h.7; R-2.9.7 6c): records-in/records-out like
//! `lab.debt.*`. `design` validates + content-addresses an
//! `hh-attribution-design/1`; `attribute` desugars it into
//! `hh_analysis::attribution`'s arm plan and folds caller-supplied
//! `ArmOutcome` records into `AttributionReport/1` — with
//! `open_arms: true` every distinct `(target, fork_point)` cell is
//! *first* opened through the real Group W `counterfactual` path
//! (instrument-charged `branch_kind = counterfactual` branches, the
//! `noise_coupling`/`target` members stamped — the "executes through
//! Group W" leg, AC-R-2.9.7-1). `locus` and `quality` are the report
//! projections. The estimator stays pure; the typed refusals render as
//! `Refused{reason: <code>}` (T-LCD-14).

use hh_embed_schema::errors::EmbedError;
use hh_wire::json::Json;

use crate::service::EmbedService;

fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

fn attr_err(e: hh_analysis::attribution::AttributionError) -> EmbedError {
    match e {
        hh_analysis::attribution::AttributionError::Schema(d) => EmbedError::SchemaViolation {
            path: "/attribution".to_string(),
            code: d,
        },
        hh_analysis::attribution::AttributionError::UnbudgetedArm => EmbedError::UnbudgetedArm,
        other => EmbedError::Refused {
            reason: other.code().to_string(),
        },
    }
}

impl EmbedService {
    /// `lab.attribution.design{design, n_fork_points?}` →
    /// `{design_ref, design, estimate_rollouts, plan}` — validate +
    /// content-address the `hh-attribution-design/1` and return the
    /// scheduler's rollout price *before* any reservation (§05e;
    /// AC-R-2.9.7-14).
    pub(crate) fn lab_attribution_design(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let design =
            hh_analysis::attribution::AttributionDesign::from_json(req(params, "design")?)
                .map_err(attr_err)?;
        hh_analysis::attribution::validate_design(&design).map_err(attr_err)?;
        let n_fp = params
            .get("n_fork_points")
            .and_then(Json::as_int)
            .unwrap_or(1) as usize;
        let mut dj = design.to_json();
        let design_ref = hh_identity::idp_id(
            "attribution.design",
            dj.to_canonical_string().as_bytes(),
        );
        if let Json::Obj(m) = &mut dj {
            m.insert("design_ref".into(), Json::str(&design_ref));
        }
        let design = hh_analysis::attribution::AttributionDesign::from_json(&dj)
            .map_err(attr_err)?;
        let fps: Vec<i64> = params
            .get("fork_points")
            .and_then(|f| match f {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|v| v.as_int())
                        .collect::<Vec<i64>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let plan = hh_analysis::attribution::attribution_plan(&design, &fps)
            .map_err(attr_err)?;
        Ok(Json::obj([
            ("design_ref", Json::str(&design_ref)),
            ("design", dj),
            (
                "estimate_rollouts",
                Json::Int(hh_analysis::attribution::estimate_rollouts(
                    &design, n_fp,
                )),
            ),
            (
                "plan",
                Json::Arr(plan.iter().map(|c| c.to_json()).collect()),
            ),
        ]))
    }

    /// `lab.attribution.attribute{session_id?, design, fork_points[]?,
    /// outcomes[], open_arms?, run_id?}` → `{report}` — the M2–M5 fold:
    /// plan the arms, (with `open_arms`) open each `(target,
    /// fork_point)` through the real `counterfactual` op — the
    /// instrument-charged Group W legs — then fold the supplied
    /// `ArmOutcome` records. Unexecuted cells report `n.not_run`
    /// honestly (V6's cousin — never a fabricated outcome).
    pub(crate) fn lab_attribution_attribute(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let design =
            hh_analysis::attribution::AttributionDesign::from_json(req(params, "design")?)
                .map_err(attr_err)?;
        let fps: Vec<i64> = params
            .get("fork_points")
            .and_then(|f| match f {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|v| v.as_int())
                        .collect::<Vec<i64>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let plan = hh_analysis::attribution::attribution_plan(&design, &fps)
            .map_err(attr_err)?;
        // `open_arms` — the real Group W execution half: one
        // `counterfactual` call per distinct (target, fork_point) with
        // the design's replicate count and noise coupling; the arm
        // records land in the report's `arms` member.
        let open_arms = params
            .get("open_arms")
            .and_then(|j| match j {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);
        let mut arm_records: Vec<Json> = Vec::new();
        if open_arms {
            let session_id = params
                .get("session_id")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("/session_id", "required_for_open_arms"))?;
            use std::collections::BTreeSet;
            // One `counterfactual` call per distinct
            // (target, fork_point, intervention_kind ≠ do_resample) — the
            // factual arm rides inside every call; M5's `direct` arm is
            // the second call's counterfactual.
            let mut seen: BTreeSet<(String, i64, String)> = BTreeSet::new();
            for cell in plan
                .iter()
                .filter(|c| c.intervention_kind != "do_resample")
            {
                if !seen.insert((
                    cell.target.clone(),
                    cell.fork_point,
                    cell.intervention_kind.clone(),
                )) {
                    continue;
                }
                let intervention = Json::obj([
                    ("kind", Json::str(cell.intervention_kind.clone())),
                    ("target", Json::str(cell.target.clone())),
                    ("earliest_affected_seq", Json::Int(cell.fork_point)),
                    (
                        "coupling_assumption",
                        Json::str(design.coupling_assumption.clone()),
                    ),
                    ("hypothesis", Json::str("attribution")),
                ]);
                let mut cf = vec![
                    ("session_id", Json::str(session_id)),
                    ("at_seq", Json::Int(cell.fork_point)),
                    ("intervention", intervention),
                    (
                        "design",
                        Json::obj([
                            ("n_seeds", Json::Int(design.k as i64)),
                            ("k", Json::Int(design.k as i64)),
                            ("budget", design.match_spec.clone()),
                            ("factual_arm", Json::Bool(true)),
                            (
                                "noise_coupling",
                                Json::str(design.noise_coupling.clone()),
                            ),
                            ("target", Json::str(cell.target.clone())),
                        ]),
                    ),
                ];
                if let Some(r) = params.get("run_id") {
                    cf.push(("run_id", r.clone()));
                }
                cf.push((
                    "env",
                    params
                        .get("env")
                        .cloned()
                        .unwrap_or_else(|| Json::str("trace_only")),
                ));
                let opened = self.counterfactual(&Json::obj(cf));
                match opened {
                    Ok(r) => arm_records.push(Json::obj([
                        ("target", Json::str(cell.target.clone())),
                        ("fork_point", Json::Int(cell.fork_point)),
                        (
                            "intervention_kind",
                            Json::str(cell.intervention_kind.clone()),
                        ),
                        ("result", r),
                    ])),
                    Err(e) => {
                        arm_records.push(Json::obj([
                            ("target", Json::str(cell.target.clone())),
                            ("fork_point", Json::Int(cell.fork_point)),
                            (
                                "refused",
                                Json::str(format!("{e:?}")),
                            ),
                        ]));
                    }
                }
            }
        }
        let outcomes: Vec<hh_analysis::attribution::ArmOutcome> = match req(params, "outcomes")? {
            Json::Arr(a) => a
                .iter()
                .map(hh_analysis::attribution::ArmOutcome::from_json)
                .collect::<Result<_, _>>()
                .map_err(attr_err)?,
            _ => return Err(bad("/outcomes", "type_mismatch")),
        };
        let report = hh_analysis::attribution::attribute(&design, &outcomes, &plan)
            .map_err(attr_err)?;
        let mut out = Json::obj([("report", report)]);
        if !arm_records.is_empty() {
            if let Json::Obj(m) = &mut out {
                m.insert("arms".into(), Json::Arr(arm_records));
            }
        }
        Ok(out)
    }

    /// `lab.attribution.locus{report}` → `{locus}` — the
    /// `point_of_commitment`/`earliest_direct` projection (ADR-0200 D4;
    /// `null` when no effect's interval excludes 0).
    pub(crate) fn lab_attribution_locus(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let report = req(params, "report")?;
        Ok(Json::obj([(
            "locus",
            hh_analysis::attribution::locus(report).unwrap_or(Json::Null),
        )]))
    }

    /// `lab.attribution.quality{delta, reports[]|hosted?}` →
    /// `{metric}` — the `attribution_quality` MetricValue shape
    /// (ADR-0201 D3; `n/a{not_run}`/`n/a{estimator_undefined}`/
    /// `n/a{class}`).
    pub(crate) fn lab_attribution_quality(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let delta = req(params, "delta")?;
        let reports: Vec<Json> = match params.get("reports") {
            Some(Json::Arr(a)) => a.clone(),
            _ => Vec::new(),
        };
        let hosted = params
            .get("hosted")
            .and_then(|j| match j {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);
        Ok(Json::obj([(
            "metric",
            hh_analysis::attribution::attribution_quality(delta, &reports, hosted),
        )]))
    }
}
