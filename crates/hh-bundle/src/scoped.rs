//! `bundle(scope, kind ∈ {arm, experiment, lineage})` — the scoped-kind
//! assembly pass (§5h.3 §2 `bundle`; ADR-0141 D2 "Composition":
//! `experiment ⊃ arm ⊃ run` reference contained manifests by
//! `version_id`; S4.2, AC-R-2.10.3-11 / AC-R-2.9.3-12).
//!
//! A scoped bundle is one `hh-bundle/1` manifest whose `subject` covers
//! every contained subject run, whose `traces` carry each run's
//! `LedgerExport`, whose `results` section binds the experiment plane
//! (`design`, `pre_registration`, `arm?`, `match_spec`, `search_budget`,
//! the per-arm budget table and the pre-registration/plan seq pair S9
//! verifies), and whose `composition.contains[]` names the contained
//! bundles — each contained manifest carried verbatim as a
//! `contains:<bundle_id>` member so `validate_bundle` S9 can decode it
//! without touching a store.
//!
//! Kind rules:
//! - `arm` covers the arm's subject runs and `contains[]` the run bundles
//!   over them; `results.arm` is mandatory.
//! - `experiment` covers every arm's runs and `contains[]` the arm
//!   bundles; `results.design`/`pre_registration`/`match_spec`/
//!   `search_budget` are mandatory (AC-R-2.10.3-11).
//! - `lineage` covers a fork-prefix chain; `contains[]` names every
//!   prefix bundle ("lineage bundles reference every fork-prefix
//!   bundle", §5h.3 §2 composition note) and `composition.lineage.root`
//!   names the chain's root run.

use std::collections::BTreeMap;

use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::assemble::Assembled;
use crate::error::BundleError;
use crate::export::{add_member, build_ledger_export, MemberBytes};
use crate::levels;
use crate::manifest::{BundleManifest, BundlePolicy, MemberRef, MemberStatus, SubjectSection};

/// The scoped kinds `assemble_scoped` produces.
pub const KIND_ARM: &str = "arm";
/// `kind = experiment`.
pub const KIND_EXPERIMENT: &str = "experiment";
/// `kind = lineage`.
pub const KIND_LINEAGE: &str = "lineage";

/// A contained bundle — the child manifest's canonical bytes (the member
/// payload) plus the coordinates `composition.contains[]` records.
#[derive(Debug, Clone)]
pub struct ChildBundle {
    /// The child's `version_id` (the `contains[]` reference).
    pub bundle_id: String,
    /// The child's `bundle_kind` (`run` under `arm`, `arm` under
    /// `experiment`, any under `lineage`).
    pub kind: String,
    /// The child's canonical manifest bytes — deposited verbatim as a
    /// `contains:<bundle_id>` member (never rewritten, never re-issued).
    pub manifest_bytes: Vec<u8>,
}

/// Everything `assemble_scoped` needs from its caller's planes.
pub struct ScopedInputs<'a> {
    /// The ledger store (read-only — `bundle` is a read).
    pub store: &'a Store,
    /// The bundle policy (`materialize`, `claimed_level`, `claims`).
    pub policy: &'a BundlePolicy,
    /// RFC 3339 ms creation stamp.
    pub created_at: String,
    /// The producer `ProvenanceRecord`.
    pub producer: Json,
    /// The `ContractIdentity` (carried in `instrument.component_versions`).
    pub contract_identity: Json,
    /// `InstrumentRecord.version`.
    pub kernel_version_id: String,
    /// `InstrumentRecord.dirty` (`true` caps `max_supported_level` at R0).
    pub instrument_dirty: bool,
    /// `arm | experiment | lineage`.
    pub kind: String,
    /// The experiment run the scope binds (`None` for `lineage`).
    pub experiment_run_id: Option<String>,
    /// The arm (`kind = arm`; the `results.arm` record).
    pub arm: Option<Json>,
    /// The `Design` document (kind `arm`/`experiment`).
    pub design: Option<Json>,
    /// The `PreRegistration` document (kind `arm`/`experiment`).
    pub pre_registration: Option<Json>,
    /// The seq the pre-registration committed at (the `declared` fact's
    /// seq — `PreRegistrationLate`'s left-hand side).
    pub pre_registration_seq: Option<u64>,
    /// The first `run_planned` seq on the experiment run (`PreRegistrationLate`'s
    /// right-hand side).
    pub first_plan_seq: Option<u64>,
    /// The `MatchSpec` the scope reports under.
    pub match_spec: Option<Json>,
    /// The declared `SearchBudgetRecord`/`search_budget` document.
    pub search_budget: Option<Json>,
    /// The per-arm table S9 checks — `{arm_id, definition_ref,
    /// eval_budget, level_assignment}` per covered arm
    /// (`UnmatchedBudget` compares `eval_budget` under a matched mode;
    /// `ContainmentMismatch` compares a contained manifest's
    /// `definition` against its arm's declared `definition_ref`).
    pub arms: Vec<Json>,
    /// The covered subject runs.
    pub subject_runs: Vec<String>,
    /// The contained bundles (the `experiment ⊃ arm ⊃ run` DAG).
    pub children: Vec<ChildBundle>,
    /// Result-row version ids the bundle declares (S9 `RowOrphan` checks
    /// each names a `row:<version_id>` member).
    pub row_refs: Vec<String>,
    /// Result-row payloads, when the caller materializes them —
    /// `(version_id, canonical row bytes)`; deposited as `row:<id>`
    /// members.
    pub row_payloads: Vec<(String, Vec<u8>)>,
    /// `native | hosted` — `hosted` when any covered subject is hosted
    /// (the aggregate claims the weakest class).
    pub participant_class: Option<String>,
}

/// `bundle(kind ∈ {arm, experiment, lineage})` — the scoped assembly.
/// Refuses `ScopeInvalid` on an input that cannot resolve (never a
/// partial bundle — CC3).
pub fn assemble_scoped(inputs: &ScopedInputs<'_>) -> Result<Assembled, BundleError> {
    match inputs.kind.as_str() {
        KIND_ARM | KIND_EXPERIMENT | KIND_LINEAGE => {}
        other => {
            return Err(BundleError::ScopeInvalid {
                detail: format!("kind `{other}` — expected arm|experiment|lineage"),
            })
        }
    }
    if inputs.subject_runs.is_empty() && inputs.kind != KIND_LINEAGE {
        return Err(BundleError::ScopeInvalid {
            detail: "subject_runs is empty — a scoped bundle covers ≥ 1 run".into(),
        });
    }
    if inputs.kind == KIND_ARM && inputs.arm.is_none() {
        return Err(BundleError::ScopeInvalid {
            detail: "kind arm requires `arm`".into(),
        });
    }
    if inputs.kind != KIND_LINEAGE {
        for (name, doc) in [
            ("design", &inputs.design),
            ("pre_registration", &inputs.pre_registration),
            ("match_spec", &inputs.match_spec),
            ("search_budget", &inputs.search_budget),
        ] {
            if doc.is_none() {
                return Err(BundleError::ScopeInvalid {
                    detail: format!("kind {} requires `{name}`", inputs.kind),
                });
            }
        }
    }
    let store = inputs.store;
    let mut members: MemberBytes = BTreeMap::new();
    let mut index: Vec<MemberRef> = Vec::new();
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

    // ── subject section + per-run LedgerExports ─────────────────────
    let mut heads = BTreeMap::new();
    let mut watermarks = BTreeMap::new();
    let mut experiment_bindings = BTreeMap::new();
    let mut lineage: Vec<Json> = Vec::new();
    let mut traces = BTreeMap::new();
    let mut all_finished = true;
    for run_id in &inputs.subject_runs {
        let rm = store
            .manifest(run_id)
            .map_err(|_| BundleError::RunNotFound {
                run_id: run_id.clone(),
            })?;
        let (export, page_roles) = build_ledger_export(store, run_id, &mut members)?;
        let finished = store
            .envelopes(run_id)
            .unwrap_or_default()
            .iter()
            .any(|e| e.class == "lifecycle.run.finished");
        all_finished &= finished;
        let head = store.head(run_id).unwrap_or(hh_ledger::event::Head {
            seq: 0,
            event_id: String::new(),
            hash: String::new(),
        });
        heads.insert(
            run_id.clone(),
            Json::obj([
                ("seq", Json::Int(head.seq as i64)),
                ("event_id", Json::str(head.event_id.clone())),
                ("hash", Json::str(head.hash.clone())),
            ]),
        );
        watermarks.insert(run_id.clone(), head.seq);
        if let Some(b) = rm.experiment.as_ref() {
            if let (Some(e), Some(a)) = (&b.experiment_run_id, &b.arm_id) {
                experiment_bindings.insert(
                    run_id.clone(),
                    Json::obj([
                        ("experiment_run_id", Json::str(e.clone())),
                        ("arm_id", Json::str(a.clone())),
                    ]),
                );
            }
        }
        for l in store.lineage(run_id).unwrap_or_default() {
            lineage.push(Json::obj([
                ("run_id", Json::str(l.run_id.clone())),
                ("up_to_seq", Json::Int(l.up_to_seq as i64)),
                ("head_hash", Json::str(l.head_hash.clone())),
            ]));
        }
        for (role, addr, size) in page_roles {
            index.push(MemberRef::present(
                role,
                addr,
                "application/vnd.hh.ledger-page+json",
                size,
            ));
        }
        traces.insert(run_id.clone(), export);
    }
    lineage.sort_by(|a, b| {
        let ka = (
            a.get("run_id").and_then(Json::as_str).unwrap_or(""),
            a.get("up_to_seq").and_then(Json::as_int).unwrap_or(0),
        );
        let kb = (
            b.get("run_id").and_then(Json::as_str).unwrap_or(""),
            b.get("up_to_seq").and_then(Json::as_int).unwrap_or(0),
        );
        ka.cmp(&kb)
    });
    lineage.dedup();
    let subject = SubjectSection {
        run_ids: inputs.subject_runs.clone(),
        heads,
        lineage,
        watermarks,
        status: if all_finished { "finished" } else { "open" }.to_string(),
        experiment: experiment_bindings,
    };

    // ── contained-bundle members + composition.contains[] ────────────
    let mut contains: Vec<Json> = Vec::new();
    for child in &inputs.children {
        let (addr, size) = add_member(
            &mut members,
            child.manifest_bytes.clone(),
            "application/vnd.hh.bundle+json",
        );
        index.push(MemberRef::present(
            format!("contains:{}", child.bundle_id),
            addr.clone(),
            "application/vnd.hh.bundle+json",
            size,
        ));
        contains.push(Json::obj([
            ("bundle_id", Json::str(child.bundle_id.clone())),
            ("kind", Json::str(child.kind.clone())),
            ("member", Json::str(addr)),
        ]));
    }

    // ── section documents ────────────────────────────────────────────
    let subject_doc = Json::obj([
        (
            "run_ids",
            Json::Arr(subject.run_ids.iter().map(Json::str).collect()),
        ),
        ("status", Json::str(subject.status.clone())),
    ]);
    doc_member(&mut members, "subject", &subject_doc, &mut index);

    let mut results_doc = Json::obj([
        (
            "rows",
            Json::Arr(inputs.row_refs.iter().map(Json::str).collect()),
        ),
        ("metric_declarations", Json::Arr(vec![])),
        ("oracle_declarations", Json::Arr(vec![])),
    ]);
    if let Json::Obj(m) = &mut results_doc {
        if let Some(d) = &inputs.design {
            let addr = doc_member(&mut members, "design", d, &mut index);
            m.insert("design".into(), Json::str(addr));
        }
        if let Some(p) = &inputs.pre_registration {
            let addr = doc_member(&mut members, "pre_registration", p, &mut index);
            m.insert("pre_registration".into(), Json::str(addr));
            if let Some(s) = inputs.pre_registration_seq {
                m.insert("pre_registration_seq".into(), Json::Int(s as i64));
            }
        }
        if let Some(s) = inputs.first_plan_seq {
            m.insert("first_plan_seq".into(), Json::Int(s as i64));
        }
        if let Some(a) = &inputs.arm {
            let addr = doc_member(&mut members, "arm", a, &mut index);
            m.insert("arm".into(), Json::str(addr));
        }
        if let Some(ms) = &inputs.match_spec {
            let addr = doc_member(&mut members, "match_spec", ms, &mut index);
            m.insert("match_spec".into(), Json::str(addr));
        }
        if let Some(sb) = &inputs.search_budget {
            let addr = doc_member(&mut members, "search_budget", sb, &mut index);
            m.insert("search_budget".into(), Json::str(addr));
        }
        m.insert("arms".into(), Json::Arr(inputs.arms.to_vec()));
        if let Some(e) = &inputs.experiment_run_id {
            m.insert("experiment_run_id".into(), Json::str(e.clone()));
        }
    }
    // Declared rows materialize as `row:<version_id>` members when the
    // caller supplies the payloads (S9's `RowOrphan` folds the rest).
    for (vid, bytes) in &inputs.row_payloads {
        let (addr, size) = add_member(
            &mut members,
            bytes.clone(),
            "application/vnd.hh.results-row+json",
        );
        index.push(MemberRef::present(
            format!("row:{vid}"),
            addr,
            "application/vnd.hh.results-row+json",
            size,
        ));
    }
    let results_addr = doc_member(&mut members, "results", &results_doc, &mut index);
    if let Json::Obj(m) = &mut results_doc {
        m.insert("member".into(), Json::str(results_addr));
    }

    // The aggregate definition section — the experiment scope's
    // declaration is its `Design` (the per-arm definitions live in the
    // contained bundles and the `arms[]` table).
    let definition_doc = Json::obj([
        (
            "kind",
            Json::str(match inputs.kind.as_str() {
                KIND_ARM => "arm_definition",
                KIND_EXPERIMENT => "experiment_design",
                _ => "lineage",
            }),
        ),
        (
            "experiment_run_id",
            inputs
                .experiment_run_id
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "arms",
            Json::Arr(
                inputs
                    .arms
                    .iter()
                    .map(|a| {
                        Json::obj([
                            ("arm_id", a.get("arm_id").cloned().unwrap_or(Json::Null)),
                            (
                                "definition_ref",
                                a.get("definition_ref").cloned().unwrap_or(Json::Null),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let _def_addr = doc_member(&mut members, "definition", &definition_doc, &mut index);

    let config_doc = Json::obj([
        (
            "configuration_ids",
            Json::Arr(
                inputs
                    .arms
                    .iter()
                    .filter_map(|a| a.get("configuration_id").cloned())
                    .collect(),
            ),
        ),
        (
            "budgets",
            Json::Arr(
                inputs
                    .arms
                    .iter()
                    .map(|a| {
                        Json::obj([
                            ("arm_id", a.get("arm_id").cloned().unwrap_or(Json::Null)),
                            (
                                "eval_budget",
                                a.get("eval_budget").cloned().unwrap_or(Json::Null),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let _cfg_addr = doc_member(&mut members, "configuration", &config_doc, &mut index);

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
    let _inst_addr = doc_member(&mut members, "instrument", &instrument_doc, &mut index);

    let model_doc = Json::obj([("snapshots", Json::Arr(vec![]))]);
    doc_member(&mut members, "model", &model_doc, &mut index);
    let env_doc = Json::obj([("class", Json::str("aggregate"))]);
    doc_member(&mut members, "environment", &env_doc, &mut index);
    let deps_doc = Json::obj([
        ("registry_snapshot_id", Json::Null),
        ("variants", Json::Arr(vec![])),
        ("run_refs", Json::obj([])),
    ]);
    doc_member(&mut members, "resolved_dependencies", &deps_doc, &mut index);
    let nd_doc = Json::obj([("declarations", Json::Arr(vec![]))]);
    let nd_addr = doc_member(&mut members, "nondeterminism", &nd_doc, &mut index);

    // ── composition section ──────────────────────────────────────────
    let mut composition = Json::obj([("contains", Json::Arr(contains.clone()))]);
    if inputs.kind == KIND_LINEAGE {
        if let Json::Obj(m) = &mut composition {
            m.insert(
                "lineage".into(),
                Json::obj([(
                    "root_run_id",
                    inputs
                        .experiment_run_id
                        .clone()
                        .or_else(|| inputs.subject_runs.first().cloned())
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                )]),
            );
        }
    }

    // ── materialize policy → member statuses + fetch[] ──────────────
    let mut fetch_entries = Vec::new();
    if inputs.policy.materialize != "self_contained" {
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

    crate::assemble::secret_scan(&members)?;

    let mut manifest = BundleManifest {
        bundle_kind: inputs.kind.clone(),
        created_at: inputs.created_at.clone(),
        producer: inputs.producer.clone(),
        participant_class: inputs
            .participant_class
            .clone()
            .unwrap_or_else(|| "native".into()),
        observability_levels: vec!["ledger".into()],
        claims: inputs.policy.claims.clone(),
        name_bindings: vec![],
        fetch_policy: if inputs.policy.materialize == "self_contained" {
            "self_contained".into()
        } else {
            "detached_allowed".into()
        },
        fetch: fetch_entries,
        subject,
        definition: definition_doc,
        configuration: config_doc,
        resolved_dependencies: deps_doc,
        model: model_doc,
        instrument: instrument_doc,
        traces,
        results: results_doc,
        composition,
        reproducibility: Json::Null,
        members: index,
        unpinned: vec![],
        ext: BTreeMap::new(),
        version_id: String::new(),
    };

    let (max_supported, basis) = levels::derive(&manifest, false);
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
        ("nondeterminism", Json::str(nd_addr)),
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
        ("bundle_kind", Json::str(inputs.kind.clone())),
        ("max_supported_level", Json::str(max_supported.name())),
    ])];

    Ok(Assembled {
        manifest,
        members,
        report_rows,
    })
}
