//! `SinkPolicy` and the L0–L3 instrumentation/content classes (§5h.1 §3;
//! ADR-0044 D1/D2/D3). The policy is MUST-data — a content-free record whose
//! `version_id` comes from `hh-identity`'s `idp/1` (CC1); the closed codec
//! refuses unknown members with `BadMember`.
//!
//! The content classes are *what a sink may receive*:
//! - **L0 `accounting`** — counts, `TokenVector`, `Money`, durations, bucketed
//!   magnitudes, error classes, ids — never free text, never content bytes.
//! - **L1 `structural`** — every `durability = ledger` event (the event rows).
//! - **L2 `content`** — model requests/responses, tool arguments/observations,
//!   context items, judged evidence — only as the capability declaration and
//!   consent permit; `requires_consent = true` is mandatory (AC-R-2.9.1-8).
//! - **L3 `diagnostic`** — streaming deltas, permission *requests*, previews,
//!   process stats — ephemeral rows; L3 never carries content.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::identify_bytes;
use hh_identity::RecordKind;
use hh_wire::json::Json;

use crate::codec::{arr_at, bool_at, expect_obj, int_at, reject_unknown, str_at};
use crate::errors::{CodecError, TelemetryError};

const RECORD: &str = "SinkPolicy";

/// The L0–L3 content class a sink declares (§5h.1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentClass {
    /// L0 — counts, vectors, `Money`, durations, bucketed magnitudes, error
    /// classes, ids. Never free text, never content bytes.
    Accounting,
    /// L1 — the event rows themselves (`durability = ledger`).
    Structural,
    /// L2 — content (requests/responses, arguments/observations, context items,
    /// judged evidence) behind `ContentAddress`es.
    Content,
    /// L3 — ephemeral diagnostics (stream deltas, permission requests,
    /// previews, process stats). Never carries content.
    Diagnostic,
}

impl ContentClass {
    /// The four classes in level order.
    pub const ALL: [ContentClass; 4] = [
        ContentClass::Accounting,
        ContentClass::Structural,
        ContentClass::Content,
        ContentClass::Diagnostic,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContentClass::Accounting => "accounting",
            ContentClass::Structural => "structural",
            ContentClass::Content => "content",
            ContentClass::Diagnostic => "diagnostic",
        }
    }

    /// The level (L0–L3).
    pub fn level(self) -> u8 {
        match self {
            ContentClass::Accounting => 0,
            ContentClass::Structural => 1,
            ContentClass::Content => 2,
            ContentClass::Diagnostic => 3,
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Result<ContentClass, TelemetryError> {
        ContentClass::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| TelemetryError::UnknownContentClass {
                spelling: s.to_string(),
            })
    }
}

/// `redaction ∈ {none, allowlist, pseudonymize}` — governs content classes and
/// personal data only; secret redaction is upstream of every sink and
/// unconditional (ADR-0057; CF-133).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redaction {
    /// No redaction.
    None,
    /// An allowlist keeps only the named fields.
    Allowlist,
    /// Stable pseudonyms replace identifiers.
    Pseudonymize,
}

impl Redaction {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Redaction::None => "none",
            Redaction::Allowlist => "allowlist",
            Redaction::Pseudonymize => "pseudonymize",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Result<Redaction, TelemetryError> {
        Ok(match s {
            "none" => Redaction::None,
            "allowlist" => Redaction::Allowlist,
            "pseudonymize" => Redaction::Pseudonymize,
            _ => {
                return Err(TelemetryError::UnknownSinkMember {
                    detail: format!("redaction {s}"),
                })
            }
        })
    }
}

/// `sampling{mode ∈ {all, ratio, parent_based}, ratio?}` — applies to the
/// *exported* rows only; the ledger is never sampled and a `metric_view`
/// export refuses any mode below `all` (ADR-0044 D3; AC-R-2.9.1-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingMode {
    /// Every row is exported.
    All,
    /// A deterministic `ratio` share (hash-of-row-id < threshold).
    Ratio,
    /// The trace-root's decision governs every span of the trace.
    ParentBased,
}

impl SamplingMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SamplingMode::All => "all",
            SamplingMode::Ratio => "ratio",
            SamplingMode::ParentBased => "parent_based",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Result<SamplingMode, TelemetryError> {
        Ok(match s {
            "all" => SamplingMode::All,
            "ratio" => SamplingMode::Ratio,
            "parent_based" => SamplingMode::ParentBased,
            _ => {
                return Err(TelemetryError::UnknownSinkMember {
                    detail: format!("sampling.mode {s}"),
                })
            }
        })
    }
}

/// `sampling{mode, ratio_ppm?}` — `ratio` is an integer ppm of 1.0 (the
/// canonical form carries no floats; `1_000_000` = keep everything).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sampling {
    /// The mode.
    pub mode: SamplingMode,
    /// The keep ratio in ppm — required for `ratio`/`parent_based`, absent for
    /// `all`.
    pub ratio_ppm: Option<i64>,
}

impl Sampling {
    /// `sampling{mode = all}` — the default.
    pub const ALL: Sampling = Sampling {
        mode: SamplingMode::All,
        ratio_ppm: None,
    };

    /// Check the shape: `ratio_ppm` present iff the mode needs it, in
    /// `(0, 1_000_000]`.
    pub fn validate(&self) -> Result<(), TelemetryError> {
        let bad = |detail: String| TelemetryError::UnknownSinkMember { detail };
        match self.mode {
            SamplingMode::All => {
                if self.ratio_ppm.is_some() {
                    return Err(bad("sampling.ratio present under mode=all".into()));
                }
            }
            SamplingMode::Ratio | SamplingMode::ParentBased => match self.ratio_ppm {
                Some(r) if r > 0 && r <= hh_budget::quantity::PPM_SCALE => {}
                _ => {
                    return Err(bad(format!(
                        "sampling.ratio must be in (0, {}] ppm for mode={}",
                        hh_budget::quantity::PPM_SCALE,
                        self.mode.as_str()
                    )))
                }
            },
        }
        Ok(())
    }
}

/// `rate_limit?` — `{events_per_sec}` (the Stage-1 shape; the full rate grammar
/// lands with the sink bindings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    /// Maximum exported rows per second.
    pub events_per_sec: u64,
}

/// `SinkPolicy{sink_id, content_classes ⊆ {accounting, structural, content,
/// diagnostic}, redaction, sampling{mode, ratio?}, rate_limit?,
/// max_field_bytes, requires_consent}` (§5h.1 §3 — verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct SinkPolicy {
    /// The sink's name (`sink_id` is the comparison coordinate — version-only).
    pub sink_id: String,
    /// The content classes the sink may receive (L0–L3).
    pub content_classes: BTreeSet<ContentClass>,
    /// The redaction mode for content/personal data.
    pub redaction: Redaction,
    /// The export sampling rule (never the ledger's).
    pub sampling: Sampling,
    /// The rate limit.
    pub rate_limit: Option<RateLimit>,
    /// Per-field byte cap — longer fields truncate with a loss entry.
    pub max_field_bytes: u64,
    /// Whether the sink's declaration must be matched by manifest consent.
    /// Mandatory `true` for any `content`-declaring sink (AC-R-2.9.1-8).
    pub requires_consent: bool,
}

impl SinkPolicy {
    /// A minimal `{accounting}` sink — the always-on default shape.
    pub fn accounting(sink_id: impl Into<String>) -> SinkPolicy {
        SinkPolicy {
            sink_id: sink_id.into(),
            content_classes: [ContentClass::Accounting].into_iter().collect(),
            redaction: Redaction::None,
            sampling: Sampling::ALL,
            rate_limit: None,
            max_field_bytes: 4096,
            requires_consent: false,
        }
    }

    /// The declared checks (§5h.1 §3 + AC-R-2.9.1-8's schema half):
    /// `{content}` ⇒ `requires_consent = true`; the sampling shape is coherent;
    /// `content_classes` is non-empty; L3 `diagnostic` may not combine with
    /// `content` (diagnostic never carries content — a `{content, diagnostic}`
    /// sink would let L3 rows smuggle L2 fields).
    pub fn validate(&self) -> Result<(), TelemetryError> {
        if self.content_classes.is_empty() {
            return Err(TelemetryError::UnknownSinkMember {
                detail: "content_classes must be non-empty".into(),
            });
        }
        if self.content_classes.contains(&ContentClass::Content) && !self.requires_consent {
            return Err(TelemetryError::ContentRequiresConsent {
                sink_id: self.sink_id.clone(),
            });
        }
        if self.content_classes.contains(&ContentClass::Diagnostic)
            && self.content_classes.contains(&ContentClass::Content)
        {
            return Err(TelemetryError::UnknownSinkMember {
                detail: "content+diagnostic may not mix — L3 never carries content".into(),
            });
        }
        self.sampling.validate()?;
        Ok(())
    }

    /// The `idp/1` version id over the canonical bytes (identity through
    /// `hh-identity` — CC1; `RecordKind::SinkPolicy`).
    pub fn version_id(&self) -> String {
        identify_bytes(
            RecordKind::SinkPolicy,
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("sink_id".into(), Json::str(&self.sink_id));
        m.insert(
            "content_classes".into(),
            Json::Arr(
                self.content_classes
                    .iter()
                    .map(|c| Json::str(c.as_str()))
                    .collect(),
            ),
        );
        m.insert("redaction".into(), Json::str(self.redaction.as_str()));
        let mut s = BTreeMap::new();
        s.insert("mode".into(), Json::str(self.sampling.mode.as_str()));
        if let Some(r) = self.sampling.ratio_ppm {
            s.insert("ratio_ppm".into(), Json::Int(r));
        }
        m.insert("sampling".into(), Json::Obj(s));
        if let Some(rl) = &self.rate_limit {
            m.insert(
                "rate_limit".into(),
                Json::obj([("events_per_sec", Json::Int(rl.events_per_sec as i64))]),
            );
        }
        m.insert(
            "max_field_bytes".into(),
            Json::Int(self.max_field_bytes as i64),
        );
        m.insert("requires_consent".into(), Json::Bool(self.requires_consent));
        Json::Obj(m)
    }

    /// Strict decode — `BadMember` on any unknown member; `validate()` runs on
    /// the decoded record (an invalid policy is unrepresentable).
    pub fn from_json(j: &Json) -> Result<SinkPolicy, TelemetryError> {
        let m = expect_obj(j, RECORD)?;
        reject_unknown(
            m,
            &[
                "sink_id",
                "content_classes",
                "redaction",
                "sampling",
                "rate_limit",
                "max_field_bytes",
                "requires_consent",
            ],
            RECORD,
        )?;
        let mut content_classes = BTreeSet::new();
        for c in arr_at(m, "content_classes", RECORD)? {
            let s = c.as_str().ok_or_else(|| CodecError::TypeMismatch {
                member: "content_classes[]".to_string(),
                expected: "string",
            })?;
            content_classes.insert(ContentClass::parse(s)?);
        }
        let sm = expect_obj(
            m.get("sampling").ok_or(CodecError::MissingMember {
                member: "sampling",
                record: RECORD,
            })?,
            RECORD,
        )?;
        reject_unknown(sm, &["mode", "ratio_ppm"], RECORD)?;
        let sampling = Sampling {
            mode: SamplingMode::parse(str_at(sm, "mode", RECORD)?)?,
            ratio_ppm: crate::codec::opt_int_at(sm, "ratio_ppm")?,
        };
        let rate_limit = match m.get("rate_limit") {
            Some(Json::Null) | None => None,
            Some(j) => {
                let rm = expect_obj(j, RECORD)?;
                reject_unknown(rm, &["events_per_sec"], RECORD)?;
                Some(RateLimit {
                    events_per_sec: int_at(rm, "events_per_sec", RECORD)? as u64,
                })
            }
        };
        let p = SinkPolicy {
            sink_id: str_at(m, "sink_id", RECORD)?.to_string(),
            content_classes,
            redaction: Redaction::parse(str_at(m, "redaction", RECORD)?)?,
            sampling,
            rate_limit,
            max_field_bytes: int_at(m, "max_field_bytes", RECORD)? as u64,
            requires_consent: bool_at(m, "requires_consent", RECORD)?,
        };
        p.validate()?;
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_sink() -> SinkPolicy {
        SinkPolicy {
            sink_id: "sink:content".into(),
            content_classes: [ContentClass::Content].into_iter().collect(),
            redaction: Redaction::None,
            sampling: Sampling::ALL,
            rate_limit: None,
            max_field_bytes: 4096,
            requires_consent: true,
        }
    }

    #[test]
    fn l0_l3_classes_are_the_closed_sum() {
        let s: Vec<(&str, u8)> = ContentClass::ALL
            .iter()
            .map(|c| (c.as_str(), c.level()))
            .collect();
        assert_eq!(
            s,
            [
                ("accounting", 0),
                ("structural", 1),
                ("content", 2),
                ("diagnostic", 3)
            ]
        );
        assert!(ContentClass::parse("metrics").is_err());
    }

    #[test]
    fn ac8_schema_half_content_requires_consent() {
        // AC-R-2.9.1-8 (schema half): a {content} sink without
        // requires_consent=true is unrepresentable.
        let mut p = content_sink();
        p.requires_consent = false;
        assert!(matches!(
            p.validate(),
            Err(TelemetryError::ContentRequiresConsent { .. })
        ));
        // …and cannot arrive through the codec either.
        let mut j = match p.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        j.insert("requires_consent".into(), Json::Bool(false));
        assert!(SinkPolicy::from_json(&Json::Obj(j)).is_err());
    }

    #[test]
    fn codec_round_trips_and_refuses_unknown_members() {
        let p = SinkPolicy {
            sampling: Sampling {
                mode: SamplingMode::Ratio,
                ratio_ppm: Some(250_000),
            },
            rate_limit: Some(RateLimit { events_per_sec: 50 }),
            ..SinkPolicy::accounting("sink:acct")
        };
        p.validate().unwrap();
        let j = p.to_json();
        assert_eq!(SinkPolicy::from_json(&j).unwrap(), p);
        // Unknown member → BadMember.
        let mut m = match j.clone() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("extra".into(), Json::Null);
        assert!(matches!(
            SinkPolicy::from_json(&Json::Obj(m)),
            Err(TelemetryError::Codec(CodecError::BadMember { .. }))
        ));
        // sampling.ratio under mode=all is incoherent.
        let bad = SinkPolicy {
            sampling: Sampling {
                mode: SamplingMode::All,
                ratio_ppm: Some(1),
            },
            ..SinkPolicy::accounting("sink:bad")
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn version_id_is_idp1_over_canonical_bytes() {
        let p = SinkPolicy::accounting("sink:a");
        let id = p.version_id();
        assert!(id.starts_with("sha256:"));
        // Deterministic + content-sensitive.
        assert_eq!(id, p.version_id());
        assert_ne!(id, SinkPolicy::accounting("sink:b").version_id());
    }
}
