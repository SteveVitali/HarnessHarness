//! `project_row(run_id, until_seq?, scoring?)` — the pure fold from a
//! durable run prefix to `ResultsRow/1` (§6.5 §2.1 row 1; ADR-0161 D1/D4).
//! Pure: equal `(prefix, scoring)` inputs produce equal bytes; the row's
//! `derived_from`/`watermark_set` pin exactly what was read.

use std::collections::{BTreeMap, BTreeSet};

use hh_eval::catalogue::scorecard_metrics;
use hh_experiment::docs::LabDocs;
use hh_ledger::event::EventEnvelope;
use hh_ledger::manifest::RunKind;
use hh_ledger::store::Store;
use hh_ontology::control::StopReason;
use hh_ontology::eval::MetricValue;
use hh_ontology::eval::MetricValueKind;
use hh_ontology::participant::{Observability, ParticipantClass as OntClass};
use hh_wire::json::Json;

use crate::audit::AuditRef;
use crate::error::ResultsError;
use crate::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use crate::scoring::ScoringContext;
use crate::watermark::WatermarkSet;

/// `project_row(run_id, until_seq?, scoring?)` — the §6.5 §2.1 op. `docs`
/// resolves the bound arm's budget/`MatchSpec` through the experiment's
/// `LabDocs` (an unbound or doc-less run simply carries fewer budget
/// members — the row never fabricates refs).
pub fn project_row(
    store: &Store,
    docs: Option<&LabDocs>,
    run_id: &str,
    until_seq: Option<u64>,
    scoring: Option<&ScoringContext>,
) -> Result<ResultsRow, ResultsError> {
    let manifest = store.manifest(run_id).map_err(ResultsError::from)?;
    if manifest.run_kind != RunKind::Agent {
        return Err(ResultsError::NotSubjectRun {
            run_id: run_id.to_string(),
            run_kind: manifest.run_kind.as_str().to_string(),
        });
    }
    let head = store.head(run_id).map_err(ResultsError::from)?;
    let until = until_seq.unwrap_or(head.seq);
    if until > head.seq {
        return Err(ResultsError::RunNotDurable {
            run_id: run_id.to_string(),
            until_seq: until,
            head: head.seq,
        });
    }
    // The cited prefix must verify — a tampered source refuses the
    // projection rather than producing a row over broken bytes.
    store
        .verify_run(run_id, None, Some(until), None)
        .map_err(ResultsError::from)?;

    let scoring = scoring.cloned().unwrap_or_else(ScoringContext::native);
    scoring.validate()?;

    let events: Vec<&EventEnvelope> = store
        .envelopes(run_id)
        .map_err(ResultsError::from)?
        .iter()
        .filter(|e| e.seq <= until)
        .collect();

    // ── terminal + measurement fold ────────────────────────────────────
    let mut outcome = OutcomeSection {
        status: "open".to_string(),
        activation_no: manifest.activation_no,
        ..OutcomeSection::default()
    };
    let mut consumed: BTreeMap<String, i64> = BTreeMap::new();
    let mut vetoes: BTreeSet<String> = BTreeSet::new();
    let mut metrics: BTreeMap<String, Vec<(u64, MetricValue, Vec<String>)>> = BTreeMap::new();
    let mut checkpoint_ref: Option<String> = None;
    let mut discontinuities: Vec<Json> = Vec::new();
    let mut experiment_id_from_bound: Option<String> = None;

    for e in &events {
        match e.class.as_str() {
            "lifecycle.run.finished" => {
                outcome.status = "finished".to_string();
                outcome.finished_at = Some(e.ts.clone());
                let stop = e
                    .payload
                    .get("stop_reason")
                    .and_then(StopReason::from_json)
                    .unwrap_or(StopReason::Completed);
                let oracle_failed = match e.payload.get("outcome_class").and_then(Json::as_str) {
                    Some("oracle_failure") => true,
                    _ => matches!(e.payload.get("oracle_failed"), Some(Json::Bool(true))),
                };
                outcome.stop_reason = Some(stop.kind().as_str().to_string());
                outcome.outcome_class = Some(
                    hh_ontology::eval::derive_outcome_class(&stop, oracle_failed)
                        .as_str()
                        .to_string(),
                );
                if let Some(Json::Int(w)) = e.payload.get("wall_ms") {
                    outcome.wall_ms = Some(*w);
                }
            }
            "control.budget.consumed" => {
                if let (Some(d), Some(a)) = (
                    e.payload.get("dimension").and_then(Json::as_str),
                    e.payload.get("amount").and_then(Json::as_int),
                ) {
                    *consumed.entry(d.to_string()).or_insert(0) += a;
                }
            }
            "measurement.metric.emitted" => {
                if let Ok(v) = MetricValue::from_json(&e.payload) {
                    let mut refs: Vec<String> = e.refs.iter().map(|r| r.id()).collect();
                    if let Some(er) = &v.evidence_ref {
                        refs.push(er.clone());
                    }
                    metrics
                        .entry(v.metric_ref.clone())
                        .or_default()
                        .push((e.seq, v, refs));
                }
            }
            "measurement.experiment.bound" => {
                if let Some(id) = e.payload.get("experiment_id").and_then(Json::as_str) {
                    experiment_id_from_bound = Some(id.to_string());
                }
            }
            "security.audit.checkpoint" => {
                checkpoint_ref = Some(hh_ledger::tree::checkpoint_idp(&e.payload));
            }
            c if c.starts_with("security.invariant") || c.starts_with("control.veto") => {
                for k in ["veto_id", "metric", "invariant"] {
                    if let Some(v) = e.payload.get(k).and_then(Json::as_str) {
                        vetoes.insert(v.to_string());
                    }
                }
            }
            c if c.contains("discontinu") => {
                discontinuities.push(Json::obj([
                    ("seq", Json::Int(e.seq as i64)),
                    ("class", Json::str(c)),
                    ("hash", Json::str(&e.hash)),
                ]));
            }
            _ => {}
        }
    }
    outcome.veto_tripped = vetoes.into_iter().collect();

    // ── overlay (regrade) fold — ADR-0161 D2 ───────────────────────────
    // A verdict targets the subject iff its `target` names the subject run
    // or the envelope's `causes[]` cite a subject event; the metric it
    // overrides is `payload.metric` (or `criterion_ref` as the fallback
    // ref). An overlay that never targets the subject refuses
    // `OverlayNotTargeting`.
    let mut overlay_watermarks: BTreeMap<String, u64> = BTreeMap::new();
    let mut overlay_cells: BTreeMap<String, (MetricValueKind, String, String, u64)> =
        BTreeMap::new();
    let mut overlay_outcome_oracle_failure = false;
    for ov in &scoring.overlay_runs {
        let ohead = store
            .head(&ov.run_id)
            .map_err(|_| ResultsError::OverlayNotTargeting {
                overlay_run_id: ov.run_id.clone(),
                subject_run_id: run_id.to_string(),
            })?;
        let oseq = ov.until_seq.unwrap_or(ohead.seq);
        if oseq > ohead.seq {
            return Err(ResultsError::RunNotDurable {
                run_id: ov.run_id.clone(),
                until_seq: oseq,
                head: ohead.seq,
            });
        }
        let oevents: Vec<&EventEnvelope> = store
            .envelopes(&ov.run_id)
            .map_err(ResultsError::from)?
            .iter()
            .filter(|e| e.seq <= oseq)
            .collect();
        let mut targeted = false;
        for e in &oevents {
            let causes_hit = e.causes.iter().any(|c| c.run_id == run_id);
            if e.class != "verification.validator.verdict" && !causes_hit {
                continue;
            }
            let p = &e.payload;
            let target_hit = p.get("target").and_then(Json::as_str) == Some(run_id);
            if !(target_hit || causes_hit) {
                continue;
            }
            if e.class == "verification.validator.verdict" {
                targeted = true;
                let metric = p
                    .get("metric")
                    .and_then(Json::as_str)
                    .or_else(|| p.get("criterion_ref").and_then(Json::as_str))
                    .unwrap_or("")
                    .to_string();
                let status = p.get("status").and_then(Json::as_str).unwrap_or("");
                if status == "oracle_failure" {
                    overlay_outcome_oracle_failure = true;
                    continue;
                }
                if status != "decided" || metric.is_empty() {
                    continue;
                }
                let value = verdict_value_to_kind(p.get("value"));
                let detector = p
                    .get("detector")
                    .and_then(Json::as_str)
                    .unwrap_or("deterministic")
                    .to_string();
                overlay_cells.insert(
                    metric,
                    (value, detector, format!("{}:{}", ov.run_id, e.seq), e.seq),
                );
            } else if causes_hit {
                targeted = true;
            }
        }
        if !targeted {
            return Err(ResultsError::OverlayNotTargeting {
                overlay_run_id: ov.run_id.clone(),
                subject_run_id: run_id.to_string(),
            });
        }
        overlay_watermarks.insert(ov.run_id.clone(), oseq);
    }
    if overlay_outcome_oracle_failure {
        outcome.outcome_class = Some("oracle_failure".to_string());
    }

    // ── cells — the total list (R-ROW-3) ───────────────────────────────
    let head_ref = |seq: u64, hash: &str| AuditRef {
        run_id: run_id.to_string(),
        seq,
        hash: hash.to_string(),
        checkpoint_ref: checkpoint_ref.clone(),
        content_refs: Vec::new(),
    };
    let run_head_ref = head_ref(
        until,
        &events.last().map(|e| e.hash.clone()).unwrap_or_default(),
    );
    let pclass = manifest.participant_class;
    let mut obs: BTreeSet<Observability> = manifest
        .observability_level
        .iter()
        .map(|o| Observability::parse(o.as_str()).unwrap_or(Observability::Events))
        .collect();
    // `ledger` entails the lower levels — the full run ledger *carries*
    // the events, the model I/O, and the end state (§2.7.2: `ledger ⇔
    // native` is the maximal observability). Without the entailment a
    // native run declaring `{ledger}` would n/a{observability} every
    // `end_state` metric — the inverse of the intent.
    if obs.contains(&Observability::Ledger) {
        obs.extend([
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
        ]);
    }
    let ont_class = OntClass::parse(pclass.as_str());

    let mut cells: Vec<Cell> = Vec::new();
    for decl in scorecard_metrics() {
        let na = if !decl
            .applies_to_classes
            .iter()
            .any(|c| Some(*c) == ont_class)
        {
            Some(hh_ontology::compliance::NaReason::Class)
        } else if !decl.requires_observability.is_subset(&obs) {
            Some(hh_ontology::compliance::NaReason::Observability)
        } else if !decl.requires_capabilities.is_empty() {
            // Stage 3 has no capability-vector resolution — a required
            // capability is honestly `n/a{capability}` (never silently
            // scored; OQ-377's capability plane lands at C2).
            Some(hh_ontology::compliance::NaReason::Capability)
        } else {
            None
        };
        let mut cell = match na {
            Some(r) => Cell {
                metric_ref: decl.name.clone(),
                value: MetricValueKind::Na(r),
                detector: None,
                oracle_ref: None,
                confidence: None,
                evidence: vec![run_head_ref.clone()],
                computed_from: Vec::new(),
            },
            None => match metrics.get(&decl.name).and_then(|v| v.last()) {
                Some((seq, v, refs)) => Cell {
                    metric_ref: decl.name.clone(),
                    value: v.value.clone(),
                    detector: Some(v.detector.as_str().to_string()),
                    oracle_ref: Some(v.oracle_ref.clone()),
                    confidence: v.confidence,
                    evidence: vec![AuditRef {
                        content_refs: refs.clone(),
                        ..head_ref(*seq, &event_hash(&events, *seq))
                    }],
                    computed_from: vec![seq.to_string()],
                },
                None => Cell {
                    metric_ref: decl.name.clone(),
                    value: MetricValueKind::Na(hh_ontology::compliance::NaReason::NotRun),
                    detector: None,
                    oracle_ref: None,
                    confidence: None,
                    evidence: vec![run_head_ref.clone()],
                    computed_from: Vec::new(),
                },
            },
        };
        if let Some((value, detector, osource, _oseq)) = overlay_cells.get(&decl.name) {
            cell.value = value.clone();
            cell.detector = Some(detector.clone());
            cell.computed_from.push(osource.clone());
        }
        cells.push(cell);
    }

    // ── coordinates ────────────────────────────────────────────────────
    let cvid = manifest
        .configuration_version_id
        .clone()
        .ok_or_else(|| ResultsError::Schema {
            member: "configuration_version_id".into(),
            detail: "agent manifest lacks the row-key coordinate".into(),
        })?;
    let key = RowKey {
        configuration_version_id: cvid.clone(),
        run_id: run_id.to_string(),
    };

    // The bound arm's budget block — resolved through the experiment's
    // LabDocs when the binding exists (§6.5 coordinates.budget).
    let mut budget = BTreeMap::new();
    if let Some(b) = &manifest.budget {
        budget.insert("budget_ref".into(), Json::str(b));
    }
    let mut cache = Json::obj([]);
    let binding = manifest.experiment.as_ref();
    let experiment_id = binding
        .and_then(|b| b.experiment_id.clone())
        .or(experiment_id_from_bound);
    if let (Some(docs), Some(exp_id), Some(b)) = (docs, experiment_id.as_ref(), binding) {
        if let Ok(Some(spec)) = docs.spec(exp_id) {
            if let Some(arm_id) = &b.arm_id {
                if let Some(arm) = spec.arms.iter().find(|a| &a.arm_id == arm_id) {
                    budget.insert("eval_budget_ref".into(), Json::str(&arm.eval_budget));
                    if let Some(s) = &arm.search_budget {
                        budget.insert("search_budget_ref".into(), Json::str(s));
                    }
                    if let Some(ms) = &arm.match_spec {
                        budget.insert("match_spec".into(), ms.to_json());
                        let msr = b.match_spec_ref.clone().unwrap_or_else(|| {
                            hh_identity::idp::idp_id(
                                "match_spec",
                                ms.to_json().to_canonical_string().as_bytes(),
                            )
                        });
                        budget.insert("match_spec_ref".into(), Json::str(&msr));
                        cache = Json::obj([("policy", ms.cache_policy.to_json())]);
                    }
                }
            }
        }
    }

    let experiment = binding.map(|b| {
        let mut m = BTreeMap::new();
        if let Some(r) = &b.experiment_run_id {
            m.insert("experiment_run_id".into(), Json::str(r));
        }
        if let Some(a) = &b.arm_id {
            m.insert("arm_id".into(), Json::str(a));
        }
        if let Some(c) = &b.cell_id {
            m.insert("cell_id".into(), Json::str(c));
        }
        if let Some(d) = &b.design_ref {
            m.insert("design_ref".into(), Json::str(d));
        }
        if let Some(p) = &b.pre_registration_ref {
            m.insert("pre_registration_ref".into(), Json::str(p));
        }
        if let Some(i) = b.replicate_index {
            m.insert("replicate_index".into(), Json::Int(i as i64));
        }
        if let Some(a) = b.attempt_no {
            m.insert("attempt_no".into(), Json::Int(a as i64));
        }
        if let Some(c) = b.comparable {
            m.insert("comparable".into(), Json::Bool(c));
        }
        if let Some(r) = &b.registry_snapshot_id {
            m.insert("registry_snapshot_id".into(), Json::str(r));
        }
        Json::Obj(m)
    });

    let mut task = None;
    if let Some(t) = &manifest.task_ref {
        task = Some(Json::obj([
            ("task_id", Json::str(&t.task_id)),
            ("suite_id", Json::str(&t.suite_id)),
            ("split_label", Json::str(t.split_label.name())),
        ]));
    }
    let mut replicate = BTreeMap::new();
    if let Some(s) = manifest.seed {
        replicate.insert("seed".into(), Json::Int(s as i64));
    }
    if let Some(i) = binding.and_then(|b| b.replicate_index) {
        replicate.insert("replicate_index".into(), Json::Int(i as i64));
    }
    let mut lineage = BTreeMap::new();
    if let Some(p) = &manifest.parent_run_id {
        lineage.insert("parent_run_id".into(), Json::str(p));
    }
    if let Some(f) = &manifest.forked_from {
        lineage.insert(
            "forked_from".into(),
            Json::obj([
                ("run_id", Json::str(&f.run_id)),
                ("at_seq", Json::Int(f.at_seq as i64)),
                ("head_hash", Json::str(&f.head_hash)),
            ]),
        );
    }
    if let Some(c) = &manifest.continued_from {
        lineage.insert(
            "continued_from".into(),
            Json::obj([
                ("run_id", Json::str(&c.run_id)),
                ("at_seq", Json::Int(c.at_seq as i64)),
                ("head_hash", Json::str(&c.head_hash)),
            ]),
        );
    }
    let mut model_snapshots: BTreeMap<String, String> = BTreeMap::new();
    if let Some(Json::Obj(ms)) = manifest.extra.get("model_snapshots") {
        for (k, v) in ms {
            if let Some(s) = v.as_str() {
                model_snapshots.insert(k.clone(), s.to_string());
            }
        }
    }
    let annotations_from_ledger = Json::obj([("discontinuities", Json::Arr(discontinuities))]);

    let coordinates = Coordinates {
        configuration_id: manifest.configuration_id.clone(),
        configuration_version_id: cvid,
        model_snapshots,
        harness_def_ref: manifest.harness_def_ref.clone(),
        model_profile_ref: manifest.model_profile_ref.clone(),
        environment_ref: manifest.environment_ref.clone(),
        environment_version_id: manifest.environment_version_id.clone(),
        task,
        budget: Json::Obj(budget),
        replicate: Json::Obj(replicate),
        participant_class: pclass.as_str().to_string(),
        observability_level: manifest
            .observability_level
            .iter()
            .map(|o| o.as_str().to_string())
            .collect(),
        hosting_mechanism: manifest.hosting_mechanism.clone(),
        capability_vector_ref: manifest.capability_declaration_ref.clone(),
        // §6.6 — the mediation channels the arm's hosting declaration covered,
        // stamped at bind time into `manifest.extra["mediation"]` (Stage 4
        // producer); absent/empty = no mediated channels claimed (the scorecard
        // reads it through `EvalRun.mediation` — never defaulted, T-LCD-07).
        mediation: manifest
            .extra
            .get("mediation")
            .and_then(|m| match m {
                Json::Arr(items) => Some(
                    items
                        .iter()
                        .filter_map(|i| i.as_str().map(String::from))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default(),
        registry_snapshot_id: manifest
            .registry_snapshot_id
            .clone()
            .or_else(|| binding.and_then(|b| b.registry_snapshot_id.clone())),
    };

    let mut watermark_set = WatermarkSet::new();
    watermark_set.pin(run_id, until);
    for (r, s) in &overlay_watermarks {
        watermark_set.pin(r, *s);
    }

    let mut row = ResultsRow {
        key,
        coordinates,
        experiment,
        outcome,
        cells,
        consumption: ConsumptionSection {
            dimensions: consumed,
            utilization: None,
            spend: None,
            instrument_spend: None,
        },
        cache,
        lineage: Json::Obj(lineage),
        annotations_from_ledger,
        audit: AuditSection {
            head: run_head_ref,
            checkpoint_ref,
        },
        scoring,
        derived_from: DerivedFrom {
            run_id: run_id.to_string(),
            seq: until,
            overlay_watermarks,
        },
        watermark_set,
        view_hash: String::new(),
        version_id: String::new(),
    };
    row.version_id = row.compute_version_id();
    row.view_hash = row.compute_view_hash();
    Ok(row)
}

fn event_hash(events: &[&EventEnvelope], seq: u64) -> String {
    events
        .iter()
        .find(|e| e.seq == seq)
        .map(|e| e.hash.clone())
        .unwrap_or_default()
}

/// `verification.validator.verdict`'s `value` member → `MetricValueKind`
/// (the overlay carries a typed verdict; the cell stores the metric lattice
/// — one value vocabulary, CC7).
fn verdict_value_to_kind(v: Option<&Json>) -> MetricValueKind {
    match v {
        Some(Json::Bool(b)) => MetricValueKind::Bool(*b),
        Some(Json::Int(i)) => MetricValueKind::Decimal(*i),
        Some(Json::Str(s)) => hh_ontology::eval::LatticeValue::parse(s)
            .map(MetricValueKind::Verdict)
            .unwrap_or_else(|| MetricValueKind::Vector(v.cloned().unwrap_or(Json::Null))),
        Some(other) => MetricValueKind::Vector((*other).clone()),
        None => MetricValueKind::Na(hh_ontology::compliance::NaReason::EstimatorUndefined),
    }
}
