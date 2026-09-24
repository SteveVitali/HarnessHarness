//! The `model.*` event payload builders (ADR-0118 d.3/d.7; §5b.1 terminal
//! payload; §5b.2 `model.route.decided`/`model.rerouted`; §5b.3
//! `model.profile.*`; §5b.4 `model.cache.resolved`). The gateway is the **sole
//! emitter** of usage, cost provenance, attempt timing, served-model drift and
//! transport capability facts — the payload is built here; the kernel's
//! `Store::append` is the durable writer.
//!
//! Payloads carry references and `provided: yes/no`, never secret material —
//! a credential appears as `credential_binding_id`/`provided` only.

use hh_wire::json::Json;

use crate::cache::{CacheKind, CacheOutcome};
use crate::plan::CachePlan;
use crate::vocab::{ModelError, Purpose, RerouteReason};

/// `model.call.requested{model_call_id, model_ref, request_ref?, plan_hash?,
/// view_hash?, token_estimate?, cache{…}, credential_binding_id, role,
/// dialect{…}, endpoint_ref, request_class, stream, context_label?}` — opens
/// the `model_call_id` scope (§5b.1 `open_call` / the §5b.4 R1 payload;
/// `request_ref` is a blob iff `model_io` — the kernel writes it, the
/// payload carries the hash; `cache` carries the request-side rows:
/// `semantics_kind`, `affinity_key`, `affinity_wire_len`, `static_hash`,
/// `purpose`, `expected_state` + `basis`, `markers` (count),
/// `markers_dropped[]`, `markers_substituted` (count),
/// `retention_classes_requested[]`).
pub fn call_requested(
    req: &crate::plan::InferenceRequest,
    semantics: &crate::cache::CacheSemantics,
    credential_binding_id: Option<&str>,
    plan_hash: Option<String>,
    view_hash: Option<&str>,
    token_estimate: Option<u64>,
    request_ref: Option<&str>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("model_call_id".into(), Json::str(&req.model_call_id));
    m.insert("model_ref".into(), req.model_ref.to_json());
    if let Some(r) = request_ref {
        m.insert("request_ref".into(), Json::str(r));
    }
    m.insert(
        "plan_hash".into(),
        Json::str(plan_hash.unwrap_or_else(|| req.plan.content_id())),
    );
    if let Some(h) = view_hash.or(req.view_hash.as_deref()) {
        m.insert("view_hash".into(), Json::str(h));
    }
    if let Some(t) = token_estimate {
        m.insert("token_estimate".into(), Json::Int(t as i64));
    }
    // `cache{…}` — the request-side rows (the resolved state rides
    // `model.cache.resolved` / `model.call.completed`; §5b.4 R1).
    let cache = &req.plan.cache;
    let mut c = std::collections::BTreeMap::new();
    c.insert("semantics_kind".into(), Json::str(semantics.kind()));
    if let Some(a) = &cache.affinity_full {
        c.insert("affinity_key".into(), Json::str(a.clone()));
    }
    if let Some(w) = &cache.affinity_wire {
        c.insert("affinity_wire_len".into(), Json::Int(w.len() as i64));
    }
    if let Some(s) = &cache.static_hash {
        c.insert("static_hash".into(), Json::str(s.clone()));
    }
    c.insert("purpose".into(), Json::str(req.purpose.as_str()));
    if let Some(e) = &cache.expected {
        c.insert("expected_state".into(), Json::str(e.expected.as_str()));
        c.insert("basis".into(), expected_basis_json(&e.basis));
    }
    c.insert("markers".into(), Json::Int(cache.markers.len() as i64));
    if !cache.dropped.is_empty() {
        c.insert(
            "markers_dropped".into(),
            Json::Arr(
                cache
                    .dropped
                    .iter()
                    .map(|d| {
                        Json::obj([
                            ("position", Json::str(d.position.as_str())),
                            ("reason", Json::str(d.reason.as_str())),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    c.insert(
        "markers_substituted".into(),
        Json::Int(cache.substituted.len() as i64),
    );
    c.insert(
        "retention_classes_requested".into(),
        Json::Arr(
            semantics
                .retention_classes()
                .iter()
                .map(|r| Json::str(r.class_id.clone()))
                .collect(),
        ),
    );
    m.insert("cache".into(), Json::Obj(c));
    m.insert(
        "credential_binding_id".into(),
        match credential_binding_id {
            Some(b) => Json::str(b),
            None => Json::Null,
        },
    );
    m.insert("role".into(), Json::str(req.role.clone()));
    m.insert(
        "dialect".into(),
        Json::obj([
            ("dialect_id", Json::str(&req.plan.dialect_id)),
            ("version", Json::str(&req.plan.dialect_version)),
        ]),
    );
    m.insert("endpoint_ref".into(), Json::str(&req.plan.endpoint_ref));
    m.insert(
        "request_class".into(),
        Json::str(req.request_class.as_str()),
    );
    m.insert("stream".into(), Json::Bool(req.stream));
    if let Some(l) = &req.context_label {
        m.insert("context_label".into(), Json::str(l.clone()));
    }
    Json::Obj(m)
}

/// The `cache.basis{last_call?, retention_class, elapsed_ms?, margin_ms,
/// cold_reason?}` projection record (ADR-0128 d.2).
pub fn expected_basis_json(b: &crate::cache::ExpectedBasis) -> Json {
    let mut m = std::collections::BTreeMap::new();
    if let Some(l) = &b.last_call {
        m.insert("last_call".into(), Json::str(l.clone()));
    }
    if let Some(r) = &b.retention_class {
        m.insert("retention_class".into(), Json::str(r.clone()));
    }
    if let Some(e) = b.elapsed_ms {
        m.insert("elapsed_ms".into(), Json::Int(e as i64));
    }
    m.insert("margin_ms".into(), Json::Int(b.margin_ms as i64));
    if let Some(r) = &b.cold_reason {
        m.insert("cold_reason".into(), Json::str(r.as_str()));
    }
    Json::Obj(m)
}

/// An `AssumptionDebtRecord`/`ProfileDebtRecord`'s canonical JSON
/// (`{rule_id, hypothesis, evidence_refs[], owner, expiry_condition{kind,
/// value?}, removal_test_ref, status}` — CF-049 / ADR-0124).
pub fn debt_json(d: &hh_compiler::profile::ProfileDebtRecord) -> Json {
    let mut e = std::collections::BTreeMap::new();
    e.insert("kind".into(), Json::str(d.expiry_condition.kind.name()));
    if let Some(v) = &d.expiry_condition.value {
        e.insert("value".into(), Json::str(v.clone()));
    }
    Json::obj([
        ("rule_id", Json::str(&d.rule_id)),
        ("hypothesis", Json::str(&d.hypothesis)),
        (
            "evidence_refs",
            Json::Arr(
                d.evidence_refs
                    .iter()
                    .map(|r| Json::str(r.clone()))
                    .collect(),
            ),
        ),
        ("owner", Json::str(&d.owner)),
        ("expiry_condition", Json::Obj(e)),
        ("removal_test_ref", Json::str(&d.removal_test_ref)),
        ("status", Json::str(d.status.name())),
    ])
}

/// `model.call.completed{model_call_id, usage{record{raw?, canonical{…},
/// view: TokenVector, normalizer_ref}, available, arrival}, cost?,
/// timing{latency_ms, attempts, ttft_ms?, queue_wait_ms, measured_at},
/// stop_reason, stop_details?, raw_stop_reason?, served_model?, snapshot_id?,
/// substitution, surface_ids{response_id}, cache_observation?,
/// credential_binding_id?}` — the terminal success row; the sole
/// usage/cost-provenance emission (ADR-0118 d.7).
#[allow(clippy::too_many_arguments)] // the arity is the payload's.
pub fn call_completed(
    model_call_id: &str,
    message: &crate::message::ModelMessage,
    usage: Option<&hh_telemetry::TokenVector>,
    usage_raw: Option<&Json>,
    arrival: Option<&str>,
    _estimate: Option<&crate::codec::Estimate>,
    cost: Option<&Json>,
    timing: &crate::grammar::Timing,
    cache: Option<&CachePlan>,
    observed: Option<&crate::cache::CacheObservation>,
    credential_binding_id: Option<&str>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("model_call_id".into(), Json::str(model_call_id));
    // `usage{record, available, arrival}` — `available = false` ⇒ `n/a`,
    // never zero (I3/R-ACC-2).
    let mut u = std::collections::BTreeMap::new();
    match usage {
        Some(v) => {
            u.insert("available".into(), Json::Bool(true));
            let mut rec = std::collections::BTreeMap::new();
            if let Some(raw) = usage_raw {
                rec.insert("raw".into(), raw.clone());
            }
            rec.insert("view".into(), v.to_json());
            rec.insert("normalizer_ref".into(), Json::str(v.normalizer_ref.clone()));
            u.insert("record".into(), Json::Obj(rec));
        }
        None => {
            u.insert("available".into(), Json::Bool(false));
        }
    }
    if let Some(a) = arrival {
        u.insert("arrival".into(), Json::str(a));
    }
    m.insert("usage".into(), Json::Obj(u));
    if let Some(c) = cost {
        m.insert("cost".into(), c.clone());
    }
    let mut t = std::collections::BTreeMap::new();
    t.insert("latency_ms".into(), Json::Int(timing.latency_ms as i64));
    t.insert("attempts".into(), Json::Int(timing.attempts as i64));
    if let Some(ttft) = timing.ttft_ms {
        t.insert("ttft_ms".into(), Json::Int(ttft as i64));
    }
    t.insert(
        "queue_wait_ms".into(),
        Json::Int(timing.queue_wait_ms as i64),
    );
    t.insert("measured_at".into(), Json::str(timing.measured_at.clone()));
    m.insert("timing".into(), Json::Obj(t));
    m.insert(
        "stop_reason".into(),
        Json::str(message.stop_reason.as_str()),
    );
    if let Some(d) = &message.stop_details {
        m.insert(
            "stop_details".into(),
            Json::obj([
                (
                    "categories",
                    Json::Arr(
                        d.categories
                            .iter()
                            .map(|c| {
                                let mut cm = std::collections::BTreeMap::new();
                                cm.insert("category".into(), Json::str(c.category.clone()));
                                if let Some(l) = &c.level {
                                    cm.insert("level".into(), Json::str(l.clone()));
                                }
                                Json::Obj(cm)
                            })
                            .collect(),
                    ),
                ),
                (
                    "explanation",
                    match &d.explanation {
                        Some(e) => Json::str(e.clone()),
                        None => Json::Null,
                    },
                ),
            ]),
        );
    }
    if let Some(raw) = &message.raw_stop_reason {
        m.insert("raw_stop_reason".into(), Json::str(raw.clone()));
    }
    if let Some(served) = &message.served_model {
        m.insert("served_model".into(), Json::str(served.clone()));
    }
    if let Some(snap) = &message.snapshot_id {
        m.insert("snapshot_id".into(), Json::str(snap.clone()));
    }
    if !message.surface_ids.is_empty() {
        m.insert(
            "surface_ids".into(),
            Json::Obj(
                message
                    .surface_ids
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        );
    }
    if let Some(c) = cache {
        let mut cm = std::collections::BTreeMap::new();
        if let Some(e) = &c.expected {
            cm.insert("expected_state".into(), Json::str(e.expected.as_str()));
        }
        if let Some(o) = observed {
            cm.insert("observed_state".into(), Json::str(o.observed.as_str()));
            cm.insert("cache_read".into(), Json::Int(o.read));
            if let Some(h) = o.hit_ratio_ppm {
                cm.insert("hit_ratio_ppm".into(), Json::Int(h));
            }
            if let Some(w) = o.write_ratio_ppm {
                cm.insert("write_ratio_ppm".into(), Json::Int(w));
            }
            if !o.write_by_class.is_empty() {
                cm.insert(
                    "write_by_class".into(),
                    Json::Obj(
                        o.write_by_class
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Int(*v)))
                            .collect(),
                    ),
                );
            }
            if let Some(e) = &c.expected {
                cm.insert(
                    "agreement".into(),
                    Json::Bool(crate::cache::cache_agreement(e.expected, o.observed)),
                );
            }
        }
        if let Some(a) = &c.affinity_full {
            cm.insert("affinity_key".into(), Json::str(a.clone()));
        }
        if let Some(s) = &c.static_hash {
            cm.insert("static_hash".into(), Json::str(s.clone()));
        }
        if !c.dropped.is_empty() {
            cm.insert(
                "markers_dropped".into(),
                Json::Arr(
                    c.dropped
                        .iter()
                        .map(|d| {
                            Json::obj([
                                ("position", Json::str(d.position.as_str())),
                                ("reason", Json::str(d.reason.as_str())),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        if !c.substituted.is_empty() {
            cm.insert(
                "markers_substituted".into(),
                Json::Arr(
                    c.substituted
                        .iter()
                        .map(|s| {
                            Json::obj([
                                ("requested", Json::str(s.requested.as_str())),
                                ("substituted_index", Json::Int(s.substituted_index as i64)),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        m.insert("cache_observation".into(), Json::Obj(cm));
    }
    m.insert(
        "substitution".into(),
        Json::str(if message.served_model.is_some() {
            "provider_fallback"
        } else {
            "none"
        }),
    );
    if let Some(b) = credential_binding_id {
        m.insert("credential_binding_id".into(), Json::str(b));
    }
    Json::Obj(m)
}

/// `model.call.failed{model_call_id, error{…}, usage?, timing{…},
/// credential_binding_id?}` — the terminal failure row (the classified
/// `ModelError`; the raw frame's address is `raw_ref`, never the bytes).
pub fn call_failed(
    model_call_id: &str,
    error: &ModelError,
    usage: Option<&hh_telemetry::TokenVector>,
    timing: &crate::grammar::Timing,
    credential_binding_id: Option<&str>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("model_call_id".into(), Json::str(model_call_id));
    m.insert("error".into(), error.to_json());
    if let Some(v) = usage {
        m.insert("usage".into(), v.to_json());
    }
    let mut t = std::collections::BTreeMap::new();
    t.insert("latency_ms".into(), Json::Int(timing.latency_ms as i64));
    t.insert("attempts".into(), Json::Int(timing.attempts as i64));
    if let Some(ttft) = timing.ttft_ms {
        t.insert("ttft_ms".into(), Json::Int(ttft as i64));
    }
    t.insert(
        "queue_wait_ms".into(),
        Json::Int(timing.queue_wait_ms as i64),
    );
    t.insert("measured_at".into(), Json::str(timing.measured_at.clone()));
    m.insert("timing".into(), Json::Obj(t));
    if let Some(b) = credential_binding_id {
        m.insert("credential_binding_id".into(), Json::str(b));
    }
    Json::Obj(m)
}

/// `model.call.attempt.started{model_call_id, attempt_no, queue_wait_ms?}` —
/// one per attempt (R-RT-3/R-RT-6 — the span carries `queue_wait_ms`).
pub fn attempt_started(model_call_id: &str, attempt_no: u32, queue_wait_ms: u64) -> Json {
    Json::obj([
        ("model_call_id", Json::str(model_call_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("queue_wait_ms", Json::Int(queue_wait_ms as i64)),
    ])
}

/// `model.call.attempt.completed{model_call_id, attempt_no, duration_ms,
/// served_model?}` — an attempt's transport success.
pub fn attempt_completed(
    model_call_id: &str,
    attempt_no: u32,
    duration_ms: u64,
    served_model: Option<&str>,
) -> Json {
    Json::obj([
        ("model_call_id", Json::str(model_call_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("duration_ms", Json::Int(duration_ms as i64)),
        (
            "served_model",
            match served_model {
                Some(s) => Json::str(s),
                None => Json::Null,
            },
        ),
    ])
}

/// `model.call.attempt.failed{model_call_id, attempt_no, error{…},
/// will_retry, next_delay_ms?}` — an attempt's classified failure with the
/// retry decision (R-RT-1…4 read `will_retry`/`next_delay_ms`).
pub fn attempt_failed(
    model_call_id: &str,
    attempt_no: u32,
    error: &ModelError,
    will_retry: bool,
    next_delay_ms: Option<u64>,
) -> Json {
    Json::obj([
        ("model_call_id", Json::str(model_call_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("error", error.to_json()),
        ("will_retry", Json::Bool(will_retry)),
        (
            "next_delay_ms",
            match next_delay_ms {
                Some(d) => Json::Int(d as i64),
                None => Json::Null,
            },
        ),
    ])
}

/// `model.stream.delta{model_call_id, attempt_no, seq_in_attempt,
/// block_index?, kind}` — ephemeral: the streamed delta fact for
/// subscribers, never durable (G5 — the message reconstructs from
/// `block.completed` alone).
pub fn stream_delta(
    model_call_id: &str,
    attempt_no: u32,
    seq_in_attempt: u32,
    block_index: Option<u32>,
    kind: &str,
) -> Json {
    Json::obj([
        ("model_call_id", Json::str(model_call_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("seq_in_attempt", Json::Int(seq_in_attempt as i64)),
        (
            "block_index",
            match block_index {
                Some(i) => Json::Int(i as i64),
                None => Json::Null,
            },
        ),
        ("kind", Json::str(kind)),
    ])
}

/// `model.route.decided{RoutingDecision}` — the C0 route record (§5b.2):
/// `{decision_id, model_call_id, role, selected: ModelRef{profile_ref,
/// provider_model_id, serving_route, effort?}, reservation_id, policy_ref{
/// variant_ref, version_id}, rule_ids_fired[], candidates_considered[{
/// model_ref, verdict, score?}], inputs_read[], deviation, relower_required}`.
pub fn route_decided(d: &crate::router::RoutingDecision) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("decision_id".into(), Json::str(d.decision_id.clone()));
    m.insert(
        "model_call_id".into(),
        match &d.model_call_id {
            Some(id) => Json::str(id.clone()),
            None => Json::Null,
        },
    );
    m.insert("role".into(), Json::str(d.role.clone()));
    m.insert("selected".into(), d.selected.to_json());
    m.insert(
        "reservation_id".into(),
        match &d.reservation_id {
            Some(r) => Json::str(r.clone()),
            None => Json::Null,
        },
    );
    m.insert(
        "policy_ref".into(),
        Json::obj([
            ("variant_ref", Json::str(d.policy_ref.clone())),
            ("version_id", Json::str(d.policy_version_id.clone())),
        ]),
    );
    m.insert(
        "rule_ids_fired".into(),
        Json::Arr(
            d.rule_ids_fired
                .iter()
                .map(|r| Json::str(r.clone()))
                .collect(),
        ),
    );
    m.insert(
        "candidates_considered".into(),
        Json::Arr(
            d.candidates_considered
                .iter()
                .map(|c| {
                    let mut cm = std::collections::BTreeMap::new();
                    cm.insert("model_ref".into(), Json::str(c.model_ref.clone()));
                    cm.insert("verdict".into(), Json::str(c.verdict.spelling()));
                    if let Some(s) = c.score {
                        cm.insert("score".into(), Json::Int(s));
                    }
                    Json::Obj(cm)
                })
                .collect(),
        ),
    );
    m.insert(
        "inputs_read".into(),
        Json::Arr(d.inputs_read.iter().map(|i| Json::str(i.clone())).collect()),
    );
    m.insert("deviation".into(), Json::Bool(d.deviation));
    m.insert("relower_required".into(), Json::Bool(d.relower_required));
    Json::Obj(m)
}

/// `model.cache.resolved{cache_kind, key: ContentAddress, scope, outcome,
/// reason, entry_ref?, verifier_verdict?, avoided{…}, attribution}` — one per
/// K2–K6 lookup (ADR-0128 d.3; K1 has no such event).
#[allow(clippy::too_many_arguments)] // the arity is the payload's.
pub fn cache_resolved(
    cache_kind: CacheKind,
    key: &str,
    scope: &str,
    outcome: CacheOutcome,
    reason: Option<&crate::cache::MissReason>,
    entry_ref: Option<&str>,
    avoided: Option<&Json>,
    attribution: &str,
    purpose: &Purpose,
    lookup_duration_ms: u64,
    served_by: Option<&str>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("cache_kind".into(), Json::str(cache_kind.as_str()));
    m.insert("key".into(), Json::str(key));
    m.insert("scope".into(), Json::str(scope));
    m.insert("outcome".into(), Json::str(outcome.as_str()));
    m.insert(
        "reason".into(),
        match reason {
            Some(r) => Json::str(r.as_str()),
            None => Json::Null,
        },
    );
    if let Some(e) = entry_ref {
        m.insert("entry_ref".into(), Json::str(e));
    }
    if let Some(a) = avoided {
        m.insert("avoided".into(), a.clone());
    }
    m.insert("attribution".into(), Json::str(attribution));
    m.insert("purpose".into(), Json::str(purpose.as_str()));
    m.insert(
        "lookup_duration_ms".into(),
        Json::Int(lookup_duration_ms as i64),
    );
    m.insert(
        "served_by".into(),
        match served_by {
            Some(s) => Json::str(s),
            None => Json::Null,
        },
    );
    Json::Obj(m)
}

/// `model.rerouted{model_call_id, from, to, reason, attempt_no, relowered,
/// relower_event_ref?, decision_ref}` (ADR-0122 d.3).
#[allow(clippy::too_many_arguments)] // the arity is the payload's.
pub fn rerouted(
    model_call_id: &str,
    from_model_ref: &str,
    to_model_ref: &str,
    reason: &RerouteReason,
    attempt_no: u32,
    relowered: bool,
    relower_event_ref: Option<&str>,
    decision_ref: &str,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("model_call_id".into(), Json::str(model_call_id));
    m.insert("from".into(), Json::str(from_model_ref));
    m.insert("to".into(), Json::str(to_model_ref));
    m.insert("reason".into(), Json::str(reason.as_str()));
    m.insert("attempt_no".into(), Json::Int(attempt_no as i64));
    m.insert("relowered".into(), Json::Bool(relowered));
    if let Some(r) = relower_event_ref {
        m.insert("relower_event_ref".into(), Json::str(r));
    }
    m.insert("decision_ref".into(), Json::str(decision_ref));
    Json::Obj(m)
}

/// `model.profile.status.changed{profile_ref, rule_id?, from, to, trigger,
/// evidence_ref}` — the status state machine's transitions (ADR-0126 d.3;
/// `causes[]` is an envelope member).
pub fn profile_status_changed(
    profile_ref: &str,
    rule_id: Option<&str>,
    from: &str,
    to: &str,
    trigger: &str,
    evidence_ref: Option<&str>,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("profile_ref".into(), Json::str(profile_ref));
    if let Some(r) = rule_id {
        m.insert("rule_id".into(), Json::str(r));
    }
    m.insert("from".into(), Json::str(from));
    m.insert("to".into(), Json::str(to));
    m.insert("trigger".into(), Json::str(trigger));
    if let Some(e) = evidence_ref {
        m.insert("evidence_ref".into(), Json::str(e));
    }
    Json::Obj(m)
}

/// `model.profile.probed{profile_ref, probe_run_id, records[]}` — a profile
/// probe's `ConformanceRecord`s (Stage-3 wiring; the class is registered now
/// so the schema is sealed).
pub fn profile_probed(profile_ref: &str, probe_run_id: &str, records: &[Json]) -> Json {
    Json::obj([
        ("profile_ref", Json::str(profile_ref)),
        ("probe_run_id", Json::str(probe_run_id)),
        ("records", Json::Arr(records.to_vec())),
    ])
}

/// `model.profile.expired_used{profile_ref, intent_ref}` — appended to any
/// run compiled under `expired` (ADR-0126 d.6; the intent is recorded, the
/// rows are excluded from headline claims).
pub fn profile_expired_used(profile_ref: &str, intent_ref: &str) -> Json {
    Json::obj([
        ("profile_ref", Json::str(profile_ref)),
        ("intent_ref", Json::str(intent_ref)),
    ])
}
