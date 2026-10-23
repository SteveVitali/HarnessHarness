//! R3.8 acceptance coverage — per-event blob encryption at rest in the blob
//! pool (BL-45; DF-S1.15-3's last cell; R-2.8.6's non-custody emission legs;
//! ADR-0345 revisit trigger (a) fired-answered; ADR-0359 scope consumed;
//! ADR-0361).
//!
//! This battery exercises the *pool plumbing* over a **fixture cipher** —
//! `FixtureCipher` is a deterministic address-derived mask, NOT
//! authenticated encryption and NOT custody evidence (`n/a{custody}`;
//! OQ-170). What it honestly covers: the `HHBE` frame residency the store
//! writes, the seam's install/clear posture, the `Missing{key_unavailable}`
//! typing on foreign/absent custody, and the `BlobCorrupt` digest fallback
//! a non-authenticating fixture leaves to the pool. The real AEAD's
//! authenticated-tamper leg lives in `hh-secrets/tests/r3_8.rs` — together
//! the two batteries fail if the encryption is removed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_ledger::cipher::{self, BlobCipher, SealHeader};
use hharness_ledger::errors::{LedgerError, MissingReason};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::ids::{ManualClock, SeqIds, ROOT_EVENT};
use hharness_ledger::manifest::{BlobEncryptionDecl, RunKind, RunManifest};
use hharness_ledger::retention::RetentionPolicy;
use hharness_ledger::store::{Lease, RedactTarget, Store, DEFAULT_BLOB_MAX_BYTES};
use hharness_ledger::views::ViewKind;
use hharness_ledger::BlobStatus;
use hharness_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-ledger-r38-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

// ── the fixture cipher (explicitly not crypto — see the header note) ───────

const FIXTURE_ALG: &str = "toy-mask.hh-blob.fixture";
const FIXTURE_KEY_ID: &str = "fixture-key-a";

/// `FixtureCipher{key_id}` — a deterministic address-keyed mask. `seal`
/// XORs the body with the plaintext address's digest bytes and frames it;
/// `open` refuses a foreign `alg`/`key_id` (the custody-coordinate refusal
/// the pool maps to `Missing{key_unavailable}`) and unmasks otherwise.
/// Tamper the fixture cannot authenticate surfaces honestly downstream as
/// `BlobCorrupt` (the plaintext-digest check — defense in depth the spec
/// binds at I-A8 regardless of cipher strength).
struct FixtureCipher {
    key_id: &'static str,
}

impl BlobCipher for FixtureCipher {
    fn algorithm(&self) -> &str {
        FIXTURE_ALG
    }
    fn key_id(&self) -> &str {
        self.key_id
    }
    fn seal(
        &self,
        address: &hharness_identity::idp::ContentAddress,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, String> {
        let mask = address.digest.as_bytes();
        let body: Vec<u8> = plaintext
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % mask.len()])
            .collect();
        cipher::encode_frame(
            &SealHeader {
                alg: FIXTURE_ALG.to_string(),
                key_id: self.key_id.to_string(),
                nonce: vec![0x00],
            },
            &body,
        )
    }
    fn open(
        &self,
        address: &hharness_identity::idp::ContentAddress,
        frame: &[u8],
    ) -> Result<Vec<u8>, String> {
        let (h, ct) = cipher::parse_frame(frame).ok_or("not an HHBE v1 frame")?;
        if h.alg != FIXTURE_ALG {
            return Err(format!("foreign alg {}", h.alg));
        }
        if h.key_id != self.key_id {
            return Err(format!("foreign key_id {}", h.key_id));
        }
        let mask = address.digest.as_bytes();
        Ok(ct
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % mask.len()])
            .collect())
    }
}

/// A cipher whose `seal` always faults — the custody-side refusal the pool
/// must surface as `CipherFault`, never a silent plaintext write (CC3).
struct FailingCipher;

impl BlobCipher for FailingCipher {
    fn algorithm(&self) -> &str {
        "fails.hh-blob.fixture"
    }
    fn key_id(&self) -> &str {
        "fixture-key-fails"
    }
    fn seal(
        &self,
        _address: &hharness_identity::idp::ContentAddress,
        _plaintext: &[u8],
    ) -> Result<Vec<u8>, String> {
        Err("custody refused the seal".to_string())
    }
    fn open(
        &self,
        _address: &hharness_identity::idp::ContentAddress,
        _frame: &[u8],
    ) -> Result<Vec<u8>, String> {
        Err("custody refused the open".to_string())
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn open(tag: &str, manifest: RunManifest) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s.open_run(manifest, "writer-a").unwrap();
    (s, run, lease)
}

fn k_ev(id: &str, class: &str, payload: Json) -> Event {
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

/// The blob file bytes at `root` for a content address — the at-rest
/// residency the byte-level assertions read.
fn stored(root: &Path, addr: &hharness_identity::idp::ContentAddress) -> Vec<u8> {
    std::fs::read(root.join("blobs").join(&addr.digest)).unwrap()
}

/// Append a `lifecycle.run.rolled_back` row whose `record_ref` names the
/// blob — the `content_refs` member the `audit_view` fold accounts.
fn emit_ref_row(s: &mut Store, run: &str, lease: &Lease, id: &str, addr_str: &str) {
    s.append(
        run,
        lease,
        vec![k_ev(
            id,
            "lifecycle.run.rolled_back",
            Json::obj([
                ("to_seq", Json::Int(0)),
                ("to_event_id", Json::str(ROOT_EVENT)),
                ("reason_code", Json::str("test")),
                ("rewound", Json::Bool(true)),
                ("rollback_event_id", Json::str(id)),
                ("head_event_id", Json::str(ROOT_EVENT)),
                ("head_seq", Json::Int(0)),
                ("record_ref", Json::str(addr_str)),
            ]),
        )],
    )
    .unwrap();
}

fn encryption_member(view: &hharness_ledger::views::View) -> Json {
    let Json::Obj(p) = &view.payload else {
        panic!("audit_view payload not an object")
    };
    p.get("encryption").cloned().expect("encryption member")
}

fn addr_str(addr: &hharness_identity::idp::ContentAddress) -> String {
    format!("{}:{}", addr.algorithm, addr.digest)
}

// ── the mechanism ──────────────────────────────────────────────────────────

#[test]
fn sealed_blob_stores_ciphertext_not_plaintext_and_roundtrips() {
    let (mut s, run, _lease) = open("seal", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let plaintext = b"the audit dossier bytes - plaintext at put, ciphertext at rest";
    let addr = s.put_blob(plaintext, "application/octet-stream").unwrap();
    let bytes = stored(s.root(), &addr);
    // Byte-level: the residency is an HHBE frame, not the plaintext.
    assert!(cipher::is_sealed(&bytes), "stored bytes are sealed");
    assert_ne!(bytes.as_slice(), plaintext.as_slice());
    assert!(
        !bytes
            .windows(plaintext.len())
            .any(|w| w == plaintext.as_slice()),
        "stored bytes carry no plaintext window"
    );
    let (h, _) = cipher::parse_frame(&bytes).expect("well-formed frame");
    assert_eq!(h.alg, FIXTURE_ALG);
    assert_eq!(h.key_id, FIXTURE_KEY_ID);
    // The address still covers the plaintext (I-A8) and the read opens.
    assert_eq!(s.get_blob(&addr).unwrap(), plaintext);
    let _ = run;
}

#[test]
fn sealed_dedup_keeps_one_file_under_the_plaintext_address() {
    let (mut s, _run, _lease) = open("dedup", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let a1 = s.put_blob(b"same bytes", "text/plain").unwrap();
    let a2 = s.put_blob(b"same bytes", "text/plain").unwrap();
    assert_eq!(a1, a2, "identical plaintext deduplicates to one address");
    assert_eq!(
        std::fs::read_dir(s.root().join("blobs")).unwrap().count(),
        1,
        "one file per address"
    );
}

#[test]
fn tampered_ciphertext_is_blob_corrupt_not_plaintext() {
    let (mut s, _run, _lease) = open("tamper", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s.put_blob(b"integrity matters", "text/plain").unwrap();
    let path = s.root().join("blobs").join(&addr.digest);
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();
    // The fixture's `open` cannot authenticate — it unmasks garbage, and the
    // pool's own plaintext-digest check refuses `BlobCorrupt` (I-A8's
    // defense-in-depth leg; the authenticated-tamper leg is hharness-secrets').
    match s.get_blob(&addr) {
        Err(LedgerError::BlobCorrupt { address }) => assert_eq!(address, addr_str(&addr)),
        other => panic!("tampered sealed blob must refuse BlobCorrupt, got {other:?}"),
    }
}

#[test]
fn malformed_hhbe_frame_is_blob_corrupt() {
    let (s, _run, _lease) = open("malformed", RunManifest::minimal(RunKind::Agent));
    let addr = hharness_identity::idp::address(b"x", "text/plain");
    std::fs::create_dir_all(s.root().join("blobs")).unwrap();
    std::fs::write(
        s.root().join("blobs").join(&addr.digest),
        b"HHBEmalformed-garbage",
    )
    .unwrap();
    match s.get_blob(&addr) {
        Err(LedgerError::BlobCorrupt { .. }) => {}
        other => panic!("malformed HHBE frame must refuse BlobCorrupt, got {other:?}"),
    }
}

#[test]
fn absent_cipher_reports_missing_key_unavailable() {
    let d = dir("nocipher");
    let (mut s, _run, _lease) = {
        let mut s = Store::open_test(&d, 1_000).unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "w")
            .unwrap();
        s.set_blob_cipher(Box::new(FixtureCipher {
            key_id: FIXTURE_KEY_ID,
        }));
        (s, run, lease)
    };
    let addr = s.put_blob(b"sealed then orphaned", "text/plain").unwrap();
    drop(s);
    // Reopen with NO cipher — the custody-loss posture (ADR-0361 D5).
    let s2 = Store::open_test(&d, 2_000).unwrap();
    let id = addr_str(&addr);
    match s2.get_blob(&addr) {
        Err(LedgerError::Missing {
            address,
            reason: MissingReason::KeyUnavailable,
        }) => assert_eq!(address, id),
        other => panic!("sealed blob with no cipher must be key_unavailable, got {other:?}"),
    }
    // The residency fact is still honest without key material.
    assert_eq!(
        s2.blob_status(&id),
        BlobStatus::Sealed {
            key_id: FIXTURE_KEY_ID.to_string()
        }
    );
}

#[test]
fn foreign_key_id_reports_missing_key_unavailable() {
    let d = dir("foreign");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (_run, _lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s.put_blob(b"sealed under key-a", "text/plain").unwrap();
    drop(s);
    let mut s2 = Store::open_test(&d, 2_000).unwrap();
    s2.set_blob_cipher(Box::new(FixtureCipher { key_id: "key-b" }));
    match s2.get_blob(&addr) {
        Err(LedgerError::Missing {
            reason: MissingReason::KeyUnavailable,
            ..
        }) => {}
        other => panic!("foreign-key frame must be key_unavailable, got {other:?}"),
    }
}

#[test]
fn seal_fault_is_cipher_fault_and_writes_nothing() {
    let (mut s, _run, _lease) = open("fault", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FailingCipher));
    match s.put_blob(b"never sealed", "text/plain") {
        Err(LedgerError::CipherFault { detail }) => {
            assert!(detail.contains("fixture-key-fails"));
        }
        other => panic!("a refusing seal must fail CipherFault, got {other:?}"),
    }
    let addr = hharness_identity::idp::address(b"never sealed", "text/plain");
    assert!(
        !s.root().join("blobs").join(&addr.digest).exists(),
        "a refused seal writes no plaintext — CC3, never a silent degrade"
    );
}

#[test]
fn restart_reopens_sealed_blob_under_reinstalled_cipher() {
    let d = dir("restart");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (_run, _lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s.put_blob(b"durable ciphertext", "text/plain").unwrap();
    drop(s);
    let mut s2 = Store::open_test(&d, 2_000).unwrap();
    s2.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    assert_eq!(s2.get_blob(&addr).unwrap(), b"durable ciphertext");
    assert!(cipher::is_sealed(&stored(&d, &addr)));
}

#[test]
fn gc_and_redact_compose_over_sealed_blobs() {
    let (mut s, run, lease) = open("gcseal", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let a1 = s.put_blob(b"retired sealed", "text/plain").unwrap();
    let a2 = s.put_blob(b"redacted sealed", "text/plain").unwrap();
    s.gc(
        &run,
        &lease,
        vec![addr_str(&a1)],
        &gc_policy("pol"),
        "cold",
        None,
    )
    .unwrap();
    assert_eq!(
        s.blob_status(&addr_str(&a1)),
        BlobStatus::Tombstoned(MissingReason::Gc)
    );
    match s.get_blob(&a1) {
        Err(LedgerError::Missing {
            reason: MissingReason::Gc,
            ..
        }) => {}
        other => panic!("gc'd sealed blob must be Missing{{gc}}, got {other:?}"),
    }
    let endorser = ProvenanceRecord::minted(
        Origin::human("human:alice", HumanRole::Principal),
        PersistenceScope::Run,
        0,
    );
    s.redact(
        &run,
        &lease,
        vec![RedactTarget::Address(a2.id())],
        "subject_request",
        &endorser,
        "approval",
    )
    .unwrap();
    assert_eq!(
        s.blob_status(&addr_str(&a2)),
        BlobStatus::Tombstoned(MissingReason::Redacted)
    );
}

#[test]
fn sealed_frames_bypass_the_hhz1_residency_codec() {
    // The codec toggle rides `evaluate_retention` — the public op (ADR-0333).
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir("hhz1"),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.lease_ttl.writer_ms = 600_000;
    let (run, lease) = s.open_run(m, "writer-a").unwrap();
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s
        .put_blob(b"ciphertext does not compress", "text/plain")
        .unwrap();
    let policy = RetentionPolicy {
        policy_ref: "policy:r3.8-test".to_string(),
        blob_residency_ms: 60_000,
        bundle_retention_ms: 120_000,
        compress_cold: true,
        legal_hold: false,
    };
    s.evaluate_retention(&run, &lease, &policy, &Default::default())
        .unwrap();
    clock.advance(61_000);
    let r = s
        .evaluate_retention(&run, &lease, &policy, &Default::default())
        .unwrap();
    // The tier transition still lands (durable row + fold); the byte move
    // honestly does not — ciphertext does not compress and the `HHZ1`
    // codec cannot reach inside the AEAD envelope (ADR-0361 D4).
    assert_eq!(r.compressed, 0);
    let bytes = stored(s.root(), &addr);
    assert!(cipher::is_sealed(&bytes), "still an HHBE frame, never HHZ1");
    assert!(!retention_marker(&bytes));
    assert_eq!(s.get_blob(&addr).unwrap(), b"ciphertext does not compress");
}

/// Marker check — the retention codec's `HHZ1` magic, asserted absent
/// without touching the private codec (the residency fact, not the impl).
fn retention_marker(bytes: &[u8]) -> bool {
    bytes.starts_with(b"HHZ1")
}

#[test]
fn plaintext_blobs_keep_reading_with_a_cipher_installed() {
    let d = dir("mixed");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (_run, _lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "w")
        .unwrap();
    let plain_addr = s.put_blob(b"pre-cipher residency", "text/plain").unwrap();
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    // A blob written before the seam was installed reads as plaintext — the
    // pool never fabricates a seal retroactively (and `audit_view` counts it
    // `refs_unsealed`, honestly).
    assert_eq!(s.get_blob(&plain_addr).unwrap(), b"pre-cipher residency");
    assert_eq!(s.blob_status(&addr_str(&plain_addr)), BlobStatus::Present);
}

// ── the audit surface (the non-custody R-2.8.6 emission legs) ─────────────

fn encryption_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.blob_encryption = Some(BlobEncryptionDecl {
        alg: FIXTURE_ALG.to_string(),
        key_id: FIXTURE_KEY_ID.to_string(),
    });
    m
}

#[test]
fn audit_view_reports_enforced_when_declared_and_sealed() {
    let (mut s, run, lease) = open("enforced", encryption_manifest());
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s.put_blob(b"the dossier", "application/json").unwrap();
    emit_ref_row(&mut s, &run, &lease, "rb-1", &addr_str(&addr));
    let view = s.project(&run, ViewKind::AuditView, None).unwrap();
    let enc = encryption_member(&view);
    assert_eq!(enc.get("status").and_then(Json::as_str), Some("enforced"));
    assert_eq!(
        enc.get("declared")
            .and_then(|d| d.get("key_id"))
            .and_then(Json::as_str),
        Some(FIXTURE_KEY_ID)
    );
    assert_eq!(
        enc.get("installed")
            .and_then(|d| d.get("alg"))
            .and_then(Json::as_str),
        Some(FIXTURE_ALG)
    );
    match enc.get("refs_sealed") {
        Some(Json::Arr(a)) => assert_eq!(a.len(), 1),
        other => panic!("refs_sealed not an array: {other:?}"),
    }
    let Json::Obj(p) = &view.payload else {
        panic!()
    };
    let comp = p.get("completeness").unwrap();
    assert_eq!(comp.get("encryption_ok"), Some(&Json::Bool(true)));
}

#[test]
fn audit_view_reports_unenforced_for_plaintext_under_a_declaration() {
    let (mut s, run, lease) = open("unenforced", encryption_manifest());
    // No cipher installed — the declaration is real, the residency is not.
    let addr = s
        .put_blob(b"plaintext despite the claim", "text/plain")
        .unwrap();
    emit_ref_row(&mut s, &run, &lease, "rb-1", &addr_str(&addr));
    let view = s.project(&run, ViewKind::AuditView, None).unwrap();
    let enc = encryption_member(&view);
    assert_eq!(
        enc.get("status").and_then(Json::as_str),
        Some("unenforced"),
        "declared-but-unsealed residency never reports enforced — CC3"
    );
    let Json::Obj(p) = &view.payload else {
        panic!()
    };
    let comp = p.get("completeness").unwrap();
    assert_eq!(comp.get("encryption_ok"), Some(&Json::Bool(false)));
    assert_eq!(comp.get("headline"), Some(&Json::Bool(false)));
}

#[test]
fn audit_view_reports_undeclared_when_frames_seal_without_a_declaration() {
    let (mut s, run, lease) = open("undeclared", RunManifest::minimal(RunKind::Agent));
    s.set_blob_cipher(Box::new(FixtureCipher {
        key_id: FIXTURE_KEY_ID,
    }));
    let addr = s
        .put_blob(b"sealed but never declared", "text/plain")
        .unwrap();
    emit_ref_row(&mut s, &run, &lease, "rb-1", &addr_str(&addr));
    let view = s.project(&run, ViewKind::AuditView, None).unwrap();
    let enc = encryption_member(&view);
    assert_eq!(
        enc.get("status").and_then(Json::as_str),
        Some("undeclared"),
        "sealed residency without a durable declaration is a finding"
    );
}

#[test]
fn audit_view_reports_na_without_declaration_or_sealed_residents() {
    let (mut s, run, lease) = open("na", RunManifest::minimal(RunKind::Agent));
    let addr = s.put_blob(b"plaintext pool", "text/plain").unwrap();
    emit_ref_row(&mut s, &run, &lease, "rb-1", &addr_str(&addr));
    let view = s.project(&run, ViewKind::AuditView, None).unwrap();
    let enc = encryption_member(&view);
    assert!(
        enc.get("status").and_then(|s| s.get("n/a")).is_some() || enc.get("n/a").is_some(),
        "no declaration + no sealed refs renders n/a, got {enc:?}"
    );
}

#[test]
fn audit_view_flags_a_foreign_key_id_as_unenforced() {
    let mut m = encryption_manifest();
    m.blob_encryption.as_mut().unwrap().key_id = "declared-key-a".into();
    let (mut s, run, lease) = open("foreignkey", m);
    s.set_blob_cipher(Box::new(FixtureCipher { key_id: "key-b" }));
    let addr = s
        .put_blob(b"sealed under the wrong coordinate", "text/plain")
        .unwrap();
    emit_ref_row(&mut s, &run, &lease, "rb-1", &addr_str(&addr));
    let view = s.project(&run, ViewKind::AuditView, None).unwrap();
    let enc = encryption_member(&view);
    assert_eq!(
        enc.get("status").and_then(Json::as_str),
        Some("unenforced"),
        "a frame sealed under a foreign key_id is not enforcement of the declaration"
    );
}

#[test]
fn manifest_blob_encryption_round_trips_and_validates() {
    let m = encryption_manifest();
    let j = m.to_json();
    let back = RunManifest::from_json(&j).unwrap();
    assert_eq!(back.blob_encryption, m.blob_encryption);
    // An empty member refuses — a half-declared claim is a schema error.
    let mut bad = encryption_manifest();
    bad.blob_encryption.as_mut().unwrap().key_id = String::new();
    assert!(bad.validate().is_err());
}

fn gc_policy(policy_ref: &str) -> hharness_ledger::retention::RetentionPolicy {
    hharness_ledger::retention::RetentionPolicy {
        policy_ref: policy_ref.to_string(),
        blob_residency_ms: 0,
        bundle_retention_ms: 0,
        compress_cold: false,
        legal_hold: false,
    }
}
