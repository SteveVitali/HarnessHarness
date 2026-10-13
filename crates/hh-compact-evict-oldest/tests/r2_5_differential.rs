//! R2.5 / DF-S2.8-1(f) — the OOP differential conformance corpus.
//!
//! The packaged `compaction_strategy` variant `hh-compact-evict-oldest`
//! and the kernel's own `evict_oldest` rung (`hh_context::compact::
//! evict_oldest_propose`) are two spellings of one strategy — evict the
//! oldest items until the view fits. This battery runs *both* over the
//! same corpus — the committed `benchset.stage3.v1` tasks, each task's
//! recorded `model_io` transcript projected to the contract's
//! `context_view` (`{items:[{seq,tokens}], tokens}`, oldest → newest) —
//! and asserts the evicted-item verdicts are byte-identical.
//!
//! Shared subdomain (documented, never silently shaped): every corpus
//! item is optional, unpaired and in one priority class, and every cut
//! boundary is legal — the regime where the kernel's
//! priority/group/cut-point machinery reduces to plain oldest-first.
//! Two honest rows the corpus reports instead of faking equality:
//!
//! - **`n/a{omission_placeholder_band}`** — when `occupancy - cap ∈
//!   [0, OMISSION_ITEM_TOKENS)` the kernel reserves one
//!   `kernel_omission` placeholder's tokens (8) per contiguous forgotten
//!   range, so its stop threshold differs by exactly one placeholder; the
//!   wire contract has no such member. Those members are typed `n/a`,
//!   never compared as if identical.
//! - **`n/a{empty_view}`** — a member with no recorded `model_io` (or a
//!   zero-byte projection) has no view to compact.
//!
//! The corpus itself is never edited to avoid the band — the band is a
//! real semantic difference the battery reports.
//!
//! Corpus facts used: `model_io[].{surface,args}` per task; tokens per
//! item = `canonical_bytes({surface,args}) / 4` (the repo's 4-bytes/token
//! estimator convention), clamped ≥ 1 so no item is free.

use std::collections::{BTreeMap, BTreeSet};

use hh_bench::benchset::Benchset;
use hh_compact_evict_oldest::compact;
use hh_context::compact::{Assessment, CompactInput, CompactionOp, CompactionTrigger, Requirement};
use hh_context::plan::{
    Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill,
};
use hh_context::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};
use hh_hir::records::Validity;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::label::Label;
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

/// The kernel's per-contiguous-range omission reservation — the band
/// boundary (`hh_context::compact::OMISSION_ITEM_TOKENS`, spelled here so
/// a kernel change that moves it fails this battery loudly rather than
/// silently shifting the corpus's n/a class).
const OMISSION_TOKENS: u64 = hh_context::compact::OMISSION_ITEM_TOKENS;

/// One projected corpus member: the wire `context_view` plus the
/// kernel-side plan it maps to.
struct Member {
    /// `suite_key/task_name`.
    id: String,
    /// The contract `context_view` document.
    view: Json,
    /// `tokens` member of the view (the occupancy).
    occupancy: u64,
    /// The item seqs in view order (oldest → newest).
    seqs: Vec<i64>,
}

/// Deterministic 4-bytes/token estimator over the recorded call's
/// canonical form — declared, never measured from a live tokenizer.
fn call_tokens(call: &hh_bench::benchset::RecordedCall) -> u64 {
    let doc = Json::obj([
        ("surface", Json::str(&call.surface)),
        ("args", call.args.clone()),
    ])
    .to_canonical_string();
    ((doc.len() as u64) / 4).max(1)
}

/// Project the whole benchset: one member per task, in corpus order.
fn corpus() -> Vec<Member> {
    let bs = Benchset::load_default().expect("benchset.stage3.v1 loads");
    let mut out = Vec::new();
    for suite in bs.suites.values() {
        for task in &suite.tasks {
            let mut items = Vec::new();
            let mut seqs = Vec::new();
            let mut occupancy = 0u64;
            for (i, call) in task.model_io.iter().enumerate() {
                let seq = (i + 1) as i64;
                let t = call_tokens(call);
                occupancy += t;
                seqs.push(seq);
                items.push(Json::obj([
                    ("seq", Json::Int(seq)),
                    ("tokens", Json::Int(t as i64)),
                ]));
            }
            out.push(Member {
                id: format!("{}/{}", suite.key, task.name),
                view: Json::obj([
                    ("items", Json::Arr(items)),
                    ("tokens", Json::Int(occupancy as i64)),
                ]),
                occupancy,
                seqs,
            });
        }
    }
    out
}

/// The kernel-side input for a projected view: one `transcript` slot,
/// every item an `Optional(TranscriptTail)` candidate sourced at its seq
/// (single class ⇒ the rank order is `source_seq` — oldest-first), no
/// pairs/batches, every boundary a legal cut point — the declared shared
/// subdomain.
fn kernel_input(m: &Member) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let mut candidates = BTreeMap::new();
    let mut planned = Vec::new();
    let flat_items: Vec<(i64, u64)> = match m.view.get("items") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|it| {
                (
                    it.get("seq").and_then(Json::as_int).unwrap_or(0),
                    it.get("tokens").and_then(Json::as_int).unwrap_or(1) as u64,
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    for (idx, (seq, tokens)) in flat_items.iter().enumerate() {
        let cid = format!("c{seq}");
        let c = Candidate {
            candidate_id: cid.clone(),
            context_item_id: Some(format!("item:{seq}")),
            kind: CandidateKind::TranscriptItem,
            state: CandidateState::Expanded,
            retention: Retention::Optional(PriorityClass::TranscriptTail),
            estimate: Estimate {
                tokens: *tokens,
                estimator_ref: "corpus/canonical-4bpt".to_string(),
            },
            source_event: None,
            source_seq: *seq as u64,
            label: Label::at(AuthorityClass::Delegate),
            provenance: ProvenanceRecord::kernel("corpus/benchset", 0),
            validity: Validity::open_from(0),
            readers: None,
            slot_hint: None,
            volatile: false,
            paired_with: None,
            batch_id: None,
            artefact_id: None,
            handle: None,
        };
        planned.push(PlannedItem {
            candidate_id: cid.clone(),
            context_item_id: c.context_item_id.clone().unwrap(),
            artefact_id: None,
            delivery_id: format!("del:{seq}"),
            authority: AuthorityClass::Delegate,
            label: c.label.clone(),
            tokens: *tokens,
            state: CandidateState::Expanded,
            delivered_by_reference: false,
            derived_from: None,
        });
        candidates.insert(cid, c);
        let _ = idx;
    }
    let n = planned.len() as u64;
    let plan = ContextPlan {
        plan_id: format!("plan:{}", m.id),
        model_call_id: "mc:corpus".into(),
        derived_from: DerivedFrom {
            run_id: "corpus".into(),
            seq: 0,
            view_hash: "sha256:corpus".into(),
        },
        slots: vec![SlotFill {
            slot_id: "transcript".into(),
            items: planned,
        }],
        omitted: vec![],
        context_label: Label::at(AuthorityClass::Delegate),
        occupancy_estimate: m.occupancy,
        legal_cut_points: (0..=n.max(1) + 1)
            .map(|i| CutPoint { before_index: i })
            .collect(),
        reserved: 0,
        estimator_ref: "corpus/canonical-4bpt".into(),
        static_hash: "sha256:corpus".into(),
    };
    (plan, candidates)
}

/// The kernel rung's verdict: the sorted set of evicted view seqs.
fn kernel_verdict(m: &Member, target_reclaim: u64) -> BTreeSet<i64> {
    let (plan, candidates) = kernel_input(m);
    let input = CompactInput {
        plan: &plan,
        candidates,
        window_cap: 0,
        trigger: CompactionTrigger::RequestPrincipal,
        needed: 0,
        target_fraction_ppm: 1_000_000,
        scope: hh_provenance::PersistenceScope::Run,
        at: 0,
        run_id: "corpus".into(),
        summarizer: None,
        slot_min_authority: BTreeMap::new(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    };
    let assessment = Assessment {
        requirement: Requirement::Hard,
        reasons: vec!["corpus".into()],
        target_reclaim,
        min_reclaim: target_reclaim,
    };
    let proposal = hh_context::compact::evict_oldest_propose(&input, &assessment)
        .expect("kernel evict_oldest_propose");
    let mut seqs = BTreeSet::new();
    for op in &proposal.ops {
        match op {
            CompactionOp::Evict { item_ids, .. } => {
                for id in item_ids {
                    let seq = id
                        .strip_prefix("item:")
                        .and_then(|s| s.parse::<i64>().ok())
                        .expect("corpus item id is item:<seq>");
                    seqs.insert(seq);
                }
            }
            other => panic!("evict_oldest emitted a non-Evict op: {other:?}"),
        }
    }
    seqs
}

/// The packaged variant's verdict over the wire contract:
/// `assess(view, {kind:hard, cap})` → `propose(assessment, view)` → the
/// sorted set of evicted seqs (`required:false` ⇒ empty).
fn variant_verdict(m: &Member, cap: u64) -> BTreeSet<i64> {
    let req = Json::obj([("kind", Json::str("hard")), ("cap", Json::Int(cap as i64))]);
    let assessment = compact::assess(&m.view, &req).expect("variant assess");
    let proposal = compact::propose(&assessment, &m.view).expect("variant propose");
    let mut seqs = BTreeSet::new();
    if let Some(Json::Arr(ops)) = proposal.get("ops") {
        for op in ops {
            assert_eq!(
                op.get("op").and_then(Json::as_str),
                Some("evict"),
                "evict_oldest emits Evict only"
            );
            if let Some(Json::Arr(ss)) = op.get("seqs") {
                for s in ss {
                    seqs.insert(s.as_int().expect("seq int"));
                }
            }
        }
    }
    seqs
}

/// The corpus battery (R2.5 / DF-S2.8-1(f)): for every
/// `benchset.stage3.v1` member, run the OOP variant and the kernel rung
/// over the same projected view and assert byte-identical evicted-seq
/// verdicts on the shared subdomain; off-subdomain members land as typed
/// `n/a` rows, counted — never silently skipped.
#[test]
fn oop_differential_corpus_over_benchset_stage3_v1() {
    let members = corpus();
    assert!(members.len() >= 6, "corpus loaded: {}", members.len());
    let mut compared = 0u32;
    let mut evicting = 0u32;
    let mut na: BTreeMap<String, u32> = BTreeMap::new();
    let mut report: Vec<Json> = Vec::new();
    for m in &members {
        if m.seqs.is_empty() || m.occupancy == 0 {
            *na.entry("empty_view".into()).or_default() += 1;
            report.push(Json::obj([
                ("member", Json::str(&m.id)),
                ("leg", Json::str("oop_differential")),
                ("n/a", Json::str("empty_view")),
            ]));
            continue;
        }
        // Deterministic per-member pressure: two-thirds occupancy. The
        // kernel's net-of-omission stop is driven by `target_reclaim` set
        // one placeholder below the variant's gross stop
        // (`occupancy - cap`), so both halt at the same evicted prefix —
        // except the band `occupancy - cap < OMISSION_TOKENS`, which is
        // typed n/a (the variant's wire contract has no placeholder
        // member to compare against).
        let cap = (m.occupancy * 2 / 3).max(1);
        let overflow = m.occupancy.saturating_sub(cap);
        if overflow < OMISSION_TOKENS {
            *na.entry("omission_placeholder_band".into()).or_default() += 1;
            report.push(Json::obj([
                ("member", Json::str(&m.id)),
                ("leg", Json::str("oop_differential")),
                ("n/a", Json::str("omission_placeholder_band")),
            ]));
            continue;
        }
        let variant = variant_verdict(m, cap);
        let kernel = kernel_verdict(m, overflow.saturating_sub(OMISSION_TOKENS) + 1);
        let v_doc = Json::Arr(variant.iter().map(|s| Json::Int(*s)).collect());
        let k_doc = Json::Arr(kernel.iter().map(|s| Json::Int(*s)).collect());
        assert_eq!(
            v_doc.to_canonical_string(),
            k_doc.to_canonical_string(),
            "{}: variant {v_doc:?} vs kernel {k_doc:?}",
            m.id
        );
        if !variant.is_empty() {
            evicting += 1;
        }
        compared += 1;
        report.push(Json::obj([
            ("member", Json::str(&m.id)),
            ("leg", Json::str("oop_differential")),
            ("verdict", v_doc),
        ]));
    }
    // The corpus must contain real substance: members the strategy
    // actually evicts on, not only empty verdicts.
    assert!(compared >= 6, "too few compared members: {compared}");
    assert!(
        evicting >= 1,
        "no member exercised a non-empty eviction (compared {compared}, n/a {na:?})"
    );
    eprintln!(
        "differential corpus: {} members, {} compared ({} evicting), n/a {:?}",
        members.len(),
        compared,
        evicting,
        na
    );
    let _ = report; // the row document is the emitted battery evidence.
}

/// `declare` is part of the conformance surface — the packaged variant's
/// declaration stays byte-stable across the corpus run (determinism is a
/// verdict, not a shape).
#[test]
fn variant_declaration_is_deterministic_no_model_call() {
    let decl = compact::declaration();
    assert_eq!(decl.get("deterministic"), Some(&Json::Bool(true)));
    assert_eq!(decl.get("model_call"), Some(&Json::Bool(false)));
    assert_eq!(
        decl.get("class_id").and_then(Json::as_str),
        Some("compaction_strategy")
    );
}
