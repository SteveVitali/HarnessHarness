//! The §5b.4 C0 slice (R-2.3.4⁰; ADR-0127/0128): the `CacheSemantics` closed sum
//! carried as **data** on `WireDialect.cache_semantics` (overridable per model
//! through the profile's `cache_control_convention`), the kernel-derived
//! affinity key (CK-1…CK-5), `expected`/`observed` cache state, the closed
//! `miss_reason` vocabulary, `place_markers` (cap/lookback/carrier-eligibility
//! as data — every drop/substitution a ledger fact), and the `model.cache.
//! resolved` vocabulary.
//!
//! **No kernel or codec code branches on a provider or model identifier to
//! choose semantics** (K1-1; AC-R-2.3.4-1) — every decision reads the declared
//! `CacheSemantics` value.

use hh_wire::json::Json;

use crate::errors::CodecError;
use crate::vocab::Purpose;

/// `CacheTier ∈ {static, dynamic, transcript}` — LC-1's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheTier {
    /// `static` — identical bytes across same-purpose calls unless invalidated.
    Static,
    /// `dynamic` — the volatile head (reserved volatile items live here).
    Dynamic,
    /// `transcript` — append-only between compaction/context-edit events.
    Transcript,
}

impl CacheTier {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheTier::Static => "static",
            CacheTier::Dynamic => "dynamic",
            CacheTier::Transcript => "transcript",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CacheTier> {
        Some(match s {
            "static" => CacheTier::Static,
            "dynamic" => CacheTier::Dynamic,
            "transcript" => CacheTier::Transcript,
            _ => return None,
        })
    }
}

/// `RetentionClass{class_id ∈ {short, long, extended}, nominal_ms, guaranteed}`
/// (ADR-0127 d.2 — lowered per dialect; a `DebtHomes/1` home whose removal
/// signal is `cache.expectation_agreement`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionClass {
    /// `class_id` — `short | long | extended`.
    pub class_id: String,
    /// `nominal_ms` — the provider-declared nominal retention.
    pub nominal_ms: u64,
    /// `guaranteed` — whether the retention is guaranteed.
    pub guaranteed: bool,
}

/// `strictness ∈ {tolerant, strict}` (implicit-prefix dialects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplicitStrictness {
    /// Prefix matching tolerates a trailing partial block.
    Tolerant,
    /// Prefix matching is byte-strict.
    Strict,
}

impl ImplicitStrictness {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ImplicitStrictness::Tolerant => "tolerant",
            ImplicitStrictness::Strict => "strict",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ImplicitStrictness> {
        Some(match s {
            "tolerant" => ImplicitStrictness::Tolerant,
            "strict" => ImplicitStrictness::Strict,
            _ => return None,
        })
    }
}

/// `affinity_key ∈ {supported{max_key_length}, unsupported}` — whether the
/// dialect honours a kernel-derived affinity key and its wire bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinitySupport {
    /// Supported with a wire-form length bound.
    Supported {
        /// `max_key_length` — the `wire` form never exceeds this.
        max_key_length: u64,
    },
    /// Unsupported — the key is omitted and the omission recorded.
    Unsupported,
}

/// `isolation ∈ {workspace, organization, unknown}` — the provider-side
/// isolation scope the dialect declares for prefix entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationScope {
    /// Workspace-scoped entries.
    Workspace,
    /// Organization-scoped entries.
    Organization,
    /// Undeclared.
    Unknown,
}

impl IsolationScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationScope::Workspace => "workspace",
            IsolationScope::Organization => "organization",
            IsolationScope::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<IsolationScope> {
        Some(match s {
            "workspace" => IsolationScope::Workspace,
            "organization" => IsolationScope::Organization,
            "unknown" => IsolationScope::Unknown,
            _ => return None,
        })
    }
}

/// `position_rule ∈ {block, run_of_same_kind}` — where a marker may land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerPositionRule {
    /// Any block boundary.
    Block,
    /// Only at the end of a run of same-kind blocks.
    RunOfSameKind,
}

impl MarkerPositionRule {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MarkerPositionRule::Block => "block",
            MarkerPositionRule::RunOfSameKind => "run_of_same_kind",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<MarkerPositionRule> {
        Some(match s {
            "block" => MarkerPositionRule::Block,
            "run_of_same_kind" => MarkerPositionRule::RunOfSameKind,
            _ => return None,
        })
    }
}

/// `CacheSemantics` — the closed kernel sum per dialect version (ADR-0127 d.2).
/// `unknown` is the safe default (no markers; state `unknown`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheSemantics {
    /// `explicit_breakpoints{max_markers, lookback_positions?, position_rule,
    /// retention_classes, min_cacheable_tokens?, isolation,
    /// eligible_carriers[]}` — markers attach to carrier blocks.
    ExplicitBreakpoints {
        /// `max_markers` — the per-request cap.
        max_markers: u32,
        /// `lookback_positions` — how far back a marker may reference.
        lookback_positions: Option<u32>,
        /// The position rule.
        position_rule: MarkerPositionRule,
        /// Declared retention classes.
        retention_classes: Vec<RetentionClass>,
        /// `min_cacheable_tokens` — below this the prefix is `uncacheable`.
        min_cacheable_tokens: Option<u64>,
        /// The provider-side isolation scope.
        isolation: IsolationScope,
        /// The carrier block kinds a marker may attach to (LC-9).
        eligible_carriers: Vec<crate::vocab::BlockKind>,
    },
    /// `implicit_prefix{strictness, affinity_key, retention_classes,
    /// min_cacheable_tokens?, reporting_granularity_tokens?}`.
    ImplicitPrefix {
        /// The prefix-match strictness.
        strictness: ImplicitStrictness,
        /// Whether the dialect honours an affinity key.
        affinity_key: AffinitySupport,
        /// Declared retention classes.
        retention_classes: Vec<RetentionClass>,
        /// `min_cacheable_tokens`.
        min_cacheable_tokens: Option<u64>,
        /// Usage reporting granularity.
        reporting_granularity_tokens: Option<u64>,
    },
    /// `explicit_resource{ttl_classes, storage_priced, reference_form}` (C1 —
    /// declared now, inadmissible for markers at C0).
    ExplicitResource {
        /// The TTL classes.
        ttl_classes: Vec<RetentionClass>,
        /// Whether storage is priced.
        storage_priced: bool,
        /// The reference form spelling.
        reference_form: String,
    },
    /// `uncached` — the provider keeps nothing.
    Uncached,
    /// `unknown` — the safe default (no markers; state `unknown`).
    Unknown,
}

impl CacheSemantics {
    /// The canonical kind spelling (the profile's `cache_control_convention`
    /// names this vocabulary — ADR-0127 d.2).
    pub fn kind(&self) -> &'static str {
        match self {
            CacheSemantics::ExplicitBreakpoints { .. } => "explicit_breakpoints",
            CacheSemantics::ImplicitPrefix { .. } => "implicit_prefix",
            CacheSemantics::ExplicitResource { .. } => "explicit_resource",
            CacheSemantics::Uncached => "uncached",
            CacheSemantics::Unknown => "unknown",
        }
    }

    /// The `min_cacheable_tokens` bound, if the semantics declares one.
    pub fn min_cacheable_tokens(&self) -> Option<u64> {
        match self {
            CacheSemantics::ExplicitBreakpoints {
                min_cacheable_tokens,
                ..
            }
            | CacheSemantics::ImplicitPrefix {
                min_cacheable_tokens,
                ..
            } => *min_cacheable_tokens,
            _ => None,
        }
    }

    /// The declared retention classes (empty for `uncached`/`unknown`/
    /// `explicit_resource` — the latter's `ttl_classes` are storage TTLs, not
    /// prefix retention).
    pub fn retention_classes(&self) -> &[RetentionClass] {
        match self {
            CacheSemantics::ExplicitBreakpoints {
                retention_classes, ..
            }
            | CacheSemantics::ImplicitPrefix {
                retention_classes, ..
            } => retention_classes,
            _ => &[],
        }
    }

    /// The affinity-key support declared by the semantics (`unsupported` for
    /// non-implicit kinds).
    pub fn affinity_support(&self) -> AffinitySupport {
        match self {
            CacheSemantics::ImplicitPrefix { affinity_key, .. } => *affinity_key,
            _ => AffinitySupport::Unsupported,
        }
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let retention = |rc: &[RetentionClass]| {
            Json::Arr(
                rc.iter()
                    .map(|r| {
                        Json::obj([
                            ("class_id", Json::str(r.class_id.clone())),
                            ("nominal_ms", Json::Int(r.nominal_ms as i64)),
                            ("guaranteed", Json::Bool(r.guaranteed)),
                        ])
                    })
                    .collect(),
            )
        };
        match self {
            CacheSemantics::ExplicitBreakpoints {
                max_markers,
                lookback_positions,
                position_rule,
                retention_classes,
                min_cacheable_tokens,
                isolation,
                eligible_carriers,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("kind".into(), Json::str("explicit_breakpoints"));
                m.insert("max_markers".into(), Json::Int(*max_markers as i64));
                if let Some(l) = lookback_positions {
                    m.insert("lookback_positions".into(), Json::Int(*l as i64));
                }
                m.insert("position_rule".into(), Json::str(position_rule.as_str()));
                m.insert("retention_classes".into(), retention(retention_classes));
                if let Some(t) = min_cacheable_tokens {
                    m.insert("min_cacheable_tokens".into(), Json::Int(*t as i64));
                }
                m.insert("isolation".into(), Json::str(isolation.as_str()));
                m.insert(
                    "eligible_carriers".into(),
                    Json::Arr(
                        eligible_carriers
                            .iter()
                            .map(|k| Json::str(k.as_str()))
                            .collect(),
                    ),
                );
                Json::Obj(m)
            }
            CacheSemantics::ImplicitPrefix {
                strictness,
                affinity_key,
                retention_classes,
                min_cacheable_tokens,
                reporting_granularity_tokens,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("kind".into(), Json::str("implicit_prefix"));
                m.insert("strictness".into(), Json::str(strictness.as_str()));
                m.insert(
                    "affinity_key".into(),
                    match affinity_key {
                        AffinitySupport::Supported { max_key_length } => Json::obj([(
                            "supported",
                            Json::obj([("max_key_length", Json::Int(*max_key_length as i64))]),
                        )]),
                        AffinitySupport::Unsupported => Json::str("unsupported"),
                    },
                );
                m.insert("retention_classes".into(), retention(retention_classes));
                if let Some(t) = min_cacheable_tokens {
                    m.insert("min_cacheable_tokens".into(), Json::Int(*t as i64));
                }
                if let Some(g) = reporting_granularity_tokens {
                    m.insert("reporting_granularity_tokens".into(), Json::Int(*g as i64));
                }
                Json::Obj(m)
            }
            CacheSemantics::ExplicitResource {
                ttl_classes,
                storage_priced,
                reference_form,
            } => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("kind".into(), Json::str("explicit_resource"));
                m.insert("ttl_classes".into(), retention(ttl_classes));
                m.insert("storage_priced".into(), Json::Bool(*storage_priced));
                m.insert("reference_form".into(), Json::str(reference_form.clone()));
                Json::Obj(m)
            }
            CacheSemantics::Uncached => Json::str("uncached"),
            CacheSemantics::Unknown => Json::str("unknown"),
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json, path: &str) -> Result<CacheSemantics, CodecError> {
        let bad = |m: &str| CodecError::TypeMismatch {
            member: format!("{path}.{m}"),
            expected: "cache semantics member",
        };
        let retention = |j: &Json, key: &str| -> Result<Vec<RetentionClass>, CodecError> {
            let mut out = Vec::new();
            if let Some(Json::Arr(items)) = j.get(key) {
                for it in items {
                    let class_id = it
                        .get("class_id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("class_id"))?
                        .to_string();
                    if !matches!(class_id.as_str(), "short" | "long" | "extended") {
                        return Err(CodecError::UnknownVariant {
                            member: format!("{path}.{key}.class_id"),
                            value: class_id,
                        });
                    }
                    out.push(RetentionClass {
                        class_id,
                        nominal_ms: it
                            .get("nominal_ms")
                            .and_then(Json::as_int)
                            .ok_or_else(|| bad("nominal_ms"))?
                            as u64,
                        guaranteed: matches!(it.get("guaranteed"), Some(Json::Bool(true))),
                    });
                }
            }
            Ok(out)
        };
        match j {
            Json::Str(s) if s == "uncached" => Ok(CacheSemantics::Uncached),
            Json::Str(s) if s == "unknown" => Ok(CacheSemantics::Unknown),
            Json::Obj(_) => {
                let kind = j
                    .get("kind")
                    .and_then(Json::as_str)
                    .ok_or_else(|| bad("kind"))?;
                match kind {
                    "explicit_breakpoints" => {
                        let carriers = match j.get("eligible_carriers") {
                            Some(Json::Arr(items)) => items
                                .iter()
                                .map(|i| {
                                    crate::vocab::BlockKind::parse(i.as_str().unwrap_or(""))
                                        .ok_or_else(|| CodecError::UnknownVariant {
                                            member: format!("{path}.eligible_carriers"),
                                            value: i.as_str().unwrap_or("").to_string(),
                                        })
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                            _ => Vec::new(),
                        };
                        Ok(CacheSemantics::ExplicitBreakpoints {
                            max_markers: j
                                .get("max_markers")
                                .and_then(Json::as_int)
                                .ok_or_else(|| bad("max_markers"))?
                                as u32,
                            lookback_positions: j
                                .get("lookback_positions")
                                .and_then(Json::as_int)
                                .map(|v| v as u32),
                            position_rule: MarkerPositionRule::parse(
                                j.get("position_rule")
                                    .and_then(Json::as_str)
                                    .unwrap_or("block"),
                            )
                            .ok_or_else(|| {
                                CodecError::UnknownVariant {
                                    member: format!("{path}.position_rule"),
                                    value: String::new(),
                                }
                            })?,
                            retention_classes: retention(j, "retention_classes")?,
                            min_cacheable_tokens: j
                                .get("min_cacheable_tokens")
                                .and_then(Json::as_int)
                                .map(|v| v as u64),
                            isolation: IsolationScope::parse(
                                j.get("isolation")
                                    .and_then(Json::as_str)
                                    .unwrap_or("unknown"),
                            )
                            .unwrap_or(IsolationScope::Unknown),
                            eligible_carriers: carriers,
                        })
                    }
                    "implicit_prefix" => Ok(CacheSemantics::ImplicitPrefix {
                        strictness: ImplicitStrictness::parse(
                            j.get("strictness")
                                .and_then(Json::as_str)
                                .unwrap_or("tolerant"),
                        )
                        .ok_or_else(|| CodecError::UnknownVariant {
                            member: format!("{path}.strictness"),
                            value: String::new(),
                        })?,
                        affinity_key: match j.get("affinity_key") {
                            Some(Json::Str(s)) if s == "unsupported" => {
                                AffinitySupport::Unsupported
                            }
                            Some(v @ Json::Obj(_)) => {
                                if v.get("supported").is_some() {
                                    AffinitySupport::Supported {
                                        max_key_length: v
                                            .get("supported")
                                            .and_then(|s| s.get("max_key_length"))
                                            .and_then(Json::as_int)
                                            .ok_or_else(|| bad("affinity_key.max_key_length"))?
                                            as u64,
                                    }
                                } else {
                                    return Err(CodecError::UnknownVariant {
                                        member: format!("{path}.affinity_key"),
                                        value: String::new(),
                                    });
                                }
                            }
                            _ => AffinitySupport::Unsupported,
                        },
                        retention_classes: retention(j, "retention_classes")?,
                        min_cacheable_tokens: j
                            .get("min_cacheable_tokens")
                            .and_then(Json::as_int)
                            .map(|v| v as u64),
                        reporting_granularity_tokens: j
                            .get("reporting_granularity_tokens")
                            .and_then(Json::as_int)
                            .map(|v| v as u64),
                    }),
                    "explicit_resource" => Ok(CacheSemantics::ExplicitResource {
                        ttl_classes: retention(j, "ttl_classes")?,
                        storage_priced: matches!(j.get("storage_priced"), Some(Json::Bool(true))),
                        reference_form: j
                            .get("reference_form")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    }),
                    other => Err(CodecError::UnknownVariant {
                        member: format!("{path}.kind"),
                        value: other.to_string(),
                    }),
                }
            }
            _ => Err(bad("kind")),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The affinity key (ADR-0128 d.1)
// ─────────────────────────────────────────────────────────────────────────────

/// `AffinityKey{full: ContentAddress, wire: prefix ≤ max_key_length, scope,
/// purpose}` — `full` is the ledger fact; `wire` is a surface alias recorded in
/// `surface_ids.affinity_key` (T-LCD-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffinityKey {
    /// `full` — the content-derived key (`idp/1` id over `scope ∥ static_hash ∥
    /// purpose`).
    pub full: String,
    /// `wire` — the surface alias, a `full` prefix bounded by the dialect's
    /// `max_key_length`.
    pub wire: String,
    /// The scope coordinate (run or root run id — never a raw session id,
    /// credential, principal identifier or `readers`-restricted content, CK-5).
    pub scope: String,
    /// The purpose.
    pub purpose: Purpose,
}

/// `derive_affinity_key(scope, static_hash, purpose, support)` — CK-1 a pure
/// function of `(scope, static_hash, purpose)`; CK-2 `purpose ≠ main` never
/// equals a `main` key (the purpose spelling is in the digest); CK-5 the key is
/// a content address over kernel coordinates — never raw session/credential/
/// principal material. `AffinityUnsupported` is the caller's refusal (the key
/// is omitted and recorded).
pub fn derive_affinity_key(
    scope: &str,
    static_hash: &str,
    purpose: &Purpose,
    support: AffinitySupport,
) -> Result<AffinityKey, CodecError> {
    let material = format!("{scope}\u{1f}{static_hash}\u{1f}{}", purpose.as_str());
    let full = hh_identity::idp_id("cache.affinity.1", material.as_bytes());
    let wire = match support {
        AffinitySupport::Supported { max_key_length } => {
            // The wire form is a `full` prefix at the declared bound — a
            // surface alias (CK-4), never a second derivation.
            let cap = (max_key_length as usize).min(full.len());
            full[..cap].to_string()
        }
        AffinitySupport::Unsupported => {
            return Err(CodecError::TypeMismatch {
                member: "affinity_key".into(),
                expected: "dialect affinity_key = supported{max_key_length}",
            });
        }
    };
    Ok(AffinityKey {
        full,
        wire,
        scope: scope.to_string(),
        purpose: purpose.clone(),
    })
}

/// `cold_start` salts the scope with `configuration_version_id` and the
/// replicate index (ADR-0128 d.1 — every run pays writes; no prior run's prefix
/// is hit). The salt is computed **here once** — the scope string handed to
/// [`derive_affinity_key`] is already salted (one scheme, CC1).
pub fn cold_start_scope(scope: &str, configuration_version_id: &str, replicate: u64) -> String {
    format!("{scope}\u{1f}cold\u{1f}{configuration_version_id}\u{1f}r{replicate}")
}

// ─────────────────────────────────────────────────────────────────────────────
// Expected / observed state (ADR-0128 d.2)
// ─────────────────────────────────────────────────────────────────────────────

/// `miss_reason` — the closed vocabulary (ADR-0128 d.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MissReason {
    /// The retention window elapsed.
    TtlElapsed,
    /// A sibling call opened before the prior call's first byte.
    ConcurrentSibling,
    /// The provider evicted the prefix.
    ProviderEviction,
    /// A `prefix_affecting` parameter changed mid-run.
    ParameterChange,
    /// A compaction invalidated the prefix.
    Compaction,
    /// A context edit invalidated the prefix.
    ContextEdit,
    /// A relower invalidated the prefix.
    Relower,
    /// The profile version changed.
    ProfileVersion,
    /// The definition version changed.
    DefinitionVersion,
    /// The tool set changed.
    ToolSetChange,
    /// The marker policy changed.
    MarkerPolicy,
    /// The dialect descriptor changed.
    DialectChange,
    /// Unknown — never coerced into another reason.
    Unknown,
}

impl MissReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MissReason::TtlElapsed => "ttl_elapsed",
            MissReason::ConcurrentSibling => "concurrent_sibling",
            MissReason::ProviderEviction => "provider_eviction",
            MissReason::ParameterChange => "parameter_change",
            MissReason::Compaction => "compaction",
            MissReason::ContextEdit => "context_edit",
            MissReason::Relower => "relower",
            MissReason::ProfileVersion => "profile_version",
            MissReason::DefinitionVersion => "definition_version",
            MissReason::ToolSetChange => "tool_set_change",
            MissReason::MarkerPolicy => "marker_policy",
            MissReason::DialectChange => "dialect_change",
            MissReason::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<MissReason> {
        Some(match s {
            "ttl_elapsed" => MissReason::TtlElapsed,
            "concurrent_sibling" => MissReason::ConcurrentSibling,
            "provider_eviction" => MissReason::ProviderEviction,
            "parameter_change" => MissReason::ParameterChange,
            "compaction" => MissReason::Compaction,
            "context_edit" => MissReason::ContextEdit,
            "relower" => MissReason::Relower,
            "profile_version" => MissReason::ProfileVersion,
            "definition_version" => MissReason::DefinitionVersion,
            "tool_set_change" => MissReason::ToolSetChange,
            "marker_policy" => MissReason::MarkerPolicy,
            "dialect_change" => MissReason::DialectChange,
            "unknown" => MissReason::Unknown,
            _ => return None,
        })
    }
}

/// `expected ∈ {warm, cold, uncacheable, unknown}` (ADR-0128 d.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedState {
    /// A prior completed same-`(key, static_hash)` call is within retention.
    Warm,
    /// A miss is expected (`cold{reason}` — e.g. `concurrent_sibling`).
    Cold,
    /// Below `min_cacheable_tokens` — not a cacheable call.
    Uncacheable,
    /// Semantics or retention undeclared — `unknown`, never coerced.
    Unknown,
}

impl ExpectedState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ExpectedState::Warm => "warm",
            ExpectedState::Cold => "cold",
            ExpectedState::Uncacheable => "uncacheable",
            ExpectedState::Unknown => "unknown",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ExpectedState> {
        Some(match s {
            "warm" => ExpectedState::Warm,
            "cold" => ExpectedState::Cold,
            "uncacheable" => ExpectedState::Uncacheable,
            "unknown" => ExpectedState::Unknown,
            _ => return None,
        })
    }
}

/// `observed ∈ {warm, cold, unknown}` — what the usage roles reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedState {
    /// `cache_read > 0`.
    Warm,
    /// `cache_read = 0` with usage available.
    Cold,
    /// `usage.available = false`.
    Unknown,
}

impl ObservedState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservedState::Warm => "warm",
            ObservedState::Cold => "cold",
            ObservedState::Unknown => "unknown",
        }
    }
}

/// `expect_cache_state`'s basis record — `{last_call?, retention_class,
/// elapsed_ms, margin_ms}` (ADR-0128 d.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedBasis {
    /// The prior call's id, when one exists.
    pub last_call: Option<String>,
    /// The retention class consulted.
    pub retention_class: Option<String>,
    /// Milliseconds since the prior call's `request_sent`.
    pub elapsed_ms: Option<u64>,
    /// The safety margin applied.
    pub margin_ms: u64,
    /// The `cold{…}` reason, when `expected = cold`.
    pub cold_reason: Option<MissReason>,
}

/// The `expect_cache_state` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedCacheState {
    /// `expected`.
    pub expected: ExpectedState,
    /// `basis`.
    pub basis: ExpectedBasis,
}

/// One prior call fact — the durable-prefix projection input (the Stage-2
/// ledger fold supplies these; the function itself is pure over them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheCallFact {
    /// The call's affinity key (`full`).
    pub affinity_key: String,
    /// The call's `static_hash`.
    pub static_hash: String,
    /// `request_sent` — when the attempt's first byte went out (ms).
    pub request_sent_ms: u64,
    /// Whether the call reached `completed`.
    pub completed: bool,
    /// The call's `model_call_id` (for `basis.last_call`).
    pub model_call_id: String,
}

/// `expect_cache_state(run-facts, key, static_hash, request_sent_at, est_tokens,
/// semantics, margin_ms)` — a pure projection (CK-3): `warm` iff a prior
/// completed same-`(key, static_hash)` call exists whose
/// `request_sent + nominal_ms − margin_ms > now` (the lease rule — the earlier
/// clock wins; TTL runs from request start); `cold{concurrent_sibling}` if a
/// same-key call opened but has not completed; `uncacheable` below
/// `min_cacheable_tokens`; `unknown` when semantics/retention are undeclared.
pub fn expect_cache_state(
    facts: &[CacheCallFact],
    key: &str,
    static_hash: &str,
    now_ms: u64,
    est_tokens: Option<u64>,
    semantics: &CacheSemantics,
    margin_ms: u64,
) -> ExpectedCacheState {
    let unknown = |basis: ExpectedBasis| ExpectedCacheState {
        expected: ExpectedState::Unknown,
        basis,
    };
    let empty_basis = ExpectedBasis {
        last_call: None,
        retention_class: None,
        elapsed_ms: None,
        margin_ms,
        cold_reason: None,
    };
    // `uncacheable` below `min_cacheable_tokens` (estimate_tokens-derived).
    if let (Some(min), Some(est)) = (semantics.min_cacheable_tokens(), est_tokens) {
        if est < min {
            return ExpectedCacheState {
                expected: ExpectedState::Uncacheable,
                basis: empty_basis.clone(),
            };
        }
    }
    // `unknown` when semantics or retention are undeclared.
    let retention = semantics.retention_classes();
    if matches!(
        semantics,
        CacheSemantics::Uncached | CacheSemantics::Unknown
    ) || retention.is_empty()
    {
        return unknown(empty_basis);
    }
    // The conservative retention bound: the shortest nominal class.
    let nominal_ms = retention.iter().map(|r| r.nominal_ms).min().unwrap_or(0);
    let nominal_class = retention
        .iter()
        .min_by_key(|r| r.nominal_ms)
        .map(|r| r.class_id.clone());
    // The most recent same-(key, static_hash) call fact.
    let same: Vec<&CacheCallFact> = facts
        .iter()
        .filter(|f| f.affinity_key == key && f.static_hash == static_hash)
        .collect();
    // A same-key sibling still in flight ⇒ `cold{concurrent_sibling}`.
    if let Some(open) = same.iter().find(|f| !f.completed) {
        return ExpectedCacheState {
            expected: ExpectedState::Cold,
            basis: ExpectedBasis {
                last_call: Some(open.model_call_id.clone()),
                retention_class: nominal_class.clone(),
                elapsed_ms: None,
                margin_ms,
                cold_reason: Some(MissReason::ConcurrentSibling),
            },
        };
    }
    match same
        .iter()
        .filter(|f| f.completed)
        .max_by_key(|f| f.request_sent_ms)
    {
        None => ExpectedCacheState {
            expected: ExpectedState::Cold,
            basis: ExpectedBasis {
                cold_reason: Some(MissReason::Unknown),
                ..empty_basis
            },
        },
        Some(last) => {
            let elapsed = now_ms.saturating_sub(last.request_sent_ms);
            let warm_until = last.request_sent_ms + nominal_ms.saturating_sub(margin_ms);
            if now_ms < warm_until {
                ExpectedCacheState {
                    expected: ExpectedState::Warm,
                    basis: ExpectedBasis {
                        last_call: Some(last.model_call_id.clone()),
                        retention_class: nominal_class,
                        elapsed_ms: Some(elapsed),
                        margin_ms,
                        cold_reason: None,
                    },
                }
            } else {
                ExpectedCacheState {
                    expected: ExpectedState::Cold,
                    basis: ExpectedBasis {
                        last_call: Some(last.model_call_id.clone()),
                        retention_class: nominal_class,
                        elapsed_ms: Some(elapsed),
                        margin_ms,
                        cold_reason: Some(MissReason::TtlElapsed),
                    },
                }
            }
        }
    }
}

/// `CacheObservation{observed, read, write_by_class, hit_ratio, write_ratio}`
/// — the gateway stamps `observed` (`warm` iff `cache_read > 0`; `unknown` iff
/// `usage.available = false`); `agreement`/`miss_reason` are a projection view
/// (ADR-0128 d.2 — raw roles stay on the producing event).
#[derive(Debug, Clone, PartialEq)]
pub struct CacheObservation {
    /// `observed`.
    pub observed: ObservedState,
    /// `cache_read` tokens.
    pub read: i64,
    /// `write_by_class` — cache-write tokens per retention class.
    pub write_by_class: std::collections::BTreeMap<String, i64>,
    /// `hit_ratio` in ppm (`read / input.total`; `None` when the denominator
    /// is 0 — `n/a`, never 0).
    pub hit_ratio_ppm: Option<i64>,
    /// `write_ratio` in ppm (`cache_write / input.total`).
    pub write_ratio_ppm: Option<i64>,
}

/// `observe_cache(usage, expected)` — the gateway-side stamp. `usage = None`
/// (unavailable) ⇒ `observed = unknown`.
pub fn observe_cache(usage: Option<&hh_telemetry::TokenVector>) -> CacheObservation {
    match usage {
        None => CacheObservation {
            observed: ObservedState::Unknown,
            read: 0,
            write_by_class: Default::default(),
            hit_ratio_ppm: None,
            write_ratio_ppm: None,
        },
        Some(v) => {
            let mut write_by_class = std::collections::BTreeMap::new();
            if v.cache_write > 0 {
                write_by_class.insert("default".to_string(), v.cache_write);
            }
            CacheObservation {
                observed: if v.cache_read > 0 {
                    ObservedState::Warm
                } else {
                    ObservedState::Cold
                },
                read: v.cache_read,
                write_by_class,
                hit_ratio_ppm: v.cache_hit_rate_ppm(),
                write_ratio_ppm: if v.input_total == 0 {
                    None
                } else {
                    Some(v.cache_write * hh_budget::quantity::PPM_SCALE / v.input_total)
                },
            }
        }
    }
}

/// Whether `expected` and `observed` agree (the projection view's `agreement`;
/// `unknown` never coerces — an `unknown` either side agrees only with
/// `unknown`).
pub fn cache_agreement(expected: ExpectedState, observed: ObservedState) -> bool {
    matches!(
        (expected, observed),
        (ExpectedState::Warm, ObservedState::Warm)
            | (ExpectedState::Cold, ObservedState::Cold)
            | (ExpectedState::Unknown, ObservedState::Unknown)
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Marker placement (ADR-0127 d.5)
// ─────────────────────────────────────────────────────────────────────────────

/// `MarkerPosition` — where a `caching_markers` rule asks for a breakpoint
/// (ADR-0127 d.4: `tier_boundary(static) | tier_boundary(dynamic) |
/// transcript_tail{n_recent_user_turns | last_cacheable} | block_index(i)`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkerPosition {
    /// At the end of the named tier.
    TierBoundary(CacheTier),
    /// `transcript_tail{last_cacheable}` — the last cacheable transcript block.
    TranscriptTailLastCacheable,
    /// `transcript_tail{n_recent_user_turns}` — n user turns back.
    TranscriptTailUserTurns(u32),
    /// An explicit block index.
    BlockIndex(u32),
}

impl MarkerPosition {
    /// The canonical spelling.
    pub fn as_str(&self) -> String {
        match self {
            MarkerPosition::TierBoundary(t) => format!("tier_boundary({})", t.as_str()),
            MarkerPosition::TranscriptTailLastCacheable => {
                "transcript_tail(last_cacheable)".to_string()
            }
            MarkerPosition::TranscriptTailUserTurns(n) => {
                format!("transcript_tail(n_recent_user_turns={n})")
            }
            MarkerPosition::BlockIndex(i) => format!("block_index({i})"),
        }
    }
}

/// One carrier block the plan exposes — the facts `place_markers` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierBlock {
    /// The block's position in the rendered request.
    pub index: u32,
    /// The tier it renders in.
    pub tier: CacheTier,
    /// The block kind (the carrier-eligibility check reads it, LC-9).
    pub kind: crate::vocab::BlockKind,
    /// The message role, when the block is a message block (the
    /// `n_recent_user_turns` anchor counts `user` roles).
    pub role: Option<String>,
}

/// A dropped marker — `{position, reason ∈ {cap, ineligible_carrier,
/// below_minimum}}` (ADR-0127 d.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedMarker {
    /// The requested position.
    pub position: MarkerPosition,
    /// The closed drop reason.
    pub reason: DropReason,
}

/// The closed drop-reason vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The `max_markers` cap (invalidation priority).
    Cap,
    /// No eligible carrier exists for the position (LC-9).
    IneligibleCarrier,
    /// Below `min_cacheable_tokens`.
    BelowMinimum,
}

impl DropReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DropReason::Cap => "cap",
            DropReason::IneligibleCarrier => "ineligible_carrier",
            DropReason::BelowMinimum => "below_minimum",
        }
    }
}

/// A substituted carrier — the marker's requested position was ineligible and
/// the nearest eligible earlier carrier took it (recorded, never silent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerSubstitution {
    /// The requested position.
    pub requested: MarkerPosition,
    /// The carrier index the marker landed on.
    pub substituted_index: u32,
}

/// A placed marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedMarker {
    /// The requested position.
    pub requested: MarkerPosition,
    /// The carrier block index the marker attaches to.
    pub carrier_index: u32,
    /// The retention class requested for this marker.
    pub retention_class: Option<String>,
}

/// `place_markers`'s result.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerPlacement {
    /// The markers to emit (≤ `max_markers`).
    pub markers: Vec<PlacedMarker>,
    /// The dropped markers (in invalidation-priority order).
    pub dropped: Vec<DroppedMarker>,
    /// The carrier substitutions.
    pub substituted: Vec<MarkerSubstitution>,
}

/// The invalidation priority of a position — `static → dynamic →
/// transcript tail` (static is most valuable; the tail drops first).
fn position_priority(p: &MarkerPosition) -> u8 {
    match p {
        MarkerPosition::TierBoundary(CacheTier::Static) => 0,
        MarkerPosition::TierBoundary(CacheTier::Dynamic) => 1,
        MarkerPosition::TierBoundary(CacheTier::Transcript)
        | MarkerPosition::TranscriptTailLastCacheable
        | MarkerPosition::TranscriptTailUserTurns(_) => 2,
        MarkerPosition::BlockIndex(_) => 3,
    }
}

/// `place_markers(positions, carriers, semantics, est_tokens)` — pure
/// (ADR-0127 d.5): the cap is enforced by invalidation priority; an ineligible
/// carrier substitutes the nearest eligible *earlier* carrier (recorded);
/// nothing is dropped silently.
pub fn place_markers(
    positions: &[MarkerPosition],
    carriers: &[CarrierBlock],
    semantics: &CacheSemantics,
    est_tokens: Option<u64>,
) -> MarkerPlacement {
    let empty = MarkerPlacement {
        markers: Vec::new(),
        dropped: Vec::new(),
        substituted: Vec::new(),
    };
    // Markers emit only under `explicit_breakpoints` (ADR-0127 d.5).
    let (max_markers, eligible, min_tokens, position_rule) = match semantics {
        CacheSemantics::ExplicitBreakpoints {
            max_markers,
            position_rule,
            min_cacheable_tokens,
            eligible_carriers,
            ..
        } => (
            *max_markers,
            eligible_carriers.clone(),
            *min_cacheable_tokens,
            *position_rule,
        ),
        _ => return empty,
    };
    // A prefix below `min_cacheable_tokens` drops every marker
    // (`below_minimum`; `expected_state = uncacheable` is the caller's record).
    if let (Some(min), Some(est)) = (min_tokens, est_tokens) {
        if est < min {
            return MarkerPlacement {
                dropped: positions
                    .iter()
                    .map(|p| DroppedMarker {
                        position: p.clone(),
                        reason: DropReason::BelowMinimum,
                    })
                    .collect(),
                ..empty
            };
        }
    }
    // Resolve each requested position to a carrier index.
    let resolve = |pos: &MarkerPosition| -> Option<u32> {
        match pos {
            MarkerPosition::TierBoundary(tier) => carriers
                .iter()
                .filter(|c| c.tier == *tier)
                .map(|c| c.index)
                .max(),
            MarkerPosition::TranscriptTailLastCacheable => carriers
                .iter()
                .filter(|c| c.tier == CacheTier::Transcript)
                .map(|c| c.index)
                .max(),
            MarkerPosition::TranscriptTailUserTurns(n) => {
                let users: Vec<u32> = carriers
                    .iter()
                    .filter(|c| c.tier == CacheTier::Transcript)
                    .filter(|c| c.role.as_deref() == Some("user"))
                    .map(|c| c.index)
                    .collect();
                users
                    .iter()
                    .rev()
                    .nth(n.saturating_sub(1) as usize)
                    .copied()
            }
            MarkerPosition::BlockIndex(i) => Some(*i),
        }
    };
    // Cap by invalidation priority — keep the first `max_markers` positions in
    // (priority, requested-order); the rest drop `cap` in that order.
    let mut ordered: Vec<(usize, &MarkerPosition)> = positions.iter().enumerate().collect();
    ordered.sort_by_key(|(i, p)| (position_priority(p), *i));
    let kept: Vec<&(usize, &MarkerPosition)> = ordered.iter().take(max_markers as usize).collect();
    let dropped: Vec<DroppedMarker> = ordered
        .iter()
        .skip(max_markers as usize)
        .map(|(_, p)| DroppedMarker {
            position: (*p).clone(),
            reason: DropReason::Cap,
        })
        .collect();
    let mut markers = Vec::new();
    let mut substituted = Vec::new();
    let mut extra_dropped = Vec::new();
    for (_, pos) in kept {
        let Some(target) = resolve(pos) else {
            extra_dropped.push(DroppedMarker {
                position: (*pos).clone(),
                reason: DropReason::IneligibleCarrier,
            });
            continue;
        };
        // The carrier at/earlier-than the target that is eligible (LC-9;
        // `run_of_same_kind` additionally requires the carrier's successor to
        // differ in kind or be absent — the run end).
        let is_eligible = |c: &CarrierBlock| -> bool {
            if !eligible.contains(&c.kind) {
                return false;
            }
            if position_rule == MarkerPositionRule::RunOfSameKind {
                if let Some(next) = carriers.iter().find(|o| o.index == c.index + 1) {
                    return next.kind != c.kind;
                }
            }
            true
        };
        match carriers
            .iter()
            .filter(|c| c.index <= target)
            .rev()
            .find(|c| is_eligible(c))
        {
            Some(c) => {
                if c.index != target {
                    substituted.push(MarkerSubstitution {
                        requested: (*pos).clone(),
                        substituted_index: c.index,
                    });
                }
                markers.push(PlacedMarker {
                    requested: (*pos).clone(),
                    carrier_index: c.index,
                    retention_class: None,
                });
            }
            None => extra_dropped.push(DroppedMarker {
                position: (*pos).clone(),
                reason: DropReason::IneligibleCarrier,
            }),
        }
    }
    let mut dropped_all = dropped;
    dropped_all.extend(extra_dropped);
    MarkerPlacement {
        markers,
        dropped: dropped_all,
        substituted,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `model.cache.resolved` vocabulary (ADR-0128 d.3)
// ─────────────────────────────────────────────────────────────────────────────

/// `cache_kind ∈ {compile, catalog, tool_result, response, semantic}` — the
/// K2–K6 kinds a `model.cache.resolved` reports (K1 has no such event — its hit
/// is observed from usage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheKind {
    /// K2 — compile cache.
    Compile,
    /// K3 — catalog cache.
    Catalog,
    /// K4 — tool-result/retrieval cache.
    ToolResult,
    /// K5 — exact-match response cache.
    Response,
    /// K6 — similarity-keyed semantic cache (C2).
    Semantic,
}

impl CacheKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheKind::Compile => "compile",
            CacheKind::Catalog => "catalog",
            CacheKind::ToolResult => "tool_result",
            CacheKind::Response => "response",
            CacheKind::Semantic => "semantic",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CacheKind> {
        Some(match s {
            "compile" => CacheKind::Compile,
            "catalog" => CacheKind::Catalog,
            "tool_result" => CacheKind::ToolResult,
            "response" => CacheKind::Response,
            "semantic" => CacheKind::Semantic,
            _ => return None,
        })
    }
}

/// `outcome ∈ {hit, miss, bypass, stale_withheld, refused}` (ADR-0128 d.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    /// Served.
    Hit,
    /// Not served.
    Miss,
    /// Policy bypassed the cache.
    Bypass,
    /// An entry existed but was withheld (stale stamp).
    StaleWithheld,
    /// The lookup was refused.
    Refused,
}

impl CacheOutcome {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheOutcome::Hit => "hit",
            CacheOutcome::Miss => "miss",
            CacheOutcome::Bypass => "bypass",
            CacheOutcome::StaleWithheld => "stale_withheld",
            CacheOutcome::Refused => "refused",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<CacheOutcome> {
        Some(match s {
            "hit" => CacheOutcome::Hit,
            "miss" => CacheOutcome::Miss,
            "bypass" => CacheOutcome::Bypass,
            "stale_withheld" => CacheOutcome::StaleWithheld,
            "refused" => CacheOutcome::Refused,
            _ => return None,
        })
    }
}
