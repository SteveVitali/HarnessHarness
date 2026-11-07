//! R6.7 — PRICE acceptance battery (manifest 260, AR-09; REQ-XC-116,
//! alias REQ-XC-008). The integer-µ$ pricing unit: `PricingTable` carries
//! `rate_unit ∈ {micro_per_token, micro_per_1k_tokens}` (the default is
//! the former, so every pre-existing row keeps its meaning), a call
//! prices as one exact nano-sum rounded once (ceil to the µ$), the roles
//! itemize by floor plus a `rounding` role, and a non-integral rate is
//! refused `NoPrice{unrepresentable_rate}` — never rounded into place.
//!
//! - AC-1 (`AC-4.5-1`): a golden glm-5.3-flash trial prices to the
//!   hand-computed µ$, role items summing to the total via `rounding`.
//! - AC-2: a non-integral rate refuses `NoPrice{unrepresentable_rate}`;
//!   every pre-existing pricing test stays byte-unchanged (a bare
//!   `PricingTable`'s `micro_per_token` semantics is proved below), and
//!   dropping the unit handling re-fails AC-1 (the same rates at
//!   per-token price 1000× — the last test pins the gap).

#![allow(clippy::unwrap_used)]

use hharness_budget::{
    attribute_spend, ollama_glm_5_3_flash, price, Attribution, Derivation, ModelRef, PricingRow,
    PricingTable, PricingUnit, RateUnit, RatedPricingTable, SpendError, SpendSource,
    TokenDecomposition,
};
use hharness_ledger::manifest::EventRef;

/// The golden glm-5.3-flash trial (fixed input / cached / output /
/// reasoning counts) — chosen so the exact nano-sum carries a non-zero
/// µ$ residual for the `rounding` role to hold.
fn glm_trial() -> TokenDecomposition {
    TokenDecomposition {
        input_uncached: 10_007,
        cache_read: 5_003,
        cache_write: Default::default(),
        output_visible: 2_001,
        output_reasoning: 997,
    }
}

fn glm_model() -> ModelRef {
    ModelRef {
        profile_ref: "profile:glm".into(),
        provider_model_id: "glm-5.3-flash".into(),
        serving_route: "ollama".into(),
        effort: None,
    }
}

#[test]
fn ac1_glm_trial_prices_to_the_hand_computed_micro() {
    let table = ollama_glm_5_3_flash();
    assert_eq!(table.rate_unit(), RateUnit::MicroPer1kTokens);
    let row = table.row("glm-5.3-flash@ollama").expect("the glm row");
    // The published §4.1 rates spelled at µ$/1k.
    assert_eq!(row.input_uncached, 150);
    assert_eq!(row.input_cache_read, 30);
    assert_eq!(row.output_visible, 500);
    assert_eq!(row.valid_from, "2026-10-22");
    assert!(row.source.contains("Ollama Cloud pricing"));

    // Hand-computed (REQ-XC-116's exact nano-sum, one ceil):
    //   150·10007 + 30·5003 + 500·2001 + 500·997
    // = 1_501_050 + 150_090 + 1_000_500 + 498_500
    // = 3_150_140 nµ$  →  ceil → 3_151 µ$.
    let (money, roles) = price(&glm_trial(), &glm_model(), &table).unwrap();
    assert_eq!(money.micro_units, 3_151);
    assert_eq!(money.currency, "USD");
    // Roles itemize the per-role floors …
    assert_eq!(roles["tokens.input.uncached"], 1_501);
    assert_eq!(roles["tokens.input.cache_read"], 150);
    assert_eq!(roles["tokens.output.visible"], 1_000);
    assert_eq!(roles["tokens.output.reasoning"], 498);
    // … and the `rounding` role carries the once-rounded residual:
    // 3_151 − (1501+150+1000+498) = 2.
    assert_eq!(roles["rounding"], 2);
    // The itemization sums to the total exactly.
    assert_eq!(roles.values().sum::<i64>(), money.micro_units);
}

#[test]
fn ac1_attribute_spend_carries_the_same_total() {
    let table = ollama_glm_5_3_flash();
    let row = attribute_spend(
        EventRef {
            run_id: "run:glm".into(),
            event_id: "evt-1".into(),
        },
        &glm_trial(),
        &glm_model(),
        &table,
        Some("sha256:glm".into()),
        &SpendSource::Measured,
        1_000_000,
        Attribution::subject("run:glm", "b:root", "p:agent"),
    )
    .unwrap();
    assert_eq!(row.money.micro_units, 3_151);
    assert_eq!(row.roles["rounding"], 2);
    assert_eq!(row.roles.values().sum::<i64>(), 3_151);
    assert!(matches!(
        row.derivation,
        Derivation::TablePriced {
            ref table_ref,
            ref table_version,
            ..
        } if table_ref == "ollama" && table_version == "2026-10-22"
    ));
}

#[test]
fn ac2_non_integral_rate_refuses_unrepresentable_rate() {
    // $0.15/M is 0.15 µ$/token — unrepresentable at `micro_per_token`
    // (C1 F14). The refusal is typed, never a rounded rate.
    let err = PricingRow::from_micro_per_1m(
        RateUnit::MicroPerToken,
        "glm-5.3-flash@ollama",
        150_000,
        30_000,
        500_000,
        "2026-10-22",
        "test",
    )
    .unwrap_err();
    match err {
        SpendError::NoPrice { model_ref, detail } => {
            assert_eq!(model_ref, "glm-5.3-flash@ollama");
            assert!(detail.contains("unrepresentable_rate"), "detail: {detail}");
        }
        other => panic!("expected NoPrice{{unrepresentable_rate}}, got {other:?}"),
    }
    // … on any single non-integral role; the integral per-1k spelling
    // of the same published rates admits cleanly.
    assert!(RateUnit::MicroPerToken
        .rate_from_micro_per_1m(150_000, "m@r", "tokens.input.uncached")
        .is_err());
    assert_eq!(
        RateUnit::MicroPer1kTokens
            .rate_from_micro_per_1m(150_000, "m@r", "tokens.input.uncached")
            .unwrap(),
        150
    );
    // A row that mixes a non-integral role still refuses — nothing is
    // partially declared.
    assert!(PricingRow::from_micro_per_1m(
        RateUnit::MicroPer1kTokens,
        "m@r",
        150_001, // off-by-one → non-integral at per-1k too
        30_000,
        500_000,
        "2026-10-22",
        "test",
    )
    .is_err());
}

#[test]
fn ac2_bare_pricing_table_keeps_micro_per_token_semantics() {
    // The spec's default: `PricingTable` carries `micro_per_token`, and
    // its itemization is the pre-AR-09 arithmetic exactly (every
    // pre-existing pricing test — unit tests, r5_13, golden_usage,
    // account — stays byte-unchanged against this behavior).
    let table = PricingTable {
        table_id: "t".into(),
        version: "v".into(),
        currency: "USD".into(),
        rows: vec![PricingRow {
            model_ref: "glm-5.3-flash@ollama".into(),
            input_uncached: 150,
            input_cache_read: 30,
            input_cache_write: Default::default(),
            output_visible: 500,
            output_reasoning: None,
            tiers: vec![],
            valid_from: "2026-10-22".into(),
            source: "test".into(),
        }],
    };
    assert_eq!(table.rate_unit(), RateUnit::MicroPerToken);
    let (money, roles) = price(&glm_trial(), &glm_model(), &table).unwrap();
    // Per-token the same integer rates price 1000× — the unit is what
    // makes the golden µ$ (removing `rate_unit` handling re-fails AC-1).
    assert_eq!(money.micro_units, 3_150_140);
    // … and an integral nano-sum emits no `rounding` role — the
    // pre-AR-09 itemization, byte-for-byte.
    assert!(!roles.contains_key("rounding"));
    assert_eq!(roles.values().sum::<i64>(), money.micro_units);
}

#[test]
fn unpriced_model_is_noprice_on_a_rated_table() {
    let table = ollama_glm_5_3_flash();
    let missing = ModelRef {
        provider_model_id: "unpriced-model".into(),
        ..glm_model()
    };
    assert!(matches!(
        price(&glm_trial(), &missing, &table),
        Err(SpendError::NoPrice { .. })
    ));
}

#[test]
fn rate_unit_spelling_is_closed() {
    for u in RateUnit::ALL {
        assert_eq!(RateUnit::parse(u.as_str()), Some(u));
    }
    assert_eq!(RateUnit::parse("micro_per_1m_tokens"), None);
    assert_eq!(RateUnit::parse(""), None);
    // `RatedPricingTable` delegates the table members — one table type,
    // the carrier adds only the unit.
    let rated: RatedPricingTable = ollama_glm_5_3_flash();
    assert_eq!(rated.pricing_table().table_id, "ollama");
    assert!(rated.row("glm-5.3-flash@ollama").is_some());
    assert_eq!(rated.as_ref(None).version, "2026-10-22");
}
