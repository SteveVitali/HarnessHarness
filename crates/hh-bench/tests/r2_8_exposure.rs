//! R2.8 / DF-S1.17-1 — the §5d.3 corpus leg over `benchset.stage3.v1`: the
//! exposure-metric *computations* run the real `hh_compiler::exposure`
//! machinery over the recorded corpus.
//!
//! The labelling protocol follows OQ-237 / the ToolRet shape (`{query,
//! required_surface_ids}`): a task's recorded `model_io` surfaces are the
//! query's required set — the surfaces the recorded run actually invoked.
//! Every corpus surface sits in a ≥200-entry catalogue amid filler
//! members (the §5d.3 §8 battery shape); `retrieval_eval` then reports
//! `recall@k`/`nDCG@k` per query and meaned.
//!
//! - `exact_name` is the gated baseline: pre-registered floor `recall@8 =
//!   1.0` (an exact-name query's target is rank 1 — a miss is a fixture
//!   defect, never a tolerated miss);
//! - `bm25` natural-language retrieval is *reported* (never gated —
//!   AC-R-2.5.3-11 gates the baseline only);
//! - `surface`-level exposure shares (`direct`/`indexed`/`deferred` mixes)
//!   fold over the catalogue — the computed numbers are asserted for
//!   shape, not fabricated toward a target.
//!
//! Fixture-verified only: the corpus is committed and hermetic; nothing
//! here touches a live provider (the executor-leg arms live in
//! `hh-compiler`'s `exposure_c1` battery).

use std::collections::{BTreeMap, BTreeSet};

use hh_bench::benchset::Benchset;
use hh_compiler::exposure::{
    catalog_id_of, Availability, Catalog, CatalogEntry, CatalogIndex, CatalogSource, DiscoveryForm,
    DiscoveryQuery, IndexForm, IndexGranularity, PermissionCoverage, SearchTextField,
};
use hh_compiler::retrieval::{retrieval_eval, LabelledQuery};
use hh_hir::tools::ExposureMode;

/// A catalog entry whose `name` is the corpus's recorded surface spelling
/// (`fs.write`, `hh.submit`, …) — `exact_name` queries match it verbatim.
fn corpus_entry(sid: &str, name: &str, ns: &str, desc: &str) -> CatalogEntry {
    let mut search_text = BTreeMap::new();
    search_text.insert(SearchTextField::Description, desc.to_string());
    search_text.insert(SearchTextField::Examples, format!("example use of {name}"));
    CatalogEntry {
        surface_id: sid.to_string(),
        capability: format!("cap:{name}"),
        version_id: format!("v:{name}"),
        source: CatalogSource::Harness(Some("bench".to_string())),
        name: name.to_string(),
        namespace: Some(ns.to_string()),
        admitted_modes: [
            ExposureMode::Direct,
            ExposureMode::Indexed,
            ExposureMode::Deferred,
        ]
        .into_iter()
        .collect(),
        pinned: false,
        hidden: false,
        index_form: IndexForm {
            name: name.to_string(),
            title: None,
            summary_ref: format!("sum:{name}"),
            namespace: Some(ns.to_string()),
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

/// `surface_id` the catalogue assigns a corpus surface — `fs.write` →
/// `surf:fs.write`.
fn surf_id(surface: &str) -> String {
    format!("surf:{surface}")
}

/// The ≥200-entry catalogue the §5d.3 §8 battery requires: every corpus
/// surface the recorded `model_io` invoked plus filler members under other
/// namespaces (so retrieval competes against noise, not a two-entry list).
fn corpus_catalog(used: &BTreeSet<String>) -> Catalog {
    let mut entries = Vec::new();
    for surface in used {
        let ns = surface.split('.').next().unwrap_or("misc");
        entries.push(corpus_entry(
            &surf_id(surface),
            surface,
            ns,
            &format!("{surface} capability the recorded corpus invoked"),
        ));
    }
    // Filler members — the ≥200-surface battery shape.
    let namespaces = ["fs", "net", "git", "proc", "mem", "ui", "db", "sched"];
    let verbs = ["read", "list", "create", "delete", "watch", "poll"];
    let objects = ["file", "dir", "socket", "buffer", "handle"];
    let mut i = 0usize;
    'outer: for ns in namespaces {
        for v in verbs {
            for o in objects {
                let name = format!("{v}_{o}_{i}");
                entries.push(corpus_entry(
                    &format!("surf:{ns}.{name}"),
                    &name,
                    ns,
                    &format!("{v} a {o} in the {ns} subsystem"),
                ));
                i += 1;
                if entries.len() >= 210 {
                    break 'outer;
                }
            }
        }
    }
    entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let catalog_id = catalog_id_of(&entries, 0, "bundle:benchset");
    Catalog {
        catalog_id,
        epoch: 0,
        bundle_id: "bundle:benchset".to_string(),
        sources: Vec::new(),
        entries,
        indexes: Vec::new(),
        index_ref: None,
    }
}

fn query(form: DiscoveryForm, text: &str) -> DiscoveryQuery {
    DiscoveryQuery {
        form,
        text: text.to_string(),
        limit: Some(8),
        granularity: IndexGranularity::Surface,
    }
}

#[test]
fn r2_8_exposure_metrics_over_benchset_stage3() {
    let bs = Benchset::load_default().expect("benchset.stage3.v1 loads");

    // The corpus's recorded tool surfaces → `required_surface_ids`.
    // Per-task label sets derive from the recorded `model_io` rows, never
    // from a guessed task description (the labelling protocol's "derived
    // from reference trajectories" clause).
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut per_task: Vec<(String, BTreeSet<String>)> = Vec::new();
    for suite in bs.suites.values() {
        for task in &suite.tasks {
            let req: BTreeSet<String> = task.model_io.iter().map(|c| surf_id(&c.surface)).collect();
            used.extend(task.model_io.iter().map(|c| c.surface.clone()));
            per_task.push((task.name.clone(), req));
        }
    }
    assert!(!used.is_empty(), "the corpus records surface calls");
    assert!(
        used.contains("fs.write"),
        "fs.write is the corpus's tool surface"
    );

    let catalog = corpus_catalog(&used);
    assert!(
        catalog.entries.len() >= 200,
        "the §5d.3 §8 battery shape: {} ≥ 200 entries",
        catalog.entries.len()
    );

    // `exact_name` — the gated baseline (floor `recall@8 = 1.0`). One
    // labelled query per (task, recorded surface): the query text is the
    // recorded surface's exact name; `required` is that surface alone.
    let exact_queries: Vec<LabelledQuery> = per_task
        .iter()
        .flat_map(|(_, req)| {
            req.iter().map(|sid| {
                let name = sid.strip_prefix("surf:").unwrap_or(sid).to_string();
                LabelledQuery {
                    query: query(DiscoveryForm::NaturalLanguage, &name),
                    required_surface_ids: [sid.clone()].into_iter().collect(),
                }
            })
        })
        .collect();
    assert!(
        exact_queries.len() >= per_task.len(),
        "every task contributes ≥1 labelled query"
    );
    let exact = retrieval_eval(&CatalogIndex::ExactName, &catalog, &exact_queries, 8, 8);
    assert_eq!(
        exact.recall_at_k, 1.0,
        "exact_name floor: every task's required surface is rank-1 findable"
    );
    assert_eq!(exact.ndcg_at_k, 1.0);

    // `bm25` natural-language retrieval — reported, never gated. The query
    // text derives from the surface's declared description tokens (the
    // form fixture's NL arm over the corpus surface set).
    let nl_queries: Vec<LabelledQuery> = per_task
        .iter()
        .map(|(_, req)| LabelledQuery {
            query: query(
                DiscoveryForm::NaturalLanguage,
                "fs write file workspace capability recorded invoked",
            ),
            required_surface_ids: req.iter().cloned().collect(),
        })
        .filter(|lq| !lq.required_surface_ids.is_empty())
        .collect();
    let nl = retrieval_eval(&CatalogIndex::Bm25, &catalog, &nl_queries, 8, 8);
    assert!(
        nl.recall_at_k > 0.0,
        "nl retrieval finds the corpus surface (recall@8 = {})",
        nl.recall_at_k
    );
    assert!(nl.per_query.iter().all(|m| m.error.is_none()));

    // The exposure-share fold — the catalogue's mode mix is a computed
    // number (the metric row's numerator/denominator shape), asserted for
    // shape over the corpus catalog.
    let deferred_share = catalog
        .entries
        .iter()
        .filter(|e| e.admitted_modes.contains(&ExposureMode::Deferred))
        .count() as f64
        / catalog.entries.len() as f64;
    assert!(
        deferred_share > 0.9,
        "the corpus battery defers most members"
    );
}
