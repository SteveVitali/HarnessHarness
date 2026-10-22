//! R4.7e — the `judge_adapters` hermetic half (DF-S1.21-3; BL-17's
//! evidence leg): the live `CriticDeclaration{kind = judge}` binding over
//! a loopback `TcpListener` fake provider (the r4_5b/r4_7b shape — a
//! real `GatewayModel` call path, a recorded answer the test authors).
//!
//! Covered, removal-sensitively:
//! - `link_judge` consumes `declare_critic` at link — `RedundantJudge`,
//!   `GateWithoutCalibration`, snapshot-axis overclaim — and re-bases the
//!   axis from family facts.
//! - `link_judged_pair` refuses a same-family pair (the independence
//!   rule's pair half).
//! - a decided consult mints `verification.validator.*` rows whose shape
//!   `RecordedJudge` replays verbatim (`detector:judged` +
//!   `validator_ref` + graded `value`) — replay byte-identity.
//! - `calibrate` folds the labelled set into a `CalibrationRecord`
//!   (measured agreement/error/self-consistency — never a claimed one).
//! - usage lands verbatim per call (the spend accounting).
//! - a typed wire failure still lands `oracle_failure` rows — never a
//!   silent zero.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hh_control::loops::{JudgePort, RecordedJudge};
use hh_control::policy::JudgedSpec;
use hh_embed::context_adapters::AdapterLane;
use hh_embed::judge_adapters::{
    calibrate, judge_fingerprint, link_judge, link_judged_pair, CalibrationCase, JudgeBindSpec,
};
use hh_embed_schema::errors::EmbedError;
use hh_identity::kinds::RecordKind;
use hh_identity::refs::VersionedRef;
use hh_ledger::classes::Durability;
use hh_ledger::event::{EventEnvelope, EventPlane};
use hh_ledger::manifest::ParticipantClass;
use hh_provenance::authority::PersistenceScope;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_verification::critic_rt::CalibrationContext;
use hh_verification::critics::{CalibrationRecord, CriticDeclaration, IndependenceVector};
use hh_verification::reconciler::ReconcilerDeclaration;
use hh_verification::vocab::{
    AdmissionMode, CalibrationStatus, ChargedTo, CriticKind, CriticUse, Detector, DivergenceClass,
    EvidenceMode, OracleClass, Site, SnapshotIndependence, VerdictType,
};
use hh_wire::json::Json;

// ── the loopback fake provider (r4_7b's shape) ─────────────────────────

struct FakeProvider {
    endpoint: String,
    /// The served-request counter (the leg's "provider saw the call"
    /// witness).
    count: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl FakeProvider {
    /// Whether the provider served at least `n` requests.
    fn served(&self, n: usize) -> bool {
        for _ in 0..100 {
            if *self.count.lock().unwrap() >= n {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }
}

/// `fake_provider_n(response, n)` — serve `response` to the next `n`
/// sequential requests on a loopback listener.
fn fake_provider_n(response: String, n: usize) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let count2 = count.clone();
    std::thread::spawn(move || {
        for _ in 0..n {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                loop {
                    if buf.windows(4).any(|w| w == b"\r\n\r\n")
                        || std::time::Instant::now() > deadline
                    {
                        break;
                    }
                    match sock.read(&mut tmp) {
                        Ok(0) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                        Err(_) => break,
                    }
                }
                *count2.lock().unwrap() += 1;
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
            }
        }
    });
    FakeProvider {
        endpoint: format!("http://127.0.0.1:{port}"),
        count,
    }
}

fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A chat-completions SSE stream answering `text` with a usage trailer
/// (r4_7b's `openai_sse_text` shape).
fn openai_sse_text(model: &str, text: &str) -> String {
    let chunk = |delta: Json, finish: Json, usage: Json| {
        let mut c = BTreeMap::new();
        c.insert("id".into(), Json::str("chatcmpl-1"));
        c.insert("object".into(), Json::str("chat.completion.chunk"));
        c.insert("model".into(), Json::str(model));
        c.insert(
            "choices".into(),
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("delta", delta),
                ("finish_reason", finish),
            ])]),
        );
        if !matches!(usage, Json::Null) {
            c.insert("usage".into(), usage);
        }
        format!("data: {}\n\n", Json::Obj(c).to_canonical_string())
    };
    sse_response(&format!(
        "{}{}data: [DONE]\n\n",
        chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                ("content", Json::str(text)),
            ]),
            Json::Null,
            Json::Null,
        ),
        chunk(
            Json::obj([]),
            Json::str("stop"),
            Json::obj([
                ("prompt_tokens", Json::Int(11)),
                ("completion_tokens", Json::Int(4)),
                ("total_tokens", Json::Int(15)),
            ]),
        ),
    ))
}

// ── declarations + lanes ────────────────────────────────────────────────

fn prov() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::kernel("hir/kernel/test"),
        PersistenceScope::Definition,
        1,
    )
}

fn lane(dialect: &str, endpoint: &str, model: &str) -> AdapterLane {
    AdapterLane {
        dialect_ref: dialect.to_string(),
        endpoint_ref: endpoint.to_string(),
        provider_model_id: model.to_string(),
        credential_ref: None,
        profile_ref: "profile/judge-test".to_string(),
        ..AdapterLane::default()
    }
}

/// A `CriticDeclaration{kind = judge}` for report use (the binding the
/// ticket's pair rides — veto-admissible evidence, never a gate fact).
fn judge_decl(
    critic_version: &str,
    use_: CriticUse,
    calibration_ref: Option<&str>,
) -> CriticDeclaration {
    CriticDeclaration {
        critic_ref: VersionedRef::pinned(RecordKind::Validator, critic_version, prov()),
        oracle_id: format!("oracle:{critic_version}"),
        oracle_class: OracleClass::Judge,
        deterministic: false,
        critic_kind: CriticKind::Judge,
        evidence_mode: [EvidenceMode::LedgerView, EvidenceMode::Transcript]
            .into_iter()
            .collect(),
        admission_mode: AdmissionMode::PrincipalOnly,
        probe_capabilities: vec![],
        independence: IndependenceVector::kernel_independent(),
        site: Site::Instrument,
        use_,
        placement: None,
        verdict_type: VerdictType::Graded,
        comparative: false,
        rubric_ref: None,
        calibration_ref: calibration_ref.map(str::to_string),
        held_out_from: vec![],
        debt: Some("debt:judge-link".into()),
        charged_to: ChargedTo::Instrument,
        calibration_status: CalibrationStatus::Active,
        deterministic_coverage_at_placement: false,
    }
}

fn bind_spec(
    critic_version: &str,
    lane: AdapterLane,
    family: &str,
    beneficiaries: Vec<String>,
    calibration: Option<(CalibrationRecord, CalibrationContext)>,
    use_: CriticUse,
) -> JudgeBindSpec {
    let calibration_ref = calibration.as_ref().map(|(r, _)| r.calibration_id.as_str());
    JudgeBindSpec {
        declaration: judge_decl(critic_version, use_, calibration_ref),
        lane,
        rubric_text: "flag divergence between the claim and the record".to_string(),
        family: family.to_string(),
        judge_snapshot_fingerprint: String::new(),
        beneficiary_families: beneficiaries,
        calibration,
        run_ref: "run:r4_7e".to_string(),
    }
}

/// A calibration record + context folding `active` for `lane`'s judge
/// (`calibration_status_at`'s inputs).
fn active_calibration(
    lane: &AdapterLane,
    critic_ref: &str,
) -> (CalibrationRecord, CalibrationContext) {
    let record = CalibrationRecord {
        calibration_id: format!("calib:{critic_ref}"),
        critic_ref: critic_ref.to_string(),
        judge_snapshot_fingerprint: judge_fingerprint(lane),
        labelled_set_ref: "labelled:stage3_v1".into(),
        n_per_verdict_class: 4,
        reference_oracle: hh_verification::vocab::ReferenceOracle::Executable,
        agreement: hh_verification::critics::CalibrationAgreement {
            statistic: "cohen_kappa".into(),
            value_ppm: 900_000,
            interval: "point".into(),
            method: "test".into(),
        },
        error_rates: Json::Null,
        position_consistency_ppm: 900_000,
        self_consistency_ppm: None,
        self_preference_check: None,
        evidence_ablation_ppm: Some(100_000),
        calibrated_at: 1,
        expiry: vec![],
        status: CalibrationStatus::Active,
    };
    let ctx = CalibrationContext {
        judge_snapshot_fingerprint: judge_fingerprint(lane),
        rubric_ref: "rubric:v1".into(),
        task_family: "reconciliation".into(),
        now_ms: 2,
        recalibration_period_ms: 1_000_000_000,
    };
    (record, ctx)
}

fn judged_spec(validator_ref: &str) -> JudgedSpec {
    JudgedSpec {
        validator_ref: validator_ref.to_string(),
        after_turns: 0,
        interval: 1,
        confidence_threshold_ppm: 500_000,
        charged_to: "instrument".into(),
    }
}

/// An `EventEnvelope` over a drained `(class, payload, provenance)`
/// triple — the shape `RecordedJudge::from_prefix` folds.
fn envelope(seq: u64, class: &str, payload: Json, prov: ProvenanceRecord) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("ev-{seq}"),
        run_id: "run:r4_7e".into(),
        seq,
        ts: "t".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Control),
        class: class.to_string(),
        schema_version: 1,
        producer: hh_ledger::event::Producer::kernel("hh-test"),
        participant_class: ParticipantClass::Native,
        observability_level: Default::default(),
        durability: Durability::Ledger,
        scope: Default::default(),
        lease_generation: 1,
        parent_event_id: String::new(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: Default::default(),
        provenance: Some(prov),
        payload,
        prev_hash: "h".into(),
        hash: format!("h{seq}"),
    }
}

// ── link-time consumption ───────────────────────────────────────────────

/// `link_judged_pair` refuses a same-family pair — the independence
/// rule's pair half, typed at link.
#[test]
fn link_refuses_same_family_pair() {
    let a = bind_spec(
        "judge:a",
        lane("openai-compatible@1", "http://127.0.0.1:1", "m-a"),
        "llama-family",
        vec!["beneficiary-family".to_string()],
        None,
        CriticUse::Report,
    );
    let b = bind_spec(
        "judge:b",
        lane("openai-compatible@1", "http://127.0.0.1:1", "m-b"),
        "llama-family",
        vec!["beneficiary-family".to_string()],
        None,
        CriticUse::Report,
    );
    let err = link_judged_pair(a, b).unwrap_err();
    match err {
        EmbedError::Refused { reason } => {
            assert!(reason.contains("different families"), "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
}

/// The snapshot axis derives from family facts: a judge whose family is
/// shared with a beneficiary derives `different_snapshot_same_family` —
/// a declaration claiming `different_family` overclaims and is refused
/// (the same rule `admit_judge` enforces at emission).
#[test]
fn link_refuses_snapshot_axis_overclaim_and_rebases() {
    // Overclaim — declared `different_family`, derived same-family.
    let mut spec = bind_spec(
        "judge:a",
        lane("openai-compatible@1", "http://127.0.0.1:1", "m-a"),
        "shared-family",
        vec!["shared-family".to_string()],
        None,
        CriticUse::Report,
    );
    spec.declaration.independence.snapshot = SnapshotIndependence::DifferentFamily;
    let err = link_judge(spec).unwrap_err();
    match err {
        EmbedError::Refused { reason } => assert!(reason.contains("overclaims"), "{reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    // Conservative declaration re-bases upward — `same_snapshot` declared,
    // `different_family` derived → bound with the derived axis.
    let mut spec = bind_spec(
        "judge:b",
        lane("openai-compatible@1", "http://127.0.0.1:1", "m-b"),
        "own-family",
        vec!["other-family".to_string()],
        None,
        CriticUse::Report,
    );
    spec.declaration.independence.snapshot = SnapshotIndependence::SameSnapshot;
    let judge = link_judge(spec).unwrap();
    assert_eq!(judge.family(), "own-family");
}

/// `RedundantJudge` — a model-based critic beside deterministic coverage
/// at the placement refuses at link (ADR-0311's deterministic-first
/// ordering, consumed on a real binding).
#[test]
fn link_consumes_redundant_judge_refusal() {
    let mut spec = bind_spec(
        "judge:a",
        lane("openai-compatible@1", "http://127.0.0.1:1", "m-a"),
        "own-family",
        vec!["other-family".to_string()],
        None,
        CriticUse::Report,
    );
    spec.declaration.deterministic_coverage_at_placement = true;
    let err = link_judge(spec).unwrap_err();
    match err {
        EmbedError::Refused { reason } => assert!(reason.contains("RedundantJudge"), "{reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
}

/// `GateWithoutCalibration` — a gate-use judge without an active
/// calibration refuses; the same judge with a folded `active` record
/// binds (the `calibrate` leg's product consumed at link).
#[test]
fn link_consumes_calibration_gate() {
    let judge_lane = lane("openai-compatible@1", "http://127.0.0.1:1", "m-a");
    let spec = bind_spec(
        "judge:a",
        judge_lane.clone(),
        "own-family",
        vec!["other-family".to_string()],
        None,
        CriticUse::Gate,
    );
    let err = link_judge(spec).unwrap_err();
    match err {
        EmbedError::Refused { reason } => {
            assert!(reason.contains("GateWithoutCalibration"), "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    // With an active folded record the gate judge binds.
    let (record, ctx) = active_calibration(&judge_lane, "judge:a");
    let spec = bind_spec(
        "judge:a",
        judge_lane.clone(),
        "own-family",
        vec!["other-family".to_string()],
        Some((record, ctx)),
        CriticUse::Gate,
    );
    link_judge(spec).unwrap();
    // An expired record degrades back to the refusal — the fold's status,
    // never the record's stamped `status` member.
    let (mut record, mut ctx) = active_calibration(&judge_lane, "judge:a");
    record.calibrated_at = 0;
    ctx.recalibration_period_ms = 1;
    let spec = bind_spec(
        "judge:a",
        judge_lane,
        "own-family",
        vec!["other-family".to_string()],
        Some((record, ctx)),
        CriticUse::Gate,
    );
    let err = link_judge(spec).unwrap_err();
    match err {
        EmbedError::Refused { reason } => {
            assert!(reason.contains("GateWithoutCalibration"), "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
}

/// An uncalibrated `report` judge binds exploratory — `calibration_status`
/// folds `expired` (never-active is never active) and the consult's
/// `JudgedConsult.calibration_ref` is the honest `None` the driver
/// declines `reconcile_judged` for.
#[test]
fn uncalibrated_report_judge_binds_exploratory() {
    let judge_lane = lane("openai-compatible@1", "http://127.0.0.1:1", "m-a");
    let spec = bind_spec(
        "judge:a",
        judge_lane,
        "own-family",
        vec!["other-family".to_string()],
        None,
        CriticUse::Report,
    );
    let judge = link_judge(spec).unwrap();
    assert_eq!(judge.critic_ref(), "judge:a");
    // The consult contract: no calibration → `calibration_ref: None`
    // (the driver's `JudgedWithoutCalibration` decline is the driver-half
    // test; here the consult itself reports the honest absence).
    let consults = judge.assess(
        &hh_verification::claims::Claim {
            claim_id: "c1".into(),
            run_id: "run".into(),
            model_call_id: "mc".into(),
            at_seq: 0,
            kind: hh_verification::vocab::ClaimKind::Achieved,
            subject: hh_verification::vocab::SubjectRef::Run,
            predicate: "is_done".into(),
            asserted: Json::Null,
            evidence_refs: vec![],
            extracted_by: hh_verification::vocab::ExtractedBy::Structured("finish".into()),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: prov(),
        },
        &[],
    );
    // The consult ran a call that failed typed (closed port) — the
    // abstention is honest, and the oracle_failure rows landed.
    assert!(consults.is_empty());
    let records = judge.take_records();
    assert!(
        records
            .iter()
            .any(|(c, p, _)| c == "verification.validator.verdict"
                && p.get("status").and_then(Json::as_str) == Some("oracle_failure")
                && p.get("exploratory") == Some(&Json::Bool(true))),
        "an uncalibrated judge's failure lands exploratory rows: {records:?}"
    );
}

// ── the live consult's durable trail ────────────────────────────────────

/// A decided consult: the judge answers `{"confidence_ppm": 900000}`;
/// `judge()` returns it; the drained `verification.validator.verdict`
/// row carries `detector:judged` + `validator_ref` + `value: 900000` +
/// `bundle_id`/`calibration_ref`/`independence_summary` — the DF-S1.21-3
/// acceptance members — and the `hh.model_io/1` blob drains beside it.
/// The same payload folded through `RecordedJudge` replays the answer
/// verbatim — byte identity by construction.
#[test]
fn decided_consult_mints_replayable_verdict_rows() {
    let provider = fake_provider_n(
        openai_sse_text("m-judge", r#"{"confidence_ppm": 900000}"#),
        1,
    );
    let judge_lane = lane("openai-compatible@1", &provider.endpoint, "m-judge");
    let (record, ctx) = active_calibration(&judge_lane, "judge:live-1");
    let calibration_id = record.calibration_id.clone();
    let spec = bind_spec(
        "judge:live-1",
        judge_lane,
        "own-family",
        vec!["other-family".to_string()],
        Some((record, ctx)),
        CriticUse::Report,
    );
    let judge = link_judge(spec).unwrap();
    let window = vec![Json::obj([
        ("seq", Json::Int(3)),
        ("event_id", Json::str("ev-3")),
    ])];
    let ppm = judge.judge(&judged_spec("validator:judge-live"), &window);
    assert_eq!(ppm, 900_000);
    assert!(provider.served(1), "the provider saw the consult");
    // The durable trail: model_io blob + invoked + verdict.
    let io = judge.take_model_io();
    assert_eq!(io.len(), 1, "one model_io blob per consult");
    // The wire blob's media is `application/json` — the `hh.model_io/1`
    // schema names the document, not the media string.
    assert_eq!(io[0].0, "application/json");
    let records = judge.take_records();
    let classes: Vec<&str> = records.iter().map(|(c, _, _)| c.as_str()).collect();
    assert_eq!(
        classes,
        [
            "verification.validator.invoked",
            "verification.validator.verdict"
        ]
    );
    let (_, payload, prov) = &records[1];
    assert_eq!(
        payload.get("validator_ref").and_then(Json::as_str),
        Some("judge:live-1")
    );
    assert_eq!(
        payload.get("detector").and_then(Json::as_str),
        Some("judged")
    );
    assert_eq!(
        payload.get("status").and_then(Json::as_str),
        Some("decided")
    );
    assert_eq!(payload.get("value").and_then(Json::as_int), Some(900_000));
    assert_eq!(
        payload.get("calibration_ref").and_then(Json::as_str),
        Some(calibration_id.as_str()),
        "the folded record's id rides the verdict (DF-S1.21-3 acceptance)"
    );
    assert!(payload
        .get("independence_summary")
        .and_then(Json::as_str)
        .is_some_and(|s| s.contains("snapshot=different_family")),);
    assert!(payload
        .get("bundle_id")
        .and_then(Json::as_str)
        .is_some_and(|s| !s.is_empty()),);
    // The delegate-authority provenance the verdict contract mints.
    assert_eq!(
        prov.authority,
        hh_provenance::authority::AuthorityClass::Delegate
    );
    // Usage — the spend accounting — lands verbatim per call.
    let usages = judge.usages();
    assert_eq!(usages.len(), 1);
    assert_eq!(
        usages[0].get("call_id").and_then(Json::as_str),
        Some("judge.0")
    );
    let raw = usages[0].get("raw").unwrap();
    assert_eq!(raw.get("total_tokens").and_then(Json::as_int), Some(15));

    // Replay byte-identity: the drained payload folded through
    // `RecordedJudge` answers exactly what the live port answered.
    let events: Vec<EventEnvelope> = records
        .iter()
        .enumerate()
        .map(|(i, (c, p, prov))| envelope(i as u64 + 1, c, p.clone(), prov.clone()))
        .collect();
    let replay = RecordedJudge::from_prefix(&events);
    // `RecordedJudge` keys on the recorded `validator_ref` — the bound
    // critic ref — so the spec's validator_ref names it.
    assert_eq!(
        replay.judge(&judged_spec("judge:live-1"), &[]),
        900_000,
        "the recorded verdict replays verbatim"
    );
}

/// The panel's `min` reduction + `withheld` accounting: two different-
/// family members, one decided (900000), one oracle-failed — the panel
/// answer is the decided member's (the withheld member's failure is
/// recorded, never a guessed confidence) and the panel verdict under the
/// bound `validator_ref` carries `panel{members, withheld,
/// decision_rule, dissent_ppm}` — the `RecordedJudge` key.
#[test]
fn panel_judge_min_reduction_records_withheld_members() {
    let pa = fake_provider_n(openai_sse_text("m-a", r#"{"confidence_ppm": 900000}"#), 1);
    // Member B's endpoint never answers (closed port → oracle_failure).
    let a = bind_spec(
        "judge:a",
        lane("openai-compatible@1", &pa.endpoint, "m-a"),
        "family-a",
        vec!["beneficiary".to_string()],
        None,
        CriticUse::Report,
    );
    let b = bind_spec(
        "judge:b",
        lane("openai-compatible@1", "http://127.0.0.1:9", "m-b"),
        "family-b",
        vec!["beneficiary".to_string()],
        None,
        CriticUse::Report,
    );
    let panel = link_judged_pair(a, b).unwrap();
    assert_eq!(panel.families(), vec!["family-a", "family-b"]);
    let spec = judged_spec("validator:panel");
    let ppm = panel.judge(&spec, &[]);
    assert_eq!(ppm, 900_000, "the panel's answer is min(decided members)");
    let records = panel.take_records();
    let panel_row = records
        .iter()
        .find(|(c, p, _)| {
            c == "verification.validator.verdict"
                && p.get("validator_ref").and_then(Json::as_str) == Some("validator:panel")
        })
        .expect("the panel verdict under the bound validator_ref");
    let panel = panel_row
        .1
        .get("panel")
        .expect("the panel member carries the reduction");
    assert_eq!(
        panel.get("decision_rule").and_then(Json::as_str),
        Some("min")
    );
    let withheld: Vec<&str> = panel
        .get("withheld")
        .and_then(|w| {
            if let Json::Arr(a) = w {
                Some(a.iter().filter_map(Json::as_str).collect())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert_eq!(withheld, vec!["judge:b"], "the failed member is withheld");
}

/// `assess` — the R-2.7.2b consult: a decided `{agree, classes,
/// confidence_ppm}` answer mints the verdict row and returns the
/// `JudgedConsult` whose `reconcile_judged` fold produces `detector:
/// judged` records — veto-admissible, never hold-admissible (F7).
#[test]
fn assess_consult_feeds_reconcile_judged() {
    let provider = fake_provider_n(
        openai_sse_text(
            "m-judge",
            r#"{"agree": false, "confidence_ppm": 800000, "classes": ["evidence_inversion"]}"#,
        ),
        1,
    );
    let judge_lane = lane("openai-compatible@1", &provider.endpoint, "m-judge");
    let (record, ctx) = active_calibration(&judge_lane, "judge:live-1");
    let calibration_id = record.calibration_id.clone();
    let spec = bind_spec(
        "judge:live-1",
        judge_lane,
        "own-family",
        vec!["other-family".to_string()],
        Some((record, ctx)),
        CriticUse::Report,
    );
    let judge = link_judge(spec).unwrap();
    let claim = hh_verification::claims::Claim {
        claim_id: "c1".into(),
        run_id: "run".into(),
        model_call_id: "mc".into(),
        at_seq: 7,
        kind: hh_verification::vocab::ClaimKind::Achieved,
        subject: hh_verification::vocab::SubjectRef::Run,
        predicate: "is_done".into(),
        asserted: Json::Null,
        evidence_refs: vec![],
        extracted_by: hh_verification::vocab::ExtractedBy::Structured("finish".into()),
        extraction_confidence_ppm: 1_000_000,
        criteria_status: vec![],
        provenance: prov(),
    };
    let evidence = vec![Json::obj([
        ("handle_ref", Json::str("ev:9")),
        ("authority", Json::str("kernel")),
        (
            "value",
            Json::obj([
                ("kind", Json::str("verdict")),
                ("affirmative", Json::Bool(false)),
            ]),
        ),
    ])];
    let consults = judge.assess(&claim, &evidence);
    assert_eq!(consults.len(), 1);
    let c = &consults[0];
    assert_eq!(c.judge_ref, "judge:live-1");
    assert_eq!(c.calibration_ref.as_deref(), Some(calibration_id.as_str()));
    assert_eq!(
        c.assessment.classes,
        vec![DivergenceClass::EvidenceInversion]
    );
    assert_eq!(c.assessment.confidence_ppm, 800_000);
    // The consult's own verdict row landed (detector: judged, the claim's
    // target).
    let records = judge.take_records();
    assert!(records
        .iter()
        .any(|(cl, p, _)| cl == "verification.validator.verdict"
            && p.get("target").and_then(Json::as_str) == Some("c1")),);
    // …and the consult folds `reconcile_judged` — a `detector: judged`
    // record that is veto-admissible, never hold-admissible.
    let mut decl = ReconcilerDeclaration {
        reconciler_ref: "reconciler:exec-align".into(),
        classes_detected: DivergenceClass::ALL.iter().copied().collect(),
        detector_classes: [Detector::Deterministic, Detector::Judged]
            .into_iter()
            .collect(),
        probe_capabilities: vec![],
        requires_observability: Default::default(),
        applies_to: Default::default(),
        probe_budget_share_cap_ppm: None,
        conditioned_on: None,
        assumption_debt: None,
        judge_ref: Some(c.judge_ref.clone()),
        calibration_ref: c.calibration_ref.clone(),
        independence_summary: Some(c.independence_summary.clone()),
        no_progress_k: 2,
        enabled: true,
    };
    decl.judge_ref = Some(c.judge_ref.clone());
    let records = hh_verification::reconciler::reconcile_judged(
        &claim,
        &c.assessment,
        &decl,
        10,
        ProvenanceRecord::kernel("hh-test", 0),
    )
    .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].detector, Detector::Judged);
    assert!(!records[0].hold_admissible(), "F7 — judged never holds");
    // The record itself marks the judged detector — `detector` is the
    // veto-admissibility the `reconciler.rs` contract names.
}

/// `calibrate` — the labelled-set fold mints the `CalibrationRecord`'s
/// measured facts (agreement, error rates, self-consistency over the
/// two-pass runs) — never a claimed statistic.
#[test]
fn calibrate_folds_measured_agreement() {
    // Two passes × four cases = 8 calls; the provider agrees every time.
    let provider = fake_provider_n(openai_sse_text("m-judge", r#"{"agree": true}"#), 8);
    let judge_lane = lane("openai-compatible@1", &provider.endpoint, "m-judge");
    let cases = vec![
        CalibrationCase {
            case_id: "case-1".into(),
            rendered: "{\"obs\":\"loop\"}".into(),
            label_agree: true,
            verdict_class: "diverge".into(),
        },
        CalibrationCase {
            case_id: "case-2".into(),
            rendered: "{\"obs\":\"loop2\"}".into(),
            label_agree: true,
            verdict_class: "diverge".into(),
        },
        CalibrationCase {
            case_id: "case-3".into(),
            rendered: "{\"obs\":\"clean\"}".into(),
            label_agree: false,
            verdict_class: "agree".into(),
        },
        CalibrationCase {
            case_id: "case-4".into(),
            rendered: "{\"obs\":\"clean2\"}".into(),
            label_agree: false,
            verdict_class: "agree".into(),
        },
    ];
    let run = calibrate(
        &judge_lane,
        "judge:live-1",
        "labelled:r4_7e.v1",
        &cases,
        "flag divergence",
        "rubric:v1",
        "reconciliation",
        1000,
    )
    .unwrap();
    let r = &run.record;
    assert_eq!(r.status, CalibrationStatus::Active);
    assert_eq!(r.critic_ref, "judge:live-1");
    assert_eq!(
        r.judge_snapshot_fingerprint,
        judge_fingerprint(&judge_lane),
        "the record names the lane's own fingerprint (the fold's identity)"
    );
    assert_eq!(r.n_per_verdict_class, 2);
    // An always-"agree" judge has zero agreement beyond chance — the
    // honest Cohen's κ measurement is 0 (raw agreement .5, κ 0).
    assert_eq!(r.agreement.statistic, "cohen_kappa");
    assert_eq!(r.agreement.value_ppm, 0);
    assert_eq!(
        r.error_rates.get("fp_rate").and_then(Json::as_int),
        Some(1_000_000)
    );
    // Self-consistency: every case answered identically across passes.
    assert_eq!(r.self_consistency_ppm, Some(1_000_000));
    // 8 calls → 8 verbatim usage records (the leg's spend accounting).
    assert_eq!(run.usages.len(), 8);
    assert_eq!(run.observations.len(), 4);
    // The record feeds `calibration_status_at` — `active` under a
    // matching context.
    let ctx = CalibrationContext {
        judge_snapshot_fingerprint: judge_fingerprint(&judge_lane),
        rubric_ref: "rubric:v1".into(),
        task_family: "reconciliation".into(),
        now_ms: 2000,
        recalibration_period_ms: 1_000_000_000,
    };
    assert_eq!(
        hh_verification::critic_rt::calibration_status_at(r, &ctx),
        CalibrationStatus::Active
    );
}

/// A typed wire failure (closed port) still lands `oracle_failure` rows
/// — the consult happened, the answer is typed, never a silent zero.
#[test]
fn failed_consult_lands_oracle_failure_rows() {
    let judge_lane = lane("openai-compatible@1", "http://127.0.0.1:9", "m-a");
    let spec = bind_spec(
        "judge:a",
        judge_lane,
        "own-family",
        vec!["other-family".to_string()],
        None,
        CriticUse::Report,
    );
    let judge = link_judge(spec).unwrap();
    let ppm = judge.judge(&judged_spec("validator:judge-live"), &[]);
    assert_eq!(ppm, 0, "an oracle failure abstains (0), never a guess");
    let records = judge.take_records();
    assert!(
        records
            .iter()
            .any(|(c, p, _)| c == "verification.validator.verdict"
                && p.get("status").and_then(Json::as_str) == Some("oracle_failure")),
        "the typed failure lands as a durable row: {records:?}"
    );
}
