//! `evict_oldest` — the C0 `compaction_strategy` variant (spec §5c
//! R-2.4.2; ADR-0075/0146): deterministic, model-call-free, cap-aware.
//! The contract is `{assess, propose, execute, declare}` over the closed
//! op sum `{Evict, Offload, Summarize, Restructure}`; `evict_oldest` emits
//! `Evict` only.
//!
//! Document shapes (canonical `idp/1` JSON — the class contract's Stage-2
//! spelling):
//!
//! - `context_view` — `{items: [{seq, tokens, kind?}], tokens}` ordered
//!   oldest → newest.
//! - `compaction_requirement` — `{kind: hard|soft, cap}`: `hard` requires
//!   the view under `cap`; `soft` requires it under `cap × 0.9` (the
//!   headroom floor — a soft requirement never reports "not required"
//!   while occupancy already exceeds cap).
//! - `assessment` — `{required, reason, cap, tokens}`.
//! - `proposal` — `{ops: [{op: "evict", seqs: [..]}]}` (empty when nothing
//!   is required).
//! - `compaction_result` — `{omissions: [{first_seq, last_seq, tokens}],
//!   evicted_count, remaining_tokens}`. **The omission items carry no
//!   authority member** — V1 forbids a plugin claiming `kernel`; the
//!   `kernel`-authority stamping of omission items is the kernel's own
//!   act when it materialises the result view, never the variant's
//!   claim.
//! - `variant_declaration` — `{deterministic: true, op_kinds: ["evict"],
//!   model_call: false}`.

use hh_embed_schema::plugin_abi::AbiError;
use hh_wire::json::Json;

/// The variant's declaration (conformance `static.*` checks it).
pub fn declaration() -> Json {
    Json::obj([
        ("deterministic", Json::Bool(true)),
        ("model_call", Json::Bool(false)),
        ("op_kinds", Json::Arr(vec![Json::str("evict")])),
        ("class_id", Json::str("compaction_strategy")),
        ("contract_range", Json::str("1.0")),
        ("placement", Json::str("subprocess_confined")),
    ])
}

fn items_of(view: &Json) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    if let Some(Json::Arr(items)) = view.get("items") {
        for it in items {
            let seq = it.get("seq").and_then(Json::as_int).unwrap_or(0);
            let tokens = it.get("tokens").and_then(Json::as_int).unwrap_or(0);
            out.push((seq, tokens));
        }
    }
    out
}

fn tokens_of(view: &Json) -> i64 {
    view.get("tokens")
        .and_then(Json::as_int)
        .unwrap_or_else(|| items_of(view).iter().map(|(_, t)| *t).sum())
}

/// `assess(view, requirement) → {required, reason, cap, tokens}`.
pub fn assess(view: &Json, requirement: &Json) -> Result<Json, AbiError> {
    let cap = requirement
        .get("cap")
        .and_then(Json::as_int)
        .ok_or(AbiError::SchemaViolation)?;
    let kind = requirement
        .get("kind")
        .and_then(Json::as_str)
        .unwrap_or("hard");
    let tokens = tokens_of(view);
    // `hard`: required when occupancy ≥ cap. `soft`: required when
    // occupancy ≥ 9/10 of cap — and *also* when it already exceeds cap (a
    // soft requirement never reports "not required" over the cap).
    let required = match kind {
        "soft" => tokens >= cap || tokens * 10 >= cap * 9,
        _ => tokens >= cap,
    };
    Ok(Json::obj([
        ("required", Json::Bool(required)),
        (
            "reason",
            Json::str(if required {
                format!("occupancy {tokens} meets the {kind} cap {cap}")
            } else {
                format!("occupancy {tokens} under the {kind} cap {cap}")
            }),
        ),
        ("cap", Json::Int(cap)),
        ("tokens", Json::Int(tokens)),
    ]))
}

/// `propose(assessment, view) → {ops}` — evict the oldest items until the
/// view fits the cap (`CompactionImpossible` when the view is already
/// empty yet over).
pub fn propose(assessment: &Json, view: &Json) -> Result<Json, AbiError> {
    let required = assessment
        .get("required")
        .and_then(|j| match j {
            Json::Bool(b) => Some(*b),
            _ => None,
        })
        .ok_or(AbiError::SchemaViolation)?;
    if !required {
        return Ok(Json::obj([("ops", Json::Arr(vec![]))]));
    }
    let cap = assessment
        .get("cap")
        .and_then(Json::as_int)
        .ok_or(AbiError::SchemaViolation)?;
    let mut tokens = tokens_of(view);
    let mut seqs = Vec::new();
    for (seq, t) in items_of(view) {
        if tokens < cap {
            break;
        }
        seqs.push(seq);
        tokens -= t;
    }
    if tokens >= cap {
        // Everything evictable is gone and the view still overflows.
        return Err(AbiError::InsufficientBudget);
    }
    Ok(Json::obj([(
        "ops",
        Json::Arr(vec![Json::obj([
            ("op", Json::str("evict")),
            (
                "seqs",
                Json::Arr(seqs.iter().map(|s| Json::Int(*s)).collect()),
            ),
        ])]),
    )]))
}

/// `execute(proposal, view) → compaction_result` — the contiguous evicted
/// ranges become omission items (authority stamping is the kernel's).
pub fn execute(proposal: &Json, view: &Json) -> Result<Json, AbiError> {
    let mut evicted: Vec<(i64, i64)> = Vec::new();
    if let Some(Json::Arr(ops)) = proposal.get("ops") {
        for op in ops {
            if op.get("op").and_then(Json::as_str) != Some("evict") {
                // `evict_oldest` proposes only `evict` — executing an op it
                // cannot own is an unhandled request, never a silent skip.
                return Err(AbiError::UnhandledOperation);
            }
            if let Some(Json::Arr(seqs)) = op.get("seqs") {
                for s in seqs {
                    if let Some(seq) = s.as_int() {
                        let t = items_of(view)
                            .iter()
                            .find(|(q, _)| *q == seq)
                            .map(|(_, t)| *t)
                            .unwrap_or(0);
                        evicted.push((seq, t));
                    }
                }
            }
        }
    }
    evicted.sort_by_key(|(s, _)| *s);
    // Coalesce contiguous ranges into one omission each.
    let mut omissions: Vec<(i64, i64, i64)> = Vec::new(); // (first, last, tokens)
    for (seq, t) in &evicted {
        match omissions.last_mut() {
            Some((_, last, tok)) if *last + 1 == *seq => {
                *last = *seq;
                *tok += t;
            }
            _ => omissions.push((*seq, *seq, *t)),
        }
    }
    let total: i64 = evicted.iter().map(|(_, t)| *t).sum();
    Ok(Json::obj([
        (
            "omissions",
            Json::Arr(
                omissions
                    .iter()
                    .map(|(f, l, t)| {
                        Json::obj([
                            ("kind", Json::str("omission")),
                            ("first_seq", Json::Int(*f)),
                            ("last_seq", Json::Int(*l)),
                            ("tokens", Json::Int(*t)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("evicted_count", Json::Int(evicted.len() as i64)),
        ("remaining_tokens", Json::Int(tokens_of(view) - total)),
    ]))
}
