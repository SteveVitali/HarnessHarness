//! The ledger-facts projection (spec §5h.2 §4; R-2.9.2; S3.3).
//!
//! `LedgerFacts` is the typed view of one run's event stream that the veto
//! predicates and the compliance/opacity metrics read. It is built from
//! `(seq, class, payload)` rows — the offline path supplies fixture rows, the
//! in-kernel path supplies the store's durable events. Every member records
//! *what the ledger says*; nothing is inferred (a missing decision row is a
//! fact, never a default).
//!
//! Event classes read (all registered in `hh_ledger::classes::CLASS_TABLE`):
//!
//! - `security.egress.requested` / `security.egress.decided` — the L4
//!   `benchmark_egress` veto's input.
//! - `action.environment.phase.changed` — the per-handle phase timeline
//!   (`setup | agent | verify | teardown`; the bench phase marks the L4
//!   window).
//! - `action.effect.intended` / `action.effect.committed` /
//!   `security.permission.decided` — `uncompensated_mutation` +
//!   `intervention_rate`.
//! - `context.artefact.delivered` / `context.artefact.activated` /
//!   `verification.artefact.followed` — the compliance chain (ADR-0045 D4).
//! - `context.assembled` — `opacity_dynamic`'s item mass.
//! - `verification.validator.verdict` — detector verdicts (`conformity`,
//!   grader schema/parse evidence).
//! - `measurement.metric.emitted` — emitted values.
//! - `measurement.cost.attributed` + `model.call.attempt.completed` —
//!   `attribution_completeness`.
//! - `measurement.evolution.candidate.transitioned` — the first
//!   `to: proposed` seq (the `LeakedSplit` ordering input).
//! - `lifecycle.run.created` / `lifecycle.run.finished` — the run's bounds.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::Json;

use crate::json_util::*;

/// One `security.egress.*` row — the request plus the decision that settled it
/// (when one exists — an undecided request is a `benchmark_egress` trip).
#[derive(Debug, Clone, PartialEq)]
pub struct EgressRow {
    /// The ledger seq of the request.
    pub seq: u64,
    /// `request_ref` — joins `requested` to `decided`.
    pub request_ref: String,
    /// The env handle the request issued from.
    pub env_handle: Option<String>,
    /// `host_norm` — the normalized destination.
    pub host_norm: Option<String>,
    /// The `environment_phase` the run was in at `seq` (the timeline
    /// projection — `agent` is the L4-gated window).
    pub phase: Option<String>,
    /// `decided{decision}` — `Some(true)` allow / `Some(false)` deny /
    /// `None` undecided.
    pub decision: Option<bool>,
    /// The deciding rule (`rule_ref`) when the decision names one.
    pub rule_ref: Option<String>,
}

/// One environment-phase transition (`action.environment.phase.changed`).
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseChange {
    /// The ledger seq.
    pub seq: u64,
    /// The env handle.
    pub env_handle: Option<String>,
    /// The phase entered (`to`).
    pub to: String,
}

/// One `action.effect.*` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    /// The effect id.
    pub effect_id: String,
    /// The declared `idempotency_key`.
    pub idempotency_key: Option<String>,
    /// `compensates` — the effect this row compensates/reverts, when set.
    pub compensates: Option<String>,
    /// `reverts` — the revert link, when set.
    pub reverts: Option<String>,
}

/// One `context.assembled` item — `{kind, artefact_id?, tokens}`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledItem {
    /// The item kind (`artefact`/`artifact_excerpt` when the plan names an
    /// artefact; else the authority/`untyped` spelling).
    pub kind: String,
    /// The artefact the item delivers, when the plan names one.
    pub artefact_id: Option<String>,
    /// The item's token mass.
    pub tokens: i64,
}

/// One `context.artefact.*` delivery row.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtefactRow {
    /// The artefact id.
    pub artefact_id: String,
    /// The delivery id.
    pub delivery_id: Option<String>,
    /// The detector class that produced the row (followed rows).
    pub detector: Option<String>,
}

/// One `verification.validator.verdict` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictRow {
    /// The validator ref.
    pub validator_ref: Option<String>,
    /// The oracle class spelling.
    pub oracle_class: Option<String>,
    /// The verdict status (`decided | inconclusive | oracle_failure`).
    pub status: String,
    /// The verdict value JSON (the `{kind, value}` form).
    pub value: Json,
    /// The detector spelling.
    pub detector: Option<String>,
    /// The verdict's declared verifier isolation, when carried.
    pub isolation: Option<String>,
    /// The verdict's `inputs_digest` (the evidence-binding the
    /// replayed-verification check recomputes).
    pub inputs_digest: Option<String>,
    /// The phase the verdict targets.
    pub phase: Option<String>,
}

/// The bench-grading evidence a `measurement.bench.graded`-style row or a
/// verifier verdict surfaces — kept separate so the infrastructure detector
/// reads one typed record (spec §5h.4's grading contract).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GradingEvidence {
    /// The grader's exit code, when the grade step completed.
    pub exit_code: Option<i64>,
    /// Whether the verifier ran inside its own materialization
    /// (`separate` — never inside the agent env).
    pub verifier_separate: Option<bool>,
    /// Whether the grading schema parsed.
    pub results_parsed: Option<bool>,
    /// The declared fail-to-pass checks the grader skipped.
    pub skipped_f2p: Vec<String>,
    /// Whether the suite's declared command actually executed.
    pub suite_executed: Option<bool>,
    /// Whether the run's declared delivery channel produced the artefact the
    /// grader consumed (`deliverable_missing` when false).
    pub deliverable_present: Option<bool>,
}

/// `ledger_facts/1` — one run's projected evaluation facts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LedgerFacts {
    /// `lifecycle.run.created` observed.
    pub run_created: bool,
    /// `lifecycle.run.finished` observed (with the stop-reason JSON).
    pub finished: Option<Json>,
    /// The egress rows (request + joined decision).
    pub egress: Vec<EgressRow>,
    /// The phase timeline.
    pub phases: Vec<PhaseChange>,
    /// `action.effect.intended` rows.
    pub effects_intended: Vec<EffectRow>,
    /// `action.effect.committed` rows.
    pub effects_committed: Vec<EffectRow>,
    /// The `effect_id`s `security.permission.decided` denied.
    pub permission_denials: BTreeSet<String>,
    /// `context.artefact.delivered` rows.
    pub artefacts_delivered: Vec<ArtefactRow>,
    /// `context.artefact.activated` rows.
    pub artefacts_activated: Vec<ArtefactRow>,
    /// `verification.artefact.followed` rows (carry the detector).
    pub artefacts_followed: Vec<ArtefactRow>,
    /// `context.assembled` items per call (`(model_call_id, items)`).
    pub assembled: Vec<(String, Vec<AssembledItem>)>,
    /// `verification.validator.verdict` rows.
    pub verdicts: Vec<VerdictRow>,
    /// `measurement.metric.emitted` values (decoded `MetricValue`s are on
    /// `EvalRun.values`; this is the raw count for audit).
    pub metrics_emitted: u64,
    /// The `model_call_id`s `measurement.cost.attributed` covered.
    pub cost_attributed_calls: BTreeSet<String>,
    /// The `model_call_id`s `model.call.attempt.completed` reports.
    pub model_calls_completed: BTreeSet<String>,
    /// The first `measurement.evolution.candidate.transitioned{to: proposed}`
    /// seq (the `LeakedSplit` ordering input).
    pub first_proposed_at: Option<u64>,
    /// The bench grading evidence (a verifier-side row the adapter emits —
    /// `measurement.bench.graded` or the `verdict` members it folds into).
    pub grading: GradingEvidence,
}

fn s(j: &Json, member: &str) -> Option<String> {
    j.get(member).and_then(Json::as_str).map(str::to_string)
}

fn b(j: &Json, member: &str) -> Option<bool> {
    match j.get(member) {
        Some(Json::Bool(v)) => Some(*v),
        _ => None,
    }
}

fn effect_row(j: &Json) -> EffectRow {
    EffectRow {
        effect_id: s(j, "effect_id").unwrap_or_default(),
        idempotency_key: s(j, "idempotency_key"),
        compensates: s(j, "compensates"),
        reverts: s(j, "reverts"),
    }
}

fn artefact_row(j: &Json) -> ArtefactRow {
    ArtefactRow {
        artefact_id: s(j, "artefact_id").unwrap_or_default(),
        delivery_id: s(j, "delivery_id"),
        detector: s(j, "detector"),
    }
}

impl LedgerFacts {
    /// Project one run's `(seq, class, payload)` rows into the typed view.
    /// Unknown classes are skipped — the projection reads what it owns
    /// (unknown classes are refused at *append* by the store, not here).
    pub fn from_events(events: &[(u64, String, Json)]) -> LedgerFacts {
        let mut f = LedgerFacts::default();
        // First pass — the phase timeline (egress rows join against it).
        for (seq, class, p) in events {
            if class == "action.environment.phase.changed" {
                f.phases.push(PhaseChange {
                    seq: *seq,
                    env_handle: s(p, "env_handle").or_else(|| s(p, "env_handle_id")),
                    to: s(p, "to").or_else(|| s(p, "phase")).unwrap_or_default(),
                });
            }
        }
        let phase_at = |seq: u64, handle: &Option<String>| -> Option<String> {
            f.phases
                .iter()
                .filter(|c| c.seq <= seq && (handle.is_none() || c.env_handle == *handle))
                .max_by_key(|c| c.seq)
                .map(|c| c.to.clone())
        };
        // Second pass — everything else.
        let mut decided: BTreeMap<String, (bool, Option<String>, u64)> = BTreeMap::new();
        for (_, class, p) in events {
            if class == "security.egress.decided" {
                let decision = s(p, "decision").map(|d| d == "allow" || d == "allowed");
                if let (Some(r), Some(d)) = (s(p, "request_ref"), decision) {
                    decided.insert(r, (d, s(p, "rule_ref"), 0));
                }
            }
        }
        for (seq, class, p) in events {
            match class.as_str() {
                "lifecycle.run.created" => f.run_created = true,
                "lifecycle.run.finished" => f.finished = Some(p.clone()),
                "security.egress.requested" => {
                    let request_ref = s(p, "request_ref").unwrap_or_default();
                    let env_handle = s(p, "env_handle");
                    let d = decided.get(&request_ref);
                    f.egress.push(EgressRow {
                        seq: *seq,
                        request_ref,
                        env_handle: env_handle.clone(),
                        host_norm: s(p, "host_norm"),
                        phase: phase_at(*seq, &env_handle),
                        decision: d.map(|(v, _, _)| *v),
                        rule_ref: d.and_then(|(_, r, _)| r.clone()),
                    });
                }
                "action.effect.intended" => f.effects_intended.push(effect_row(p)),
                "action.effect.committed" => f.effects_committed.push(effect_row(p)),
                "security.permission.decided" => {
                    if s(p, "decision").as_deref() == Some("deny") {
                        if let Some(e) = s(p, "effect_id") {
                            f.permission_denials.insert(e);
                        }
                    }
                }
                "context.artefact.delivered" => f.artefacts_delivered.push(artefact_row(p)),
                "context.artefact.activated" => f.artefacts_activated.push(artefact_row(p)),
                "verification.artefact.followed" => f.artefacts_followed.push(artefact_row(p)),
                "context.assembled" => {
                    let call = s(p, "model_call_id").unwrap_or_default();
                    let mut items = Vec::new();
                    if let Some(Json::Arr(is)) = p.get("items") {
                        for it in is {
                            items.push(AssembledItem {
                                kind: s(it, "kind")
                                    .or_else(|| s(it, "authority"))
                                    .unwrap_or_else(|| "untyped".to_string()),
                                artefact_id: s(it, "artefact_id"),
                                tokens: it.get("tokens").and_then(Json::as_int).unwrap_or(0),
                            });
                        }
                    }
                    f.assembled.push((call, items));
                }
                "verification.validator.verdict" | "measurement.bench.graded" => {
                    if class == "measurement.bench.graded" {
                        f.grading.exit_code = p.get("exit_code").and_then(Json::as_int);
                        f.grading.verifier_separate = b(p, "verifier_separate");
                        f.grading.results_parsed = b(p, "results_parsed");
                        f.grading.suite_executed = b(p, "suite_executed");
                        f.grading.deliverable_present = b(p, "deliverable_present");
                        if let Some(Json::Arr(sk)) = p.get("skipped_f2p") {
                            f.grading.skipped_f2p = sk
                                .iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect();
                        }
                    } else {
                        f.verdicts.push(VerdictRow {
                            validator_ref: s(p, "validator_ref"),
                            oracle_class: s(p, "oracle_class"),
                            status: s(p, "status").unwrap_or_else(|| "decided".into()),
                            value: p
                                .get("value")
                                .or_else(|| p.get("verdict"))
                                .cloned()
                                .unwrap_or(Json::Null),
                            detector: s(p, "detector"),
                            isolation: s(p, "isolation"),
                            inputs_digest: s(p, "inputs_digest"),
                            phase: s(p, "phase"),
                        });
                        // A verifier verdict doubles as grading evidence when
                        // it carries the bench members.
                        if p.get("verifier_separate").is_some() {
                            f.grading.verifier_separate = b(p, "verifier_separate");
                        }
                    }
                }
                "measurement.metric.emitted" => f.metrics_emitted += 1,
                "measurement.cost.attributed" => {
                    if let Some(id) = s(p, "subject_ref").or_else(|| s(p, "model_call_id")) {
                        f.cost_attributed_calls.insert(id);
                    }
                }
                "model.call.attempt.completed" => {
                    if let Some(id) = s(p, "model_call_id").or_else(|| s(p, "call_id")) {
                        f.model_calls_completed.insert(id);
                    }
                }
                "measurement.evolution.candidate.transitioned" => {
                    if s(p, "to").as_deref() == Some("proposed") {
                        f.first_proposed_at =
                            Some(f.first_proposed_at.map_or(*seq, |e| e.min(*seq)));
                    }
                }
                _ => {}
            }
        }
        f
    }

    /// The canonical JSON form (`ledger_facts/1`).
    pub fn to_json(&self) -> Json {
        let egress: Vec<Json> = self
            .egress
            .iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("seq".into(), Json::Int(e.seq as i64));
                m.insert("request_ref".into(), Json::str(&e.request_ref));
                insert_opt(&mut m, "env_handle", e.env_handle.as_deref().map(Json::str));
                insert_opt(&mut m, "host_norm", e.host_norm.as_deref().map(Json::str));
                insert_opt(&mut m, "phase", e.phase.as_deref().map(Json::str));
                insert_opt(&mut m, "decision", e.decision.map(Json::Bool));
                insert_opt(&mut m, "rule_ref", e.rule_ref.as_deref().map(Json::str));
                Json::Obj(m)
            })
            .collect();
        let effect = |e: &EffectRow| {
            let mut m = BTreeMap::new();
            m.insert("effect_id".into(), Json::str(&e.effect_id));
            insert_opt(
                &mut m,
                "idempotency_key",
                e.idempotency_key.as_deref().map(Json::str),
            );
            insert_opt(
                &mut m,
                "compensates",
                e.compensates.as_deref().map(Json::str),
            );
            insert_opt(&mut m, "reverts", e.reverts.as_deref().map(Json::str));
            Json::Obj(m)
        };
        let artefact = |a: &ArtefactRow| {
            let mut m = BTreeMap::new();
            m.insert("artefact_id".into(), Json::str(&a.artefact_id));
            insert_opt(
                &mut m,
                "delivery_id",
                a.delivery_id.as_deref().map(Json::str),
            );
            insert_opt(&mut m, "detector", a.detector.as_deref().map(Json::str));
            Json::Obj(m)
        };
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("ledger_facts/1"));
        m.insert("run_created".into(), Json::Bool(self.run_created));
        insert_opt(&mut m, "finished", self.finished.clone());
        m.insert("egress".into(), Json::Arr(egress));
        m.insert(
            "phases".into(),
            Json::Arr(
                self.phases
                    .iter()
                    .map(|c| {
                        let mut pm = BTreeMap::new();
                        pm.insert("seq".into(), Json::Int(c.seq as i64));
                        pm.insert("to".into(), Json::str(&c.to));
                        insert_opt(
                            &mut pm,
                            "env_handle",
                            c.env_handle.as_deref().map(Json::str),
                        );
                        Json::Obj(pm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "effects_intended".into(),
            Json::Arr(self.effects_intended.iter().map(effect).collect()),
        );
        m.insert(
            "effects_committed".into(),
            Json::Arr(self.effects_committed.iter().map(effect).collect()),
        );
        m.insert(
            "permission_denials".into(),
            Json::Arr(self.permission_denials.iter().map(Json::str).collect()),
        );
        m.insert(
            "artefacts_delivered".into(),
            Json::Arr(self.artefacts_delivered.iter().map(artefact).collect()),
        );
        m.insert(
            "artefacts_activated".into(),
            Json::Arr(self.artefacts_activated.iter().map(artefact).collect()),
        );
        m.insert(
            "artefacts_followed".into(),
            Json::Arr(self.artefacts_followed.iter().map(artefact).collect()),
        );
        m.insert(
            "assembled".into(),
            Json::Arr(
                self.assembled
                    .iter()
                    .map(|(call, items)| {
                        Json::obj([
                            ("model_call_id", Json::str(call)),
                            (
                                "items",
                                Json::Arr(
                                    items
                                        .iter()
                                        .map(|i| {
                                            let mut im = BTreeMap::new();
                                            im.insert("kind".into(), Json::str(&i.kind));
                                            if let Some(a) = &i.artefact_id {
                                                im.insert("artefact_id".into(), Json::str(a));
                                            }
                                            im.insert("tokens".into(), Json::Int(i.tokens));
                                            Json::Obj(im)
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "verdicts".into(),
            Json::Arr(
                self.verdicts
                    .iter()
                    .map(|v| {
                        let mut vm = BTreeMap::new();
                        insert_opt(
                            &mut vm,
                            "validator_ref",
                            v.validator_ref.as_deref().map(Json::str),
                        );
                        insert_opt(
                            &mut vm,
                            "oracle_class",
                            v.oracle_class.as_deref().map(Json::str),
                        );
                        vm.insert("status".into(), Json::str(&v.status));
                        vm.insert("value".into(), v.value.clone());
                        insert_opt(&mut vm, "detector", v.detector.as_deref().map(Json::str));
                        insert_opt(&mut vm, "isolation", v.isolation.as_deref().map(Json::str));
                        insert_opt(
                            &mut vm,
                            "inputs_digest",
                            v.inputs_digest.as_deref().map(Json::str),
                        );
                        insert_opt(&mut vm, "phase", v.phase.as_deref().map(Json::str));
                        Json::Obj(vm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "metrics_emitted".into(),
            Json::Int(self.metrics_emitted as i64),
        );
        m.insert(
            "cost_attributed_calls".into(),
            Json::Arr(self.cost_attributed_calls.iter().map(Json::str).collect()),
        );
        m.insert(
            "model_calls_completed".into(),
            Json::Arr(self.model_calls_completed.iter().map(Json::str).collect()),
        );
        insert_opt(
            &mut m,
            "first_proposed_at",
            self.first_proposed_at.map(|s| Json::Int(s as i64)),
        );
        let g = &self.grading;
        let mut gm = BTreeMap::new();
        insert_opt(&mut gm, "exit_code", g.exit_code.map(Json::Int));
        insert_opt(
            &mut gm,
            "verifier_separate",
            g.verifier_separate.map(Json::Bool),
        );
        insert_opt(&mut gm, "results_parsed", g.results_parsed.map(Json::Bool));
        insert_opt(&mut gm, "suite_executed", g.suite_executed.map(Json::Bool));
        insert_opt(
            &mut gm,
            "deliverable_present",
            g.deliverable_present.map(Json::Bool),
        );
        if !g.skipped_f2p.is_empty() {
            gm.insert(
                "skipped_f2p".into(),
                Json::Arr(g.skipped_f2p.iter().map(Json::str).collect()),
            );
        }
        m.insert("grading".into(), Json::Obj(gm));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<LedgerFacts, SchemaError> {
        const REC: &str = "ledger_facts/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "schema",
                "run_created",
                "finished",
                "egress",
                "phases",
                "effects_intended",
                "effects_committed",
                "permission_denials",
                "artefacts_delivered",
                "artefacts_activated",
                "artefacts_followed",
                "assembled",
                "verdicts",
                "metrics_emitted",
                "cost_attributed_calls",
                "model_calls_completed",
                "first_proposed_at",
                "grading",
            ],
            REC,
        )?;
        let mut f = LedgerFacts {
            run_created: opt_bool_at(m, "run_created")?.unwrap_or(false),
            finished: m.get("finished").cloned(),
            ..LedgerFacts::default()
        };
        if let Some(Json::Arr(es)) = m.get("egress") {
            for e in es {
                f.egress.push(EgressRow {
                    seq: e.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    request_ref: s(e, "request_ref").unwrap_or_default(),
                    env_handle: s(e, "env_handle"),
                    host_norm: s(e, "host_norm"),
                    phase: s(e, "phase"),
                    decision: b(e, "decision"),
                    rule_ref: s(e, "rule_ref"),
                });
            }
        }
        if let Some(Json::Arr(ps)) = m.get("phases") {
            for c in ps {
                f.phases.push(PhaseChange {
                    seq: c.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    env_handle: s(c, "env_handle"),
                    to: s(c, "to").unwrap_or_default(),
                });
            }
        }
        let effect = |e: &Json| EffectRow {
            effect_id: s(e, "effect_id").unwrap_or_default(),
            idempotency_key: s(e, "idempotency_key"),
            compensates: s(e, "compensates"),
            reverts: s(e, "reverts"),
        };
        let artefact = |a: &Json| ArtefactRow {
            artefact_id: s(a, "artefact_id").unwrap_or_default(),
            delivery_id: s(a, "delivery_id"),
            detector: s(a, "detector"),
        };
        if let Some(Json::Arr(es)) = m.get("effects_intended") {
            f.effects_intended = es.iter().map(effect).collect();
        }
        if let Some(Json::Arr(es)) = m.get("effects_committed") {
            f.effects_committed = es.iter().map(effect).collect();
        }
        if let Some(Json::Arr(ds)) = m.get("permission_denials") {
            f.permission_denials = ds
                .iter()
                .filter_map(|d| d.as_str().map(str::to_string))
                .collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_delivered") {
            f.artefacts_delivered = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_activated") {
            f.artefacts_activated = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_followed") {
            f.artefacts_followed = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("assembled") {
            for a in as_ {
                let call = s(a, "model_call_id").unwrap_or_default();
                let items = a
                    .get("items")
                    .and_then(|v| match v {
                        Json::Arr(is) => Some(is),
                        _ => None,
                    })
                    .map(|is| {
                        is.iter()
                            .map(|i| AssembledItem {
                                kind: s(i, "kind").unwrap_or_else(|| "untyped".into()),
                                artefact_id: s(i, "artefact_id"),
                                tokens: i.get("tokens").and_then(Json::as_int).unwrap_or(0),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                f.assembled.push((call, items));
            }
        }
        if let Some(Json::Arr(vs)) = m.get("verdicts") {
            for v in vs {
                f.verdicts.push(VerdictRow {
                    validator_ref: s(v, "validator_ref"),
                    oracle_class: s(v, "oracle_class"),
                    status: s(v, "status").unwrap_or_else(|| "decided".into()),
                    value: v.get("value").cloned().unwrap_or(Json::Null),
                    detector: s(v, "detector"),
                    isolation: s(v, "isolation"),
                    inputs_digest: s(v, "inputs_digest"),
                    phase: s(v, "phase"),
                });
            }
        }
        f.metrics_emitted = opt_int_at(m, "metrics_emitted")?.unwrap_or(0) as u64;
        if let Some(Json::Arr(cs)) = m.get("cost_attributed_calls") {
            f.cost_attributed_calls = cs
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
        }
        if let Some(Json::Arr(cs)) = m.get("model_calls_completed") {
            f.model_calls_completed = cs
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
        }
        f.first_proposed_at = opt_int_at(m, "first_proposed_at")?.map(|x| x as u64);
        if let Some(g) = m.get("grading") {
            f.grading = GradingEvidence {
                exit_code: g.get("exit_code").and_then(Json::as_int),
                verifier_separate: b(g, "verifier_separate"),
                results_parsed: b(g, "results_parsed"),
                suite_executed: b(g, "suite_executed"),
                deliverable_present: b(g, "deliverable_present"),
                skipped_f2p: g
                    .get("skipped_f2p")
                    .and_then(|v| match v {
                        Json::Arr(sk) => Some(sk),
                        _ => None,
                    })
                    .map(|sk| {
                        sk.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
        }
        Ok(f)
    }
}
