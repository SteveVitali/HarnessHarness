//! S4.15 acceptance — AC-R-2.12.1-6: `rotate(idp/1 → idp/2)` end-to-end
//! over a corpus of *chained* runs (parent → child → grandchild plus a
//! `continued_from` activation): every `idp/1` id still verifies, every
//! live run carries a rotation checkpoint whose `rehash` verifies under
//! `idp/2` and is attested, `migrate_id` round-trips for every record,
//! and no stored id is rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_identity::rotation::{migrate_id, MigrationMethod, RotationPlan};
use hh_ledger::audit::{Auditor, FixedSigner, KeyTable};
use hh_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{LineageLink, RunKind, RunManifest};
use hh_ledger::rotation::{self, rotate_corpus};
use hh_ledger::store::{Lease, Store};
use hh_ledger::tree;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-s415-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn signed_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.signer_key_ids = vec!["test-key".to_string()];
    m
}

fn signer() -> FixedSigner {
    FixedSigner::new("test-key", b"s4.15-test-signing-key".to_vec())
}

fn keys() -> KeyTable {
    KeyTable(BTreeMap::from([(
        "test-key".to_string(),
        b"s4.15-test-signing-key".to_vec(),
    )]))
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

fn claim_of(env: &EventEnvelope) -> tree::CheckpointClaim {
    tree::parse_checkpoint(&env.payload).unwrap()
}

#[test]
fn ac_r_2_12_1_6_rotate_corpus_over_chained_runs() {
    let mut s = Store::open_test(dir("corpus"), 1_000).unwrap();
    // The chained corpus: root → child → grandchild (subagent edges) plus
    // a continuation activation off the root's head.
    let (root, root_lease) = s.open_run(signed_manifest(), "writer-a").unwrap();
    fill(&mut s, &root, &root_lease, "r", 3);
    let mut cm = signed_manifest();
    cm.parent_run_id = Some(root.clone());
    let (child, child_lease) = s.open_run(cm, "writer-a").unwrap();
    fill(&mut s, &child, &child_lease, "c", 2);
    let mut gm = signed_manifest();
    gm.parent_run_id = Some(child.clone());
    let (grand, grand_lease) = s.open_run(gm, "writer-a").unwrap();
    fill(&mut s, &grand, &grand_lease, "g", 1);
    let root_head = s.head(&root).unwrap();
    let mut km = signed_manifest();
    km.continued_from = Some(LineageLink {
        run_id: root.clone(),
        at_seq: root_head.seq,
        head_hash: root_head.hash.clone(),
    });
    let (cont, cont_lease) = s.open_run(km, "writer-a").unwrap();
    fill(&mut s, &cont, &cont_lease, "k", 2);

    let runs: Vec<(&str, &Lease)> = vec![
        (root.as_str(), &root_lease),
        (child.as_str(), &child_lease),
        (grand.as_str(), &grand_lease),
        (cont.as_str(), &cont_lease),
    ];
    let run_ids: BTreeSet<String> = runs.iter().map(|(r, _)| r.to_string()).collect();

    // Snapshot every stored id — rotation must rewrite nothing.
    let before: BTreeMap<String, Vec<(String, String)>> = runs
        .iter()
        .map(|(r, _)| {
            (
                r.to_string(),
                s.events(r)
                    .unwrap()
                    .iter()
                    .map(|e| (e.event_id.clone(), e.hash.clone()))
                    .collect(),
            )
        })
        .collect();

    let plan = RotationPlan {
        from_idp: "idp/1".to_string(),
        to_idp: "idp/2".to_string(),
        declared_at: root.clone(),
        reason: "corpus rotation".to_string(),
        bridge: true,
        attestation_ref: None,
        effective_from_seq: None,
        rehash: true,
    };
    let receipts = rotate_corpus(&mut s, &runs, &mut signer(), &plan).unwrap();
    assert_eq!(receipts.len(), 4);

    let p1 = hh_identity::idp::profile_for("idp/1").unwrap();
    let corpus_ids: Vec<String> = run_ids.iter().cloned().collect();

    for (run_id, receipt) in run_ids.iter().zip(&receipts) {
        // ── every live run has an attested rotation checkpoint ────────
        let claim = claim_of(&receipt.checkpoint);
        assert_eq!(claim.kind, "rotation");
        assert_eq!(claim.identity_profile.as_deref(), Some("idp/2"));
        assert!(
            !claim.signatures.is_empty(),
            "the rotation checkpoint is attested (signed) — {run_id}"
        );
        // …whose `rehash` verifies under idp/2 over the covered prefix.
        let covered: Vec<&EventEnvelope> = s
            .events(run_id)
            .unwrap()
            .iter()
            .take(receipt.checkpoint.seq as usize)
            .collect();
        assert_eq!(
            rotation::verify_rehash(claim.rehash.as_ref().unwrap(), covered.iter().copied()),
            Some(true),
            "rehash under idp/2 fails for {run_id}"
        );
        // ── every idp/1 id still verifies (nothing rewritten) ─────────
        let stored: &[EventEnvelope] = s.events(run_id).unwrap();
        let pre_count = before[run_id].len();
        let leaves1 = rotation::rehashed_leaves(p1, stored[..pre_count].iter());
        assert_eq!(
            leaves1,
            before[run_id]
                .iter()
                .map(|(_, h)| h.clone())
                .collect::<Vec<_>>(),
            "idp/1 hashes no longer verify for {run_id}"
        );
        // …and the stored ids are byte-identical for the covered prefix.
        for (i, (eid, h)) in before[run_id].iter().enumerate() {
            assert_eq!(stored[i].event_id, *eid, "event_id rewritten in {run_id}");
            assert_eq!(stored[i].hash, *h, "stored hash rewritten in {run_id}");
        }
        // ── the bridge binds the corpus and round-trips every record ──
        let bridge = receipt.bridge.as_ref().expect("bridge record");
        assert_eq!(
            bridge.links_run_ids, corpus_ids,
            "the bridge names every corpus member"
        );
        let leaves2 =
            rotation::rehashed_leaves(hh_identity::idp::profile_for("idp/2").unwrap(), covered);
        for (ev, new_hash) in before[run_id].iter().zip(&leaves2) {
            let m = bridge
                .resolve(&ev.1)
                .unwrap_or_else(|_| panic!("bridge has no row for {} ({})", ev.0, ev.1));
            assert_eq!(&m.new_id, new_hash, "bridge maps to the idp/2 leaf");
            // …and back: the bridge's resolve is a true round-trip.
            let back = bridge.resolve(new_hash).unwrap();
            assert_eq!(back.old_id, ev.1, "bridge round-trip broke for {}", ev.0);
        }
        // The whole run still audits — both sides of the rotation, with
        // signatures verified against the key table.
        s.verify_run(run_id, None, None, Some(&keys())).unwrap();
        let mut aud = Auditor::new(run_id);
        s.audit_with(run_id, &mut aud, Some(&keys())).unwrap();
    }

    // ── migrate_id round-trips for every corpus record ────────────────
    // The bridge records are themselves content ids — forward under idp/2
    // then back under idp/1 recovers the stored id.
    let p2 = hh_identity::idp::profile_for("idp/2").unwrap();
    for receipt in &receipts {
        let b = receipt.bridge.as_ref().unwrap();
        // The `bridge_id` preimage is the unsigned body — `to_json` minus
        // the `bridge_id` member (the seal's own construction).
        let mut unsigned = b.to_json();
        if let Json::Obj(ref mut m) = unsigned {
            m.remove("bridge_id");
        }
        let canonical = unsigned.to_canonical_string().into_bytes();
        let fwd = migrate_id(
            &b.bridge_id,
            p1,
            p2,
            p2,
            "hh.bridge-record",
            Some(&canonical),
            &b.bridge_id,
            MigrationMethod::RotateFullRehash,
        )
        .expect("forward migrate");
        assert_ne!(fwd.new_id, b.bridge_id, "idp/2 rehash must differ");
        let back = migrate_id(
            &fwd.new_id,
            p2,
            p1,
            p1,
            "hh.bridge-record",
            Some(&canonical),
            &b.bridge_id,
            MigrationMethod::RotateFullRehash,
        )
        .expect("reverse migrate");
        assert_eq!(back.new_id, b.bridge_id, "migrate_id must round-trip");
    }
}
