//! R5.26 — §6.3 isolated-replay and fresh-execution admission cards
//! (`admit_replay` / `admit_fresh`; `replay-card/1` / `fresh-card/1`).
//! AC-1: disposable copy + immutable original prefix, independent
//! attempted-provider/effect counters, R1 derivation with exact hashes
//! and declared decisions/assembly, card/prerequisite/resource
//! separation. AC-2: missing response/definition, changed derivation,
//! attempted live dispatch, original mutation, replay-suffix confusion,
//! retired model / missing seeds / MatchSpec / arms floor, unfunded
//! replication, author-only independence — all fail or refuse.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_bundle::assemble::{assemble, AssembleInputs, CompileOutcome};
use hharness_bundle::cards::{
    admit_fresh, admit_replay, CardVerdict, FreshClaim, FreshLevel, Isolation, RederivedObject,
    ReplayAttempt,
};
use hharness_bundle::codec::Decoded;
use hharness_bundle::error::BundleError;
use hharness_bundle::export::decode_page;
use hharness_bundle::manifest::{BundlePolicy, LedgerExport, MemberStatus};
use hharness_identity::idp_id;
use hharness_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-r526-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

struct Rig {
    run: String,
    decoded: Decoded,
}

/// A finished native run carrying a declared replay: two
/// `control.decision`/`context.assembled` pairs, one recorded provider
/// response (`model.call.attempt.completed{response_ref}`), a pinned
/// model snapshot, environment/registry/seed/nondeterminism/budget —
/// the R2+R3 basis set lands satisfied.
fn rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.seed = Some(7);
    let (run, lease) = store.open_run(m, "r526.test").unwrap();
    store
        .append(
            &run,
            &lease,
            vec![
                ev("evt-1", "lifecycle.component.bound", Json::obj([])),
                ev(
                    "evt-2",
                    "control.decision",
                    Json::obj([
                        ("kind", Json::str("act")),
                        ("decider", Json::str("strategy")),
                        ("call_no", Json::Int(1)),
                    ]),
                ),
                ev(
                    "evt-3",
                    "context.assembled",
                    Json::obj([("view_hash", Json::str("sha256:v1"))]),
                ),
                ev(
                    "evt-4",
                    "model.call.attempt.started",
                    Json::obj([("model_call_id", Json::str("mc-1"))]),
                ),
                ev(
                    "evt-5",
                    "model.call.attempt.completed",
                    Json::obj([
                        ("model_call_id", Json::str("mc-1")),
                        ("response_ref", Json::str("sha256:resp1")),
                    ]),
                ),
                ev(
                    "evt-6",
                    "control.decision",
                    Json::obj([
                        ("kind", Json::str("act")),
                        ("decider", Json::str("strategy")),
                        ("call_no", Json::Int(2)),
                    ]),
                ),
                ev(
                    "evt-7",
                    "context.assembled",
                    Json::obj([("view_hash", Json::str("sha256:v2"))]),
                ),
                ev("evt-8", "lifecycle.run.finished", Json::obj([])),
            ],
        )
        .unwrap();
    let get = |a: &str| (a == def_addr).then(|| def_bytes.clone());
    let ab: &dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    let compiled = CompileOutcome {
        bundle_id: "sha256:compiled1".into(),
        derivation_key: "sha256:dk1".into(),
        member_bytes: Json::obj([("bundle_id", Json::str("sha256:compiled1"))])
            .to_canonical_string()
            .into_bytes(),
        lcd_report: Json::obj([]),
        opacity_report: Json::obj([]),
        validators: vec![Json::obj([("validator", Json::str("v1"))])],
        profile_refs: vec![],
    };
    let policy = BundlePolicy::default();
    let inputs = AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("r526.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([
            ("platform", Json::str("test")),
            ("nondeterminism", Json::Arr(vec![Json::str("clock")])),
        ]),
        compiled: Some(compiled),
        model_snapshots: vec![Json::obj([
            ("model_id", Json::str("m-1")),
            ("pinned", Json::Bool(true)),
            ("observed_fingerprint", Json::str("sha256:fp1")),
        ])],
        registry_snapshot_id: Some("sha256:reg".into()),
        variants: vec![],
        extensions: vec![],
        budget: Json::obj([("budgeted", Json::Bool(true)), ("cap", Json::Int(10))]),
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
        evidence_profile: None,
    };
    let out = assemble(&inputs).unwrap();
    Rig {
        run,
        decoded: Decoded {
            manifest: out.manifest,
            members: out.members,
        },
    }
}

/// A minimal rig: a finished run carrying exactly `events`, a sealed
/// definition member and the caller-supplied extras — the harness_def
/// artifact is always served so assemble deposits the `definition`
/// member.
fn rig_min(
    tag: &str,
    events: Vec<(&str, &str, Json)>,
    inputs_extra: impl FnOnce(&mut AssembleInputs<'_>),
) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    m.seed = Some(7);
    let (run, lease) = store.open_run(m, "r526.min").unwrap();
    let evs: Vec<Event> = events
        .into_iter()
        .map(|(id, class, payload)| ev(id, class, payload))
        .collect();
    store.append(&run, &lease, evs).unwrap();
    let db = def_bytes.clone();
    let da = def_addr.clone();
    let get = move |a: &str| (a == da).then(|| db.clone());
    let ab: &dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    let policy = BundlePolicy::default();
    let mut inputs = AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("r526.min", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([
            ("platform", Json::str("test")),
            ("nondeterminism", Json::Arr(vec![Json::str("clock")])),
        ]),
        compiled: Some(CompileOutcome {
            bundle_id: "sha256:c".into(),
            derivation_key: "sha256:dk".into(),
            member_bytes: b"{}".to_vec(),
            lcd_report: Json::obj([]),
            opacity_report: Json::obj([]),
            validators: vec![Json::obj([("v", Json::Int(1))])],
            profile_refs: vec![],
        }),
        model_snapshots: vec![Json::obj([
            ("model_id", Json::str("m-1")),
            ("pinned", Json::Bool(true)),
            ("observed_fingerprint", Json::str("sha256:fp1")),
        ])],
        registry_snapshot_id: Some("sha256:reg".into()),
        variants: vec![],
        extensions: vec![],
        budget: Json::obj([("budgeted", Json::Bool(true))]),
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
        evidence_profile: None,
    };
    inputs_extra(&mut inputs);
    let out = assemble(&inputs).unwrap();
    Rig {
        run,
        decoded: Decoded {
            manifest: out.manifest,
            members: out.members,
        },
    }
}

/// The original exported prefix for `rig.run`, decoded from the
/// bundle's own page members (the recipient path — never the producer
/// store).
fn orig(rig: &Rig) -> Vec<EventEnvelope> {
    let export = &rig.decoded.manifest.traces[&rig.run];
    let mut out = Vec::new();
    for p in &export.pages {
        out.extend(decode_page(&rig.decoded.members[p]).unwrap());
    }
    out
}

/// A chained `lifecycle.replay.*` marker minted on the disposable copy —
/// the only permitted suffix rows.
fn marker(prev: &EventEnvelope, class: &str, event_id: &str) -> EventEnvelope {
    let mut e = prev.clone();
    e.event_id = event_id.to_string();
    e.seq = prev.seq + 1;
    e.class = class.to_string();
    e.prev_hash = prev.hash.clone();
    e.payload = Json::obj([("driver_mode", Json::str("deterministic"))]);
    e.causes = vec![];
    e.refs = vec![];
    e.ir_refs = vec![];
    e.surface_ids = BTreeMap::new();
    e.hash = e.recompute_hash();
    e
}

/// A clean isolated replay: original prefix + two permitted markers,
/// zero attempted dispatches, declared sequences echoed, full R1
/// rederivation when `r1` is claimed.
fn attempt(rig: &Rig, r1: bool) -> ReplayAttempt {
    let o = orig(rig);
    let mut copy = o.clone();
    let m1 = marker(copy.last().unwrap(), "lifecycle.replay.started", "evt-r1");
    copy.push(m1.clone());
    copy.push(marker(&m1, "lifecycle.replay.finished", "evt-r2"));
    let decisions: Vec<Json> = o
        .iter()
        .filter(|e| e.class == "control.decision")
        .map(|e| e.payload.clone())
        .collect();
    let assembled: Vec<Json> = o
        .iter()
        .filter(|e| e.class == "context.assembled")
        .map(|e| e.payload.clone())
        .collect();
    let mut rederived = Vec::new();
    if r1 {
        rederived.push(RederivedObject {
            authoritative: "version_id".into(),
            rederived: rig.decoded.manifest.compute_id(),
        });
        for e in &o {
            rederived.push(RederivedObject {
                authoritative: e.hash.clone(),
                rederived: e.recompute_hash(),
            });
        }
        for mm in &rig.decoded.manifest.members {
            if mm.status == MemberStatus::Present {
                rederived.push(RederivedObject {
                    authoritative: mm.address.clone(),
                    rederived: idp_id("blob", &rig.decoded.members[&mm.address]),
                });
            }
        }
        let ex = &rig.decoded.manifest.traces[&rig.run];
        rederived.push(RederivedObject {
            authoritative: ex.tree.clone(),
            rederived: LedgerExport::tree_address(&ex.pages),
        });
    }
    ReplayAttempt {
        copy,
        replayed_decisions: decisions,
        replayed_assembled: assembled,
        rederived,
        isolation: Isolation {
            provider_transport_disabled: true,
            effect_execution_disabled: true,
            attempted_provider_dispatches: 0,
            attempted_effect_executions: 0,
        },
        claim_r1: r1,
    }
}

fn claim(level: FreshLevel) -> FreshClaim {
    FreshClaim {
        level,
        new_subject: true,
        funded: true,
        model_retired: false,
        seeds: vec![7, 8],
        seed_distribution: Some("design:beta-blocks".into()),
        planned_arms: 4,
        min_arms: 4,
        matchspec: Some(Json::obj([("metric", Json::str("accuracy"))])),
        margins: Some(Json::obj([("delta", Json::Int(1))])),
        charging_retains_attribution: true,
        new_phase: true,
        comparison_criterion: Some("frozen-contrast".into()),
        comparison_target: Some("target-1".into()),
        outside_authority: Some("authority:lab".into()),
        outside_attestation: true,
        reproducer: Json::obj([(
            "attestation",
            Json::obj([("anchor", Json::obj([("signer", Json::str("sig:outside"))]))]),
        )]),
        reproducer_instrument: Json::obj([("installation_id", Json::str("inst-outside"))]),
        new_task_or_model: false,
    }
}

fn refused_code(r: Result<hharness_bundle::cards::AdmissionCard, BundleError>) -> String {
    match r {
        Err(BundleError::CardRefused { code, detail }) => format!("{code}:{detail}"),
        Err(e) => panic!("unexpected error {e:?}"),
        Ok(c) => panic!("expected refusal, got card {:?}", c.verdict),
    }
}

// ── AC-1 ─────────────────────────────────────────────────────────────

#[test]
fn isolated_replay_r1_admitted() {
    let r = rig("a1");
    let card = admit_replay(&r.decoded, &r.run, &attempt(&r, true)).unwrap();
    assert_eq!(card.schema, "replay-card/1");
    assert_eq!(card.procedure, "replay");
    assert_eq!(card.level, "r1");
    assert_eq!(card.verdict, CardVerdict::Admitted);
    assert!(card.failures.is_empty());
    assert!(card.legs.values().all(|l| l == "pass"));
    assert_eq!(
        card.counters.get("attempted_provider_dispatches"),
        Some(&Json::Int(0))
    );
    assert_eq!(
        card.counters.get("attempted_effect_executions"),
        Some(&Json::Int(0))
    );
    assert_eq!(card.counters.get("appended_markers"), Some(&Json::Int(2)));
    assert!(!card.card_id.is_empty());
}

#[test]
fn isolated_replay_without_r1_admitted() {
    let r = rig("a2");
    let card = admit_replay(&r.decoded, &r.run, &attempt(&r, false)).unwrap();
    assert_eq!(card.level, "replay");
    assert_eq!(card.verdict, CardVerdict::Admitted);
    assert_eq!(
        card.legs.get("rederivation").map(String::as_str),
        Some("n_a{not_claimed}")
    );
}

#[test]
fn fresh_r2_admitted() {
    let r = rig("a3");
    let card = admit_fresh(&r.decoded, &r.run, &claim(FreshLevel::R2)).unwrap();
    assert_eq!(card.schema, "fresh-card/1");
    assert_eq!(card.procedure, "fresh");
    assert_eq!(card.level, "r2");
    assert_eq!(card.verdict, CardVerdict::Admitted);
}

#[test]
fn fresh_r3_admitted() {
    let r = rig("a4");
    let card = admit_fresh(&r.decoded, &r.run, &claim(FreshLevel::R3)).unwrap();
    assert_eq!(card.level, "r3");
    assert_eq!(card.verdict, CardVerdict::Admitted);
    assert_eq!(card.legs.get("arms").map(String::as_str), Some("pass"));
}

#[test]
fn transfer_admitted_with_outside_material() {
    let mut r = rig("a5");
    // Declare producer-side identity material — a distinct signer and
    // installation from the reproducer's.
    if let Json::Obj(m) = &mut r.decoded.manifest.producer {
        m.insert(
            "attestation".into(),
            Json::obj([("anchor", Json::obj([("signer", Json::str("sig:prod"))]))]),
        );
    }
    if let Json::Obj(m) = &mut r.decoded.manifest.instrument {
        m.insert("installation_id".into(), Json::str("inst-prod"));
    }
    let card = admit_fresh(&r.decoded, &r.run, &claim(FreshLevel::Transfer)).unwrap();
    assert_eq!(card.procedure, "transfer");
    assert_eq!(card.level, "transfer");
    assert_eq!(card.verdict, CardVerdict::Admitted);
    assert_eq!(
        card.independence.get("basis"),
        Some(&Json::str("distinct_signer_and_installation"))
    );
}

#[test]
fn cards_stay_separate() {
    let r = rig("a6");
    let rc = admit_replay(&r.decoded, &r.run, &attempt(&r, true)).unwrap();
    let fc = admit_fresh(&r.decoded, &r.run, &claim(FreshLevel::R2)).unwrap();
    // One procedure per card, distinct schema ids — verify/recompute live
    // under their own owners' records (hh-repro/1, analysis-recompute/1).
    assert_ne!(rc.schema, fc.schema);
    assert_eq!(rc.procedure, "replay");
    assert_eq!(fc.procedure, "fresh");
    // prerequisites/resources/counters are separate members, never the verdict.
    for c in [&rc, &fc] {
        let j = c.to_json();
        assert!(j.get("prerequisites").is_some());
        assert!(j.get("resources").is_some());
        assert!(j.get("counters").is_some());
        assert!(j.get("verdict").is_some());
    }
    // distinct cards seal distinct ids
    assert_ne!(rc.card_id, fc.card_id);
}

// ── AC-2 ─────────────────────────────────────────────────────────────

#[test]
fn missing_definition_refuses() {
    let mut r = rig("b1");
    let def_addr = hharness_bundle::levels::roles::DEFINITION;
    let addr = r
        .decoded
        .manifest
        .members
        .iter()
        .find(|m| m.role == def_addr)
        .unwrap()
        .address
        .clone();
    r.decoded.members.remove(&addr);
    let code = refused_code(admit_replay(&r.decoded, &r.run, &attempt(&r, false)));
    assert!(code.starts_with("missing_input"), "{code}");
}

#[test]
fn missing_response_rows_refuses() {
    // Declared replay (a control.decision row) but no recorded provider
    // response rows — F4's "recorded responses" gate.
    let r = rig_min(
        "b2",
        vec![
            (
                "e1",
                "control.decision",
                Json::obj([
                    ("kind", Json::str("act")),
                    ("decider", Json::str("strategy")),
                ]),
            ),
            ("e2", "lifecycle.run.finished", Json::obj([])),
        ],
        |_| {},
    );
    assert_eq!(
        refused_code(admit_replay(&r.decoded, &r.run, &attempt(&r, false))),
        "missing_input:no recorded provider response rows"
    );
}

#[test]
fn undeclared_replay_refuses() {
    // Recorded responses but no control.decision — replay is undeclared.
    let r = rig_min(
        "b3",
        vec![
            (
                "e1",
                "model.call.attempt.completed",
                Json::obj([("response_ref", Json::str("sha256:r"))]),
            ),
            ("e2", "lifecycle.run.finished", Json::obj([])),
        ],
        |_| {},
    );
    assert_eq!(
        refused_code(admit_replay(&r.decoded, &r.run, &attempt(&r, false))),
        "unsupported:replay undeclared — the record carries no control.decision"
    );
}

#[test]
fn enabled_isolation_refuses() {
    let r = rig("b4");
    let mut a = attempt(&r, false);
    a.isolation.provider_transport_disabled = false;
    assert_eq!(
        refused_code(admit_replay(&r.decoded, &r.run, &a)),
        "isolation_disabled:provider_transport"
    );
    let mut a = attempt(&r, false);
    a.isolation.effect_execution_disabled = false;
    assert_eq!(
        refused_code(admit_replay(&r.decoded, &r.run, &a)),
        "isolation_disabled:effect_execution"
    );
}

#[test]
fn mutated_copy_prefix_fails() {
    let r = rig("b5");
    let mut a = attempt(&r, false);
    // Flip one payload in the copy's prefix — the original is untouched.
    a.copy[1].payload = Json::obj([("kind", Json::str("tampered"))]);
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("original_mutation"))));
}

#[test]
fn truncated_copy_prefix_fails() {
    let r = rig("b6");
    let mut a = attempt(&r, false);
    a.copy.truncate(a.copy.len() - 3);
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("original_mutation"))));
}

#[test]
fn non_marker_suffix_fails() {
    let r = rig("b7");
    let mut a = attempt(&r, false);
    let bogus = marker(a.copy.last().unwrap(), "lifecycle.run.finished", "evt-x");
    a.copy.push(bogus);
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("replay_suffix"))));
}

#[test]
fn attempted_live_dispatch_fails_even_if_declared_blocked() {
    let r = rig("b8");
    for (class, prov, eff) in [
        ("model.call.attempt.started", 1u64, 0u64),
        ("action.effect.committed", 0u64, 1u64),
    ] {
        let mut a = attempt(&r, false);
        let row = marker(a.copy.last().unwrap(), class, "evt-live");
        a.copy.push(row);
        // The honest shim counted the attempt — blocked or not, it fails.
        a.isolation.attempted_provider_dispatches = prov;
        a.isolation.attempted_effect_executions = eff;
        let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
        assert_eq!(card.verdict, CardVerdict::NotAdmitted);
        assert!(card
            .failures
            .iter()
            .any(|f| f.get("code") == Some(&Json::str("attempted_live_dispatch"))));
    }
    // A dishonest shim (declares 0) is caught by the card's own count.
    let mut a = attempt(&r, false);
    let row = marker(a.copy.last().unwrap(), "model.call.requested", "evt-live2");
    a.copy.push(row);
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("counter_dishonest"))));
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("attempted_live_dispatch"))));
}

#[test]
fn changed_derivation_fails() {
    let r = rig("b9");
    let mut a = attempt(&r, false);
    a.replayed_decisions[0] = Json::obj([
        ("kind", Json::str("other")),
        ("decider", Json::str("strategy")),
    ]);
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("changed_derivation"))));
    let mut a = attempt(&r, false);
    a.replayed_assembled.pop();
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("changed_derivation"))));
}

#[test]
fn r1_rederivation_gaps_fail() {
    let r = rig("b10");
    // Missing coverage — drop one required entry.
    let mut a = attempt(&r, true);
    a.rederived.pop();
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("rederivation"))));
    // Wrong hash — changed derivation.
    let mut a = attempt(&r, true);
    a.rederived[0].rederived = "sha256:wrong".into();
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("changed_derivation"))));
    // Unknown object name.
    let mut a = attempt(&r, true);
    a.rederived.push(RederivedObject {
        authoritative: "sha256:notaderivedobject".into(),
        rederived: "sha256:x".into(),
    });
    let card = admit_replay(&r.decoded, &r.run, &a).unwrap();
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("code") == Some(&Json::str("unknown_derived"))));
}

#[test]
fn fresh_gates_refuse() {
    let r = rig("b11");
    let run = r.run.as_str();
    let mut c = claim(FreshLevel::R2);
    c.funded = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("unfunded"));
    let mut c = claim(FreshLevel::R2);
    c.model_retired = true;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("unavailable:model_retired"));
    let mut c = claim(FreshLevel::R2);
    c.new_subject = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("not_fresh"));
    let mut c = claim(FreshLevel::R2);
    c.charging_retains_attribution = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("charging_attribution"));
    let mut c = claim(FreshLevel::R2);
    c.seeds = vec![];
    assert_eq!(
        refused_code(admit_fresh(&r.decoded, run, &c)),
        "unavailable:seeds"
    );
    let mut c = claim(FreshLevel::R2);
    c.margins = None;
    assert_eq!(
        refused_code(admit_fresh(&r.decoded, run, &c)),
        "unavailable:margins"
    );
}

#[test]
fn fresh_r3_blocks() {
    let r = rig("b12");
    let run = r.run.as_str();
    let mut c = claim(FreshLevel::R3);
    c.seed_distribution = None;
    assert_eq!(
        refused_code(admit_fresh(&r.decoded, run, &c)),
        "unavailable:seed_distribution"
    );
    let mut c = claim(FreshLevel::R3);
    c.matchspec = None;
    assert_eq!(
        refused_code(admit_fresh(&r.decoded, run, &c)),
        "unavailable:matchspec"
    );
    // n < k and the one-demo floor.
    let mut c = claim(FreshLevel::R3);
    c.planned_arms = 2;
    c.min_arms = 4;
    let card = admit_fresh(&r.decoded, run, &c).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    let mut c = claim(FreshLevel::R3);
    c.planned_arms = 1;
    c.min_arms = 1;
    let card = admit_fresh(&r.decoded, run, &c).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
}

#[test]
fn transfer_independence_gates() {
    let r = rig("b13");
    let run = r.run.as_str();
    // Author-only clean directory: same signer + same installation.
    let mut c = claim(FreshLevel::Transfer);
    c.reproducer = r.decoded.manifest.producer.clone();
    c.reproducer_instrument = r.decoded.manifest.instrument.clone();
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("not_independent"));
    let mut c = claim(FreshLevel::Transfer);
    c.outside_authority = None;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("outside_relationship"));
    let mut c = claim(FreshLevel::Transfer);
    c.outside_attestation = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("outside_relationship"));
    let mut c = claim(FreshLevel::Transfer);
    c.new_phase = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("no_new_phase"));
    let mut c = claim(FreshLevel::Transfer);
    c.comparison_target = None;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("unavailable:comparison"));
    let mut c = claim(FreshLevel::Transfer);
    c.new_task_or_model = true;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("new_question"));
    let mut c = claim(FreshLevel::Transfer);
    c.funded = false;
    assert!(refused_code(admit_fresh(&r.decoded, run, &c)).starts_with("unfunded"));
}

#[test]
fn fresh_unpinned_model_basis_fails() {
    // Unpinned model snapshot without a fingerprint — the R2 basis entry
    // B-R2-model is Missing → not_admitted, never a level.
    let r = rig_min(
        "b14",
        vec![
            (
                "e1",
                "control.decision",
                Json::obj([
                    ("kind", Json::str("act")),
                    ("decider", Json::str("strategy")),
                ]),
            ),
            (
                "e2",
                "model.call.attempt.completed",
                Json::obj([("response_ref", Json::str("sha256:r"))]),
            ),
            ("e3", "lifecycle.run.finished", Json::obj([])),
        ],
        |i| {
            i.model_snapshots = vec![Json::obj([
                ("model_id", Json::str("m-1")),
                ("pinned", Json::Bool(false)),
            ])];
        },
    );
    let card = admit_fresh(&r.decoded, &r.run, &claim(FreshLevel::R2)).unwrap();
    assert_eq!(card.verdict, CardVerdict::NotAdmitted);
    assert!(card
        .failures
        .iter()
        .any(|f| f.get("leg") == Some(&Json::str("native_basis"))));
}
