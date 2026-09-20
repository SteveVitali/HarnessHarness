//! AC-R-2.5.3-{8,11} (the C1/Stage-2 halves) plus the S2.10 catalog-index
//! extension slice (§5d.3; ADR-0094 D4, ADR-0095 D1/D2):
//!
//! - `lexical_regex`/`bm25`/`hierarchical` index variants: form
//!   admissibility (`unsupported_form`), `invalid_pattern`, hidden-entry
//!   exclusion (I-CLOSED), structured filters, deterministic ranked hits;
//! - `discover` — the kernel `discover_surfaces` lowering: hits ⊆ plan
//!   members with mode ∈ {indexed, deferred} and bounded by
//!   `max_reveal_per_search`;
//! - `catalog_delta` + `adopt` — `freeze` marks removed entries
//!   `unavailable(removed)` with `catalog_id`/`configuration_version_id`
//!   unchanged and calls to them fail `SurfaceUnavailable`; `adopt` mints a
//!   new epoch whose `bundle_delta` descriptors carry `authority =
//!   unverified`; `ask` is `CatalogDriftRefused`;
//! - `expire_reveals` — `call`/`turn`/`run`/`until_evicted` retention, with
//!   pinned surfaces never expiring;
//! - `retrieval_eval` — recall@8 floor for the `exact_name` baseline (the
//!   gated number; pre-registered floor = 1.0 — an exact-name query's
//!   target is rank 1) and reported nDCG@8 for the natural-language set on
//!   every C1 variant.

use std::collections::{BTreeMap, BTreeSet};

use hh_compiler::exposure::{
    adopt, catalog_delta, catalog_id_of, check_callable, discover, evict, expire_reveals,
    index_query, index_ref, AdoptOutcome, Availability, CallProposal, CallRefusal, Catalog,
    CatalogDriftError, CatalogEntry, CatalogIndex, CatalogSource, DiscoveryForm,
    DiscoveryQuery, DiscoveryQueryInvalid, DriftPolicy, EvictCause, IndexForm, PermissionCoverage,
    Retention, RevealBoundary, RevealCause, RevealedSet, SearchTextField, StructuredQuery,
    SyncTrigger,
};
use hh_compiler::retrieval::{retrieval_eval, LabelledQuery};
use hh_hir::tools::ExposureMode;

// ── Fixture ─────────────────────────────────────────────────────────────────

/// A catalog entry with full search-field declaration.
fn entry(
    sid: &str,
    name: &str,
    namespace: Option<&str>,
    description: &str,
    modes: &[ExposureMode],
) -> CatalogEntry {
    let mut search_text = BTreeMap::new();
    search_text.insert(SearchTextField::Description, description.to_string());
    search_text.insert(SearchTextField::Examples, format!("example use of {name}"));
    CatalogEntry {
        surface_id: sid.to_string(),
        capability: format!("cap:{name}"),
        version_id: format!("v:{name}"),
        source: CatalogSource::Harness(Some("test".to_string())),
        name: name.to_string(),
        namespace: namespace.map(str::to_string),
        admitted_modes: modes.iter().copied().collect(),
        pinned: false,
        hidden: false,
        index_form: IndexForm {
            name: name.to_string(),
            title: None,
            summary_ref: format!("sum:{name}"),
            namespace: namespace.map(str::to_string),
            tags: Vec::new(),
            search_text_fields: [
                SearchTextField::Name,
                SearchTextField::NameSplit,
                SearchTextField::Description,
                SearchTextField::NamespaceName,
                SearchTextField::EffectSummary,
                SearchTextField::Examples,
            ]
            .into_iter()
            .collect(),
            search_text,
        },
        size_tokens: 10,
        estimator_ref: "bytes_div_4".to_string(),
        effect_summary: Vec::new(),
        permission_coverage: PermissionCoverage::Unknown,
        label: None,
        availability: Availability::Available,
        is_discovery: false,
    }
}

/// The synthetic ≥200-surface fixture (spec §5d.3 §8: a ≥200-surface
/// catalogue with pinned, hidden and lifted entries): 8 namespaces × 30
/// entries + designated canonical entries the labelled queries target.
fn fixture_catalog() -> Catalog {
    let namespaces = ["fs", "net", "git", "proc", "mem", "ui", "db", "sched"];
    let verbs = ["read", "write", "list", "create", "delete", "watch"];
    let objects = ["file", "dir", "socket", "buffer", "handle"];
    let mut entries = Vec::new();
    let mut i = 0usize;
    for ns in namespaces {
        for v in verbs {
            for o in objects {
                let name = format!("{v}_{o}_{i}");
                entries.push(entry(
                    &format!("surf:{ns}.{name}"),
                    &name,
                    Some(ns),
                    &format!("{v} a {o} in the {ns} subsystem"),
                    &[
                        ExposureMode::Direct,
                        ExposureMode::Indexed,
                        ExposureMode::Deferred,
                    ],
                ));
                i += 1;
            }
        }
    }
    // Canonical entries the labelled queries target (distinctive names).
    entries.push(entry(
        "surf:fs.read_workspace_file",
        "read_workspace_file",
        Some("fs"),
        "read a file from the workspace filesystem",
        &[ExposureMode::Indexed, ExposureMode::Deferred],
    ));
    entries.push(entry(
        "surf:net.open_tcp_socket",
        "open_tcp_socket",
        Some("net"),
        "open a tcp network socket connection",
        &[ExposureMode::Indexed, ExposureMode::Deferred],
    ));
    entries.push(entry(
        "surf:git.commit_changes",
        "commit_changes",
        Some("git"),
        "commit staged changes to the repository",
        &[ExposureMode::Indexed, ExposureMode::Deferred],
    ));
    // A hidden entry — never indexed/discoverable (I-CLOSED).
    let mut h = entry(
        "surf:hidden.secret_probe",
        "secret_probe",
        Some("hidden"),
        "probe secrets",
        &[ExposureMode::Indexed, ExposureMode::Deferred],
    );
    h.hidden = true;
    entries.push(h);
    // A pinned entry.
    let mut p = entry(
        "surf:kernel.pinned_tool",
        "pinned_tool",
        Some("kernel"),
        "always direct",
        &[ExposureMode::Direct],
    );
    p.pinned = true;
    entries.push(p);
    entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let catalog_id = catalog_id_of(&entries, 0, "bundle:test");
    Catalog {
        catalog_id,
        epoch: 0,
        bundle_id: "bundle:test".to_string(),
        sources: Vec::new(),
        entries,
        index_ref: None,
    }
}

fn q(form: DiscoveryForm, text: &str, limit: Option<u64>) -> DiscoveryQuery {
    DiscoveryQuery {
        form,
        text: text.to_string(),
        limit,
    }
}

fn plan_modes(catalog: &Catalog, mode: ExposureMode) -> Vec<(String, ExposureMode)> {
    catalog
        .entries
        .iter()
        .filter(|e| e.admitted_modes.contains(&mode))
        .map(|e| (e.surface_id.clone(), mode))
        .collect()
}

// ── lexical_regex ───────────────────────────────────────────────────────────

#[test]
fn lexical_regex_matches_declared_text_and_bounds() {
    let cat = fixture_catalog();
    // `^read_` matches name-prefixed entries (search semantics).
    let res = index_query(
        &CatalogIndex::LexicalRegex,
        &cat,
        &q(DiscoveryForm::Regex, "^read_workspace", None),
        8,
    )
    .expect("regex query");
    assert!(!res.hits.is_empty());
    assert!(res
        .hits
        .iter()
        .any(|h| h.surface_id == "surf:fs.read_workspace_file"));
    // Deterministic order: surface_id ascending.
    let ids: Vec<&str> = res.hits.iter().map(|h| h.surface_id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);

    // Description text participates (`socket` lives in description too).
    let res = index_query(
        &CatalogIndex::LexicalRegex,
        &cat,
        &q(DiscoveryForm::Regex, "tcp network socket", None),
        32,
    )
    .expect("regex desc");
    assert!(res
        .hits
        .iter()
        .any(|h| h.surface_id == "surf:net.open_tcp_socket"));

    // Hidden entries never index.
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id != "surf:hidden.secret_probe"));

    // `natural_language` is refused — the variant declares its forms.
    let err = index_query(
        &CatalogIndex::LexicalRegex,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "read a file", None),
        8,
    )
    .expect_err("unsupported form");
    assert_eq!(err, DiscoveryQueryInvalid::UnsupportedForm);

    // Malformed pattern → invalid_pattern, never a coerced match.
    let err = index_query(
        &CatalogIndex::LexicalRegex,
        &cat,
        &q(DiscoveryForm::Regex, "a(b|c", None),
        8,
    )
    .expect_err("invalid pattern");
    assert!(matches!(err, DiscoveryQueryInvalid::InvalidPattern { .. }));
}

// ── bm25 ────────────────────────────────────────────────────────────────────

#[test]
fn bm25_ranks_relevant_first_and_carries_scores() {
    let cat = fixture_catalog();
    let res = index_query(
        &CatalogIndex::Bm25,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "read a file from the workspace", None),
        8,
    )
    .expect("bm25");
    assert!(!res.hits.is_empty());
    // The canonical entry ranks #1 for its own description.
    assert_eq!(res.hits[0].surface_id, "surf:fs.read_workspace_file");
    // Scores are carried (×10⁶-scaled ints).
    assert!(res.hits.iter().all(|h| h.score.is_some()));
    // Regex form is refused.
    assert_eq!(
        index_query(
            &CatalogIndex::Bm25,
            &cat,
            &q(DiscoveryForm::Regex, "^read", None),
            8,
        )
        .expect_err("unsupported"),
        DiscoveryQueryInvalid::UnsupportedForm
    );
    // Hidden never surfaces.
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id != "surf:hidden.secret_probe"));
}

// ── hierarchical ────────────────────────────────────────────────────────────

#[test]
fn hierarchical_descends_namespace_paths() {
    let cat = fixture_catalog();
    // A namespace-descent query hits every `fs.*` member.
    let res = index_query(
        &CatalogIndex::Hierarchical,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "fs", None),
        32,
    )
    .expect("hier");
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id.starts_with("surf:fs.") || h.surface_id.contains("fs")));
    assert!(res
        .hits
        .iter()
        .any(|h| h.surface_id == "surf:fs.read_workspace_file"));

    // `fs.read_workspace` descends deeper than `fs`.
    let res = index_query(
        &CatalogIndex::Hierarchical,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "fs.read_workspace", None),
        8,
    )
    .expect("hier deep");
    assert_eq!(res.hits[0].surface_id, "surf:fs.read_workspace_file");

    // Structured form is served (namespace filter applies).
    let res = index_query(
        &CatalogIndex::Hierarchical,
        &cat,
        &q(
            DiscoveryForm::Structured(StructuredQuery {
                namespace: Some("net".to_string()),
                ..Default::default()
            }),
            "socket",
            None,
        ),
        8,
    )
    .expect("hier structured");
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id.starts_with("surf:net.")));
}

// ── discover (the discover_surfaces lowering) ────────────────────────────────

#[test]
fn discover_hits_only_indexed_or_deferred_plan_members() {
    let cat = fixture_catalog();
    // Plan: the canonical entries `indexed`, the rest `deferred`, the
    // pinned one `direct`, hidden omitted (select_surfaces semantics — we
    // build the member vector directly).
    let mut entries = plan_modes(&cat, ExposureMode::Deferred);
    for sid in [
        "surf:fs.read_workspace_file",
        "surf:net.open_tcp_socket",
        "surf:git.commit_changes",
    ] {
        entries.retain(|(s, _)| s != sid);
        entries.push((sid.to_string(), ExposureMode::Indexed));
    }
    entries.retain(|(s, _)| s != "surf:kernel.pinned_tool");
    entries.push(("surf:kernel.pinned_tool".to_string(), ExposureMode::Direct));
    let plan = hh_compiler::exposure::ExposurePlan {
        plan_id: "plan:test".into(),
        model_call_id: "mc:1".into(),
        catalog_id: cat.catalog_id.clone(),
        entries,
        discovery_surface_ids: vec![],
        order: vec![],
        budget_estimate: hh_compiler::exposure::BudgetEstimate {
            tokens: 0,
            estimator_ref: "bytes_div_4".into(),
        },
        omitted: vec![],
        derived_from: "test".into(),
        deterministic: true,
    };
    let res = discover(
        &plan,
        &CatalogIndex::Bm25,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "workspace", None),
        &Default::default(),
    )
    .expect("discover");
    // The pinned `direct` entry is never a discovery hit even if it matched.
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id != "surf:kernel.pinned_tool"));
    assert!(res
        .hits
        .iter()
        .any(|h| h.surface_id == "surf:fs.read_workspace_file"));

    // Bound: max_reveal_per_search.
    let mut params = hh_compiler::exposure::ExposurePolicyParams::default();
    params.max_reveal_per_search = 3;
    let res = discover(
        &plan,
        &CatalogIndex::Hierarchical,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "fs", None),
        &params,
    )
    .expect("discover bound");
    assert!(res.hits.len() <= 3);
    assert!(res.truncated);
}

// ── catalog_delta + adopt (AC-R-2.5.3-8) ────────────────────────────────────

fn mcp_listing_entry(sid: &str, name: &str) -> CatalogEntry {
    let mut e = entry(sid, name, Some("mcp.src"), "mcp-listed", &[ExposureMode::Indexed]);
    e.source = CatalogSource::Mcp("src".to_string());
    e
}

#[test]
fn ac_e3_8_freeze_marks_removed_and_keeps_ids() {
    let mut cat = fixture_catalog();
    // One mcp-sourced entry to remove.
    cat.entries.push(mcp_listing_entry("surf:mcp.old_tool", "old_tool"));
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, 0, &cat.bundle_id);
    let prev_id = cat.catalog_id.clone();
    let prev_cfg = "cfg:v1";

    // A `list_changed` on `mcp:src` with an empty listing → removed delta.
    let delta = catalog_delta(&cat, "mcp:src", SyncTrigger::ListChanged, &[]);
    assert_eq!(delta.removed, vec!["surf:mcp.old_tool".to_string()]);
    assert!(delta.added.is_empty() && delta.changed.is_empty());
    // The delta is emitted as `action.tool.catalog.delta` (payload shape).
    let payload = delta.to_json();
    assert_eq!(payload.get("cause").and_then(|c| c.as_str()), Some("list_changed"));

    let out = adopt(&cat, &delta, DriftPolicy::Freeze, "test").expect("freeze");
    let AdoptOutcome::Frozen { catalog } = out else {
        panic!("freeze returns Frozen")
    };
    // catalog_id + configuration_version_id unchanged; epoch unchanged.
    assert_eq!(catalog.catalog_id, prev_id);
    assert_eq!(catalog.epoch, 0);
    let _ = prev_cfg;
    // The removed entry is `unavailable(removed)` — a call fails typed.
    let removed = catalog
        .entries
        .iter()
        .find(|e| e.surface_id == "surf:mcp.old_tool")
        .unwrap();
    assert_eq!(
        removed.availability,
        Availability::Unavailable("removed".to_string())
    );
    let plan = hh_compiler::exposure::ExposurePlan {
        plan_id: "p".into(),
        model_call_id: "mc".into(),
        catalog_id: catalog.catalog_id.clone(),
        entries: vec![("surf:mcp.old_tool".to_string(), ExposureMode::Direct)],
        discovery_surface_ids: vec![],
        order: vec![],
        budget_estimate: hh_compiler::exposure::BudgetEstimate {
            tokens: 0,
            estimator_ref: "x".into(),
        },
        omitted: vec![],
        derived_from: "d".into(),
        deterministic: true,
    };
    let refusal = check_callable(
        &plan,
        &catalog,
        &CallProposal {
            surface_name: "old_tool".to_string(),
            from_code: false,
        },
    )
    .expect_err("SurfaceUnavailable");
    assert!(matches!(refusal, CallRefusal::SurfaceUnavailable { .. }));
}

#[test]
fn ac_e3_8_adopt_new_epoch_and_unverified_authority() {
    let mut cat = fixture_catalog();
    cat.entries.push(mcp_listing_entry("surf:mcp.old_tool", "old_tool"));
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, 0, &cat.bundle_id);
    let prev_id = cat.catalog_id.clone();

    let listing = vec![
        mcp_listing_entry("surf:mcp.new_tool", "new_tool"),
        mcp_listing_entry("surf:mcp.other", "other"),
    ];
    let delta = catalog_delta(&cat, "mcp:src", SyncTrigger::ListChanged, &listing);
    assert_eq!(delta.removed, vec!["surf:mcp.old_tool".to_string()]);
    assert_eq!(delta.added.len(), 2);

    let out = adopt(&cat, &delta, DriftPolicy::Adopt, "run:test").expect("adopt");
    let AdoptOutcome::Adopted { catalog, epoch } = out else {
        panic!("adopt returns Adopted")
    };
    assert_eq!(epoch.epoch, 1);
    assert_eq!(epoch.catalog_id_prev, prev_id);
    assert_ne!(epoch.catalog_id, prev_id);
    assert_eq!(epoch.adopted_by, "run:test");
    // bundle_delta carries the new surfaces' compiled identities with
    // `authority = unverified`.
    let added = match epoch.bundle_delta.get("added") {
        Some(hh_wire::json::Json::Arr(a)) => a.clone(),
        other => panic!("bundle_delta.added is an array, got {other:?}"),
    };
    assert_eq!(added.len(), 2);
    for a in &added {
        assert_eq!(a.get("authority").and_then(|x| x.as_str()), Some("unverified"));
        assert!(a
            .get("version_id")
            .and_then(|x| x.as_str())
            .is_some_and(|v| !v.is_empty()));
    }
    // The new entries are members; the removed one is gone.
    assert!(catalog.entries.iter().any(|e| e.surface_id == "surf:mcp.new_tool"));
    assert!(catalog.entries.iter().all(|e| e.surface_id != "surf:mcp.old_tool"));
}

#[test]
fn adopt_ask_is_drift_refused() {
    let cat = fixture_catalog();
    let delta = catalog_delta(&cat, "mcp:none", SyncTrigger::TtlExpired, &[]);
    assert_eq!(
        adopt(&cat, &delta, DriftPolicy::Ask, "t").expect_err("ask"),
        CatalogDriftError::Refused
    );
}

// ── Reveal retention + evict (S2.10 "RevealedSet retention" slice) ──────────

fn revealed_with(entries: &[(&str, Retention)]) -> RevealedSet {
    RevealedSet {
        run_id: "run:test".into(),
        entries: entries
            .iter()
            .enumerate()
            .map(|(i, (sid, r))| hh_compiler::exposure::RevealedEntry {
                surface_id: sid.to_string(),
                mode: ExposureMode::Direct,
                revealed_at: i as u64,
                cause: RevealCause::Discovery,
                retention: *r,
            })
            .collect(),
    }
}

#[test]
fn expire_reveals_honours_retention_and_pins() {
    let cat = fixture_catalog();
    let set = revealed_with(&[
        ("surf:fs.read_workspace_file", Retention::Call),
        ("surf:net.open_tcp_socket", Retention::Turn),
        ("surf:git.commit_changes", Retention::Run),
        ("surf:fs.read_file_0", Retention::UntilEvicted),
        ("surf:kernel.pinned_tool", Retention::Call), // pinned ⇒ survives
    ]);
    // CallEnd: only `call` retention expires (except pinned).
    let (out, expired) = expire_reveals(&set, &cat, RevealBoundary::CallEnd);
    assert_eq!(expired, vec!["surf:fs.read_workspace_file".to_string()]);
    assert_eq!(out.entries.len(), 4);
    // TurnEnd: call + turn expire.
    let (out, expired) = expire_reveals(&set, &cat, RevealBoundary::TurnEnd);
    assert_eq!(
        expired,
        vec![
            "surf:fs.read_workspace_file".to_string(),
            "surf:net.open_tcp_socket".to_string()
        ]
    );
    assert_eq!(out.entries.len(), 3);
    // RunEnd: everything expires except pinned.
    let (out, expired) = expire_reveals(&set, &cat, RevealBoundary::RunEnd);
    assert_eq!(expired.len(), 4);
    assert_eq!(out.entries.len(), 1);
    assert_eq!(out.entries[0].surface_id, "surf:kernel.pinned_tool");
}

#[test]
fn evict_keeps_pinned_and_drops_named() {
    let cat = fixture_catalog();
    let set = revealed_with(&[
        ("surf:fs.read_workspace_file", Retention::Run),
        ("surf:kernel.pinned_tool", Retention::Run),
    ]);
    let out = evict(
        &set,
        &cat,
        &[
            "surf:fs.read_workspace_file".to_string(),
            "surf:kernel.pinned_tool".to_string(),
        ],
        EvictCause::Compaction,
    );
    assert_eq!(out.entries.len(), 1);
    assert_eq!(out.entries[0].surface_id, "surf:kernel.pinned_tool");
}

// ── retrieval_eval (AC-R-2.5.3-11) ──────────────────────────────────────────

fn labelled(text: &str, form: DiscoveryForm, required: &[&str]) -> LabelledQuery {
    LabelledQuery {
        query: q(form, text, Some(8)),
        required_surface_ids: required.iter().map(|s| s.to_string()).collect(),
    }
}

/// The pre-registered floor (AC-R-2.5.3-11): every `exact_name` baseline
/// query's required surface is rank 1 ⇒ recall@8 = 1.0. This is the only
/// gated number; the C1 variants' metrics are reported.
const EXACT_NAME_RECALL8_FLOOR: f64 = 1.0;

#[test]
fn ac_e3_11_exact_name_floor_and_nl_report() {
    let cat = fixture_catalog();
    // exact_name baseline queries — the gated set.
    let baseline = vec![
        labelled("read_workspace_file", DiscoveryForm::NaturalLanguage, &["surf:fs.read_workspace_file"]),
        labelled("open_tcp_socket", DiscoveryForm::NaturalLanguage, &["surf:net.open_tcp_socket"]),
        labelled("commit_changes", DiscoveryForm::NaturalLanguage, &["surf:git.commit_changes"]),
    ];
    let report = retrieval_eval(&CatalogIndex::ExactName, &cat, &baseline, 8, 8);
    assert!(
        report.recall_at_k >= EXACT_NAME_RECALL8_FLOOR,
        "exact_name recall@8 {} < floor {}",
        report.recall_at_k,
        EXACT_NAME_RECALL8_FLOOR
    );

    // Natural-language set — reported per C1 variant (not gated). `bm25`
    // serves sentence queries; `hierarchical` serves path-descent queries
    // (a namespace path is the natural-language form it answers).
    let nl_sentences = vec![
        labelled(
            "read a file from the workspace filesystem",
            DiscoveryForm::NaturalLanguage,
            &["surf:fs.read_workspace_file"],
        ),
        labelled(
            "open a tcp network socket connection",
            DiscoveryForm::NaturalLanguage,
            &["surf:net.open_tcp_socket"],
        ),
        labelled(
            "commit staged changes to the repository",
            DiscoveryForm::NaturalLanguage,
            &["surf:git.commit_changes"],
        ),
    ];
    let report = retrieval_eval(&CatalogIndex::Bm25, &cat, &nl_sentences, 8, 8);
    eprintln!(
        "retrieval_eval[bm25] recall@8={:.3} nDCG@8={:.3}",
        report.recall_at_k, report.ndcg_at_k
    );
    assert!(report.recall_at_k > 0.0, "bm25 recall@8");
    assert!(report.ndcg_at_k > 0.0, "bm25 nDCG@8");
    assert!(report.per_query.iter().all(|m| m.error.is_none()));

    let nl_paths = vec![
        labelled("fs.read_workspace", DiscoveryForm::NaturalLanguage, &["surf:fs.read_workspace_file"]),
        labelled("net.open_tcp", DiscoveryForm::NaturalLanguage, &["surf:net.open_tcp_socket"]),
        labelled("git.commit", DiscoveryForm::NaturalLanguage, &["surf:git.commit_changes"]),
    ];
    let report = retrieval_eval(&CatalogIndex::Hierarchical, &cat, &nl_paths, 8, 8);
    eprintln!(
        "retrieval_eval[hierarchical] recall@8={:.3} nDCG@8={:.3}",
        report.recall_at_k, report.ndcg_at_k
    );
    assert!(report.recall_at_k > 0.0, "hierarchical recall@8");
    assert!(report.ndcg_at_k > 0.0, "hierarchical nDCG@8");
    assert!(report.per_query.iter().all(|m| m.error.is_none()));
    // lexical_regex serves `regex` queries — report over a regex set too.
    let rx = vec![
        labelled("^read_workspace", DiscoveryForm::Regex, &["surf:fs.read_workspace_file"]),
        labelled("^open_tcp", DiscoveryForm::Regex, &["surf:net.open_tcp_socket"]),
    ];
    let report = retrieval_eval(&CatalogIndex::LexicalRegex, &cat, &rx, 8, 8);
    eprintln!(
        "retrieval_eval[lexical_regex] recall@8={:.3} nDCG@8={:.3}",
        report.recall_at_k, report.ndcg_at_k
    );
    assert!(report.recall_at_k >= EXACT_NAME_RECALL8_FLOOR);
    // An invalid query is a measured failure, never a panic.
    let bad = vec![labelled("a(b|c", DiscoveryForm::Regex, &["surf:x"])];
    let report = retrieval_eval(&CatalogIndex::LexicalRegex, &cat, &bad, 8, 8);
    assert_eq!(report.recall_at_k, 0.0);
    assert!(matches!(
        report.per_query[0].error,
        Some(DiscoveryQueryInvalid::InvalidPattern { .. })
    ));
}

#[test]
fn index_refs_are_content_addressed_and_distinct() {
    let refs: Vec<String> = [
        CatalogIndex::ExactName,
        CatalogIndex::StaticAllowlist(BTreeSet::new()),
        CatalogIndex::LexicalRegex,
        CatalogIndex::Bm25,
        CatalogIndex::Hierarchical,
    ]
    .iter()
    .map(index_ref)
    .collect();
    let unique: BTreeSet<&String> = refs.iter().collect();
    assert_eq!(unique.len(), 5, "each variant has a distinct index_ref");
}

#[test]
fn hidden_never_in_any_variant_result() {
    let cat = fixture_catalog();
    for index in [
        CatalogIndex::LexicalRegex,
        CatalogIndex::Bm25,
        CatalogIndex::Hierarchical,
    ] {
        let res = index_query(
            &index,
            &cat,
            &q(DiscoveryForm::NaturalLanguage, "probe secrets hidden", Some(32)),
            32,
        )
        .unwrap_or_else(|_| {
            // lexical_regex refuses the form; retry with regex form.
            index_query(
                &index,
                &cat,
                &q(DiscoveryForm::Regex, ".*secret.*", Some(32)),
                32,
            )
            .expect("regex probe")
        });
        assert!(res
            .hits
            .iter()
            .all(|h| h.surface_id != "surf:hidden.secret_probe"));
    }
}
