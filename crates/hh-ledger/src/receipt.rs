//! `lifecycle.ledger.receipt` — the receiver-receipt lift (§5g.6 §2's C2
//! receiver-attested receipts; S-324; ADR-0345 D4). A receipt is a
//! receiver-attested record of an open-world effect: the kernel records the
//! record *as supplied* — the durable row is the kernel fact "this receipt was
//! lifted", while the receipt's own claims keep their `unverified`/`external`
//! status (T-LCD-07 — a receiver's authority never exceeds `external`; the
//! kernel never fabricates receiver verification).
//!
//! The row stays content-free: `{effect_id, observed_ref?, receiver_ref,
//! receipt_id, attested_at, attestation{key_id, alg_ref, sig}, status,
//! supplied_by}` + the optional `claim_ref` content ref (the receiver's claim
//! document lives blob-side). `status` is fixed to `unverified` — a receipt
//! row can never carry a verification claim the kernel did not make.

use hh_wire::json::Json;

/// The receipt's lift status — the only status a `lifecycle.ledger.receipt`
/// row may carry. `unverified` because the receiver's attestation is external
/// evidence: the kernel does not hold receiver key material, and custody of
/// it stays external exactly like signer custody (OQ-170).
pub const RECEIPT_STATUS_UNVERIFIED: &str = "unverified";

/// The boundary-visible receipt record — the C2 receiver-attestation lift
/// (ADR-0345 D4). `to_json` produces exactly the `lifecycle.ledger.receipt`
/// audit-field members plus the `claim_ref` content ref.
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    /// The open-world effect the receipt evidences (`scope.effect_id`'s
    /// durable id — `record_receipt` binds against the run's effect fold).
    pub effect_id: String,
    /// The `action.effect.observed` row the receipt answers, when the
    /// receipt names one (an event id in the same run).
    pub observed_ref: Option<String>,
    /// The receiver's declared identity (an external ref — the kernel
    /// asserts only that the supplier named it).
    pub receiver_ref: String,
    /// The receiver-minted receipt id — the idempotence key: a second
    /// record with the same `receipt_id` and identical members returns the
    /// existing row; a same-id/different-members record refuses.
    pub receipt_id: String,
    /// The receiver-side attestation timestamp (ms — receiver's own clock;
    /// carried as evidence, never merged into the run's HLC).
    pub attested_at: u64,
    /// The receiver's attestation `{key_id, alg_ref, sig}` — opaque to the
    /// kernel (carried, never verified here).
    pub attestation: Json,
    /// Who supplied the record (session/adapter id — the lift's provenance).
    pub supplied_by: String,
    /// The receiver claim document's content ref (blob-side), when carried.
    pub claim_ref: Option<String>,
}

impl Receipt {
    /// Shape-validate the record (the store op binds it to the run's folds —
    /// this checks only what the record itself must satisfy). `detail`
    /// strings name the refused member for the `SchemaViolation` mapping.
    pub fn validate(&self) -> Result<(), String> {
        if self.effect_id.is_empty() {
            return Err("receipt.effect_id empty".into());
        }
        if self.receiver_ref.is_empty() {
            return Err("receipt.receiver_ref empty".into());
        }
        if self.receipt_id.is_empty() {
            return Err("receipt.receipt_id empty".into());
        }
        if self.supplied_by.is_empty() {
            return Err("receipt.supplied_by empty".into());
        }
        if let Some(obs) = &self.observed_ref {
            if obs.is_empty() {
                return Err("receipt.observed_ref present but empty".into());
            }
        }
        if let Some(cr) = &self.claim_ref {
            if cr.is_empty() {
                return Err("receipt.claim_ref present but empty".into());
            }
        }
        // The attestation is opaque evidence but must carry the declared
        // triple — a receipt with no attestation shape is not a receipt.
        let (Some(_), Some(_), Some(_)) = (
            self.attestation.get("key_id").and_then(Json::as_str),
            self.attestation.get("alg_ref").and_then(Json::as_str),
            self.attestation.get("sig").and_then(Json::as_str),
        ) else {
            return Err("receipt.attestation must carry {key_id, alg_ref, sig} strings".into());
        };
        Ok(())
    }

    /// The durable row's payload members (audit fields + `claim_ref` content
    /// ref). `status` is emitted fixed — the kernel's lift claim, not the
    /// receiver's.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("effect_id", Json::str(&self.effect_id)),
            (
                "observed_ref",
                self.observed_ref
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("receiver_ref", Json::str(&self.receiver_ref)),
            ("receipt_id", Json::str(&self.receipt_id)),
            ("attested_at", Json::Int(self.attested_at as i64)),
            ("attestation", self.attestation.clone()),
            ("status", Json::str(RECEIPT_STATUS_UNVERIFIED)),
            ("supplied_by", Json::str(&self.supplied_by)),
            (
                "claim_ref",
                self.claim_ref
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
        ])
    }

    /// Parse a durable row's payload back to the record — `None` when the
    /// payload does not carry the receipt shape (used by the audit-view
    /// fold; a kernel-committed row always parses).
    pub fn from_json(j: &Json) -> Option<Self> {
        // The durable status member must be the fixed lift status — a row
        // claiming otherwise did not come through `record_receipt`.
        if j.get("status").and_then(Json::as_str) != Some(RECEIPT_STATUS_UNVERIFIED) {
            return None;
        }
        let receipt = Self {
            effect_id: j.get("effect_id").and_then(Json::as_str)?.to_string(),
            observed_ref: j
                .get("observed_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            receiver_ref: j.get("receiver_ref").and_then(Json::as_str)?.to_string(),
            receipt_id: j.get("receipt_id").and_then(Json::as_str)?.to_string(),
            attested_at: j.get("attested_at").and_then(Json::as_int)? as u64,
            attestation: j.get("attestation").cloned().unwrap_or(Json::Null),
            supplied_by: j.get("supplied_by").and_then(Json::as_str)?.to_string(),
            claim_ref: j
                .get("claim_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        };
        receipt.validate().ok()?;
        Some(receipt)
    }
}
