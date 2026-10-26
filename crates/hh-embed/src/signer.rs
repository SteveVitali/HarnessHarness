//! The OQ-170 signer-custody wiring (R4.15e; ADR-0376): the kernel-held
//! Ed25519 ledger signer — file-backed under `<store_root>/secrets/` by
//! `hharness-secrets`, provisioned at `open` through
//! `CredentialBroker::kernel_use(KernelPurpose::LedgerSigning)`, consumed
//! by the audit surface (`signer_key_ids` on every manifest the kernel
//! stamps, `kind = final` checkpoints at `lifecycle.run.finished`, the
//! co-signed `key_rotation` checkpoint at rotate).
//!
//! Custody failures refuse the boot — a mis-permissioned or corrupt key
//! file is a typed `EmbedError::Refused`, never a silently unsigned run;
//! an *absent* custody is the honest unprovisioned posture (the run stays
//! unsigned, `audit_view` reports its `n/a` reason).

use hharness_embed_schema::errors::EmbedError;
use hharness_ledger::audit::{AuditSigner, CheckpointKind};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Lease;
use hharness_wire::json::Json;

use crate::service::{ledger_err, EmbedService, LEASE_TTL_MS};

impl EmbedService {
    /// Load + provision the signer custody at boot (`EmbedService::open` —
    /// §5g.6 (a): the `security.credential.used{destination =
    /// kernel:ledger_signing}` row lands on the kernel's own custody run
    /// before the key materialises). `Ok(())` on the unprovisioned
    /// posture; `Err(Refused)` on any custody defect.
    pub(crate) fn install_signer_custody(&mut self) -> Result<(), EmbedError> {
        let dir = self.store.root().join("secrets");
        let custody =
            hharness_secrets::LedgerSignerCustody::open(&dir).map_err(|e| EmbedError::Refused {
                reason: format!("signer_custody: {e}"),
            })?;
        let Some(custody) = custody else {
            return Ok(());
        };
        // The broker's resolver must serve the custody coordinate — the
        // `CustodySourceResolver` wraps the (default deny-all) vault layer;
        // a host `set_credential_resolver` later composes the same way.
        self.credential_broker = hharness_secrets::CredentialBroker::new(
            Box::new(
                custody.source_resolver(None::<Box<dyn hharness_secrets::SecretSourceResolver>>),
            ),
            "hh-embed/credentials",
        );
        self.signer_ids = custody.key_ids();
        self.signer_custody = Some(custody);
        // The custody run declares the signer set on its manifest — the
        // provisioning row + every rotation checkpoint land under a
        // manifest whose `signer_key_ids` admits them.
        let (run_id, lease) = self.ensure_signer_run()?;
        let signer = self
            .signer_custody
            .as_ref()
            .unwrap()
            .install(
                &mut self.credential_broker,
                &mut self.store,
                &run_id,
                &lease,
                "kernel:ledger-signer",
                "eff-ledger-signer-boot",
                self.kernel_prov.clone(),
            )
            .map_err(|e| EmbedError::Refused {
                reason: format!("signer_custody: {e}"),
            })?;
        // Public keys only cross the seam — the resolver serves
        // `key_id → public bytes` (retired ids included).
        self.store
            .set_audit_key_resolver(Box::new(self.signer_custody.as_ref().unwrap().resolver()));
        self.signer = Some(signer);
        // Sign-on-boot: the custody run's first checkpoint, signed by the
        // provisioned key — the boot's own proof the real path signs.
        self.mint_custody_checkpoint(CheckpointKind::OnDemand)?;
        Ok(())
    }

    /// The kernel-internal `purpose = ledger_signing` run — created lazily
    /// like `registry_run`, re-leased on expiry; its manifest stamps the
    /// custody `signer_key_ids` so `signer_rotation_checkpoint`'s mint
    /// gate admits the incumbent (and the fold admits each successor).
    pub(crate) fn ensure_signer_run(&mut self) -> Result<(String, Lease), EmbedError> {
        if let Some((run_id, lease)) = &self.signer_run {
            match self.store.renew(lease) {
                Ok(l) => {
                    let out = (run_id.clone(), l.clone());
                    self.signer_run = Some(out.clone());
                    return Ok(out);
                }
                Err(_) => {
                    let l = self
                        .store
                        .acquire_writer(&self.holder, run_id, LEASE_TTL_MS)
                        .map_err(ledger_err)?;
                    let out = (run_id.clone(), l);
                    self.signer_run = Some(out.clone());
                    return Ok(out);
                }
            }
        }
        let mut manifest = RunManifest::minimal(RunKind::Fleet);
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest
            .extra
            .insert("purpose".to_string(), Json::str("ledger_signing"));
        manifest.signer_key_ids = self.signer_key_ids();
        let (run_id, lease) = self
            .store
            .open_run(manifest, &self.holder)
            .map_err(ledger_err)?;
        let out = (run_id, lease);
        self.signer_run = Some(out.clone());
        Ok(out)
    }

    /// The custody `signer_key_ids` for manifests the kernel stamps —
    /// active + retired key ids (empty under the unprovisioned posture).
    pub fn signer_key_ids(&self) -> Vec<String> {
        self.signer_ids.iter().cloned().collect()
    }

    /// Mint a checkpoint on the custody run signed by the provisioned
    /// signer — the sign-on-boot `on_demand` row and operator/test-facing
    /// emits (the custody run is the kernel's own — the checkpoint is a
    /// kernel act, never a model-visible op).
    pub fn mint_custody_checkpoint(&mut self, kind: CheckpointKind) -> Result<(), EmbedError> {
        let (run_id, lease) = self.ensure_signer_run()?;
        let Some(signer) = self.signer.as_mut() else {
            return Err(EmbedError::Refused {
                reason: "signer_custody: no signer provisioned".to_string(),
            });
        };
        self.store
            .checkpoint(&run_id, &lease, kind, signer)
            .map_err(ledger_err)?;
        Ok(())
    }

    /// Emit the `kind = final` checkpoint for a session run that just
    /// finished — DF-S2.5-1's emission leg: the terminal claim covers
    /// `lifecycle.run.finished`, signed by the kernel-held key. Silent
    /// only where signing was never declared (a manifest without
    /// `signer_key_ids` or no custody — the honest unsigned posture);
    /// `checkpoint`'s own idempotence makes re-calls cheap.
    pub(crate) fn emit_final_checkpoint(&mut self, run_id: &str, lease: &Lease) {
        if self.signer.is_none() {
            return;
        }
        let declared = self
            .store
            .manifest(run_id)
            .map(|m| !m.signer_key_ids.is_empty())
            .unwrap_or(false);
        if !declared {
            return;
        }
        // Renew-or-reacquire the writer fence — the session lease may have
        // expired across a long turn; `final` is the only kind a finished
        // run still admits.
        let lease = match self.store.renew(lease) {
            Ok(l) => l,
            Err(_) => match self
                .store
                .acquire_writer(&self.holder, run_id, LEASE_TTL_MS)
            {
                Ok(l) => l,
                Err(_) => return,
            },
        };
        if let Some(signer) = self.signer.as_mut() {
            let _ = self
                .store
                .checkpoint(run_id, &lease, CheckpointKind::Final, signer);
        }
    }

    /// Rotate the kernel-held signer (OQ-170; ADR-0376): the custody mints
    /// the successor `0600` file + moves the active pointer; the new key
    /// provisions through `kernel_use` (a second durable `used` row); the
    /// **co-signed** `key_rotation{from_key_id, to_key_id}` checkpoint
    /// lands on the custody run — incumbent and successor sign over the
    /// same unsigned claim. The resolver then serves active + retired
    /// public keys so historical checkpoints keep verifying. The
    /// incumbent's co-signature is required — a rotation claim without it
    /// fails `BadSignature` at verify.
    pub fn rotate_ledger_signer(&mut self) -> Result<String, EmbedError> {
        let custody = self.signer_custody.as_mut().ok_or(EmbedError::Refused {
            reason: "signer_custody: no custody provisioned".to_string(),
        })?;
        let new_key_id = custody.rotate().map_err(|e| EmbedError::Refused {
            reason: format!("signer_custody rotate: {e}"),
        })?;
        let (run_id, lease) = self.ensure_signer_run()?;
        let mut successor = self
            .signer_custody
            .as_ref()
            .unwrap()
            .install(
                &mut self.credential_broker,
                &mut self.store,
                &run_id,
                &lease,
                "kernel:ledger-signer",
                "eff-ledger-signer-rotate",
                self.kernel_prov.clone(),
            )
            .map_err(|e| EmbedError::Refused {
                reason: format!("signer_custody rotate provision: {e}"),
            })?;
        debug_assert_eq!(successor.key_id(), new_key_id.as_str());
        let mut incumbent = self.signer.take().ok_or(EmbedError::Refused {
            reason: "signer_custody: no signer provisioned".to_string(),
        })?;
        // The durable rotation record — `kind = rotation`, co-signed.
        self.store
            .signer_rotation_checkpoint(&run_id, &lease, &mut incumbent, &mut successor)
            .map_err(ledger_err)?;
        self.signer_ids = self.signer_custody.as_ref().unwrap().key_ids();
        self.store
            .set_audit_key_resolver(Box::new(self.signer_custody.as_ref().unwrap().resolver()));
        self.signer = Some(successor);
        Ok(new_key_id)
    }
}
