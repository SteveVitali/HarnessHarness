//! The Lab's debt views (spec §5h.6 §5–§6; R-2.9.6⁰ᵃ; S1.24;
//! ADR-0197/0198).
//!
//! - [`DebtIndexRow`] — one row of the debt index (a view over the registered
//!   `AssumptionDebtRecord`s, keyed by `rule_id`).
//! - [`DebtReport`] / [`AssumptionDebtHealth`] — the periodic health view:
//!   `expired_used`, `expiring`, `open`, `not_yet_testable`, and the
//!   `by_removal_test_kind` histogram (the `debt.*` metric names' data
//!   source — §5h.6 §6).
//! - [`DebtNotice`] — the notification record the expiry/removal-test-due
//!   sinks carry (OQ-446's `reach_via` sinks).
//!
//! Everything here is a derived *view* — never stored as truth (the ledger's
//! debt rows are the truth; `project`-style rebuild is the Stage-3 half).

use std::collections::BTreeMap;

use hh_ontology::debt::{
    DebtHome, DebtStatus, EvidenceRef, ExpiryCondition, ExpiryKind, OwnerRef, RemovalTestKind,
};
use hh_wire::Json;

use crate::json_util::*;

// ── DebtIndexRow ────────────────────────────────────────────────────────────

/// `DebtIndexRow` — one row of the debt index (§5h.6 §5): the index is a
/// materialized view over the registered `AssumptionDebtRecord`s; each row
/// carries the fields the health view and the notifier need.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtIndexRow {
    /// The conditioned rule's id.
    pub rule_id: String,
    /// The debt's owner (`principal`/`team` + `reach_via` sinks).
    pub owner: OwnerRef,
    /// The debt status.
    pub status: DebtStatus,
    /// The removal test's kind (`None` = not yet testable).
    pub removal_test_kind: Option<RemovalTestKind>,
    /// The expiry condition.
    pub expiry_condition: ExpiryCondition,
    /// The record's logical creation time (a `seq`).
    pub created_at: u64,
    /// The `until` bound the expiry names, where the kind carries one (the
    /// `expiring` window's right edge).
    pub expiry_at: Option<u64>,
}

impl DebtIndexRow {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("rule_id".into(), Json::str(&self.rule_id));
        m.insert("owner".into(), self.owner.to_json());
        m.insert("status".into(), Json::str(self.status.name()));
        if let Some(k) = &self.removal_test_kind {
            m.insert("removal_test_kind".into(), Json::str(k.name()));
        }
        m.insert("expiry_condition".into(), self.expiry_condition.to_json());
        m.insert("created_at".into(), Json::Int(self.created_at as i64));
        if let Some(t) = self.expiry_at {
            m.insert("expiry_at".into(), Json::Int(t as i64));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<DebtIndexRow, SchemaError> {
        const REC: &str = "DebtIndexRow";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "rule_id",
                "owner",
                "status",
                "removal_test_kind",
                "expiry_condition",
                "created_at",
                "expiry_at",
            ],
            REC,
        )?;
        Ok(DebtIndexRow {
            rule_id: str_at(m, "rule_id", REC)?.to_string(),
            owner: OwnerRef::from_json(member_at(m, "owner", REC)?, "owner")
                .map_err(|e| SchemaError::v("owner", format!("{e:?}")))?,
            status: DebtStatus::parse(str_at(m, "status", REC)?)
                .ok_or_else(|| SchemaError::v("status", "unknown debt status"))?,
            removal_test_kind: opt_str_at(m, "removal_test_kind")?
                .map(|s| {
                    RemovalTestKind::parse(s).ok_or_else(|| {
                        SchemaError::v("removal_test_kind", format!("unknown `{s}`"))
                    })
                })
                .transpose()?,
            expiry_condition: ExpiryCondition::from_json(
                member_at(m, "expiry_condition", REC)?,
                "expiry_condition",
            )
            .map_err(|e| SchemaError::v("expiry_condition", format!("{e:?}")))?,
            created_at: int_at(m, "created_at", REC)? as u64,
            expiry_at: opt_int_at(m, "expiry_at")?.map(|v| v as u64),
        })
    }

    /// Whether the row is expired at `now` — `status = expired`, or an
    /// `expiry_at` bound already passed.
    pub fn expired_at(&self, now: u64) -> bool {
        self.status == DebtStatus::Expired || self.expiry_at.map(|t| now >= t).unwrap_or(false)
    }

    /// Whether the row expires within `horizon` ms of `now` (the `expiring`
    /// bucket — `expiry_at ∈ (now, now + horizon]`).
    pub fn expiring_within(&self, now: u64, horizon: u64) -> bool {
        match self.expiry_at {
            Some(t) => t > now && t <= now.saturating_add(horizon),
            None => false,
        }
    }
}

// ── DebtReport / AssumptionDebtHealth ───────────────────────────────────────

/// `DebtReport` — the periodic assumption-debt health view (§5h.6 §6; the
/// `debt.*` metric names' data source):
/// `expired_used[]` (expired rows still conditioning a rule),
/// `expiring[]` (within the warn window), `open[]` (active, testable),
/// `not_yet_testable[]` (no executable removal test), and the
/// `by_removal_test_kind` histogram.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DebtReport {
    /// The logical generation time (a `seq`).
    pub generated_at: u64,
    /// Expired rows still in use (`lifecycle.debt.expired_used` names them).
    pub expired_used: Vec<String>,
    /// Rows expiring within the policy's `warn_within` window.
    pub expiring: Vec<String>,
    /// Active rows with an executable removal test.
    pub open: Vec<String>,
    /// Active rows with no executable removal test.
    pub not_yet_testable: Vec<String>,
    /// `map<RemovalTestKind, count>` — the per-kind histogram.
    pub by_removal_test_kind: BTreeMap<RemovalTestKind, u64>,
}

/// `AssumptionDebtHealth` — the pure derivation of [`DebtReport`] from the
/// index (§5h.6 §5–§6). `warn_within` is the policy's warn window (ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssumptionDebtHealth {
    /// The warn window (ms) — `DebtPolicy.warn_within`'s runtime value.
    pub warn_within_ms: u64,
}

impl AssumptionDebtHealth {
    /// The OQ-446/OQ-364 placeholder warn window (7 days in ms — data, not a
    /// magic constant in code paths that can read the policy).
    pub const DEFAULT_WARN_WITHIN_MS: u64 = 7 * 24 * 60 * 60 * 1000;

    /// Derive the report at `now` over `index` (deterministic — sorted input,
    /// sorted output).
    pub fn report(&self, index: &[DebtIndexRow], now: u64) -> DebtReport {
        let mut report = DebtReport {
            generated_at: now,
            ..DebtReport::default()
        };
        for row in index {
            if row.expired_at(now) {
                report.expired_used.push(row.rule_id.clone());
                continue;
            }
            if row.expiring_within(now, self.warn_within_ms) {
                report.expiring.push(row.rule_id.clone());
            }
            match row.removal_test_kind {
                Some(k) => {
                    report.open.push(row.rule_id.clone());
                    *report.by_removal_test_kind.entry(k).or_insert(0) += 1;
                }
                None => report.not_yet_testable.push(row.rule_id.clone()),
            }
        }
        report.expired_used.sort();
        report.expiring.sort();
        report.open.sort();
        report.not_yet_testable.sort();
        report
    }

    /// The notices this report implies (§5h.6 §6's notifier): one
    /// `expiry_approaching` notice per `expiring` row, one `expired_used`
    /// notice per `expired_used` row, and a `retirement_test_due` notice per
    /// `expiring` row that names a `RetirementExperiment` removal test.
    pub fn notices<'a>(
        &self,
        index: &'a [DebtIndexRow],
        report: &DebtReport,
    ) -> Vec<DebtNotice<'a>> {
        let mut notices = Vec::new();
        for row in index {
            if report.expired_used.iter().any(|r| r == &row.rule_id) {
                notices.push(DebtNotice {
                    kind: DebtNoticeKind::ExpiredUsed,
                    rule_id: &row.rule_id,
                    owner: &row.owner,
                    expires_at: row.expiry_at,
                    removal_test_kind: row.removal_test_kind,
                    related: &[],
                });
            } else if report.expiring.iter().any(|r| r == &row.rule_id) {
                notices.push(DebtNotice {
                    kind: DebtNoticeKind::ExpiryApproaching,
                    rule_id: &row.rule_id,
                    owner: &row.owner,
                    expires_at: row.expiry_at,
                    removal_test_kind: row.removal_test_kind,
                    related: &[],
                });
                if row.removal_test_kind == Some(RemovalTestKind::RetirementExperiment) {
                    notices.push(DebtNotice {
                        kind: DebtNoticeKind::RetirementTestDue,
                        rule_id: &row.rule_id,
                        owner: &row.owner,
                        expires_at: row.expiry_at,
                        removal_test_kind: row.removal_test_kind,
                        related: &[],
                    });
                }
            }
        }
        notices
    }
}

impl DebtReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("generated_at".into(), Json::Int(self.generated_at as i64));
        let list = |v: &[String]| Json::Arr(v.iter().map(Json::str).collect());
        m.insert("expired_used".into(), list(&self.expired_used));
        m.insert("expiring".into(), list(&self.expiring));
        m.insert("open".into(), list(&self.open));
        m.insert("not_yet_testable".into(), list(&self.not_yet_testable));
        m.insert(
            "by_removal_test_kind".into(),
            Json::Obj(
                self.by_removal_test_kind
                    .iter()
                    .map(|(k, v)| (k.name().to_string(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<DebtReport, SchemaError> {
        const REC: &str = "DebtReport";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "generated_at",
                "expired_used",
                "expiring",
                "open",
                "not_yet_testable",
                "by_removal_test_kind",
            ],
            REC,
        )?;
        let mut by_kind = BTreeMap::new();
        match member_at(m, "by_removal_test_kind", REC)? {
            Json::Obj(hm) => {
                for (k, v) in hm {
                    let kind = RemovalTestKind::parse(k).ok_or_else(|| {
                        SchemaError::v(
                            "by_removal_test_kind",
                            format!("unknown removal test kind `{k}`"),
                        )
                    })?;
                    let n = v.as_int().ok_or_else(|| {
                        SchemaError::v("by_removal_test_kind", "counts must be ints")
                    })?;
                    by_kind.insert(kind, n as u64);
                }
            }
            _ => return Err(SchemaError::v("by_removal_test_kind", "must be an object")),
        }
        Ok(DebtReport {
            generated_at: int_at(m, "generated_at", REC)? as u64,
            expired_used: str_vec_at(m, "expired_used", REC)?,
            expiring: str_vec_at(m, "expiring", REC)?,
            open: str_vec_at(m, "open", REC)?,
            not_yet_testable: str_vec_at(m, "not_yet_testable", REC)?,
            by_removal_test_kind: by_kind,
        })
    }
}

// ── DebtNotice ──────────────────────────────────────────────────────────────

/// The notice kinds (§5h.6 §6's notifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DebtNoticeKind {
    /// The debt's expiry is inside the warn window.
    ExpiryApproaching,
    /// The debt's retirement test is due.
    RetirementTestDue,
    /// An expired debt is still conditioning a rule.
    ExpiredUsed,
}

impl DebtNoticeKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            DebtNoticeKind::ExpiryApproaching => "expiry_approaching",
            DebtNoticeKind::RetirementTestDue => "retirement_test_due",
            DebtNoticeKind::ExpiredUsed => "expired_used",
        }
    }
}

/// `DebtNotice` — the notification record a sink carries (§5h.6 §6): the
/// kind, the rule, the owner (with `reach_via` sinks — OQ-446), the expiry
/// bound, the removal-test kind, and the related evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtNotice<'a> {
    /// The notice kind.
    pub kind: DebtNoticeKind,
    /// The conditioned rule's id.
    pub rule_id: &'a str,
    /// The debt's owner.
    pub owner: &'a OwnerRef,
    /// The expiry bound the notice names.
    pub expires_at: Option<u64>,
    /// The removal test's kind.
    pub removal_test_kind: Option<RemovalTestKind>,
    /// The related evidence refs.
    pub related: &'a [EvidenceRef],
}

impl DebtNotice<'_> {
    /// The canonical JSON (the sink payload).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind.name()));
        m.insert("rule_id".into(), Json::str(self.rule_id));
        m.insert("owner".into(), self.owner.to_json());
        if let Some(t) = self.expires_at {
            m.insert("expires_at".into(), Json::Int(t as i64));
        }
        if let Some(k) = &self.removal_test_kind {
            m.insert("removal_test_kind".into(), Json::str(k.name()));
        }
        if !self.related.is_empty() {
            m.insert(
                "related".into(),
                Json::Arr(self.related.iter().map(EvidenceRef::to_json).collect()),
            );
        }
        Json::Obj(m)
    }
}

// ── helpers re-exported for the views ───────────────────────────────────────

/// Whether `kind` is a *dated* expiry (carries an `until`-style bound the
/// index's `expiry_at` can be derived from) — the `expiring`/`expired`
/// derivations only apply to dated kinds; `experiment_ref`/
/// `dependency_change`/`model_version_change`/`evidence_refresh_due` expiries
/// resolve through their params, not a date.
pub fn dated_expiry(kind: ExpiryKind) -> bool {
    matches!(kind, ExpiryKind::Date | ExpiryKind::ModelVersionChange)
}

/// The `DebtHome` for `(record_kind, field)` — `None` = `UnknownDebtHome`
/// (the record does not live on the `DebtHomes/1` inventory).
pub fn home_for(record_kind: &str, field: &str) -> Option<&'static DebtHome> {
    hh_ontology::debt::DEBT_HOMES
        .iter()
        .find(|h| h.record_kind == record_kind && h.field == field)
}
