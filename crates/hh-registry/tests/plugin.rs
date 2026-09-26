//! R-2.12.2 acceptance tests — `PluginManifest/1` codec, `admit_plugin`
//! ordering (compatibility before any fetch/launch), locality, tier,
//! `requests`-as-claims, the golden corpus against the naive checker, and the
//! `resolve(mode = execute)` compatibility re-check (AC-R-2.12.2-{1,2,8,12}).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use hh_identity::names::ResolveMode;
use hh_plugin::{ContractRef, ContractVersionPolicy, EvalPoint, SunsetBound};
use hh_registry::corpus;
use hh_registry::extension::plugin::{
    admit_plugin, manifest_from_json, manifest_to_json, resolve_plugin_candidate, AdmissionContext,
    ManifestError, PluginManifest, Requests,
};
use hh_registry::extension::{ExtensionKind, ExtensionRecord, SourceLocator};
use hh_registry::records::{RegistryPolicy, RegistryRecord};
use hh_registry::store::{RegistryStore, ResolveInput, ResolveRequest};
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-registry-plugin-test-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn store_at(d: &std::path::Path) -> RegistryStore {
    corpus::build(d).unwrap();
    RegistryStore::open(d, &corpus::kernel_registrar()).unwrap()
}

fn summary_json() -> Json {
    Json::obj([
        ("authority", Json::str("external")),
        (
            "content_hash",
            Json::str("sha256:0000000000000000000000000000000000000000000000000000000000000001"),
        ),
        ("owner", Json::str("test")),
        (
            "provenance",
            Json::obj([
                ("authority", Json::str("external")),
                ("created_at", Json::Int(0)),
                (
                    "origin",
                    Json::obj([
                        ("author_ref", Json::str("test")),
                        ("role", Json::str("author")),
                        ("kind", Json::str("human")),
                    ]),
                ),
                ("scope", Json::str("run")),
            ]),
        ),
    ])
}

/// A minimal valid first-party manifest object (one `variant` contribution
/// for the registered `control_strategy` class contract `1.0`).
fn valid_manifest_json() -> Json {
    Json::obj([
        ("claims", Json::obj([])),
        ("conformance_claims", Json::Arr(vec![])),
        (
            "contributions",
            Json::Arr(vec![Json::obj([
                ("contract_range", Json::str("1.0")),
                (
                    "declaration",
                    Json::obj([("class_id", Json::str("control_strategy"))]),
                ),
                ("kind", Json::str("variant")),
                ("path_or_locator", Json::str("variants/v.json")),
            ])]),
        ),
        ("depends_on", Json::Arr(vec![])),
        (
            "identity",
            Json::obj([("name", Json::str("plug")), ("namespace", Json::str("hh"))]),
        ),
        (
            "requests",
            Json::obj([
                ("effects", Json::Arr(vec![])),
                ("egress", Json::Arr(vec![])),
                ("env_keys", Json::Arr(vec![])),
                ("fs_roots", Json::Arr(vec![])),
                ("hot_path_operations", Json::Arr(vec![])),
            ]),
        ),
        (
            "requires",
            Json::obj([
                ("contracts", Json::Arr(vec![])),
                ("hir_dialect", Json::str("1")),
                ("plugin_abi", Json::str("1")),
                ("records", Json::Arr(vec![])),
                ("registry_dialect", Json::str("1")),
            ]),
        ),
        ("summary", summary_json()),
        ("tier", Json::str("C0")),
    ])
}

fn ctx(first_party: bool) -> AdmissionContext {
    AdmissionContext {
        first_party,
        now: EvalPoint::kernel(),
        requests_cap: None,
    }
}

// ── codec ────────────────────────────────────────────────────────────────────

#[test]
fn manifest_round_trip_byte_identical() {
    let j = valid_manifest_json();
    let m = manifest_from_json(&j, "manifest").unwrap();
    let back = manifest_to_json(&m);
    assert_eq!(back, j);
    // Canonical bytes are stable across decode→encode (V2).
    assert_eq!(
        back.to_canonical_string(),
        manifest_to_json(&manifest_from_json(&back, "manifest").unwrap()).to_canonical_string()
    );
}

#[test]
fn unknown_top_level_member_is_refused() {
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        m.insert("evil".to_string(), Json::Null);
    }
    let e = manifest_from_json(&j, "manifest").unwrap_err();
    assert_eq!(e.kind_str(), "SchemaViolation");
}

#[test]
fn non_object_manifest_is_not_json() {
    let e = manifest_from_json(&Json::Arr(vec![]), "manifest").unwrap_err();
    assert_eq!(e.kind_str(), "NotJson");
}

#[test]
fn manifest_is_inventory_not_code() {
    // AC-1: an inline body inside a declaration is refused.
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Arr(c) = m.get_mut("contributions").unwrap() {
            if let Json::Obj(cm) = &mut c[0] {
                cm.insert(
                    "declaration".to_string(),
                    Json::obj([("body", Json::str("fn run() {}"))]),
                );
            }
        }
    }
    assert_eq!(
        manifest_from_json(&j, "manifest").unwrap_err().kind_str(),
        "SchemaViolation"
    );
}

// ── admission ────────────────────────────────────────────────────────────────

#[test]
fn admit_valid_manifest_negotiates() {
    let d = dir("admit-ok");
    let s = store_at(&d);
    let m = manifest_from_json(&valid_manifest_json(), "manifest").unwrap();
    let report = admit_plugin(&s, &m, &ctx(true)).unwrap();
    assert_eq!(report.plugin_id, "hh/plug");
    // negotiated includes hir/1, registry/1, plugin_abi/1, class_contract.
    assert!(report
        .negotiated
        .chosen
        .iter()
        .any(|(r, v)| r.id == "plugin_abi/1" && v == "1"));
    assert!(report
        .negotiated
        .chosen
        .iter()
        .any(|(r, v)| r.id == "control_strategy" && v == "1.0"));
}

#[test]
fn compat_miss_refuses_before_any_launch() {
    // AC-2: a `requires.plugin_abi` outside the policy is ContractIncompatible
    // and admission is pure — nothing is fetched/launched (the store API
    // carries no fetch/launch capability at all; the check is a read).
    let d = dir("admit-miss");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(r) = m.get_mut("requires").unwrap() {
            r.insert("plugin_abi".to_string(), Json::str(">=9"));
        }
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    let e = admit_plugin(&s, &m, &ctx(true)).unwrap_err();
    match e {
        ManifestError::ContractIncompatible { failures } => {
            assert_eq!(failures[0].contract.id, "plugin_abi/1");
            assert_eq!(failures[0].offered_range, ">=9");
        }
        other => panic!("expected ContractIncompatible, got {other:?}"),
    }
}

#[test]
fn sunset_refusal_is_dated() {
    // AC-2: a version past its sunset refuses with the dated bound — via a
    // layered `contract_version_policies` entry (operator policy narrows the
    // kernel table).
    let d = dir("admit-sunset");
    let mut s = store_at(&d);
    let mut policy = RegistryPolicy::stage1_default();
    policy
        .contract_version_policies
        .push(ContractVersionPolicy {
            contract: ContractRef::protocol_binding("plugin_abi/1", "*"),
            supported: vec!["1".to_string()],
            sunset: BTreeMap::from([("1".to_string(), SunsetBound::Stage(0))]),
            additive_only: true,
        });
    s.set_policy(policy).unwrap();
    let m = manifest_from_json(&valid_manifest_json(), "manifest").unwrap();
    match admit_plugin(&s, &m, &ctx(true)).unwrap_err() {
        ManifestError::ContractIncompatible { failures } => {
            assert_eq!(failures[0].sunset.as_deref(), Some("stage:0"));
        }
        other => panic!("expected sunset ContractIncompatible, got {other:?}"),
    }
}

#[test]
fn third_party_in_process_is_refused() {
    // CF-141 vector: a `local/` plugin preferring `in_process` is refused
    // (`in_process` is `hh/` first-party only).
    let d = dir("admit-loc");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(id) = m.get_mut("identity").unwrap() {
            id.insert("namespace".to_string(), Json::str("local"));
        }
        if let Json::Arr(c) = m.get_mut("contributions").unwrap() {
            if let Json::Obj(cm) = &mut c[0] {
                cm.insert(
                    "executable".to_string(),
                    Json::obj([
                        (
                            "code_pointer",
                            Json::obj([
                                ("algorithm", Json::str("sha256")),
                                ("digest", Json::str("aa")),
                                ("media_type", Json::str("application/octet-stream")),
                                ("size", Json::Int(4)),
                            ]),
                        ),
                        ("isolation", Json::str("subprocess_confined")),
                        ("placement_preference", Json::str("in_process")),
                    ]),
                );
            }
        }
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    assert_eq!(
        admit_plugin(&s, &m, &ctx(true)).unwrap_err().kind_str(),
        "LocalityInadmissible"
    );
}

#[test]
fn depends_on_upward_tier_is_refused() {
    // X2: a C0 manifest whose `depends_on` names a C2 contract is a tier
    // violation at admission.
    let d = dir("admit-tier");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        m.insert(
            "depends_on".to_string(),
            Json::Arr(vec![Json::obj([
                ("id", Json::str("hh-hosting/1")),
                ("kind", Json::str("protocol_binding")),
                ("version_range", Json::str("*")),
            ])]),
        );
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    assert_eq!(
        admit_plugin(&s, &m, &ctx(true)).unwrap_err().kind_str(),
        "TierViolation"
    );
}

#[test]
fn requires_hosting_abi_is_contract_incompatible() {
    // AC-12: no policy names `hh-hosting/1` — a `requires` on it is an unknown
    // contract → ContractIncompatible with empty supported_range.
    let d = dir("admit-hosting");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(r) = m.get_mut("requires").unwrap() {
            r.insert(
                "contracts".to_string(),
                Json::Arr(vec![Json::obj([
                    ("id", Json::str("hh-hosting/1")),
                    ("kind", Json::str("protocol_binding")),
                    ("version_range", Json::str("*")),
                ])]),
            );
        }
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    match admit_plugin(&s, &m, &ctx(true)).unwrap_err() {
        ManifestError::ContractIncompatible { failures } => {
            assert_eq!(failures[0].contract.id, "hh-hosting/1");
            assert_eq!(failures[0].supported_range, "");
        }
        other => panic!("expected ContractIncompatible, got {other:?}"),
    }
}

#[test]
fn requests_exceeding_cap_are_refused() {
    // Requests are claims — they never widen past the sealing cap.
    let d = dir("admit-cap");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(r) = m.get_mut("requests").unwrap() {
            r.insert(
                "env_keys".to_string(),
                Json::Arr(vec![Json::str("AWS_SECRET")]),
            );
        }
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    let mut c = ctx(true);
    c.requests_cap = Some(Requests::default());
    assert_eq!(
        admit_plugin(&s, &m, &c).unwrap_err().kind_str(),
        "RequestsExceedCap"
    );
}

// ── store integration ────────────────────────────────────────────────────────

fn plugin_record(m: &PluginManifest) -> RegistryRecord {
    RegistryRecord::Extension(ExtensionRecord {
        kind: ExtensionKind::Plugin,
        name: m.identity.id(),
        content: hh_identity::idp::address(b"pkg", "application/octet-stream"),
        manifest: manifest_to_json(m),
        contributes: vec![],
        locator: SourceLocator {
            scheme: "directory_scan".to_string(),
            credential_free_uri: "file:///plugins/plug".to_string(),
            selector: None,
            resolved: Some("sha256:pin".to_string()),
            fetched_at: Some(1),
        },
        trust: hh_registry::extension::ExtensionTrustRecord::unresolved_default(
            hh_provenance::PersistenceScope::Run,
        ),
        provenance: hh_provenance::ProvenanceRecord::kernel("test", 0),
        ext: BTreeMap::new(),
    })
}

#[test]
fn register_plugin_extension_runs_admission() {
    let d = dir("reg-plugin");
    let mut s = store_at(&d);
    let kernel = corpus::kernel_registrar();
    // A valid plugin manifest registers.
    let m = manifest_from_json(&valid_manifest_json(), "manifest").unwrap();
    let v = s.register(plugin_record(&m), &kernel, None).unwrap();
    assert!(!v.version_id.is_empty());

    // An incompatible manifest is refused at register — before any fetch.
    let mut bad = valid_manifest_json();
    if let Json::Obj(mm) = &mut bad {
        if let Json::Obj(r) = mm.get_mut("requires").unwrap() {
            r.insert("plugin_abi".to_string(), Json::str(">=9"));
        }
    }
    let m2 = manifest_from_json(&bad, "manifest").unwrap();
    let e = s.register(plugin_record(&m2), &kernel, None).unwrap_err();
    assert_eq!(e.reason(), "ContractIncompatible");
}

#[test]
fn execute_mode_resolution_rechecks_compatibility() {
    // §8.4 §2: `resolve(mode = execute)` re-runs `check_compatibility` — a
    // sunset that passed between admission and execute refuses, dated.
    let d = dir("exec-recheck");
    let mut s = store_at(&d);
    let kernel = corpus::kernel_registrar();
    let m = manifest_from_json(&valid_manifest_json(), "manifest").unwrap();
    let v = s.register(plugin_record(&m), &kernel, None).unwrap();

    // Audit resolve passes; execute resolve passes pre-sunset.
    s.resolve(
        &ResolveInput::Version(v.version_id.clone()),
        ResolveMode::Execute,
        &ResolveRequest::default(),
    )
    .unwrap();

    // Layer a sunset: plugin_abi/1's "1" is unsupported from stage 0.
    let mut policy = RegistryPolicy::stage1_default();
    policy
        .contract_version_policies
        .push(ContractVersionPolicy {
            contract: ContractRef::protocol_binding("plugin_abi/1", "*"),
            supported: vec!["1".to_string()],
            sunset: BTreeMap::from([("1".to_string(), SunsetBound::Stage(0))]),
            additive_only: true,
        });
    s.set_policy(policy).unwrap();
    let e = s
        .resolve(
            &ResolveInput::Version(v.version_id.clone()),
            ResolveMode::Execute,
            &ResolveRequest::default(),
        )
        .unwrap_err();
    assert_eq!(e.reason(), "ContractIncompatible");
    // Audit mode still reads the record (the refusal is execute-gated).
    s.resolve(
        &ResolveInput::Version(v.version_id.clone()),
        ResolveMode::Audit,
        &ResolveRequest::default(),
    )
    .unwrap();
}

// ── resolve_plugin_candidate + corpus ────────────────────────────────────────

#[test]
fn resolve_plugin_candidate_decode_admit_one_step() {
    let d = dir("resolve-cand");
    let s = store_at(&d);
    let bytes = manifest_to_json(&manifest_from_json(&valid_manifest_json(), "m").unwrap())
        .to_canonical_string();
    let report = resolve_plugin_candidate(&s, bytes.as_bytes(), &ctx(true)).unwrap();
    assert_eq!(report.plugin_id, "hh/plug");
    // Garbage bytes are NotJson.
    assert_eq!(
        resolve_plugin_candidate(&s, b"not json", &ctx(true))
            .unwrap_err()
            .kind_str(),
        "NotJson"
    );
}

/// The golden manifest corpus driven by the **naive checker** — decode +
/// admit via `resolve_plugin_candidate` under a deny-everything requests cap
/// (every fixture with a non-empty `requests` must then fail
/// `RequestsExceedCap`; all `valid/` fixtures carry empty requests).
/// `hh_plugin::corpus::run_corpus` enumerates `valid/`+`invalid/` and pins the
/// failure kind per `*.expect` file (AC-1's corpus requirement; the
/// byte-identical cross-implementation check).
#[test]
fn golden_manifest_corpus() {
    let d = dir("corpus");
    let s = store_at(&d);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugin-manifests");
    let mut c = ctx(true);
    c.requests_cap = Some(Requests::default());
    let admitted = hh_plugin::run_corpus(&dir, |bytes| {
        resolve_plugin_candidate(&s, bytes, &c)
            .map(|r| r.plugin_id)
            .map_err(|e| e.kind_str().to_string())
    })
    .unwrap_or_else(|fails| panic!("corpus failures: {fails:?}"));
    assert_eq!(admitted.len(), 2);
}

// ── spec-DAG / lint / ABI seams (integration) ────────────────────────────────

#[test]
fn spec_dag_reports_clean() {
    // AC-8 static half — the §4.4 tables parsed as data report
    // `tier_violations=[]`, `cycles=[]` (plus no unnameable/undeclared deps).
    let spec = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/CANONICAL_SPEC.md"),
    )
    .unwrap();
    let report = hh_plugin::spec_dag_check(&spec, &[], &[]);
    assert!(
        report.is_clean(),
        "tier_violations={:?} cycles={:?} unknown={:?} errors={:?}",
        report.inner.tier_violations,
        report.inner.cycles,
        report.unknown_refs,
        report.parse_errors
    );
}

#[test]
fn manifest_dag_input_feeds_the_check() {
    // A manifest's depends_on + requires.contracts become DAG edges; a C0
    // manifest depending on the C2 hosting binding is a tier violation in the
    // DAG report (the same refusal admit_plugin makes).
    let d = dir("dag");
    let s = store_at(&d);
    let mut j = valid_manifest_json();
    if let Json::Obj(m) = &mut j {
        m.insert(
            "depends_on".to_string(),
            Json::Arr(vec![Json::obj([
                ("id", Json::str("hh-hosting/1")),
                ("kind", Json::str("protocol_binding")),
                ("version_range", Json::str("*")),
            ])]),
        );
    }
    let m = manifest_from_json(&j, "manifest").unwrap();
    let spec = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/CANONICAL_SPEC.md"),
    )
    .unwrap();
    let classes = hh_registry::extension::plugin::class_catalog(&s)
        .iter()
        .map(|c| hh_plugin::ClassDagInput {
            class_id: c.class_id.clone(),
            tier: c.tier,
            depends_on: vec![],
        })
        .collect::<Vec<_>>();
    let report = hh_plugin::spec_dag_check(&spec, &classes, &[m.dag_input()]);
    assert!(!report.inner.tier_violations.is_empty());
    // And the same manifest lints clean through the module-graph lint only
    // when its code is out of process — the lint applies to in-process
    // variant *source*; manifests are data, nothing to lint.
    assert!(hh_plugin::lint_module_source("use hh_hir::leaves::Text;").is_empty());
    assert!(!hh_plugin::lint_module_source("use unsafe_code::X;").is_empty());
}
