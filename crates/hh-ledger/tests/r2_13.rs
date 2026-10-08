//! R2.13 acceptance coverage — the audit residual (R-2.8.6 C2; DF-S1.15-3;
//! DF-S1.15-1; ADR-0345):
//!
//! * witness cosignatures — `witness_policy` on the manifest,
//!   `checkpoint_witnessed`/`rotation_checkpoint_witnessed`, the shared
//!   `check_witness_cosignatures` check across `verify_run`, the independent
//!   `Auditor`, and `audit_view`'s `witness_status`;
//! * receiver receipts — `lifecycle.ledger.receipt` durable rows lifted
//!   `unverified`/`external` through `Store::record_receipt`, bound to the
//!   run's effect fold;
//! * the GC/redaction durable-before-delete contract exercised together
//!   with the new rows.
//!
//! Each test fails if the behaviour it covers is removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_identity::rotation::RotationPlan;
use hh_ledger::audit::{
    check_witness_cosignatures, Auditor, CheckpointKind, FixedSigner, KeyTable, NamedSigner,
    WitnessSigner,
};
use hh_ledger::errors::{LedgerError, MissingReason, TamperedKind};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest, WitnessDecl, WitnessPolicy};
use hh_ledger::receipt::{Receipt, RECEIPT_STATUS_UNVERIFIED};
use hh_ledger::rotation::rotate_witnessed;
use hh_ledger::store::{Lease, Store};
use hh_ledger::tree;
use hh_ledger::views::ViewKind;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-r213-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A signed run whose manifest declares a two-of-two witness quorum.
fn witnessed_manifest(required: u64) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.signer_key_ids = vec!["test-key".to_string()];
    m.witness_policy = Some(WitnessPolicy {
        required,
        witnesses: vec![
            WitnessDecl {
                witness_name: "wit-alpha".into(),
                key_id: "wit-key-1".into(),
            },
            WitnessDecl {
                witness_name: "wit-beta".into(),
                key_id: "wit-key-2".into(),
            },
        ],
    });
    m
}

fn signed_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.signer_key_ids = vec!["test-key".to_string()];
    m
}

fn open(tag: &str, manifest: RunManifest) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest, "writer-a").unwrap();
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

/// An `action.effect.*` row — audit-grade ⇒ kernel producer + provenance.
fn eff(id: &str, class: &str, effect_id: &str, payload: Json, fencing: u64) -> Event {
    let mut payload = payload;
    if fencing > 0 {
        if let Json::Obj(m) = &mut payload {
            m.insert("fencing_token".to_string(), Json::Int(fencing as i64));
        }
    }
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: if class.starts_with("action.effect.") {
                Some(effect_id.to_string())
            } else {
                None
            },
            ..Scope::default()
        },
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

fn signer() -> FixedSigner {
    FixedSigner::new("test-key", b"r2.13-test-signing-key".to_vec())
}

fn wit1() -> NamedSigner {
    NamedSigner::new("wit-alpha", "wit-key-1", b"witness-key-1".to_vec())
}

fn wit2() -> NamedSigner {
    NamedSigner::new("wit-beta", "wit-key-2", b"witness-key-2".to_vec())
}

fn keys() -> KeyTable {
    KeyTable(BTreeMap::from([
        ("test-key".to_string(), b"r2.13-test-signing-key".to_vec()),
        ("wit-key-1".to_string(), b"witness-key-1".to_vec()),
        ("wit-key-2".to_string(), b"witness-key-2".to_vec()),
    ]))
}

fn fill(s: &mut Store, run: &str, lease: &Lease, tag: &str, n: usize) {
    let batch: Vec<Event> = (0..n)
        .map(|i| {
            ev(
                &format!("{tag}-{i}"),
                "context.observation.recorded",
                Json::str(format!("{tag}-{i}")),
            )
        })
        .collect();
    s.append(run, lease, batch).unwrap();
}

fn risk(rev: &str, rs: &str, scope: &str) -> Json {
    Json::obj([
        ("reversibility", Json::str(rev)),
        ("repeat_safety", Json::str(rs)),
        ("scope", Json::str(scope)),
    ])
}

/// The full legal chain `intended → authorized → decided{allow} → prepared
/// → committed → observed{applied}` — returns the `observed` event id.
fn apply_effect(s: &mut Store, run: &str, lease: &Lease, eid: &str) -> String {
    let rc = risk("irreversible", "non_idempotent", "external");
    s.append(
        run,
        lease,
        vec![
            eff(
                &format!("{eid}-i"),
                "action.effect.intended",
                eid,
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("effective_risk_class", rc.clone()),
                ]),
                0,
            ),
            eff(
                &format!("{eid}-a"),
                "action.effect.authorized",
                eid,
                Json::obj([("effective_risk_class", rc)]),
                0,
            ),
            eff(
                &format!("{eid}-d"),
                "security.permission.decided",
                eid,
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("attempt_no", Json::Int(1)),
                    ("decision", Json::str("allow")),
                ]),
                0,
            ),
            eff(
                &format!("{eid}-p"),
                "action.effect.prepared",
                eid,
                Json::obj([("idempotency_key", Json::str(format!("key-{eid}")))]),
                0,
            ),
        ],
    )
    .unwrap();
    s.append(
        run,
        lease,
        vec![eff(
            &format!("{eid}-c"),
            "action.effect.committed",
            eid,
            Json::obj([("attempt_no", Json::Int(1))]),
            lease.generation,
        )],
    )
    .unwrap();
    let obs = format!("{eid}-o");
    s.append(
        run,
        lease,
        vec![eff(
            &obs,
            "action.effect.observed",
            eid,
            Json::obj([
                ("attempt_no", Json::Int(1)),
                ("outcome", Json::str("applied")),
            ]),
            lease.generation,
        )],
    )
    .unwrap();
    obs
}

fn receipt(effect_id: &str, observed_ref: Option<&str>, receipt_id: &str) -> Receipt {
    Receipt {
        effect_id: effect_id.to_string(),
        observed_ref: observed_ref.map(str::to_string),
        receiver_ref: "receiver:hosted-participant-1".into(),
        receipt_id: receipt_id.to_string(),
        attested_at: 1_700_000_000_000,
        attestation: Json::obj([
            ("key_id", Json::str("receiver-key-7")),
            ("alg_ref", Json::str("receiver-local-scheme")),
            ("sig", Json::str("hex:0123456789abcdef")),
        ]),
        supplied_by: "writer-a".into(),
        claim_ref: None,
    }
}

// ── witness_policy manifest member ───────────────────────────────────────────

#[test]
fn witness_policy_manifest_codec_round_trip_and_validate() {
    let m = witnessed_manifest(1);
    let j = m.to_json();
    let back = RunManifest::from_json(&j).unwrap();
    let p = back.witness_policy.clone().unwrap();
    assert_eq!(p.required, 1);
    assert!(p.declares("wit-alpha", "wit-key-1"));
    assert!(p.declares("wit-beta", "wit-key-2"));
    assert!(!p.declares("wit-alpha", "wit-key-2"));
    assert!(p.declares_key("wit-key-1"));

    // Absent on a plain manifest — pre-C2 manifests are untouched.
    let plain = RunManifest::minimal(RunKind::Agent);
    assert!(plain.to_json().get("witness_policy").is_none());
    assert!(RunManifest::from_json(&plain.to_json())
        .unwrap()
        .witness_policy
        .is_none());

    // Validation refuses degenerate declarations.
    for bad in [
        {
            let mut m = signed_manifest();
            m.witness_policy = Some(WitnessPolicy {
                required: 1,
                witnesses: vec![],
            });
            m
        },
        {
            let mut m = signed_manifest();
            m.witness_policy = Some(WitnessPolicy {
                required: 3,
                witnesses: vec![
                    WitnessDecl {
                        witness_name: "w".into(),
                        key_id: "k".into(),
                    },
                    WitnessDecl {
                        witness_name: "w".into(),
                        key_id: "k".into(),
                    },
                ],
            });
            m
        },
    ] {
        assert!(bad.validate().is_err());
    }
}

// ── witness cosignatures ─────────────────────────────────────────────────────

#[test]
fn witnessed_checkpoint_cosigns_the_same_note_and_verifies() {
    let (mut s, run, lease) = open("wit", witnessed_manifest(2));
    fill(&mut s, &run, &lease, "a", 4);

    let mut w1 = wit1();
    let mut w2 = wit2();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1, &mut w2];
    let env = s
        .checkpoint_witnessed(
            &run,
            &lease,
            CheckpointKind::Periodic,
            &mut signer(),
            &mut witnesses,
        )
        .unwrap();
    let claim = tree::parse_checkpoint(&env.payload).unwrap();
    assert_eq!(claim.witness_cosignatures.len(), 2);
    // The cosignature covers `timestamp ‖ <the same unsigned note>` —
    // recompute the HMAC by hand against the durable payload.
    for c in &claim.witness_cosignatures {
        let kid = c.get("key_id").and_then(Json::as_str).unwrap();
        let ts = c.get("timestamp").and_then(Json::as_int).unwrap() as u64;
        let sig =
            hh_ledger::audit::parse_sig(c.get("sig").and_then(Json::as_str).unwrap()).unwrap();
        let key = match kid {
            "wit-key-1" => b"witness-key-1".to_vec(),
            "wit-key-2" => b"witness-key-2".to_vec(),
            other => panic!("unexpected witness {other}"),
        };
        let preimage = tree::witness_sig_preimage(ts, &env.payload);
        assert_eq!(
            hh_wire::sha256::hmac_sha256(&key, &preimage).to_vec(),
            sig,
            "cosignature must cover timestamp ‖ unsigned note"
        );
        assert!(c.get("witness_name").and_then(Json::as_str).is_some());
    }

    // verify_run with the resolver — primary + cosignatures verify.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
    // The independent auditor agrees (the durable manifest supplies the
    // policy — the auditor's own fold).
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    assert_eq!(aud.claims().len(), 1);

    // audit_view reports the witness status per checkpoint.
    s.set_audit_key_resolver(Box::new(keys()));
    let v = s.project(&run, ViewKind::AuditView, None).unwrap();
    let Json::Arr(cps) = v.payload.get("checkpoints").unwrap() else {
        panic!("checkpoints not an array")
    };
    assert_eq!(cps.len(), 1);
    assert_eq!(
        cps[0].get("witness_status").and_then(Json::as_str),
        Some("verified")
    );
    assert_eq!(
        cps[0].get("verification_status").and_then(Json::as_str),
        Some("verified")
    );
    let Json::Arr(cos) = cps[0].get("witness_cosignatures").unwrap() else {
        panic!("witness_cosignatures not an array")
    };
    assert_eq!(cos.len(), 2);
}

#[test]
fn mint_refuses_sub_quorum_and_undeclared_witnesses() {
    // required=2 but only one witness supplied — SignerUnavailable, no row.
    let (mut s, run, lease) = open("wit-quorum", witnessed_manifest(2));
    fill(&mut s, &run, &lease, "a", 2);
    let mut w1 = wit1();
    let mut only_one: Vec<&mut dyn WitnessSigner> = vec![&mut w1];
    match s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut only_one,
    ) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
    assert!(!s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "security.audit.checkpoint"));

    // An undeclared (name, key_id) pair is refused the same way.
    let mut bad_wit = NamedSigner::new("wit-alpha", "wit-key-9", b"k".to_vec());
    let mut w2 = wit2();
    let mut decl_bad: Vec<&mut dyn WitnessSigner> = vec![&mut bad_wit, &mut w2];
    match s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut decl_bad,
    ) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }

    // The plain pre-C2 op also refuses on a declared-but-unsatisfied
    // policy — the kernel never mints a head its own policy cannot
    // substantiate.
    match s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer()) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}

#[test]
fn verify_run_flags_a_bad_witness_signature() {
    let (mut s, run, lease) = open("wit-bad", witnessed_manifest(1));
    fill(&mut s, &run, &lease, "a", 3);
    let mut w1 = wit1();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1];
    s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut witnesses,
    )
    .unwrap();
    // A resolver whose witness key bytes differ — the HMAC mismatches and
    // verify_run reports `bad_signature`, never a pass.
    let wrong = KeyTable(BTreeMap::from([
        ("test-key".to_string(), b"r2.13-test-signing-key".to_vec()),
        ("wit-key-1".to_string(), b"WRONG".to_vec()),
    ]));
    match s.verify_run(&run, None, None, Some(&wrong)) {
        Err(LedgerError::Tampered(t)) => assert_eq!(t.kind, TamperedKind::BadSignature),
        other => panic!("expected Tampered{{bad_signature}}, got {other:?}"),
    }
    // And the independent auditor faults the same way.
    let mut aud = Auditor::new(&run);
    assert!(s.audit_with(&run, &mut aud, Some(&wrong)).is_err());
}

#[test]
fn witness_resolver_gap_is_a_custody_refusal_not_a_pass() {
    let (mut s, run, lease) = open("wit-gap", witnessed_manifest(1));
    fill(&mut s, &run, &lease, "a", 2);
    let mut w1 = wit1();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1];
    s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut witnesses,
    )
    .unwrap();
    // The declared witness key does not resolve — `SignerUnavailable`,
    // the same honest custody gap as an unresolvable primary key.
    let partial = KeyTable(BTreeMap::from([(
        "test-key".to_string(),
        b"r2.13-test-signing-key".to_vec(),
    )]));
    match s.verify_run(&run, None, None, Some(&partial)) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
    // Without a resolver the view degrades honestly — shape/declaration/
    // quorum checked, HMAC `unverified`.
    let v = s.project(&run, ViewKind::AuditView, None).unwrap();
    let Json::Arr(cps) = v.payload.get("checkpoints").unwrap() else {
        panic!("checkpoints not an array")
    };
    assert_eq!(
        cps[0].get("witness_status").and_then(Json::as_str),
        Some("unverified")
    );
    assert_eq!(
        cps[0].get("verification_status").and_then(Json::as_str),
        Some("unverified")
    );
}

#[test]
fn witnessed_rotation_cosigns_the_post_rotation_note_and_chains() {
    let (mut s, run, lease) = open("wit-rot", witnessed_manifest(1));
    fill(&mut s, &run, &lease, "pre", 3);
    let mut w1 = wit1();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1];
    s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut witnesses,
    )
    .unwrap();

    let plan = RotationPlan {
        from_idp: "idp/1".to_string(),
        to_idp: "idp/2".to_string(),
        declared_at: run.clone(),
        reason: "rotate test".to_string(),
        bridge: true,
        attestation_ref: None,
        effective_from_seq: None,
        rehash: true,
    };
    let mut w2 = wit2();
    let mut rot_witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w2];
    let receipt = rotate_witnessed(
        &mut s,
        &run,
        &lease,
        &mut signer(),
        &plan,
        &mut rot_witnesses,
    )
    .unwrap();
    let claim = tree::parse_checkpoint(&receipt.checkpoint.payload).unwrap();
    assert_eq!(claim.kind, "rotation");
    assert_eq!(claim.identity_profile.as_deref(), Some("idp/2"));
    // The rotation claim is cosigned — over the *post-rotation* note.
    assert_eq!(claim.witness_cosignatures.len(), 1);
    assert_eq!(
        claim.witness_cosignatures[0]
            .get("key_id")
            .and_then(Json::as_str),
        Some("wit-key-2")
    );

    // A post-rotation witnessed checkpoint chains to the rotation claim.
    fill(&mut s, &run, &lease, "post", 2);
    let mut w1b = wit1();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1b];
    let post = s
        .checkpoint_witnessed(
            &run,
            &lease,
            CheckpointKind::OnDemand,
            &mut signer(),
            &mut witnesses,
        )
        .unwrap();
    let post_claim = tree::parse_checkpoint(&post.payload).unwrap();
    assert_eq!(post_claim.identity_profile.as_deref(), Some("idp/2"));
    assert_eq!(post_claim.witness_cosignatures.len(), 1);

    // The whole corpus verifies + audits under the quorum.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    assert_eq!(aud.claims().len(), 3);
}

#[test]
fn cosignature_check_defects_map_honestly() {
    // The shared check's vocabulary — exercised directly so the durable-
    // path mappings (verify_run/Auditor/audit_view) all lean on it.
    let (mut s, run, lease) = open("wit-check", witnessed_manifest(2));
    fill(&mut s, &run, &lease, "a", 2);
    let mut w1 = wit1();
    let mut w2 = wit2();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1, &mut w2];
    let env = s
        .checkpoint_witnessed(
            &run,
            &lease,
            CheckpointKind::Periodic,
            &mut signer(),
            &mut witnesses,
        )
        .unwrap();
    let policy = witnessed_manifest(2).witness_policy.unwrap();
    let claim = tree::parse_checkpoint(&env.payload).unwrap();
    let ktab = keys();

    // Clean — verified.
    check_witness_cosignatures(&claim, Some(&policy), Some(&ktab)).unwrap();
    // No resolver — shape/declaration/quorum only.
    check_witness_cosignatures(&claim, Some(&policy), None).unwrap();

    // An undeclared pair — mutate a copy.
    let mut forged = claim.clone();
    if let Json::Obj(m) = &mut forged.payload {
        m.insert(
            "witness_cosignatures".to_string(),
            Json::Arr(vec![Json::obj([
                ("witness_name", Json::str("mallory")),
                ("key_id", Json::str("mallory-key")),
                ("timestamp", Json::Int(1)),
                ("sig", Json::str("hmac-sha256:00")),
            ])]),
        );
    }
    let forged = tree::parse_checkpoint(&forged.payload).unwrap();
    assert!(matches!(
        check_witness_cosignatures(&forged, Some(&policy), Some(&ktab)),
        Err(hh_ledger::audit::WitnessDefect::Undeclared(_))
    ));

    // Quorum unmet — drop one cosignature.
    let mut short = claim.clone();
    if let Json::Obj(m) = &mut short.payload {
        m.insert(
            "witness_cosignatures".to_string(),
            Json::Arr(vec![claim.witness_cosignatures[0].clone()]),
        );
    }
    let short = tree::parse_checkpoint(&short.payload).unwrap();
    assert!(matches!(
        check_witness_cosignatures(&short, Some(&policy), Some(&ktab)),
        Err(hh_ledger::audit::WitnessDefect::QuorumUnmet {
            required: 2,
            satisfied: 1
        })
    ));

    // A malformed entry — missing `sig`.
    let mut malformed = claim.clone();
    if let Json::Obj(m) = &mut malformed.payload {
        m.insert(
            "witness_cosignatures".to_string(),
            Json::Arr(vec![Json::obj([("witness_name", Json::str("wit-alpha"))])]),
        );
    }
    let malformed = tree::parse_checkpoint(&malformed.payload).unwrap();
    assert!(matches!(
        check_witness_cosignatures(&malformed, Some(&policy), Some(&ktab)),
        Err(hh_ledger::audit::WitnessDefect::Malformed(_))
    ));
}

// ── receiver receipts ────────────────────────────────────────────────────────

#[test]
fn receipt_lifts_durable_unverified_and_binds_the_effect() {
    let (mut s, run, lease) = open("rcpt", signed_manifest());
    let obs_id = apply_effect(&mut s, &run, &lease, "eff-1");

    let r = receipt("eff-1", Some(&obs_id), "rcpt-1");
    let env = s.record_receipt(&run, &lease, &r).unwrap();
    assert_eq!(env.class, "lifecycle.ledger.receipt");
    // Kernel-origin + the fixed lift status — the row can never carry a
    // verification claim the kernel did not make.
    assert_eq!(
        env.payload.get("status").and_then(Json::as_str),
        Some(RECEIPT_STATUS_UNVERIFIED)
    );
    assert_eq!(
        env.payload.get("supplied_by").and_then(Json::as_str),
        Some("writer-a")
    );
    assert_eq!(
        env.payload.get("observed_ref").and_then(Json::as_str),
        Some(obs_id.as_str())
    );
    assert_eq!(
        env.provenance.as_ref().map(|p| p.authority),
        Some(hh_provenance::AuthorityClass::Kernel)
    );

    // Idempotence — the same receipt_id + identical members returns the
    // existing row; a same-id/different-members record refuses.
    let again = s.record_receipt(&run, &lease, &r).unwrap();
    assert_eq!(again.event_id, env.event_id);
    let mut changed = r.clone();
    changed.receiver_ref = "receiver:other".into();
    match s.record_receipt(&run, &lease, &changed) {
        Err(LedgerError::SchemaViolation { .. }) => {}
        other => panic!("expected SchemaViolation, got {other:?}"),
    }

    // The durable prefix verifies and the audit fold binds the row.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
    let v = s.project(&run, ViewKind::AuditView, None).unwrap();
    let Json::Arr(rows) = v.payload.get("receipts").unwrap() else {
        panic!("receipts not an array")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("binding_status").and_then(Json::as_str),
        Some("bound")
    );
    assert_eq!(
        rows[0].get("status").and_then(Json::as_str),
        Some("unverified")
    );
    assert_eq!(
        v.payload
            .get("completeness")
            .and_then(|c| c.get("receipts_ok"))
            .cloned(),
        Some(Json::Bool(true))
    );
}

#[test]
fn receipt_refusals_are_typed_never_silent() {
    let (mut s, run, lease) = open("rcpt-ref", signed_manifest());
    apply_effect(&mut s, &run, &lease, "eff-1");

    // Unknown effect — the fold has no such effect_id.
    match s.record_receipt(&run, &lease, &receipt("eff-none", None, "r-x")) {
        Err(LedgerError::UnknownEffect { effect_id }) => assert_eq!(effect_id, "eff-none"),
        other => panic!("expected UnknownEffect, got {other:?}"),
    }
    // observed_ref naming the wrong class/event.
    match s.record_receipt(&run, &lease, &receipt("eff-1", Some("bogus"), "r-y")) {
        Err(LedgerError::SchemaViolation { .. }) => {}
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
    // Shape refusals — empty members and a non-triple attestation.
    for bad in [
        Receipt {
            receiver_ref: String::new(),
            ..receipt("eff-1", None, "r-1")
        },
        Receipt {
            attestation: Json::Null,
            ..receipt("eff-1", None, "r-2")
        },
        Receipt {
            receipt_id: String::new(),
            ..receipt("eff-1", None, "r-3")
        },
    ] {
        match s.record_receipt(&run, &lease, &bad) {
            Err(LedgerError::SchemaViolation { .. }) => {}
            other => panic!("expected SchemaViolation, got {other:?}"),
        }
    }
    // Nothing durable on the refusal path.
    assert!(!s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.class == "lifecycle.ledger.receipt"));
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
}

#[test]
fn receipt_codec_round_trips_and_refuses_claimed_verification() {
    let r = receipt("eff-1", Some("ev-9"), "rcpt-rt");
    let j = r.to_json();
    let back = Receipt::from_json(&j).unwrap();
    assert_eq!(back, r);
    // A row claiming `verified` is not a receipt row — the kernel never
    // made that claim.
    let mut forged = j.clone();
    if let Json::Obj(m) = &mut forged {
        m.insert("status".to_string(), Json::str("verified"));
    }
    assert!(Receipt::from_json(&forged).is_none());
}

// ── GC/redaction + receipts + witnesses compose ──────────────────────────────

#[test]
fn gc_and_redact_stay_durable_before_delete_alongside_new_rows() {
    let (mut s, run, lease) = open("gc-red", witnessed_manifest(1));
    fill(&mut s, &run, &lease, "a", 2);
    let obs = apply_effect(&mut s, &run, &lease, "eff-1");
    let claim_blob = s.put_blob(b"receiver claim doc", "text/plain").unwrap();
    let mut r = receipt("eff-1", Some(&obs), "rcpt-gc");
    r.claim_ref = Some(claim_blob.id());
    s.record_receipt(&run, &lease, &r).unwrap();
    let mut w1 = wit1();
    let mut witnesses: Vec<&mut dyn WitnessSigner> = vec![&mut w1];
    s.checkpoint_witnessed(
        &run,
        &lease,
        CheckpointKind::Periodic,
        &mut signer(),
        &mut witnesses,
    )
    .unwrap();

    // GC the receipt's claim blob — the durable `lifecycle.ledger.gc` row
    // lands before the bytes move, and the tombstone accounts it.
    let gc_env = s
        .gc(
            &run,
            &lease,
            vec![claim_blob.id()],
            "policy:test",
            "warm",
            None,
        )
        .unwrap();
    assert_eq!(gc_env.class, "lifecycle.ledger.gc");
    match s.get_blob(&claim_blob) {
        Err(LedgerError::Missing { reason, .. }) => assert_eq!(reason, MissingReason::Gc),
        other => panic!("expected Missing{{gc}}, got {other:?}"),
    }

    // Redact a second blob — the durable `lifecycle.ledger.redacted` row.
    let secret = s.put_blob(b"secret", "text/plain").unwrap();
    let endorser = ProvenanceRecord::minted(
        hh_provenance::Origin::human("human:alice", hh_provenance::HumanRole::Principal),
        hh_provenance::PersistenceScope::Run,
        0,
    );
    let red_env = s
        .redact(
            &run,
            &lease,
            vec![hh_ledger::store::RedactTarget::Address(secret.id())],
            "subject_request",
            &endorser,
            "approval",
        )
        .unwrap();
    assert_eq!(red_env.class, "lifecycle.ledger.redacted");

    // The run still verifies and audits end-to-end; audit_view accounts
    // the gc'd claim ref under `gc`, not `missing`.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    s.set_audit_key_resolver(Box::new(keys()));
    let v = s.project(&run, ViewKind::AuditView, None).unwrap();
    let refs = v.payload.get("content_refs").unwrap();
    match refs.get("gc") {
        Some(Json::Arr(a)) => assert_eq!(a.len(), 1, "content_refs: {refs:?}"),
        other => panic!("content_refs.gc not an array: {other:?}"),
    }
    let comp = v.payload.get("completeness").unwrap();
    assert_eq!(
        comp.get("receipts_ok").cloned(),
        Some(Json::Bool(true)),
        "{comp:?}"
    );
}
