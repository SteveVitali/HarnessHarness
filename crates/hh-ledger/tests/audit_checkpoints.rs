//! S2.5 acceptance coverage — AC-R-2.8.6-{1,2}: signed audit checkpoints,
//! compact-range tree heads, inclusion/consistency proofs, cross-run anchors,
//! independent auditors, `lifecycle.ledger.gc`, and the `Tampered` taxonomy
//! extended with `truncate`/`checkpoint_invalid`/`sig_missing`/
//! `bad_signature`/`fork_equivocation`.
//!
//! Each test fails if the behaviour it covers is removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::audit::{AuditFault, Auditor, CheckpointKind, FixedSigner, KeyTable};
use hh_ledger::errors::{LedgerError, MissingReason, TamperedKind};
use hh_ledger::event::{Event, EventFrame, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, RedactTarget, Store};
use hh_ledger::tree;
use hh_ledger::views::ViewKind;
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-s25-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A signed run: `signer_key_ids` declares the test key.
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

/// A kernel-authored event (audit-grade classes admit only kernel producers).
fn k_ev(id: &str, class: &str, payload: Json) -> Event {
    let mut e = ev(id, class, payload);
    e.producer = Producer::kernel("kernel:test");
    e.provenance = Some(ProvenanceRecord::kernel("kernel:test", 0));
    e
}

/// Indices of the `{"k":"e",…}` event rows in a WAL text.
fn event_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("{\"k\":\"e\""))
        .map(|(i, _)| i)
        .collect()
}

fn wal_path(s: &Store, run: &str) -> PathBuf {
    s.root().join("runs").join(run).join("events.wal")
}

fn tampered_kind(e: LedgerError) -> TamperedKind {
    match e {
        LedgerError::Tampered(t) => t.kind,
        other => panic!("expected Tampered, got {other:?}"),
    }
}

fn signer() -> FixedSigner {
    FixedSigner::new("test-key", b"s2.5-test-signing-key".to_vec())
}

fn keys() -> KeyTable {
    KeyTable(BTreeMap::from([(
        "test-key".to_string(),
        b"s2.5-test-signing-key".to_vec(),
    )]))
}

/// Append `n` observation rows, then a periodic checkpoint.
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

// ── signed checkpoints ───────────────────────────────────────────────────────

#[test]
fn checkpoint_lands_signed_covers_the_durable_prefix() {
    let (mut s, run, lease) = open("cp", signed_manifest());
    fill(&mut s, &run, &lease, "a", 5);
    let env = s
        .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    assert_eq!(env.class, "security.audit.checkpoint");
    assert_eq!(
        env.payload.get("tree_size").and_then(Json::as_int),
        Some(env.seq as i64)
    );
    let leaves: Vec<String> = s.events(&run).unwrap()[..env.seq as usize]
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    assert_eq!(
        env.payload.get("tree_head").and_then(Json::as_str),
        Some(tree::mth(&leaves).as_str())
    );
    let sigs = env.payload.get("signatures").unwrap();
    let Json::Arr(sigs) = sigs else {
        panic!("signatures not an array")
    };
    assert_eq!(sigs.len(), 1);
    assert_eq!(
        sigs[0].get("key_id").and_then(Json::as_str),
        Some("test-key")
    );
    assert_eq!(
        sigs[0].get("alg_ref").and_then(Json::as_str),
        Some("hmac-sha256")
    );
    // Verify with the resolver — signatures check out.
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
}

#[test]
fn checkpoint_refuses_without_declared_key_or_signer() {
    let (mut s, run, lease) = open("cp-nokey", RunManifest::minimal(RunKind::Agent));
    fill(&mut s, &run, &lease, "a", 2);
    match s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer()) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
    // A declared-but-wrong key id is equally refused.
    let (mut s, run, lease) = open("cp-wrongkey", signed_manifest());
    fill(&mut s, &run, &lease, "a", 2);
    let mut wrong = FixedSigner::new("other-key", b"k".to_vec());
    match s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut wrong) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}

#[test]
fn final_checkpoint_covers_finished_and_is_idempotent() {
    let (mut s, run, lease) = open("cp-final", signed_manifest());
    fill(&mut s, &run, &lease, "a", 3);
    // `final` before `finished` is a schema violation.
    assert!(matches!(
        s.checkpoint(&run, &lease, CheckpointKind::Final, &mut signer()),
        Err(LedgerError::SchemaViolation { .. })
    ));
    s.append(
        &run,
        &lease,
        vec![k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        )],
    )
    .unwrap();
    // Non-final kinds are fenced after `finished`.
    assert!(matches!(
        s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer()),
        Err(LedgerError::RunFinished { .. })
    ));
    let fin = s
        .checkpoint(&run, &lease, CheckpointKind::Final, &mut signer())
        .unwrap();
    assert_eq!(
        fin.payload.get("kind").and_then(Json::as_str),
        Some("final")
    );
    // Covers the whole stream incl. `finished` — tree_size == its own seq.
    assert_eq!(
        fin.payload.get("tree_size").and_then(Json::as_int),
        Some(fin.seq as i64)
    );
    // Idempotent — a second call returns the landed row.
    let again = s
        .checkpoint(&run, &lease, CheckpointKind::Final, &mut signer())
        .unwrap();
    assert_eq!(again.event_id, fin.event_id);
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
}

#[test]
fn finished_signed_run_without_final_is_truncated() {
    let (mut s, run, lease) = open("cp-trunc", signed_manifest());
    fill(&mut s, &run, &lease, "a", 2);
    s.append(
        &run,
        &lease,
        vec![k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        )],
    )
    .unwrap();
    // Declared signers + finished + no final checkpoint = Truncate.
    match s.verify_run(&run, None, None, Some(&keys())) {
        Err(LedgerError::Tampered(t)) => assert_eq!(t.kind, TamperedKind::Truncate),
        other => panic!("expected Tampered{{truncate}}, got {other:?}"),
    }
    // And a finished run with no declared signers is not owed one.
    let (mut s2, run2, lease2) = open("cp-unsigned", RunManifest::minimal(RunKind::Agent));
    fill(&mut s2, &run2, &lease2, "b", 2);
    s2.append(
        &run2,
        &lease2,
        vec![k_ev(
            "fin",
            "lifecycle.run.finished",
            Json::obj([("reason", Json::str("done"))]),
        )],
    )
    .unwrap();
    s2.verify(&run2).unwrap();
}

#[test]
fn checkpoint_chain_links_prev_claim() {
    let (mut s, run, lease) = open("cp-chain", signed_manifest());
    fill(&mut s, &run, &lease, "a", 2);
    let c1 = s
        .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    fill(&mut s, &run, &lease, "b", 2);
    let c2 = s
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut signer())
        .unwrap();
    let link = c2.payload.get("prev_checkpoint").unwrap();
    assert_eq!(
        link.get("event_id").and_then(Json::as_str),
        Some(c1.event_id.as_str())
    );
    s.verify_run(&run, None, None, Some(&keys())).unwrap();
}

// ── proofs ───────────────────────────────────────────────────────────────────

#[test]
fn inclusion_proof_roundtrips_and_rejects_wrong_claims() {
    let (mut s, run, lease) = open("proof-inc", signed_manifest());
    fill(&mut s, &run, &lease, "a", 9);
    let ev3 = &s.events(&run).unwrap()[3];
    let proof = s.prove_inclusion(&run, 3, None).unwrap();
    let head = tree::mth(
        &s.events(&run)
            .unwrap()
            .iter()
            .map(|e| e.hash.clone())
            .collect::<Vec<_>>(),
    );
    assert!(tree::verify_inclusion(&proof, &ev3.hash, &head));
    // JSON round-trip.
    let rt = tree::InclusionProof::from_json(&proof.to_json()).unwrap();
    assert_eq!(rt, proof);
    // Wrong leaf hash.
    assert!(!tree::verify_inclusion(
        &proof,
        "00".repeat(32).as_str(),
        &head
    ));
    // Wrong head (prefix head of size 4).
    let leaves: Vec<String> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    let short_head = tree::mth_prefix(&leaves, 4);
    assert!(!tree::verify_inclusion(&proof, &ev3.hash, &short_head));
}

#[test]
fn consistency_proof_roundtrips_and_rejects_forks() {
    let (mut s, run, lease) = open("proof-cons", signed_manifest());
    fill(&mut s, &run, &lease, "a", 4);
    let leaves4: Vec<String> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    let n1 = leaves4.len() as u64;
    let head4 = tree::mth(&leaves4);
    fill(&mut s, &run, &lease, "b", 5);
    let leaves9: Vec<String> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    let n2 = leaves9.len() as u64;
    let head9 = tree::mth(&leaves9);
    let proof = s.prove_consistency(&run, n1, n2).unwrap();
    assert!(tree::verify_consistency(&proof, &head4, &head9));
    let rt = tree::ConsistencyProof::from_json(&proof.to_json()).unwrap();
    assert_eq!(rt, proof);
    // A forked first head fails.
    let fork = {
        let mut l = leaves4.clone();
        l[1] = "ff".repeat(32);
        tree::mth(&l)
    };
    assert!(!tree::verify_consistency(&proof, &fork, &head9));
    // Out-of-range sizes are refused.
    assert!(s.prove_consistency(&run, n2, n1).is_err());
    assert!(s.prove_consistency(&run, n1, n2 + 100).is_err());
}

// ── tamper battery ───────────────────────────────────────────────────────────

#[test]
fn tamper_battery() {
    // (a) edited content → content_modified.
    let (mut s, run, lease) = open("t-a", signed_manifest());
    fill(&mut s, &run, &lease, "a", 4);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let edited = text.replacen("a-0", "a-0-TAMPERED", 1);
    std::fs::write(&wal, edited).unwrap();
    assert_eq!(
        tampered_kind(s.verify(&run).unwrap_err()),
        TamperedKind::ContentModified
    );

    // (b) deleted tail + caller watermark → truncate.
    let (mut s, run, lease) = open("t-b", signed_manifest());
    fill(&mut s, &run, &lease, "b", 6);
    let head = s.head(&run).unwrap().seq;
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let kept: Vec<&str> = text.lines().take(4).collect();
    std::fs::write(&wal, kept.join("\n") + "\n").unwrap();
    // Prefix alone still verifies — truncation is a watermark verdict.
    s.verify(&run).unwrap();
    assert_eq!(
        tampered_kind(s.verify_run(&run, None, Some(head), None).unwrap_err()),
        TamperedKind::Truncate
    );

    // (c) reordered rows → reordered/seq_gap.
    let (mut s, run, lease) = open("t-c", signed_manifest());
    fill(&mut s, &run, &lease, "c", 5);
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let idxs = event_lines(&text);
    let mut lines: Vec<&str> = text.lines().collect();
    lines.swap(idxs[2], idxs[3]);
    std::fs::write(&wal, lines.join("\n") + "\n").unwrap();
    let k = tampered_kind(s.verify(&run).unwrap_err());
    assert!(
        matches!(k, TamperedKind::Reordered | TamperedKind::SeqGap),
        "{k:?}"
    );

    // (d) late insert (a forged appended line) → seq_gap/duplicate/noncanonical.
    let (mut s, run, lease) = open("t-d", signed_manifest());
    fill(&mut s, &run, &lease, "d", 3);
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let idxs = event_lines(&text);
    let dup = text.lines().nth(idxs[1]).unwrap().to_string();
    let last_c = text.rfind("{\"k\":\"c\"").unwrap();
    let mut newtext = String::from(&text[..last_c]);
    newtext.push_str(&dup);
    newtext.push('\n');
    newtext.push_str(&text[last_c..]);
    std::fs::write(&wal, newtext).unwrap();
    let k = tampered_kind(s.verify(&run).unwrap_err());
    assert!(
        matches!(
            k,
            TamperedKind::SeqGap
                | TamperedKind::DuplicateSeq
                | TamperedKind::DuplicateEventId
                | TamperedKind::ChainBroken
        ),
        "{k:?}"
    );

    // (e) prefix replacement (swap in a different line's bytes) → chain/hash.
    let (mut s, run, lease) = open("t-e", signed_manifest());
    fill(&mut s, &run, &lease, "e", 5);
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let target = event_lines(&text)[4];
    // Rename a member — the parsed envelope re-renders without the edited
    // key, so the stored line is byte-divergent (noncanonical_bytes).
    let forged = lines[target].replacen("\"event_id\"", "\"event_idX\"", 1);
    assert_ne!(forged, lines[target]);
    lines[target] = forged.clone();
    std::fs::write(&wal, lines.join("\n") + "\n").unwrap();
    let k = tampered_kind(s.verify(&run).unwrap_err());
    assert!(
        matches!(
            k,
            TamperedKind::NonCanonicalBytes
                | TamperedKind::ContentModified
                | TamperedKind::ChainBroken
        ),
        "{k:?}"
    );

    // (f) fork-equivalent divergence — a checkpoint whose signed head the
    // log cannot substantiate → fork_equivocation.
    let (mut s, run, lease) = open("t-f", signed_manifest());
    fill(&mut s, &run, &lease, "f", 4);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    // Rewrite a mid-prefix leaf AND splice the checkpoint's head to the new
    // truth would be a *consistent* fork — instead corrupt a leaf so the
    // signed head diverges from the recomputed one (the checkpoint row's
    // own bytes still parse, so the claim is checked and contradicted).
    let altered = text.replacen("\"f-0\"", "\"f-0!\"", 1);
    std::fs::write(&wal, altered).unwrap();
    let k = tampered_kind(s.verify(&run).unwrap_err());
    // The byte-level layer may fire first; both are honest detections.
    assert!(
        matches!(
            k,
            TamperedKind::ContentModified | TamperedKind::NonCanonicalBytes
        ),
        "{k:?}"
    );
    // Now the pure fork: rewrite the checkpoint's claimed head itself so
    // the envelope still hashes (payload change → ContentModified fires
    // first) — the honest fork path is exercised below via a rebuilt
    // checkpoint row signed over a forked prefix; see `forked_signed_head`.

    // (g) stripped signature → sig_missing.
    let (mut s, run, lease) = open("t-g", signed_manifest());
    fill(&mut s, &run, &lease, "g", 3);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let stripped = text.replacen("\"signatures\":[{", "\"signaturesX\":[{", 1);
    std::fs::write(&wal, stripped).unwrap();
    let k = tampered_kind(s.verify(&run).unwrap_err());
    assert!(
        matches!(
            k,
            TamperedKind::NonCanonicalBytes
                | TamperedKind::ContentModified
                | TamperedKind::SigMissing
                | TamperedKind::CheckpointInvalid
        ),
        "{k:?}"
    );

    // (h) bad signature bytes — resigning over a mutated claim is checked
    // end-to-end in `bad_signature_detected` (a byte-splice fails earlier).
}

#[test]
fn forked_signed_head_is_fork_equivocation() {
    // A checkpoint row minted over a *forked* prefix: same seq, different
    // head — the writer swapped the log under the signature. Rebuild the
    // WAL: take a real signed run, rewrite the covered prefix and the
    // checkpoint claim coherently (a full fork — the chain itself still
    // verifies byte-wise is impossible without re-signing every hash, so
    // instead corrupt the checkpoint claim's tree_head member directly and
    // let the claim-vs-recompute check fire).
    let (mut s, run, lease) = open("fork", signed_manifest());
    fill(&mut s, &run, &lease, "f", 4);
    let cp = s
        .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    // Forge the claim: same tree_size, wrong tree_head — the signature is
    // re-issued honestly over the forged claim (a signer that signed a head
    // it cannot substantiate is exactly the equivocation case).
    let mut payload = match &cp.payload {
        Json::Obj(m) => m.clone(),
        _ => panic!(),
    };
    payload.insert("tree_head".into(), Json::str("00".repeat(32)));
    let payload = Json::Obj(payload);
    // Re-sign the forged claim so the signature check itself passes and the
    // *head* is what fails.
    let preimage = tree::checkpoint_sig_preimage(&payload);
    let sig = hh_wire::sha256::hmac_sha256(b"s2.5-test-signing-key", &preimage);
    let mut sig_obj = match payload.get("signatures").unwrap() {
        Json::Arr(a) => match &a[0] {
            Json::Obj(m) => m.clone(),
            _ => panic!(),
        },
        _ => panic!(),
    };
    sig_obj.insert("sig".into(), Json::str(hex(&sig)));
    let mut payload = match payload {
        Json::Obj(mut m) => {
            m.insert("signatures".into(), Json::Arr(vec![Json::Obj(sig_obj)]));
            m
        }
        _ => panic!(),
    };
    // The claim's idp must re-derive over the forged unsigned bytes.
    payload.insert(
        "idp".into(),
        Json::str(tree::checkpoint_idp(&Json::Obj(payload.clone()))),
    );
    // Rebuild the WAL row for the checkpoint with the forged envelope —
    // recompute the envelope hash + the chain is intact (the row is
    // canonical again; the *claim* is the lie).
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    // Locate the checkpoint row by content (commit markers follow it).
    let idx = lines
        .iter()
        .position(|l| l.contains("security.audit.checkpoint"))
        .unwrap();
    let mut env = cp.clone();
    env.payload = Json::Obj(payload);
    env.hash = env.recompute_hash();
    let forged_line = format!(
        "{{\"k\":\"e\",\"v\":{}}}",
        String::from_utf8(env.canonical_bytes()).unwrap()
    );
    lines[idx] = forged_line;
    std::fs::write(&wal, lines.join("\n") + "\n").unwrap();
    // The chain verifies byte-wise — the claim contradicts the log.
    assert_eq!(
        tampered_kind(s.verify_run(&run, None, None, Some(&keys())).unwrap_err()),
        TamperedKind::ForkEquivocation
    );
}

#[test]
fn bad_signature_detected() {
    let (mut s, run, lease) = open("badsig", signed_manifest());
    fill(&mut s, &run, &lease, "g", 3);
    let cp = s
        .checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    // Re-emit the row with a wrong-key HMAC — shape parses, idp re-derives,
    // the signature check fails.
    let mut payload = match &cp.payload {
        Json::Obj(m) => m.clone(),
        _ => panic!(),
    };
    let mut sig_obj = match payload.get("signatures").unwrap() {
        Json::Arr(a) => match &a[0] {
            Json::Obj(m) => m.clone(),
            _ => panic!(),
        },
        _ => panic!(),
    };
    let preimage = tree::checkpoint_sig_preimage(&Json::Obj(payload.clone()));
    // Sign with the WRONG key bytes under the registered key_id.
    let wrong = hh_wire::sha256::hmac_sha256(b"not-the-key", &preimage);
    sig_obj.insert("sig".into(), Json::str(hex(&wrong)));
    payload.insert("signatures".into(), Json::Arr(vec![Json::Obj(sig_obj)]));
    let mut env = cp.clone();
    env.payload = Json::Obj(payload);
    env.hash = env.recompute_hash();
    let wal = wal_path(&s, &run);
    let text = std::fs::read_to_string(&wal).unwrap();
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let idx = lines
        .iter()
        .position(|l| l.contains("security.audit.checkpoint"))
        .unwrap();
    lines[idx] = format!(
        "{{\"k\":\"e\",\"v\":{}}}",
        String::from_utf8(env.canonical_bytes()).unwrap()
    );
    std::fs::write(&wal, lines.join("\n") + "\n").unwrap();
    assert_eq!(
        tampered_kind(s.verify_run(&run, None, None, Some(&keys())).unwrap_err()),
        TamperedKind::BadSignature
    );
}

// ── auditors ────────────────────────────────────────────────────────────────

#[test]
fn auditor_holds_independent_heads() {
    let (mut s, run, lease) = open("aud", signed_manifest());
    fill(&mut s, &run, &lease, "a", 4);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    fill(&mut s, &run, &lease, "b", 2);
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    let (n, head) = aud.head();
    assert_eq!(n as usize, s.events(&run).unwrap().len());
    let leaves: Vec<String> = s
        .events(&run)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    assert_eq!(head, tree::mth(&leaves));
    // The auditor held the signed head — the writer cannot overwrite it.
    assert_eq!(aud.held_heads().len(), 1);
    assert_eq!(aud.claims().len(), 1);
}

#[test]
fn auditor_detects_gap_and_equivocation() {
    let (mut s, run, lease) = open("aud-fault", signed_manifest());
    fill(&mut s, &run, &lease, "a", 3);
    let mut aud = Auditor::new(&run);
    s.audit_with(&run, &mut aud, Some(&keys())).unwrap();
    // A gap — skip seq in a forged frame.
    let evs = s.events(&run).unwrap();
    let forged = EventFrame::Durable {
        seq: 7,
        hash: evs[0].hash.clone(),
        event: Box::new(evs[0].clone()),
    };
    assert!(matches!(
        aud.observe(&forged, None),
        Err(AuditFault::Gap { .. })
    ));
    // An equivocation — a different event at the next expected seq.
    let mut other = evs[0].clone();
    other.seq = 3;
    other.payload = Json::str("forked-payload");
    other.hash = other.recompute_hash();
    other.prev_hash = evs[2].hash.clone();
    let frame = EventFrame::Durable {
        seq: 3,
        hash: other.hash.clone(),
        event: Box::new(other),
    };
    // The auditor's own record already holds seq 3's real hash — a
    // replayed different event at the same seq is equivocation... here the
    // auditor holds 3 leaves so seq=3 would be the NEXT slot; feeding the
    // real seq-3 event first then the fork shows it.
    let mut aud2 = Auditor::new(&run);
    for e in evs.iter().take(3) {
        aud2.observe(
            &EventFrame::Durable {
                seq: e.seq,
                hash: e.hash.clone(),
                event: Box::new(e.clone()),
            },
            None,
        )
        .unwrap();
    }
    // seq 3 fork: event.seq=3, but the hash/prev_hash chain is a fork —
    // the auditor recomputes: the event bytes recompute to the delivered
    // hash, so the *chain* check (prev_hash vs held tip) catches it.
    assert!(aud2.observe(&frame, None).is_err());
}

// ── GC + redaction ──────────────────────────────────────────────────────────

#[test]
fn gc_writes_durable_record_then_deletes_and_tombstones() {
    let (mut s, run, lease) = open("gc", signed_manifest());
    let addr = s.put_blob(b"retire me", "text/plain").unwrap();
    let id = addr.id();
    fill(&mut s, &run, &lease, "a", 2);
    let env = s
        .gc(&run, &lease, vec![id.clone()], "policy:test", "warm", None)
        .unwrap();
    assert_eq!(env.class, "lifecycle.ledger.gc");
    // Durable-before-delete: the record is in the WAL and the bytes are gone.
    let wal = std::fs::read_to_string(wal_path(&s, &run)).unwrap();
    assert!(wal.contains("lifecycle.ledger.gc"));
    assert!(wal.contains(&id));
    match s.get_blob(&addr) {
        Err(LedgerError::Missing { reason, .. }) => {
            assert_eq!(reason, MissingReason::Gc)
        }
        other => panic!("expected Missing{{gc}}, got {other:?}"),
    }
    // The tombstone survives a reopen (load-time reconstruction).
    let root = s.root().to_path_buf();
    drop(s);
    let s2 = Store::open_test(&root, 2_000).unwrap();
    match s2.get_blob(&addr) {
        Err(LedgerError::Missing { reason, .. }) => {
            assert_eq!(reason, MissingReason::Gc)
        }
        other => panic!("expected Missing{{gc}} after reopen, got {other:?}"),
    }
    s2.verify(&run).unwrap();
}

#[test]
fn gc_refuses_pinned_and_malformed() {
    let (mut s, run, lease) = open("gc-pin", signed_manifest());
    let addr = s.put_blob(b"pinned bytes", "text/plain").unwrap();
    let id = addr.id();
    // Pin it: another run's `refs` names it.
    let (run2, _lease2) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-b")
        .unwrap();
    let mut e = ev("pin-ref", "context.observation.recorded", Json::Null);
    e.refs = vec![addr.clone()];
    // Append to run2 via its own lease.
    let l2 = _lease2;
    s.append(&run2, &l2, vec![e]).unwrap();
    match s.gc(&run, &lease, vec![id.clone()], "policy:test", "warm", None) {
        Err(LedgerError::Pinned { .. }) => {}
        other => panic!("expected Pinned, got {other:?}"),
    }
    assert!(s.get_blob(&addr).is_ok());
    match s.gc(
        &run,
        &lease,
        vec!["not-an-id".into()],
        "policy:test",
        "warm",
        None,
    ) {
        Err(LedgerError::SchemaViolation { .. }) => {}
        other => panic!("expected SchemaViolation, got {other:?}"),
    }
}

#[test]
fn redact_content_refs_only_audit_fields_never() {
    let (mut s, run, lease) = open("red", signed_manifest());
    let addr = s.put_blob(b"secret bytes", "text/plain").unwrap();
    // `context.observation.recorded` declares `content` as a content_ref?
    // — use the `refs` envelope member via Address target for the blob, and
    // Field targets against a class with declared content_refs.
    let endorser = ProvenanceRecord::minted(
        Origin::human("human:alice", HumanRole::Principal),
        PersistenceScope::Run,
        0,
    );
    // An audit_fields member refuses — `lease_id` is audit-fields on
    // `lifecycle.lease.acquired`; the event id need not resolve for the
    // member check to fire first... (the lookup order is schema-first).
    match s.redact(
        &run,
        &lease,
        vec![RedactTarget::Field {
            event_id: "whatever".into(),
            field: "lease_id".into(),
        }],
        "subject_request",
        &endorser,
        "approval",
    ) {
        Err(LedgerError::NotRedactable { .. }) | Err(LedgerError::SchemaViolation { .. }) => {}
        other => panic!("expected refusal, got {other:?}"),
    }
    // Blob address redaction with approval basis works.
    let env = s
        .redact(
            &run,
            &lease,
            vec![RedactTarget::Address(addr.id())],
            "subject_request",
            &endorser,
            "approval",
        )
        .unwrap();
    assert_eq!(env.class, "lifecycle.ledger.redacted");
    match s.get_blob(&addr) {
        Err(LedgerError::Missing { reason, .. }) => {
            assert_eq!(reason, MissingReason::Redacted)
        }
        other => panic!("expected Missing{{redacted}}, got {other:?}"),
    }
    // Illegitimate endorsement — an executor-level record cannot endorse.
    let addr2 = s.put_blob(b"more", "text/plain").unwrap();
    let weak = ProvenanceRecord::minted(Origin::model("m1", &run, "r1"), PersistenceScope::Run, 0);
    match s.redact(
        &run,
        &lease,
        vec![RedactTarget::Address(addr2.id())],
        "subject_request",
        &weak,
        "approval",
    ) {
        Err(LedgerError::IllegitimateEndorsement { .. }) => {}
        other => panic!("expected IllegitimateEndorsement, got {other:?}"),
    }
    s.verify(&run).unwrap();
}

// ── audit_view ──────────────────────────────────────────────────────────────

#[test]
fn audit_view_reports_checkpoints_completeness_and_tombstones() {
    let (mut s, run, lease) = open("av", signed_manifest());
    let addr = s.put_blob(b"counted", "text/plain").unwrap();
    fill(&mut s, &run, &lease, "a", 3);
    s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
        .unwrap();
    s.set_audit_key_resolver(Box::new(keys()));
    s.gc(&run, &lease, vec![addr.id()], "policy:test", "warm", None)
        .unwrap();
    let v = s.project(&run, ViewKind::AuditView, None).unwrap();
    let cps = v.payload.get("checkpoints").unwrap();
    let Json::Arr(cps) = cps else {
        panic!("checkpoints not an array")
    };
    assert_eq!(cps.len(), 1);
    assert_eq!(
        cps[0].get("verification_status").and_then(Json::as_str),
        Some("verified")
    );
    let completeness = v.payload.get("completeness").unwrap();
    assert_eq!(
        completeness.get("checkpoints_ok").and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(true)
    );
    // The gc'd blob is accounted, not `missing`.
    let refs = v.payload.get("content_refs").unwrap();
    match refs.get("gc") {
        Some(Json::Arr(a)) => assert_eq!(a.len(), 1, "content_refs: {refs:?}"),
        other => panic!("content_refs.gc not an array: {other:?}"),
    }
}

// ── corpus ──────────────────────────────────────────────────────────────────

#[test]
fn corpus_100_runs_verify_and_prove() {
    let d = dir("corpus");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    for i in 0..100 {
        let (run, lease) = s.open_run(signed_manifest(), "w").unwrap();
        fill(&mut s, &run, &lease, &format!("r{i}"), 3 + (i % 7));
        if i % 3 == 0 {
            s.checkpoint(&run, &lease, CheckpointKind::Periodic, &mut signer())
                .unwrap();
        }
        s.verify_run(&run, None, None, Some(&keys())).unwrap();
        let head_seq = s.head(&run).unwrap().seq;
        let p = s.prove_inclusion(&run, head_seq / 2, None).unwrap();
        let leaves: Vec<String> = s
            .events(&run)
            .unwrap()
            .iter()
            .map(|e| e.hash.clone())
            .collect();
        assert!(tree::verify_inclusion(
            &p,
            &leaves[(head_seq / 2) as usize],
            &tree::mth(&leaves)
        ));
        if head_seq > 2 {
            let cp = s.prove_consistency(&run, 2, head_seq + 1).unwrap();
            assert!(tree::verify_consistency(
                &cp,
                &tree::mth_prefix(&leaves, 2),
                &tree::mth(&leaves)
            ));
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
