//! S3.6 — the ledger half of the replay slice (R-2.2.4⁰ᵇ; §5a.4; ADR-0135):
//! `ReplayValidityReport` over the pure fold, both recording rules the
//! verdict reads (`calls`/`response_ref` on `model.call.completed`,
//! `control.clock.read`/`control.random.read`), the `reconstruct` driver's
//! view rebuild, `lifecycle.replay.started`/`finished` durable bookkeeping,
//! and the `effects_by_key`/`stored_observation` dedup consult.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.4-2  — the four-valued verdict over recorded evidence.
//! - AC-R-2.2.4-4  — `reconstruct` rebuilds views + env binding, nothing
//!   re-executed.
//! - AC-R-2.2.4-5  — `deterministic` claims need a declaring variant *and*
//!   a reproduced drive.
//! - AC-R-2.2.4-6  — divergence ⇒ `invalid`; fingerprint/probe/coverage
//!   gaps ⇒ `degraded`.
//! - AC-R-2.2.4-7  — `started`/`finished` rows are durable; `report_ref`
//!   names the content-addressed report.
//! - AC-R-2.2.4-8  — the report is canonical + rebuildable (equal
//!   `content_ref` across folds).
//! - AC-R-2.2.4-11 — rebuildability: the report rebuilds from durable
//!   ledger state alone.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::effect::idempotency_key;
use hh_ledger::event::{Cursor, Direction, Event, Producer, Scope};
use hh_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::replay::{
    self, DependencyProbe, ReplayDriverMode, ReplayValidityReport, SourceKind, ValidityInput,
    ValidityMode,
};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-replay-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

fn k_ev(id: &str, class: &str, payload: Json) -> Event {
    let mut e = ev(id, class, payload);
    e.producer = Producer::kernel("kernel:test");
    e.provenance = Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0));
    e
}

/// The `turn ⊃ model_call` scope chain the recorded run carries.
fn open_chain(s: &mut Store, run: &str, lease: &Lease) {
    let mut t = k_ev("t1", "lifecycle.turn.started", Json::obj([]));
    t.scope.turn_id = Some("turn-1".into());
    let mut m = k_ev("m1", "model.call.requested", Json::obj([]));
    m.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    s.append(run, lease, vec![t, m]).unwrap();
}

/// A fully-recorded `model.call.completed` — `calls` + `response_ref`
/// present (the S3.6 recording rule).
fn completed_call(id: &str, with_calls: bool) -> Event {
    let mut e = k_ev(
        id,
        "model.call.completed",
        Json::obj([
            ("model_call_id", Json::str("mc-1")),
            ("attempt_no", Json::Int(1)),
            ("stop_reason", Json::str("stop")),
            ("response_ref", Json::str("sha256:resp")),
        ]),
    );
    if with_calls {
        if let Json::Obj(m) = &mut e.payload {
            m.insert("calls".to_string(), Json::Arr(vec![]));
        }
    }
    e.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    e
}

fn classes_of(s: &Store, run: &str) -> Vec<String> {
    s.read(run, Cursor::Seq(0), None, Direction::Fwd, usize::MAX)
        .unwrap()
        .events
        .iter()
        .map(|e| e.class.clone())
        .collect()
}

// ── spellings ────────────────────────────────────────────────────────────────

#[test]
fn replay_vocabularies_round_trip() {
    for m in [
        ReplayDriverMode::Reconstruct,
        ReplayDriverMode::Deterministic,
    ] {
        assert_eq!(ReplayDriverMode::parse(m.as_str()), Some(m));
    }
    for v in [
        ValidityMode::Deterministic,
        ValidityMode::ReExecuted,
        ValidityMode::Degraded,
        ValidityMode::Invalid,
    ] {
        assert_eq!(ValidityMode::parse(v.as_str()), Some(v));
    }
    for k in [
        SourceKind::ProviderSampling,
        SourceKind::WallClock,
        SourceKind::Randomness,
        SourceKind::HumanInput,
        SourceKind::EnvironmentState,
        SourceKind::Network,
        SourceKind::ExternalService,
        SourceKind::FilesystemOrder,
        SourceKind::Concurrency,
    ] {
        assert_eq!(SourceKind::parse(k.as_str()), Some(k));
    }
    assert_eq!(ReplayDriverMode::parse("nope"), None);
    assert_eq!(ValidityMode::parse("nope"), None);
    assert_eq!(SourceKind::parse("nope"), None);
}

// ── AC-R-2.2.4-2/-5/-6 — the validity fold ───────────────────────────────────

#[test]
fn ac_r_2_2_4_5_deterministic_requires_declaration_and_reproduction() {
    let (mut s, run, lease) = open("validity-det");
    open_chain(&mut s, &run, &lease);
    s.append(&run, &lease, vec![completed_call("mc1done", true)])
        .unwrap();

    // A non-declaring variant never claims `deterministic` — and without
    // the declaration the clock/random reads cannot be proven confined:
    // the honest verdict is `degraded` (uncaptured sources), not a silent
    // `re_executed` pass.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Degraded, "{:?}", r.reasons);
    assert!(r.reasons.iter().any(|x| x == "uncaptured_sources"));

    // Declaring + reproduced ⇒ `deterministic`.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            reproduced: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Deterministic, "{:?}", r.reasons);

    // Declaring but the drive did not reproduce ⇒ `invalid`
    // (`replay_diverged` — a false `deterministic` claim is worse than a
    // refusal).
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            reproduced: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Invalid);
    assert!(r.reasons.iter().any(|x| x == "replay_diverged"));
}

#[test]
fn ac_r_2_2_4_6_divergence_invalidates_and_gaps_degrade() {
    let (mut s, run, lease) = open("validity-modes");
    open_chain(&mut s, &run, &lease);
    s.append(&run, &lease, vec![completed_call("mc1done", true)])
        .unwrap();

    // An explicit divergence ⇒ `invalid`.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            diverged: Some((7, "sha256:aaa".into(), "sha256:bbb".into())),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Invalid);
    assert!(r.reasons.iter().any(|x| x == "replay_diverged"));

    // A fingerprint mismatch ⇒ `degraded`.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            fingerprint_match: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Degraded);
    assert!(r.reasons.iter().any(|x| x == "fingerprint_mismatch"));

    // A contradicting probe ⇒ `degraded`.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            probes: vec![DependencyProbe {
                service: "svc:payment".into(),
                probed_verdict: "contradict".into(),
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Degraded);
    assert!(r.reasons.iter().any(|x| x == "contradicting_probe"));

    // Uncaptured sources the caller knows were read ⇒ `degraded`.
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            uncaptured: vec!["wall_clock".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::Degraded);
    assert!(r.reasons.iter().any(|x| x == "uncaptured_sources"));
}

/// The recording rule the confinement check reads: a `model.call.completed`
/// without the `calls` member is *unrecorded* — the verdict degrades to
/// `re_executed`, never a silent `deterministic` claim.
#[test]
fn ac_r_2_2_4_2_unrecorded_model_io_is_re_executed() {
    let (mut s, run, lease) = open("validity-unrec");
    open_chain(&mut s, &run, &lease);
    s.append(&run, &lease, vec![completed_call("mc1done", false)])
        .unwrap();
    let r = replay::validity(
        &s,
        &run,
        &ValidityInput {
            variant_declares: true,
            reproduced: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.mode, ValidityMode::ReExecuted, "{:?}", r.reasons);
    assert!(r.reasons.iter().any(|x| x == "model_io_unrecorded"));
}

// ── AC-R-2.2.4-7/-8/-11 — durable bookkeeping + rebuildability ───────────────

#[test]
fn ac_r_2_2_4_7_started_finished_rows_are_durable() {
    let (mut s, run, _lease) = open("replay-rows");
    replay::replay_started(&mut s, &run, ReplayDriverMode::Reconstruct, Some(4)).unwrap();
    let report = replay::validity(&s, &run, &ValidityInput::default()).unwrap();
    replay::replay_finished(&mut s, &run, ReplayDriverMode::Reconstruct, &report).unwrap();

    let classes = classes_of(&s, &run);
    assert!(classes.contains(&"lifecycle.replay.started".to_string()));
    assert!(classes.contains(&"lifecycle.replay.finished".to_string()));
    // `finished{mode, report_ref}` names the stored report blob.
    let fin = s
        .envelopes(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "lifecycle.replay.finished")
        .unwrap();
    let report_ref = fin
        .payload
        .get("report_ref")
        .and_then(Json::as_str)
        .unwrap();
    assert_eq!(report_ref, report.content_ref());
}

#[test]
fn ac_r_2_2_4_11_report_rebuilds_from_durable_state() {
    let (mut s, run, lease) = open("rebuild");
    open_chain(&mut s, &run, &lease);
    s.append(&run, &lease, vec![completed_call("mc1done", true)])
        .unwrap();
    let input = ValidityInput {
        variant_declares: true,
        reproduced: Some(true),
        ..Default::default()
    };
    let a = replay::validity(&s, &run, &input).unwrap();
    let b = replay::validity(&s, &run, &input).unwrap();
    assert_eq!(a, b);
    // The canonical form round-trips and the content ref is stable.
    let parsed = ReplayValidityReport::from_json(&a.to_json()).unwrap();
    assert_eq!(parsed, a);
    assert_eq!(a.content_ref(), b.content_ref());
}

// ── AC-R-2.2.4-4 — the reconstruct driver ────────────────────────────────────

#[test]
fn ac_r_2_2_4_4_reconstruct_rebuilds_views_without_executing() {
    let (mut s, run, lease) = open("recon");
    open_chain(&mut s, &run, &lease);
    s.append(&run, &lease, vec![completed_call("mc1done", true)])
        .unwrap();
    let r = replay::reconstruct(&s, &run, None).unwrap();
    // The six views rebuilt — each carries its `view_hash` (the rebuild
    // evidence; a second build returns the identical hash).
    assert_eq!(r.views.len(), 6);
    for v in &r.views {
        assert!(!v.view_hash.is_empty());
    }
    let r2 = replay::reconstruct(&s, &run, None).unwrap();
    assert_eq!(
        r.views.iter().map(|v| &v.view_hash).collect::<Vec<_>>(),
        r2.views.iter().map(|v| &v.view_hash).collect::<Vec<_>>(),
        "reconstruct is not rebuildable"
    );
    // `until` bounds the rebuild — the prefix's views differ from the tip's
    // only where the prefix excludes rows (run_summary folds strictly more
    // over the full log).
    let prefix = replay::reconstruct(&s, &run, Some(0)).unwrap();
    assert_eq!(prefix.views.len(), 6);
    // Nothing re-executed: reconstruct never writes.
    assert_eq!(classes_of(&s, &run).len(), 5);
}

// ── effects_by_key / stored_observation — the durable dedup consult ──────────

fn intended(id: &str, effect_id: &str, extra: Json) -> Event {
    let mut m = BTreeMap::from([(
        "effective_risk_class".to_string(),
        Json::obj([
            ("reversibility", Json::str("reversible")),
            ("repeat_safety", Json::str("idempotent")),
            ("scope", Json::str("workspace_local")),
        ]),
    )]);
    if let Json::Obj(x) = extra {
        m.extend(x);
    }
    let mut e = k_ev(id, "action.effect.intended", Json::Obj(m));
    e.scope.effect_id = Some(effect_id.to_string());
    e
}

fn append_lifecycle(s: &mut Store, run: &str, lease: &Lease, effect_id: &str, key: &str) {
    let mut auth = k_ev(
        "a0",
        "action.effect.authorized",
        Json::obj([(
            "effective_risk_class",
            Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("workspace_local")),
            ]),
        )]),
    );
    auth.scope.effect_id = Some(effect_id.to_string());
    let mut prep = k_ev(
        "p1",
        "action.effect.prepared",
        Json::obj([
            ("idempotency_key", Json::str(key)),
            ("baseline_ref", Json::str("sha256:baseline")),
        ]),
    );
    prep.scope.effect_id = Some(effect_id.to_string());
    let mut dec = k_ev(
        "d1",
        "security.permission.decided",
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("attempt_no", Json::Int(1)),
            ("decision", Json::str("allow")),
        ]),
    );
    dec.scope.effect_id = Some(effect_id.to_string());
    let mut com = k_ev(
        "c1",
        "action.effect.committed",
        Json::obj([
            ("attempt_no", Json::Int(1)),
            ("fencing_token", Json::Int(lease.generation as i64)),
        ]),
    );
    com.scope.effect_id = Some(effect_id.to_string());
    let mut obs = k_ev(
        "o1",
        "action.effect.observed",
        Json::obj([
            ("attempt_no", Json::Int(1)),
            ("outcome", Json::str("applied")),
            ("fencing_token", Json::Int(lease.generation as i64)),
        ]),
    );
    obs.scope.effect_id = Some(effect_id.to_string());
    s.append(
        run,
        lease,
        vec![
            intended("i1", effect_id, Json::Null),
            auth,
            prep,
            dec,
            com,
            obs,
        ],
    )
    .unwrap();
}

#[test]
fn ac_r_2_2_2_11_effects_by_key_is_durable_and_observation_is_served() {
    let tag = "bykey";
    let root = dir(tag);
    let (mut s, run, lease) = {
        let clock = ManualClock::at(1_000);
        let mut s = Store::open_with(
            &root,
            Box::new(clock),
            Some(Box::new(SeqIds::new())),
            DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        (s, run, lease)
    };
    let effect_id = Store::effect_id(&run, "mc-1", "tc-1", 0);
    let key = idempotency_key(&run, &effect_id, "sha256:args", "v-cap");
    append_lifecycle(&mut s, &run, &lease, &effect_id, &key);

    // The projection answers key → {effect_id, phase, outcome}.
    let evs = s.envelopes(&run).unwrap();
    let view = hh_ledger::views::effects_by_key_view(&run, evs, None);
    let Json::Obj(m) = view.payload.get("effects").unwrap().clone() else {
        panic!("effects_by_key payload malformed")
    };
    let entry = m.get(&key).unwrap();
    assert_eq!(
        entry.get("effect_id").and_then(Json::as_str),
        Some(effect_id.as_str())
    );
    assert_eq!(entry.get("phase").and_then(Json::as_str), Some("observed"));
    // The stored observation is the recorded payload — served, never re-run.
    let obs = replay::stored_observation(&s, &run, &effect_id)
        .unwrap()
        .unwrap();
    assert_eq!(obs.get("outcome").and_then(Json::as_str), Some("applied"));
    // The durable-side lookup the dispatcher consults.
    let hit = replay::key_lookup(&s, &run, &key).unwrap().unwrap();
    assert_eq!(hit.0, effect_id);
    assert_eq!(hit.1, hh_ledger::effect::EffectPhase::Observed);
    assert_eq!(replay::key_lookup(&s, &run, "sha256:nope").unwrap(), None);

    drop(s);
    // Restart — the projection rebuilds from the WAL, not process memory.
    let s2 = Store::open_with(
        &root,
        Box::new(ManualClock::at(120_000)),
        Some(Box::new(SeqIds::starting_at(1_000_000))),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let evs2 = s2.envelopes(&run).unwrap();
    let view2 = hh_ledger::views::effects_by_key_view(&run, evs2, None);
    assert_eq!(
        view2.view_hash, view.view_hash,
        "effects_by_key did not survive restart"
    );
    assert_eq!(
        replay::stored_observation(&s2, &run, &effect_id)
            .unwrap()
            .unwrap()
            .get("outcome")
            .and_then(Json::as_str),
        Some("applied")
    );
}
