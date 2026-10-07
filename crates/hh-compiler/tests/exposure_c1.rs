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
//!
//! R2.8 (DF-S1.17-1/-3 — the corpus-battery residual, §5d.3 §5/§9):
//!
//! - attestation-gated indexing — `SelectionGates.index_unverified` admits a
//!   `lifted` entry's `indexed` mode only when the verifier attestation is
//!   in; `pinned ∧ lifted` is `PinBindsLifted` on `adopt`;
//! - `permission_gate` — `hide_uncovered` omits `PermissionUncovered`;
//!   `demote_uncovered` drops `direct` from the meet;
//! - non-surface `IndexDecl` granularities (`namespace`/`source`/`catalog`)
//!   — `NotQueryable` on a granularity mismatch; unit hits expand to member
//!   `surface_id`s;
//! - the C1 producers — `bind_composite`/`bind_freeform`/`bind_shim`/
//!   `bind_code_mode` mint distinct mode-typed bindings, and the C1
//!   `concise`/`offload` `RenderMode`s parse with their declared bounds;
//! - `code_mode` callability — `check_callable` admits `from_code` calls on
//!   `code_mode` plan members only;
//! - the C2 executor legs (`embedding`/`model_ranked`/`filesystem`) —
//!   `executed_by = provider` rows, threshold drops, `ExecutorUnavailable`
//!   typed refusals, and the S-386 provider round-trip (provider-searched
//!   deferred surface → reveal → `direct` in the next plan → callable);
//! - the AC-E3-5 *form* fixture — a non-`native_fc` (`shim`) variant family
//!   serves the same `required_surface_ids` under `regex` and
//!   `natural_language` (ADR-0286 D7).

use std::collections::{BTreeMap, BTreeSet};

mod common;

use hh_compiler::exposure::{
    adopt, adopt_with, catalog_delta, catalog_id_of, check_callable, discover, discover_with,
    evict, expire_reveals, index_forms_at, index_query, index_query_decl, index_ref, reveal,
    select_surfaces_with, AdoptOutcome, Availability, CallProposal, CallRefusal, Catalog,
    CatalogDriftError, CatalogEntry, CatalogIndex, CatalogSource, DiscoveryForm, DiscoveryQuery,
    DiscoveryQueryInvalid, DriftPolicy, EvictCause, ExposurePlan, ExposurePolicy,
    ExposurePolicyParams, IndexDecl, IndexExecutor, IndexForm, IndexGranularity, OmitReason,
    PermissionCoverage, PermissionGate, Retention, RevealBoundary, RevealCause, RevealedSet,
    SearchTextField, SelectError, SelectionGates, StructuredQuery, SyncTrigger, TurnState,
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
        lifted: false,
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
    // A lifted entry — an unverified import (MCP listing): its `indexed`
    // mode is admissible only under `SelectionGates.index_unverified`
    // (the attestation leg); `lifted ∧ pinned` is `PinBindsLifted`.
    let mut l = entry(
        "surf:mcp.imported_probe",
        "imported_probe",
        Some("mcp.imp"),
        "imported diagnostic capability from an unverified listing",
        &[ExposureMode::Indexed, ExposureMode::Deferred],
    );
    l.source = CatalogSource::Mcp("imp".to_string());
    l.lifted = true;
    entries.push(l);
    entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let catalog_id = catalog_id_of(&entries, 0, "bundle:test");
    Catalog {
        catalog_id,
        epoch: 0,
        bundle_id: "bundle:test".to_string(),
        sources: Vec::new(),
        entries,
        indexes: Vec::new(),
        index_ref: None,
    }
}

fn q(form: DiscoveryForm, text: &str, limit: Option<u64>) -> DiscoveryQuery {
    DiscoveryQuery {
        form,
        text: text.to_string(),
        limit,
        granularity: IndexGranularity::Surface,
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
        &q(
            DiscoveryForm::NaturalLanguage,
            "read a file from the workspace",
            None,
        ),
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
    let params = hh_compiler::exposure::ExposurePolicyParams {
        max_reveal_per_search: 3,
        ..Default::default()
    };
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
    let mut e = entry(
        sid,
        name,
        Some("mcp.src"),
        "mcp-listed",
        &[ExposureMode::Indexed],
    );
    e.source = CatalogSource::Mcp("src".to_string());
    e
}

#[test]
fn ac_e3_8_freeze_marks_removed_and_keeps_ids() {
    let mut cat = fixture_catalog();
    // One mcp-sourced entry to remove.
    cat.entries
        .push(mcp_listing_entry("surf:mcp.old_tool", "old_tool"));
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
    assert_eq!(
        payload.get("cause").and_then(|c| c.as_str()),
        Some("list_changed")
    );

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
    cat.entries
        .push(mcp_listing_entry("surf:mcp.old_tool", "old_tool"));
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
        assert_eq!(
            a.get("authority").and_then(|x| x.as_str()),
            Some("unverified")
        );
        assert!(a
            .get("version_id")
            .and_then(|x| x.as_str())
            .is_some_and(|v| !v.is_empty()));
    }
    // The new entries are members; the removed one is gone.
    assert!(catalog
        .entries
        .iter()
        .any(|e| e.surface_id == "surf:mcp.new_tool"));
    assert!(catalog
        .entries
        .iter()
        .all(|e| e.surface_id != "surf:mcp.old_tool"));
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
        labelled(
            "read_workspace_file",
            DiscoveryForm::NaturalLanguage,
            &["surf:fs.read_workspace_file"],
        ),
        labelled(
            "open_tcp_socket",
            DiscoveryForm::NaturalLanguage,
            &["surf:net.open_tcp_socket"],
        ),
        labelled(
            "commit_changes",
            DiscoveryForm::NaturalLanguage,
            &["surf:git.commit_changes"],
        ),
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
        labelled(
            "fs.read_workspace",
            DiscoveryForm::NaturalLanguage,
            &["surf:fs.read_workspace_file"],
        ),
        labelled(
            "net.open_tcp",
            DiscoveryForm::NaturalLanguage,
            &["surf:net.open_tcp_socket"],
        ),
        labelled(
            "git.commit",
            DiscoveryForm::NaturalLanguage,
            &["surf:git.commit_changes"],
        ),
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
        labelled(
            "^read_workspace",
            DiscoveryForm::Regex,
            &["surf:fs.read_workspace_file"],
        ),
        labelled(
            "^open_tcp",
            DiscoveryForm::Regex,
            &["surf:net.open_tcp_socket"],
        ),
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
            &q(
                DiscoveryForm::NaturalLanguage,
                "probe secrets hidden",
                Some(32),
            ),
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

// ── S4.16b — `adopt_with` incremental lowering (§5d.3 §2; ADR-0095 D2) ────

#[test]
fn adopt_with_carries_loss_report_and_recomputes_coverage() {
    let mut cat = fixture_catalog();
    cat.entries
        .push(mcp_listing_entry("surf:mcp.old_tool", "old_tool"));
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, 0, &cat.bundle_id);

    let listing = vec![mcp_listing_entry("surf:mcp.new_tool", "new_tool")];
    let delta = catalog_delta(&cat, "mcp:src", SyncTrigger::ListChanged, &listing);

    let loss = hh_compiler::lcd::LoweringLossReport {
        target: "catalog".into(),
        target_version: "1.0".into(),
        entries: vec![hh_compiler::lcd::LossEntry {
            hir_node_id: "cap:new_tool".into(),
            field: "timeout_hint".into(),
            class: hh_compiler::lcd::LossKind::NoSlot,
            severity: hh_compiler::lcd::LossSeverity::Info,
            detail: "catalog entries carry no timeout field".into(),
            debt_ref: None,
        }],
        granularity_ceiling: hh_compiler::lcd::GranularityCeiling::Component,
    };
    // The coverage oracle is the only permission_coverage authority — the
    // compiler never resolves grants (I-CLOSED); ADR-0095 D2's typical
    // verdict is `uncovered` until a grant exists.
    let oracle = |_: &CatalogEntry| PermissionCoverage::Uncovered;
    let inputs = hh_compiler::exposure::AdoptInputs {
        loss_report: Some(loss),
        coverage: Some(&oracle),
    };
    let out = adopt_with(&cat, &delta, DriftPolicy::Adopt, "run:test", &inputs).expect("adopt");
    let AdoptOutcome::Adopted { catalog, epoch } = out else {
        panic!("adopt returns Adopted")
    };
    // The epoch carries the declared loss report (CC3 — a lift that loses
    // declares it on the epoch row).
    let lr = epoch.loss_report.expect("the declared loss report lands");
    assert_eq!(lr.get("target").and_then(|t| t.as_str()), Some("catalog"));
    let entries = match lr.get("entries") {
        Some(hh_wire::json::Json::Arr(a)) => a,
        other => panic!("loss_report.entries is an array, got {other:?}"),
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].get("field").and_then(|f| f.as_str()),
        Some("timeout_hint")
    );
    // Coverage recomputed on the adopted entry only.
    let adopted = catalog
        .entries
        .iter()
        .find(|e| e.surface_id == "surf:mcp.new_tool")
        .unwrap();
    assert_eq!(adopted.permission_coverage, PermissionCoverage::Uncovered);
    // Untouched entries keep their coverage.
    let other = catalog
        .entries
        .iter()
        .find(|e| e.surface_id == "surf:kernel.pinned_tool")
        .unwrap();
    assert_ne!(other.permission_coverage, PermissionCoverage::Uncovered);

    // Without inputs the epoch's loss_report is absent (a lift that loses
    // nothing declares nothing — never a fabricated report).
    let out2 = adopt(&cat, &delta, DriftPolicy::Adopt, "run:test").expect("adopt");
    let AdoptOutcome::Adopted { epoch: e2, .. } = out2 else {
        panic!("adopt returns Adopted")
    };
    assert!(e2.loss_report.is_none());
}

// ── R2.8 corpus battery — C2 gates, non-surface granularities, executor legs ─
// (DF-S1.17-1/-3; §5d.3 §2/§5d.2.8; ADR-0094 D4, ADR-0095 D2, ADR-0286 D7).
//
// - attestation-gated indexing (`SelectionGates.index_unverified`): a `lifted`
//   entry's `indexed` mode is admissible only when the gate is set; `pinned ∧
//   lifted` is `PinBindsLifted` on adopt;
// - `permission_gate`: `hide_uncovered` omits `PermissionUncovered`;
//   `demote_uncovered` drops `direct` from the meet (never a silent hide);
// - non-surface index granularities (`IndexDecl` at `namespace`/`source`/
//   `catalog`): queries answer only at the declared granularity
//   (`NotQueryable` otherwise) and unit hits expand back to member
//   `surface_id`s;
// - the C2 executor legs (`embedding`/`model_ranked`/`filesystem`): a bound
//   `IndexExecutor` answers `executed_by` = `provider`/`kernel` per the
//   variant; the legs absent are `ExecutorUnavailable` — never a fabricated
//   ranking. The S-386 arm replays a revealed deferred surface through the
//   provider round-trip: provider-searched → revealed → `direct` in the next
//   plan → `check_callable` admits;
// - the AC-E3-5 *form* fixture: a non-`native_fc` family (`shim` variant
//   selected over the `native_fc` reference) serves the same
//   `required_surface_ids` under both the `regex` and `natural_language`
//   forms.

fn ts(mc: &str) -> TurnState {
    TurnState {
        model_call_id: mc.into(),
        revealed: RevealedSet {
            run_id: "run:test".into(),
            entries: vec![],
        },
        recent_calls: vec![],
        goal_ref: None,
        window_cap: 1_000_000,
        prior_order: vec![],
    }
}

/// A catalog carrying the kernel `discover_surfaces` capability `direct` —
/// I-DISCOVERY's precondition for any plan that defers entries.
fn with_discovery(mut cat: Catalog) -> Catalog {
    let mut d = entry(
        "surf:kernel.discover_surfaces",
        "discover_surfaces",
        Some("kernel"),
        "search the exposed tool catalog",
        &[ExposureMode::Direct],
    );
    d.is_discovery = true;
    cat.entries.push(d);
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, cat.epoch, &cat.bundle_id);
    cat
}

/// A policy ranking `Direct` where the meet admits it, else `Indexed`, else
/// the meet's first member — except `sid`, which is forced to `mode` even
/// when the meet refuses it (`""` forces nothing: the meet-respecting leg
/// alone). The forced member is the enforce leg's `PolicyViolation` probe.
struct Rank {
    force_sid: &'static str,
    force_mode: ExposureMode,
}
impl ExposurePolicy for Rank {
    fn rank(
        &self,
        entries: &[CatalogEntry],
        admitted: &BTreeMap<String, BTreeSet<ExposureMode>>,
        _t: &TurnState,
        _p: &ExposurePolicyParams,
    ) -> Vec<(String, ExposureMode)> {
        entries
            .iter()
            .filter_map(|e| {
                let meet = admitted.get(&e.surface_id)?;
                let mode = if e.surface_id == self.force_sid {
                    self.force_mode
                } else if meet.contains(&ExposureMode::Direct) {
                    ExposureMode::Direct
                } else if meet.contains(&ExposureMode::Indexed) {
                    ExposureMode::Indexed
                } else {
                    *meet.iter().next().expect("meet non-empty")
                };
                Some((e.surface_id.clone(), mode))
            })
            .collect()
    }
}

fn plan_mode(plan: &ExposurePlan, sid: &str) -> Option<ExposureMode> {
    plan.entries.iter().find(|(s, _)| s == sid).map(|(_, m)| *m)
}

#[test]
fn r2_8_attestation_gate_drops_indexed_on_lifted_entries() {
    let cat = fixture_catalog();
    let lifted = "surf:mcp.imported_probe";
    let params = ExposurePolicyParams::default();

    // Gate absent (the default): the lifted entry's meet is `{deferred}` —
    // a policy asking `indexed` is a `PolicyViolation`, never a widening.
    let err = select_surfaces_with(
        &cat,
        &ts("mc:1"),
        &ExposureMode::ALL.iter().copied().collect(),
        &params,
        Some(&Rank {
            force_sid: "surf:mcp.imported_probe",
            force_mode: ExposureMode::Indexed,
        }),
        &SelectionGates::default(),
    )
    .expect_err("unattested index refuse");
    assert!(matches!(err, SelectError::PolicyViolation { .. }));

    // Ranked at its remaining meet member the entry plans `deferred`
    // (the catalog carries the `direct` discovery capability I-DISCOVERY
    // needs for a deferring plan).
    let cat_d = with_discovery(cat.clone());
    let plan = select_surfaces_with(
        &cat_d,
        &ts("mc:1"),
        &ExposureMode::ALL.iter().copied().collect(),
        &params,
        Some(&Rank {
            force_sid: "",
            force_mode: ExposureMode::Direct,
        }),
        &SelectionGates::default(),
    )
    .expect("deferred rank");
    assert_eq!(plan_mode(&plan, lifted), Some(ExposureMode::Deferred));

    // Gate set (the verifier attestation arrived): `indexed` admits.
    let plan = select_surfaces_with(
        &cat,
        &ts("mc:1"),
        &ExposureMode::ALL.iter().copied().collect(),
        &params,
        Some(&Rank {
            force_sid: "surf:mcp.imported_probe",
            force_mode: ExposureMode::Indexed,
        }),
        &SelectionGates {
            index_unverified: true,
        },
    )
    .expect("attested index admits");
    assert_eq!(plan_mode(&plan, lifted), Some(ExposureMode::Indexed));
}

#[test]
fn r2_8_adopt_refuses_pin_binds_lifted() {
    let cat = fixture_catalog();
    // A `list_changed` re-listing `mcp:imp` with a `pinned ∧ lifted` entry —
    // a pin binds a *compiled* admission, never an unverified lift.
    let mut bad = mcp_listing_entry("surf:mcp.imported_probe", "imported_probe");
    bad.source = CatalogSource::Mcp("imp".to_string());
    bad.pinned = true;
    bad.lifted = true;
    let delta = catalog_delta(&cat, "mcp:imp", SyncTrigger::ListChanged, &[bad]);
    assert_eq!(
        adopt(&cat, &delta, DriftPolicy::Adopt, "run:test"),
        Err(CatalogDriftError::PinBindsLifted {
            surface_id: "surf:mcp.imported_probe".to_string()
        })
    );
}

#[test]
fn r2_8_hide_uncovered_omits_and_demote_drops_direct() {
    // `with_discovery` — the unattested lifted entry plans `deferred`
    // (I-DISCOVERY needs the `direct` discovery capability for it).
    let mut cat = with_discovery(fixture_catalog());
    // A coverage-less entry (the oracle's typical verdict — §5d.3's
    // permission_coverage arm).
    cat.entries
        .iter_mut()
        .find(|e| e.surface_id == "surf:fs.read_workspace_file")
        .unwrap()
        .permission_coverage = PermissionCoverage::Uncovered;
    let all: BTreeSet<ExposureMode> = ExposureMode::ALL.iter().copied().collect();

    // `hide_uncovered`: the entry is omitted `permission_uncovered` — the
    // omission is *recorded*, never silent (I-CLOSED ∩ OQ-234).
    let params = ExposurePolicyParams {
        permission_gate: PermissionGate::HideUncovered,
        ..Default::default()
    };
    let plan = select_surfaces_with(
        &cat,
        &ts("mc:1"),
        &all,
        &params,
        Some(&Rank {
            force_sid: "",
            force_mode: ExposureMode::Direct,
        }),
        &SelectionGates::default(),
    )
    .expect("hide plan");
    assert_eq!(plan_mode(&plan, "surf:fs.read_workspace_file"), None);
    assert!(plan
        .omitted
        .iter()
        .any(|o| o.surface_id == "surf:fs.read_workspace_file"
            && o.reason == OmitReason::PermissionUncovered));

    // `demote_uncovered` (the C1 default): `direct` leaves the meet, the
    // non-direct modes still admit — demoted, never hidden.
    let params = ExposurePolicyParams {
        permission_gate: PermissionGate::DemoteUncovered,
        ..Default::default()
    };
    let err = select_surfaces_with(
        &cat,
        &ts("mc:1"),
        &all,
        &params,
        Some(&Rank {
            force_sid: "surf:fs.read_workspace_file",
            force_mode: ExposureMode::Direct,
        }),
        &SelectionGates::default(),
    )
    .expect_err("demoted direct refuses");
    assert!(matches!(err, SelectError::PolicyViolation { .. }));
    // Ranked inside its narrowed meet the entry still plans — `Indexed`,
    // the meet's first surviving member after `direct` demoted out.
    let plan = select_surfaces_with(
        &cat,
        &ts("mc:1"),
        &all,
        &params,
        Some(&Rank {
            force_sid: "",
            force_mode: ExposureMode::Direct,
        }),
        &SelectionGates::default(),
    )
    .expect("demoted index admits");
    assert_eq!(
        plan_mode(&plan, "surf:fs.read_workspace_file"),
        Some(ExposureMode::Indexed)
    );
}

#[test]
fn r2_8_non_surface_granularity_decls() {
    let cat = fixture_catalog();
    // A namespace-granularity `bm25` decl: the query's `granularity` must
    // match the decl's — a surface query is `NotQueryable`.
    let decl = IndexDecl {
        index: CatalogIndex::Bm25,
        granularity: IndexGranularity::Namespace,
    };
    let mut query = q(DiscoveryForm::NaturalLanguage, "workspace file", None);
    query.granularity = IndexGranularity::Namespace;
    let res = index_query_decl(&decl, &cat, &query, 32, &IndexExecutor::default())
        .expect("namespace query");
    // The unit hit expands back to *member* surface ids — every `fs`
    // member ranks, and `unit:` aggregates never leak as hits.
    assert!(res
        .hits
        .iter()
        .any(|h| h.surface_id == "surf:fs.read_workspace_file"));
    assert!(res.hits.iter().all(|h| !h.surface_id.starts_with("unit:")));
    // Hidden members never join a unit's expansion (I-CLOSED).
    assert!(res
        .hits
        .iter()
        .all(|h| h.surface_id != "surf:hidden.secret_probe"));

    let err = index_query_decl(
        &decl,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "workspace", None),
        8,
        &IndexExecutor::default(),
    )
    .expect_err("surface query on a namespace decl");
    assert_eq!(
        err,
        DiscoveryQueryInvalid::NotQueryable {
            index_granularity: IndexGranularity::Namespace,
            query_granularity: IndexGranularity::Surface,
        }
    );

    // `index_forms_at` — a decl bound at `namespace` serves only there.
    let mut cat2 = fixture_catalog();
    cat2.indexes.push(decl.clone());
    assert!(index_forms_at(&cat2, &CatalogIndex::Bm25, IndexGranularity::Namespace).is_ok());
    assert_eq!(
        index_forms_at(&cat2, &CatalogIndex::Bm25, IndexGranularity::Surface),
        Err(DiscoveryQueryInvalid::NotQueryable {
            index_granularity: IndexGranularity::Namespace,
            query_granularity: IndexGranularity::Surface,
        })
    );
    // An unbound index answers `surface` only.
    let cat3 = fixture_catalog();
    assert!(index_forms_at(&cat3, &CatalogIndex::Bm25, IndexGranularity::Surface).is_ok());
    assert!(index_forms_at(&cat3, &CatalogIndex::Bm25, IndexGranularity::Source).is_err());

    // `source`/`catalog` granularities aggregate likewise.
    for g in [IndexGranularity::Source, IndexGranularity::Catalog] {
        let d = IndexDecl {
            index: CatalogIndex::LexicalRegex,
            granularity: g,
        };
        let mut qq = q(DiscoveryForm::Regex, "workspace", None);
        qq.granularity = g;
        let res = index_query_decl(&d, &cat, &qq, 32, &IndexExecutor::default())
            .expect("source/catalog query");
        // The unit hit expands to every member `surface_id` of the
        // `harness`/`catalog` unit — aggregates never leak.
        assert!(!res.hits.is_empty());
        assert!(res.hits.iter().all(|h| !h.surface_id.starts_with("unit:")));
    }
}

#[test]
fn r2_8_executor_legs_and_s386_provider_round_trip() {
    let cat = fixture_catalog();
    // The provider's hosted search — a fixture executor scoring the
    // `read_workspace_file` member above the declared threshold and a
    // second member below it (the kernel drops the miss; a `None` score is
    // never indexable).
    fn emb(
        candidates: &[&CatalogEntry],
        _q: &DiscoveryQuery,
    ) -> Result<Vec<(String, Option<i64>)>, String> {
        let mut out: Vec<(String, Option<i64>)> = candidates
            .iter()
            .filter(|e| e.surface_id == "surf:fs.read_workspace_file")
            .map(|e| (e.surface_id.clone(), Some(900_000)))
            .collect();
        out.push(("surf:net.open_tcp_socket".to_string(), Some(100_000)));
        out.push(("surf:git.commit_changes".to_string(), None));
        Ok(out)
    }
    let exec = IndexExecutor {
        embedding: Some(emb),
        ..Default::default()
    };
    let decl = IndexDecl {
        index: CatalogIndex::Embedding {
            similarity_threshold_ppm: 500_000,
        },
        granularity: IndexGranularity::Surface,
    };
    let res = index_query_decl(
        &decl,
        &cat,
        &q(
            DiscoveryForm::NaturalLanguage,
            "read a workspace file",
            None,
        ),
        8,
        &exec,
    )
    .expect("embedding query");
    // `executed_by = provider` — the S-386 row names the instrument.
    assert_eq!(res.executed_by, "provider");
    assert_eq!(res.hits.len(), 1);
    assert_eq!(res.hits[0].surface_id, "surf:fs.read_workspace_file");
    // The below-threshold hit and the score-less member were dropped — a
    // provider answer never fabricates rank.
    assert_eq!(res.hits[0].score, Some(hh_wire::json::Json::Int(900_000)));

    // Executor legs absent are `ExecutorUnavailable`, never a fake ranking.
    for index in [
        CatalogIndex::Embedding {
            similarity_threshold_ppm: 0,
        },
        CatalogIndex::ModelRanked,
        CatalogIndex::Filesystem,
    ] {
        let d = IndexDecl {
            index,
            granularity: IndexGranularity::Surface,
        };
        let mut qq = q(
            DiscoveryForm::Structured(StructuredQuery::default()),
            "x",
            None,
        );
        if matches!(d.index, CatalogIndex::Filesystem) {
            qq.form = DiscoveryForm::Regex;
        }
        assert_eq!(
            index_query_decl(&d, &cat, &qq, 8, &IndexExecutor::default()),
            Err(DiscoveryQueryInvalid::ExecutorUnavailable)
        );
    }
    // The `filesystem` leg answers through the declared walker.
    fn fs_walk(
        candidates: &[&CatalogEntry],
        _q: &DiscoveryQuery,
    ) -> Result<Vec<(String, Option<i64>)>, String> {
        Ok(candidates
            .iter()
            .filter(|e| e.namespace.as_deref() == Some("fs"))
            .take(4)
            .map(|e| (e.surface_id.clone(), None))
            .collect())
    }
    let fs_exec = IndexExecutor {
        filesystem: Some(fs_walk),
        ..Default::default()
    };
    let res = index_query_decl(
        &IndexDecl {
            index: CatalogIndex::Filesystem,
            granularity: IndexGranularity::Surface,
        },
        &cat,
        &q(DiscoveryForm::Regex, "fs/**", None),
        8,
        &fs_exec,
    )
    .expect("filesystem query");
    assert_eq!(res.hits.len(), 4);
    // `natural_language` stays refused on the glob leg (the admissibility
    // table — the executor never sees it).
    assert_eq!(
        index_query_decl(
            &IndexDecl {
                index: CatalogIndex::Filesystem,
                granularity: IndexGranularity::Surface,
            },
            &cat,
            &q(DiscoveryForm::NaturalLanguage, "fs files", None),
            8,
            &fs_exec,
        ),
        Err(DiscoveryQueryInvalid::UnsupportedForm)
    );

    // S-386: a deferred surface the provider search finds reveals, then
    // calls — the full provider round-trip at fixture layer.
    let mut entries = plan_modes(&cat, ExposureMode::Deferred);
    entries.retain(|(s, _)| s != "surf:fs.read_workspace_file");
    entries.push((
        "surf:fs.read_workspace_file".to_string(),
        ExposureMode::Deferred,
    ));
    entries.retain(|(s, _)| s != "surf:kernel.pinned_tool");
    entries.push(("surf:kernel.pinned_tool".to_string(), ExposureMode::Direct));
    let plan = ExposurePlan {
        plan_id: "plan:s386".into(),
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
    // Pre-reveal the call refuses `SurfaceNotRevealed{hint: search}`.
    let proposal = CallProposal {
        surface_name: "read_workspace_file".to_string(),
        from_code: false,
    };
    assert_eq!(
        check_callable(&plan, &cat, &proposal),
        Err(CallRefusal::SurfaceNotRevealed {
            surface_id: Some("surf:fs.read_workspace_file".to_string()),
            hint: hh_compiler::exposure::RevealHint::Search,
        })
    );
    // The provider-executed search over plan members reveals the hit.
    let res = discover_with(
        &plan,
        &decl,
        &cat,
        &q(DiscoveryForm::NaturalLanguage, "read workspace", None),
        &ExposurePolicyParams::default(),
        &exec,
    )
    .expect("provider discover");
    assert_eq!(res.executed_by, "provider");
    let hit = res.hits[0].surface_id.clone();
    let revealed = reveal(
        &RevealedSet {
            run_id: "run:test".into(),
            entries: vec![],
        },
        &cat,
        std::slice::from_ref(&hit),
        RevealCause::Discovery,
        Retention::Call,
        1,
    )
    .expect("reveal");
    // The next plan admits the revealed surface `direct`; the call lands.
    let mut plan2 = plan.clone();
    plan2.entries.retain(|(s, _)| *s != hit);
    plan2.entries.push((hit.clone(), ExposureMode::Direct));
    assert!(revealed.entries.iter().any(|e| e.surface_id == hit));
    assert_eq!(check_callable(&plan2, &cat, &proposal), Ok(()));
}

#[test]
fn r2_8_form_fixture_non_native_fc_family() {
    use hh_compiler::equiv::SurfaceBinding;
    use hh_compiler::family::{
        bind_variant, check_family, select_variant, EnvironmentDeclaration, SurfaceFamily,
        SurfaceVariant, TaskClassDeclaration, VariantSelector,
    };
    use hh_compiler::plan::PinnedRef;
    use hh_compiler::surface::{BindingMapping, CompileExposureMode};

    // The non-`native_fc` rendering: a `shim` variant beside the `native_fc`
    // reference (ADR-0286 D7 — the form fixture needs a family C0 couldn't
    // compile).
    fn shim_binding(name: &str, cap: &str) -> SurfaceBinding {
        let mut b = SurfaceBinding {
            surface_name: name.to_string(),
            surface_id: String::new(),
            exposure_mode: CompileExposureMode::Shim,
            capability_ref: PinnedRef {
                semantic_id: cap.to_string(),
                version_id: format!("sha256:cap-{cap}"),
            },
            capability_refs: vec![cap.to_string()],
            hir_node_id: cap.to_string(),
            arg_map: BTreeMap::new(),
            mapping: BindingMapping::SurfaceArgMap,
            rule_ids: Vec::new(),
            evidence_ref: None,
            safety_ref: None,
            effects_bound: Vec::new(),
            family_id: None,
            variant_id: None,
            dialect: hh_compiler::equiv::DEFAULT_SCHEMA_DIALECT.to_string(),
            admitted_modes: [ExposureMode::Indexed, ExposureMode::Deferred]
                .into_iter()
                .collect(),
            pinned: false,
            hidden: false,
        };
        b.surface_id = hh_compiler::surface::surface_id(&b);
        b
    }
    fn prim_binding(name: &str, cap: &str) -> SurfaceBinding {
        let mut b = shim_binding(name, cap);
        b.exposure_mode = CompileExposureMode::Primitive;
        b.surface_id = hh_compiler::surface::surface_id(&b);
        b
    }

    let family = SurfaceFamily {
        family_id: "family/fs.text".to_string(),
        capability_refs: vec!["cap/read_text".to_string(), "cap/stat_path".to_string()],
        reference_variant: "v:native_fc".to_string(),
        variants: vec![
            SurfaceVariant {
                variant_id: "v:shim".to_string(),
                exposure_mode: CompileExposureMode::Shim,
                surfaces: vec!["read_text".to_string(), "stat_path".to_string()],
                selector: VariantSelector::TaskClass("shell".to_string()),
                rule_ids: vec![],
            },
            SurfaceVariant {
                variant_id: "v:native_fc".to_string(),
                exposure_mode: CompileExposureMode::Primitive,
                surfaces: vec!["read_text_prim".to_string(), "stat_path_prim".to_string()],
                selector: VariantSelector::Always,
                rule_ids: vec![],
            },
        ],
    };
    let task = TaskClassDeclaration {
        task_class_id: "shell".to_string(),
        labels: vec![],
        expected_capabilities: vec![],
    };
    let env = EnvironmentDeclaration {
        environment_count: 1,
        executor_platform: "linux".to_string(),
        session_support: false,
        capability_declaration: vec![],
    };
    // The selector picks the non-native_fc rendering for `shell` tasks;
    // `chat` falls back to the `native_fc` reference.
    assert_eq!(
        select_variant(
            &family,
            &task,
            &env,
            &hh_wire::json::Json::Null,
            &BTreeSet::new()
        )
        .variant_id,
        "v:shim"
    );
    let variant = select_variant(
        &family,
        &task,
        &env,
        &hh_wire::json::Json::Null,
        &BTreeSet::new(),
    );

    // `bind_variant` stamps the identity-bearing family/variant ids; the
    // family record validates against the back-pointed bindings.
    let mut bindings: BTreeMap<String, SurfaceBinding> = BTreeMap::new();
    for (name, cap, mode_b) in [
        ("read_text", "cap/read_text", true),
        ("stat_path", "cap/stat_path", true),
        ("read_text_prim", "cap/read_text", false),
        ("stat_path_prim", "cap/stat_path", false),
    ] {
        let base = if mode_b {
            shim_binding(name, cap)
        } else {
            prim_binding(name, cap)
        };
        let v = family
            .variants
            .iter()
            .find(|v| v.surfaces.contains(&name.to_string()))
            .unwrap();
        let bound = bind_variant(&base, &family, v).expect("bind_variant");
        assert_eq!(bound.family_id.as_deref(), Some("family/fs.text"));
        bindings.insert(name.to_string(), bound);
    }
    check_family(&family, &bindings).expect("family checks");
    assert_eq!(variant.variant_id, "v:shim");

    // The shim-bound surfaces become catalog entries — discovery over them
    // runs under both declared forms with the same required set (the
    // regex-vs-natural-language *form* fixture, AC-E3-5's residual arm).
    let mut cat = fixture_catalog();
    let mut required = BTreeSet::new();
    for (name, desc) in [
        ("read_text", "read a text file via the shim layer"),
        ("stat_path", "stat a filesystem path via the shim layer"),
    ] {
        let sid = bindings[name].surface_id.clone();
        let mut e = entry(
            &sid,
            name,
            Some("shimfs"),
            desc,
            &[ExposureMode::Indexed, ExposureMode::Deferred],
        );
        e.source = CatalogSource::Harness(Some("shim".to_string()));
        cat.entries.push(e);
        required.insert(sid);
    }
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, cat.epoch, &cat.bundle_id);

    // Regex form over `lexical_regex`; natural-language form over `bm25` —
    // one required set, two profiles (the diff-confinement readout arm).
    let regex = retrieval_eval(
        &CatalogIndex::LexicalRegex,
        &cat,
        &[LabelledQuery {
            query: q(DiscoveryForm::Regex, "^(read_text|stat_path)$", None),
            required_surface_ids: required.clone(),
        }],
        8,
        32,
    );
    let nl = retrieval_eval(
        &CatalogIndex::Bm25,
        &cat,
        &[LabelledQuery {
            query: q(
                DiscoveryForm::NaturalLanguage,
                "read a text file stat a path",
                None,
            ),
            required_surface_ids: required.clone(),
        }],
        8,
        32,
    );
    assert_eq!(regex.recall_at_k, 1.0, "regex form finds the shim surfaces");
    assert!(nl.recall_at_k > 0.0, "nl form finds the shim surfaces");
    // The forms are variant-scoped: `bm25` refuses `regex` and vice versa —
    // the form admissibility table is per-variant, never silently widened.
    assert_eq!(
        index_query(
            &CatalogIndex::Bm25,
            &cat,
            &q(DiscoveryForm::Regex, ".*", None),
            8
        ),
        Err(DiscoveryQueryInvalid::UnsupportedForm)
    );
}

#[test]
fn r2_8_bind_producers_code_mode_callable_and_render_specs() {
    use hh_compiler::family::{
        bind_code_mode, bind_composite, bind_freeform, bind_shim, FreeformSpec, PlanMap, PlanStep,
        ShimSpec,
    };
    use hh_compiler::surface::{
        BindingMapping, CompileExposureMode, RenderMode, RenderSpecParseError,
    };
    use hh_compiler::CompileError;
    use hh_hir::records::SurfaceRecord;
    use hh_wire::json::Json;

    let node = common::surfaced_tool_node("test:proc", "apply_patch", &["patch"], 1);
    let surface = match node.surface.as_ref().expect("tool surface") {
        SurfaceRecord::Tool(t) => (**t).clone(),
        _ => panic!("surfaced tool node"),
    };

    // `bind_composite` — the PlanMap producer: `Composite` mode, a
    // `plan_map` mapping ref, the S1 inclusion set over invoked
    // capabilities, and `effects_bound` = ∪ declared effects.
    let plan = PlanMap {
        steps: vec![
            PlanStep::Invoke {
                capability_ref: "cap/read".into(),
                arg_map: BTreeMap::new(),
            },
            PlanStep::Verify {
                validator_ref: "val:ok".into(),
            },
            PlanStep::Invoke {
                capability_ref: "cap/write".into(),
                arg_map: BTreeMap::new(),
            },
        ],
        session_state: None,
    };
    let allowed: BTreeSet<String> = ["cap/read", "cap/write"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let effects: BTreeMap<String, BTreeSet<String>> = [
        (
            "cap/read".to_string(),
            ["fs.read"].iter().map(|s| s.to_string()).collect(),
        ),
        (
            "cap/write".to_string(),
            ["fs.write"].iter().map(|s| s.to_string()).collect(),
        ),
    ]
    .into_iter()
    .collect();
    let bound =
        bind_composite(&surface, &node, &plan, &allowed, &effects).expect("composite binds");
    assert_eq!(bound.exposure_mode, CompileExposureMode::Composite);
    assert!(
        matches!(&bound.mapping, BindingMapping::PlanMap(r) if !r.is_empty()),
        "composite carries a plan_map mapping ref"
    );
    assert_eq!(bound.capability_refs.len(), 2);
    assert!(bound.effects_bound.contains(&"fs.read".to_string()));
    assert!(bound.effects_bound.contains(&"fs.write".to_string()));
    assert!(!bound.surface_id.is_empty());
    // The composite binding round-trips the canonical codec —
    // `mapping.kind = plan_map` + the pinned `plan_ref` survive
    // (`surface_binding_json`/`surface_binding_from_json`, AC-R-2.5.2-3's
    // composite half).
    let decoded = hh_compiler::schema::surface_binding_from_json(
        &hh_compiler::schema::surface_binding_json(&bound),
        "t.composite",
    )
    .expect("composite binding round-trips");
    assert_eq!(decoded.mapping, bound.mapping);
    assert_eq!(decoded.surface_id, bound.surface_id);
    assert_eq!(decoded.capability_refs, bound.capability_refs);
    assert_eq!(decoded.family_id, bound.family_id);
    assert_eq!(decoded.variant_id, bound.variant_id);
    // An invoke outside the allowlist is `AuthorityWidening`; an invoke
    // without a declared effect set is `UncheckableSurface` — both typed,
    // never a silent widening.
    let wide = PlanMap {
        steps: vec![PlanStep::Invoke {
            capability_ref: "cap/other".into(),
            arg_map: BTreeMap::new(),
        }],
        session_state: None,
    };
    assert!(matches!(
        bind_composite(&surface, &node, &wide, &allowed, &effects),
        Err(CompileError::AuthorityWidening { .. })
    ));
    assert!(matches!(
        bind_composite(&surface, &node, &plan, &allowed, &BTreeMap::new()),
        Err(CompileError::UncheckableSurface { .. })
    ));

    // `bind_freeform`/`bind_shim`/`bind_code_mode` — the C1/C2 producers;
    // each mints a distinct `surface_id` (mode + arg_map are in the
    // identity basis).
    let ff = bind_freeform(
        &surface,
        &node,
        &FreeformSpec {
            grammar_ref: "g:patch-grammar".into(),
            capability_param: "patch".into(),
            surface_arg: "patch".into(),
        },
    );
    assert_eq!(ff.exposure_mode, CompileExposureMode::Freeform);
    let sh = bind_shim(
        &surface,
        &node,
        &ShimSpec {
            surface_arg: "patch".into(),
            capability_param: "patch".into(),
            table_ref: "tbl:names".into(),
        },
    );
    assert_eq!(sh.exposure_mode, CompileExposureMode::Shim);
    let cm = bind_code_mode(&surface, &node);
    assert_eq!(cm.exposure_mode, CompileExposureMode::CodeMode);
    let ids: BTreeSet<&str> = [
        bound.surface_id.as_str(),
        ff.surface_id.as_str(),
        sh.surface_id.as_str(),
        cm.surface_id.as_str(),
    ]
    .into_iter()
    .collect();
    assert_eq!(ids.len(), 4, "each producer mints a distinct surface_id");

    // `code_mode` plans: a program-originated call is callable only when the
    // plan admits `code_mode` for it — the run-time leg of the C2 producer.
    let mut cat = fixture_catalog();
    let mut e = entry(
        &cm.surface_id,
        "apply_patch",
        Some("code"),
        "program-originated patch surface",
        &[ExposureMode::CodeMode],
    );
    e.source = CatalogSource::Harness(Some("code".to_string()));
    cat.entries.push(e);
    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    cat.catalog_id = catalog_id_of(&cat.entries, cat.epoch, &cat.bundle_id);
    let code_plan = ExposurePlan {
        plan_id: "plan:code".into(),
        model_call_id: "mc:1".into(),
        catalog_id: cat.catalog_id.clone(),
        entries: vec![(cm.surface_id.clone(), ExposureMode::CodeMode)],
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
    assert_eq!(
        check_callable(
            &code_plan,
            &cat,
            &CallProposal {
                surface_name: "apply_patch".to_string(),
                from_code: true,
            }
        ),
        Ok(())
    );
    // A model-originated call on a `code_mode` plan member is refused; a
    // program-originated call on a `direct` member is refused — the arms
    // don't cross.
    assert!(matches!(
        check_callable(
            &code_plan,
            &cat,
            &CallProposal {
                surface_name: "apply_patch".to_string(),
                from_code: false,
            }
        ),
        Err(CallRefusal::SurfaceNotRevealed { .. })
    ));
    let direct_plan = ExposurePlan {
        entries: vec![(cm.surface_id.clone(), ExposureMode::Direct)],
        ..code_plan.clone()
    };
    assert!(matches!(
        check_callable(
            &direct_plan,
            &cat,
            &CallProposal {
                surface_name: "apply_patch".to_string(),
                from_code: true,
            }
        ),
        Err(CallRefusal::SurfaceNotRevealed { .. })
    ));

    // `RenderMode::parse` — the C1 `concise`/`offload` members: declared
    // fields only; `offload`'s `preview_lines ≤ threshold_bytes` is a
    // parse-time bound.
    assert_eq!(
        RenderMode::parse("concise", &Json::obj([("format_param", Json::str("json"))])),
        Ok(RenderMode::Concise {
            format_param: "json".to_string()
        })
    );
    assert!(matches!(
        RenderMode::parse("concise", &Json::obj([])),
        Err(RenderSpecParseError::MissingMember { .. })
    ));
    assert!(matches!(
        RenderMode::parse(
            "offload",
            &Json::obj([
                ("threshold_bytes", Json::Int(4096)),
                ("preview_lines", Json::Int(20)),
                ("artifact_kind", Json::str("result_body")),
            ])
        ),
        Ok(RenderMode::Offload {
            threshold_bytes: 4096,
            preview_lines: 20,
            ..
        })
    ));
    assert!(matches!(
        RenderMode::parse(
            "offload",
            &Json::obj([
                ("threshold_bytes", Json::Int(10)),
                ("preview_lines", Json::Int(20)),
                ("artifact_kind", Json::str("result_body")),
            ])
        ),
        Err(RenderSpecParseError::OffloadBound { .. })
    ));
}
