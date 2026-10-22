//! R4.15e acceptance coverage — OQ-170 signer-key custody (ADR-0376):
//! the kernel-held Ed25519 ledger signer under `<store_root>/secrets/`.
//!
//! Covered: per-installation provisioning (`0600` key-id-derived files +
//! the `ledger-signer.active` pointer), load-at-boot with the sign-on-boot
//! self-check, every typed refusal (missing / wrong permissions / corrupt
//! / ambiguous / dangling — never a fabricated signer), co-signed
//! rotation custody (retired files stay resolvable), `kernel_use`
//! provisioning through the broker (the durable `security.credential.used`
//! row), and the no-leak property (key material never enters a ledger
//! payload).
//!
//! Each test fails if the behaviour it covers is removed.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::audit::{AuditKeyResolver, AuditSigner, ED25519_ALG};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_secrets::{
    signer_key_id, signer_key_path, CredentialBroker, CustodyError, CustodySourceResolver,
    DenyAllResolver, LedgerSignerCustody, LedgerSignerKey, SecretSource, SecretSourceResolver,
    SIGNER_ACTIVE_FILE, SIGNER_CHANNEL_ID, SIGNER_FILE_MODE, SIGNER_VAULT_REF,
};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-r415e-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn secrets(tag: &str) -> PathBuf {
    dir(tag).join("secrets")
}

fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, m: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(m)).unwrap();
}

/// The only `.pkcs8` file under a freshly provisioned dir.
fn sole_key_file(dir: &Path) -> PathBuf {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            n.strip_prefix("ledger-signer.")
                .and_then(|s| s.strip_suffix(".pkcs8"))
                .map(|_| dir.join(&n))
        })
        .next()
        .expect("a provisioned key file")
}

fn key_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            n.strip_prefix("ledger-signer.")
                .and_then(|s| s.strip_suffix(".pkcs8"))
                .map(|_| dir.join(&n))
        })
        .collect();
    out.sort();
    out
}

/// Pull the typed refusal out of an `open` — `Ok` is a test failure.
fn custody_err(r: Result<Option<LedgerSignerCustody>, CustodyError>) -> CustodyError {
    match r {
        Ok(_) => panic!("expected a custody refusal, got provisioned custody"),
        Err(e) => e,
    }
}

fn open_store(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

// ── provisioning + load-at-boot ─────────────────────────────────────────────

#[test]
fn provision_mints_0600_key_named_by_key_id() {
    let dir = secrets("prov");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    let files = key_files(&dir);
    assert_eq!(files.len(), 1, "one key file at provision");
    // Mode 0600, owner-only.
    assert_eq!(mode(&files[0]), SIGNER_FILE_MODE);
    // The file name is the content-derived key id — `key_id` comes back
    // out of the name (the name can never lie about the key it holds).
    let active = custody.active_key_id().to_string();
    assert!(active.starts_with("ed25519-"));
    assert_eq!(
        files[0],
        signer_key_path(&dir, &active).unwrap(),
        "the file name is the derived key id"
    );
    // The active pointer names the same key.
    let ptr = std::fs::read_to_string(dir.join(SIGNER_ACTIVE_FILE)).unwrap();
    assert_eq!(ptr.trim(), active);
}

#[test]
fn open_unprovisioned_dir_is_the_honest_none() {
    // A missing directory: no custody, no error.
    assert!(LedgerSignerCustody::open(&dir("none")).unwrap().is_none());
    // An existing-but-empty directory: the same honest posture.
    let empty = secrets("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(LedgerSignerCustody::open(&empty).unwrap().is_none());
}

#[test]
fn load_at_boot_round_trips_sign_and_verify() {
    let dir = secrets("boot");
    LedgerSignerCustody::provision(&dir).unwrap();
    // Re-open (the boot path) — the same derived table.
    let custody = LedgerSignerCustody::open(&dir).unwrap().unwrap();
    let resolver = custody.resolver();
    let active = custody.active_key_id().to_string();
    let public = resolver.verify_key(&active).expect("active resolves");
    // Build the signer from the file bytes the way `install` does after
    // `kernel_use` delivers the document.
    let doc = std::fs::read(sole_key_file(&dir)).unwrap();
    let hex: String = doc.iter().map(|b| format!("{b:02x}")).collect();
    let mut key = LedgerSignerKey::from_pkcs8_hex(&hex).unwrap();
    assert_eq!(key.key_id(), active);
    assert_eq!(key.alg_ref(), ED25519_ALG);
    // Sign → verify round trip under the public half.
    let pre = b"unsigned checkpoint preimage".as_slice();
    let sig = key.sign(pre).unwrap();
    assert_eq!(sig.len(), 64, "Ed25519 signatures are 64 bytes");
    assert_eq!(
        resolver.verify_sig(ED25519_ALG, &public, pre, &sig),
        Some(true)
    );
    // A *different* public key refuses — wrong-key verify is never a pass.
    let other_dir = secrets("other");
    let other = LedgerSignerCustody::provision(&other_dir).unwrap();
    let other_pub = other.resolver().verify_key(other.active_key_id()).unwrap();
    assert_eq!(
        resolver.verify_sig(ED25519_ALG, &other_pub, pre, &sig),
        Some(false),
        "a foreign public key must not verify the signature"
    );
    // A different preimage refuses too.
    assert_eq!(
        resolver.verify_sig(ED25519_ALG, &public, b"other preimage", &sig),
        Some(false)
    );
}

// ── the typed refusals ──────────────────────────────────────────────────────

#[test]
fn wrong_permissions_refuse_at_boot() {
    let dir = secrets("perms");
    LedgerSignerCustody::provision(&dir).unwrap();
    let f = sole_key_file(&dir);
    set_mode(&f, 0o644);
    match custody_err(LedgerSignerCustody::open(&dir)) {
        CustodyError::WrongPermissions { path, mode } => {
            assert_eq!(path, f);
            assert_eq!(mode, 0o644);
        }
        e => panic!("expected WrongPermissions, got {e:?}"),
    }
    // The mode check precedes the read — a group-readable file is untrusted.
    set_mode(&f, 0o640);
    assert!(matches!(
        LedgerSignerCustody::open(&dir),
        Err(CustodyError::WrongPermissions { mode: 0o640, .. })
    ));
}

#[test]
fn a_filename_that_lies_about_its_key_is_corrupt() {
    let dir = secrets("lie");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    let f = sole_key_file(&dir);
    // Rename under a *different* well-formed key id — the name now lies.
    let forged_id = signer_key_id(b"a different public key");
    assert_ne!(forged_id, custody.active_key_id());
    let forged = signer_key_path(&dir, &forged_id).unwrap();
    std::fs::rename(&f, &forged).unwrap();
    set_mode(&forged, SIGNER_FILE_MODE); // rename preserves mode; assert anyway
    match custody_err(LedgerSignerCustody::open(&dir)) {
        CustodyError::Corrupt { path, .. } => assert_eq!(path, forged),
        e => panic!("expected Corrupt, got {e:?}"),
    }
}

#[test]
fn unparsable_key_material_is_corrupt() {
    let dir = secrets("junk");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = dir.display();
    let bogus = dir.join("ledger-signer.ed25519-0123456789abcdef.pkcs8");
    std::fs::write(&bogus, b"not a pkcs8 document").unwrap();
    set_mode(&bogus, SIGNER_FILE_MODE);
    assert!(matches!(
        LedgerSignerCustody::open(&dir),
        Err(CustodyError::Corrupt { .. })
    ));
}

#[test]
fn two_keys_without_a_pointer_are_ambiguous() {
    let dir = secrets("amb");
    let mut custody = LedgerSignerCustody::provision(&dir).unwrap();
    custody.rotate().unwrap(); // two files, pointer names the new one
    std::fs::remove_file(dir.join(SIGNER_ACTIVE_FILE)).unwrap();
    match custody_err(LedgerSignerCustody::open(&dir)) {
        CustodyError::AmbiguousActive { files } => assert_eq!(files, 2),
        e => panic!("expected AmbiguousActive, got {e:?}"),
    }
}

#[test]
fn a_dangling_active_pointer_refuses() {
    let dir = secrets("dangle");
    let mut custody = LedgerSignerCustody::provision(&dir).unwrap();
    custody.rotate().unwrap();
    std::fs::write(dir.join(SIGNER_ACTIVE_FILE), "ed25519-ffffffffffffffff\n").unwrap();
    match custody_err(LedgerSignerCustody::open(&dir)) {
        CustodyError::DanglingActive { key_id } => {
            assert_eq!(key_id, "ed25519-ffffffffffffffff")
        }
        e => panic!("expected DanglingActive, got {e:?}"),
    }
}

#[test]
fn malformed_key_ids_have_no_path() {
    let dir = dir("ids");
    for bad in [
        "hmac-0123456789abcdef",     // wrong alg prefix — closed file set
        "ed25519-xyz",               // too short
        "ed25519-0123456789abcdef0", // too long
        "ed25519-0123456789abcdeg",  // non-hex
        "../../etc/passwd",          // traversal — the shape check refuses
    ] {
        match signer_key_path(&dir, bad) {
            Err(CustodyError::MissingKey { key_id }) => assert_eq!(key_id, bad),
            other => panic!("expected MissingKey for {bad}, got {other:?}"),
        }
    }
}

// ── rotation custody ────────────────────────────────────────────────────────

#[test]
fn rotation_moves_the_pointer_and_retires_nothing() {
    let dir = secrets("rot");
    let mut custody = LedgerSignerCustody::provision(&dir).unwrap();
    let incumbent = custody.active_key_id().to_string();
    let successor = custody.rotate().unwrap();
    assert_ne!(incumbent, successor);
    // Both files exist; both are 0600; the pointer names the successor.
    let files = key_files(&dir);
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| mode(f) == SIGNER_FILE_MODE));
    let ptr = std::fs::read_to_string(dir.join(SIGNER_ACTIVE_FILE)).unwrap();
    assert_eq!(ptr.trim(), successor);
    // The retired public key stays resolvable — historical checkpoints
    // keep verifying (OQ-170).
    let ids = custody.key_ids();
    assert!(ids.contains(&incumbent) && ids.contains(&successor));
    let resolver = custody.resolver();
    assert!(resolver.verify_key(&incumbent).is_some());
    assert!(resolver.verify_key(&successor).is_some());
    // Re-open at boot: the same active + retired table.
    let again = LedgerSignerCustody::open(&dir).unwrap().unwrap();
    assert_eq!(again.active_key_id(), successor);
    assert!(again.key_ids().contains(&incumbent));
    // The retired key's signatures still verify under the resolver.
    let doc = std::fs::read(signer_key_path(&dir, &incumbent).unwrap()).unwrap();
    let hex: String = doc.iter().map(|b| format!("{b:02x}")).collect();
    let mut old_key = LedgerSignerKey::from_pkcs8_hex(&hex).unwrap();
    let pre = b"historical claim".as_slice();
    let sig = old_key.sign(pre).unwrap();
    assert_eq!(
        resolver.verify_sig(
            ED25519_ALG,
            &resolver.verify_key(&incumbent).unwrap(),
            pre,
            &sig
        ),
        Some(true)
    );
}

// ── kernel_use provisioning ─────────────────────────────────────────────────

#[test]
fn kernel_use_provisions_the_signer_with_a_durable_row() {
    let dir = secrets("ku");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    let (mut store, run, lease) = open_store("ku-store");
    let mut broker = CredentialBroker::new(Box::new(custody.source_resolver(None)), "test-broker");
    let signer = custody
        .install(
            &mut broker,
            &mut store,
            &run,
            &lease,
            "kernel:test",
            "eff-signer",
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    assert_eq!(signer.key_id(), custody.active_key_id());
    assert_eq!(signer.alg_ref(), ED25519_ALG);
    // The durable `security.credential.used` row landed *before* the
    // value returned — §5g.6 (a)'s durable-before-visible rule.
    let used = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.credential.used")
        .count();
    assert_eq!(used, 1, "one kernel_use row");
    let row = store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| e.class == "security.credential.used")
        .unwrap();
    let dest = row
        .payload
        .get("destination")
        .and_then(hh_wire::json::Json::as_str)
        .unwrap_or("");
    assert_eq!(dest, "kernel:ledger_signing", "{row:?}");
    // The signer really signs — the self-check at build + a fresh sign.
    let mut s = signer;
    assert!(s.sign(b"preimage").is_ok());
    // Re-install is idempotent on the channel (ChannelExists is fine).
    let _signer2 = custody
        .install(
            &mut broker,
            &mut store,
            &run,
            &lease,
            "kernel:test",
            "eff-signer-2",
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
}

#[test]
fn key_material_never_enters_the_ledger() {
    let dir = secrets("noleak");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    let (mut store, run, lease) = open_store("noleak-store");
    let mut broker = CredentialBroker::new(Box::new(custody.source_resolver(None)), "test-broker");
    custody
        .install(
            &mut broker,
            &mut store,
            &run,
            &lease,
            "kernel:test",
            "eff-signer",
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
    // The PKCS#8 document (what `kernel_use` resolved) must appear in no
    // durable payload — `provided: yes/no` only, never bytes.
    let doc = std::fs::read(sole_key_file(&dir)).unwrap();
    let doc_hex: String = doc.iter().map(|b| format!("{b:02x}")).collect();
    let doc_b64ish = String::from_utf8_lossy(&doc).to_string();
    for e in store.events(&run).unwrap() {
        let text = e.payload.to_canonical_string();
        assert!(
            !text.contains(&doc_hex),
            "pkcs8 hex leaked into {} payload",
            e.class
        );
        // The first 32 private bytes would also be a leak in any encoding.
        let frag: String = doc[..32.min(doc.len())]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(
            !text.contains(&frag),
            "private-key fragment leaked into {} payload",
            e.class
        );
        let _ = doc_b64ish; // raw bytes are never valid utf-8 JSON anyway
    }
}

// ── the custody source resolver ─────────────────────────────────────────────

#[test]
fn custody_source_fails_closed_when_unprovisioned() {
    let dir = secrets("unprov");
    std::fs::create_dir_all(&dir).unwrap();
    let resolver = CustodySourceResolver::new(dir.clone(), None::<Box<dyn SecretSourceResolver>>);
    let src = SecretSource::OperatorVault {
        vault_ref: SIGNER_VAULT_REF.to_string(),
    };
    let err = resolver.resolve(&src).unwrap_err();
    assert!(err.detail.contains("no ledger-signer custody"), "{err:?}");
}

#[test]
fn custody_source_delegates_foreign_sources() {
    let dir = secrets("deleg");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    // A next-hop resolver that serves exactly one vault ref.
    struct OneVault;
    impl SecretSourceResolver for OneVault {
        fn resolve(&self, source: &SecretSource) -> Result<String, hh_secrets::SourceError> {
            match source {
                SecretSource::OperatorVault { vault_ref } if vault_ref == "vault:other" => {
                    Ok("foreign-secret".to_string())
                }
                _ => Err(hh_secrets::SourceError {
                    kind: source.kind(),
                    detail: "denied".into(),
                }),
            }
        }
    }
    let resolver = custody.source_resolver(Some(Box::new(OneVault)));
    // The custody coordinate answers the active key's document.
    let doc = resolver
        .resolve(&SecretSource::OperatorVault {
            vault_ref: SIGNER_VAULT_REF.to_string(),
        })
        .unwrap();
    assert!(!doc.is_empty() && doc.chars().all(|c| c.is_ascii_hexdigit()));
    // A foreign coordinate delegates.
    assert_eq!(
        resolver
            .resolve(&SecretSource::OperatorVault {
                vault_ref: "vault:other".to_string(),
            })
            .unwrap(),
        "foreign-secret"
    );
    // With no next resolver a foreign coordinate fails closed — exactly
    // like DenyAllResolver.
    let resolver = custody.source_resolver(None::<Box<dyn SecretSourceResolver>>);
    let err = resolver
        .resolve(&SecretSource::OperatorVault {
            vault_ref: "vault:nowhere".to_string(),
        })
        .unwrap_err();
    assert!(!err.detail.is_empty());
    let deny = DenyAllResolver;
    assert!(deny
        .resolve(&SecretSource::OperatorVault {
            vault_ref: "vault:nowhere".to_string(),
        })
        .is_err());
}

#[test]
fn the_channel_spec_is_kernel_only_and_unbindable() {
    let spec = LedgerSignerCustody::channel_spec();
    assert_eq!(
        spec.access_class,
        hh_secrets::AccessClass::KernelOnly,
        "the signer channel is kernel-only"
    );
    assert!(!spec.bindable, "a signing key never mints a binding");
    assert!(spec.destinations.is_empty());
    assert!(spec.allowed_env_names.is_none(), "never an env projection");
    assert_eq!(spec.kind, hh_secrets::CredentialKind::SigningKey);
    // The broker admits it under its canonical id.
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "test-broker");
    broker
        .register_channel(
            SIGNER_CHANNEL_ID,
            spec,
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();
}

#[test]
fn public_resolver_table_carries_public_bytes_only() {
    let dir = secrets("pubtab");
    let custody = LedgerSignerCustody::provision(&dir).unwrap();
    for bytes in custody.public_keys().values() {
        // Ed25519 public keys are 32 bytes — never a PKCS#8 document
        // (which is ~85 bytes and carries the private half).
        assert_eq!(bytes.len(), 32, "only the public half crosses the seam");
    }
    let mut table = BTreeMap::new();
    table.insert("k".to_string(), b"pub".to_vec());
    let _ = table; // the map shape is `key_id → public bytes`
}
