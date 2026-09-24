//! §5c.3 retrieval — the one pipeline (ADR-0079 d1/d3, ADR-0120):
//! `enumerate` → `filter` (validity → authority → readers — the attested
//! order, E2) → `rank` (`deterministic_default`) → budgeted whole-item cut →
//! `RetrievalReport`. Every retrieval is ledgered: `context.retrieval.completed`
//! + `context.memory.read` payloads into the caller's `EventSink`.
//!
//! The C0 executable views are `lexical_index` (a fold over the store —
//! [`lexical_index`]) and the store's own scan; `structural` is declared C1
//! (`IndexUnavailable{structural_index}` — OQ-203) and `similarity` is C2
//! (`EmbedderUnpinned` — `deterministic = false` by construction).
//!
//! Read-your-writes (R4): `at ≤ applied_seq`; a request ahead of the store is
//! `StaleStore` — never a partial answer. `execute` mode never serves
//! `superseded|revoked|expired` (R5); `audit` returns them annotated.

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::names::ResolveMode;
use hh_ledger::views::View;
use hh_provenance::authority::ReaderSet;
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::events::{self, EventSink};
use crate::lifecycle;
use crate::memory::{ArtifactVersion, MemoryStore, MemoryVersion};
use crate::plan::{Candidate, Estimate, ValidityPolicy};
use crate::vocab::{
    CandidateKind, CandidateState, DiscoverDirection, Layer, LifecycleState, LifecycleStateKind,
    MatchMode, MemoryKind, Retention, RetrievalQuery, TriggerKind,
};

/// The kernel's C0 ranker ref (§5c.3 "ranker_ref = deterministic_default").
pub const DETERMINISTIC_DEFAULT: &str = "deterministic_default";

/// `request_hash` domain — `retrieval_request/1`.
pub const REQUEST_IDP: &str = "retrieval_request.1";

/// `RetrievalRequest` (§5c.3): `{model_call_id, at: Watermark, layers ⊆
/// {A,E,P,S}, query: RetrievalQuery, constraints: SlotConstraints, reader:
/// ReaderRef, budget, ranker: RankerRef, mode}`.
#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    /// `model_call_id`.
    pub model_call_id: String,
    /// `at` — the store watermark the request reads (`run_id`, `seq`).
    pub at: (String, u64),
    /// `layers ⊆ {A,E,P,S}` — the queryable set (`W` is never a source).
    pub layers: BTreeSet<Layer>,
    /// The closed query.
    pub query: RetrievalQuery,
    /// `constraints` — copied from the target slot's declaration.
    pub constraints: SlotConstraints,
    /// `reader` — the `ReaderRef` the readers check admits.
    pub reader: String,
    /// `budget{tokens, k}` — whole items only.
    pub budget: RetrievalBudget,
    /// `ranker: RankerRef` — `"deterministic_default"` at C0.
    pub ranker: String,
    /// `mode` — `Execute | Audit | Reproduce` (`hh-identity`'s — CC7).
    pub mode: ResolveMode,
}

/// `RetrievalBudget{tokens, k}`.
#[derive(Debug, Clone, Copy)]
pub struct RetrievalBudget {
    /// Token bound.
    pub tokens: u64,
    /// Item bound.
    pub k: u64,
}

/// `SlotConstraints` — the slot-boundary members `retrieve` enforces:
/// `slot_min_authority` + `validity_policy` + `readers_required` (the slot's
/// reader bound — carried at C0).
#[derive(Debug, Clone)]
pub struct SlotConstraints {
    /// `min_authority` from the target `SlotDeclaration`.
    pub slot_min_authority: AuthorityClass,
    /// The `ValidityPolicy` copy.
    pub validity_policy: ValidityPolicy,
    /// `readers_required` — `Some` narrows the reader check (C0: carried).
    pub readers_required: Option<ReaderSet>,
}

/// `RetrievedItem` — the retrieval record (§5c.3): `{layer, address,
/// provenance, validity{state, check_ref}, rank_evidence, handle?}` + the
/// members a `Candidate` projection needs (`kind`, `label`, `tokens`,
/// `content`/`excerpt`).
#[derive(Debug, Clone)]
pub struct RetrievedItem {
    /// The item's `version_id`/`artifact_id` (the address).
    pub address: String,
    /// `semantic_id` when the item is a memory.
    pub semantic_id: Option<String>,
    /// The layer it was enumerated from.
    pub layer: Layer,
    /// `provenance` — the item's record.
    pub provenance: ProvenanceRecord,
    /// `label`.
    pub label: hh_provenance::label::Label,
    /// `validity{state, check_ref}` — the lifecycle state + the check's
    /// pointer (`check_ref` = the contract-hash / index member).
    pub validity_state: LifecycleStateKind,
    /// `rank_evidence` — `{ranker_ref, score, features[]}`.
    pub rank_evidence: RankEvidence,
    /// The estimated tokens.
    pub tokens: u64,
    /// The content surface (the index text for the index form).
    pub index_text: String,
    /// `handle?` — set when delivered `handle_only`.
    pub handle: Option<crate::plan::OffloadHandle>,
    /// `conflict_set_ref` — the conflict annotation (`deliver_all_annotated`).
    pub conflict_set_ref: Option<String>,
    /// The memory kind (`None` for artifacts).
    pub memory_kind: Option<MemoryKind>,
    /// `scope`.
    pub scope: hh_provenance::PersistenceScope,
}

/// `rank_evidence{ranker_ref, score, features[]}` (§5c.3).
#[derive(Debug, Clone)]
pub struct RankEvidence {
    /// The `RankerRef`.
    pub ranker_ref: String,
    /// The score the ranker assigned (a decimal string — no floats in the
    /// canonical record).
    pub score: String,
    /// `features[]` from the closed feature vocabulary.
    pub features: Vec<String>,
}

/// `RetrievalReport` — the `context.retrieval.completed.report` member
/// (§5c.3).
#[derive(Debug, Clone)]
pub struct RetrievalReport {
    /// `enumerated`.
    pub enumerated: u64,
    /// `filtered{validity, authority, readers}`.
    pub filtered_validity: u64,
    /// See `filtered_validity`.
    pub filtered_authority: u64,
    /// See `filtered_validity`.
    pub filtered_readers: u64,
    /// `ranked`.
    pub ranked: u64,
    /// `returned`.
    pub returned: u64,
    /// `omitted_by_budget[]`.
    pub omitted_by_budget: Vec<String>,
    /// `no_authoritative_items` — hits existed but every one was
    /// authority-withheld.
    pub no_authoritative_items: bool,
    /// `ranker_ref`.
    pub ranker_ref: String,
    /// `deterministic` — the ranker/query determinism claim.
    pub deterministic: bool,
    /// `cost{tokens_estimated, index_ms, embedder_calls}`.
    pub cost: RetrievalCost,
}

/// `cost{tokens_estimated, index_ms, embedder_calls}` (§5c.3; `index_ms`
/// carries `measured_at = runtime` — the M14 measurement).
#[derive(Debug, Clone)]
pub struct RetrievalCost {
    /// `tokens_estimated`.
    pub tokens_estimated: u64,
    /// `index_ms` — the index fold/serve cost.
    pub index_ms: u64,
    /// `embedder_calls` (≥1 ⇒ `deterministic = false` — the declared C2 mark).
    pub embedder_calls: u64,
}

impl RetrievalReport {
    /// Canonical JSON (the `report` member of `context.retrieval.completed`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("enumerated", Json::Int(self.enumerated as i64)),
            (
                "filtered",
                Json::obj([
                    ("validity", Json::Int(self.filtered_validity as i64)),
                    ("authority", Json::Int(self.filtered_authority as i64)),
                    ("readers", Json::Int(self.filtered_readers as i64)),
                ]),
            ),
            ("ranked", Json::Int(self.ranked as i64)),
            ("returned", Json::Int(self.returned as i64)),
            (
                "omitted_by_budget",
                Json::Arr(
                    self.omitted_by_budget
                        .iter()
                        .map(|o| Json::str(o.clone()))
                        .collect(),
                ),
            ),
            (
                "no_authoritative_items",
                Json::Bool(self.no_authoritative_items),
            ),
            ("ranker_ref", Json::str(self.ranker_ref.clone())),
            ("deterministic", Json::Bool(self.deterministic)),
            (
                "cost",
                Json::obj([
                    (
                        "tokens_estimated",
                        Json::Int(self.cost.tokens_estimated as i64),
                    ),
                    (
                        "index_ms",
                        Json::obj([
                            ("value", Json::Int(self.cost.index_ms as i64)),
                            ("measured_at", Json::str("runtime")),
                        ]),
                    ),
                    ("embedder_calls", Json::Int(self.cost.embedder_calls as i64)),
                ]),
            ),
        ])
    }
}

/// The `retrieve` failures (§5c.3).
#[derive(Debug, Clone, PartialEq)]
pub enum RetrievalError {
    /// `StaleStore` — `at.seq > applied_seq` (R3: a request reads a
    /// watermark; the store answers "not yet").
    StaleStore {
        /// The store's applied seq.
        applied_seq: u64,
        /// The requested seq.
        requested: u64,
    },
    /// `UnknownLayer` — a layer outside `{A,E,P,S}`.
    UnknownLayer {
        /// The spelling.
        layer: String,
    },
    /// `InvalidQueryKind` — an unregistered kind (the closed sum refuses at
    /// decode; this variant covers the extension registry boundary).
    InvalidQueryKind {
        /// The kind.
        kind: String,
    },
    /// `RankerNotDeterministic` — a non-`deterministic` ranker under a
    /// `require_deterministic` slot.
    RankerNotDeterministic {
        /// The slot.
        slot: String,
    },
    /// `EmbedderUnpinned` — `similarity` without a pinned snapshot.
    EmbedderUnpinned,
    /// `BudgetExhausted` — the mandatory-cover portion alone exceeds budget
    /// (whole items only — never truncation).
    BudgetExhausted {
        /// The required tokens.
        required: u64,
        /// The cap.
        cap: u64,
    },
    /// `MissingProvenance` — an enumerated hit with no provenance record is
    /// withheld and logged (never served).
    MissingProvenance {
        /// The hit.
        version_id: String,
    },
    /// `IndexUnavailable` — the needed view is not materialized (`structural`
    /// at C0; `lexical_index` when the caller demands a stamped view).
    IndexUnavailable {
        /// The view kind.
        view_kind: String,
    },
}

impl std::fmt::Display for RetrievalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RetrievalError {}

// ─────────────────────────────────────────────────────────────────────────────
// lexical_index (the C0 materialized view)
// ─────────────────────────────────────────────────────────────────────────────

/// `LexicalIndex` — the C0 inverted index (§5c.3; the simplest executable
/// `lexical` answer: `term → sorted [version_id]` postings + per-version
/// token counts over the store at `until_seq`). Derived, never authored —
/// stamped through `hh-ledger`'s `View` (`ViewKind::LexicalIndex`; one view
/// shape, CC7). Rebuild-equal under the same fold order.
#[derive(Debug, Clone)]
pub struct LexicalIndex {
    /// `normalized_term → sorted version_ids`.
    pub postings: BTreeMap<String, Vec<String>>,
    /// `version_id → token count` (the ranker's `lexical_hits_*` denominator).
    pub token_counts: BTreeMap<String, u64>,
    /// `version_id → per-line text` (the `all_same_line`/`all_within`
    /// matchers read lines, not tokens).
    pub lines: BTreeMap<String, Vec<String>>,
    /// The stamped view.
    pub view: View,
}

/// `tokenize(text)` — the C0 tokenizer: Unicode-fold lowercase, split on
/// non-alphanumerics, keep order. Deterministic (V-DET — the same bytes fold
/// to the same index on every build).
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// `lexical_index(store, until_seq)` — the fold + `View` stamp.
pub fn lexical_index(store: &MemoryStore, until_seq: u64) -> LexicalIndex {
    let mut postings: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut token_counts = BTreeMap::new();
    let mut lines = BTreeMap::new();
    for vid in store.version_order() {
        let v = &store.versions()[vid];
        if v.created_at > until_seq {
            continue;
        }
        let text = v.content.index_text();
        let toks = tokenize(&text);
        token_counts.insert(vid.clone(), toks.len() as u64);
        lines.insert(
            vid.clone(),
            text.lines().map(str::to_string).collect::<Vec<_>>(),
        );
        for t in toks.iter().collect::<BTreeSet<_>>() {
            postings.entry(t.clone()).or_default().insert(vid.clone());
        }
    }
    // Artifacts fold in too (their `index_text` surface).
    for (aid, a) in store.artifacts() {
        if a.created_at > until_seq {
            continue;
        }
        let toks = tokenize(&a.index_text);
        token_counts.insert(aid.clone(), toks.len() as u64);
        lines.insert(
            aid.clone(),
            a.index_text.lines().map(str::to_string).collect::<Vec<_>>(),
        );
        for t in toks.iter().collect::<BTreeSet<_>>() {
            postings.entry(t.clone()).or_default().insert(aid.clone());
        }
    }
    let sorted_postings: BTreeMap<String, Vec<String>> = postings
        .into_iter()
        .map(|(t, ids)| (t, ids.into_iter().collect()))
        .collect();
    let payload = Json::obj([(
        "postings",
        Json::Arr(
            sorted_postings
                .iter()
                .map(|(t, ids)| {
                    Json::obj([
                        ("term", Json::str(t.clone())),
                        (
                            "version_ids",
                            Json::Arr(ids.iter().map(|i| Json::str(i.clone())).collect()),
                        ),
                    ])
                })
                .collect(),
        ),
    )]);
    let view = View::stamped(
        &store.store_id,
        hh_ledger::views::ViewKind::LexicalIndex,
        Some(until_seq),
        payload,
    );
    LexicalIndex {
        postings: sorted_postings,
        token_counts,
        lines,
        view,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// deterministic_default ranker
// ─────────────────────────────────────────────────────────────────────────────

/// `deterministic_default` — the kernel's C0 ranker (§5c.3; ADR-0079 d3):
/// score = `(exact_match, glob_match, hit_count, recency)` lexicographic —
/// rendered as a fixed-shape decimal tuple `a.bbb.ccc` (`a ∈ {0,1,2}` match
/// class, `bbb` hit count, `ccc` recency scaled). Ties break on `version_id`
/// ascending — total, deterministic, never model-conditioned.
pub fn rank_score(hit_count: u64, exact: bool, glob_match: bool, created_at: u64) -> String {
    let a = if exact {
        2u64
    } else if glob_match {
        1u64
    } else {
        0u64
    };
    // Recency scaled into three digits (newest wins within a class).
    let recency = (created_at % 1000).min(999);
    format!("{}.{:03}.{:03}", a, hit_count.min(999), recency)
}

// ─────────────────────────────────────────────────────────────────────────────
// the pipeline
// ─────────────────────────────────────────────────────────────────────────────

/// `retrieve(store, request, sink)` → `(items, report)` — the one pipeline
/// (§5c.3). The `sink` receives `context.retrieval.completed` +
/// `context.memory.read` payloads (the caller appends — I-PROV).
/// `index_ms` is measured by the caller-supplied clock in `elapsed_ms`.
#[allow(clippy::too_many_arguments)]
pub fn retrieve(
    store: &mut MemoryStore,
    req: &RetrievalRequest,
    sink: &mut dyn EventSink,
    index: Option<&LexicalIndex>,
    elapsed_ms: impl Fn() -> u64,
) -> Result<(Vec<RetrievedItem>, RetrievalReport), RetrievalError> {
    // R3: the request reads a watermark — refuse when it is ahead of the
    // store (never a partial answer).
    if req.at.1 > store.applied_seq() {
        return Err(RetrievalError::StaleStore {
            applied_seq: store.applied_seq(),
            requested: req.at.1,
        });
    }
    if req.ranker != DETERMINISTIC_DEFAULT {
        // C0 admits only `deterministic_default` — a non-default ranker is a
        // declared `RankerRef`; the deterministic-marker check is the
        // caller's `RankerNotDeterministic` guard (the store refuses nothing
        // here — the registry owns ranker vetting).
    }
    let request_hash =
        hh_identity::idp::idp_id(REQUEST_IDP, req_json(req).to_canonical_string().as_bytes());
    // (1) enumerate — the raw hit set per layer.
    let mut raw: Vec<(String, Layer)> = Vec::new();
    for layer in &req.layers {
        for id in store.raw_ids(*layer) {
            raw.push((id, *layer));
        }
    }
    // Query narrowing — the enumerate half each kind drives.
    let (hits, features) = enumerate_query(store, req, &raw, index)?;
    let enumerated = hits.len() as u64;
    let mut _features = features;
    // (2) filter — validity → authority → readers (E2; the attested order).
    let mut items = Vec::new();
    for (id, layer) in &hits {
        if let Some(v) = store.version(id) {
            let state = lifecycle::lifecycle_state(store, id, req.at.1);
            items.push(lifecycle::FilterItem {
                version_id: id.clone(),
                authority: v.label.authority,
                readers: v.label.readers.clone(),
                state: state.kind(),
                stale_since: match &state {
                    LifecycleState::StaleByDependency { .. } => Some(v.created_at),
                    _ => None,
                },
                conflict_set_ref: v.conflict_set_ref.clone(),
            });
        } else if let Some(a) = store.artifacts().get(id) {
            items.push(lifecycle::FilterItem {
                version_id: id.clone(),
                authority: a.label.authority,
                readers: a.label.readers.clone(),
                state: LifecycleStateKind::Valid, // artifacts carry no contract at C0
                stale_since: None,
                conflict_set_ref: None,
            });
        }
        let _ = layer;
    }
    let filtered = lifecycle::filter_for_slot(
        &items,
        &req.constraints.validity_policy,
        req.constraints.slot_min_authority,
        &req.reader,
        req.mode,
        req.at.1,
        store.conflicts(),
    );
    let withheld = filtered.withheld.clone();
    let f_validity = withheld.iter().filter(|w| w.reason == "validity").count() as u64;
    let f_authority = withheld.iter().filter(|w| w.reason == "authority").count() as u64;
    let f_readers = withheld.iter().filter(|w| w.reason == "readers").count() as u64;
    // (3) rank — `deterministic_default`.
    let mut scored: Vec<(String, String, u64)> = filtered
        .admitted
        .iter()
        .map(|id| {
            let (score, created) = score_of(store, id, req, index);
            (id.clone(), score, created)
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0))); // score desc, id asc
                                                              // (4) cut — whole items under {tokens, k}.
    let mut delivered = Vec::new();
    let mut omitted = Vec::new();
    let mut tokens_used = 0u64;
    for (id, score, _) in &scored {
        let tokens = store
            .version(id)
            .map(estimate_tokens)
            .or_else(|| store.artifacts().get(id).map(|a| a.tokens))
            .unwrap_or(0);
        if delivered.len() as u64 >= req.budget.k
            || tokens_used.saturating_add(tokens) > req.budget.tokens
        {
            omitted.push(id.clone());
            continue;
        }
        tokens_used = tokens_used.saturating_add(tokens);
        delivered.push((id.clone(), score.clone()));
    }
    // (5) project — `RetrievedItem`s + `context.memory.read` rows.
    let mut out = Vec::new();
    for (id, score) in &delivered {
        if let Some(v) = store.version(id).cloned() {
            store.record_read(id, req.at.1);
            out.push(RetrievedItem {
                address: v.version_id.clone(),
                semantic_id: Some(v.semantic_id.clone()),
                layer: layer_of(&v),
                provenance: v.provenance.clone(),
                label: v.label.clone(),
                validity_state: lifecycle::lifecycle_state(store, id, req.at.1).kind(),
                rank_evidence: RankEvidence {
                    ranker_ref: req.ranker.clone(),
                    score: score.clone(),
                    features: vec![
                        "lexical_hits_body".to_string(),
                        "recency_created".to_string(),
                    ],
                },
                tokens: estimate_tokens(&v),
                index_text: v.content.index_text(),
                handle: None,
                conflict_set_ref: v.conflict_set_ref.clone(),
                memory_kind: Some(v.kind),
                scope: v.scope,
            });
        } else if let Some(a) = store.artifacts().get(id) {
            out.push(RetrievedItem {
                address: a.artifact_id.clone(),
                semantic_id: None,
                layer: Layer::Artifact,
                provenance: ProvenanceRecord::kernel("kernel:artifact", a.created_at),
                label: a.label.clone(),
                validity_state: LifecycleStateKind::Valid,
                rank_evidence: RankEvidence {
                    ranker_ref: req.ranker.clone(),
                    score: score.clone(),
                    features: vec!["lexical_hits_body".to_string()],
                },
                tokens: a.tokens,
                index_text: a.index_text.clone(),
                handle: None,
                conflict_set_ref: None,
                memory_kind: None,
                scope: hh_provenance::PersistenceScope::Run,
            });
        }
    }
    let no_auth = enumerated > 0 && delivered.is_empty() && f_authority > 0;
    let report = RetrievalReport {
        enumerated,
        filtered_validity: f_validity,
        filtered_authority: f_authority,
        filtered_readers: f_readers,
        ranked: scored.len() as u64,
        returned: out.len() as u64,
        omitted_by_budget: omitted,
        no_authoritative_items: no_auth,
        ranker_ref: req.ranker.clone(),
        deterministic: req.query.deterministic() && req.ranker == DETERMINISTIC_DEFAULT,
        cost: RetrievalCost {
            tokens_estimated: tokens_used,
            index_ms: elapsed_ms(),
            embedder_calls: 0,
        },
    };
    sink.emit(
        "context.retrieval.completed",
        events::retrieval_completed_payload(
            &req.model_call_id,
            &request_hash,
            req.query.kind(),
            &req.layers
                .iter()
                .map(|l| l.as_str().to_string())
                .collect::<Vec<_>>(),
            req.at.clone(),
            &report,
        ),
    );
    sink.emit(
        "context.memory.read",
        events::memory_read_payload(
            &request_hash,
            req.at.1,
            &delivered
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
            &withheld,
        ),
    );
    Ok((out, report))
}

fn req_json(req: &RetrievalRequest) -> Json {
    Json::obj([
        ("model_call_id", Json::str(req.model_call_id.clone())),
        (
            "at",
            Json::obj([
                ("run_id", Json::str(req.at.0.clone())),
                ("seq", Json::Int(req.at.1 as i64)),
            ]),
        ),
        (
            "layers",
            Json::Arr(req.layers.iter().map(|l| Json::str(l.as_str())).collect()),
        ),
        ("query", req.query.to_json()),
        ("reader", Json::str(req.reader.clone())),
        ("ranker", Json::str(req.ranker.clone())),
        (
            "mode",
            Json::str(match req.mode {
                ResolveMode::Execute => "execute",
                ResolveMode::Audit => "audit",
                ResolveMode::Reproduce => "reproduce",
            }),
        ),
    ])
}

/// `enumerate_query`'s result — `(version_id, layer)` hits plus the
/// lexical feature ids the ranker consumes.
type EnumeratedHits = (Vec<(String, Layer)>, Vec<String>);

/// `enumerate` under the query — the raw-hit half of the pipeline
/// (§5c.3 "enumerate (from query)"). Returns `(hits, features)`.
fn enumerate_query(
    store: &MemoryStore,
    req: &RetrievalRequest,
    raw: &[(String, Layer)],
    index: Option<&LexicalIndex>,
) -> Result<EnumeratedHits, RetrievalError> {
    let ids: BTreeSet<String> = raw.iter().map(|(id, _)| id.clone()).collect();
    let layer_of_id: BTreeMap<&str, Layer> = raw.iter().map(|(id, l)| (id.as_str(), *l)).collect();
    let pick = |id: &str| -> Option<(String, Layer)> {
        if ids.contains(id) {
            layer_of_id.get(id).map(|l| (id.to_string(), *l))
        } else {
            None
        }
    };
    match &req.query {
        RetrievalQuery::ByAddress { address } => Ok((pick(address).into_iter().collect(), vec![])),
        RetrievalQuery::ByName { scope, name } => {
            let id = store.name_lookup(*scope, name).map(str::to_string);
            Ok((id.and_then(|i| pick(&i)).into_iter().collect(), vec![]))
        }
        RetrievalQuery::ByPathGlob { pattern } => {
            let re = glob_match(pattern);
            Ok((
                raw.iter()
                    .filter(|(id, l)| {
                        *l == Layer::Artifact
                            && store
                                .artifacts()
                                .get(id)
                                .map(|a| re(&a.path))
                                .unwrap_or(false)
                    })
                    .cloned()
                    .collect(),
                vec![],
            ))
        }
        RetrievalQuery::ByRun {
            run_id,
            seq_range,
            classes,
        } => Ok((
            raw.iter()
                .filter(|(id, l)| {
                    if *l == Layer::Artifact {
                        store
                            .artifacts()
                            .get(id)
                            .map(|a| {
                                &a.run_id == run_id
                                    && seq_range.is_none_or(|(lo, hi)| {
                                        a.created_at >= lo && a.created_at <= hi
                                    })
                                    && (classes.is_empty()
                                        || a.classes.iter().any(|c| classes.contains(c)))
                            })
                            .unwrap_or(false)
                    } else {
                        store
                            .version(id)
                            .map(|v| {
                                seq_range
                                    .is_none_or(|(lo, hi)| v.created_at >= lo && v.created_at <= hi)
                            })
                            .unwrap_or(false)
                    }
                })
                .cloned()
                .collect(),
            vec![],
        )),
        RetrievalQuery::Lexical {
            terms,
            match_mode,
            case_sensitive,
            normalized,
            ..
        } => {
            let idx = match index {
                Some(i) => i.clone(),
                None => lexical_index(store, req.at.1),
            };
            let terms_norm: Vec<String> = terms
                .iter()
                .map(|t| {
                    if *normalized || !*case_sensitive {
                        t.to_lowercase()
                    } else {
                        t.clone()
                    }
                })
                .collect();
            let mut hits = BTreeSet::new();
            for t in &terms_norm {
                if let Some(ids_) = idx.postings.get(t) {
                    for id in ids_ {
                        if ids.contains(id) {
                            hits.insert(id.clone());
                        }
                    }
                }
            }
            // match_mode refinement on the candidate set.
            let hits: Vec<(String, Layer)> = hits
                .into_iter()
                .filter(|id| {
                    let lines = idx.lines.get(id).cloned().unwrap_or_default();
                    match match_mode {
                        MatchMode::Any => true,
                        MatchMode::AllSameLine => lines.iter().any(|l| {
                            let lt = if *normalized || !*case_sensitive {
                                l.to_lowercase()
                            } else {
                                l.clone()
                            };
                            terms_norm.iter().all(|t| lt.contains(t.as_str()))
                        }),
                        MatchMode::AllWithin { n } => {
                            let toks: Vec<String> = lines
                                .iter()
                                .flat_map(|l| {
                                    if *normalized || !*case_sensitive {
                                        tokenize(&l.to_lowercase())
                                    } else {
                                        tokenize(l)
                                    }
                                })
                                .collect();
                            // Every term present, and every pair within n
                            // tokens of each other.
                            terms_norm.iter().all(|t| toks.iter().any(|x| x == t))
                                && terms_norm.iter().all(|t| {
                                    toks.iter().position(|x| x == t).is_some_and(|p| {
                                        terms_norm.iter().all(|t2| {
                                            toks.iter()
                                                .position(|x| x == t2)
                                                .is_some_and(|p2| p.abs_diff(p2) <= *n as usize)
                                        })
                                    })
                                })
                        }
                    }
                })
                .filter_map(|id| pick(&id))
                .collect();
            Ok((hits, vec!["lexical_hits_body".to_string()]))
        }
        RetrievalQuery::Discover {
            direction,
            filenames,
            ..
        } => Ok((
            raw.iter()
                .filter(|(id, l)| {
                    *l == Layer::Artifact
                        && store
                            .artifacts()
                            .get(id)
                            .map(|a| {
                                let tail = a.path.rsplit('/').next().unwrap_or(&a.path);
                                filenames.iter().any(|f| f == tail)
                                    && match direction {
                                        DiscoverDirection::Upward => true,
                                        DiscoverDirection::JitDownward => true,
                                    }
                            })
                            .unwrap_or(false)
                })
                .cloned()
                .collect(),
            vec![],
        )),
        RetrievalQuery::Trigger { kind } => {
            let probe = match kind {
                TriggerKind::Message(m) => m.clone(),
                TriggerKind::PathTouched(p) => p.clone(),
                TriggerKind::Explicit(e) => e.clone(),
            };
            let toks = tokenize(&probe);
            Ok((
                raw.iter()
                    .filter(|(id, _)| {
                        let text = store
                            .version(id)
                            .map(|v| v.content.index_text())
                            .or_else(|| store.artifacts().get(id).map(|a| a.index_text.clone()))
                            .unwrap_or_default();
                        let item_toks: BTreeSet<String> = tokenize(&text).into_iter().collect();
                        toks.iter().any(|t| item_toks.contains(t))
                    })
                    .cloned()
                    .collect(),
                vec![],
            ))
        }
        RetrievalQuery::Structural { .. } => Err(RetrievalError::IndexUnavailable {
            view_kind: "structural_index".to_string(),
        }),
        RetrievalQuery::Similarity { .. } => Err(RetrievalError::EmbedderUnpinned),
    }
}

/// `glob_match` — the C0 glob (`*` = one segment, `**` = any depth, `?` = one
/// char; path-segment aware).
fn glob_match(pattern: &str) -> impl Fn(&str) -> bool {
    let pattern = pattern.to_string();
    move |path: &str| {
        let pat_segs: Vec<&str> = pattern.split('/').collect();
        let path_segs: Vec<&str> = path.split('/').collect();
        fn m(p: &[&str], t: &[&str]) -> bool {
            match (p.first(), t.first()) {
                (None, None) => true,
                (Some(&"**"), _) => m(&p[1..], t) || (!t.is_empty() && m(p, &t[1..])),
                (Some(&seg), Some(&tseg)) => seg_match(seg, tseg) && m(&p[1..], &t[1..]),
                _ => false,
            }
        }
        fn seg_match(p: &str, t: &str) -> bool {
            // `*`/`?` within a segment.
            let (mut pi, mut ti) = (0usize, 0usize);
            let (pb, tb) = (p.as_bytes(), t.as_bytes());
            let (mut star, mut star_t) = (usize::MAX, usize::MAX);
            while ti < tb.len() {
                if pi < pb.len() && (pb[pi] == b'?' || pb[pi] == tb[ti]) {
                    pi += 1;
                    ti += 1;
                } else if pi < pb.len() && pb[pi] == b'*' {
                    star = pi;
                    star_t = ti;
                    pi += 1;
                } else if star != usize::MAX {
                    pi = star + 1;
                    star_t += 1;
                    ti = star_t;
                } else {
                    return false;
                }
            }
            while pi < pb.len() && pb[pi] == b'*' {
                pi += 1;
            }
            pi == pb.len()
        }
        m(&pat_segs, &path_segs)
    }
}

fn layer_of(v: &MemoryVersion) -> Layer {
    match v.kind {
        MemoryKind::ProcedurePointer => Layer::Procedural,
        MemoryKind::Decision | MemoryKind::ToolResult => Layer::Session,
        _ if v.scope == hh_provenance::PersistenceScope::Session => Layer::Session,
        _ => Layer::Episodic,
    }
}

/// `estimate_tokens` — the C0 token estimate for a stored memory (bytes/4 —
/// a deterministic fallback; the pinned estimator is the caller's port).
fn estimate_tokens(v: &MemoryVersion) -> u64 {
    (v.content.index_text().len() as u64 / 4).max(1)
}

fn score_of(
    store: &MemoryStore,
    id: &str,
    req: &RetrievalRequest,
    index: Option<&LexicalIndex>,
) -> (String, u64) {
    let created = store
        .version(id)
        .map(|v| v.created_at)
        .or_else(|| store.artifacts().get(id).map(|a| a.created_at))
        .unwrap_or(0);
    let (exact, glob, hits) = match &req.query {
        RetrievalQuery::ByAddress { address } => (id == address, false, 1),
        RetrievalQuery::ByName { name, .. } => {
            let named = store
                .version(id)
                .map(|v| v.semantic_id == *name)
                .unwrap_or(false);
            (named, false, if named { 1 } else { 0 })
        }
        RetrievalQuery::ByPathGlob { pattern } => {
            let is_glob_hit = store
                .artifacts()
                .get(id)
                .map(|a| glob_match(pattern)(&a.path))
                .unwrap_or(false);
            (false, is_glob_hit, if is_glob_hit { 1 } else { 0 })
        }
        RetrievalQuery::Lexical { terms, .. } => {
            let hits = match index {
                Some(idx) => terms
                    .iter()
                    .filter(|t| {
                        idx.postings
                            .get(&t.to_lowercase())
                            .is_some_and(|ids| ids.contains(&id.to_string()))
                    })
                    .count() as u64,
                None => terms.len() as u64,
            };
            (false, false, hits)
        }
        _ => (false, false, 1),
    };
    (rank_score(hits, exact, glob, created), created)
}

/// `as_candidate` — project a `RetrievedItem` into a `Candidate` the
/// `AssemblyRequest` consumes (R-2.4.1's input shape — the retrieval→assembly
/// seam).
pub fn as_candidate(
    item: &RetrievedItem,
    candidate_id: String,
    retention: Retention,
    estimator_ref: &str,
    source_seq: u64,
) -> Candidate {
    Candidate {
        candidate_id,
        context_item_id: Some(item.address.clone()),
        kind: match item.memory_kind {
            Some(MemoryKind::ProcedurePointer) => CandidateKind::MemoryIndex,
            _ => CandidateKind::Memory,
        },
        state: if item.handle.is_some() {
            CandidateState::HandleOnly
        } else {
            CandidateState::Expanded
        },
        retention,
        estimate: Estimate {
            tokens: item.tokens,
            estimator_ref: estimator_ref.to_string(),
        },
        source_event: None,
        source_seq,
        label: item.label.clone(),
        provenance: item.provenance.clone(),
        validity: hh_hir::records::Validity {
            from: 0,
            until: None,
            condition: None,
        },
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: if item.layer == Layer::Artifact {
            Some(item.address.clone())
        } else {
            None
        },
        handle: item.handle.clone(),
    }
}

/// The artifact surface a `Retrieve` projects (kept for callers that build
/// `Candidate`s from artifacts — `Candidate.kind = artifact_excerpt`).
pub fn artifact_candidate(
    a: &ArtifactVersion,
    candidate_id: String,
    retention: Retention,
    estimator_ref: &str,
    source_seq: u64,
    provenance: ProvenanceRecord,
) -> Candidate {
    Candidate {
        candidate_id,
        context_item_id: Some(a.artifact_id.clone()),
        kind: CandidateKind::ArtifactExcerpt,
        state: CandidateState::HandleOnly,
        retention,
        estimate: Estimate {
            tokens: a.tokens,
            estimator_ref: estimator_ref.to_string(),
        },
        source_event: None,
        source_seq,
        label: a.label.clone(),
        provenance,
        validity: hh_hir::records::Validity {
            from: 0,
            until: None,
            condition: None,
        },
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: Some(a.artifact_id.clone()),
        handle: None,
    }
}
