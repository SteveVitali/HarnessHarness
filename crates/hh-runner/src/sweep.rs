//! `sweep` — the phase-4 sweep's runner half (manifest 304/R6.43;
//! D3-c first pass; REQ-XD-220's gate, REQ-XE-124's sealed-ref
//! naming, REQ-XH-115's label).
//!
//! The drive packet's `sweep` member declares the one first-pass
//! contrast: the V5 `rsn-drop` level of one registry factor, sealed
//! through hh-lab's enumerate → resolve → seal chain (the slot record
//! names the sealed `version_id` via `participant.hir`), bound to the
//! **exact baseline H swept**. The schedule gate for a V5 arm:
//!
//! - `participant.hir.definition_version_id` equals the declared
//!   `level_ref` — every slot names its document (REQ-XE-124);
//! - `participant.hir.level_assignment` names `{factor → level}` and
//!   `override_spellings` the attribution spellings (REQ-XE-126);
//! - the declared `hh-l1-conformance/1` pass id recomputes over the
//!   baseline identity the level derives from — the exact H swept
//!   (REQ-XD-220). A pass minted for any other H refuses, and an H
//!   change yields a new phase-4 cohort id;
//! - the declared `cohort_id` recomputes and the departure member
//!   cites AR-33 (ADR-0439) — the first pass runs phase 4
//!   concurrently with L2 only as AR-33 admitted it at GATE-G23
//!   (X-01); the REQ-XH-115 label is carried verbatim (X-08).
//!
//! A failure withholds the packing unit `withheld{l1_not_passed}` —
//! the closed disposition's own spelling (R6.41's gate shares it).

use std::collections::BTreeMap;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;
use crate::l1_gate::L1Identity;
use crate::native::{l1_gate_match, L1GateDecl};
use crate::packet::PacketError;

/// `hh-runner-sweep/1` — the drive packet's sweep declaration.
pub const SWEEP_SCHEMA: &str = "hh-runner-sweep/1";

/// The AR-33 citation the `departure` member must carry — the
/// concurrency-with-L2 departure is admitted only as ADR-0439
/// recorded it at GATE-G23.
pub const AR33_DEPARTURE: &str =
    "AR-33 (ADR-0439, admitted GATE-G23): phase-4 runs concurrently with L2's H side under one pre-registration — the first-pass exception to REQ-XD-220's no-concurrency clause";

/// `phase4_cohort_id(baseline, level_ref)` — the phase-4 cohort:
/// bound to the exact baseline H the sweep derives from plus the
/// sealed level ref. An H change (or a level-doc change) mints a new
/// cohort (REQ-XD-220's "any H change starts a new phase-4 cohort").
pub fn phase4_cohort_id(h: &L1Identity, level_ref: &str) -> String {
    let pre = Json::obj([
        ("participant_ref", Json::str(&h.participant_ref)),
        ("version", Json::str(&h.version)),
        ("digest", Json::str(&h.digest)),
        ("level_ref", Json::str(level_ref)),
    ])
    .to_canonical_string();
    format!("p4-cohort:sha256:{}", sha256_hex(pre.as_bytes()))
}

/// The declared sweep — one level of one factor, the exact baseline
/// H it derives from, the cohort and the AR-33 departure citation.
#[derive(Debug, Clone)]
pub struct SweepDecl {
    /// The frozen `sweep-registry/1` hash (REQ-XD-201).
    pub registry_sha256: String,
    /// The level id (`V5`).
    pub level_id: String,
    /// The level name (`rsn-drop`).
    pub level_name: String,
    /// The factor id.
    pub factor_id: String,
    /// The sealed level's `version_id` — `LevelSpec.ref_` (REQ-XE-124;
    /// digest-frozen, never a mutable string).
    pub level_ref: String,
    /// The exact baseline H the level derives from (the identity the
    /// G-L1 pass binds).
    pub baseline: L1Identity,
    /// The declared cohort — recomputed at schedule (REQ-XD-220).
    pub cohort_id: String,
    /// The AR-33 departure citation (verbatim).
    pub departure: String,
    /// The REQ-XH-115 label (verbatim).
    pub label: String,
}

impl PartialEq for SweepDecl {
    fn eq(&self, o: &Self) -> bool {
        self.registry_sha256 == o.registry_sha256
            && self.level_id == o.level_id
            && self.level_name == o.level_name
            && self.factor_id == o.factor_id
            && self.level_ref == o.level_ref
            && self.cohort_id == o.cohort_id
            && self.departure == o.departure
            && self.label == o.label
            && self.baseline.participant_ref == o.baseline.participant_ref
            && self.baseline.version == o.baseline.version
            && self.baseline.digest == o.baseline.digest
    }
}

fn serr(detail: impl Into<String>) -> PacketError {
    PacketError {
        detail: format!("sweep: {}", detail.into()),
    }
}

impl SweepDecl {
    /// Strict decode — `sweep{…}`, every member required, unknown
    /// members refuse.
    pub fn from_json(j: &Json) -> Result<SweepDecl, PacketError> {
        let m = j.as_obj().ok_or_else(|| serr("must be an object"))?;
        for k in m.keys() {
            if ![
                "schema",
                "registry_sha256",
                "level_id",
                "level_name",
                "factor_id",
                "level_ref",
                "baseline",
                "cohort_id",
                "departure",
                "label",
            ]
            .contains(&k.as_str())
            {
                return Err(serr(format!("unknown member {k}")));
            }
        }
        if m.get("schema").and_then(Json::as_str) != Some(SWEEP_SCHEMA) {
            return Err(serr("schema is not hh-runner-sweep/1"));
        }
        let need = |k: &str| -> Result<String, PacketError> {
            m.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| serr(format!("{k} required")))
        };
        let bm = m
            .get("baseline")
            .and_then(JsonExt::as_obj)
            .ok_or_else(|| serr("baseline must be an object"))?;
        for k in bm.keys() {
            if !["participant_ref", "version", "digest"].contains(&k.as_str()) {
                return Err(serr(format!("baseline: unknown member {k}")));
            }
        }
        let bneed = |k: &str| -> Result<String, PacketError> {
            bm.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| serr(format!("baseline.{k} required")))
        };
        Ok(SweepDecl {
            registry_sha256: need("registry_sha256")?,
            level_id: need("level_id")?,
            level_name: need("level_name")?,
            factor_id: need("factor_id")?,
            level_ref: need("level_ref")?,
            baseline: L1Identity {
                participant_ref: bneed("participant_ref")?,
                version: bneed("version")?,
                digest: bneed("digest")?,
            },
            cohort_id: need("cohort_id")?,
            departure: need("departure")?,
            label: need("label")?,
        })
    }

    /// The baseline identity in the `hir` spelling `l1_gate_match`
    /// reads — the same three members `h_identity_of` projects
    /// (`bundle_id`, `definition_version_id`, `kernel_binary_sha256`):
    /// one identity read discipline (CC1), the pass id's preimage is
    /// the exact H swept.
    pub fn baseline_hir(&self) -> Json {
        Json::obj([
            ("bundle_id", Json::str(&self.baseline.participant_ref)),
            ("definition_version_id", Json::str(&self.baseline.version)),
            ("kernel_binary_sha256", Json::str(&self.baseline.digest)),
        ])
    }
}

/// `check_sweep_decl(decl)` — the record-level checks before a V5 arm
/// is armed: the cohort recomputes over the declared baseline +
/// level_ref (REQ-XD-220's cohort identity), the departure member
/// cites AR-33, and the REQ-XH-115 label is the verbatim recorded
/// string (X-08 — no paraphrase, no parity claim).
pub fn check_sweep_decl(decl: &SweepDecl) -> Vec<String> {
    let mut out = Vec::new();
    if phase4_cohort_id(&decl.baseline, &decl.level_ref) != decl.cohort_id {
        out.push(format!(
            "cohort_id does not recompute over baseline+level_ref — an H change is a new phase-4 cohort"
        ));
    }
    if !decl.departure.contains("AR-33") {
        out.push("departure does not cite AR-33 — the concurrency-with-L2 departure is admitted only as AR-33 recorded it".to_string());
    }
    if decl.label != hharness_lab::sweep::SWEEP_VARIANT_LABEL {
        out.push("label is not the REQ-XH-115 verbatim string".to_string());
    }
    out
}

/// `sweep_arm_ok(decl, gate, hir)` — the V5 slot's schedule gate:
/// the slot names its document (`definition_version_id` = the sealed
/// `level_ref`, `level_assignment` = `{factor → level}`), the cohort
/// recomputes, and the declared L1 pass id binds the exact baseline H
/// swept — `l1_gate_match` recomputes the id over that identity (any
/// other H's pass refuses).
pub fn sweep_arm_ok(decl: &SweepDecl, gate: &L1GateDecl, hir: &Json) -> bool {
    if hir.get("definition_version_id").and_then(Json::as_str) != Some(decl.level_ref.as_str()) {
        return false;
    }
    let assigned = hir
        .get("level_assignment")
        .and_then(JsonExt::as_obj)
        .and_then(|m: &BTreeMap<String, Json>| m.get(&decl.factor_id))
        .and_then(Json::as_str);
    if assigned != Some(decl.level_id.as_str()) {
        return false;
    }
    if !check_sweep_decl(decl).is_empty() {
        return false;
    }
    l1_gate_match(gate, &decl.baseline_hir())
}
