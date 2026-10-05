//! Identity rotation against a run ledger (spec §5g.6 §2, R-2.8.6;
//! ADR-0307): `rotate(store, run, plan)` — the `idp/1 → idp/2`
//! transition. The record shapes (`RotationPlan`, `BridgeRecord`,
//! `IdMigration`, `migrate_id`) live in `hh-identity::rotation`; this
//! module is the store-driving arm (CC5: ledger writes happen here,
//! identity stays pure).
//!
//! Semantics:
//!
//! - The covered durable prefix's event hashes are *recomputed* under
//!   `to_idp` — `rehashed_leaves` replays `H_{idp'}(preimage_i ∥
//!   chain'_{i-1})` where the preimage's own `prev_hash` member stays
//!   frozen as data (stored ids are never rewritten — spec §5g.6 §2 "old
//!   ids retained; verification across profiles").
//! - The rotation mints `security.audit.bridge` (the sealed
//!   `BridgeRecord` binding `old_id → new_id` per covered event) then
//!   `security.audit.checkpoint{kind = rotation, identity_profile =
//!   to_idp, rehash{idp', chain_hash', tree_head'}, bridge_record_ref,
//!   attestation_ref?}` — the rotation claim's own digests are under
//!   `to_idp`, so the checkpoint is self-evidencing (`rehash` values equal
//!   the claim's `tree_head`/`chain_hash`) and post-rotation checkpoints
//!   continue under `to_idp` by inheriting the last claim's
//!   `identity_profile`.
//! - Refusals: `unknown_idp` / `idp_not_writable` / `bridge_missing`
//!   surface as `LedgerError::SchemaViolation` with the refusal name in
//!   `detail` (ADR-0307 — the spec's refusal column is a string
//!   vocabulary; typed variants exist in
//!   `hh_identity::rotation::RotationError`).

use hh_identity::idp::{profile_for, IdentityProfile};
use hh_identity::rotation::{BridgeRecord, IdMigration, MigrationMethod, RotationPlan};
use hh_wire::json::Json;

use crate::audit::AuditSigner;
use crate::errors::LedgerError;
use crate::event::EventEnvelope;
use crate::ids::GENESIS_HASH;
use crate::store::Lease;
use crate::store::Store;

/// The rehashed leaf chain under `profile`: `chain'_i =
/// H_{profile}(preimage_i ∥ chain'_{i-1})` with `chain'_0`'s predecessor
/// = the first event's stored `prev_hash` member (the genesis sentinel —
/// a constant, profile-independent). Under `idp/1` this returns exactly
/// the stored `hash` values — the same construction, one code path (CC1).
pub fn rehashed_leaves<'a, I>(profile: &IdentityProfile, events: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a EventEnvelope>,
{
    let events: Vec<&EventEnvelope> = events.into_iter().collect();
    let mut out = Vec::with_capacity(events.len());
    let mut prev = events
        .first()
        .map(|e| e.prev_hash.clone())
        .unwrap_or_else(|| GENESIS_HASH.to_string());
    for ev in events {
        let h = ev.recompute_hash_in(profile, &prev);
        prev = h.clone();
        out.push(h);
    }
    out
}

/// What `rotate` returns — the minted checkpoint envelope plus the sealed
/// bridge record (`bridge` is `Some` only when the plan asked for one).
#[derive(Debug, Clone)]
pub struct RotationReceipt {
    /// The `security.audit.checkpoint{kind = rotation}` envelope.
    pub checkpoint: EventEnvelope,
    /// The sealed `BridgeRecord` (`security.audit.bridge` row payload).
    pub bridge: Option<BridgeRecord>,
    /// The `bridge` row's event id (what `bridge_record_ref` names).
    pub bridge_event_id: Option<String>,
}

/// `identity.rotate(store, run_id, lease, signer, plan)` (spec §5g.6 §2):
/// emits the bridge record (when `plan.bridge`) and the rotation
/// checkpoint; post-rotation claims continue under `to_idp`.
///
/// The plan's `from_idp` must name the run's *current* claim profile —
/// rotating a run already rotated requires a second plan
/// (`idp_not_writable`-class refusal, never a silent re-anchor).
pub fn rotate(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    signer: &mut dyn AuditSigner,
    plan: &RotationPlan,
) -> Result<RotationReceipt, LedgerError> {
    let (from, to) = plan.validate().map_err(rot_err)?;
    // The bridge record is minted first — the checkpoint's
    // `bridge_record_ref` names its event id.
    let (bridge, bridge_event_id) = if plan.bridge {
        let state = store.run(run_id)?;
        let leaves2 = rehashed_leaves(to, state.events.iter());
        let rotations: Vec<IdMigration> = state
            .events
            .iter()
            .zip(leaves2.iter())
            .map(|(ev, new_id)| IdMigration {
                old_id: ev.hash.clone(),
                new_id: new_id.clone(),
                object_ref: ev.event_id.clone(),
                method: MigrationMethod::RotateFullRehash,
            })
            .collect();
        let mut bridge = BridgeRecord {
            bridge_id: String::new(),
            run_id: run_id.to_string(),
            from_idp: from.idp_id.to_string(),
            to_idp: to.idp_id.to_string(),
            rotations,
            links_run_ids: vec![run_id.to_string()],
            prev_bridge_ref: state
                .events
                .iter()
                .rev()
                .find(|e| e.class == "security.audit.bridge")
                .and_then(|e| e.payload.get("bridge_id").and_then(Json::as_str))
                .map(String::from),
            at_ms: store.now_ms().min(i64::MAX as u64) as i64,
        };
        bridge.seal();
        let ev = store.emit_system(run_id, lease, "security.audit.bridge", bridge.to_json())?;
        (Some(bridge), Some(ev.event_id))
    } else {
        (None, None)
    };
    let checkpoint =
        store.rotation_checkpoint(run_id, lease, signer, plan, bridge_event_id.as_deref())?;
    let _ = from;
    Ok(RotationReceipt {
        checkpoint,
        bridge,
        bridge_event_id,
    })
}

/// Map an identity-layer [`RotationError`] to the ledger refusal — the
/// spec's refusal names are preserved verbatim in `detail`.
fn rot_err(e: hh_identity::rotation::RotationError) -> LedgerError {
    LedgerError::SchemaViolation {
        detail: e.to_string(),
    }
}

/// `rehash{idp', chain_hash', tree_head'}` over `events` under `profile`
/// — the rotation claim's member body (spec §5g.6 §2). `from_idp` is
/// recorded for readers (additive — the spec's three members stand).
pub fn rehash_claim<'a, I>(events: I, from_idp: &str, profile: &IdentityProfile) -> Json
where
    I: IntoIterator<Item = &'a EventEnvelope>,
{
    let leaves = rehashed_leaves(profile, events);
    let chain = leaves
        .last()
        .cloned()
        .unwrap_or_else(|| GENESIS_HASH.to_string());
    Json::obj([
        ("idp", Json::str(profile.idp_id)),
        ("from_idp", Json::str(from_idp)),
        ("chain_hash", Json::str(chain)),
        (
            "tree_head",
            Json::str(crate::tree::mth_in(profile, &leaves)),
        ),
        ("tree_size", Json::Int(leaves.len() as i64)),
    ])
}

/// Verify a rotation claim's `rehash` member against the covered prefix
/// (auditor/`verify_run` shared core): `rehash.idp` registered, and the
/// recomputed digests equal the claimed ones. `None` on parse failure.
pub fn verify_rehash<'a, I>(rehash: &Json, events: I) -> Option<bool>
where
    I: IntoIterator<Item = &'a EventEnvelope>,
{
    let events: Vec<&EventEnvelope> = events.into_iter().collect();
    let idp_name = rehash.get("idp")?.as_str()?;
    let profile = profile_for(idp_name)?;
    let covered = rehash
        .get("tree_size")
        .and_then(Json::as_int)
        .map(|v| v.max(0) as usize)
        .unwrap_or(events.len())
        .min(events.len());
    let leaves = rehashed_leaves(profile, events[..covered].iter().copied());
    let ok_chain = rehash.get("chain_hash").and_then(Json::as_str)
        == leaves.last().map(String::as_str).or(Some(GENESIS_HASH));
    let ok_head = rehash.get("tree_head").and_then(Json::as_str)
        == Some(crate::tree::mth_in(profile, &leaves).as_str());
    Some(ok_chain && ok_head)
}
