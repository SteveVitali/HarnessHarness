//! The deterministic golden corpus for AC-R-2.10.2-1: **3 classes × 3 variants**
//! across the **2 namespaces**, with **one yanked** name and **one revoked**
//! version, frozen by a `registry_snapshot_id`. `build` constructs the corpus in
//! a fresh store (deterministic — every id derives from canonical content, never
//! from a wall clock); `naive_resolve` is the *second, independent*
//! implementation the AC compares against (it reads the persisted log lines
//! directly, bypassing `RegistryStore::resolve`).
//!
//! Every builder is content-only: no clocks, no randomness — the same corpus
//! always mints the same `version_id`s, which is what makes the byte-equal
//! comparison meaningful.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use hh_hir::leaves::Text;
use hh_identity::idp::address;
use hh_identity::names::{NameStatus, ResolveMode};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::kinds::{Cardinality, Placement};
use crate::records::{
    AppliesTo, ClassRecord, Implementation, ParamDecl, RegistryRecord, VariantRecord,
};
use crate::store::{
    QueryClause, QueryOp, QueryPredicate, RegistryStore, ResolveInput, ResolveRequest,
    SlotConstraints,
};

/// The kernel registrar (first-party — `hh/` writes, `resolved` admissions).
pub fn kernel_registrar() -> ProvenanceRecord {
    ProvenanceRecord::kernel("registry.corpus", 0)
}

/// The run's principal registrar (`local/` writes).
pub fn principal_registrar() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        hh_provenance::Origin::human("principal.corpus", hh_provenance::HumanRole::Principal),
        hh_provenance::PersistenceScope::Run,
        0,
    )
}

fn text(s: &str) -> Text {
    Text::new(s, "corpus", kernel_registrar())
}

fn fake_impl(tag: &str) -> Implementation {
    Implementation {
        content: address(
            format!("impl-body-{tag}").as_bytes(),
            "application/octet-stream",
        ),
        placement: Placement::SubprocessConfined,
        host_requirements: Json::Null,
    }
}

fn decl(extra: &[(&str, bool)]) -> BTreeMap<String, Json> {
    let mut d = BTreeMap::new();
    d.insert("deterministic".to_string(), Json::Bool(true));
    for (k, v) in extra {
        d.insert(k.to_string(), Json::Bool(*v));
    }
    d
}

fn variant(
    class_version_id: &str,
    variant_id: &str,
    extra_decl: &[(&str, bool)],
    placement: Placement,
) -> VariantRecord {
    VariantRecord {
        variant_id: variant_id.to_string(),
        class_ref: class_version_id.to_string(),
        contract_range: "1.0-1.9".to_string(),
        version_label: Some("1.0.0".to_string()),
        param_schema: BTreeMap::from([(
            "depth".to_string(),
            ParamDecl {
                value_type: "int".to_string(),
                domain: Some(Json::Arr(vec![Json::Int(1), Json::Int(2), Json::Int(4)])),
                default: Some(Json::Int(1)),
                unit: None,
                sweepable: true,
                budget_relevant: true,
                affects: vec![],
            },
        )]),
        implementation: Implementation {
            content: fake_impl(variant_id).content,
            placement,
            host_requirements: Json::Null,
        },
        capability_declaration: decl(extra_decl),
        conditioned_rules: vec![],
        applies_to: AppliesTo {
            participant_classes: BTreeSet::from(["native".to_string()]),
            families: vec![],
        },
        declared_costs: None,
        summary: text(&format!("variant {variant_id}")),
        dialect_range: "registry/1".to_string(),
    }
}

fn extra_class(class_id: &str) -> ClassRecord {
    ClassRecord {
        class_id: class_id.to_string(),
        contract: vec![crate::records::ContractOperation {
            name: "run".to_string(),
            inputs: Json::obj([("ctx", Json::str("context"))]),
            outputs: Json::obj([("out", Json::str("result"))]),
            invariants: vec!["deterministic".to_string()],
            failure_modes: vec!["refuse".to_string()],
        }],
        cardinality: Cardinality::ExactlyOne,
        required_inputs: BTreeSet::from([
            "ModelProfile".to_string(),
            "ResourceAccount".to_string(),
        ]),
        base_param_schema: BTreeMap::new(),
        hot_path: false,
        dialect_introduced: "registry/1".to_string(),
        contract_version: "1.0".to_string(),
        home: "kernel".to_string(),
        declaration_schema: Json::obj([
            ("additionalProperties", Json::Bool(false)),
            (
                "properties",
                Json::obj([
                    ("deterministic", Json::Null),
                    ("supports_export", Json::Null),
                ]),
            ),
            ("required", Json::Arr(vec![Json::str("deterministic")])),
        ]),
        conformance_suite_ref: None,
        decision_points: vec![],
        metrics_declared: vec![],
        slot_key: class_id.to_string(),
        tier: "C0".to_string(),
    }
}

/// The corpus' shape: class ids in registration order.
pub const CLASS_IDS: [&str; 3] = ["control_strategy", "context_policy", "export_sink"];
/// Variants per class.
pub const VARIANTS_PER_CLASS: usize = 3;

/// Build the corpus in the store at `dir`. Returns `(snapshot_id, variant_version_ids)`
/// — the ids the byte-equal assertions resolve.
///
/// Layout: `control_strategy` + `context_policy` get their real class/suite
/// records (suites registered, `conformance_suite_ref` back-patched by a second
/// class version — the class *upgrade* path); `export_sink` exercises a third
/// class with the closed schema only. Variants publish across `hh/` (kernel) and
/// `local/` (principal); `hh/context_policy/faulty` is **yanked**, and the third
/// `export_sink` variant is **revoked** — the AC's one-yanked-one-revoked pair.
pub fn build(dir: &Path) -> Result<(String, Vec<String>), RegistryError> {
    let kernel = kernel_registrar();
    let principal = principal_registrar();
    let mut store = RegistryStore::open(dir, &kernel)?;
    // Policy floor: `declared` — variants carry declarations, so the floor passes.
    store.set_policy(crate::records::RegistryPolicy::stage1_default())?;

    // ── classes + suites ──
    let mut class_vids = Vec::new();
    for class in [
        crate::suites::control_strategy_class(),
        crate::suites::context_policy_class(),
        extra_class("export_sink"),
    ] {
        let cid = class.class_id.clone();
        let v = store
            .register(RegistryRecord::Class(class), &kernel, None)?
            .version_id;
        // Pin the suite into a *new* class version (the upgrade path — the class
        // record carrying conformance_suite_ref supersedes the suite-less one).
        let suite = match cid.as_str() {
            "control_strategy" => Some(crate::suites::control_strategy_suite(&v)),
            "context_policy" => Some(crate::suites::context_policy_suite(&v)),
            _ => None,
        };
        if let Some(mut s) = suite {
            s.class_ref = v.clone();
            let sv = store
                .register(RegistryRecord::Suite(s), &kernel, None)?
                .version_id;
            let mut upgraded = match store.get(&v).map(|(_, r)| r.clone()) {
                Some(RegistryRecord::Class(c)) => c,
                _ => unreachable!(),
            };
            upgraded.conformance_suite_ref = Some(sv);
            let v2 = store
                .register(RegistryRecord::Class(upgraded), &kernel, None)?
                .version_id;
            class_vids.push(v2);
        } else {
            class_vids.push(v);
        }
    }

    // ── variants: 3 per class, mixed namespaces ──
    let mut variant_vids = Vec::new();
    for (ci, class_vid) in class_vids.iter().enumerate() {
        for vi in 0..VARIANTS_PER_CLASS {
            let vid_tag = format!("{}-v{}", CLASS_IDS[ci], vi);
            let placement = match vi % 3 {
                0 => Placement::SubprocessConfined,
                1 => Placement::Container,
                _ => Placement::Remote,
            };
            let extra: Vec<(&str, bool)> = match CLASS_IDS[ci] {
                "control_strategy" => vec![("supports_fallback", vi > 0)],
                "context_policy" => vec![("supports_redaction", vi > 0)],
                _ => vec![("supports_export", true)],
            };
            let v = variant(class_vid, &vid_tag, &extra, placement);
            let version_id = store
                .register(RegistryRecord::Variant(v), &kernel, None)?
                .version_id;
            variant_vids.push(version_id.clone());
            // Alternate namespaces: hh/ under kernel, local/ under principal.
            let (ns, reg) = if vi % 2 == 0 {
                ("hh", &kernel)
            } else {
                ("local", &principal)
            };
            store.publish(
                ns,
                &format!("{}/{}", CLASS_IDS[ci], vid_tag),
                &version_id,
                Some(format!("1.0.{vi}")),
                None,
                reg,
            )?;
        }
    }

    // ── one yanked name + one revoked version ──
    // The yanked entry binds a NEWER version than the head it supersedes — the
    // execute fallback then visibly returns the older binding (v0 under v1).
    store.publish(
        "hh",
        "control_strategy/control_strategy-v0",
        &variant_vids[1],
        Some("1.0.1".to_string()),
        Some(variant_vids[0].clone()),
        &kernel,
    )?;
    store.set_name_status(
        "hh",
        "control_strategy/control_strategy-v0",
        NameStatus::Yanked,
        &kernel,
    )?;
    store.revoke(
        &variant_vids[8], // third export_sink variant
        hh_identity::supersede::SupersedeReason::Revocation,
        &kernel,
        None,
    )?;

    // ── the snapshot that freezes the corpus ──
    let snap = store.snapshot(&kernel)?;
    Ok((snap.snapshot_id, variant_vids))
}

/// The AC-1 output bundle for one store: the canonical bytes of `resolve` (one
/// pinned + one selector per class), `query` (all variants) and `slot_choices`
/// (per class) — everything over the one snapshot.
pub fn outputs(
    store: &RegistryStore,
    snapshot_id: &str,
    variant_vids: &[String],
) -> Result<Vec<u8>, RegistryError> {
    let mut out = Vec::new();
    for vid in variant_vids {
        let r = store.resolve(
            &ResolveInput::Version(vid.clone()),
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )?;
        out.push(resolved_json(&r));
    }
    for (ns, name) in [
        ("hh", "control_strategy/control_strategy-v0"),
        ("local", "control_strategy/control_strategy-v1"),
        ("hh", "context_policy/context_policy-v0"),
        ("local", "context_policy/context_policy-v1"),
        ("hh", "export_sink/export_sink-v0"),
        ("local", "export_sink/export_sink-v1"),
    ] {
        match store.resolve(
            &ResolveInput::Selector {
                namespace: ns.to_string(),
                name: name.to_string(),
                label: None,
                snapshot_id: Some(snapshot_id.to_string()),
            },
            ResolveMode::Execute,
            &ResolveRequest::default(),
        ) {
            Ok(r) => out.push(resolved_json(&r)),
            Err(e) => out.push(Json::obj([("error", Json::str(e.reason()))])),
        }
    }
    let rows = store.query(&QueryPredicate {
        clauses: vec![QueryClause {
            field: "kind".to_string(),
            op: QueryOp::Eq,
            value: "variant".to_string(),
        }],
        snapshot_id: Some(snapshot_id.to_string()),
    })?;
    out.push(Json::Arr(rows.iter().map(resolved_json).collect()));
    for cid in CLASS_IDS {
        match store.slot_choices(
            cid,
            &SlotConstraints {
                supports: BTreeSet::new(),
                contract_version: Some("1.0".to_string()),
                conformance_floor: None,
                snapshot_id: Some(snapshot_id.to_string()),
            },
        ) {
            Ok(choices) => out.push(Json::Arr(
                choices
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("variant_id", Json::str(c.variant_id.clone())),
                            ("version_id", Json::str(c.version_id.clone())),
                        ])
                    })
                    .collect(),
            )),
            Err(e) => out.push(Json::obj([("error", Json::str(e.reason()))])),
        }
    }
    Ok(Json::Arr(out).to_canonical_string().into_bytes())
}

fn resolved_json(r: &crate::store::ResolvedRecord) -> Json {
    Json::obj([
        ("version_id", Json::str(r.versioned_ref.version_id.clone())),
        (
            "semantic_id",
            r.versioned_ref
                .semantic_id
                .clone()
                .map_or(Json::Null, Json::Str),
        ),
        ("admission", Json::str(r.admission.as_str())),
        (
            "name_status",
            r.name_status
                .map(|s| Json::str(crate::schema::name_status_str(s)))
                .unwrap_or(Json::Null),
        ),
        (
            "depends_on_revoked",
            Json::Arr(
                r.depends_on_revoked
                    .iter()
                    .map(|s| Json::str(s.revoked_member.clone()))
                    .collect(),
            ),
        ),
    ])
}

/// The *second, independent* `resolve` — reads the persisted log directly and
/// computes the name head by line order, bypassing `RegistryStore` entirely
/// (AC-1: two implementations, byte-equal results over one snapshot).
pub fn naive_resolve(
    dir: &Path,
    snapshot_id: &str,
    namespace: &str,
    name: &str,
) -> Result<Option<String>, RegistryError> {
    let bytes =
        std::fs::read(dir.join("registry.json")).map_err(|e| RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: format!("{e}"),
        })?;
    let text = std::str::from_utf8(&bytes).map_err(|e| RegistryError::SchemaViolation {
        path: "log".to_string(),
        detail: format!("{e}"),
    })?;
    let j = hh_wire::json::parse(text).map_err(|e| RegistryError::SchemaViolation {
        path: "log".to_string(),
        detail: format!("{e:?}"),
    })?;
    let Json::Arr(lines) = j else {
        return Err(RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: "expected array".to_string(),
        });
    };
    // The snapshot's binding for the name.
    let mut binding: Option<String> = None;
    for l in &lines {
        if l.get("type").and_then(|t| t.as_str()) != Some("record") {
            continue;
        }
        let env = l.get("envelope").cloned().unwrap_or(Json::Null);
        if env.get("kind").and_then(|k| k.as_str()) != Some("registry_snapshot") {
            continue;
        }
        if env.get("version_id").and_then(|v| v.as_str()) != Some(snapshot_id) {
            continue;
        }
        let body = l.get("body").cloned().unwrap_or(Json::Null);
        if let Some(Json::Obj(bindings)) = body.get("name_bindings") {
            binding = bindings
                .get(&format!("{namespace}/{name}"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }
    Ok(binding)
}
