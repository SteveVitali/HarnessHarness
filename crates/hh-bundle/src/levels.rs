//! `max_supported_level` — the **closed** `B-R*` requirement set over a
//! `BundleManifest` (§5h.3 §2 `reproduce` basis; ADR-0140 D2). One
//! evaluator, used twice: `bundle()` derives the level it stamps, and
//! `validate_bundle` S4 re-derives it from the decoded manifest — a
//! disagreement is `unsupported_level`. Levels are *not* cumulative:
//! R3's set stands alone (it is the unpinned-model compare level —
//! requiring `B-R2-model` would defeat it).
//!
//! Satisfaction rules (ADR-0139 R-ID-1/2/3/6):
//! - `satisfied_by` names a *member* (`members[]` with `status ∈
//!   {present, fetch}` — a declared ref satisfies; only `redacted`/`gc`/
//!   absent fails);
//! - `unpinned` satisfies nothing above R0;
//! - `n/a{class}` for hosted-class requirements is neither blocking nor
//!   counted ("R1/R2 `n/a{class}`, never 0" — the basis reports it);
//! - claims never satisfy anything above R0.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::manifest::{BasisSatisfaction, BundleManifest, LevelBasis, MemberStatus, ReproLevel};

/// Member roles the basis pins to (the `hh-bundle/1` role vocabulary —
/// part of the schema contract).
pub mod roles {
    /// The subject-section document.
    pub const SUBJECT: &str = "subject";
    /// The sealed definition bytes.
    pub const DEFINITION: &str = "definition";
    /// The `EnvironmentRecord` document.
    pub const ENVIRONMENT: &str = "environment";
    /// The `CompiledBundle` bytes.
    pub const COMPILED_BUNDLE: &str = "compiled_bundle";
    /// The `model` section document.
    pub const MODEL: &str = "model";
    /// The `nondeterminism` declaration document.
    pub const NONDETERMINISM: &str = "nondeterminism";
    /// The `instrument` section document.
    pub const INSTRUMENT: &str = "instrument";
    /// The `configuration` section document.
    pub const CONFIGURATION: &str = "configuration";
    /// The `resolved_dependencies` section document.
    pub const RESOLVED_DEPENDENCIES: &str = "resolved_dependencies";
    /// The `results` section document.
    pub const RESULTS: &str = "results";
    /// The `LedgerExport` page/tree member prefix.
    pub const LEDGER_TREE: &str = "ledger_tree:";
    /// The `LedgerExport` page member prefix.
    pub const LEDGER_PAGE: &str = "ledger_page:";
}

/// The address a role's member carries — `None` when the role is absent
/// or its only entries are `redacted`/`gc`.
fn member_addr(manifest: &BundleManifest, role: &str) -> Option<String> {
    manifest
        .members
        .iter()
        .find(|m| {
            m.role == role && !matches!(m.status, MemberStatus::Redacted | MemberStatus::Gc)
        })
        .map(|m| m.address.clone())
}

/// `traces` covers every subject run → the tree address of the first
/// (kind = run → exactly one subject).
fn traces_tree(manifest: &BundleManifest) -> Option<String> {
    manifest
        .subject
        .run_ids
        .first()
        .and_then(|r| manifest.traces.get(r))
        .map(|t| t.tree.clone())
}

/// Every subject run has a `LedgerExport` with pages.
fn traces_complete(manifest: &BundleManifest) -> bool {
    !manifest.subject.run_ids.is_empty()
        && manifest
            .subject
            .run_ids
            .iter()
            .all(|r| manifest.traces.contains_key(r))
}

/// The model section's snapshots.
fn snapshots(manifest: &BundleManifest) -> Vec<Json> {
    match manifest.model.get("snapshots") {
        Some(Json::Arr(s)) => s.clone(),
        _ => Vec::new(),
    }
}

/// The configuration section's member reference, when present.
fn section_member(section: &Json) -> Option<String> {
    section
        .get("member")
        .and_then(Json::as_str)
        .map(String::from)
}

/// Derive `basis[]` + `max_supported_level` for `manifest`.
///
/// `replay_declared` — whether the subject run's ledger actually carries
/// the declared-replay control rows (`control.decision`); the assembler
/// knows this from the live store, `validate_bundle` derives it from the
/// decoded pages.
pub fn derive(
    manifest: &BundleManifest,
    replay_declared: bool,
) -> (ReproLevel, Vec<LevelBasis>) {
    let hosted = manifest.participant_class == "hosted";
    let dirty = manifest.instrument.get("dirty") == Some(&Json::Bool(true));
    let open = manifest.subject.status == "open";
    let mut entries: Vec<LevelBasis> = Vec::new();
    let mut push = |level: ReproLevel, id: &str, sat: BasisSatisfaction| {
        entries.push(LevelBasis {
            level,
            requirement_id: id.to_string(),
            satisfied_by: sat,
        });
    };
    let member = |addr: Option<String>| match addr {
        Some(a) => BasisSatisfaction::Member(a),
        None => BasisSatisfaction::Missing,
    };

    // ── R0 ───────────────────────────────────────────────────────────
    if traces_complete(manifest) {
        push(
            ReproLevel::R0,
            "B-R0-traces",
            member(traces_tree(manifest)),
        );
    } else {
        push(ReproLevel::R0, "B-R0-traces", BasisSatisfaction::Missing);
    }
    push(
        ReproLevel::R0,
        "B-R0-complete",
        member(member_addr(manifest, roles::SUBJECT)),
    );
    push(
        ReproLevel::R0,
        "B-R0-instrument",
        member(member_addr(manifest, roles::INSTRUMENT)),
    );

    // ── R1 ───────────────────────────────────────────────────────────
    push(
        ReproLevel::R1,
        "B-R1-canonical",
        member(member_addr(manifest, roles::SUBJECT)),
    );
    let compiled_ref = manifest
        .definition
        .get("compiled_bundle")
        .and_then(|c| c.get("member"))
        .and_then(Json::as_str)
        .map(String::from);
    push(ReproLevel::R1, "B-R1-derivations", member(compiled_ref));
    if hosted {
        push(
            ReproLevel::R1,
            "B-R1-replay",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
    } else if replay_declared {
        push(
            ReproLevel::R1,
            "B-R1-replay",
            member(traces_tree(manifest)),
        );
    } else {
        push(ReproLevel::R1, "B-R1-replay", BasisSatisfaction::Missing);
    }

    // ── R2 ───────────────────────────────────────────────────────────
    let snaps = snapshots(manifest);
    let all_pinned = !snaps.is_empty()
        && snaps
            .iter()
            .all(|s| s.get("pinned") == Some(&Json::Bool(true)));
    let env_foreign = manifest
        .unpinned
        .iter()
        .any(|u| u.role == "environment" && u.reason == "foreign_only");
    let registry_snap = manifest
        .resolved_dependencies
        .get("registry_snapshot_id")
        .filter(|r| !matches!(r, Json::Null))
        .and_then(Json::as_str)
        .is_some();
    let seed_present = !matches!(
        manifest.configuration.get("seed"),
        None | Some(Json::Null)
    );
    let nd_present = manifest
        .reproducibility
        .get("nondeterminism")
        .and_then(Json::as_str)
        .is_some()
        || member_addr(manifest, roles::NONDETERMINISM).is_some();
    if hosted {
        push(
            ReproLevel::R2,
            "B-R2-model",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
        push(
            ReproLevel::R2,
            "B-R2-environment",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
        push(
            ReproLevel::R2,
            "B-R2-closure",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
        push(
            ReproLevel::R2,
            "B-R2-seed",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
        push(
            ReproLevel::R2,
            "B-R2-nondeterminism",
            BasisSatisfaction::NotApplicable("hosted".into()),
        );
        // Hosted R2 exists only with `end_state` — the member is absent
        // at this stage.
        push(
            ReproLevel::R2,
            "B-R2-end_state",
            BasisSatisfaction::Missing,
        );
    } else {
        push(
            ReproLevel::R2,
            "B-R2-model",
            if all_pinned {
                member(member_addr(manifest, roles::MODEL))
            } else {
                BasisSatisfaction::Missing
            },
        );
        push(
            ReproLevel::R2,
            "B-R2-environment",
            if env_foreign {
                BasisSatisfaction::Missing
            } else {
                member(member_addr(manifest, roles::ENVIRONMENT))
            },
        );
        push(
            ReproLevel::R2,
            "B-R2-closure",
            if registry_snap {
                member(member_addr(manifest, roles::RESOLVED_DEPENDENCIES))
            } else {
                BasisSatisfaction::Missing
            },
        );
        push(
            ReproLevel::R2,
            "B-R2-seed",
            if seed_present {
                member(member_addr(manifest, roles::CONFIGURATION))
            } else {
                BasisSatisfaction::Missing
            },
        );
        push(
            ReproLevel::R2,
            "B-R2-nondeterminism",
            if nd_present {
                member(member_addr(manifest, roles::NONDETERMINISM))
            } else {
                BasisSatisfaction::Missing
            },
        );
    }

    // ── R3 ───────────────────────────────────────────────────────────
    let budgeted = !matches!(
        manifest.configuration.get("budget"),
        None | Some(Json::Null)
    ) && manifest.configuration.get("budget")
        != Some(&Json::obj([("budgeted", Json::Bool(false))]));
    let all_fingerprinted = !snaps.is_empty()
        && snaps
            .iter()
            .all(|s| s.get("observed_fingerprint").and_then(Json::as_str).is_some());
    push(
        ReproLevel::R3,
        "B-R3-budget",
        if budgeted {
            member(member_addr(manifest, roles::CONFIGURATION))
        } else {
            BasisSatisfaction::Missing
        },
    );
    push(
        ReproLevel::R3,
        "B-R3-fingerprint",
        if all_fingerprinted {
            member(member_addr(manifest, roles::MODEL))
        } else {
            BasisSatisfaction::Missing
        },
    );
    push(
        ReproLevel::R3,
        "B-R3-design",
        member(member_addr(manifest, roles::SUBJECT)),
    );

    // The level: highest whose every requirement is satisfied or n/a.
    let mut max = ReproLevel::R0;
    for level in [ReproLevel::R1, ReproLevel::R2, ReproLevel::R3] {
        let ok = entries
            .iter()
            .filter(|e| e.level == level)
            .all(|e| !matches!(e.satisfied_by, BasisSatisfaction::Missing));
        if ok {
            max = level;
        }
    }
    // Caps: `InstrumentRecord.dirty` ⇒ R0; an open (non-durable) run ⇒ R0.
    if dirty || open {
        max = ReproLevel::R0;
    }
    (max, entries)
}

/// The declared `max_supported_level` the manifest carries (for S4
/// comparison); `None` when the section is absent.
pub fn declared_max(manifest: &BundleManifest) -> Option<ReproLevel> {
    manifest
        .reproducibility
        .get("max_supported_level")
        .and_then(Json::as_str)
        .and_then(ReproLevel::parse)
}

/// The declared `claimed_level`; `None` when absent.
pub fn declared_claimed(manifest: &BundleManifest) -> Option<ReproLevel> {
    manifest
        .reproducibility
        .get("claimed_level")
        .and_then(Json::as_str)
        .and_then(ReproLevel::parse)
}

/// The declared `basis[]` requirement ids → satisfied_by, for the S4
/// per-entry comparison (a manifest whose basis names a member that
/// doesn't exist fails `manifest_reference_unresolved`).
pub fn declared_basis(manifest: &BundleManifest) -> BTreeMap<String, Json> {
    let mut out = BTreeMap::new();
    if let Some(Json::Arr(bs)) = manifest.reproducibility.get("basis") {
        for b in bs {
            if let Some(id) = b.get("requirement_id").and_then(Json::as_str) {
                out.insert(
                    id.to_string(),
                    b.get("satisfied_by").cloned().unwrap_or(Json::Null),
                );
            }
        }
    }
    out
}

/// The section-doc "member" backrefs a manifest carries (S3 walks them).
pub fn section_member_refs(manifest: &BundleManifest) -> Vec<String> {
    let mut refs = Vec::new();
    for section in [
        &manifest.definition,
        &manifest.configuration,
        &manifest.resolved_dependencies,
        &manifest.model,
        &manifest.instrument,
        &manifest.results,
    ] {
        if let Some(a) = section_member(section) {
            refs.push(a);
        }
    }
    if let Some(a) = manifest
        .reproducibility
        .get("nondeterminism")
        .and_then(Json::as_str)
    {
        refs.push(a.to_string());
    }
    if let Some(a) = manifest
        .definition
        .get("compiled_bundle")
        .and_then(|c| c.get("member"))
        .and_then(Json::as_str)
    {
        refs.push(a.to_string());
    }
    for t in manifest.traces.values() {
        refs.push(t.tree.clone());
        refs.extend(t.pages.iter().cloned());
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{MemberRef, SubjectSection};

    fn base_manifest() -> BundleManifest {
        BundleManifest {
            bundle_kind: "run".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            producer: Json::Null,
            participant_class: "native".into(),
            observability_levels: vec![],
            claims: vec![],
            name_bindings: vec![],
            fetch_policy: "self_contained".into(),
            fetch: vec![],
            subject: SubjectSection {
                run_ids: vec!["r1".into()],
                heads: BTreeMap::new(),
                lineage: vec![],
                watermarks: BTreeMap::new(),
                status: "finished".into(),
            },
            definition: Json::Null,
            configuration: Json::obj([
                ("seed", Json::Int(7)),
                ("budget", Json::obj([("budgeted", Json::Bool(true))])),
                ("member", Json::str("sha256:cfg")),
            ]),
            resolved_dependencies: Json::obj([
                ("registry_snapshot_id", Json::str("sha256:reg")),
                ("member", Json::str("sha256:deps")),
            ]),
            model: Json::obj([("snapshots", Json::Arr(vec![Json::obj([
                ("pinned", Json::Bool(true)),
                ("observed_fingerprint", Json::str("sha256:fp")),
            ])]))]),
            instrument: Json::obj([("dirty", Json::Bool(false))]),
            traces: {
                let mut t = BTreeMap::new();
                t.insert(
                    "r1".to_string(),
                    crate::manifest::LedgerExport {
                        run_id: "r1".into(),
                        page_size: 64,
                        pages: vec!["sha256:p0".into()],
                        tree: "sha256:tree".into(),
                        checkpoints: vec![],
                        blob_index: vec![],
                        head: Json::Null,
                        lineage_prefixes: vec![],
                    },
                );
                t
            },
            results: Json::Null,
            reproducibility: Json::obj([("nondeterminism", Json::str("sha256:nd"))]),
            members: vec![
                MemberRef::present(roles::SUBJECT, "sha256:sub".into(), "", 1),
                MemberRef::present(roles::INSTRUMENT, "sha256:inst".into(), "", 1),
                MemberRef::present(roles::MODEL, "sha256:model".into(), "", 1),
                MemberRef::present(roles::CONFIGURATION, "sha256:cfg".into(), "", 1),
                MemberRef::present(
                    roles::RESOLVED_DEPENDENCIES,
                    "sha256:deps".into(),
                    "",
                    1,
                ),
                MemberRef::present(roles::ENVIRONMENT, "sha256:env".into(), "", 1),
                MemberRef::present(roles::NONDETERMINISM, "sha256:nd".into(), "", 1),
            ],
            unpinned: vec![],
            ext: BTreeMap::new(),
            version_id: String::new(),
        }
    }

    #[test]
    fn fully_pinned_derives_r3() {
        let (max, basis) = derive(&base_manifest(), true);
        // B-R1-derivations is missing (no compiled bundle) → R1 fails;
        // R2 and R3 stand alone and pass.
        assert_eq!(max, ReproLevel::R3);
        let r1 = basis
            .iter()
            .find(|b| b.requirement_id == "B-R1-derivations")
            .unwrap();
        assert!(matches!(r1.satisfied_by, BasisSatisfaction::Missing));
    }

    #[test]
    fn dirty_caps_r0() {
        let mut m = base_manifest();
        if let Json::Obj(i) = &mut m.instrument {
            i.insert("dirty".into(), Json::Bool(true));
        }
        let (max, _) = derive(&m, true);
        assert_eq!(max, ReproLevel::R0);
    }

    #[test]
    fn open_caps_r0() {
        let mut m = base_manifest();
        m.subject.status = "open".into();
        let (max, _) = derive(&m, true);
        assert_eq!(max, ReproLevel::R0);
    }

    #[test]
    fn hosted_na_not_zero() {
        let mut m = base_manifest();
        m.participant_class = "hosted".into();
        let (max, basis) = derive(&m, false);
        let nas = basis
            .iter()
            .filter(|b| matches!(b.satisfied_by, BasisSatisfaction::NotApplicable(_)))
            .count();
        assert!(nas >= 5, "hosted R1/R2 requirements report n/a: {nas}");
        // B-R2-end_state missing → R2 fails; R3 passes (budget +
        // fingerprint + design) — so max is R3 at this fixture, ≥ R0.
        assert!(max >= ReproLevel::R0);
        assert_eq!(max, ReproLevel::R3);
    }

    #[test]
    fn unpinned_model_drops_r2_keeps_r3() {
        let mut m = base_manifest();
        m.model = Json::obj([(
            "snapshots",
            Json::Arr(vec![Json::obj([
                ("pinned", Json::Bool(false)),
                ("observed_fingerprint", Json::str("sha256:fp")),
            ])]),
        )]);
        let (max, basis) = derive(&m, true);
        assert_eq!(max, ReproLevel::R3);
        let r2m = basis
            .iter()
            .find(|b| b.requirement_id == "B-R2-model")
            .unwrap();
        assert!(matches!(r2m.satisfied_by, BasisSatisfaction::Missing));
    }
}
