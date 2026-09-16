//! AC-R-2.1.6-2 — the `usage_mapping` golden tests.
//!
//! A golden usage payload in each provider dialect lowers to the canonical
//! exclusive decomposition with `input.total = uncached + cache_read +
//! cache_write` under the declared `usage_mapping`; and the same decomposition
//! priced by two independent implementations of the spend derivation agrees to
//! the micro-unit — `hh_budget::price`/`attribute_spend` vs the reference
//! pricer written from §8.2 §3 in this file (the within-E1 half; the
//! cross-candidate half is DF-S1.2-2's corpus extension, Stage 3).

use std::collections::BTreeMap;

use hh_budget::{
    attribute_spend, decompose, price, Attribution, ModelRef, PricingRow, PricingTable, RawUsage,
    ReasoningSource, SpendSource, UsageMapping,
};
use hh_ledger::manifest::EventRef;

fn raw(
    input: i64,
    read: i64,
    write: &[(&str, i64)],
    output: i64,
    reasoning: Option<i64>,
) -> RawUsage {
    RawUsage {
        input,
        cache_read: read,
        cache_write: write.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        output,
        reasoning,
    }
}

/// The golden fixture: one logical call — 100 uncached input + 20 cache-read +
/// 10 cache-write (`5m`) + 20 visible output + 30 reasoning.
fn golden() -> (i64, i64, i64, i64, i64) {
    // (input_uncached, cache_read, cache_write_total, output_visible, output_reasoning)
    (100, 20, 10, 20, 30)
}

#[test]
fn exclusive_dialect_lowers_to_the_canonical_roles() {
    // Anthropic-style: `input` already excludes cached tokens.
    let m = UsageMapping {
        dialect: hh_budget::UsageDialect::InputExclusive,
        cache_write_ttl_classes: vec!["5m".into()],
        reasoning_source: ReasoningSource::WithinOutput,
    };
    let d = decompose(&m, &raw(100, 20, &[("5m", 10)], 50, Some(30))).unwrap();
    let (u, r, w, v, t) = golden();
    assert_eq!(d.input_uncached, u);
    assert_eq!(d.cache_read, r);
    assert_eq!(d.cache_write_total(), w);
    assert_eq!(d.output_visible, v);
    assert_eq!(d.output_reasoning, t);
    // The AC-2 identity holds by construction.
    assert_eq!(d.input_total(), u + r + w);
    assert_eq!(d.output_total(), v + t);
    assert_eq!(d.blended(), u + r + w + v + t);
}

#[test]
fn inclusive_dialect_lowers_to_the_same_canonical_roles() {
    // OpenAI-style: `input` includes the cached tokens — the same logical call
    // reports 130, and lowering recovers the identical exclusive roles.
    let m = UsageMapping::inclusive(&["5m"], ReasoningSource::WithinOutput);
    let d = decompose(&m, &raw(130, 20, &[("5m", 10)], 50, Some(30))).unwrap();
    let (u, r, w, v, t) = golden();
    assert_eq!(d.input_uncached, u);
    assert_eq!(d.cache_read, r);
    assert_eq!(d.cache_write_total(), w);
    assert_eq!(d.output_visible, v);
    assert_eq!(d.output_reasoning, t);
    assert_eq!(d.input_total(), u + r + w);
}

#[test]
fn separate_reasoning_source_does_not_subtract() {
    // A provider reporting reasoning beside output (not inside it): `output` is
    // all visible; reasoning rides its own role.
    let m = UsageMapping {
        dialect: hh_budget::UsageDialect::InputExclusive,
        cache_write_ttl_classes: vec![],
        reasoning_source: ReasoningSource::Separate,
    };
    let d = decompose(&m, &raw(100, 0, &[], 20, Some(30))).unwrap();
    assert_eq!(d.output_visible, 20);
    assert_eq!(d.output_reasoning, 30);
    assert_eq!(d.output_total(), 50);
}

#[test]
fn a_payload_inconsistent_with_its_dialect_is_refused_never_clamped() {
    // Inclusive input smaller than the cached sum — clamping would lie.
    let m = UsageMapping::inclusive(&["5m"], ReasoningSource::WithinOutput);
    assert!(decompose(&m, &raw(25, 20, &[("5m", 10)], 50, Some(30))).is_err());
    // An undeclared ttl class is refused too.
    assert!(decompose(&m, &raw(130, 20, &[("1h", 10)], 50, Some(30))).is_err());
    // WithinOutput with reasoning > output refuses.
    assert!(decompose(&m, &raw(130, 20, &[("5m", 10)], 20, Some(30))).is_err());
}

/// The reference pricer — written straight from §8.2 §3 ("per-role quantity ×
/// micro-unit rate, summed in the row's currency"), independently of
/// `pricing.rs`'s `Role`/`rate`/`add` machinery: different traversal (field
/// order, no role enum, zero-quantity roles included in the sum).
fn reference_price_micro(d: &hh_budget::TokenDecomposition, row: &PricingRow) -> i64 {
    let mut sum = 0i64;
    sum += d.input_uncached * row.input_uncached;
    sum += d.cache_read * row.input_cache_read;
    for (ttl, qty) in &d.cache_write {
        sum += qty * row.input_cache_write[ttl];
    }
    sum += d.output_visible * row.output_visible;
    sum += d.output_reasoning * row.output_reasoning.unwrap_or(row.output_visible);
    sum
}

fn table() -> PricingTable {
    PricingTable {
        table_id: "golden-pricing".into(),
        version: "v1".into(),
        currency: "USD".into(),
        rows: vec![PricingRow {
            model_ref: "m-1@route-a".into(),
            input_uncached: 3,
            input_cache_read: 1,
            input_cache_write: BTreeMap::from([("5m".to_string(), 4)]),
            output_visible: 10,
            output_reasoning: None, // the audited default: reasoning = visible rate
            tiers: vec![],
            valid_from: "2026-01-01".into(),
            source: "golden fixture".into(),
        }],
    }
}

fn model() -> ModelRef {
    ModelRef {
        profile_ref: "profile:golden".into(),
        provider_model_id: "m-1".into(),
        serving_route: "route-a".into(),
        effort: None,
    }
}

#[test]
fn two_implementations_agree_to_the_micro_unit() {
    let m = UsageMapping {
        dialect: hh_budget::UsageDialect::InputExclusive,
        cache_write_ttl_classes: vec!["5m".into()],
        reasoning_source: ReasoningSource::WithinOutput,
    };
    let d = decompose(&m, &raw(100, 20, &[("5m", 10)], 50, Some(30))).unwrap();
    let t = table();
    let mr = model();
    let row = t.row(&mr.pricing_key()).unwrap();

    // Implementation 1: the crate's `price` (quantity × rate per role, tiered).
    let (money, roles) = price(&d, &mr, &t).unwrap();
    // Implementation 2: the reference pricer in this file.
    let reference = reference_price_micro(&d, row);
    assert_eq!(money.micro_units, reference);
    assert_eq!(money.currency, "USD");
    // And the role-level map sums to the same total.
    assert_eq!(roles.values().sum::<i64>(), reference);

    // `attribute_spend` carries the identical figure.
    let row_out = attribute_spend(
        EventRef {
            run_id: "run:g".into(),
            event_id: "evt-src".into(),
        },
        &d,
        &mr,
        &t,
        Some("sha256:pinned".into()),
        &SpendSource::Measured,
        1_000_000,
        Attribution::subject("run:g", "b:root", "p:agent"),
    )
    .unwrap();
    assert_eq!(row_out.money.micro_units, reference);
    assert_eq!(row_out.pricing_ref.as_deref(), Some("sha256:pinned"));
}

#[test]
fn a_model_with_no_pricing_row_is_no_price_not_zero() {
    let d = decompose(
        &UsageMapping::exclusive_default(),
        &raw(10, 0, &[], 5, None),
    )
    .unwrap();
    let mut mr = model();
    mr.provider_model_id = "unpriced".into();
    let e = price(&d, &mr, &table()).unwrap_err();
    assert!(matches!(e, hh_budget::SpendError::NoPrice { .. }));
}
