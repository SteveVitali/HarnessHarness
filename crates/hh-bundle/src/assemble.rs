//! `bundle(scope, kind = run, policy)` — the layer-M assembly pass over
//! the stored projection (§5h.3 §2). Reads `hh-ledger` through `Store`
//! only; takes the compiled artefact, environment record, model
//! snapshots, and instrument fields as *inputs* (the caller owns those
//! planes — kernel, env, catalog). Writes nothing: `bundle` is a read.

use std::collections::BTreeMap;

use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::error::BundleError;
use crate::export::{add_member, build_ledger_export, MemberBytes};
use crate::levels;
use crate::manifest::{
    BundleManifest, BundlePolicy, MemberRef, MemberStatus, SubjectSection, Unpinned,
};

/// A compiled-bundle member the caller produced (`hh-compiler` output).
#[derive(Debug, Clone)]
pub struct CompileOutcome {
    /// `CompiledBundle.bundle_id`.
    pub bundle_id: String,
    /// `CompiledBundle.derivation_key`.
    pub derivation_key: String,
    /// The canonical `CompiledBundle` bytes.
    pub member_bytes: Vec<u8>,
    /// The LCD report (recorded in the definition section).
    pub lcd_report: Json,
    /// The opacity report.
    pub opacity_report: Json,
}

/// Everything `assemble` needs from its caller's planes.
pub struct AssembleInputs<'a> {
    /// The ledger store (read-only).
    pub store: &'a Store,
    /// The subject run.
    pub run_id: &'a str,
    /// The bundle policy.
    pub policy: &'a BundlePolicy,
    /// RFC 3339 ms creation stamp.
    pub created_at: String,
    /// The producer `ProvenanceRecord` (`kernel(hh.bundle)`).
    pub producer: Json,
    /// The `ContractIdentity` — recorded into
    /// `instrument.component_versions` (R-3.1: contracts ride in bundles).
    pub contract_identity: Json,
    /// The kernel version ref (`InstrumentRecord.version`).
    pub kernel_version_id: String,
    /// `InstrumentRecord.dirty` — `true` caps `max_supported_level` at R0.
    pub instrument_dirty: bool,
    /// Sealed-artifact reader (`artifacts/<version_id>` → bytes).
    pub artifact_bytes: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    /// The `EnvironmentRecord` (canonical JSON) the run executed under.
    pub environment: Json,
    /// The compile output for the run's sealed definition, when the
    /// caller produced one (R1 derivations gate on it).
    pub compiled: Option<CompileOutcome>,
    /// `ModelSnapshotRecord[]` for the run's model members.
    pub model_snapshots: Vec<Json>,
    /// The registry snapshot the run's variants resolved under.
    pub registry_snapshot_id: Option<String>,
    /// The `VariantRecord[]` the sealed definition binds.
    pub variants: Vec<Json>,
    /// The run's `eval_budget` document.
    pub budget: Json,
    /// The run's bound profile document (`profile:none` when none).
    pub profile: Json,
    /// Extra `nondeterminism` declarations (beyond env/model).
    pub nondeterminism: Vec<Json>,
    /// `participant_class` override — the run manifest's class is the
    /// default.
    pub participant_class: Option<String>,
}

/// The assembled bundle: manifest + payload tree (`address → bytes`).
pub struct Assembled {
    /// The `BundleManifest` (`version_id` stamped).
    pub manifest: BundleManifest,
    /// Content-addressed member bytes for every `status = present`
    /// member.
    pub members: MemberBytes,
    /// The assembled loss/claim report rows (the `kernel.bundle`
    /// boundary result carries them).
    pub report_rows: Vec<Json>,
}

/// Scan member payloads for secret material — `bundle`'s own
/// `SecretMaterialPresent` refusal (§5h.3 §2 error column). A
/// `placeholder_passthrough` mark (`tombstone: None`) is the *safe* form —
/// it is not a leak.
fn secret_scan(members: &MemberBytes) -> Result<(), BundleError> {
    let detectors = hh_secrets::DetectorSet::standard(hh_secrets::MaskSet::default());
    for (addr, bytes) in members {
        if let Ok(text) = std::str::from_utf8(bytes) {
            if let Some(hit) = hh_secrets::detect(text, &detectors)
                .iter()
                .find(|h| h.tombstone.is_some())
            {
                return Err(BundleError::SecretMaterialPresent {
                    detail: format!("member {addr} matches detector {}", hit.detector.as_str()),
                });
            }
        }
    }
    Ok(())
}

/// `bundle` — assemble the `hh-bundle/1` manifest + payload tree for
/// `run_id`.
pub fn assemble(inputs: &AssembleInputs<'_>) -> Result<Assembled, BundleError> {
    let store = inputs.store;
    let run_id = inputs.run_id;
    let rm = store
        .manifest(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;

    // ── Layer P: the LedgerExport + its page/tree members ────────────
    let mut members: MemberBytes = BTreeMap::new();
    let (export, page_roles) = build_ledger_export(store, run_id, &mut members)?;
    let _tree_addr = export.tree.clone();
    let replay_declared = store
        .envelopes(run_id)
        .unwrap_or_default()
        .iter()
        .any(|e| e.class == "control.decision");
    let finished = store
        .envelopes(run_id)
        .unwrap_or_default()
        .iter()
        .any(|e| e.class == "lifecycle.run.finished");
    let status = if finished { "finished" } else { "open" }.to_string();

    // ── Section documents (each a member so basis rows can pin them) ─
    let doc_member =
        |members: &mut MemberBytes, role: &str, doc: &Json, index: &mut Vec<MemberRef>| {
            let bytes = doc.to_canonical_string().into_bytes();
            let (addr, size) = add_member(members, bytes, "application/vnd.hh.bundle-doc+json");
            index.push(MemberRef::present(
                role,
                addr.clone(),
                "application/vnd.hh.bundle-doc+json",
                size,
            ));
            addr
        };
    let mut index: Vec<MemberRef> = Vec::new();

    // subject doc — the completeness anchor.
    let subject_doc = Json::obj([
        ("run_ids", Json::Arr(vec![Json::str(run_id)])),
        ("status", Json::str(status.clone())),
    ]);
    let _subject_doc_addr = doc_member(&mut members, "subject", &subject_doc, &mut index);

    // definition member — the sealed definition bytes.
    let def_ref = rm
        .harness_def_ref
        .clone()
        .ok_or_else(|| BundleError::MemberUnavailable {
            address: "harness_def_ref unset".into(),
        })?;
    let def_bytes =
        (inputs.artifact_bytes)(&def_ref).ok_or_else(|| BundleError::MemberUnavailable {
            address: def_ref.clone(),
        })?;
    let (def_addr, def_size) =
        add_member(&mut members, def_bytes, "application/vnd.hh.sealed+json");
    index.push(MemberRef::present(
        "definition",
        def_addr.clone(),
        "application/vnd.hh.sealed+json",
        def_size,
    ));

    // environment member — the EnvironmentRecord.
    let env_bytes = inputs.environment.to_canonical_string().into_bytes();
    let (env_addr, env_size) = add_member(
        &mut members,
        env_bytes,
        "application/vnd.hh.environment+json",
    );
    index.push(MemberRef::present(
        "environment",
        env_addr.clone(),
        "application/vnd.hh.environment+json",
        env_size,
    ));
    // A foreign image → R2 environment is foreign_only.
    let env_foreign = inputs
        .environment
        .get("image")
        .and_then(Json::as_str)
        .filter(|s| !s.starts_with("sha256:"))
        .map(String::from)
        .map(Json::str);

    // compiled bundle member.
    let compiled_member = inputs.compiled.as_ref().map(|c| {
        let (addr, size) = add_member(
            &mut members,
            c.member_bytes.clone(),
            "application/vnd.hh.compiled+json",
        );
        index.push(MemberRef::present(
            "compiled_bundle",
            addr.clone(),
            "application/vnd.hh.compiled+json",
            size,
        ));
        addr
    });

    // model doc member.
    let model_doc = Json::obj([("snapshots", Json::Arr(inputs.model_snapshots.clone()))]);
    let _model_doc_addr = doc_member(&mut members, "model", &model_doc, &mut index);

    // nondeterminism doc member — env + model + caller declarations.
    let mut nd: Vec<Json> = inputs.nondeterminism.clone();
    if let Some(Json::Arr(xs)) = inputs.environment.get("nondeterminism") {
        nd.extend(xs.clone());
    }
    for s in &inputs.model_snapshots {
        if let Some(Json::Arr(xs)) = s.get("nondeterminism") {
            nd.extend(xs.clone());
        }
    }
    let nd_doc = Json::obj([("declarations", Json::Arr(nd))]);
    let nd_addr = doc_member(&mut members, "nondeterminism", &nd_doc, &mut index);

    // instrument doc member — carries ContractIdentity.
    let instrument_doc = Json::obj([
        ("version", Json::str(inputs.kernel_version_id.clone())),
        ("dirty", Json::Bool(inputs.instrument_dirty)),
        ("idp", Json::str("idp/1")),
        (
            "component_versions",
            Json::Arr(vec![inputs.contract_identity.clone()]),
        ),
        ("source_commit", Json::Null),
    ]);
    let _instrument_addr = doc_member(&mut members, "instrument", &instrument_doc, &mut index);

    // configuration doc member.
    let config_doc = Json::obj([
        (
            "configuration_id",
            rm.configuration_id
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "configuration_version_id",
            rm.configuration_version_id
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "seed",
            rm.seed.map(|s| Json::Int(s as i64)).unwrap_or(Json::Null),
        ),
        ("budget", inputs.budget.clone()),
        (
            "budget_node_ref",
            rm.budget.clone().map(Json::str).unwrap_or(Json::Null),
        ),
        ("profile", inputs.profile.clone()),
        ("environment", Json::str(env_addr.clone())),
        ("permissions", Json::Arr(vec![])),
        (
            "policies",
            Json::Arr(
                [
                    rm.envelope_policy_ref.as_ref(),
                    rm.healing_policy_ref.as_ref(),
                    rm.audit_policy_ref.as_ref(),
                ]
                .into_iter()
                .flatten()
                .map(|r| Json::str(r.clone()))
                .collect(),
            ),
        ),
    ]);
    let config_addr = doc_member(&mut members, "configuration", &config_doc, &mut index);

    // resolved_dependencies doc member.
    let deps_doc = Json::obj([
        (
            "registry_snapshot_id",
            inputs
                .registry_snapshot_id
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        ("variants", Json::Arr(inputs.variants.clone())),
        ("tools", Json::Arr(vec![])),
        ("text_leaves", Json::Arr(vec![])),
        ("images", Json::Arr(vec![])),
    ]);
    let _deps_addr = doc_member(&mut members, "resolved_dependencies", &deps_doc, &mut index);

    // results doc member (empty at this stage).
    let results_doc = Json::obj([
        ("rows", Json::Arr(vec![])),
        ("metric_declarations", Json::Arr(vec![])),
        ("oracle_declarations", Json::Arr(vec![])),
    ]);
    let results_addr = doc_member(&mut members, "results", &results_doc, &mut index);

    // ledger page roles into the index.
    for (role, addr, size) in page_roles {
        index.push(MemberRef::present(
            role,
            addr,
            "application/vnd.hh.ledger-page+json",
            size,
        ));
    }

    // ── Secret scan (the `bundle` error column) ──────────────────────
    secret_scan(&members)?;

    // ── subject section ──────────────────────────────────────────────
    let head = store.head(run_id).unwrap_or(hh_ledger::event::Head {
        seq: 0,
        event_id: String::new(),
        hash: String::new(),
    });
    let mut heads = BTreeMap::new();
    heads.insert(
        run_id.to_string(),
        Json::obj([
            ("seq", Json::Int(head.seq as i64)),
            ("event_id", Json::str(head.event_id.clone())),
            ("hash", Json::str(head.hash.clone())),
        ]),
    );
    let mut watermarks = BTreeMap::new();
    watermarks.insert(run_id.to_string(), head.seq);
    let lineage: Vec<Json> = store
        .lineage(run_id)
        .unwrap_or_default()
        .iter()
        .map(|l| {
            Json::obj([
                ("run_id", Json::str(l.run_id.clone())),
                ("up_to_seq", Json::Int(l.up_to_seq as i64)),
                ("head_hash", Json::str(l.head_hash.clone())),
            ])
        })
        .collect();
    let subject = SubjectSection {
        run_ids: vec![run_id.to_string()],
        heads,
        lineage,
        watermarks,
        status,
    };

    // ── materialize policy → member statuses + fetch[] ───────────────
    let mut fetch_entries = Vec::new();
    if inputs.policy.materialize != "self_contained" {
        // `detached`/`manifest_only`: payload members are listed, not
        // shipped — status flips to `fetch`, bytes leave the tree.
        for m in index.iter_mut() {
            if m.role != "subject" {
                m.status = MemberStatus::Fetch;
                fetch_entries.push(crate::manifest::FetchEntry {
                    address: m.address.clone(),
                    size: Some(m.size),
                    locations: vec![],
                    expires: None,
                });
                members.remove(&m.address);
            }
        }
    }

    // Unpinned declarations — `pinned = false` model snapshots and a
    // foreign-image environment are carried as `unpinned[]` entries (they
    // satisfy nothing above R0; the level derivation reads them).
    let all_pinned = inputs
        .model_snapshots
        .iter()
        .all(|s| s.get("pinned") == Some(&Json::Bool(true)));
    let mut unpinned: Vec<Unpinned> = Vec::new();
    if !all_pinned && !inputs.model_snapshots.is_empty() {
        unpinned.push(Unpinned {
            role: "model".into(),
            reason: "provider_opaque".into(),
            claim: None,
        });
    }
    if let Some(f) = env_foreign.clone() {
        unpinned.push(Unpinned {
            role: "environment".into(),
            reason: "foreign_only".into(),
            claim: Some(f),
        });
    }
    let participant_class = inputs
        .participant_class
        .clone()
        .unwrap_or_else(|| rm.participant_class.as_str().to_string());

    // definition / results sections carry member refs.
    let definition_section = Json::obj([
        ("version_id", Json::str(def_ref.clone())),
        (
            "semantic_id",
            rm.extra
                .get("definition_semantic_id")
                .cloned()
                .unwrap_or(Json::Null),
        ),
        (
            "dialect",
            rm.extra
                .get("definition_dialect")
                .cloned()
                .unwrap_or(Json::Null),
        ),
        ("member", Json::str(def_addr)),
        (
            "compiled_bundle",
            match (&inputs.compiled, &compiled_member) {
                (Some(c), Some(a)) => Json::obj([
                    ("bundle_id", Json::str(c.bundle_id.clone())),
                    ("derivation_key", Json::str(c.derivation_key.clone())),
                    ("member", Json::str(a.clone())),
                    ("lcd_report", c.lcd_report.clone()),
                    ("opacity_report", c.opacity_report.clone()),
                ]),
                _ => Json::Null,
            },
        ),
    ]);
    let mut results_section = results_doc.clone();
    if let Json::Obj(m) = &mut results_section {
        m.insert("member".into(), Json::str(results_addr));
    }

    let mut manifest = BundleManifest {
        bundle_kind: "run".into(),
        created_at: inputs.created_at.clone(),
        producer: inputs.producer.clone(),
        participant_class,
        observability_levels: rm
            .observability_level
            .iter()
            .map(|l| l.as_str().to_string())
            .collect(),
        claims: inputs.policy.claims.clone(),
        name_bindings: vec![],
        fetch_policy: if inputs.policy.materialize == "self_contained" {
            "self_contained".into()
        } else {
            "detached_allowed".into()
        },
        fetch: fetch_entries,
        subject,
        definition: definition_section,
        configuration: {
            let mut c = config_doc;
            if let Json::Obj(m) = &mut c {
                m.insert("member".into(), Json::str(config_addr));
                m.insert("environment_member".into(), Json::str(env_addr));
            }
            c
        },
        resolved_dependencies: deps_doc,
        model: model_doc,
        instrument: instrument_doc,
        traces: {
            let mut t = BTreeMap::new();
            t.insert(run_id.to_string(), export);
            t
        },
        results: results_section,
        // Stamped below — the basis derivation reads the manifest's own
        // member index, so the section is built after the literal.
        reproducibility: Json::Null,
        members: index,
        unpinned,
        ext: BTreeMap::new(),
        version_id: String::new(),
    };

    // ── max_supported_level + basis — the shared closed B-R* evaluator
    // (`levels::derive`; `validate_bundle` S4 re-derives the same way) ──
    let (max_supported, basis) = levels::derive(&manifest, replay_declared);
    let claimed = inputs.policy.claimed_level.unwrap_or(max_supported);
    if claimed > max_supported {
        return Err(BundleError::ReproClaimUnsupported {
            claimed: claimed.name().to_string(),
            max_supported: max_supported.name().to_string(),
        });
    }
    manifest.reproducibility = Json::obj([
        ("claimed_level", Json::str(claimed.name())),
        ("max_supported_level", Json::str(max_supported.name())),
        (
            "basis",
            Json::Arr(basis.iter().map(|b| b.to_json()).collect()),
        ),
        ("nondeterminism", Json::str(nd_addr.clone())),
        (
            "evidence",
            Json::obj([
                ("oracle_diff", Json::Arr(vec![])),
                ("fingerprints", Json::Arr(vec![])),
            ]),
        ),
        ("independent_reproductions", Json::Arr(vec![])),
    ]);
    manifest.version_id = manifest.compute_id();

    let report_rows = vec![Json::obj([
        ("kind", Json::str("bundle_assembled")),
        ("bundle_id", Json::str(manifest.version_id.clone())),
        ("max_supported_level", Json::str(max_supported.name())),
    ])];

    Ok(Assembled {
        manifest,
        members,
        report_rows,
    })
}
