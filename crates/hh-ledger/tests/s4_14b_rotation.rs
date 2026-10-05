//! S4.14b acceptance coverage — AC-R-2.8.6-8: end-to-end identity rotation.
//! `idp/1 → idp/2`: pre-rotation checkpoints verify under `idp/1`, the
//! rotation claim carries a verified `rehash` + `bridge_record_ref`, and
//! post-rotation checkpoints chain to the rotation claim under `idp/2`.
//! AC-R-2.8.6-10: a hosted run at `observability_level = {events}` renders
//! ledger-needing components `n/a{observability}` while `chain_ok` stays a
//! real recompute.
//!
//! Each test fails if the behaviour it covers is removed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_identity::rotation::RotationPlan;
use hh_ledger::audit::{Auditor, CheckpointKind, FixedSigner, KeyTable};
use hh_ledger::errors::LedgerError;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass, RunKind, RunManifest};
use hh_ledger::rotation::{self, rotate};
use hh_ledger::store::{Lease, Store};
use hh_ledger::tree;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-s414b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
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

fn signer() -> FixedSigner {
    FixedSigner::new("test-key", b"s4.14b-test-signing-key".to_vec())
}

fn keys() -> KeyTable {
    KeyTable(BTreeMap::from([(
        "test-key".to_string(),
        b"s4.14b-test-signing-key".to_vec(),
    )]))
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

fn claim_of(env: &hh_ledger::event::EventEnvelope) -> tree::CheckpointClaim {
    tree::parse_checkpoint(&env.payload).unwrap()
}

/// The declared `idp/1 → idp/2` rotation plan.
fn plan(run: &str) -> RotationPlan {
    RotationPlan {
        from_idp: "idp/1".to_string(),
        to_idp: "idp/2".to_string(),
        declared_at: run.to_string(),
        reason: "rotate test".to_string(),
        bridge: true,
        attestation_ref: None,
        effective_from_seq: None,
        rehash: true,
    }
}

// ── AC-R-2.8.6-8: end-to-end rotation ────────────────────────────────────────

#[test]
fn rotation_checkpoint_carries_verified_rehash_and_post_rotation_claims_chain() {
    let (mut s, run, lease) = open("rot", signed_manifest());
    fill(&mut s, &run, &lease, "pre", 4);

    // Pre-rotation checkpoint — verifies under idp/1.
    let pre = s
        .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    let pre_claim = claim_of(&pre);
    assert_eq!(
        pre_claim.identity_profile.as_deref().unwrap_or("idp/1"),
        "idp/1"
    );
    let p1 = hh_identity::idp::profile_for("idp/1").unwrap();
    let pre_leaves: Vec<String> = s.events(&run).unwrap()[..pre.seq as usize]
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    assert_eq!(
        pre_claim.tree_head.as_deref(),
        Some(tree::mth_in(p1, &pre_leaves).as_str()),
        "pre-rotation head must verify under idp/1"
    );

    // Rotate — bridge row then rotation claim.
    let receipt = rotate(&mut s, &run, &lease, &mut signer(), &plan(&run)).unwrap();
    let rot_claim = claim_of(&receipt.checkpoint);
    assert_eq!(rot_claim.kind, "rotation");
    assert_eq!(rot_claim.identity_profile.as_deref(), Some("idp/2"));
    // `rehash` re-verifies over the covered prefix.
    let covered: Vec<_> = s.events(&run).unwrap()[..receipt.checkpoint.seq as usize]
        .iter()
        .collect();
    assert_eq!(
        rotation::verify_rehash(rot_claim.rehash.as_ref().unwrap(), covered),
        Some(true),
        "the rotation rehash must recompute under idp/2"
    );
    // `bridge_record_ref` names the emitted `security.audit.bridge` row.
    let bref = rot_claim.bridge_record_ref.as_deref().unwrap();
    assert_eq!(receipt.bridge_event_id.as_deref(), Some(bref));
    assert!(s
        .events(&run)
        .unwrap()
        .iter()
        .any(|e| e.event_id == bref && e.class == "security.audit.bridge"));

    // Post-rotation events + checkpoint — the claim is under idp/2 and its
    // `prev_checkpoint` names the rotation claim.
    fill(&mut s, &run, &lease, "post", 3);
    let post = s
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut signer())
        .unwrap();
    let post_claim = claim_of(&post);
    assert_eq!(post_claim.identity_profile.as_deref(), Some("idp/2"));
    let Some(Json::Obj(link)) = &post_claim.prev_checkpoint else {
        panic!("post-rotation claim lacks prev_checkpoint")
    };
    assert_eq!(
        link.get("event_id").and_then(Json::as_str),
        Some(receipt.checkpoint.event_id.as_str()),
        "the post-rotation claim chains to the rotation checkpoint"
    );
    assert_eq!(
        link.get("tree_head").and_then(Json::as_str),
        rot_claim.tree_head.as_deref()
    );
    // The post-rotation head verifies under idp/2 over rehashed leaves.
    let p2 = hh_identity::idp::profile_for("idp/2").unwrap();
    let covered_post: Vec<&_> = s.events(&run).unwrap()[..post.seq as usize]
        .iter()
        .collect();
    let post_leaves = rotation::rehashed_leaves(p2, covered_post);
    assert_eq!(
        post_claim.tree_head.as_deref(),
        Some(tree::mth_in(p2, &post_leaves).as_str())
    );

    // The whole run verifies — both sides of the rotation.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();

    // The independent auditor agrees.
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    assert_eq!(aud.claims().len(), 3);
}

#[test]
fn rotation_plan_naming_the_wrong_from_profile_refuses() {
    let (mut s, run, lease) = open("rot-bad", signed_manifest());
    fill(&mut s, &run, &lease, "a", 2);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    // `from_idp` must name the run's *current* claim profile.
    let mut bad = plan(&run);
    bad.from_idp = "idp/2".to_string();
    bad.to_idp = "idp/1".to_string();
    match rotate(&mut s, &run, &lease, &mut signer(), &bad) {
        Err(LedgerError::SchemaViolation { detail }) => {
            assert!(detail.contains("idp_not_writable"), "{detail}");
        }
        other => panic!("expected idp_not_writable refusal, got {other:?}"),
    }
}

#[test]
fn rotation_kind_through_plain_checkpoint_refuses() {
    let (mut s, run, lease) = open("rot-kind", signed_manifest());
    fill(&mut s, &run, &lease, "a", 1);
    // `kind = rotation` without the rotation op's members is a forged
    // boundary — refuse at mint.
    match s.checkpoint(&run, &lease, CheckpointKind::Rotation, &mut signer()) {
        Err(LedgerError::SchemaViolation { detail }) => {
            assert!(detail.contains("rotation_checkpoint"), "{detail}");
        }
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
}

// ── AC-R-2.8.6-10: hosted observability ─────────────────────────────────────

#[test]
fn hosted_events_only_view_renders_ledger_components_na_observability() {
    let mut m = signed_manifest();
    m.participant_class = ParticipantClass::Hosted;
    m.observability_level = BTreeSet::from([ObservabilityLevel::Events]);
    let (mut s, run, lease) = open("hosted-view", m);
    fill(&mut s, &run, &lease, "h", 3);

    let manifest = s.manifest(&run).unwrap().clone();
    let events = s.events(&run).unwrap();
    let view = hh_ledger::audit::audit_view(
        &run,
        events,
        &BTreeMap::new(),
        false,
        |_| hh_ledger::audit::BlobStatus::Present,
        None,
        &manifest,
        |_, _| None,
        Some(&keys()),
    );
    let Json::Obj(p) = &view.payload else {
        panic!("audit_view payload not an object")
    };
    // `chain_ok` is a real recompute — never n/a, never fabricated.
    assert_eq!(p.get("chain_ok"), Some(&Json::Bool(true)));
    // Ledger-needing components render n/a{observability}.
    for member in ["checkpoints", "cross_run"] {
        let v = p.get(member).unwrap_or_else(|| panic!("{member} missing"));
        assert_eq!(
            v.get("n/a").and_then(Json::as_str),
            Some("observability"),
            "{member} must render n/a{{observability}} at events-only"
        );
    }
    let completeness = p.get("completeness").unwrap();
    for member in ["checkpoints_ok", "cross_run_ok"] {
        let v = completeness.get(member).unwrap();
        assert_eq!(
            v.get("n/a").and_then(Json::as_str),
            Some("observability"),
            "completeness.{member} must render n/a{{observability}}"
        );
    }
    // The headline is not `false` merely because ledger visibility is absent.
    assert_eq!(
        completeness.get("headline"),
        Some(&Json::Bool(true)),
        "headline must not be false for ledger-blindness alone"
    );
}

// ── R-2.8.6¹: the lab-wide audit index (ADR-0067 D5) ───────────────────────

use hh_ledger::audit::{AuditFault, AuditIndex, AuditIndexEntry};
use hh_ledger::event::EventFrame;

#[test]
fn audit_index_records_auditor_heads_and_surfaces_equivocation() {
    let (mut s, run, lease) = open("idx", signed_manifest());
    fill(&mut s, &run, &lease, "pre", 4);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    fill(&mut s, &run, &lease, "post", 2);
    s.checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut signer())
        .unwrap();

    // The auditor observes every durable frame, then its verified claims
    // land in the lab-wide index — `(run_id, checkpoint_ref, tree_size,
    // tree_head, signatures)` per signed checkpoint (ADR-0067 D5).
    let mut auditor = Auditor::new(&run);
    for env in s.envelopes(&run).unwrap() {
        auditor
            .observe(
                &EventFrame::Durable {
                    seq: env.seq,
                    event: Box::new(env.clone()),
                    hash: env.hash.clone(),
                },
                Some(&keys()),
            )
            .unwrap();
    }
    let mut index = AuditIndex::new();
    for env in s.envelopes(&run).unwrap() {
        if env.class != "security.audit.checkpoint" {
            continue;
        }
        let claim = tree::parse_checkpoint(&env.payload).unwrap();
        let entry = AuditIndexEntry {
            run_id: run.clone(),
            checkpoint_ref: Json::obj([
                ("run_id", Json::str(run.clone())),
                ("event_id", Json::str(env.event_id.clone())),
                ("seq", Json::Int(env.seq as i64)),
            ]),
            tree_size: claim.tree_size.unwrap(),
            tree_head: claim.tree_head.unwrap(),
            signatures: claim.signatures.clone(),
        };
        assert!(index.record(entry).unwrap());
    }
    assert_eq!(index.rows_for(&run).len(), 2);
    let latest = index.latest(&run).unwrap();
    // A claim covers `leaves[0..seq)` — its `tree_size` is the checkpoint
    // row's own seq; the auditor additionally folds the checkpoint row
    // itself into its head.
    let cp_seq = s
        .envelopes(&run)
        .unwrap()
        .iter()
        .rev()
        .find(|e| e.class == "security.audit.checkpoint")
        .unwrap()
        .seq;
    assert_eq!(latest.tree_size, cp_seq);
    assert_eq!(auditor.head().0, cp_seq + 1);

    // Idempotent re-record — a byte-equal row is not a fault.
    let dup = index.rows_for(&run)[0].clone();
    assert_eq!(index.record(dup), Ok(false));
    assert_eq!(index.rows().len(), 2);

    // The one semantics: a different head for a held (run, tree_size) is
    // `Equivocation` — two auditors can never silently disagree.
    let bad = AuditIndexEntry {
        tree_head: "sha256:forged".into(),
        ..index.rows_for(&run)[0].clone()
    };
    match index.record(bad) {
        Err(AuditFault::Equivocation { .. }) => {}
        other => panic!("expected Equivocation, got {other:?}"),
    }
}
