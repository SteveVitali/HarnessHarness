//! The blob-pool encryption seam — per-event AEAD at rest (§5g.6 C2 "blob
//! encryption with per-event keys"; R-2.8.6's last DF-S1.15-3 cell; ADR-0345
//! revisit trigger (a) fired and answered; ADR-0361).
//!
//! # Custody never crosses into `hharness-ledger`
//!
//! Same posture as the checkpoint-signer seam (`AuditSigner`/`AuditKeyResolver`
//! — ADR-0067 (e); ADR-0331 D3; OQ-170): the embed layer resolves key material
//! through `CredentialBroker::kernel_use` and wraps it in a [`BlobCipher`]; the
//! store sees `seal`/`open` plus the content-free coordinates (`algorithm`,
//! `key_id`) only. No key bytes ever enter ledger state — the sealed frame
//! header carries the *coordinate* (`key_id`), never material.
//!
//! # The sealed frame (`HHBE` v1)
//!
//! The blob pool is content-addressed: the file is `blobs/<digest>` where the
//! digest covers the **plaintext** (§5g.6 I-A8 — "hashes are over plaintext
//! bytes at `put_blob`"). When a cipher is installed the stored bytes are a
//! self-describing frame instead:
//!
//! ```text
//! "HHBE" ‖ ver:u8 (=1) ‖ alg_len:u8 ‖ alg ‖ key_id_len:u8 ‖ key_id ‖
//! nonce_len:u8 ‖ nonce ‖ ciphertext ‖ tag
//! ```
//!
//! The header is readable without key material so `blob_status`/`audit_view`
//! can report sealed residency and the bound `key_id` honestly. The codec
//! lives here — the one spelling (CC1); the AEAD construction is the
//! `BlobCipher` impl's own business (the workspace's `ring`-backed
//! construction is `hharness_secrets::aead::RingBlobCipher`, the only impl the
//! ADR-0359 scope admits).
//!
//! # Failure classes (§5g.6 I-A8 — binding)
//!
//! * a sealed frame whose header cannot parse is `BlobCorrupt` (structural
//!   corruption — the same class an undecodable `HHZ1` frame reports);
//! * a well-formed frame that cannot be opened — no cipher installed, a
//!   foreign `key_id`, wrong key bytes, or an AEAD tag failure — is
//!   `Missing{reason: key_unavailable}`, **never `Tampered`** (ADR-0068 R3);
//! * the plaintext the open yields still has to hash back to the address —
//!   a mismatch is `BlobCorrupt` (defense in depth; AEAD already binds it).

use hharness_identity::idp::ContentAddress;

/// The sealed-frame magic (`"HHBE"` — HarnessHarness Blob Encrypted).
pub const SEALED_MAGIC: &[u8; 4] = b"HHBE";

/// The only frame version this build reads/writes.
pub const SEALED_VERSION: u8 = 1;

/// Is the stored byte string a sealed frame? (magic prefix — same
/// detect-by-tag posture as the `HHZ1` residency frame.)
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(SEALED_MAGIC)
}

/// The parsed frame header — the members readable without key material
/// (`blob_status` reports `Sealed{key_id}` from this alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealHeader {
    /// The construction id the cipher reports (`algorithm()`).
    pub alg: String,
    /// The custody coordinate the frame was sealed under — never key bytes.
    pub key_id: String,
    /// The nonce the frame was sealed with (opaque to the pool).
    pub nonce: Vec<u8>,
}

/// `header_bytes(header) → the encoded frame head` — magic ‖ version ‖
/// `alg` ‖ `key_id` ‖ `nonce` (each length-prefixed, ≤ 255 bytes). The
/// `BlobCipher` impl binds this byte string as the AEAD's associated data
/// so a header-tampered frame refuses at `open` — the custody coordinate
/// an attacker flips is as authenticated as the body. Returns `None` on an
/// out-of-bounds member, never a truncated head.
pub fn header_bytes(header: &SealHeader) -> Option<Vec<u8>> {
    for len in [header.alg.len(), header.key_id.len(), header.nonce.len()] {
        if len > u8::MAX as usize || len == 0 {
            return None;
        }
    }
    let mut out = Vec::with_capacity(
        4 + 1 + 1 + header.alg.len() + 1 + header.key_id.len() + 1 + header.nonce.len(),
    );
    out.extend_from_slice(SEALED_MAGIC);
    out.push(SEALED_VERSION);
    out.push(header.alg.len() as u8);
    out.extend_from_slice(header.alg.as_bytes());
    out.push(header.key_id.len() as u8);
    out.extend_from_slice(header.key_id.as_bytes());
    out.push(header.nonce.len() as u8);
    out.extend_from_slice(&header.nonce);
    Some(out)
}

/// `encode_frame(header, ciphertext) → the stored bytes`. Length fields are
/// bounded (`alg`/`key_id`/`nonce` ≤ 255 bytes each); an oversized member is
/// an `Err`, never a truncated frame.
pub fn encode_frame(header: &SealHeader, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let mut out =
        header_bytes(header).ok_or_else(|| "seal frame member len out of bounds".to_string())?;
    if ciphertext.is_empty() {
        return Err("seal frame carries an empty ciphertext".into());
    }
    out.extend_from_slice(ciphertext);
    Ok(out)
}

/// `parse_frame(bytes) → (header, ciphertext)` — `None` when the bytes are
/// not a well-formed `HHBE` v1 frame (the caller maps that to `BlobCorrupt`,
/// never serves them as plaintext).
pub fn parse_frame(bytes: &[u8]) -> Option<(SealHeader, &[u8])> {
    if !is_sealed(bytes) {
        return None;
    }
    let mut cur = 4usize; // past the magic
    let ver = *bytes.get(cur)?;
    cur += 1;
    if ver != SEALED_VERSION {
        return None;
    }
    let take = |cur: &mut usize| -> Option<&[u8]> {
        let len = *bytes.get(*cur)? as usize;
        *cur += 1;
        let s = bytes.get(*cur..*cur + len)?;
        *cur += len;
        Some(s)
    };
    let alg = String::from_utf8(take(&mut cur)?.to_vec()).ok()?;
    let key_id = String::from_utf8(take(&mut cur)?.to_vec()).ok()?;
    let nonce = take(&mut cur)?.to_vec();
    let ciphertext = bytes.get(cur..)?;
    if ciphertext.is_empty() {
        return None;
    }
    Some((SealHeader { alg, key_id, nonce }, ciphertext))
}

/// `frame_header(bytes)` — the header half of [`parse_frame`] for the
/// custody-free status reads (`blob_status`, `audit_view`'s residency
/// fold). `None` on a malformed frame.
pub fn frame_header(bytes: &[u8]) -> Option<SealHeader> {
    parse_frame(bytes).map(|(h, _)| h)
}

/// The blob-pool cipher seam — `seal`/`open` over the plaintext content
/// address. Custody stays outside the ledger exactly like the signer seam:
/// the boundary resolves key material (a `kernel_use` channel, R-2.8.3) and
/// wraps it; `hharness-ledger` sees only this surface (ADR-0361 D1).
///
/// `address` is the `idp/1` content address **of the plaintext** — the
/// construction binds every frame to it (wrong-address or foreign-key frames
/// refuse at `open`, never silently decode).
pub trait BlobCipher {
    /// The construction id — stamped in every frame header and surfaced by
    /// `audit_view`'s `encryption` member (e.g. `"aes-256-gcm.hh-blob.v1"`).
    fn algorithm(&self) -> &str;
    /// The custody coordinate the frames bind to — a `key_id`, never bytes.
    fn key_id(&self) -> &str;
    /// `seal(address, plaintext) → the complete stored frame`. The plaintext
    /// content address is computed by the pool before sealing (hashes cover
    /// plaintext — §5g.6 I-A8).
    fn seal(&self, address: &ContentAddress, plaintext: &[u8]) -> Result<Vec<u8>, String>;
    /// `open(address, frame) → plaintext`. Every `Err` is a *could-not-open*
    /// class — key not held, foreign `key_id`, wrong bytes, or tag failure —
    /// which the pool reports `Missing{key_unavailable}`, never `Tampered`
    /// (ADR-0068 R3; §5g.6 I-A8).
    fn open(&self, address: &ContentAddress, frame: &[u8]) -> Result<Vec<u8>, String>;
}
