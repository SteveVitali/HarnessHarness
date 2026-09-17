//! `TokenVector` normalization (§5h.1 §2.2/§2.6; ADR-0043 D5): the gateway
//! normalizes provider usage **once** into the canonical inclusive form —
//! `input_total = input_uncached + cache_read + cache_write`,
//! `output_total ⊇ output_reasoning`, `convention = "hh-inclusive/1"`,
//! `normalizer_ref` pointing at the Model Profile's `usage_mapping` (§05b).
//! Each per-provider shape is a conditioned rule; the normalized form is what
//! the ledger row, `cost_view` and `cache_hit_rate` all read (one convention —
//! CC1).

use std::collections::BTreeMap;

use hh_budget::{DimensionId, ResourceVector};
use hh_wire::json::Json;

use crate::codec::{expect_obj, int_at, opt_int_at, opt_str_at, reject_unknown, str_at};
use crate::errors::{CodecError, TelemetryError};

/// The one token convention — `"hh-inclusive/1"` (§5h.1 §2.6).
pub const TOKEN_CONVENTION: &str = "hh-inclusive/1";

const RECORD: &str = "TokenVector";

/// `TokenVector{input_total, input_uncached, cache_read, cache_write,
/// output_total, output_reasoning, output_text?}` under
/// `input_total = input_uncached + cache_read + cache_write` and
/// `output_total ⊇ output_reasoning` (§5h.1 §2.6/§3). The exclusive token-role
/// counters of §08 (`DimensionId::TokensInput*`/`TokensOutput*`) are primary;
/// this vector is the view (CF-105/106) — [`TokenVector::to_resource_vector`]
/// is the one projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenVector {
    /// `input_total` — the inclusive input count.
    pub input_total: i64,
    /// `input_uncached` — input tokens that were neither read nor written cache.
    pub input_uncached: i64,
    /// `cache_read` — input tokens served from cache.
    pub cache_read: i64,
    /// `cache_write` — input tokens written to cache.
    pub cache_write: i64,
    /// `output_total` — the inclusive output count (⊇ `output_reasoning`).
    pub output_total: i64,
    /// `output_reasoning` — reasoning/thinking tokens inside the output.
    pub output_reasoning: i64,
    /// `output_text` — visible output tokens, when the provider reports the
    /// split (absent ⇒ `output_total − output_reasoning` is the view's value).
    pub output_text: Option<i64>,
    /// `convention` — always `hh-inclusive/1`.
    pub convention: String,
    /// `normalizer_ref` — the Model Profile's `usage_mapping` ref the
    /// normalization ran under (mandatory — CF-106).
    pub normalizer_ref: String,
}

impl TokenVector {
    /// Check the invariants — every `TokenVector` that exists satisfies them
    /// (construction, normalization and decode all funnel through here).
    pub fn validate(&self) -> Result<(), TelemetryError> {
        let bad = |detail: String| TelemetryError::TokenInvariant { detail };
        if self.convention != TOKEN_CONVENTION {
            return Err(bad(format!(
                "convention must be {TOKEN_CONVENTION}, got {}",
                self.convention
            )));
        }
        if self.normalizer_ref.is_empty() {
            return Err(bad("normalizer_ref is mandatory".into()));
        }
        for (member, v) in [
            ("input_total", self.input_total),
            ("input_uncached", self.input_uncached),
            ("cache_read", self.cache_read),
            ("cache_write", self.cache_write),
            ("output_total", self.output_total),
            ("output_reasoning", self.output_reasoning),
        ] {
            if v < 0 {
                return Err(bad(format!("{member} is negative")));
            }
        }
        if self.input_total != self.input_uncached + self.cache_read + self.cache_write {
            return Err(bad(format!(
                "input_total {} != input_uncached + cache_read + cache_write {}",
                self.input_total,
                self.input_uncached + self.cache_read + self.cache_write
            )));
        }
        if self.output_reasoning > self.output_total {
            return Err(bad("output_reasoning outside output_total".into()));
        }
        if let Some(t) = self.output_text {
            if t < 0 || self.output_reasoning + t > self.output_total {
                return Err(bad(
                    "output_text negative or output_reasoning + output_text > output_total".into(),
                ));
            }
        }
        Ok(())
    }

    /// `cache_read / input_total` in ppm of 1.0 — `None` when `input_total` is
    /// 0 (a 0/0 rate is `n/a{estimator_undefined}`, never 0 — T-LCD-15).
    pub fn cache_hit_rate_ppm(&self) -> Option<i64> {
        if self.input_total == 0 {
            return None;
        }
        Some(self.cache_read * hh_budget::quantity::PPM_SCALE / self.input_total)
    }

    /// The projection onto the primary §08 dimensions (CF-105 — the vector is
    /// the view, the exclusive role counters are primary). `output_text` absent
    /// ⇒ visible = `output_total − output_reasoning`.
    pub fn to_resource_vector(&self) -> ResourceVector {
        let mut v = ResourceVector::zero();
        v.add(DimensionId::TokensInputUncached, self.input_uncached);
        v.add(DimensionId::TokensInputCacheRead, self.cache_read);
        v.add(DimensionId::TokensInputCacheWrite, self.cache_write);
        v.add(DimensionId::TokensOutputReasoning, self.output_reasoning);
        v.add(
            DimensionId::TokensOutputVisible,
            self.output_text
                .unwrap_or(self.output_total - self.output_reasoning),
        );
        v
    }

    /// The canonical JSON form (`output_text` omitted when absent — a member is
    /// never serialized as `null` in the canonical shape).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("input_total".into(), Json::Int(self.input_total));
        m.insert("input_uncached".into(), Json::Int(self.input_uncached));
        m.insert("cache_read".into(), Json::Int(self.cache_read));
        m.insert("cache_write".into(), Json::Int(self.cache_write));
        m.insert("output_total".into(), Json::Int(self.output_total));
        m.insert("output_reasoning".into(), Json::Int(self.output_reasoning));
        if let Some(t) = self.output_text {
            m.insert("output_text".into(), Json::Int(t));
        }
        m.insert("convention".into(), Json::str(&self.convention));
        m.insert("normalizer_ref".into(), Json::str(&self.normalizer_ref));
        Json::Obj(m)
    }

    /// Strict decode — `BadMember` on any unknown member, `TokenInvariant` on a
    /// violated identity.
    pub fn from_json(j: &Json) -> Result<TokenVector, TelemetryError> {
        let m = expect_obj(j, RECORD)?;
        reject_unknown(
            m,
            &[
                "input_total",
                "input_uncached",
                "cache_read",
                "cache_write",
                "output_total",
                "output_reasoning",
                "output_text",
                "convention",
                "normalizer_ref",
            ],
            RECORD,
        )?;
        let v = TokenVector {
            input_total: int_at(m, "input_total", RECORD)?,
            input_uncached: int_at(m, "input_uncached", RECORD)?,
            cache_read: int_at(m, "cache_read", RECORD)?,
            cache_write: int_at(m, "cache_write", RECORD)?,
            output_total: int_at(m, "output_total", RECORD)?,
            output_reasoning: int_at(m, "output_reasoning", RECORD)?,
            output_text: opt_int_at(m, "output_text")?,
            convention: str_at(m, "convention", RECORD)?.to_string(),
            normalizer_ref: str_at(m, "normalizer_ref", RECORD)?.to_string(),
        };
        v.validate()?;
        Ok(v)
    }
}

/// A provider usage shape — the four fixture shapes AC-R-2.9.1-5 names
/// (inclusive-cached, exclusive-cached, details-nested, flat). The gateway's
/// Model Profile `usage_mapping` selects the shape; this crate owns the one
/// normalization (CC1 — never a per-call-site conversion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderUsage {
    /// Inclusive-cached: `input_tokens` already includes the cached read
    /// (`cached_tokens`) — OpenAI-style `{input_tokens, cached_tokens,
    /// cache_creation_tokens?, output_tokens, reasoning_tokens?}`.
    InclusiveCached {
        /// `input_tokens` (includes `cached_tokens`).
        input_tokens: i64,
        /// `cached_tokens` — tokens read from cache inside `input_tokens`.
        cached_tokens: i64,
        /// `cache_creation_tokens` — written-to-cache tokens (0 when absent).
        cache_creation_tokens: i64,
        /// `output_tokens`.
        output_tokens: i64,
        /// Reasoning tokens inside `output_tokens` (0 when absent).
        reasoning_tokens: i64,
    },
    /// Exclusive-cached: `input_tokens` *excludes* the cached read —
    /// Anthropic-style `{input_tokens, cache_read_input_tokens,
    /// cache_creation_input_tokens, output_tokens}`.
    ExclusiveCached {
        /// `input_tokens` — the uncached share only.
        input_tokens: i64,
        /// `cache_read_input_tokens`.
        cache_read_input_tokens: i64,
        /// `cache_creation_input_tokens`.
        cache_creation_input_tokens: i64,
        /// `output_tokens`.
        output_tokens: i64,
    },
    /// Details-nested — `{prompt: {total, cached, cache_write}, completion:
    /// {total, reasoning}}`.
    DetailsNested {
        /// Prompt total (inclusive).
        prompt_total: i64,
        /// Prompt cached (inside the total).
        prompt_cached: i64,
        /// Prompt cache-write (inside the total).
        prompt_cache_write: i64,
        /// Completion total.
        completion_total: i64,
        /// Reasoning inside the completion.
        completion_reasoning: i64,
    },
    /// Flat — `{prompt_tokens, completion_tokens}` with no cache detail
    /// (the cache members are honestly 0 — never guessed).
    Flat {
        /// `prompt_tokens`.
        prompt_tokens: i64,
        /// `completion_tokens`.
        completion_tokens: i64,
    },
}

/// `emit_usage`'s normalization half (§5h.1 §2.2): provider usage → the
/// canonical `TokenVector` under `hh-inclusive/1`, `normalizer_ref` stamped.
/// Every shape converges on the same logical vector (AC-R-2.9.1-5's invariant —
/// `cache_hit_rate` identical for the same logical usage across shapes).
pub fn normalize_usage(
    usage: &ProviderUsage,
    normalizer_ref: &str,
) -> Result<TokenVector, TelemetryError> {
    let v = match usage {
        ProviderUsage::InclusiveCached {
            input_tokens,
            cached_tokens,
            cache_creation_tokens,
            output_tokens,
            reasoning_tokens,
        } => {
            // `input_tokens` includes the cache *read*; the cache *write* is
            // billed on top (provider convention — recorded by the mapping).
            let input_total = input_tokens + cache_creation_tokens;
            TokenVector {
                input_total,
                input_uncached: input_total - cached_tokens - cache_creation_tokens,
                cache_read: *cached_tokens,
                cache_write: *cache_creation_tokens,
                output_total: *output_tokens,
                output_reasoning: *reasoning_tokens,
                output_text: None,
                convention: TOKEN_CONVENTION.to_string(),
                normalizer_ref: normalizer_ref.to_string(),
            }
        }
        ProviderUsage::ExclusiveCached {
            input_tokens,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            output_tokens,
        } => TokenVector {
            input_total: input_tokens + cache_read_input_tokens + cache_creation_input_tokens,
            input_uncached: *input_tokens,
            cache_read: *cache_read_input_tokens,
            cache_write: *cache_creation_input_tokens,
            output_total: *output_tokens,
            output_reasoning: 0,
            output_text: None,
            convention: TOKEN_CONVENTION.to_string(),
            normalizer_ref: normalizer_ref.to_string(),
        },
        ProviderUsage::DetailsNested {
            prompt_total,
            prompt_cached,
            prompt_cache_write,
            completion_total,
            completion_reasoning,
        } => TokenVector {
            input_total: *prompt_total,
            input_uncached: prompt_total - prompt_cached - prompt_cache_write,
            cache_read: *prompt_cached,
            cache_write: *prompt_cache_write,
            output_total: *completion_total,
            output_reasoning: *completion_reasoning,
            output_text: None,
            convention: TOKEN_CONVENTION.to_string(),
            normalizer_ref: normalizer_ref.to_string(),
        },
        ProviderUsage::Flat {
            prompt_tokens,
            completion_tokens,
        } => TokenVector {
            input_total: *prompt_tokens,
            input_uncached: *prompt_tokens,
            cache_read: 0,
            cache_write: 0,
            output_total: *completion_tokens,
            output_reasoning: 0,
            output_text: None,
            convention: TOKEN_CONVENTION.to_string(),
            normalizer_ref: normalizer_ref.to_string(),
        },
    };
    // A provider shape that normalizes to a violating vector is refused with
    // the real invariant error — never clamped or coerced.
    v.validate()?;
    Ok(v)
}

/// Decode a `usage` payload member (`{TokenVector}`) — the strict codec over
/// the event payload, `None`-free: malformed usage is a `CodecError`, never a
/// coerced zero vector.
pub fn usage_from_json(j: &Json) -> Result<TokenVector, CodecError> {
    TokenVector::from_json(j).map_err(|e| match e {
        TelemetryError::Codec(c) => c,
        other => CodecError::TypeMismatch {
            member: other.to_string(),
            expected: "valid TokenVector",
        },
    })
}

/// The optional `usage` member of a model-call payload.
pub fn opt_usage(m: &BTreeMap<String, Json>) -> Result<Option<TokenVector>, TelemetryError> {
    match m.get("usage") {
        Some(j) => Ok(Some(TokenVector::from_json(j)?)),
        None => Ok(None),
    }
}

/// The optional `measured_at` member — parses to the closed sum.
pub fn opt_measured_at(m: &BTreeMap<String, Json>) -> Option<crate::clocks::MeasuredAt> {
    opt_str_at(m, "measured_at")
        .ok()
        .flatten()
        .and_then(|s| crate::clocks::MeasuredAt::parse(s).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec(
        input_total: i64,
        uncached: i64,
        read: i64,
        write: i64,
        output_total: i64,
        reasoning: i64,
    ) -> TokenVector {
        TokenVector {
            input_total,
            input_uncached: uncached,
            cache_read: read,
            cache_write: write,
            output_total,
            output_reasoning: reasoning,
            output_text: None,
            convention: TOKEN_CONVENTION.into(),
            normalizer_ref: "profile:x#usage_mapping".into(),
        }
    }

    #[test]
    fn the_invariant_is_enforced() {
        vec(100, 60, 30, 10, 40, 15).validate().unwrap();
        // input_total = uncached + read + write — violated.
        assert!(matches!(
            vec(100, 61, 30, 10, 40, 15).validate(),
            Err(TelemetryError::TokenInvariant { .. })
        ));
        // output_reasoning ⊆ output_total — violated.
        assert!(vec(100, 60, 30, 10, 10, 15).validate().is_err());
        // Foreign convention refused.
        let mut v = vec(100, 60, 30, 10, 40, 15);
        v.convention = "other/9".into();
        assert!(v.validate().is_err());
        // Missing normalizer_ref refused.
        let mut v = vec(100, 60, 30, 10, 40, 15);
        v.normalizer_ref = String::new();
        assert!(v.validate().is_err());
        // Negative member refused.
        assert!(vec(100, -20, 30, 90, 40, 15).validate().is_err());
    }

    #[test]
    fn the_four_provider_shapes_converge() {
        // AC-R-2.9.1-5's fixture family: the same logical usage (100 input of
        // which 30 cache-read + 10 cache-write, 40 output of which 15 reasoning)
        // across all four shapes yields one TokenVector — cache_hit_rate
        // identical across shapes.
        let shapes = [
            ProviderUsage::InclusiveCached {
                input_tokens: 90, // includes the 30 cached
                cached_tokens: 30,
                cache_creation_tokens: 10,
                output_tokens: 40,
                reasoning_tokens: 15,
            },
            ProviderUsage::ExclusiveCached {
                input_tokens: 60,
                cache_read_input_tokens: 30,
                cache_creation_input_tokens: 10,
                output_tokens: 40,
            },
            ProviderUsage::DetailsNested {
                prompt_total: 100,
                prompt_cached: 30,
                prompt_cache_write: 10,
                completion_total: 40,
                completion_reasoning: 15,
            },
        ];
        let vectors: Vec<TokenVector> = shapes
            .iter()
            .map(|s| normalize_usage(s, "profile:x#usage_mapping").unwrap())
            .collect();
        for v in &vectors {
            assert_eq!(v.input_total, 100);
            assert_eq!(v.input_uncached, 60);
            assert_eq!(v.cache_read, 30);
            assert_eq!(v.cache_write, 10);
            assert_eq!(v.output_total, 40);
            assert_eq!(v.cache_hit_rate_ppm(), Some(300_000));
        }
        // Flat — no cache detail; the cache members are honestly 0.
        let flat = normalize_usage(
            &ProviderUsage::Flat {
                prompt_tokens: 100,
                completion_tokens: 40,
            },
            "profile:x#usage_mapping",
        )
        .unwrap();
        assert_eq!(flat.cache_read, 0);
        assert_eq!(flat.input_total, 100);
    }

    #[test]
    fn codec_is_strict_and_canonical() {
        let v = vec(100, 60, 30, 10, 40, 15);
        let j = v.to_json();
        assert_eq!(TokenVector::from_json(&j).unwrap(), v);
        // Unknown member → BadMember.
        let mut m = match j {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("surprise".into(), Json::Int(1));
        assert!(matches!(
            TokenVector::from_json(&Json::Obj(m)),
            Err(TelemetryError::Codec(CodecError::BadMember { .. }))
        ));
    }

    #[test]
    fn cache_hit_rate_is_never_a_fake_zero() {
        let mut v = vec(0, 0, 0, 0, 0, 0);
        assert_eq!(v.cache_hit_rate_ppm(), None); // 0/0 → n/a, never 0
        v = vec(4, 1, 3, 0, 0, 0);
        assert_eq!(v.cache_hit_rate_ppm(), Some(750_000));
    }
}
