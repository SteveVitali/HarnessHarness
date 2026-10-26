//! Kernel-held Ed25519 audit-signer custody (R4.15e; OQ-170; ADR-0376).
//!
//! The adopted GATE-G7 ruling (the BL-30 OQ-170 brief): **the kernel holds
//! the ledger-signing key**; `hh-secrets` is the custody home — the only
//! crate admitted to `ring` (ADR-0359 D2, extended by ADR-0376 to
//! production Ed25519 signing + key generation/persistence). `hh-ledger`
//! sees the landed `AuditSigner`/`AuditKeyResolver` seam only: `key_id` +
//! `sign(preimage)` + public verify bytes — key material never crosses.
//!
//! # The custody layout (`<HH_STORE_ROOT>/secrets/`)
//!
//! - `ledger-signer.<key_id>.pkcs8` — a PKCS#8 v2 Ed25519 document, mode
//!   `0600`, owner-only. `key_id` is `ed25519-<hex(sha256(public_key))[..16]>`
//!   — content-derived, so the file name can never lie about which key it
//!   holds (a parse whose `key_id` does not match is `Corrupt`).
//! - `ledger-signer.active` — one line, the active `key_id`. Rotation
//!   appends a new `.pkcs8` file and moves the pointer; retired files stay
//!   so historical checkpoints keep verifying (public bytes only cross the
//!   resolver seam).
//!
//! # The honest postures (never a fabricated signer)
//!
//! - **Unprovisioned** — `LedgerSignerCustody::open` answers `Ok(None)`
//!   when no key files exist: `signer_key_ids` stays empty and
//!   `audit_view` keeps reporting `n/a{no signer_key_ids declared}`.
//! - **Missing / wrong permissions / corrupt** — `Err(CustodyError)`, a
//!   typed refusal at boot: a `0600` file it is or the kernel refuses, no
//!   silent placeholder, no ambiguous half-loaded set.
//! - **Provisioning** — `install` declares the `kernel_only`
//!   `CredentialKind::SigningKey` channel and resolves through
//!   `CredentialBroker::kernel_use(KernelPurpose::LedgerSigning)`: the
//!   `security.credential.used{destination = kernel:ledger_signing}` row is
//!   durable before the value returns (§5g.6 (a)). The delivered value is
//!   the PKCS#8 document's hex — the `AuditSigner` is built from *that*,
//!   never a side channel.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hh_ledger::audit::{AuditKeyResolver, AuditSigner, CHECKPOINT_ALG, ED25519_ALG};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::sha256::{hmac_sha256, sha256_hex};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};

use crate::broker::{CredentialBroker, SecretSourceResolver, SourceError};
use crate::channel::{CredentialKind, SecretChannelSpec, SecretSource, SenderConstraint};
use crate::errors::Refused;
use crate::types::{AccessClass, KernelPurpose};

/// The custody file-name prefix — `ledger-signer.<key_id>.pkcs8`.
pub const SIGNER_FILE_PREFIX: &str = "ledger-signer.";
/// The active-key pointer file — one line, the active `key_id`.
pub const SIGNER_ACTIVE_FILE: &str = "ledger-signer.active";
/// The broker channel id — `kernel_only`, `bindable: false` (`kernel_use`
/// is its only path, §5g.3 §2).
pub const SIGNER_CHANNEL_ID: &str = "ledger-signer";
/// The `SecretSource::OperatorVault` coordinate the channel resolves —
/// the custody directory's active key.
pub const SIGNER_VAULT_REF: &str = "ledger-signer";
/// The file mode every key file must carry — owner-only, `0600`.
pub const SIGNER_FILE_MODE: u32 = 0o600;
/// The boot self-check's signed canary — a fixed string, not a secret.
const SELF_CHECK_CANARY: &[u8] = b"hh-secrets ledger-signer self-check v1";

/// A custody-layer typed refusal (OQ-170: missing, mis-permissioned or
/// corrupt key material refuses — never a placeholder, never a fabricated
/// signature). Content-free details: paths and key ids only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustodyError {
    /// `kernel_use`/`activate` named a `key_id` with no file under the
    /// custody dir.
    MissingKey {
        /// The named key id.
        key_id: String,
    },
    /// A key file's mode is not `0600` — the file is untrusted.
    WrongPermissions {
        /// The offending file.
        path: PathBuf,
        /// The observed `mode & 0o777`.
        mode: u32,
    },
    /// A key file does not parse, or its `key_id` does not derive from the
    /// public key it carries (the name would lie).
    Corrupt {
        /// The offending file.
        path: PathBuf,
        /// The content-free detail.
        detail: String,
    },
    /// Multiple key files exist but no `ledger-signer.active` pointer
    /// names the live one — an ambiguous set is never guessed at.
    AmbiguousActive {
        /// How many key files exist.
        files: usize,
    },
    /// The active pointer names a `key_id` with no file.
    DanglingActive {
        /// The named key id.
        key_id: String,
    },
    /// The boot self-check failed: the loaded pair does not sign/verify
    /// under its derived `key_id`.
    SelfCheck {
        /// The key id that failed.
        key_id: String,
    },
    /// A filesystem operation failed (`read`/`write`/`create_dir_all`).
    Io {
        /// The path the operation named.
        path: PathBuf,
        /// The content-free error string.
        detail: String,
    },
    /// The broker refused the provisioning (`Refused` from `kernel_use` or
    /// `register_channel`) — the durable-row details ride the ledger.
    Provision {
        /// The content-free detail.
        detail: String,
    },
}

impl std::fmt::Display for CustodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CustodyError::MissingKey { key_id } => {
                write!(f, "signer key {key_id} has no custody file")
            }
            CustodyError::WrongPermissions { path, mode } => write!(
                f,
                "signer key file {} has mode {mode:03o}, required 600",
                path.display()
            ),
            CustodyError::Corrupt { path, detail } => {
                write!(f, "signer key file {} is corrupt: {detail}", path.display())
            }
            CustodyError::AmbiguousActive { files } => write!(
                f,
                "{files} signer key files exist and no ledger-signer.active names the live one"
            ),
            CustodyError::DanglingActive { key_id } => {
                write!(
                    f,
                    "ledger-signer.active names {key_id} but no such key file exists"
                )
            }
            CustodyError::SelfCheck { key_id } => {
                write!(f, "signer key {key_id} failed the boot self-check")
            }
            CustodyError::Io { path, detail } => {
                write!(f, "custody io at {}: {detail}", path.display())
            }
            CustodyError::Provision { detail } => {
                write!(f, "signer provisioning refused: {detail}")
            }
        }
    }
}

impl std::error::Error for CustodyError {}

/// The content-derived key id — `ed25519-<hex(sha256(public_key))[..16]>`:
/// one deterministic spelling a file name, a manifest `signer_key_ids`
/// member and a `signatures[].key_id` entry all share (CC1).
pub fn signer_key_id(public_key: &[u8]) -> String {
    format!("ed25519-{}", &sha256_hex(public_key)[..16])
}

/// The `ledger-signer.<key_id>.pkcs8` path for `key_id` — the name is
/// `key_id`-derived only; an id outside the `ed25519-[0-9a-f]{16}` shape
/// is refused (the file set is closed).
pub fn signer_key_path(dir: &Path, key_id: &str) -> Result<PathBuf, CustodyError> {
    let tail = key_id
        .strip_prefix("ed25519-")
        .ok_or(CustodyError::MissingKey {
            key_id: key_id.to_string(),
        })?;
    if tail.len() != 16 || !tail.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CustodyError::MissingKey {
            key_id: key_id.to_string(),
        });
    }
    Ok(dir.join(format!("{SIGNER_FILE_PREFIX}{key_id}.pkcs8")))
}

/// The `0600`-or-refuse check (OQ-170 — wrong permissions are a typed
/// refusal, never a warning). Unix-only surface; every build target in the
/// workspace is unix-flavoured.
fn check_permissions(path: &Path) -> Result<(), CustodyError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| CustodyError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let mode = meta.permissions().mode() & 0o777;
    if mode != SIGNER_FILE_MODE {
        return Err(CustodyError::WrongPermissions {
            path: path.to_path_buf(),
            mode,
        });
    }
    Ok(())
}

/// Read a key file's PKCS#8 bytes — mode-checked first (the check precedes
/// the read so a mis-permissioned file is never consumed).
fn read_key_file(path: &Path) -> Result<Vec<u8>, CustodyError> {
    check_permissions(path)?;
    std::fs::read(path).map_err(|e| CustodyError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

/// Write `bytes` to `path` at mode `0600`, `create_new` — a pre-existing
/// file is a refusal (rotation mints fresh ids; overwriting custody is
/// never silent).
fn write_key_file(path: &Path, bytes: &[u8]) -> Result<(), CustodyError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(SIGNER_FILE_MODE)
        .open(path)
        .map_err(|e| CustodyError::Io {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
    use std::io::Write;
    f.write_all(bytes).map_err(|e| CustodyError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    check_permissions(path)
}

/// The lowercase hex encoding used to carry the PKCS#8 document through
/// `kernel_use`'s `String` value — the same hex convention the sig codec
/// and fingerprints use (CC1).
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode(s: &str) -> Result<Vec<u8>, CustodyError> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return Err(CustodyError::Corrupt {
            path: PathBuf::new(),
            detail: "pkcs8 hex is odd-length".into(),
        });
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    for i in (0..b.len()).step_by(2) {
        let hi = (b[i] as char).to_digit(16);
        let lo = (b[i + 1] as char).to_digit(16);
        match (hi, lo) {
            (Some(hi), Some(lo)) => out.push(((hi << 4) | lo) as u8),
            _ => {
                return Err(CustodyError::Corrupt {
                    path: PathBuf::new(),
                    detail: "pkcs8 hex is malformed".into(),
                })
            }
        }
    }
    Ok(out)
}

/// The loaded signer — a `key_id` + the in-memory `Ed25519KeyPair`. The
/// private material lives only here, kernel-side; the public half crosses
/// the resolver seam as bytes.
pub struct LedgerSignerKey {
    key_id: String,
    pair: Ed25519KeyPair,
    public_key: Vec<u8>,
}

impl LedgerSignerKey {
    /// Build a signer from the `kernel_use`-delivered PKCS#8 document hex.
    /// Parses, derives the `key_id` from the public half, then self-checks:
    /// sign the canary, verify it under the derived public key — a pair
    /// that cannot sign-and-verify is `SelfCheck`, never installed.
    pub fn from_pkcs8_hex(pkcs8_hex: &str) -> Result<Self, CustodyError> {
        let doc = hex_decode(pkcs8_hex)?;
        Self::from_pkcs8(&doc)
    }

    /// Build a signer from raw PKCS#8 bytes (custody-internal path — the
    /// file scan re-derives ids for the public table).
    pub fn from_pkcs8(doc: &[u8]) -> Result<Self, CustodyError> {
        let pair = Ed25519KeyPair::from_pkcs8(doc).map_err(|_| CustodyError::Corrupt {
            path: PathBuf::new(),
            detail: "pkcs8 does not parse as an Ed25519 document".into(),
        })?;
        let public_key = pair.public_key().as_ref().to_vec();
        let key_id = signer_key_id(&public_key);
        let key = Self {
            key_id,
            pair,
            public_key,
        };
        // Sign-on-boot verification: the loaded pair must sign the canary
        // and the derived public key must verify it.
        let sig = key.pair.sign(SELF_CHECK_CANARY).as_ref().to_vec();
        UnparsedPublicKey::new(&ED25519, &key.public_key)
            .verify(SELF_CHECK_CANARY, &sig)
            .map_err(|_| CustodyError::SelfCheck {
                key_id: key.key_id.clone(),
            })?;
        Ok(key)
    }

    /// The public half — the only bytes the resolver ever serves.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }
}

impl AuditSigner for LedgerSignerKey {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn alg_ref(&self) -> &'static str {
        ED25519_ALG
    }

    fn sign(&mut self, preimage: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.pair.sign(preimage).as_ref().to_vec())
    }
}

/// The resolver over the custody's public half — `key_id → public bytes`
/// for every key file (current *and* retired — historical checkpoints keep
/// verifying), `verify_sig` dispatching the two admitted `alg_ref`
/// spellings (Ed25519 through `ring`; `hmac-sha256` keeps the C0 fixture
/// construction verifiable — the bytes are the HMAC key in that arm).
pub struct LedgerSignerResolver {
    /// `key_id → public key bytes`.
    public_keys: BTreeMap<String, Vec<u8>>,
}

impl LedgerSignerResolver {
    /// The table over explicit `(key_id, public_key)` pairs.
    pub fn new(public_keys: BTreeMap<String, Vec<u8>>) -> Self {
        Self { public_keys }
    }

    /// The declared key ids (current + retired).
    pub fn key_ids(&self) -> BTreeSet<String> {
        self.public_keys.keys().cloned().collect()
    }
}

impl AuditKeyResolver for LedgerSignerResolver {
    fn verify_key(&self, key_id: &str) -> Option<Vec<u8>> {
        self.public_keys.get(key_id).cloned()
    }

    fn verify_sig(&self, alg_ref: &str, key: &[u8], preimage: &[u8], sig: &[u8]) -> Option<bool> {
        match alg_ref {
            ED25519_ALG => Some(
                UnparsedPublicKey::new(&ED25519, key)
                    .verify(preimage, sig)
                    .is_ok(),
            ),
            CHECKPOINT_ALG => Some(hmac_sha256(key, preimage).to_vec() == sig),
            _ => None,
        }
    }
}

/// The file-backed custody table — the `secrets/` directory scan: every
/// `ledger-signer.<key_id>.pkcs8` file (mode-checked, parsed, `key_id`
/// re-derived from its own public key) plus the `ledger-signer.active`
/// pointer. Key bytes stay on disk until `kernel_use` provisions them; the
/// table holds **public keys only**.
pub struct LedgerSignerCustody {
    dir: PathBuf,
    /// `key_id → public key` — current and retired alike.
    public_keys: BTreeMap<String, Vec<u8>>,
    /// The `ledger-signer.active` pointer's key id.
    active: String,
}

impl LedgerSignerCustody {
    /// Scan `dir` — `Ok(None)` is the unprovisioned posture (no key files;
    /// the run stays honestly unsigned). Any file that exists is
    /// mode-checked (`0600` or `WrongPermissions`) and parsed (a `key_id`
    /// that does not derive from the file's own public key is `Corrupt` —
    /// the name must never lie). The active pointer is mandatory when two
    /// or more keys exist (`AmbiguousActive`) and must name a live file
    /// (`DanglingActive`).
    pub fn open(dir: &Path) -> Result<Option<Self>, CustodyError> {
        if !dir.is_dir() {
            return Ok(None);
        }
        let mut public_keys = BTreeMap::new();
        let rd = std::fs::read_dir(dir).map_err(|e| CustodyError::Io {
            path: dir.to_path_buf(),
            detail: e.to_string(),
        })?;
        for entry in rd {
            let entry = entry.map_err(|e| CustodyError::Io {
                path: dir.to_path_buf(),
                detail: e.to_string(),
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(stem) = name
                .strip_prefix(SIGNER_FILE_PREFIX)
                .and_then(|s| s.strip_suffix(".pkcs8"))
            else {
                continue;
            };
            // `ledger-signer.<key_id>.pkcs8` — the stem IS the key id.
            let named_id = stem.to_string();
            let bytes = read_key_file(&entry.path())?;
            let key = LedgerSignerKey::from_pkcs8(&bytes).map_err(|e| match e {
                CustodyError::Corrupt { detail, .. } => CustodyError::Corrupt {
                    path: entry.path(),
                    detail,
                },
                CustodyError::SelfCheck { key_id } => CustodyError::Corrupt {
                    path: entry.path(),
                    detail: format!("self-check failed under derived id {key_id}"),
                },
                other => other,
            })?;
            if key.key_id != named_id {
                return Err(CustodyError::Corrupt {
                    path: entry.path(),
                    detail: format!(
                        "file name says {named_id} but the public key derives {}",
                        key.key_id
                    ),
                });
            }
            public_keys.insert(key.key_id.clone(), key.public_key.clone());
        }
        if public_keys.is_empty() {
            return Ok(None);
        }
        let active = if public_keys.len() == 1 {
            // One file, no pointer required — the only possible live key.
            let ptr = dir.join(SIGNER_ACTIVE_FILE);
            if let Ok(line) = std::fs::read_to_string(&ptr) {
                line.trim().to_string()
            } else {
                public_keys.keys().next().unwrap().clone()
            }
        } else {
            let ptr = dir.join(SIGNER_ACTIVE_FILE);
            let line =
                std::fs::read_to_string(&ptr).map_err(|_| CustodyError::AmbiguousActive {
                    files: public_keys.len(),
                })?;
            line.trim().to_string()
        };
        if !public_keys.contains_key(&active) {
            return Err(CustodyError::DanglingActive { key_id: active });
        }
        Ok(Some(Self {
            dir: dir.to_path_buf(),
            public_keys,
            active,
        }))
    }

    /// Mint a fresh installation: `Ed25519KeyPair::generate_pkcs8` under
    /// `SystemRandom`, the `0600` file, the active pointer — the
    /// per-installation provisioning the adopted ruling names (an
    /// operator-placed file works identically: `open` accepts it on the
    /// same checks).
    pub fn provision(dir: &Path) -> Result<Self, CustodyError> {
        std::fs::create_dir_all(dir).map_err(|e| CustodyError::Io {
            path: dir.to_path_buf(),
            detail: e.to_string(),
        })?;
        let doc =
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| CustodyError::Io {
                path: dir.to_path_buf(),
                detail: "key generation failed".into(),
            })?;
        let key = LedgerSignerKey::from_pkcs8(doc.as_ref())?;
        let path = signer_key_path(dir, &key.key_id)?;
        write_key_file(&path, doc.as_ref())?;
        std::fs::write(dir.join(SIGNER_ACTIVE_FILE), format!("{}\n", key.key_id)).map_err(|e| {
            CustodyError::Io {
                path: dir.join(SIGNER_ACTIVE_FILE),
                detail: e.to_string(),
            }
        })?;
        Self::open(dir)?.ok_or_else(|| CustodyError::Io {
            path: dir.to_path_buf(),
            detail: "provisioned key did not reopen".into(),
        })
    }

    /// Rotate custody: mint the successor `0600` file, move the active
    /// pointer, return the successor's `key_id` (the retired file stays —
    /// the resolver still serves its public key for historical claims).
    /// The caller co-signs the `key_rotation` checkpoint through
    /// `Store::signer_rotation_checkpoint` (incumbent + successor) — the
    /// durable rotation record (CC3).
    pub fn rotate(&mut self) -> Result<String, CustodyError> {
        let doc =
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| CustodyError::Io {
                path: self.dir.clone(),
                detail: "key generation failed".into(),
            })?;
        let key = LedgerSignerKey::from_pkcs8(doc.as_ref())?;
        if self.public_keys.contains_key(&key.key_id) {
            return Err(CustodyError::Io {
                path: self.dir.clone(),
                detail: "generated key id collides with an existing file".into(),
            });
        }
        let path = signer_key_path(&self.dir, &key.key_id)?;
        write_key_file(&path, doc.as_ref())?;
        std::fs::write(
            self.dir.join(SIGNER_ACTIVE_FILE),
            format!("{}\n", key.key_id),
        )
        .map_err(|e| CustodyError::Io {
            path: self.dir.join(SIGNER_ACTIVE_FILE),
            detail: e.to_string(),
        })?;
        self.public_keys
            .insert(key.key_id.clone(), key.public_key.clone());
        self.active = key.key_id.clone();
        Ok(key.key_id)
    }

    /// The active key id.
    pub fn active_key_id(&self) -> &str {
        &self.active
    }

    /// Every known key id — current and retired (what `signer_key_ids`
    /// accumulates onto a manifest).
    pub fn key_ids(&self) -> BTreeSet<String> {
        self.public_keys.keys().cloned().collect()
    }

    /// `key_id → public key` — the resolver's table (public bytes only).
    pub fn public_keys(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.public_keys
    }

    /// The resolver over the custody's public half — install on the
    /// `Store` (`set_audit_key_resolver`) so `verify`/`audit_view` check
    /// real signatures.
    pub fn resolver(&self) -> LedgerSignerResolver {
        LedgerSignerResolver::new(self.public_keys.clone())
    }

    /// The `SecretSourceResolver` serving the custody's `OperatorVault`
    /// coordinate — `operator_vault:ledger-signer` resolves the **active**
    /// key's PKCS#8 document as hex (the `kernel_use` value). Every other
    /// coordinate delegates to `next` (the host's resolver); `None` fails
    /// closed like `DenyAllResolver`.
    pub fn source_resolver(
        &self,
        next: Option<Box<dyn SecretSourceResolver>>,
    ) -> CustodySourceResolver {
        CustodySourceResolver::new(self.dir.clone(), next)
    }

    /// The `kernel_only` channel spec the broker carries for
    /// `kernel_use(KernelPurpose::LedgerSigning)` — `SigningKey`,
    /// `bindable: false`, no env projection ever.
    pub fn channel_spec() -> SecretChannelSpec {
        SecretChannelSpec {
            kind: CredentialKind::SigningKey,
            source: SecretSource::OperatorVault {
                vault_ref: SIGNER_VAULT_REF.to_string(),
            },
            destinations: Vec::new(),
            allowed_env_names: None,
            delivery_modes: BTreeSet::new(),
            max_lifetime_ms: None,
            rotation_policy: Some("operator".to_string()),
            sender_constraint: SenderConstraint::None,
            constraints: Default::default(),
            bindable: false,
            access_class: AccessClass::KernelOnly,
            canary: false,
            description: "the kernel-held Ed25519 ledger-signing key (OQ-170)".to_string(),
        }
    }

    /// Provision the signer through the broker (OQ-170): declare the
    /// `kernel_only` channel (idempotent — an existing channel is left),
    /// then `kernel_use(KernelPurpose::LedgerSigning)` — the
    /// `security.credential.used{destination = kernel:ledger_signing}` row
    /// is durable before the value returns. The returned signer is built
    /// *from the provisioned value* — never from a file read that bypassed
    /// the seam.
    #[allow(clippy::too_many_arguments)] // the arity is `kernel_use`'s record shape + the channel/provenance inputs.
    pub fn install(
        &self,
        broker: &mut CredentialBroker,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        caller: &str,
        effect_id: &str,
        provenance: ProvenanceRecord,
    ) -> Result<LedgerSignerKey, CustodyError> {
        match broker.register_channel(SIGNER_CHANNEL_ID, Self::channel_spec(), provenance) {
            Ok(_) | Err(crate::errors::BrokerError::ChannelExists { .. }) => {}
            Err(e) => {
                return Err(CustodyError::Provision {
                    detail: format!("register_channel: {e}"),
                })
            }
        }
        let value = broker
            .kernel_use(
                store,
                run_id,
                lease,
                SIGNER_CHANNEL_ID,
                KernelPurpose::LedgerSigning,
                caller,
                effect_id,
            )
            .map_err(|e: Refused| CustodyError::Provision {
                detail: format!("kernel_use({}): {}", e.code.as_str(), e.detail),
            })?;
        LedgerSignerKey::from_pkcs8_hex(&value)
    }
}

/// The custody-side `SecretSourceResolver` — serves
/// `operator_vault:ledger-signer` (the active key's PKCS#8 hex,
/// mode-checked at resolve) and delegates every other coordinate to the
/// host's resolver. Never an env-var fallback — the file *is* the custody
/// boundary.
pub struct CustodySourceResolver {
    dir: PathBuf,
    next: Option<Box<dyn SecretSourceResolver>>,
}

impl CustodySourceResolver {
    /// A resolver over the custody directory `dir` — reads the active
    /// posture fresh at every resolve (a booted `LedgerSignerCustody`
    /// hands one out via `source_resolver`; this constructor serves the
    /// compose-ahead-of-boot case, e.g. an unprovisioned dir that must
    /// still fail closed rather than panic).
    pub fn new(dir: PathBuf, next: Option<Box<dyn SecretSourceResolver>>) -> Self {
        Self { dir, next }
    }
}

impl SecretSourceResolver for CustodySourceResolver {
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError> {
        let fail = |detail: &str| {
            Err(SourceError {
                kind: source.kind(),
                detail: detail.to_string(),
            })
        };
        let SecretSource::OperatorVault { vault_ref } = source else {
            return match &self.next {
                Some(n) => n.resolve(source),
                None => fail("resolver unavailable"),
            };
        };
        if vault_ref != SIGNER_VAULT_REF {
            return match &self.next {
                Some(n) => n.resolve(source),
                None => fail("resolver unavailable"),
            };
        }
        // The active pointer + the key file, mode-checked at resolve — the
        // custody posture is re-read, never cached stale.
        let custody = LedgerSignerCustody::open(&self.dir).map_err(|e| SourceError {
            kind: source.kind(),
            detail: format!("custody: {e}"),
        })?;
        let Some(custody) = custody else {
            return Err(SourceError {
                kind: source.kind(),
                detail: "no ledger-signer custody provisioned".into(),
            });
        };
        let path = signer_key_path(&self.dir, &custody.active).map_err(|e| SourceError {
            kind: source.kind(),
            detail: format!("custody: {e}"),
        })?;
        let bytes = read_key_file(&path).map_err(|e| SourceError {
            kind: source.kind(),
            detail: format!("custody: {e}"),
        })?;
        Ok(hex_encode(&bytes))
    }
}
