//! R4.15e acceptance coverage — the ledger's alg-aware checkpoint seam
//! (OQ-170; ADR-0376). The real Ed25519 signing lives in `hh-secrets`
//! (the only `ring`-admitted crate — its battery covers the crypto); the
//! ledger side is the *seam*: `alg_ref` plumbing, the `<alg>:<hex>` codec,
//! the effective signer-key set (`signer_key_ids` extended by verified
//! `key_rotation` claims), and the co-signed custody-rotation mint.
//!
//! The `EdSigner`/`EdTable` fixtures stand in for the Ed25519 pair with a
//! deterministic 64-byte construction — they exercise the algorithm's
//! declared `alg_ref`, signature length and resolver dispatch, not the
//! crypto itself (hh-ledger holds no ring dep by contract).
//!
//! Each test fails if the behaviour it covers is removed.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::audit::{
    check_checkpoint_signatures, key_rotation_members, parse_sig_alg, render_sig_alg,
    AuditKeyResolver, AuditSigner, CheckpointKind, SignatureDefect, CHECKPOINT_ALG, ED25519_ALG,
};
use hh_ledger::errors::{LedgerError, TamperedKind};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::tree::{checkpoint_sig_preimage, parse_checkpoint};
use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-r415e-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A deterministic 64-byte stand-in for an Ed25519 signature — twice the
/// HMAC construction so the bytes are length-faithful for the codec. The
/// public "key" the resolver holds IS the signing secret here (fixture
/// symmetry — the same trick `FixedSigner` uses for HMAC).
fn ed_sig(key: &[u8], preimage: &[u8]) -> Vec<u8> {
    let first = hmac_sha256(key, preimage);
    let second = hmac_sha256(key, &first);
    [first.as_slice(), second.as_slice()].concat()
}

struct EdSigner {
    key_id: String,
    key: Vec<u8>,
}

impl EdSigner {
    fn new(key_id: &str) -> Self {
        Self {
            key_id: key_id.to_string(),
            key: format!("ed-secret:{key_id}").into_bytes(),
        }
    }
}

impl AuditSigner for EdSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }
    fn alg_ref(&self) -> &'static str {
        ED25519_ALG
    }
    fn sign(&mut self, preimage: &[u8]) -> Result<Vec<u8>, String> {
        Ok(ed_sig(&self.key, preimage))
    }
}

/// The resolver over the fixture "public" table — dispatches both
/// admitted `alg_ref` spellings like the custody resolver does.
struct EdTable(BTreeMap<String, Vec<u8>>);

impl EdTable {
    fn of(signers: &[&EdSigner]) -> Self {
        Self(
            signers
                .iter()
                .map(|s| (s.key_id.clone(), s.key.clone()))
                .collect(),
        )
    }
}

impl AuditKeyResolver for EdTable {
    fn verify_key(&self, key_id: &str) -> Option<Vec<u8>> {
        self.0.get(key_id).cloned()
    }
    fn verify_sig(&self, alg_ref: &str, key: &[u8], preimage: &[u8], sig: &[u8]) -> Option<bool> {
        match alg_ref {
            ED25519_ALG => Some(ed_sig(key, preimage) == sig),
            CHECKPOINT_ALG => Some(hmac_sha256(key, preimage).to_vec() == sig),
            _ => None,
        }
    }
}

fn signed_manifest(key_ids: &[&str]) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.signer_key_ids = key_ids.iter().map(|s| s.to_string()).collect();
    m
}

fn open(tag: &str, manifest: RunManifest) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest, "writer-a").unwrap();
    (s, run, lease)
}

// ── the alg-aware codec ─────────────────────────────────────────────────────

#[test]
fn ed25519_sig_codec_round_trips() {
    let sig = vec![0xabu8; 64];
    let rendered = render_sig_alg(ED25519_ALG, &sig);
    assert!(rendered.starts_with("ed25519:"));
    assert_eq!(rendered.len(), "ed25519:".len() + 128);
    assert_eq!(parse_sig_alg(ED25519_ALG, &rendered), Some(sig.clone()));
    // Wrong alg spelling, wrong length, malformed hex — all refuse.
    assert_eq!(parse_sig_alg(CHECKPOINT_ALG, &rendered), None);
    assert_eq!(parse_sig_alg(ED25519_ALG, "ed25519:abcd"), None);
    assert_eq!(
        parse_sig_alg(ED25519_ALG, &format!("ed25519:{}", "zz".repeat(64))),
        None
    );
    // The HMAC construction keeps its own spelling (C0 compatibility).
    let hmac = render_sig_alg(CHECKPOINT_ALG, &[0x01u8; 32]);
    assert!(hmac.starts_with("hmac-sha256:"));
    assert_eq!(parse_sig_alg(ED25519_ALG, &hmac), None);
}

// ── signed checkpoints under the Ed25519 seam ───────────────────────────────

#[test]
fn ed25519_checkpoint_verifies_through_the_resolver() {
    let signer = EdSigner::new("ed25519-aaaabbbbccccdddd");
    let (mut store, run, lease) = open("ed-ok", signed_manifest(&[&signer.key_id]));
    store.set_audit_key_resolver(Box::new(EdTable::of(&[&signer])));
    let mut s = signer;
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut s)
        .unwrap();
    // The durable row spells the algorithm honestly.
    let cp = store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.audit.checkpoint")
        .unwrap();
    let claim = parse_checkpoint(&cp.payload).unwrap();
    let entry = &claim.signatures[0];
    assert_eq!(
        entry.get("alg_ref").and_then(Json::as_str),
        Some(ED25519_ALG)
    );
    assert!(entry
        .get("sig")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("ed25519:"));
    store.verify(&run).unwrap();
}

#[test]
fn wrong_key_bytes_fail_verify() {
    let signer = EdSigner::new("ed25519-1111222233334444");
    let kid = signer.key_id.clone();
    let (mut store, run, lease) = open("ed-bad", signed_manifest(&[&kid]));
    let mut s = signer;
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut s)
        .unwrap();
    // The resolver serves *different* bytes for the same id — a custody
    // substitution must surface as a signature failure, never a pass.
    let mut table = BTreeMap::new();
    table.insert(kid, b"substituted-key".to_vec());
    store.set_audit_key_resolver(Box::new(EdTable(table)));
    match store.verify(&run) {
        Err(LedgerError::Tampered(t)) => {
            assert_eq!(t.kind, TamperedKind::BadSignature, "{t:?}")
        }
        other => panic!("expected Tampered(BadSignature), got {other:?}"),
    }
}

#[test]
fn undeclared_signer_is_refused_at_mint() {
    let signer = EdSigner::new("ed25519-notdeclared001");
    let (mut store, run, lease) = open("ed-undecl", signed_manifest(&["ed25519-other00000000"]));
    let mut s = signer;
    match store.checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut s) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}

// ── the co-signed custody rotation ──────────────────────────────────────────

#[test]
fn key_rotation_checkpoint_is_cosigned_and_folds() {
    let incumbent = EdSigner::new("ed25519-incumbent00000");
    let successor = EdSigner::new("ed25519-successor00000");
    // The manifest declares the incumbent only — the successor is admitted
    // by the verified rotation claim itself.
    let (mut store, run, lease) = open("ed-rot", signed_manifest(&[&incumbent.key_id]));
    store.set_audit_key_resolver(Box::new(EdTable::of(&[&incumbent, &successor])));
    let mut inc = incumbent;
    let mut suc = successor;
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut inc)
        .unwrap();
    let rot = store
        .signer_rotation_checkpoint(&run, &lease, &mut inc, &mut suc)
        .unwrap();
    // The durable claim carries the custody pair + both signatures.
    let claim = parse_checkpoint(&rot.payload).unwrap();
    assert_eq!(claim.kind, "rotation");
    let (from, to) = key_rotation_members(&claim).unwrap().unwrap();
    assert_eq!(from, inc.key_id());
    assert_eq!(to, suc.key_id());
    let signers: BTreeSet<&str> = claim
        .signatures
        .iter()
        .filter_map(|s| s.get("key_id").and_then(Json::as_str))
        .collect();
    assert!(signers.contains(inc.key_id()), "incumbent co-signs");
    assert!(signers.contains(suc.key_id()), "successor co-signs");
    // The successor is now admitted: a later checkpoint under its key
    // mints without a manifest amendment.
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut suc)
        .unwrap();
    store.verify(&run).unwrap();
}

#[test]
fn a_successor_mint_before_its_rotation_is_refused() {
    let incumbent = EdSigner::new("ed25519-incumbent00000");
    let successor = EdSigner::new("ed25519-successor00000");
    let (mut store, run, lease) = open("ed-order", signed_manifest(&[&incumbent.key_id]));
    let mut suc = successor;
    match store.checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut suc) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}

#[test]
fn rotation_claim_without_the_incumbent_cosignature_fails_verify() {
    // Hand-build the unsigned claim the mint would produce, sign it ONLY
    // under the successor — `check_checkpoint_signatures` must refuse the
    // missing incumbent attestation.
    let incumbent = EdSigner::new("ed25519-incumbent00000");
    let successor = EdSigner::new("ed25519-successor00000");
    let (mut store, run, lease) = open("ed-forge", signed_manifest(&[&incumbent.key_id]));
    let mut inc = incumbent;
    let mut suc = successor;
    let rot = store
        .signer_rotation_checkpoint(&run, &lease, &mut inc, &mut suc)
        .unwrap();
    let mut claim = parse_checkpoint(&rot.payload).unwrap();
    // Strip the incumbent's signature — the forgery the verifier rejects.
    claim
        .signatures
        .retain(|s| s.get("key_id").and_then(Json::as_str) == Some(suc.key_id()));
    let preimage = checkpoint_sig_preimage(&rot.payload);
    let effective: BTreeSet<String> = [inc.key_id.to_string()].into_iter().collect();
    let table = EdTable::of(&[&inc, &suc]);
    match check_checkpoint_signatures(&claim, &preimage, &effective, Some(&table)) {
        Err(SignatureDefect::BadRotation(detail)) => {
            assert!(detail.contains("incumbent"), "{detail}");
        }
        other => panic!("expected BadRotation, got {other:?}"),
    }
}

#[test]
fn rotation_from_an_unadmitted_incumbent_is_refused_at_mint() {
    let outsider = EdSigner::new("ed25519-outsider0000000");
    let insider = EdSigner::new("ed25519-insider00000000");
    let (mut store, run, lease) = open("ed-rot-gate", signed_manifest(&[&insider.key_id]));
    let mut out = outsider;
    let mut ins = insider;
    match store.signer_rotation_checkpoint(&run, &lease, &mut out, &mut ins) {
        Err(LedgerError::SignerUnavailable { .. }) => {}
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}

#[test]
fn a_malformed_key_rotation_member_is_a_defect() {
    let incumbent = EdSigner::new("ed25519-incumbent00000");
    let successor = EdSigner::new("ed25519-successor00000");
    let (mut store, run, lease) = open("ed-rot-shape", signed_manifest(&[&incumbent.key_id]));
    let mut inc = incumbent;
    let mut suc = successor;
    let rot = store
        .signer_rotation_checkpoint(&run, &lease, &mut inc, &mut suc)
        .unwrap();
    let mut claim = parse_checkpoint(&rot.payload).unwrap();
    // Corrupt the member — `to_key_id` gone.
    claim.key_rotation = Some(Json::obj([("from_key_id", Json::str(inc.key_id()))]));
    match key_rotation_members(&claim) {
        Err(SignatureDefect::BadRotation(detail)) => {
            assert!(detail.contains("to_key_id"), "{detail}")
        }
        other => panic!("expected BadRotation, got {other:?}"),
    }
}

// ── the effective-set fold through verify_run ───────────────────────────────

#[test]
fn verify_run_admits_rotated_keys_across_the_chain() {
    let k1 = EdSigner::new("ed25519-keyone00000000");
    let k2 = EdSigner::new("ed25519-keytwo00000000");
    let k3 = EdSigner::new("ed25519-keythree000000");
    let (mut store, run, lease) = open("ed-chain", signed_manifest(&[&k1.key_id]));
    store.set_audit_key_resolver(Box::new(EdTable::of(&[&k1, &k2, &k3])));
    let mut a = k1;
    let mut b = k2;
    let mut c = k3;
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut a)
        .unwrap();
    // Two chained rotations: k1→k2, then k2→k3 — each co-signed.
    store
        .signer_rotation_checkpoint(&run, &lease, &mut a, &mut b)
        .unwrap();
    store
        .signer_rotation_checkpoint(&run, &lease, &mut b, &mut c)
        .unwrap();
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut c)
        .unwrap();
    store.verify(&run).unwrap();
}

#[test]
fn verify_run_without_a_resolver_stays_honest() {
    let signer = EdSigner::new("ed25519-noresolve00000");
    let (mut store, run, lease) = open("ed-nores", signed_manifest(&[&signer.key_id]));
    let mut s = signer;
    store
        .checkpoint(&run, &lease, CheckpointKind::OnDemand, &mut s)
        .unwrap();
    // No resolver installed: shape-level checks still run (declared id +
    // alg + parseable sig) but crypto can't be asserted — verify must not
    // fabricate ok either way; an *unresolvable* key is a defect when a
    // resolver IS present.
    let empty = EdTable(BTreeMap::new());
    store.set_audit_key_resolver(Box::new(empty));
    // A key the resolver cannot serve is `SignerUnavailable` — an honest
    // typed refusal, never a fabricated pass (CC4).
    match store.verify(&run) {
        Err(LedgerError::SignerUnavailable { detail, .. }) => {
            assert!(detail.contains("does not resolve"), "{detail}");
        }
        other => panic!("expected SignerUnavailable, got {other:?}"),
    }
}
