//! `retrieval_eval` — the offline retrieval suite for `catalog_index`
//! variants (§5d.3 §2; ADR-0094 D5; the S2.10 C1 slice).
//!
//! `retrieval_eval(index, catalog, labelled_queries, k)` runs each
//! `DiscoveryQuery` through the kernel executor and reports `recall@k` and
//! `nDCG@k` per query and averaged. Binary relevance: a hit is relevant iff
//! its `surface_id ∈ required_surface_ids`. `recall@k = |hits@k ∩ required|
//! / |required|`; `nDCG@k = DCG@k / IDCG@k` with `DCG = Σ rel_i / log2(i+1)`
//! over the ranked hits.
//!
//! Determinism: the metric is a pure fold over `index_query` results —
//! scores and ranks come from the index; ties are the index's own ordering
//! (the C1 variants order by `surface_id` under equal scores). A query that
//! fails `DiscoveryQueryInvalid` scores `0.0`/`0.0` and records the refusal
//! in `per_query[].error` — an invalid query is a measured failure, never
//! silently dropped (T-LCD-15).
//!
//! AC-R-2.5.3-11 gates only the `exact_name` baseline floor; the C1
//! variants' `recall`/`nDCG` are *reported*, not gated.

use std::collections::BTreeSet;

use crate::exposure::{
    index_query, Catalog, CatalogIndex, DiscoveryQuery, DiscoveryQueryInvalid,
};

/// One labelled query — the ToolRet shape (`{query, required_surface_ids}`).
#[derive(Debug, Clone, PartialEq)]
pub struct LabelledQuery {
    /// The discovery query.
    pub query: DiscoveryQuery,
    /// The surfaces the labelling requires the index to return.
    pub required_surface_ids: BTreeSet<String>,
}

/// Per-query metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryMetric {
    /// `recall@k` for this query (`0.0` when `required` is empty or the
    /// query was invalid).
    pub recall: f64,
    /// `nDCG@k` for this query.
    pub ndcg: f64,
    /// The hit count the index returned (post-bound).
    pub hits: usize,
    /// The `DiscoveryQueryInvalid` the query failed with, if any.
    pub error: Option<DiscoveryQueryInvalid>,
}

/// `{recall@k, nDCG@k}` — the eval report (component-level, native only,
/// `charged_to = instrument`).
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalReport {
    /// The cut-off.
    pub k: u64,
    /// Mean `recall@k` over the labelled queries.
    pub recall_at_k: f64,
    /// Mean `nDCG@k` over the labelled queries.
    pub ndcg_at_k: f64,
    /// The per-query rows (same order as the input).
    pub per_query: Vec<QueryMetric>,
}

/// `log2(x)` — canonical-json-compatible scoring aid (the metric is not
/// ledger data; `f64::log2` is deterministic for the report).
fn log2(x: f64) -> f64 {
    x.log2()
}

/// `retrieval_eval(index, catalog, queries, k, max_reveal_per_search)` —
/// runs every labelled query and folds the metrics. `k` caps the ranked
/// list measured (`rank ≤ k`); the query's own bound still applies.
pub fn retrieval_eval(
    index: &CatalogIndex,
    catalog: &Catalog,
    queries: &[LabelledQuery],
    k: u64,
    max_reveal_per_search: u64,
) -> RetrievalReport {
    let mut per_query = Vec::with_capacity(queries.len());
    for lq in queries {
        match index_query(index, catalog, &lq.query, max_reveal_per_search) {
            Err(e) => per_query.push(QueryMetric {
                recall: 0.0,
                ndcg: 0.0,
                hits: 0,
                error: Some(e),
            }),
            Ok(res) => {
                let at_k: Vec<&crate::exposure::DiscoveryHit> =
                    res.hits.iter().filter(|h| h.rank <= k).collect();
                let relevant = at_k
                    .iter()
                    .filter(|h| lq.required_surface_ids.contains(&h.surface_id))
                    .count();
                let recall = if lq.required_surface_ids.is_empty() {
                    0.0
                } else {
                    relevant as f64 / lq.required_surface_ids.len() as f64
                };
                let dcg: f64 = at_k
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        if lq.required_surface_ids.contains(&h.surface_id) {
                            1.0 / log2(i as f64 + 2.0)
                        } else {
                            0.0
                        }
                    })
                    .sum();
                let ideal = lq.required_surface_ids.len().min(k as usize);
                let idcg: f64 = (0..ideal).map(|i| 1.0 / log2(i as f64 + 2.0)).sum();
                let ndcg = if idcg > 0.0 { dcg / idcg } else { 0.0 };
                per_query.push(QueryMetric {
                    recall,
                    ndcg,
                    hits: res.hits.len(),
                    error: None,
                });
            }
        }
    }
    let n = per_query.len() as f64;
    let recall_at_k = if n > 0.0 {
        per_query.iter().map(|m| m.recall).sum::<f64>() / n
    } else {
        0.0
    };
    let ndcg_at_k = if n > 0.0 {
        per_query.iter().map(|m| m.ndcg).sum::<f64>() / n
    } else {
        0.0
    };
    RetrievalReport {
        k,
        recall_at_k,
        ndcg_at_k,
        per_query,
    }
}
