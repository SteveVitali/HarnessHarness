//! R3.8 acceptance coverage — `hharness_secrets::aead::RingBlobCipher`, the
//! workspace's one real AEAD construction over `ring::aead::AES_256_GCM`
//! (ADR-0359's reserved `ring::aead`/`ring::rand` scope consumed;
//! ADR-0361; §5g.6 C2 "blob encryption with per-event keys"; I-A8's
//! `missing{key_unavailable}` typing on the authenticated-tamper leg).
//!
//! The master keys here are **fixture constructions** — `n/a{custody}`,
//! never custody evidence (OQ-170; real key custody is external by design
//! and arrives through the same `BlobCipher` seam, not this file).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_ledger::cipher::{self, BlobCipher};
use hharness_ledger::errors::{LedgerError, MissingReason};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_secrets::{RingBlobCipher, BLOB_AEAD_ALG};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-r38-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A 32-byte fixture master key — deterministic test material, explicitly
/// `n/a{custody}`.
fn fixture_master(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn cipher_with(key_id: &str, master: [u8; 32]) -> RingBlobCipher {
    RingBlobCipher::new(key_id, master).unwrap()
}

fn blob_addr(bytes: &[u8]) -> hharness_identity::idp::ContentAddress {
    hharness_identity::idp::address(bytes, "text/plain")
}

#[test]
fn ring_seal_produces_real_ciphertext_bound_to_header() {
    let c = cipher_with("key-a", fixture_master(0xA1));
    let plaintext = b"per-event audit payload - sealed at rest";
    let addr = blob_addr(plaintext);
    let frame = c.seal(&addr, plaintext).unwrap();
    assert!(cipher::is_sealed(&frame));
    assert!(
        !frame
            .windows(plaintext.len())
            .any(|w| w == plaintext.as_slice()),
        "the frame carries no plaintext window"
    );
    let (h, ct) = cipher::parse_frame(&frame).unwrap();
    assert_eq!(h.alg, BLOB_AEAD_ALG);
    assert_eq!(h.key_id, "key-a");
    assert_eq!(h.nonce.len(), 12, "AES-256-GCM's 96-bit nonce");
    assert_eq!(ct.len(), plaintext.len() + 16, "body + 128-bit tag");
    assert_eq!(c.open(&addr, &frame).unwrap(), plaintext);
    assert_eq!(c.algorithm(), BLOB_AEAD_ALG);
    assert_eq!(c.key_id(), "key-a");
}

#[test]
fn ring_frames_carry_fresh_nonces_for_identical_plaintext() {
    let c = cipher_with("key-a", fixture_master(0xA2));
    let addr = blob_addr(b"same");
    let f1 = c.seal(&addr, b"same").unwrap();
    let f2 = c.seal(&addr, b"same").unwrap();
    assert_ne!(f1, f2, "SystemRandom draws distinct nonces per seal");
    assert_eq!(c.open(&addr, &f1).unwrap(), b"same");
    assert_eq!(c.open(&addr, &f2).unwrap(), b"same");
}

#[test]
fn tampered_ciphertext_fails_the_tag_and_reports_key_unavailable() {
    // The I-A8 leg the fixture cipher cannot cover: a *real* AEAD tag
    // failure opens as `Err` and the pool reports
    // `Missing{key_unavailable}` — never Tampered (ADR-0068 R3).
    let d = dir("aead-tamper");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    s.open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    s.set_blob_cipher(Box::new(cipher_with("key-a", fixture_master(0xA3))));
    let addr = s
        .put_blob(b"authenticated residency", "text/plain")
        .unwrap();
    let path = d.join("blobs").join(&addr.digest);
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();
    match s.get_blob(&addr) {
        Err(LedgerError::Missing {
            reason: MissingReason::KeyUnavailable,
            ..
        }) => {}
        other => panic!("AEAD tag failure must be key_unavailable, got {other:?}"),
    }
}

#[test]
fn header_tamper_fails_the_tag_too() {
    // The HHBE header is the AEAD's AAD — flipping a `key_id` byte refuses
    // at `open`, so a forged custody coordinate cannot ride a valid body.
    let c = cipher_with("key-a", fixture_master(0xA4));
    let addr = blob_addr(b"body");
    let mut frame = c.seal(&addr, b"body").unwrap();
    // Find the 'k' of "key-a" inside the frame header and flip it.
    let needle = b"key-a";
    let pos = frame
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    frame[pos] = b'K';
    match c.open(&addr, &frame) {
        Err(_) => {}
        Ok(_) => panic!("a header tamper must refuse at open"),
    }
}

#[test]
fn wrong_key_material_and_foreign_coordinates_refuse() {
    let addr = blob_addr(b"bound bytes");
    let a = cipher_with("key-a", fixture_master(0xA5));
    let frame = a.seal(&addr, b"bound bytes").unwrap();
    // Foreign key_id — the coordinate check fires before the AEAD.
    let b = cipher_with("key-b", fixture_master(0xA5));
    assert!(b.open(&addr, &frame).is_err());
    // Same coordinate, wrong master — the per-blob key differs; the tag fails.
    let c2 = cipher_with("key-a", fixture_master(0xB5));
    assert!(c2.open(&addr, &frame).is_err());
    // Wrong address — the per-event key derivation binds the plaintext id;
    // a frame replayed under another address refuses (per-event keys, C2).
    let other_addr = blob_addr(b"other bytes");
    assert!(a.open(&other_addr, &frame).is_err());
}

#[test]
fn store_round_trips_through_the_ring_cipher_end_to_end() {
    let d = dir("e2e");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, _lease) = s
        .open_run(
            {
                let mut m = RunManifest::minimal(RunKind::Agent);
                m.blob_encryption = Some(hharness_ledger::manifest::BlobEncryptionDecl {
                    alg: BLOB_AEAD_ALG.to_string(),
                    key_id: "key-a".to_string(),
                });
                m
            },
            "w",
        )
        .unwrap();
    s.set_blob_cipher(Box::new(cipher_with("key-a", fixture_master(0xA6))));
    let addr = s.put_blob(b"the dossier", "application/json").unwrap();
    let stored = std::fs::read(d.join("blobs").join(&addr.digest)).unwrap();
    assert!(cipher::is_sealed(&stored));
    assert_eq!(s.get_blob(&addr).unwrap(), b"the dossier");
    // Restart under the same custody coordinate — the frame still opens.
    drop(s);
    let mut s2 = Store::open_test(&d, 2_000).unwrap();
    s2.set_blob_cipher(Box::new(cipher_with("key-a", fixture_master(0xA6))));
    assert_eq!(s2.get_blob(&addr).unwrap(), b"the dossier");
    // The audit surface reads the declared coverage honestly.
    let view = s2.project(&run, hharness_ledger::views::ViewKind::AuditView, None);
    assert!(view.is_ok());
}
