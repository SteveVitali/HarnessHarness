//! S3.8 eval evidence — the §5c.2 Stage-3 trace predicates
//! (`reacquisition`, `repeated_action`, `recall_probe`) over transcript
//! rows, plus the `context_metrics` fold (AC-R-2.4.2-9, AC-R-2.4.3-9,
//! AC-R-2.4.4-11/-12).

use hh_eval::catalogue;
use hh_eval::context_metrics;
use hh_eval::oracle::{run_oracle, trace_predicate, OracleRequest};
use hh_ontology::eval::LatticeValue;
use hh_ontology::eval::OracleClass;

fn trace_oracle() -> hh_ontology::eval::OracleDeclaration {
    catalogue::oracle_declarations()
        .into_iter()
        .find(|o| o.class == OracleClass::TracePredicate)
        .expect("the catalogue declares a trace_predicate oracle")
}

fn req(criterion: &str, transcript: &str) -> OracleRequest {
    OracleRequest {
        oracle: trace_oracle(),
        criterion: criterion.into(),
        expected: None,
        actual: vec![],
        transcript: Some(transcript.as_bytes().to_vec()),
    }
}

/// A compaction-completed row forgetting `item:a` at seq 5.
fn compaction_row() -> &'static str {
    r#"{"seq":5,"class":"context.compaction.completed","payload":{"variant_ref":"hh/evict-oldest@1","status":"applied","forgotten":["item:a"],"tokens_freed":100}}"#
}

#[test]
fn trace_predicate_inapplicable_without_evidence() {
    // No compaction evidence at all → I (inapplicable), never a false C.
    let t = r#"[{"seq":1,"class":"lifecycle.run.created","payload":{}}]"#;
    assert_eq!(
        trace_predicate("reacquisition", t.as_bytes()),
        Some(LatticeValue::I)
    );
    assert_eq!(
        trace_predicate("trace_predicate:repeated_action", t.as_bytes()),
        Some(LatticeValue::I)
    );
    assert_eq!(
        trace_predicate("recall_probe", t.as_bytes()),
        Some(LatticeValue::I)
    );
}

#[test]
fn reacquisition_flags_a_forgotten_item_retrieved_again() {
    let t = format!(
        "[{},{},{}]",
        compaction_row(),
        // A later memory.read delivers the forgotten item — damage.
        r#"{"seq":7,"class":"context.memory.read","payload":{"delivered":["item:a"],"withheld":[]}}"#,
        // An earlier read predating the compaction is not reacquisition.
        r#"{"seq":2,"class":"context.memory.read","payload":{"delivered":["item:z"],"withheld":[]}}"#
    );
    assert_eq!(
        trace_predicate("reacquisition", t.as_bytes()),
        Some(LatticeValue::C)
    );
    // Remove the offending read → N.
    let t = format!(
        "[{},{}]",
        compaction_row(),
        r#"{"seq":7,"class":"context.memory.read","payload":{"delivered":["item:b"],"withheld":[]}}"#
    );
    assert_eq!(
        trace_predicate("reacquisition", t.as_bytes()),
        Some(LatticeValue::N)
    );
}

#[test]
fn repeated_action_flags_a_duplicate_completion_across_compaction() {
    // The same (capability, args) completes at 3 and 8 with a compaction at
    // 5 between them — the earlier result was forgotten, the action repeated.
    let t = format!(
        "[{},{},{}]",
        r#"{"seq":3,"class":"action.tool.completed","payload":{"capability":"tool:shell","args":{"cmd":"ls"}}}"#,
        compaction_row(),
        r#"{"seq":8,"class":"action.tool.completed","payload":{"capability":"tool:shell","args":{"cmd":"ls"}}}"#
    );
    assert_eq!(
        trace_predicate("repeated_action", t.as_bytes()),
        Some(LatticeValue::C)
    );
    // Both completions before the compaction → no repeat across a boundary.
    let t = format!(
        "[{},{},{}]",
        r#"{"seq":1,"class":"action.tool.completed","payload":{"capability":"tool:shell","args":{"cmd":"ls"}}}"#,
        r#"{"seq":2,"class":"action.tool.completed","payload":{"capability":"tool:shell","args":{"cmd":"ls"}}}"#,
        compaction_row()
    );
    assert_eq!(
        trace_predicate("repeated_action", t.as_bytes()),
        Some(LatticeValue::N)
    );
}

#[test]
fn recall_probe_measures_forgotten_presence_in_probe_assemblies() {
    // A probe-purpose call whose assembly still carries the forgotten item →
    // hit rate 100% → N (the recall probe succeeded); a probe missing it → C.
    let t = format!(
        "[{},{},{}]",
        compaction_row(),
        r#"{"seq":6,"class":"model.call.requested","payload":{"model_call_id":"mc:probe","cache":{"purpose":"probe"}}}"#,
        r#"{"seq":7,"class":"context.assembled","payload":{"model_call_id":"mc:probe","items":[{"kind":"memory","artefact_id":"item:a","tokens":10}]}}"#
    );
    assert_eq!(
        trace_predicate("recall_probe", t.as_bytes()),
        Some(LatticeValue::N)
    );
    let t = format!(
        "[{},{},{}]",
        compaction_row(),
        r#"{"seq":6,"class":"model.call.requested","payload":{"model_call_id":"mc:probe","cache":{"purpose":"probe"}}}"#,
        r#"{"seq":7,"class":"context.assembled","payload":{"model_call_id":"mc:probe","items":[{"kind":"memory","artefact_id":"item:b","tokens":10}]}}"#
    );
    assert_eq!(
        trace_predicate("recall_probe", t.as_bytes()),
        Some(LatticeValue::C)
    );
}

#[test]
fn run_oracle_dispatches_the_predicates() {
    // The criterion routes through the oracle boundary — typed verdicts, not
    // strings.
    let t = format!(
        "[{},{}]",
        compaction_row(),
        r#"{"seq":7,"class":"context.memory.read","payload":{"delivered":["item:a"],"withheld":[]}}"#
    );
    let v = run_oracle(&req("trace_predicate:reacquisition", &t)).unwrap();
    assert_eq!(v.verdict, LatticeValue::C);
    // Missing transcript is a typed failure, not a false verdict.
    let mut r = req("reacquisition", &t);
    r.transcript = None;
    assert!(run_oracle(&r).is_err());
}

#[test]
fn unknown_criterion_falls_through_to_the_verdict_event_contract() {
    // A non-predicate criterion keeps the declared `"<criterion>":true`
    // contract.
    let v = run_oracle(&req("output_matches", r#"{"output_matches":true}"#)).unwrap();
    assert_eq!(v.verdict, LatticeValue::N);
    let v = run_oracle(&req("output_matches", r#"{"output_matches":false}"#)).unwrap();
    assert_eq!(v.verdict, LatticeValue::C);
}

// ── context_metrics fold ─────────────────────────────────────────────────────

#[test]
fn context_metrics_fold_counts_the_compliance_chain() {
    let rows = format!(
        "[{},{},{},{},{}]",
        r#"{"seq":1,"class":"context.artefact.delivered","payload":{"artefact_id":"mem:1","delivery_id":"d1","kind":"memory","by_reference":true}}"#,
        r#"{"seq":2,"class":"context.artefact.activated","payload":{"artefact_id":"mem:1","delivery_id":"d1","signal":"tool_used"}}"#,
        r#"{"seq":3,"class":"verification.artefact.followed","payload":{"artefact_id":"mem:1","delivery_id":"d1"}}"#,
        r#"{"seq":4,"class":"context.memory.written","payload":{"version_id":"v9","label":{"authority":"promoted_endorsed"}}}"#,
        r#"{"seq":5,"class":"context.memory.read","payload":{"delivered":["v1"],"withheld":["v2"]}}"#
    );
    let j = hh_wire::json::parse(&rows).unwrap();
    let Json::Arr(rs) = j else { panic!() };
    let rows: Vec<hh_eval::facts::FactRow> = rs
        .iter()
        .map(|r| hh_eval::facts::FactRow {
            seq: r.get("seq").and_then(Json::as_int).unwrap() as u64,
            event_id: None,
            class: r.get("class").and_then(Json::as_str).unwrap().to_string(),
            payload: r.get("payload").cloned().unwrap(),
        })
        .collect();
    let facts = hh_eval::facts::LedgerFacts::from_rows(&rows);
    let m = context_metrics::fold(&facts);
    assert_eq!(m.memory_delivered, 2, "one artefact + one read delivery");
    assert_eq!(m.memory_activated, 1);
    assert_eq!(m.memory_followed, 1);
    assert_eq!(m.memory_promoted, 1);
    // v2 withheld with no invalidation evidence → over-invalidation.
    assert_eq!(m.memory_withheld, 1);
    assert_eq!(m.memory_over_invalidation, 1);
    // Derived rates (AC-R-2.4.4-11, AC-R-2.4.3-9): validity = delivered /
    // (delivered + withheld) = 2/3; P(activated|delivered) = 1/2.
    assert_eq!(m.memory_validity_rate, Some(666_666));
    assert_eq!(m.memory_activated_given_delivered, Some(500_000));
    let emitted: std::collections::BTreeMap<_, _> = context_metrics::emit(&m).into_iter().collect();
    assert_eq!(emitted["memory.validity_rate"], 666_666);
    assert_eq!(emitted["memory.activated_given_delivered"], 500_000);
    // The catalogue declares every emitted row.
    for name in emitted.keys() {
        assert!(
            hh_eval::catalogue::metric(name).is_some(),
            "catalogue misses {name}"
        );
    }
}

use hh_wire::json::Json;
