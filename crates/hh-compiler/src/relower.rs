//! `relower(bundle, new_profile)` (§3.2.2 "Re-lowering"; §3.2.7; ADR-0126): stages
//! 3–5 re-run under the new profile, `runtime_plan` is **unchanged**, a new
//! `bundle_id` is issued, and the transcript migration is returned — profile-opaque
//! items are dropped or replaced by a typed placeholder per the profile's
//! `transcript_render.stale_signature` rule (OQ-067). The durable record is the
//! `model.surface.relowered` ledger event — `relowered_event` produces its payload.
//!
//! "Retire rule R" is `relower(bundle, profile − R)` (T-LCD-05): a delete-this-rule
//! arm is one compile away.

use hh_assembly::ClassCatalog;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::compiler::{compile, CompileInputs};
use crate::errors::CompileError;
use crate::link::VariantView;
use crate::profile::ProfileView;
use crate::seal::{CompiledBundle, ModelSurfaceState};

/// `TranscriptMigration{dropped_items[], rewritten_items[]}` (§3.2.7) — what the
/// re-lower changed about the model-facing transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptMigration {
    /// The old bundle.
    pub from_bundle: String,
    /// The new bundle.
    pub to_bundle: String,
    /// The old profile coordinate.
    pub old_profile_ref: String,
    /// The new profile coordinate.
    pub new_profile_ref: String,
    /// Items the new profile cannot represent — dropped per
    /// `transcript_render.stale_signature = drop`, or surfaces unexposed under the
    /// new profile.
    pub dropped_items: Vec<Json>,
    /// Items rewritten in place — renamed surfaces, `stale_signature = placeholder`
    /// replacements.
    pub rewritten_items: Vec<Json>,
    /// The recorded reason (`compatibility_token_changed`, `rule_retired`, …).
    pub reason: String,
}

/// `relower(old, inputs, …)` — `inputs` carries the *new* `profile_refs` (the rest
/// is the same compile input set: same sealed definition, same targets).
///
/// The runtime plan is profile-independent by construction — `lower_native` re-runs
/// deterministically and the result must be identical; a difference is a defect
/// (`PlanError`, never a silent change).
pub fn relower(
    old: &CompiledBundle,
    inputs: &CompileInputs,
    profiles: &dyn ProfileView,
    variants: &dyn VariantView,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
    reason: &str,
) -> Result<(CompiledBundle, TranscriptMigration), CompileError> {
    let new = compile(inputs, profiles, variants, catalog, kernel)?;
    if new.runtime_plan != old.runtime_plan {
        return Err(CompileError::PlanError {
            node: "runtime_plan".to_string(),
            detail: "relower produced a different runtime_plan — stages 3–5 only may run"
                .to_string(),
        });
    }

    let (old_profile, new_profile) = (bundle_profile(old), bundle_profile(&new));
    let mut dropped_items = Vec::new();
    let mut rewritten_items = Vec::new();

    if let (ModelSurfaceState::Lowered(om), ModelSurfaceState::Lowered(nm)) =
        (&old.model_surface, &new.model_surface)
    {
        // Signature staleness — the new profile's `transcript_render.stale_signature`
        // decides: `drop` → dropped item; `placeholder` → rewritten (typed
        // placeholder). Recorded once per bundle (the transcript is the renderer's
        // domain; the migration lists the *kind*, per §3.2.2).
        let stale = nm
            .transcript_renderer
            .get("stale_signature")
            .and_then(Json::as_str)
            .unwrap_or("placeholder");
        if om.profile != nm.profile {
            match stale {
                "drop" => dropped_items.push(Json::obj([
                    ("kind", Json::str("stale_signature")),
                    ("profile", Json::str(om.profile.clone())),
                ])),
                _ => rewritten_items.push(Json::obj([
                    ("kind", Json::str("stale_signature")),
                    ("profile", Json::str(om.profile.clone())),
                    ("replacement", Json::str("placeholder")),
                ])),
            }
        }

        // Surface renames / drops — matched on the capability (the identity anchor;
        // a rename never touches `capability_refs`, E7).
        for ot in &om.tools {
            let cap = &ot.binding.hir_node_id;
            match nm.tools.iter().find(|nt| &nt.binding.hir_node_id == cap) {
                Some(nt) if nt.binding.surface_name != ot.binding.surface_name => {
                    rewritten_items.push(Json::obj([
                        ("capability", Json::str(cap.clone())),
                        ("from", Json::str(ot.binding.surface_name.clone())),
                        ("kind", Json::str("rename")),
                        ("to", Json::str(nt.binding.surface_name.clone())),
                    ]));
                }
                Some(_) => {}
                None => {
                    dropped_items.push(Json::obj([
                        ("capability", Json::str(cap.clone())),
                        ("kind", Json::str("surface")),
                        ("surface_name", Json::str(ot.binding.surface_name.clone())),
                    ]));
                }
            }
        }
    }

    let migration = TranscriptMigration {
        from_bundle: old.bundle_id.clone(),
        to_bundle: new.bundle_id.clone(),
        old_profile_ref: old_profile,
        new_profile_ref: new_profile,
        dropped_items,
        rewritten_items,
        reason: reason.to_string(),
    };
    Ok((new, migration))
}

/// The bundle's bound profile coordinate (the lowered surface's `profile` member,
/// or the chain head for a deferred surface).
fn bundle_profile(b: &CompiledBundle) -> String {
    match &b.model_surface {
        ModelSurfaceState::Lowered(s) => s.profile.clone(),
        ModelSurfaceState::Deferred => b.profile_chain.last().cloned().unwrap_or_default(),
    }
}

/// The `model.surface.relowered` ledger-event payload (§3.2.8's event record —
/// `{old_bundle_id, new_bundle_id, old_profile_ref, new_profile_ref,
/// dropped_items[], rewritten_items[], reason}`).
pub fn relowered_event(m: &TranscriptMigration) -> Json {
    Json::obj([
        ("dropped_items", Json::Arr(m.dropped_items.clone())),
        ("new_bundle_id", Json::str(m.to_bundle.clone())),
        ("new_profile_ref", Json::str(m.new_profile_ref.clone())),
        ("old_bundle_id", Json::str(m.from_bundle.clone())),
        ("old_profile_ref", Json::str(m.old_profile_ref.clone())),
        ("reason", Json::str(m.reason.clone())),
        ("rewritten_items", Json::Arr(m.rewritten_items.clone())),
    ])
}
