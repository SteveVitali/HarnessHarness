//! The S6.3b battery (§5h.7 §5f.4 §8.1; R-2.9.7, R-2.7.3⁴, R-2.12.1⁴)
//! — the designed-attribution link gates (S2 `attribution_ref` /
//! `TargetMismatch`, S5's `hh-attribution/1` label gate), the judge-
//! integrity legs (`JudgeLeakedIntoArtifact`, the monitor selector
//! protocols — `untrusted_unmonitored` refused, deterministic-first
//! assignment, veto-only), and the lineage DAG attribution
//! (R-2.12.1⁴).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::matchspec::{MatchMode, MatchSpec};
use hh_evolution::campaign::{doc_kind, EvolutionCampaign};
use hh_evolution::errors::{EvolutionError, Refusal};
use hh_evolution::lineage::{attribute_lineage, LineageDag};
use hh_evolution::records::{
    AcceptanceItem, CandidateProposal, CorpusSpec, EvolutionAcceptanceReport,
    EvolutionCampaignSpec, FailureHypothesis, ItemStatus, JudgePolicy, PortabilityLabel,
    PredictedDelta, PredictedEffect, ScreenReport, SelectorDeclaration, SlotAllocation, StopRule,
};
use hh_experiment::docs::LabDocs;
use hh_hir::diff::{self, DiffDerivation, HirDiff};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_identity::idp::idp_id;
use hh_lab::bench::SplitAssignmentRecord;
use hh_ledger::ids::ManualClock;
use hh_ledger::store::{Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::lab::{EnvironmentFamily, SplitLabel};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

// ── fixture plumbing (the s6_1a shape) ──────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-evolution-s63b-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn rig(tag: &str) -> (PathBuf, ManualClock, Store, LabDocs) {
    let root = tmp(tag);
    let clock = ManualClock::at(1_000);
    let store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    (root, clock, store, docs)
}

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn rule_node(id: &str, seq: u64, trigger: Json) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            seq,
        ),
        id,
    )
}

fn budget_node(id: &str, seq: u64) -> Node {
    let dimensions = BTreeMap::from([(
        "tokens.blended".to_string(),
        DimensionBound {
            hard: Some(1_000),
            soft: None,
        },
    )]);
    sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions,
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            seq,
        ),
        id,
    )
}

fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel(holder),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            seq,
        ),
        id,
    )
}

fn agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
    let mut slots = BTreeMap::new();
    for name in ["control_strategy", "context_policy"] {
        slots.insert(
            name.to_string(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::pinned(
                name,
                format!("hh/{name}/test"),
                format!("sha256:variant-{name}"),
            ))),
        );
    }
    sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel(id),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel(budget),
                    permissions: sel(perm),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            seq,
        ),
        id,
    )
}

fn base_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node(
        "test:rule",
        1,
        Json::obj([("on", Json::str("turn_end"))]),
    ));
    doc.nodes.push(budget_node("test:budget", 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

fn target_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes.push(rule_node(
        "test:rule",
        1,
        Json::obj([("on", Json::str("turn_start"))]),
    ));
    doc.nodes.push(budget_node("test:budget", 2));
    doc.nodes.push(perm_node("test:perm", "test:agent", 3));
    doc.nodes
        .push(agent_node("test:agent", "test:budget", "test:perm", 4));
    doc
}

fn candidate_diff(base: &HirDocument, target: &HirDocument) -> HirDiff {
    match diff::diff(
        base,
        target,
        prov(10),
        DiffDerivation {
            hypothesis: None,
            trajectories: vec![],
            candidate_id: None,
        },
    ) {
        Ok(d) => d,
        Err(errs) => panic!("fixture diff failed: {errs:?}"),
    }
}

fn deposit_split(docs: &LabDocs, splits: &[(&str, SplitLabel)]) -> String {
    let rec = SplitAssignmentRecord {
        suite_id: "suite:test".into(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: splits.iter().map(|(t, l)| (t.to_string(), *l)).collect(),
        split_hash: idp_id("test.split", b"fixture").to_string(),
        registered_at: 1,
    };
    docs.put_named(doc_kind::SPLIT, "test-split", &rec.to_json())
        .unwrap();
    "test-split".to_string()
}

fn spec(split_ref: &str) -> EvolutionCampaignSpec {
    EvolutionCampaignSpec {
        campaign_id: String::new(),
        protocol: "human_proposed".into(),
        base_definition_ref: "def:base".into(),
        service_definition_ref: "def:evolution-service".into(),
        corpus: CorpusSpec {
            environments: vec![EnvironmentFamily::CodingTerminal],
            suite_refs: vec!["suite:test".into()],
            metric_refs: vec!["metric:success".into(), "metric:latency".into()],
            task_ids: vec!["task:a".into(), "task:b".into()],
            evidence_refs: vec!["ev:1".into(), "ev:2".into()],
            split_assignment_ref: split_ref.into(),
            layers: None,
        },
        exclusion_targets: vec![],
        must_code_targets: vec![],
        allowed_target_kinds: None,
        semantic_ops_bound: 8,
        min_flip_share_ppm: 500_000,
        min_replicates: 1,
        retention_margin_ppm: 50_000,
        veto_metrics: vec!["metric:safety".into()],
        slot_allocation: SlotAllocation::Uniform { min_share_ppm: 0 },
        stop_rule: StopRule {
            max_candidates: Some(10),
            stagnation_window: Some(5),
            budget_cap_ref: None,
            no_addressable_failure: true,
        },
        maturity_flags: vec![],
        proposer_family: "human".into(),
        hosted_participants: false,
        reported_only_dimensions: vec![],
        proposer_variant_ref: None,
        target_class: None,
        target_classes: Vec::new(),
        hosted_coordinates: Vec::new(),
        hosted_descriptor_refs: Vec::new(),
        authority_cap: None,
        rollout_policy: None,
        judge_policy: None,
    }
}

fn open(tag: &str) -> (PathBuf, ManualClock, Store, LabDocs, EvolutionCampaign) {
    let (root, clock, mut store, docs) = rig(tag);
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::Search)],
    );
    let s = spec(&split_ref);
    let (_, eng) = EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s)
        .unwrap_or_else(|e| panic!("campaign open: {e}"));
    (root, clock, store, docs, eng)
}

fn refusal(e: &EvolutionError) -> &Refusal {
    match e {
        EvolutionError::Refusal(r) => r,
        o => panic!("expected a typed refusal, got {o:?}"),
    }
}

fn propose_ok(
    store: &mut Store,
    eng: &mut EvolutionCampaign,
    base: &HirDocument,
    target: &HirDocument,
) -> String {
    let d = candidate_diff(base, target);
    eng.propose(
        store,
        &CandidateProposal {
            base_ref: "def:base".into(),
            diff: d,
            slot: "control_strategy".into(),
            hypothesis: None,
            install: None,
            coordinate_values: Default::default(),
        },
        base,
    )
    .unwrap_or_else(|e| panic!("propose: {e}"))
}

fn classified_candidate(
    tag: &str,
) -> (
    PathBuf,
    ManualClock,
    Store,
    LabDocs,
    EvolutionCampaign,
    String,
) {
    let (root, clock, mut store, docs, mut eng) = open(tag);
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    (root, clock, store, docs, eng, cid)
}

fn hypothesis() -> FailureHypothesis {
    FailureHypothesis {
        kind: "failure".into(),
        evidence_refs: vec!["ev:1".into()],
        predicted: PredictedEffect {
            deltas: vec![PredictedDelta {
                metric: "metric:success".into(),
                direction: "increase".into(),
            }],
            affected_task_ids: vec!["task:a".into()],
            model_scope: "same_snapshot".into(),
            horizon: None,
        },
        semantic_op_targets: vec![],
        reference_trajectories: Vec::new(),
        attribution_ref: None,
    }
}

/// A deposited `hh-attribution-design/1` naming `targets[]`.
fn deposit_design(docs: &LabDocs, name: &str, targets: &[&str]) -> String {
    let j = Json::obj([
        ("schema", Json::str("hh-attribution-design/1")),
        ("method", Json::str("M2")),
        (
            "targets",
            Json::Arr(
                targets
                    .iter()
                    .map(|t| Json::obj([("kind", Json::str("rule")), ("ref", Json::str(*t))]))
                    .collect(),
            ),
        ),
    ]);
    docs.put_named(doc_kind::ATTRIBUTION_DESIGN, name, &j)
        .unwrap();
    name.to_string()
}

/// A deposited `hh-attribution/1` report body.
fn deposit_report(docs: &LabDocs, name: &str, design_ref: &str, label: &str) -> String {
    let j = Json::obj([
        ("schema", Json::str("hh-attribution/1")),
        ("kind", Json::str("attribution")),
        ("design_ref", Json::str(design_ref)),
        ("attribution_label", Json::str(label)),
        ("label", Json::str("exploratory")),
        (
            "effects",
            Json::Arr(vec![Json::obj([
                ("target", Json::str("test:rule")),
                ("point", Json::Int(50_000)),
            ])]),
        ),
    ]);
    docs.put_named(doc_kind::ATTRIBUTION_REPORT, name, &j)
        .unwrap();
    name.to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. S2 — the designed-attribution link (§5h.7; R-2.9.7).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn s2_designed_claim_without_attribution_ref_refuses() {
    let (_r, _c, mut store, _docs, mut eng, cid) = classified_candidate("s2a");
    let mut h = hypothesis();
    // The designed claim — the diff's semantic op on `test:rule` must
    // name a design (KA-I7-12's label-gate leg).
    h.semantic_op_targets = vec!["test:rule".into()];
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::AttributionMissing { .. }));
    assert_eq!(eng.view.state_of(&cid), Some("rejected"));
}

#[test]
fn s2_attribution_design_must_resolve_and_cover_targets() {
    // Unresolvable ref → AttributionReportInvalid.
    let (_r, _c, mut store, _docs, mut eng, cid) = classified_candidate("s2b1");
    let mut h = hypothesis();
    h.semantic_op_targets = vec!["test:rule".into()];
    h.attribution_ref = Some("design:nope".into());
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::AttributionReportInvalid { .. }
    ));

    // Wrong schema member → AttributionReportInvalid.
    let (_r, _c, mut store, docs, mut eng, cid) = classified_candidate("s2b2");
    docs.put_named(
        doc_kind::ATTRIBUTION_DESIGN,
        "design:bad",
        &Json::obj([("schema", Json::str("not-a-design"))]),
    )
    .unwrap();
    let mut h = hypothesis();
    h.semantic_op_targets = vec!["test:rule".into()];
    h.attribution_ref = Some("design:bad".into());
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::AttributionReportInvalid { .. }
    ));

    // Design whose targets do not cover the semantic op → TargetMismatch.
    let (_r, _c, mut store, docs, mut eng, cid) = classified_candidate("s2b3");
    let aref = deposit_design(&docs, "design:partial", &["test:other"]);
    let mut h = hypothesis();
    h.semantic_op_targets = vec!["test:rule".into(), "test:other".into()];
    h.attribution_ref = Some(aref);
    let e = eng.hypothesize(&mut store, &cid, &h).unwrap_err();
    assert!(matches!(refusal(&e), Refusal::TargetMismatch { .. }));
}

#[test]
fn s2_attribution_ref_binds_and_mints_the_row_member() {
    let (_r, _c, mut store, docs, mut eng, cid) = classified_candidate("s2c");
    let aref = deposit_design(&docs, "design:ok", &["test:rule"]);
    let mut h = hypothesis();
    h.semantic_op_targets = vec!["test:rule".into()];
    h.attribution_ref = Some(aref.clone());
    eng.hypothesize(&mut store, &cid, &h)
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    assert_eq!(eng.view.state_of(&cid), Some("hypothesized"));
    // The durable `hypothesized` row carries `attribution_ref` — the
    // hypothesis→design link the S5 gate re-reads.
    let row = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .rev()
        .find(|e| {
            e.class == "measurement.evolution.candidate.transitioned"
                && e.payload.get("to").and_then(Json::as_str) == Some("hypothesized")
        })
        .expect("hypothesized row");
    assert_eq!(
        row.payload.get("attribution_ref").and_then(Json::as_str),
        Some(aref.as_str())
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Judge integrity (§5f.4, §8.1; R-2.7.3⁴).
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn monitor_selector_untrusted_unmonitored_refused_at_open() {
    let mut s = spec("test-split");
    s.judge_policy = Some(JudgePolicy {
        selectors: vec![SelectorDeclaration {
            selector_ref: "mon:wild".into(),
            kind: "monitor".into(),
            calibration_ref: "cal:m".into(),
            independent_of: vec!["def:base".into()],
            adversarial: Some("untrusted_unmonitored".into()),
            held_out_from: Vec::new(),
        }],
        audit_budget_ref: "budget:audit".into(),
        audited_share_ppm: 100_000,
        min_honeypots: 3,
        artifact_benefit_selector: None,
    });
    let (_r, _c, mut store, docs) = rig("mon-wild");
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::Search)],
    );
    s.corpus.split_assignment_ref = split_ref;
    let e = match EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s) {
        Err(e) => e,
        Ok(_) => panic!("expected MonitorSelectorInadmissible"),
    };
    assert!(matches!(
        refusal(&e),
        Refusal::MonitorSelectorInadmissible { .. }
    ));
}

#[test]
fn monitor_veto_requires_the_deterministic_first_assignment() {
    let mut s = spec("test-split");
    s.judge_policy = Some(JudgePolicy {
        selectors: vec![SelectorDeclaration {
            selector_ref: "mon:a".into(),
            kind: "monitor".into(),
            calibration_ref: "cal:m".into(),
            independent_of: vec!["def:base".into()],
            adversarial: Some("trusted_weaker".into()),
            held_out_from: Vec::new(),
        }],
        audit_budget_ref: "budget:audit".into(),
        audited_share_ppm: 100_000,
        min_honeypots: 3,
        artifact_benefit_selector: None,
    });
    let (_r, _c, mut store, docs) = rig("mon-veto");
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::Search)],
    );
    s.corpus.split_assignment_ref = split_ref;
    let (_, mut eng) =
        EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s).unwrap();
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);

    // A verdict with no prior assignment never counts.
    let e = eng
        .monitor_veto(&mut store, &cid, "mon:a", "early verdict")
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::MonitorSelectorInadmissible { .. }
    ));

    // The undeclared selector cannot assign either.
    let e = eng
        .monitor_assign(&mut store, &cid, "mon:undeclared")
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::MonitorSelectorInadmissible { .. }
    ));

    // Assign, then veto — the veto row mints beside `rejected`.
    eng.monitor_assign(&mut store, &cid, "mon:a")
        .unwrap_or_else(|e| panic!("monitor_assign: {e}"));
    let e = eng
        .monitor_veto(&mut store, &cid, "mon:a", "the monitor disagrees")
        .unwrap_err();
    assert!(matches!(refusal(&e), Refusal::MonitorVetoed { .. }));
    assert_eq!(eng.view.state_of(&cid), Some("rejected"));
    let classes: Vec<&str> = store
        .events(&eng.run_id)
        .unwrap()
        .iter()
        .filter(|ev| ev.payload.get("candidate_id").and_then(Json::as_str) == Some(cid.as_str()))
        .map(|ev| ev.class.as_str())
        .collect();
    assert!(classes.contains(&"measurement.evolution.monitor.assigned"));
    assert!(classes.contains(&"measurement.evolution.monitor.veto"));
}

#[test]
fn s5_judge_leaked_into_artifact_refuses() {
    // The campaign declares an `artifact_benefit` judge selector that is
    // held out from the candidate's producer — naming it at S5 leaks.
    let mut s = spec("test-split");
    s.judge_policy = Some(JudgePolicy {
        selectors: vec![],
        audit_budget_ref: "budget:audit".into(),
        audited_share_ppm: 0,
        min_honeypots: 0,
        artifact_benefit_selector: Some(SelectorDeclaration {
            selector_ref: "sel:benefit".into(),
            kind: "judge".into(),
            calibration_ref: "cal:b".into(),
            independent_of: vec!["def:base".into()],
            adversarial: None,
            held_out_from: Vec::new(), // filled per-candidate below
        }),
    });
    let (_r, _c, mut store, docs) = rig("leak5");
    let split_ref = deposit_split(
        &docs,
        &[("task:a", SplitLabel::Dev), ("task:b", SplitLabel::HeldOut)],
    );
    s.corpus.split_assignment_ref = split_ref;
    let (_, mut eng) =
        EvolutionCampaign::open(&mut store, docs.clone(), "holder", 60_000, s).unwrap();
    let base = base_doc();
    let target = target_doc();
    let cid = propose_ok(&mut store, &mut eng, &base, &target);
    // Re-declare the closure over *this* candidate (records-in — the
    // closure is the campaign's declared surface).
    eng.spec
        .judge_policy
        .as_mut()
        .unwrap()
        .artifact_benefit_selector = Some(SelectorDeclaration {
        selector_ref: "sel:benefit".into(),
        kind: "judge".into(),
        calibration_ref: "cal:b".into(),
        independent_of: vec!["def:base".into()],
        adversarial: None,
        held_out_from: vec![cid.clone()],
    });
    eng.hypothesize(&mut store, &cid, &hypothesis()).unwrap();
    eng.screen(
        &mut store,
        &cid,
        &ScreenReport {
            counterexample_set_ref: "ce:set".into(),
            observations: 10,
            flips_in_direction: 6,
            replicate_count: 1,
            split_labels_used: vec![SplitLabel::Dev],
            judge_only: false,
            selector_ref: None,
            honeypots: 0,
        },
    )
    .unwrap();
    // S4 — a registered comparative spec + matched search_time_benefit.
    drive_s4(&mut store, &docs, &mut eng, &cid, "leak5");

    // S5 — the held-out artifact_benefit report names the held-out
    // selector → JudgeLeakedIntoArtifact.
    let held = held_stage_spec(&eng);
    docs.put_spec(&held).unwrap();
    docs.put_named(
        doc_kind::REPORT,
        "rep:held",
        &hh_lab::analysis::ComparisonReport {
            arm_a: "a".into(),
            arm_b: "b".into(),
            metric: "task_success".into(),
            pairing: "by_task".into(),
            paired_effect: hh_lab::analysis::PairedEffect {
                point: Some(Json::str("0.05")),
                interval: Some(Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))])),
                method: hh_ontology::eval::IntervalMethod::ClusteredClt,
            },
            per_task_effects_ref: None,
            budget_match: hh_lab::analysis::BudgetMatch {
                limits_equal: true,
                consumption_imbalance: None,
                tolerance_ppm: 50_000,
                status: hh_lab::analysis::BudgetMatchStatus::Matched,
                search_unknown: false,
            },
            benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
            held_out: true,
            estimated: None,
            test: hh_lab::analysis::TestRecord {
                kind: hh_lab::analysis::TestKind::PermutationSignflip,
            },
            sign_profile: hh_lab::analysis::SignProfile {
                helped: 3,
                hurt: 0,
                unchanged: 1,
            },
            tail_effects: hh_lab::analysis::TailEffects {
                p50: Json::str("0.04"),
                p95: Json::str("0.12"),
                max: Json::str("0.30"),
            },
            outcome_bounds: hh_lab::analysis::OutcomeBounds {
                lower: Json::str("0.0"),
                upper: Json::str("1.0"),
                verdict: hh_lab::analysis::OutcomeBoundsVerdict::Robust,
            },
            multiplicity: hh_lab::analysis::Multiplicity {
                family_size: 1,
                adjusted: "holm".into(),
                raw_ppm: None,
                adjusted_ppm: None,
                label: None,
            },
            label: hh_lab::analysis::ReportLabelKind::Headlined,
            estimator_selection: hh_ontology::eval::EstimatorSelection {
                method: hh_ontology::eval::IntervalMethod::ClusteredClt,
                selection_rule: "adr-0158.clt_floor".into(),
                floors: BTreeMap::new(),
                fallback_chain: vec![],
                substituted: None,
            },
        }
        .to_json(),
    )
    .unwrap();
    let e = eng
        .held_out_eval(
            &mut store,
            &cid,
            &held.experiment_id,
            "rep:held",
            &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
            &BTreeMap::from([("metric:safety".to_string(), false)]),
            Some("sel:benefit"),
            None,
            false,
        )
        .unwrap_err();
    assert!(matches!(
        refusal(&e),
        Refusal::JudgeLeakedIntoArtifact { .. }
    ));
    assert_eq!(eng.view.state_of(&cid), Some("rejected"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. The S5 attribution label gate (§5h.7).
// ═══════════════════════════════════════════════════════════════════════════

fn stage_arm(id: &str, lid: &str, mode: MatchMode) -> hh_lab::experiment::ArmSpec {
    hh_lab::experiment::ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("model".to_string(), lid.to_string())]
            .into_iter()
            .collect(),
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        inference_budget: None,
        match_spec: Some(MatchSpec {
            mode,
            ..MatchSpec::matched_cap(&[])
        }),
        artifact_ref: hh_ontology::config::Ref::new("def:x", "sha256:ee55"),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    }
}

/// Drive `cid` through S4 (registered comparative spec + a matched
/// `search_time_benefit` report) — the shared leg the S5 tests need.
fn drive_s4(store: &mut Store, docs: &LabDocs, eng: &mut EvolutionCampaign, cid: &str, tag: &str) {
    use hh_lab::experiment::*;
    use hh_ontology::eval::*;
    use hh_ontology::participant::ParticipantClass;
    let seed_policy = SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let mode = MatchMode::MatchedTotal;
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: format!("design:evo-stage-{tag}"),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy.clone(),
            held_out_split_ref: Some("split:held_out".into()),
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "the candidate's predicted delta holds".into(),
                primary_metrics: vec!["task_success".into()],
                equivalence_margin: None,
                min_n: 1,
                analysis_plan_ref: "analysis:plan".into(),
                task_split_hash: "sha256:cc33".into(),
                interactions: vec![],
            },
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("base"), level("cand")],
        }],
        arms: vec![stage_arm("a", "base", mode), stage_arm("b", "cand", mode)],
        suite: SuiteBinding {
            suite_ref: "suite:test".into(),
            split_labels_used: vec![SplitLabel::Search],
            split_assignment_ref: Some("test-split".into()),
        },
        replicates_per_cell: 2,
        seed_policy,
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 2,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 1,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: format!("budget:exp-{tag}"),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::from([(
            "evolution_campaign".to_string(),
            Json::str(&eng.spec.campaign_id),
        )]),
    };
    s.experiment_id = s.experiment_id();
    docs.put_spec(&s).unwrap();
    let _ = tag;
    docs.put_named(
        doc_kind::SEARCH_BUDGET,
        "budget:search",
        &hh_ontology::eval::SearchBudgetRecord {
            rollouts: 4,
            model_calls: 4,
            tokens: 8_000,
            feedback_labels_used: 0,
            judge_calls: 0,
            wall_clock: 1_000,
            allocation: [
                ("search".to_string(), 900_000),
                ("audit".to_string(), 100_000),
            ]
            .into_iter()
            .collect(),
            adaptive_selection: false,
        }
        .to_json(),
    )
    .unwrap();
    let rep = format!("rep:search-{tag}");
    docs.put_named(
        doc_kind::REPORT,
        &rep,
        &comparison_json(
            hh_lab::analysis::BenefitKind::SearchTimeBenefit,
            false,
            Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
        ),
    )
    .unwrap();
    eng.matched_eval(store, cid, &s.experiment_id, &rep)
        .unwrap_or_else(|e| panic!("matched_eval: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("searched"));
}

fn held_stage_spec(eng: &EvolutionCampaign) -> hh_lab::experiment::ExperimentSpec {
    use hh_lab::experiment::*;
    use hh_ontology::eval::*;
    use hh_ontology::participant::ParticipantClass;
    let seed_policy = SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    };
    let level = |id: &str| LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    };
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "design:evo-held".into(),
            kind: DesignKind::Paired,
            factors: vec![],
            blocking: vec!["task".into()],
            replicates_per_cell: 2,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy.clone(),
            held_out_split_ref: Some("split:held_out".into()),
            pre_registration: PreRegistration {
                registered_at: 1,
                hypothesis: "h".into(),
                primary_metrics: vec!["task_success".into()],
                equivalence_margin: None,
                min_n: 1,
                analysis_plan_ref: "analysis:plan".into(),
                task_split_hash: "sha256:cc33".into(),
                interactions: vec![],
            },
            registry_snapshot_id: None,
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: None,
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("base"), level("cand")],
        }],
        arms: vec![
            stage_arm("a", "base", MatchMode::MatchedTotal),
            stage_arm("b", "cand", MatchMode::MatchedTotal),
        ],
        suite: SuiteBinding {
            suite_ref: "suite:test".into(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some("test-split".into()),
        },
        replicates_per_cell: 2,
        seed_policy,
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 2,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 1,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::from([(
            "evolution_campaign".to_string(),
            Json::str(&eng.spec.campaign_id),
        )]),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn comparison_json(benefit: hh_lab::analysis::BenefitKind, held_out: bool, interval: Json) -> Json {
    hh_lab::analysis::ComparisonReport {
        arm_a: "a".into(),
        arm_b: "b".into(),
        metric: "task_success".into(),
        pairing: "by_task".into(),
        paired_effect: hh_lab::analysis::PairedEffect {
            point: Some(Json::str("0.05")),
            interval: Some(interval),
            method: hh_ontology::eval::IntervalMethod::ClusteredClt,
        },
        per_task_effects_ref: None,
        budget_match: hh_lab::analysis::BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 50_000,
            status: hh_lab::analysis::BudgetMatchStatus::Matched,
            search_unknown: false,
        },
        benefit_kind: benefit,
        held_out,
        estimated: None,
        test: hh_lab::analysis::TestRecord {
            kind: hh_lab::analysis::TestKind::PermutationSignflip,
        },
        sign_profile: hh_lab::analysis::SignProfile {
            helped: 3,
            hurt: 0,
            unchanged: 1,
        },
        tail_effects: hh_lab::analysis::TailEffects {
            p50: Json::str("0.04"),
            p95: Json::str("0.12"),
            max: Json::str("0.30"),
        },
        outcome_bounds: hh_lab::analysis::OutcomeBounds {
            lower: Json::str("0.0"),
            upper: Json::str("1.0"),
            verdict: hh_lab::analysis::OutcomeBoundsVerdict::Robust,
        },
        multiplicity: hh_lab::analysis::Multiplicity {
            family_size: 1,
            adjusted: "holm".into(),
            raw_ppm: None,
            adjusted_ppm: None,
            label: None,
        },
        label: hh_lab::analysis::ReportLabelKind::Headlined,
        estimator_selection: hh_ontology::eval::EstimatorSelection {
            method: hh_ontology::eval::IntervalMethod::ClusteredClt,
            selection_rule: "adr-0158.clt_floor".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
    }
    .to_json()
}

#[test]
fn s5_attribution_report_gate_binds_the_named_design() {
    // A hypothesis naming a design must land its `hh-attribution/1`
    // report at S5: absent → invalid; wrong `design_ref` → invalid;
    // `designed_ablation` cannot carry a locality claim.
    for (tag, rref, locality, expect_ok) in [
        ("g1", None, false, false),                     // no report at all
        ("g2", Some("rep:wrong-design"), false, false), // wrong design_ref
        ("g3", Some("rep:ablation"), true, false),      // ablation cannot carry locality
        ("g4", Some("rep:causal"), true, true),         // interventional carries it
    ] {
        let (_r, _c, mut store, docs, mut eng, cid) = classified_candidate(tag);
        let aref = deposit_design(&docs, &format!("design:{tag}"), &["test:rule"]);
        let mut h = hypothesis();
        h.semantic_op_targets = vec!["test:rule".into()];
        h.attribution_ref = Some(aref.clone());
        eng.hypothesize(&mut store, &cid, &h)
            .unwrap_or_else(|e| panic!("hypothesize {tag}: {e}"));
        eng.screen(
            &mut store,
            &cid,
            &ScreenReport {
                counterexample_set_ref: "ce:set".into(),
                observations: 10,
                flips_in_direction: 6,
                replicate_count: 1,
                split_labels_used: vec![SplitLabel::Dev],
                judge_only: false,
                selector_ref: None,
                honeypots: 0,
            },
        )
        .unwrap();
        drive_s4(&mut store, &docs, &mut eng, &cid, tag);
        let held = held_stage_spec(&eng);
        docs.put_spec(&held).unwrap();
        docs.put_named(
            doc_kind::REPORT,
            &format!("rep:held-{tag}"),
            &comparison_json(
                hh_lab::analysis::BenefitKind::ArtifactBenefit,
                true,
                Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
            ),
        )
        .unwrap();
        // The attribution reports the legs name.
        deposit_report(
            &docs,
            "rep:wrong-design",
            "design:other",
            "causal_interventional",
        );
        deposit_report(&docs, "rep:ablation", &aref, "designed_ablation");
        deposit_report(&docs, "rep:causal", &aref, "causal_interventional");
        let res = eng.held_out_eval(
            &mut store,
            &cid,
            &held.experiment_id,
            &format!("rep:held-{tag}"),
            &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
            &BTreeMap::from([("metric:safety".to_string(), false)]),
            None,
            rref,
            locality,
        );
        if expect_ok {
            res.unwrap_or_else(|e| panic!("held_out_eval {tag}: {e}"));
            assert_eq!(eng.view.state_of(&cid), Some("validated"));
        } else {
            let e = res.unwrap_err();
            assert!(
                matches!(refusal(&e), Refusal::AttributionReportInvalid { .. }),
                "{tag}: {e:?}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. The lineage DAG + attribution over it (R-2.12.1⁴).
// ═══════════════════════════════════════════════════════════════════════════

/// Drive a candidate all the way to `active` so the lineage head moves
/// to its `target_ref` — the second candidate then parents to it.
fn drive_to_active(
    store: &mut Store,
    docs: &LabDocs,
    eng: &mut EvolutionCampaign,
    cid: &str,
    tag: &str,
) {
    eng.hypothesize(store, cid, &hypothesis())
        .unwrap_or_else(|e| panic!("hypothesize: {e}"));
    eng.screen(
        store,
        cid,
        &ScreenReport {
            counterexample_set_ref: "ce:set".into(),
            observations: 10,
            flips_in_direction: 6,
            replicate_count: 1,
            split_labels_used: vec![SplitLabel::Dev],
            judge_only: false,
            selector_ref: None,
            honeypots: 0,
        },
    )
    .unwrap();
    drive_s4(store, docs, eng, cid, tag);
    let held = held_stage_spec(eng);
    docs.put_spec(&held).unwrap();
    docs.put_named(
        doc_kind::REPORT,
        &format!("rep:held-{tag}"),
        &comparison_json(
            hh_lab::analysis::BenefitKind::ArtifactBenefit,
            true,
            Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(9))]),
        ),
    )
    .unwrap();
    eng.held_out_eval(
        store,
        cid,
        &held.experiment_id,
        &format!("rep:held-{tag}"),
        &Json::obj([("regressed_tasks", Json::Arr(vec![]))]),
        &BTreeMap::from([("metric:safety".to_string(), false)]),
        None,
        None,
        false,
    )
    .unwrap_or_else(|e| panic!("held_out_eval: {e}"));
    use hh_evolution::records::TransferRow;
    eng.transfer(
        store,
        cid,
        &[TransferRow {
            environment_family: EnvironmentFamily::StructuredTool,
            status: "measured".into(),
            point: Some(Json::str("0.03")),
            interval: Some(Json::obj([("lo", Json::Int(1)), ("hi", Json::Int(7))])),
            method: "clustered_clt".into(),
            held_out: true,
        }],
        &[hh_lab::model::CompatibilityRecord {
            snapshot_ref: "snap:1".into(),
            definition_semantic_id: "def:base".into(),
            profile_semantic_id: "sha256:profile".into(),
            status: hh_lab::model::CompatibilityStatus::Verified,
            evidence_ref: Some("ev:compat".into()),
            regression_suite_ref: Some("suite:regression".into()),
            created_at: 1,
            expiry_condition: None::<hh_ontology::debt::ExpiryCondition>,
            provenance: prov(20),
        }
        .to_json()],
    )
    .unwrap_or_else(|e| panic!("transfer: {e}"));
    eng.security_check(
        store,
        cid,
        &hh_evolution::records::SecurityInvarianceReport {
            placement: "subprocess_confined".into(),
            has_interface: true,
            policy_leaves: BTreeMap::from([("policy:test".into(), "narrowing".into())]),
            dynamic_veto_table: BTreeMap::from([("metric:safety".into(), false)]),
        },
    )
    .unwrap_or_else(|e| panic!("security_check: {e}"));
    let mut seal = ProvenanceRecord::minted(
        Origin::human("test:sealer", HumanRole::Principal),
        PersistenceScope::Definition,
        99,
    );
    seal.authority = AuthorityClass::Definition;
    eng.seal(
        store,
        cid,
        &EvolutionAcceptanceReport {
            items: (1..=7)
                .map(|i| AcceptanceItem {
                    item: i,
                    status: ItemStatus::Pass,
                    evidence_refs: vec![format!("ev:{i}")],
                })
                .collect(),
            attribution_granularity: "designed_ablation".into(),
            portability_label: PortabilityLabel::Reported,
            label: None,
        },
        &seal,
    )
    .unwrap_or_else(|e| panic!("seal: {e}"));
    eng.canary(store, cid, "shadow", "int:counterfactual-1")
        .unwrap_or_else(|e| panic!("canary: {e}"));
    eng.canary_settle(store, cid, "clean", "", &[], None)
        .unwrap_or_else(|e| panic!("canary_settle: {e}"));
    assert_eq!(eng.view.state_of(cid), Some("active"));
}

#[test]
fn lineage_dag_parents_children_and_attributes_shares() {
    let (_r, _c, mut store, docs, mut eng) = open("lin");
    let base = base_doc();
    let t1 = target_doc();
    let c1 = propose_ok(&mut store, &mut eng, &base, &t1);
    drive_to_active(&mut store, &docs, &mut eng, &c1, "c1");

    // The head moved to c1's applied target — c2 proposes over it.
    let head = eng.view.head_ref().expect("head after c1 active");
    let c1_target_ref = eng
        .view
        .candidate(&c1)
        .and_then(|c| c.target_ref.clone())
        .expect("c1 target_ref");
    assert_eq!(head, c1_target_ref);

    // A second candidate over the moved head: a new target doc (another
    // trigger edit) whose diff's `base` is c1's target.
    let t2 = {
        let mut doc = HirDocument::new(sel("test:agent"));
        doc.nodes.push(rule_node(
            "test:rule",
            1,
            Json::obj([("on", Json::str("step_start"))]),
        ));
        doc.nodes.push(budget_node("test:budget", 2));
        doc.nodes.push(perm_node("test:perm", "test:agent", 3));
        doc.nodes
            .push(agent_node("test:agent", "test:budget", "test:perm", 4));
        doc
    };
    let d2 = candidate_diff(&t1, &t2);
    let c2 = eng
        .propose(
            &mut store,
            &CandidateProposal {
                base_ref: c1_target_ref.clone(),
                diff: d2,
                slot: "control_strategy".into(),
                hypothesis: None,
                install: None,
                coordinate_values: Default::default(),
            },
            &t1,
        )
        .unwrap_or_else(|e| panic!("propose c2: {e}"));

    // The DAG — c1 is a root (generation 0), c2 its child (generation 1).
    let dag = LineageDag::from_view(&eng.view, &eng.spec.base_definition_ref);
    assert_eq!(dag.nodes[&c1].parent, None);
    assert_eq!(dag.nodes[&c1].generation, 0);
    assert_eq!(dag.nodes[&c2].parent.as_deref(), Some(c1.as_str()));
    assert_eq!(dag.nodes[&c2].generation, 1);

    // Attribution over the DAG — c1's report contributes its edge
    // effect; c2 carries no report (`reported = false`, effect 0).
    let mut reports = BTreeMap::new();
    reports.insert(
        c1.clone(),
        Json::obj([
            ("schema", Json::str("hh-attribution/1")),
            (
                "effects",
                Json::Arr(vec![
                    Json::obj([
                        ("target", Json::str("test:rule")),
                        ("point", Json::Int(50_000)),
                    ]),
                    Json::obj([
                        ("target", Json::str("test:budget")),
                        ("point", Json::Int(-10_000)),
                    ]),
                ]),
            ),
        ]),
    );
    let attr = attribute_lineage(&dag, &reports);
    let nodes = match attr.get("nodes") {
        Some(Json::Arr(a)) => a.clone(),
        other => panic!("nodes: {other:?}"),
    };
    let n1 = nodes
        .iter()
        .find(|n| n.get("candidate_id").and_then(Json::as_str) == Some(c1.as_str()))
        .expect("c1 row");
    let n2 = nodes
        .iter()
        .find(|n| n.get("candidate_id").and_then(Json::as_str) == Some(c2.as_str()))
        .expect("c2 row");
    // The edge effect is the signed sum of the report's effects.
    assert_eq!(n1.get("edge_effect").and_then(Json::as_int), Some(40_000));
    assert_eq!(
        n1.get("edge_share_ppm").and_then(Json::as_int),
        Some(1_000_000)
    );
    assert_eq!(n2.get("edge_effect").and_then(Json::as_int), Some(0));
    assert_eq!(
        n2.get("reported").and_then(|v| match v {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(false)
    );
    // The DAG's own record is deterministic.
    assert_eq!(dag.to_json(), dag.to_json());
}
