//! `PricingTable`, `measurement.cost.attributed` (the `SpendRow`), and the ADR-0043
//! measurement stamps (§8.2 §3; ADR-0039 D4; ADR-0043 D4; ADR-0165).
//!
//! **Spend is derived, never raw.** A missing pricing row is [`SpendError::NoPrice`],
//! never zero; a sum across currencies is [`SpendError::MixedCurrency`]; `exact`
//! requires `measured` provenance class and `coverage = 1`; `reported`
//! (`participant_reported`) is `estimate` unless the participant's capability
//! declaration marks cost accounting `supported` and a probe confirmed it (T-LCD-07 —
//! the caller attests that via [`SpendSource::ReportedVerified`]); avoided cost on
//! cache hits is `estimated_from_pricing`.
//!
//! `PricingTable{table_id, version, currency, rows[{model_ref, input_uncached,
//! input_cache_read, input_cache_write{by ttl_class}, output_visible,
//! output_reasoning?, tiers[{threshold_tokens, rates}]?, valid_from, source}]}` — a
//! versioned, content-addressed bundle artifact pinned by `pricing_ref`.

use hharness_ledger::manifest::EventRef;
use hharness_wire::json::Json;
use std::collections::BTreeMap;

use crate::attribution::{Attribution, ModelRef};
use crate::errors::SpendError;
use crate::quantity::{Money, ResourceVector, PPM_SCALE};
use crate::usage::TokenDecomposition;
use hharness_ontology::dimensions::DimensionId;

/// `Money`-carrying provenance — the closed six-valued enum (ADR-0039 P1 amendment;
/// ADR-0043 D4). Where the spend number actually came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CostProvenance {
    /// The provider reported the price (e.g. a `total_cost` field).
    ProviderReported,
    /// A gateway reported the price (e.g. a billing header).
    GatewayReported,
    /// A hosted participant self-reported — `reported` class, `estimate` confidence
    /// unless capability+probe say otherwise.
    ParticipantReported,
    /// Derived from a pinned `PricingTable` — carries `pricing_ref`.
    EstimatedFromPricing,
    /// Reconstructed from a participant's native log.
    ReconstructedFromNativeLog,
    /// Provenance cannot be determined — never coerced to a known class.
    Unknown,
}

impl CostProvenance {
    pub const ALL: [CostProvenance; 6] = [
        CostProvenance::ProviderReported,
        CostProvenance::GatewayReported,
        CostProvenance::ParticipantReported,
        CostProvenance::EstimatedFromPricing,
        CostProvenance::ReconstructedFromNativeLog,
        CostProvenance::Unknown,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CostProvenance::ProviderReported => "provider_reported",
            CostProvenance::GatewayReported => "gateway_reported",
            CostProvenance::ParticipantReported => "participant_reported",
            CostProvenance::EstimatedFromPricing => "estimated_from_pricing",
            CostProvenance::ReconstructedFromNativeLog => "reconstructed_from_native_log",
            CostProvenance::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<CostProvenance> {
        CostProvenance::ALL
            .iter()
            .copied()
            .find(|p| p.as_str() == s)
    }

    /// The derived coarse class (ADR-0043 D4): `measured ⊇ {provider_reported,
    /// gateway_reported}`, `reported = participant_reported`, `reconstructed ⊇
    /// {estimated_from_pricing, reconstructed_from_native_log}`. `unknown` maps to
    /// `ProvenanceClass::Unknown` — never coerced into a known class (ADR-0236 D-6).
    pub fn class(self) -> ProvenanceClass {
        match self {
            CostProvenance::ProviderReported | CostProvenance::GatewayReported => {
                ProvenanceClass::Measured
            }
            CostProvenance::ParticipantReported => ProvenanceClass::Reported,
            CostProvenance::EstimatedFromPricing | CostProvenance::ReconstructedFromNativeLog => {
                ProvenanceClass::Reconstructed
            }
            CostProvenance::Unknown => ProvenanceClass::Unknown,
        }
    }
}

/// `provenance_class ∈ {measured, reported, reconstructed}` — derived, never stored
/// independently. Plus `unknown` for the `CostProvenance::Unknown` residue the spec's
/// three-class mapping does not cover (ADR-0236 D-6 — `unknown` never coerced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProvenanceClass {
    /// Provider- or gateway-reported (measured at the boundary).
    Measured,
    /// Participant-reported.
    Reported,
    /// Estimated from a pinned table or reconstructed from a native log.
    Reconstructed,
    /// `CostProvenance::Unknown` — the spec mapping's uncovered residue.
    Unknown,
}

impl ProvenanceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ProvenanceClass::Measured => "measured",
            ProvenanceClass::Reported => "reported",
            ProvenanceClass::Reconstructed => "reconstructed",
            ProvenanceClass::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<ProvenanceClass> {
        match s {
            "measured" => Some(ProvenanceClass::Measured),
            "reported" => Some(ProvenanceClass::Reported),
            "reconstructed" => Some(ProvenanceClass::Reconstructed),
            "unknown" => Some(ProvenanceClass::Unknown),
            _ => None,
        }
    }
}

/// `confidence ∈ {exact, bounded{lo, hi}, estimate, unknown}` (ADR-0043 D4; CF-105 —
/// `high` collapsed into `bounded`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Measured and complete — requires `provenance_class = measured` and
    /// `coverage = 1` (ppm = 1_000_000).
    Exact,
    /// Bounded — carries `{lo, hi}` micro-unit bounds.
    Bounded { lo: i64, hi: i64 },
    /// An estimate — the default for `reported` provenance.
    Estimate,
    /// Unknown — never coerced.
    Unknown,
}

impl Confidence {
    /// The order for "minimum confidence" reporting (exact > bounded > estimate >
    /// unknown) — `cost_view` reports the minimum, never an average.
    pub fn rank(self) -> u8 {
        match self {
            Confidence::Exact => 3,
            Confidence::Bounded { .. } => 2,
            Confidence::Estimate => 1,
            Confidence::Unknown => 0,
        }
    }

    /// Is this confidence admissible as a *point* on the M2 frontier
    /// (`{exact, bounded}` — estimate/unknown render bands, never points)?
    pub fn is_point_admissible(self) -> bool {
        matches!(self, Confidence::Exact | Confidence::Bounded { .. })
    }

    pub fn to_json(&self) -> Json {
        match self {
            Confidence::Exact => Json::str("exact"),
            Confidence::Bounded { lo, hi } => Json::obj([(
                "bounded",
                Json::obj([("lo", Json::Int(*lo)), ("hi", Json::Int(*hi))]),
            )]),
            Confidence::Estimate => Json::str("estimate"),
            Confidence::Unknown => Json::str("unknown"),
        }
    }

    pub fn from_json(j: &Json) -> Option<Confidence> {
        match j.as_str() {
            Some("exact") => Some(Confidence::Exact),
            Some("estimate") => Some(Confidence::Estimate),
            Some("unknown") => Some(Confidence::Unknown),
            _ => match j.get("bounded") {
                Some(b) => Some(Confidence::Bounded {
                    lo: b.get("lo")?.as_int()?,
                    hi: b.get("hi")?.as_int()?,
                }),
                None => None,
            },
        }
    }
}

/// `derivation ∈ {provider_priced{unit, rate_ref?}, table_priced{table_ref,
/// table_version, tier?, ttl_class?}}` (§8.2 §3) — how the spend figure was derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derivation {
    /// The provider priced it — the reported unit and an optional rate ref.
    ProviderPriced {
        /// The provider's billing unit name.
        unit: String,
        /// An optional rate reference.
        rate_ref: Option<String>,
    },
    /// Priced against a pinned table.
    TablePriced {
        /// `PricingTable.table_id`.
        table_ref: String,
        /// `PricingTable.version`.
        table_version: String,
        /// The tier matched, when the row carries tiers.
        tier: Option<String>,
        /// The ttl class the cache-write role was priced under.
        ttl_class: Option<String>,
    },
}

impl Derivation {
    pub fn to_json(&self) -> Json {
        match self {
            Derivation::ProviderPriced { unit, rate_ref } => {
                let mut m = BTreeMap::new();
                m.insert("unit".to_string(), Json::str(unit));
                if let Some(r) = rate_ref {
                    m.insert("rate_ref".to_string(), Json::str(r));
                }
                Json::obj([("provider_priced", Json::Obj(m))])
            }
            Derivation::TablePriced {
                table_ref,
                table_version,
                tier,
                ttl_class,
            } => {
                let mut m = BTreeMap::new();
                m.insert("table_ref".to_string(), Json::str(table_ref));
                m.insert("table_version".to_string(), Json::str(table_version));
                if let Some(t) = tier {
                    m.insert("tier".to_string(), Json::str(t));
                }
                if let Some(t) = ttl_class {
                    m.insert("ttl_class".to_string(), Json::str(t));
                }
                Json::obj([("table_priced", Json::Obj(m))])
            }
        }
    }

    pub fn from_json(j: &Json) -> Option<Derivation> {
        if let Some(p) = j.get("provider_priced") {
            return Some(Derivation::ProviderPriced {
                unit: p.get("unit")?.as_str()?.to_string(),
                rate_ref: p.get("rate_ref").and_then(Json::as_str).map(str::to_string),
            });
        }
        if let Some(t) = j.get("table_priced") {
            return Some(Derivation::TablePriced {
                table_ref: t.get("table_ref")?.as_str()?.to_string(),
                table_version: t.get("table_version")?.as_str()?.to_string(),
                tier: t.get("tier").and_then(Json::as_str).map(str::to_string),
                ttl_class: t
                    .get("ttl_class")
                    .and_then(Json::as_str)
                    .map(str::to_string),
            });
        }
        None
    }
}

/// `PricingTableRef` — the pinned table coordinate (`pricing_ref` is the content
/// address of the snapshot pinned in the bundle; §8.2 `measurement.cost.attributed`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PricingTableRef {
    /// `PricingTable.table_id`.
    pub table_id: String,
    /// `PricingTable.version`.
    pub version: String,
    /// The content address (`<algorithm>:<hex>`) of the pinned snapshot.
    pub pin: Option<String>,
}

impl PricingTableRef {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("table_id".to_string(), Json::str(&self.table_id));
        m.insert("version".to_string(), Json::str(&self.version));
        if let Some(p) = &self.pin {
            m.insert("pin".to_string(), Json::str(p));
        }
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<PricingTableRef> {
        Some(PricingTableRef {
            table_id: j.get("table_id")?.as_str()?.to_string(),
            version: j.get("version")?.as_str()?.to_string(),
            pin: j.get("pin").and_then(Json::as_str).map(str::to_string),
        })
    }
}

/// `rate_unit ∈ {micro_per_token, micro_per_1k_tokens}` — the denomination a
/// pricing carrier's integer rates are spelled in (AR-09; REQ-XC-116). The
/// default is `micro_per_token`: every pre-existing row keeps its meaning.
/// `micro_per_1k_tokens` aligns a table with the per-1k convention
/// `pricing.json` and the router's `DeclaredPricing` already spell (F11) —
/// a provider rate that lands non-integral at the declared unit is refused
/// `NoPrice{unrepresentable_rate}`, never rounded into place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RateUnit {
    /// Micro-units per token.
    MicroPerToken,
    /// Micro-units per 1k tokens.
    MicroPer1kTokens,
}

impl RateUnit {
    /// The closed set.
    pub const ALL: [RateUnit; 2] = [RateUnit::MicroPerToken, RateUnit::MicroPer1kTokens];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RateUnit::MicroPerToken => "micro_per_token",
            RateUnit::MicroPer1kTokens => "micro_per_1k_tokens",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RateUnit> {
        RateUnit::ALL.iter().copied().find(|u| u.as_str() == s)
    }

    /// The nano-µ$ one `rate × token` product yields under this unit —
    /// the exact per-call sum accumulates nano-units, then rounds once.
    fn nano_scale(self) -> i64 {
        match self {
            // µ$/token × tokens is already µ$ — scale to nano-µ$.
            RateUnit::MicroPerToken => 1_000,
            // µ$/1k tokens × tokens is nano-µ$ directly.
            RateUnit::MicroPer1kTokens => 1,
        }
    }

    /// Convert a provider-published `micro-units per 1M tokens` rate to
    /// this unit's integer spelling — `NoPrice{unrepresentable_rate}`
    /// when the rate is not integral at this unit (a per-1M rate like
    /// glm-5.3-flash's $0.15/M is 0.15 µ$/token: unrepresentable at
    /// `micro_per_token`, exactly 150 at `micro_per_1k_tokens`; F11).
    pub fn rate_from_micro_per_1m(
        self,
        micro_per_1m: i64,
        model_ref: &str,
        role: &str,
    ) -> Result<i64, SpendError> {
        let denom: i64 = match self {
            RateUnit::MicroPerToken => 1_000_000,
            RateUnit::MicroPer1kTokens => 1_000,
        };
        if micro_per_1m % denom != 0 {
            return Err(SpendError::NoPrice {
                model_ref: model_ref.to_string(),
                detail: format!(
                    "unrepresentable_rate: {role} {micro_per_1m} micro-units/1M is not integral at {}",
                    self.as_str()
                ),
            });
        }
        Ok(micro_per_1m / denom)
    }
}

/// `PricingUnit` — the declared [`RateUnit`] a pricing-table carrier binds
/// (AR-09 D2; REQ-XC-116). `PricingTable` carries `micro_per_token` by
/// construction — the spec's default — and [`RatedPricingTable`] carries a
/// declared non-default unit beside the same rows. One table type, one
/// pricing path (CC1): the carrier annotates the unit, it never re-spells
/// the rows or adds a second rate representation.
pub trait PricingUnit {
    /// The unit this carrier's `PricingRow` rates are spelled in.
    fn rate_unit(&self) -> RateUnit;
    /// The underlying table (row lookup; `derivation.table_priced` refs).
    fn pricing_table(&self) -> &PricingTable;
}

/// One `PricingTable` row (§8.2 §3): per-role micro-unit rates for one `model_ref`,
/// `input_cache_write` priced by `ttl_class`, optional `tiers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingRow {
    /// The model this row prices (`ModelRef::pricing_key()` — `provider@serving`).
    pub model_ref: String,
    /// Micro-units per `tokens.input.uncached` token.
    pub input_uncached: i64,
    /// Micro-units per `tokens.input.cache_read` token.
    pub input_cache_read: i64,
    /// Micro-units per `tokens.input.cache_write` token, per `ttl_class`.
    pub input_cache_write: BTreeMap<String, i64>,
    /// Micro-units per `tokens.output.visible` token.
    pub output_visible: i64,
    /// Micro-units per `tokens.output.reasoning` token — absent ⇒ reasoning is priced
    /// at the visible rate (a documented provider default; never a silent zero — a row
    /// that *declares* reasoning but prices it absent falls back to `output_visible`,
    /// which is the audited provider behaviour for reasoning = output price).
    pub output_reasoning: Option<i64>,
    /// Optional tiers `[{threshold_tokens, rates}]` — a quantity at or above a
    /// threshold is priced at that tier's rates (the highest matching threshold wins).
    /// `rates` is `{role → micro-per-unit}` with the same role keys as this row.
    pub tiers: Vec<PricingTier>,
    /// `valid_from` — ISO-8601 date the row takes effect.
    pub valid_from: String,
    /// The rate's provenance note (e.g. `"provider page, 2026-08"`) — record, never
    /// parsed.
    pub source: String,
}

impl PricingRow {
    /// Declare a row from provider-published `micro-units per 1M tokens`
    /// rates at `unit` — `NoPrice{unrepresentable_rate}` on the first
    /// role whose rate is not integral at the declared unit (AR-09;
    /// REQ-XC-116). `input_cache_write` is empty (per-ttl rates declare
    /// their own rows) and `output_reasoning` is absent — reasoning
    /// prices at the visible rate, the audited provider default.
    pub fn from_micro_per_1m(
        unit: RateUnit,
        model_ref: &str,
        input: i64,
        cache_read: i64,
        output: i64,
        valid_from: &str,
        source: &str,
    ) -> Result<PricingRow, SpendError> {
        Ok(PricingRow {
            model_ref: model_ref.to_string(),
            input_uncached: unit.rate_from_micro_per_1m(
                input,
                model_ref,
                Role::InputUncached.as_str(),
            )?,
            input_cache_read: unit.rate_from_micro_per_1m(
                cache_read,
                model_ref,
                Role::InputCacheRead.as_str(),
            )?,
            input_cache_write: BTreeMap::new(),
            output_visible: unit.rate_from_micro_per_1m(
                output,
                model_ref,
                Role::OutputVisible.as_str(),
            )?,
            output_reasoning: None,
            tiers: vec![],
            valid_from: valid_from.to_string(),
            source: source.to_string(),
        })
    }
}

/// A `tiers[]` entry: `{threshold_tokens, rates{role → micro-per-unit}}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingTier {
    /// The input-total threshold at which this tier applies.
    pub threshold_tokens: i64,
    /// The tier's per-role rates (`None` role ⇒ the row's base rate).
    pub rates: BTreeMap<String, i64>,
}

/// `PricingTable{table_id, version, currency, rows[…]}` — a versioned,
/// content-addressed bundle artifact pinned by `pricing_ref`; snapshots by default
/// (OQ-117).
#[derive(Debug, Clone, PartialEq)]
pub struct PricingTable {
    /// The table's semantic id.
    pub table_id: String,
    /// The table version.
    pub version: String,
    /// ISO-4217 currency — every row's micro-units are in this currency.
    pub currency: String,
    /// The per-model rows.
    pub rows: Vec<PricingRow>,
}

impl PricingTable {
    /// The row covering `model_ref` — `None` ⇒ [`SpendError::NoPrice`].
    pub fn row(&self, model_ref: &str) -> Option<&PricingRow> {
        self.rows.iter().find(|r| r.model_ref == model_ref)
    }

    /// This table's ref (for `derivation.table_priced` / `pricing_ref`).
    pub fn as_ref(&self, pin: Option<String>) -> PricingTableRef {
        PricingTableRef {
            table_id: self.table_id.clone(),
            version: self.version.clone(),
            pin,
        }
    }

    /// The declared rate unit — `micro_per_token` by construction (the
    /// REQ-XC-116 default; AR-09): a bare `PricingTable`'s rates are
    /// micro-units per token, and a non-default unit rides
    /// [`PricingTable::at_unit`] instead of a new field the frozen
    /// literal construction sites could not spell.
    pub fn rate_unit(&self) -> RateUnit {
        RateUnit::MicroPerToken
    }

    /// This table under a declared non-default unit — the carrier
    /// [`RatedPricingTable`] the unit-aware `price`/`attribute_spend`
    /// read (AR-09; REQ-XC-116).
    pub fn at_unit(self, rate_unit: RateUnit) -> RatedPricingTable {
        RatedPricingTable {
            table: self,
            rate_unit,
        }
    }
}

impl PricingUnit for PricingTable {
    fn rate_unit(&self) -> RateUnit {
        RateUnit::MicroPerToken
    }
    fn pricing_table(&self) -> &PricingTable {
        self
    }
}

/// `RatedPricingTable { table, rate_unit }` — the one pricing table plus a
/// declared non-default rate unit (AR-09 D2; REQ-XC-116). The rows and
/// their integer rates live only in the wrapped `PricingTable` — the
/// carrier adds the unit, never a second rate representation (CC1).
#[derive(Debug, Clone, PartialEq)]
pub struct RatedPricingTable {
    /// The table whose rows this carrier prices.
    pub table: PricingTable,
    /// The unit the rows' rates are spelled in.
    pub rate_unit: RateUnit,
}

impl RatedPricingTable {
    /// `new(table, rate_unit)` — a table priced under a declared unit.
    pub fn new(table: PricingTable, rate_unit: RateUnit) -> RatedPricingTable {
        RatedPricingTable { table, rate_unit }
    }

    /// The row covering `model_ref` — `None` ⇒ [`SpendError::NoPrice`].
    pub fn row(&self, model_ref: &str) -> Option<&PricingRow> {
        self.table.row(model_ref)
    }

    /// This table's ref (for `derivation.table_priced` / `pricing_ref`).
    pub fn as_ref(&self, pin: Option<String>) -> PricingTableRef {
        self.table.as_ref(pin)
    }
}

impl PricingUnit for RatedPricingTable {
    fn rate_unit(&self) -> RateUnit {
        self.rate_unit
    }
    fn pricing_table(&self) -> &PricingTable {
        &self.table
    }
}

/// The round-6 Ollama pricing table (AR-09; REQ-XC-116): glm-5.3-flash at
/// the provider's published rates — $0.15 input / $0.03 cached input /
/// $0.50 output per 1M tokens (experiments-spec §4.1) — spelled
/// 150 / 30 / 500 micro-units per 1k tokens under
/// `micro_per_1k_tokens`. The published rates land integrally at per-1k;
/// at `micro_per_token` the same declaration refuses
/// `NoPrice{unrepresentable_rate}` (0.15 µ$/token is unrepresentable —
/// C1 F14). Built through [`PricingRow::from_micro_per_1m`] so the
/// conversion path is the row's own.
pub fn ollama_glm_5_3_flash() -> RatedPricingTable {
    let row = PricingRow::from_micro_per_1m(
        RateUnit::MicroPer1kTokens,
        "glm-5.3-flash@ollama",
        150_000, // $0.15 input / 1M tokens
        30_000,  // $0.03 cached input / 1M tokens
        500_000, // $0.50 output / 1M tokens
        "2026-10-22",
        "Ollama Cloud pricing — $0.15/$0.03/$0.50 per 1M (experiments-spec §4.1; design/33 C1, retrieved 2026-11-06)",
    )
    .expect("the published glm-5.3-flash rates are integral at micro_per_1k_tokens");
    PricingTable {
        table_id: "ollama".to_string(),
        version: "2026-10-22".to_string(),
        currency: "USD".to_string(),
        rows: vec![row],
    }
    .at_unit(RateUnit::MicroPer1kTokens)
}

/// The spend-provenance input to `attribute_spend` — where the usage number came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpendSource {
    /// Kernel-measured decomposition (native adapter / boundary interception) —
    /// `provenance = estimated_from_pricing` when priced against a table, or the named
    /// measured provenance when the provider priced it.
    Measured,
    /// A hosted participant's `usage_update` / self-report — `participant_reported`,
    /// `confidence = estimate` (T-LCD-07) unless verified.
    ParticipantReported,
    /// A participant report whose capability declaration marks cost accounting
    /// `supported` *and* a probe confirmed it — may carry `bounded` confidence.
    ReportedVerified { lo: i64, hi: i64 },
    /// Reconstructed from a native log.
    Reconstructed,
}

/// `measurement.cost.attributed` (the `SpendRow`; §8.2 §3 — verbatim fields).
#[derive(Debug, Clone, PartialEq)]
pub struct SpendRow {
    /// Always `spend`.
    pub resource: DimensionId,
    /// The derived amount.
    pub money: Money,
    /// `CostProvenance` (six-valued).
    pub provenance: CostProvenance,
    /// `provenance_class` — derived from `provenance`, carried for readers.
    pub provenance_class: ProvenanceClass,
    /// `derivation`.
    pub derivation: Derivation,
    /// `confidence`.
    pub confidence: Confidence,
    /// `coverage` — fraction of the run's model calls with usage available, ppm of
    /// 1.0. Never summed as complete when < 1 (OQ-033).
    pub coverage_ppm: i64,
    /// `pricing_ref` — the pinned table snapshot's content address id
    /// (`<algorithm>:<hex>`; present for `estimated_from_pricing`/`table_priced`).
    pub pricing_ref: Option<String>,
    /// The producing event.
    pub source_event: EventRef,
    /// The model priced.
    pub model_ref: ModelRef,
    /// The charge attribution.
    pub attribution: Attribution,
    /// The roles priced (per-role micro amounts — the row's own itemization).
    pub roles: BTreeMap<String, i64>,
}

impl SpendRow {
    /// The amount's micro-units.
    pub fn micro_units(&self) -> i64 {
        self.money.micro_units
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("resource".to_string(), Json::str("spend"));
        m.insert("money".to_string(), self.money.to_json());
        m.insert(
            "provenance".to_string(),
            Json::str(self.provenance.as_str()),
        );
        m.insert(
            "provenance_class".to_string(),
            Json::str(self.provenance_class.as_str()),
        );
        m.insert("derivation".to_string(), self.derivation.to_json());
        m.insert("confidence".to_string(), self.confidence.to_json());
        m.insert("coverage".to_string(), Json::Int(self.coverage_ppm));
        if let Some(p) = &self.pricing_ref {
            m.insert("pricing_ref".to_string(), Json::str(p));
        }
        m.insert(
            "source_event".to_string(),
            Json::obj([
                ("run_id", Json::str(&self.source_event.run_id)),
                ("event_id", Json::str(&self.source_event.event_id)),
            ]),
        );
        m.insert("model_ref".to_string(), self.model_ref.to_json());
        m.insert("attribution".to_string(), self.attribution.to_json());
        m.insert(
            "roles".to_string(),
            Json::Obj(
                self.roles
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v)))
                    .collect(),
            ),
        );
        Json::Obj(m)
    }

    /// Decode a `measurement.cost.attributed` payload — `None` on any malformed
    /// member (a corrupt row is never coerced into a known spend). The tree's
    /// fold path reads these.
    pub fn from_json(j: &Json) -> Option<SpendRow> {
        if j.get("resource")?.as_str()? != "spend" {
            return None;
        }
        let se = j.get("source_event")?;
        let mut roles = BTreeMap::new();
        if let Some(Json::Obj(rs)) = j.get("roles") {
            for (k, v) in rs {
                roles.insert(k.clone(), v.as_int()?);
            }
        }
        let provenance = CostProvenance::parse(j.get("provenance")?.as_str()?)?;
        Some(SpendRow {
            resource: DimensionId::Spend,
            money: Money::from_json(j.get("money")?)?,
            provenance,
            provenance_class: provenance.class(),
            derivation: Derivation::from_json(j.get("derivation")?)?,
            confidence: Confidence::from_json(j.get("confidence")?)?,
            coverage_ppm: j.get("coverage")?.as_int()?,
            pricing_ref: j
                .get("pricing_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            source_event: EventRef {
                run_id: se.get("run_id")?.as_str()?.to_string(),
                event_id: se.get("event_id")?.as_str()?.to_string(),
            },
            model_ref: ModelRef::from_json(j.get("model_ref")?)?,
            attribution: Attribution::from_json(j.get("attribution")?)?,
            roles,
        })
    }
}

/// A role in a [`PricingRow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    InputUncached,
    InputCacheRead,
    InputCacheWrite,
    OutputVisible,
    OutputReasoning,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::InputUncached => "tokens.input.uncached",
            Role::InputCacheRead => "tokens.input.cache_read",
            Role::InputCacheWrite => "tokens.input.cache_write",
            Role::OutputVisible => "tokens.output.visible",
            Role::OutputReasoning => "tokens.output.reasoning",
        }
    }
}

fn rate(
    row: &PricingRow,
    tier: Option<&PricingTier>,
    role: Role,
    ttl: Option<&str>,
) -> Option<i64> {
    // Tier rates override the base when named; a tier silent on a role falls back to
    // the row's base rate. A `cache_write` tier rate may key `role[ttl]` or the bare
    // role (the qualified key wins).
    let t = tier.and_then(|t| {
        ttl.and_then(|ttl| t.rates.get(&format!("{}[{}]", role.as_str(), ttl)).copied())
            .or_else(|| t.rates.get(role.as_str()).copied())
    });
    let base = match (role, ttl) {
        (Role::InputUncached, _) => Some(row.input_uncached),
        (Role::InputCacheRead, _) => Some(row.input_cache_read),
        (Role::InputCacheWrite, Some(ttl)) => row.input_cache_write.get(ttl).copied(),
        (Role::InputCacheWrite, None) => None,
        (Role::OutputVisible, _) => Some(row.output_visible),
        (Role::OutputReasoning, _) => row.output_reasoning.or(Some(row.output_visible)),
    };
    t.or(base)
}

/// Price a [`TokenDecomposition`] against a pricing-table carrier —
/// pure micro-unit arithmetic. `Err(NoPrice)` on any missing row/rate,
/// never a silent zero (§8.2 `attribute_spend`; the silent-zero failure
/// mode's containment).
///
/// The carrier's declared [`RateUnit`] fixes the denomination
/// (AR-09; REQ-XC-116): the exact per-call nano-µ$ sum `Σ rate·qty·scale`
/// is computed over every role, then rounded **once** — ceil to the µ$,
/// never per-role. `roles` itemizes each role's floor share; a
/// `rounding` role carries the residual the floors don't cover, so the
/// itemization always sums exactly to `money` (the role is present iff
/// the residual is non-zero — a `micro_per_token` call's nano-sum is
/// always integral, so pre-existing itemizations are byte-identical).
///
/// Returns `(Money, roles)` — `roles` itemizes per-role micro amounts.
pub fn price<T: PricingUnit + ?Sized>(
    decomp: &TokenDecomposition,
    model_ref: &ModelRef,
    table: &T,
) -> Result<(Money, BTreeMap<String, i64>), SpendError> {
    let unit = table.rate_unit();
    let table = table.pricing_table();
    let row = table
        .row(&model_ref.pricing_key())
        .ok_or_else(|| SpendError::NoPrice {
            model_ref: model_ref.pricing_key(),
            detail: format!("no row in table {} v{}", table.table_id, table.version),
        })?;
    // Highest matching tier by threshold over input_total.
    let input_total = decomp.input_total();
    let tier = row
        .tiers
        .iter()
        .filter(|t| t.threshold_tokens <= input_total)
        .max_by_key(|t| t.threshold_tokens);
    let mut roles: BTreeMap<String, i64> = BTreeMap::new();
    let mut nano: i128 = 0;
    let scale = unit.nano_scale() as i128;
    let mut add =
        |role: Role, qty: i64, ttl: Option<&str>, key: String| -> Result<(), SpendError> {
            if qty == 0 {
                return Ok(());
            }
            let r = rate(row, tier, role, ttl).ok_or_else(|| SpendError::NoPrice {
                model_ref: model_ref.pricing_key(),
                detail: format!("no rate for {} (ttl {:?})", role.as_str(), ttl),
            })?;
            // The role's exact nano-µ$ (rate in the declared unit × qty ×
            // scale); the itemized share is the floor — the once-rounded
            // residual rides the `rounding` role, never a per-role ceil.
            let n = (r as i128) * (qty as i128) * scale;
            let floor = i64::try_from(n.div_euclid(1_000)).map_err(|_| {
                SpendError::ConfidenceViolation {
                    detail: format!("{} role exceeds i64 micro-units", role.as_str()),
                }
            })?;
            roles.insert(key, floor);
            nano += n;
            Ok(())
        };
    add(
        Role::InputUncached,
        decomp.input_uncached,
        None,
        Role::InputUncached.as_str().into(),
    )?;
    add(
        Role::InputCacheRead,
        decomp.cache_read,
        None,
        Role::InputCacheRead.as_str().into(),
    )?;
    for (ttl, qty) in &decomp.cache_write {
        add(
            Role::InputCacheWrite,
            *qty,
            Some(ttl),
            format!("{}[{}]", Role::InputCacheWrite.as_str(), ttl),
        )?;
    }
    add(
        Role::OutputVisible,
        decomp.output_visible,
        None,
        Role::OutputVisible.as_str().into(),
    )?;
    add(
        Role::OutputReasoning,
        decomp.output_reasoning,
        None,
        Role::OutputReasoning.as_str().into(),
    )?;
    // One rounding for the whole call: ceil to the µ$.
    let money_nano = nano;
    let mut money = money_nano.div_euclid(1_000);
    if money_nano.rem_euclid(1_000) != 0 {
        money += 1;
    }
    let money = i64::try_from(money).map_err(|_| SpendError::ConfidenceViolation {
        detail: "per-call spend exceeds i64 micro-units".to_string(),
    })?;
    let floor_sum: i64 = roles.values().sum();
    let rounding = money - floor_sum;
    if rounding != 0 {
        roles.insert("rounding".to_string(), rounding);
    }
    Ok((
        Money {
            micro_units: money,
            currency: table.currency.clone(),
        },
        roles,
    ))
}

/// `attribute_spend(source, decomposition, model_ref, table, pin, source_kind,
/// coverage_ppm, attribution) → SpendRow` — the pure core of the ledgered op (the
/// `Account` wrapper appends `measurement.cost.attributed`). Per-role micro sums under
/// one currency; `exact` only when measured with `coverage = 1`.
///
/// `pin` is the content-address id of the pinned table snapshot (the bundle member —
/// `pricing_ref`); the table itself is the value it resolves to.
#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the row's.
pub fn attribute_spend<T: PricingUnit + ?Sized>(
    source_event: EventRef,
    decomp: &TokenDecomposition,
    model_ref: &ModelRef,
    table: &T,
    pin: Option<String>,
    source: &SpendSource,
    coverage_ppm: i64,
    attribution: Attribution,
) -> Result<SpendRow, SpendError> {
    if !(0..=PPM_SCALE).contains(&coverage_ppm) {
        return Err(SpendError::ConfidenceViolation {
            detail: format!("coverage {coverage_ppm} out of ppm range"),
        });
    }
    let (money, roles) = price(decomp, model_ref, table)?;
    let (provenance, confidence) = match source {
        SpendSource::Measured => (
            CostProvenance::EstimatedFromPricing,
            if coverage_ppm == PPM_SCALE {
                Confidence::Exact
            } else {
                Confidence::Estimate
            },
        ),
        SpendSource::ParticipantReported => {
            (CostProvenance::ParticipantReported, Confidence::Estimate)
        }
        SpendSource::ReportedVerified { lo, hi } => (
            CostProvenance::ParticipantReported,
            Confidence::Bounded { lo: *lo, hi: *hi },
        ),
        SpendSource::Reconstructed => (
            CostProvenance::ReconstructedFromNativeLog,
            Confidence::Estimate,
        ),
    };
    // `exact` requires `measured` provenance class and coverage = 1 (§8.2). A table
    // estimate at full coverage is `estimated_from_pricing` → class `reconstructed`;
    // the spec's "exact requires measured AND coverage = 1" means table-priced rows
    // are never `exact` — the provider/gateway-priced paths are. Table-priced rows
    // cap at `estimate`/`bounded`:
    let confidence = match (provenance.class(), confidence) {
        (ProvenanceClass::Reconstructed, Confidence::Exact) => Confidence::Bounded {
            lo: money.micro_units,
            hi: money.micro_units,
        },
        (ProvenanceClass::Reported, Confidence::Exact) => Confidence::Estimate,
        (ProvenanceClass::Unknown, _) => Confidence::Unknown,
        (_, c) => c,
    };
    Ok(SpendRow {
        resource: DimensionId::Spend,
        money,
        provenance,
        provenance_class: provenance.class(),
        derivation: Derivation::TablePriced {
            table_ref: table.pricing_table().table_id.clone(),
            table_version: table.pricing_table().version.clone(),
            tier: None,
            ttl_class: None,
        },
        confidence,
        coverage_ppm,
        pricing_ref: pin,
        source_event,
        model_ref: model_ref.clone(),
        attribution,
        roles,
    })
}

/// Sum money across rows — `MixedCurrency` on a mix, never coerced (ADR-0043 D4).
pub fn sum_money(rows: &[&SpendRow]) -> Result<Money, SpendError> {
    let mut out: Option<Money> = None;
    for r in rows {
        match &mut out {
            None => out = Some(r.money.clone()),
            Some(m) => {
                if m.currency != r.money.currency {
                    return Err(SpendError::MixedCurrency {
                        a: m.currency.clone(),
                        b: r.money.currency.clone(),
                    });
                }
                m.micro_units += r.money.micro_units;
            }
        }
    }
    Ok(out.unwrap_or(Money {
        micro_units: 0,
        currency: String::new(),
    }))
}

/// The `cost_totals` materialized-view result — grouped sums over charge and spend
/// rows (§8.2 `totals`; rebuilt, never a stored counter; reverts/rollbacks exclude
/// by projection, never subtract).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CostTotals {
    /// Sum by dimension.
    pub by_dimension: ResourceVector,
    /// Sum by `model_ref.pricing_key()`.
    pub by_model: BTreeMap<String, ResourceVector>,
    /// Sum by `charged_to`.
    pub by_charged_to: BTreeMap<String, ResourceVector>,
    /// Sum by component variant ref (`""` = unattributed variant).
    pub by_component_variant: BTreeMap<String, ResourceVector>,
    /// Sum by participant ref.
    pub by_participant: BTreeMap<String, ResourceVector>,
    /// Spend rows: provenance mix + minimum confidence + coverage (never averaged).
    pub provenance_mix: BTreeMap<String, i64>,
    /// The minimum `confidence` over spend rows (`None` when no spend rows).
    pub min_confidence: Option<Confidence>,
    /// Spend micro-units by currency (`{currency → micro}` — one entry typical).
    pub spend_by_currency: BTreeMap<String, i64>,
    /// Avoided-cost estimates by cache kind — *beside* realized spend, never
    /// subtracted (ADR-0128: reported in an `avoided_by_cache_kind` group).
    pub avoided_by_cache_kind: BTreeMap<String, i64>,
}

impl CostTotals {
    /// `budget_utilization = consumed / hard` per bounded dimension, ppm.
    pub fn utilization_ppm(&self, dimension: DimensionId, hard: i64) -> Option<i64> {
        if hard <= 0 {
            return None;
        }
        Some(self.by_dimension.get(dimension) * PPM_SCALE / hard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::ChargedTo;
    use crate::usage::{ReasoningSource, UsageDialect, UsageMapping};

    fn model() -> ModelRef {
        ModelRef {
            profile_ref: "sha256:pp".into(),
            provider_model_id: "m1".into(),
            serving_route: "route-a".into(),
            effort: None,
        }
    }

    fn table() -> PricingTable {
        PricingTable {
            table_id: "ptable".into(),
            version: "2026-09".into(),
            currency: "USD".into(),
            rows: vec![PricingRow {
                model_ref: "m1@route-a".into(),
                input_uncached: 3,   // 3 µ$/token
                input_cache_read: 0, // cache reads priced at 0.1× — here 0 is DECLARED
                input_cache_write: [("5m".to_string(), 4)].into_iter().collect(),
                output_visible: 15,
                output_reasoning: Some(15),
                tiers: vec![],
                valid_from: "2026-09-01".into(),
                source: "test".into(),
            }],
        }
    }

    fn decomp() -> TokenDecomposition {
        let m = UsageMapping {
            dialect: UsageDialect::InputInclusive,
            cache_write_ttl_classes: vec!["5m".into()],
            reasoning_source: ReasoningSource::WithinOutput,
        };
        crate::usage::decompose(
            &m,
            &crate::usage::RawUsage {
                input: 100,
                cache_read: 30,
                cache_write: [("5m".to_string(), 10)].into_iter().collect(),
                output: 40,
                reasoning: Some(15),
            },
        )
        .unwrap()
    }

    #[test]
    fn price_sums_roles_in_micro_units() {
        let (money, roles) = price(&decomp(), &model(), &table()).unwrap();
        // 60*3 + 30*0 + 10*4 + 25*15 + 15*15 = 180+0+40+375+225 = 820
        assert_eq!(money.micro_units, 820);
        assert_eq!(money.currency, "USD");
        assert_eq!(roles["tokens.input.uncached"], 180);
        assert_eq!(roles["tokens.input.cache_write[5m]"], 40);
    }

    #[test]
    fn missing_row_is_noprrice_never_zero() {
        let mut t = table();
        t.rows.clear();
        assert!(matches!(
            price(&decomp(), &model(), &t),
            Err(SpendError::NoPrice { .. })
        ));
    }

    #[test]
    fn missing_ttl_rate_is_noprice() {
        let mut d = decomp();
        d.cache_write.insert("1h".into(), 5);
        assert!(matches!(
            price(&d, &model(), &table()),
            Err(SpendError::NoPrice { .. })
        ));
    }

    #[test]
    fn participant_reported_is_estimate() {
        let row = attribute_spend(
            EventRef {
                run_id: "r".into(),
                event_id: "e".into(),
            },
            &decomp(),
            &model(),
            &table(),
            None,
            &SpendSource::ParticipantReported,
            PPM_SCALE,
            Attribution {
                charged_to: ChargedTo::Subject,
                ..Attribution::subject("r", "b", "p")
            },
        )
        .unwrap();
        // `reported` is `estimate` unless capability+probe (T-LCD-07).
        assert_eq!(row.provenance, CostProvenance::ParticipantReported);
        assert_eq!(row.provenance_class, ProvenanceClass::Reported);
        assert_eq!(row.confidence, Confidence::Estimate);
    }

    #[test]
    fn table_priced_full_coverage_is_bounded_not_exact() {
        // `exact` requires *measured* provenance class; a table estimate never is.
        let row = attribute_spend(
            EventRef {
                run_id: "r".into(),
                event_id: "e".into(),
            },
            &decomp(),
            &model(),
            &table(),
            None,
            &SpendSource::Measured,
            PPM_SCALE,
            Attribution::subject("r", "b", "p"),
        )
        .unwrap();
        assert_eq!(row.provenance, CostProvenance::EstimatedFromPricing);
        assert_eq!(row.provenance_class, ProvenanceClass::Reconstructed);
        assert!(matches!(row.confidence, Confidence::Bounded { .. }));
    }

    #[test]
    fn mixed_currency_sum_refuses() {
        let mut row = attribute_spend(
            EventRef {
                run_id: "r".into(),
                event_id: "e".into(),
            },
            &decomp(),
            &model(),
            &table(),
            None,
            &SpendSource::Measured,
            PPM_SCALE,
            Attribution::subject("r", "b", "p"),
        )
        .unwrap();
        let eur = SpendRow {
            money: Money {
                micro_units: 5,
                currency: "EUR".into(),
            },
            ..row.clone()
        };
        assert!(matches!(
            sum_money(&[&row, &eur]),
            Err(SpendError::MixedCurrency { .. })
        ));
        row.money.micro_units = 7;
        let s = sum_money(&[&row]).unwrap();
        assert_eq!(s.micro_units, 7);
    }
}
