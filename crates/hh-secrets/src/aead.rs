//! `aead` — the workspace's one blob-pool AEAD construction (R3.8;
//! §5g.6 C2 "blob encryption with per-event keys"; R-2.8.6's DF-S1.15-3
//! cell; ADR-0359's reserved `ring::aead`/`ring::rand` scope consumed;
//! ADR-0361).
//!
//! # The one construction (CC1)
//!
//! `RingBlobCipher` implements [`hharness_ledger::cipher::BlobCipher`] over
//! `ring::aead::AES_256_GCM` — the sealed `HHBE` frame `hharness-ledger`'s blob
//! pool stores and reads. There is no second AEAD spelling anywhere in
//! the workspace.
//!
//! # Per-event keys (§5g.6 C2)
//!
//! The cipher holds one 32-byte **master key** resolved through the
//! custody seam (the embed layer's `CredentialBroker::kernel_use` answer —
//! the broker mediates, never the ledger). Each blob seals under a
//! *derived* per-blob key, the HKDF-expand-shaped single-block expand over
//! the workspace's admitted HMAC (CC1 — `hharness_wire::sha256::hmac_sha256`,
//! no second KDF, no `ring::hkdf` — outside the ADR-0359 scope):
//!
//! ```text
//! blob_key = HMAC-SHA256(master, "hh-ledger/blob/v1" ␟ <address>)
//! ```
//!
//! where `<address>` is the blob's `idp/1` content address *of the
//! plaintext* (hashes cover plaintext — §5g.6 I-A8). One key per address:
//! a leaked or rotated coordinate never mixes frames, and `open` bound to
//! a different address refuses.
//!
//! # The frame's AEAD binding
//!
//! `seal` draws a fresh 96-bit nonce from `ring::rand::SystemRandom`
//! (ADR-0359's reserved nonce-generation scope) and seals with the encoded
//! `HHBE` header as the AEAD's associated data — `alg`, `key_id` and
//! `nonce` are as authenticated as the body, so a header tamper refuses at
//! `open` exactly like a body tamper.
//!
//! # Honesty bounds
//!
//! - **Fixture-verified, never custody.** The master key reaching this
//!   type in the build's batteries is a fixture construction —
//!   `n/a{custody}` (OQ-170: real key custody is external by design; a
//!   KMS/HSM/issuer leg would arrive through the same `BlobCipher` seam
//!   without touching this module).
//! - **Failure is typed.** Every `open` `Err` is the *could-not-open*
//!   class the pool reports `Missing{key_unavailable}` — never
//!   `Tampered` (ADR-0068 R3; §5g.6 I-A8). A `seal` `Err` propagates as
//!   `CipherFault`, never a silent plaintext write.
//! - **No `unsafe`, no new dep.** `ring` stays inside this crate under
//!   ADR-0359's pin; this module is the R3.8-reserved surface.

use hharness_identity::idp::ContentAddress;
use hharness_ledger::cipher::{self, BlobCipher, SealHeader};
use hharness_wire::sha256::hmac_sha256;
use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};

/// The construction id — stamped in every frame header, declared in the
/// manifest's `blob_encryption{alg}`, and surfaced by `audit_view`'s
/// `encryption.installed.alg`.
pub const BLOB_AEAD_ALG: &str = "aes-256-gcm.hh-blob.v1";

/// The per-blob key-derivation info prefix (the HKDF-expand `info`'s
/// domain tag — ␟ is U+001F, the repo's own separator convention).
const KDF_INFO_PREFIX: &[u8] = b"hh-ledger/blob/v1\x1f";

/// The nonce length — AES-256-GCM's 96-bit standard.
const NONCE_LEN: usize = 12;

/// `RingBlobCipher{key_id, master_key}` — the [`BlobCipher`] impl the pool
/// installs via `Store::set_blob_cipher`. `key_id` is the custody
/// coordinate the frame headers bind to (never key bytes); `master_key`
/// is the resolved material — held only in this value, never persisted by
/// the ledger and never written to build memory.
pub struct RingBlobCipher {
    /// The custody coordinate (`key_id`, never material).
    key_id: String,
    /// The resolved master key — 32 bytes of uniform key material from the
    /// broker's `kernel_use` answer (fixture-derived in the build's tests;
    /// `n/a{custody}` per OQ-170).
    master_key: [u8; 32],
    /// The nonce source.
    rng: SystemRandom,
}

impl RingBlobCipher {
    /// `new(key_id, master_key)` — the custody seam's product: the embed
    /// layer resolves `master_key` (a broker `kernel_use` answer in live
    /// posture; an explicit fixture in the batteries) and names its
    /// coordinate `key_id`. `key_id` must be non-empty — the frame header
    /// binds it.
    pub fn new(key_id: impl Into<String>, master_key: [u8; 32]) -> Result<Self, String> {
        let key_id = key_id.into();
        if key_id.is_empty() || key_id.len() > u8::MAX as usize {
            return Err("blob cipher key_id out of bounds".into());
        }
        Ok(Self {
            key_id,
            master_key,
            rng: SystemRandom::new(),
        })
    }

    /// The per-blob AEAD key — HKDF-expand-shaped over the admitted HMAC:
    /// `HMAC-SHA256(master, KDF_INFO_PREFIX ‖ address)` (the address is the
    /// content address of the *plaintext*). One key per address — the
    /// per-event keying §5g.6's C2 row names.
    fn blob_key(&self, address: &ContentAddress) -> aead::LessSafeKey {
        let mut info = Vec::with_capacity(KDF_INFO_PREFIX.len() + 64);
        info.extend_from_slice(KDF_INFO_PREFIX);
        info.extend_from_slice(address.algorithm.as_bytes());
        info.push(0x1f);
        info.extend_from_slice(address.digest.as_bytes());
        let key_bytes = hmac_sha256(&self.master_key, &info);
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
            .expect("aes-256-gcm accepts a 32-byte key");
        aead::LessSafeKey::new(unbound)
    }

    /// The AEAD's associated data — the encoded `HHBE` header, so `alg`,
    /// `key_id` and `nonce` are authenticated with the body.
    fn aad(header: &SealHeader) -> Result<Vec<u8>, String> {
        cipher::header_bytes(header).ok_or_else(|| "seal header out of bounds".to_string())
    }
}

impl BlobCipher for RingBlobCipher {
    fn algorithm(&self) -> &str {
        BLOB_AEAD_ALG
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn seal(&self, address: &ContentAddress, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut nonce = [0u8; NONCE_LEN];
        self.rng
            .fill(&mut nonce)
            .map_err(|_| "nonce draw failed".to_string())?;
        let header = SealHeader {
            alg: BLOB_AEAD_ALG.to_string(),
            key_id: self.key_id.clone(),
            nonce: nonce.to_vec(),
        };
        let aad = Self::aad(&header)?;
        let key = self.blob_key(address);
        let mut body = plaintext.to_vec();
        let nonce = aead::Nonce::try_assume_unique_for_key(&nonce)
            .map_err(|_| "nonce not 96 bits".to_string())?;
        key.seal_in_place_append_tag(nonce, aead::Aad::from(aad.as_slice()), &mut body)
            .map_err(|_| "aead seal failed".to_string())?;
        cipher::encode_frame(&header, &body)
    }

    fn open(&self, address: &ContentAddress, frame: &[u8]) -> Result<Vec<u8>, String> {
        let (header, ciphertext) = cipher::parse_frame(frame)
            .ok_or_else(|| "frame not well-formed HHBE v1".to_string())?;
        if header.alg != BLOB_AEAD_ALG {
            return Err(format!("foreign alg {}", header.alg));
        }
        if header.key_id != self.key_id {
            return Err(format!("foreign key_id {}", header.key_id));
        }
        let aad = Self::aad(&header)?;
        let key = self.blob_key(address);
        let mut body = ciphertext.to_vec();
        let nonce = aead::Nonce::try_assume_unique_for_key(&header.nonce)
            .map_err(|_| "nonce not 96 bits".to_string())?;
        let opened = key
            .open_in_place(nonce, aead::Aad::from(aad.as_slice()), &mut body)
            .map_err(|_| "aead open failed".to_string())?;
        Ok(opened.to_vec())
    }
}
