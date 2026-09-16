//! `usage_mapping` — the per-Model-Profile declaration that lowers a provider's raw
//! usage payload to the **canonical exclusive-role decomposition** (§8.2 §3;
//! ADR-0039 D2; ADR-0043 D5 for the `hh-inclusive/1` `TokenVector` view).
//!
//! Two provider dialects exist in the audited corpus:
//! - `input_exclusive` — `input` already excludes cached tokens (Anthropic/Inspect);
//! - `input_inclusive` — `input` *includes* the cached roles (OpenAI/OTel convention).
//!
//! The canonical decomposition is exclusive: `input.uncached` never contains a cached
//! token, so `tokens.input.total = uncached + cache_read + cache_write` always holds
//! (AC-2). A payload inconsistent with its declared dialect is **refused**
//! ([`UsageError::InconsistentRaw`]) — a negative residual would be a silent lie.
//!
//! Field extraction from the provider's wire shape is adapter-owned (each adapter knows
//! its provider's JSON); the dialect *math* — the part that must be uniform — is here.
//! `cache_write` keeps its `ttl_class` split (provider-conditioned; priced per class —
//! ADR-0039 (d)); the `TokenVector` view sums it.

use hh_ontology::dimensions::{DerivedDimension, DimensionId};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::UsageError;
use crate::quantity::ResourceVector;

/// The provider's usage dialect (§8.2 §3 `usage_mapping.dialect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageDialect {
    /// `input` already excludes cached tokens — uncached = `input` verbatim.
    InputExclusive,
    /// `input` *includes* the cached roles — `uncached = input − cache_read −
    /// Σ cache_write`; a negative residual is [`UsageError::InconsistentRaw`].
    InputInclusive,
}

impl UsageDialect {
    pub fn as_str(self) -> &'static str {
        match self {
            UsageDialect::InputExclusive => "input_exclusive",
            UsageDialect::InputInclusive => "input_inclusive",
        }
    }

    pub fn parse(s: &str) -> Option<UsageDialect> {
        match s {
            "input_exclusive" => Some(UsageDialect::InputExclusive),
            "input_inclusive" => Some(UsageDialect::InputInclusive),
            _ => None,
        }
    }
}

/// Where a provider puts reasoning tokens (§8.2 §3 `usage_mapping.reasoning_source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReasoningSource {
    /// `output` includes reasoning tokens — `visible = output − reasoning`.
    WithinOutput,
    /// `output` excludes reasoning — `visible = output`, reasoning is extra.
    Separate,
}

impl ReasoningSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ReasoningSource::WithinOutput => "within_output",
            ReasoningSource::Separate => "separate",
        }
    }

    pub fn parse(s: &str) -> Option<ReasoningSource> {
        match s {
            "within_output" => Some(ReasoningSource::WithinOutput),
            "separate" => Some(ReasoningSource::Separate),
            _ => None,
        }
    }
}

/// `usage_mapping{dialect, cache_write_ttl_source, reasoning_source}` — the per-profile
/// declaration `normalizer_ref` points at (§8.2 §3; CF-106). Each per-provider mapping
/// is a conditioned rule carrying an assumption-debt record (T-LCD-05) — that record
/// lives on the Model Profile, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMapping {
    /// The dialect the provider's `input` field follows.
    pub dialect: UsageDialect,
    /// The `ttl_class` spellings the provider reports for cache writes (e.g.
    /// `["5m", "1h"]`). `RawUsage.cache_write` keys must be a subset — an undeclared
    /// class is refused.
    pub cache_write_ttl_classes: Vec<String>,
    /// Where reasoning tokens sit in the provider's `output` field.
    pub reasoning_source: ReasoningSource,
}

impl UsageMapping {
    /// The most common audited shape: exclusive input, reasoning inside output.
    pub fn exclusive_default() -> UsageMapping {
        UsageMapping {
            dialect: UsageDialect::InputExclusive,
            cache_write_ttl_classes: vec![],
            reasoning_source: ReasoningSource::WithinOutput,
        }
    }

    /// The inclusive dialect (`input` includes cached tokens).
    pub fn inclusive(ttl_classes: &[&str], reasoning: ReasoningSource) -> UsageMapping {
        UsageMapping {
            dialect: UsageDialect::InputInclusive,
            cache_write_ttl_classes: ttl_classes.iter().map(|s| s.to_string()).collect(),
            reasoning_source: reasoning,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("dialect", Json::str(self.dialect.as_str())),
            (
                "cache_write_ttl_classes",
                Json::Arr(self.cache_write_ttl_classes.iter().map(Json::str).collect()),
            ),
            (
                "reasoning_source",
                Json::str(self.reasoning_source.as_str()),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> Option<UsageMapping> {
        Some(UsageMapping {
            dialect: UsageDialect::parse(j.get("dialect")?.as_str()?)?,
            cache_write_ttl_classes: match j.get("cache_write_ttl_classes") {
                Some(Json::Arr(cs)) => cs
                    .iter()
                    .map(|c| c.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => vec![],
            },
            reasoning_source: ReasoningSource::parse(j.get("reasoning_source")?.as_str()?)?,
        })
    }
}

/// The adapter-extracted raw usage — field *extraction* from the provider's wire JSON
/// is the adapter's job; this is the typed raw shape the decomposition math consumes.
/// All amounts are non-negative integers in the provider's own unit semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawUsage {
    /// The provider's `input`/`prompt_tokens` field (dialect-dependent semantics).
    pub input: i64,
    /// Provider-reported cache reads (K1 hits).
    pub cache_read: i64,
    /// Provider-reported cache writes, keyed by `ttl_class`.
    pub cache_write: BTreeMap<String, i64>,
    /// The provider's `output`/`completion_tokens` field (semantics per
    /// `reasoning_source`).
    pub output: i64,
    /// Provider-reported reasoning/thinking tokens, if reported at all.
    pub reasoning: Option<i64>,
}

/// The canonical exclusive decomposition — the stored primary roles
/// (§8.2 `DimensionId`; ADR-0039 D2/P1 amendment).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenDecomposition {
    /// `tokens.input.uncached` — input tokens that hit no cache.
    pub input_uncached: i64,
    /// `tokens.input.cache_read`.
    pub cache_read: i64,
    /// `tokens.input.cache_write` split by `ttl_class` (the `[ttl_class]` qualifier).
    pub cache_write: BTreeMap<String, i64>,
    /// `tokens.output.visible`.
    pub output_visible: i64,
    /// `tokens.output.reasoning` (0 when the provider does not report reasoning).
    pub output_reasoning: i64,
}

impl TokenDecomposition {
    /// `Σ cache_write` over ttl classes.
    pub fn cache_write_total(&self) -> i64 {
        self.cache_write.values().sum()
    }

    /// `tokens.input.total = uncached + cache_read + cache_write` — the identity that
    /// must hold by construction (AC-2).
    pub fn input_total(&self) -> i64 {
        self.input_uncached + self.cache_read + self.cache_write_total()
    }

    /// `tokens.output.total = visible + reasoning`.
    pub fn output_total(&self) -> i64 {
        self.output_visible + self.output_reasoning
    }

    /// `tokens.blended`.
    pub fn blended(&self) -> i64 {
        self.input_total() + self.output_total()
    }

    /// The `hh-inclusive/1` view (ADR-0043 D5).
    pub fn to_vector(&self) -> TokenVector {
        TokenVector {
            input_total: self.input_total(),
            input_uncached: self.input_uncached,
            cache_read: self.cache_read,
            cache_write: self.cache_write_total(),
            output_total: self.output_total(),
            output_reasoning: self.output_reasoning,
            output_text: Some(self.output_visible),
        }
    }

    /// The chargeable [`ResourceVector`] — one entry per primary role (cache_write's
    /// per-ttl split sums into `tokens.input.cache_write`; the per-class detail rides
    /// on the charge's `cache_ttl` field).
    pub fn to_resource_vector(&self) -> ResourceVector {
        let mut v = ResourceVector::zero();
        v.add(DimensionId::TokensInputUncached, self.input_uncached);
        v.add(DimensionId::TokensInputCacheRead, self.cache_read);
        v.add(DimensionId::TokensInputCacheWrite, self.cache_write_total());
        v.add(DimensionId::TokensOutputVisible, self.output_visible);
        v.add(DimensionId::TokensOutputReasoning, self.output_reasoning);
        v
    }

    /// Canonical JSON form (the `usage.canonical` member of `model.call.completed`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("tokens.input.uncached", Json::Int(self.input_uncached)),
            ("tokens.input.cache_read", Json::Int(self.cache_read)),
            (
                "tokens.input.cache_write",
                Json::Obj(
                    self.cache_write
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v)))
                        .collect(),
                ),
            ),
            ("tokens.output.visible", Json::Int(self.output_visible)),
            ("tokens.output.reasoning", Json::Int(self.output_reasoning)),
            ("tokens.input.total", Json::Int(self.input_total())),
            ("tokens.output.total", Json::Int(self.output_total())),
        ])
    }
}

/// `TokenVector{input_total, input_uncached, cache_read, cache_write, output_total,
/// output_reasoning, output_text?}` under `convention = "hh-inclusive/1"`
/// (ADR-0043 D5) — the *view* shape; the exclusive roles are primary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenVector {
    /// `input_uncached + cache_read + cache_write`.
    pub input_total: i64,
    /// Exclusive uncached input.
    pub input_uncached: i64,
    /// Cache-read input.
    pub cache_read: i64,
    /// Cache-write input (ttl classes summed — the view drops the per-class split).
    pub cache_write: i64,
    /// `output_visible + output_reasoning` (`⊇ output_reasoning`).
    pub output_total: i64,
    /// Hidden reasoning tokens.
    pub output_reasoning: i64,
    /// Visible output text, when known.
    pub output_text: Option<i64>,
}

impl TokenVector {
    /// The convention tag (ADR-0043 D5).
    pub const CONVENTION: &'static str = "hh-inclusive/1";

    /// `budget_utilization`-style derived views can also read token dims straight off
    /// the vector.
    pub fn derived(&self, d: DerivedDimension) -> i64 {
        match d {
            DerivedDimension::TokensInputTotal => self.input_total,
            DerivedDimension::TokensOutputTotal => self.output_total,
            DerivedDimension::TokensBlended => self.input_total + self.output_total,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("convention".to_string(), Json::str(Self::CONVENTION));
        m.insert("input_total".to_string(), Json::Int(self.input_total));
        m.insert("input_uncached".to_string(), Json::Int(self.input_uncached));
        m.insert("cache_read".to_string(), Json::Int(self.cache_read));
        m.insert("cache_write".to_string(), Json::Int(self.cache_write));
        m.insert("output_total".to_string(), Json::Int(self.output_total));
        m.insert(
            "output_reasoning".to_string(),
            Json::Int(self.output_reasoning),
        );
        if let Some(t) = self.output_text {
            m.insert("output_text".to_string(), Json::Int(t));
        }
        Json::Obj(m)
    }
}

/// Lower a [`RawUsage`] under a declared [`UsageMapping`] to the canonical exclusive
/// decomposition. Refuses on inconsistency — a negative residual or an undeclared ttl
/// class is never clamped to a lie.
pub fn decompose(mapping: &UsageMapping, raw: &RawUsage) -> Result<TokenDecomposition, UsageError> {
    // Non-negative inputs only.
    for (name, a) in [
        ("input", raw.input),
        ("cache_read", raw.cache_read),
        ("output", raw.output),
    ] {
        if a < 0 {
            return Err(UsageError::InvalidQuantity {
                detail: format!("{name} = {a} < 0"),
            });
        }
    }
    for (k, a) in &raw.cache_write {
        if *a < 0 {
            return Err(UsageError::InvalidQuantity {
                detail: format!("cache_write[{k}] = {a} < 0"),
            });
        }
        if !mapping.cache_write_ttl_classes.iter().any(|c| c == k) {
            return Err(UsageError::InconsistentRaw {
                detail: format!("cache_write ttl class {k:?} not declared by the usage_mapping"),
            });
        }
    }
    let cache_write_total: i64 = raw.cache_write.values().sum();
    let input_uncached = match mapping.dialect {
        UsageDialect::InputExclusive => raw.input,
        UsageDialect::InputInclusive => {
            let residual = raw.input - raw.cache_read - cache_write_total;
            if residual < 0 {
                return Err(UsageError::InconsistentRaw {
                    detail: format!(
                        "input_inclusive: input {} < cache_read {} + cache_write {}",
                        raw.input, raw.cache_read, cache_write_total
                    ),
                });
            }
            residual
        }
    };
    let reasoning = raw.reasoning.unwrap_or(0);
    if reasoning < 0 {
        return Err(UsageError::InvalidQuantity {
            detail: format!("reasoning = {reasoning} < 0"),
        });
    }
    let output_visible = match mapping.reasoning_source {
        ReasoningSource::Separate => raw.output,
        ReasoningSource::WithinOutput => {
            let residual = raw.output - reasoning;
            if residual < 0 {
                return Err(UsageError::InconsistentRaw {
                    detail: format!(
                        "within_output: output {} < reasoning {}",
                        raw.output, reasoning
                    ),
                });
            }
            residual
        }
    };
    let d = TokenDecomposition {
        input_uncached,
        cache_read: raw.cache_read,
        cache_write: raw.cache_write.clone(),
        output_visible,
        output_reasoning: reasoning,
    };
    // Post-condition (AC-2): the inclusive identity holds by construction.
    debug_assert_eq!(
        d.input_total(),
        input_uncached + raw.cache_read + cache_write_total
    );
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn exclusive_dialect_passes_through() {
        let m = UsageMapping {
            dialect: UsageDialect::InputExclusive,
            cache_write_ttl_classes: vec!["5m".into()],
            reasoning_source: ReasoningSource::WithinOutput,
        };
        let d = decompose(&m, &raw(60, 30, &[("5m", 10)], 40, Some(15))).unwrap();
        assert_eq!(d.input_uncached, 60);
        assert_eq!(d.cache_read, 30);
        assert_eq!(d.cache_write_total(), 10);
        assert_eq!(d.output_visible, 25);
        assert_eq!(d.output_reasoning, 15);
        assert_eq!(d.input_total(), 100);
        assert_eq!(d.output_total(), 40);
        assert_eq!(d.blended(), 140);
    }

    #[test]
    fn inclusive_dialect_subtracts_cached_roles() {
        let m = UsageMapping::inclusive(&["5m", "1h"], ReasoningSource::WithinOutput);
        // Same logical usage, inclusive wire shape — the decomposition is identical.
        let d = decompose(&m, &raw(100, 30, &[("5m", 10)], 40, Some(15))).unwrap();
        assert_eq!(d.input_uncached, 60);
        assert_eq!(d.input_total(), 100);
    }

    #[test]
    fn inclusive_negative_residual_is_refused_not_clamped() {
        let m = UsageMapping::inclusive(&["5m"], ReasoningSource::WithinOutput);
        // input 50 < read 30 + write 30 — inconsistent, never a silent negative.
        let e = decompose(&m, &raw(50, 30, &[("5m", 30)], 10, None)).unwrap_err();
        assert!(matches!(e, UsageError::InconsistentRaw { .. }));
    }

    #[test]
    fn undeclared_ttl_class_is_refused() {
        let m = UsageMapping::inclusive(&["5m"], ReasoningSource::WithinOutput);
        let e = decompose(&m, &raw(10, 0, &[("1h", 5)], 10, None)).unwrap_err();
        assert!(matches!(e, UsageError::InconsistentRaw { .. }));
    }

    #[test]
    fn reasoning_separate_keeps_output_visible() {
        let m = UsageMapping {
            dialect: UsageDialect::InputExclusive,
            cache_write_ttl_classes: vec![],
            reasoning_source: ReasoningSource::Separate,
        };
        let d = decompose(&m, &raw(60, 0, &[], 40, Some(15))).unwrap();
        assert_eq!(d.output_visible, 40);
        assert_eq!(d.output_reasoning, 15);
        assert_eq!(d.output_total(), 55);
    }

    #[test]
    fn reasoning_within_output_must_fit() {
        let m = UsageMapping::exclusive_default();
        let e = decompose(&m, &raw(60, 0, &[], 10, Some(15))).unwrap_err();
        assert!(matches!(e, UsageError::InconsistentRaw { .. }));
    }

    #[test]
    fn absent_reasoning_is_zero_not_missing() {
        let m = UsageMapping::exclusive_default();
        let d = decompose(&m, &raw(60, 0, &[], 40, None)).unwrap();
        assert_eq!(d.output_reasoning, 0);
        assert_eq!(d.output_visible, 40);
    }
}
