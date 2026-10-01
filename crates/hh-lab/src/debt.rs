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
//! - the Stage-3 retirement slice (R-2.9.6⁰ᵇ): [`settle_removal_test`]
//!   (`RemovalVerdict` + the settle effects — pass ⇒ retirement-eligible,
//!   fail ⇒ `revalidated{evidence_ref}`, inconclusive ⇒ reschedule),
//!   [`retire`] (the human-sealed `expired → retired` gate), and the
//!   `expired_used` exclusion helpers
//!   ([`expired_used_payload`]/[`headline_admitted`]).
//!
//! Everything here is a derived *view* — never stored as truth (the ledger's
//! debt rows are the truth; `project`-style rebuild is the Stage-3 half).

use std::collections::BTreeMap;

use hh_ontology::debt::{
    DebtHome, DebtPolicy, DebtStatus, EvidenceRef, ExpiryCondition, ExpiryKind, OwnerRef,
    RemovalTestKind, RemovalVerdict, Verdict, DEBT_HOMES,
};
use hh_wire::Json;

use crate::json_util::*;

// ── DebtIndexRow ────────────────────────────────────────────────────────────

/// `removal_test{kind, executable, last_report_ref?, last_verdict?}` — the
/// removal-test state a `DebtIndexRow` carries (§5h.6 §3; S5.4).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RemovalTestState {
    /// Whether the record's `RemovalTest` instantiates (`UnexecutableRemovalTest`
    /// otherwise — `validate_removal_test` is the record-side check).
    pub executable: bool,
    /// The last removal-test report ref.
    pub last_report_ref: Option<String>,
    /// The last verdict (`pass`/`fail`/`inconclusive`).
    pub last_verdict: Option<String>,
}

impl RemovalTestState {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("executable".into(), Json::Bool(self.executable));
        if let Some(r) = &self.last_report_ref {
            m.insert("last_report_ref".into(), Json::str(r));
        }
        if let Some(v) = &self.last_verdict {
            m.insert("last_verdict".into(), Json::str(v));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<RemovalTestState, SchemaError> {
        const REC: &str = "RemovalTestState";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["executable", "last_report_ref", "last_verdict"], REC)?;
        Ok(RemovalTestState {
            executable: bool_at(m, "executable", REC)?,
            last_report_ref: opt_str_at(m, "last_report_ref")?.map(str::to_string),
            last_verdict: opt_str_at(m, "last_verdict")?.map(str::to_string),
        })
    }
}

/// `DebtIndexRow` — one row of the debt index (§5h.6 §3's shape; S5.4
/// completed the member set): `debt_ref{home, version_id, rule_id}`,
/// `debt_class`, `owner`, `stored_status` (at seal/registration),
/// `current_status` (the fold of `status.changed` events — CF-423),
/// `evidence_grade`, `deficiency_class`, `last_trigger?`,
/// `next_time_expiry?`, `removal_test{…}`, `used_by[]`,
/// `expired_used_runs`, `staleness_reasons[]`.
///
/// Back-compat (CC8): the Stage-1 members (`rule_id`, `owner`, `status`,
/// `removal_test_kind`, `expiry_condition`, `created_at`, `expiry_at`)
/// still decode; `status` = `current_status`, `expiry_at` =
/// `next_time_expiry` when the new spellings are absent.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtIndexRow {
    /// The conditioned rule's id.
    pub rule_id: String,
    /// The debt's owner (`principal`/`team` + `reach_via` sinks).
    pub owner: OwnerRef,
    /// The debt's *current* status (stored status ⊕ the transition fold).
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
    // ── S5.4 members (§5h.6 §3) — additive ────────────────────────────
    /// `debt_ref.home` — the `DebtHomes/1` row id.
    pub home: Option<u8>,
    /// `debt_ref.version_id` — the registry record version the debt rides.
    pub version_id: Option<String>,
    /// `debt_class`.
    pub debt_class: Option<hh_ontology::debt::DebtClass>,
    /// The status stored at seal/registration (before the event fold).
    pub stored_status: Option<DebtStatus>,
    /// `evidence_grade` — derived, never stored (ADR-0197 D3).
    pub evidence_grade: Option<hh_ontology::debt::EvidenceGrade>,
    /// `deficiency_class` (the `hypothesis_typed.deficiency_class`
    /// spelling, when the record carries one).
    pub deficiency_class: Option<String>,
    /// The trigger spelling of the last `status.changed` event.
    pub last_trigger: Option<String>,
    /// `removal_test{executable, last_report_ref?, last_verdict?}`.
    pub removal_test_state: Option<RemovalTestState>,
    /// `used_by[]` — the sealed-definition version ids the rule is bound in.
    pub used_by: Vec<String>,
    /// `expired_used_runs` — runs that executed while the debt was `expired`.
    pub expired_used_runs: u64,
    /// `staleness_reasons[]` — the causes the current status rides on.
    pub staleness_reasons: Vec<String>,
}

impl DebtIndexRow {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if self.home.is_some() || self.version_id.is_some() {
            let mut dr = BTreeMap::new();
            if let Some(h) = self.home {
                dr.insert("home".into(), Json::Int(h as i64));
            }
            if let Some(v) = &self.version_id {
                dr.insert("version_id".into(), Json::str(v));
            }
            dr.insert("rule_id".into(), Json::str(&self.rule_id));
            m.insert("debt_ref".into(), Json::Obj(dr));
        }
        m.insert("rule_id".into(), Json::str(&self.rule_id));
        m.insert("owner".into(), self.owner.to_json());
        m.insert("status".into(), Json::str(self.status.name()));
        m.insert("current_status".into(), Json::str(self.status.name()));
        if let Some(s) = &self.stored_status {
            m.insert("stored_status".into(), Json::str(s.name()));
        }
        if let Some(c) = &self.debt_class {
            m.insert("debt_class".into(), Json::str(c.name()));
        }
        if let Some(g) = &self.evidence_grade {
            m.insert("evidence_grade".into(), Json::str(g.name()));
        }
        if let Some(d) = &self.deficiency_class {
            m.insert("deficiency_class".into(), Json::str(d));
        }
        if let Some(t) = &self.last_trigger {
            m.insert("last_trigger".into(), Json::str(t));
        }
        if let Some(k) = &self.removal_test_kind {
            m.insert("removal_test_kind".into(), Json::str(k.name()));
        }
        if let Some(ts) = &self.removal_test_state {
            let mut rt = match ts.to_json() {
                Json::Obj(mm) => mm,
                other => unreachable!("RemovalTestState::to_json is an object: {other:?}"),
            };
            if let Some(k) = &self.removal_test_kind {
                rt.insert("kind".into(), Json::str(k.name()));
            }
            m.insert("removal_test".into(), Json::Obj(rt));
        }
        m.insert("expiry_condition".into(), self.expiry_condition.to_json());
        m.insert("created_at".into(), Json::Int(self.created_at as i64));
        if let Some(t) = self.expiry_at {
            m.insert("expiry_at".into(), Json::Int(t as i64));
            m.insert("next_time_expiry".into(), Json::Int(t as i64));
        }
        if !self.used_by.is_empty() {
            m.insert(
                "used_by".into(),
                Json::Arr(self.used_by.iter().map(Json::str).collect()),
            );
        }
        m.insert(
            "expired_used_runs".into(),
            Json::Int(self.expired_used_runs as i64),
        );
        if !self.staleness_reasons.is_empty() {
            m.insert(
                "staleness_reasons".into(),
                Json::Arr(self.staleness_reasons.iter().map(Json::str).collect()),
            );
        }
        Json::Obj(m)
    }

    /// Strict decode — additive members are optional; the Stage-1 spelling
    /// (`status`, `expiry_at`) decodes unchanged.
    pub fn from_json(j: &Json) -> Result<DebtIndexRow, SchemaError> {
        const REC: &str = "DebtIndexRow";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "debt_ref",
                "rule_id",
                "owner",
                "status",
                "current_status",
                "stored_status",
                "debt_class",
                "evidence_grade",
                "deficiency_class",
                "last_trigger",
                "removal_test_kind",
                "removal_test",
                "expiry_condition",
                "created_at",
                "expiry_at",
                "next_time_expiry",
                "used_by",
                "expired_used_runs",
                "staleness_reasons",
            ],
            REC,
        )?;
        let (home, version_id, ref_rule_id) = match m.get("debt_ref") {
            Some(dr) => {
                let dm = expect_obj(dr, "DebtIndexRow.debt_ref")?;
                reject_unknown(dm, &["home", "version_id", "rule_id"], "debt_ref")?;
                (
                    opt_int_at(dm, "home")?.map(|v| v as u8),
                    opt_str_at(dm, "version_id")?.map(str::to_string),
                    opt_str_at(dm, "rule_id")?.map(str::to_string),
                )
            }
            None => (None, None, None),
        };
        let rule_id = opt_str_at(m, "rule_id")?
            .map(str::to_string)
            .or(ref_rule_id)
            .ok_or_else(|| SchemaError::v("rule_id", "missing"))?;
        let status_str = opt_str_at(m, "current_status")?
            .or(opt_str_at(m, "status")?)
            .ok_or_else(|| SchemaError::v("status", "missing"))?;
        let status = DebtStatus::parse(status_str)
            .ok_or_else(|| SchemaError::v("status", "unknown debt status"))?;
        let removal_test_state = match m.get("removal_test") {
            Some(v) => Some(RemovalTestState::from_json(v)?),
            None => None,
        };
        Ok(DebtIndexRow {
            rule_id,
            owner: OwnerRef::from_json(member_at(m, "owner", REC)?, "owner")
                .map_err(|e| SchemaError::v("owner", format!("{e:?}")))?,
            status,
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
            expiry_at: opt_int_at(m, "next_time_expiry")?
                .or(opt_int_at(m, "expiry_at")?)
                .map(|v| v as u64),
            home,
            version_id,
            debt_class: opt_str_at(m, "debt_class")?
                .map(|s| {
                    hh_ontology::debt::DebtClass::parse(s)
                        .ok_or_else(|| SchemaError::v("debt_class", format!("unknown `{s}`")))
                })
                .transpose()?,
            stored_status: opt_str_at(m, "stored_status")?
                .map(|s| {
                    DebtStatus::parse(s)
                        .ok_or_else(|| SchemaError::v("stored_status", format!("unknown `{s}`")))
                })
                .transpose()?,
            evidence_grade: opt_str_at(m, "evidence_grade")?
                .map(|s| {
                    match s {
                        "hypothesized" => Some(hh_ontology::debt::EvidenceGrade::Hypothesized),
                        "evidenced" => Some(hh_ontology::debt::EvidenceGrade::Evidenced),
                        "confirmed" => Some(hh_ontology::debt::EvidenceGrade::Confirmed),
                        _ => None,
                    }
                    .ok_or_else(|| SchemaError::v("evidence_grade", format!("unknown `{s}`")))
                })
                .transpose()?,
            deficiency_class: opt_str_at(m, "deficiency_class")?.map(str::to_string),
            last_trigger: opt_str_at(m, "last_trigger")?.map(str::to_string),
            removal_test_state,
            used_by: match opt_arr_at(m, "used_by")? {
                Some(a) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| SchemaError::v("used_by", "strings only"))
                    })
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            },
            expired_used_runs: opt_int_at(m, "expired_used_runs")?.unwrap_or(0) as u64,
            staleness_reasons: match opt_arr_at(m, "staleness_reasons")? {
                Some(a) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| SchemaError::v("staleness_reasons", "strings only"))
                    })
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            },
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
    /// `scope{home?, version_id?, rule_id?, owner?}` — the subset the report
    /// covers (§5h.6 §3; `None` = the whole index).
    pub scope: Option<Json>,
    /// `grace_bucket` — the bucket names ordered by the policy's `priority`
    /// (the report's own ordering declaration).
    pub priority: Vec<String>,
    /// `hosted_rows[]` — `{key, n/a{class}}` rows for hosted-only surfaces
    /// the health view cannot compute natively (P4 `n/a` — never zero).
    pub hosted_rows: Vec<Json>,
    /// `generation_id` — the caller's content address of this report.
    pub generation_id: Option<String>,
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
        if let Some(s) = &self.scope {
            m.insert("scope".into(), s.clone());
        }
        if !self.priority.is_empty() {
            m.insert(
                "priority".into(),
                Json::Arr(self.priority.iter().map(Json::str).collect()),
            );
        }
        if !self.hosted_rows.is_empty() {
            m.insert("hosted_rows".into(), Json::Arr(self.hosted_rows.clone()));
        }
        if let Some(g) = &self.generation_id {
            m.insert("generation_id".into(), Json::str(g));
        }
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
                "scope",
                "priority",
                "hosted_rows",
                "generation_id",
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
            scope: m.get("scope").cloned(),
            priority: match opt_arr_at(m, "priority")? {
                Some(a) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| SchemaError::v("priority", "strings only"))
                    })
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            },
            hosted_rows: match opt_arr_at(m, "hosted_rows")? {
                Some(a) => a.clone(),
                None => Vec::new(),
            },
            generation_id: opt_str_at(m, "generation_id")?.map(str::to_string),
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

// ── Stage-3 retirement machinery (R-2.9.6⁰ᵇ; §5h.6 §2; ADR-0197 D6/D7) ──────

/// `DebtTransition{debt_ref, from, to, trigger, evidence_ref?, causes[]}` —
/// the pure transition record `settle_removal_test`/`retire`/`evaluate_debt`
/// produce; the caller (kernel/registry) appends
/// `lifecycle.debt.status.changed` rows. Nothing here writes a ledger —
/// records-in, transitions-out (the manager holds no authority handle —
/// §5h.6 §6 D-2).
#[derive(Debug, Clone, PartialEq)]
pub struct DebtTransition {
    /// The debt record the transition settles.
    pub debt_ref: String,
    /// The status before.
    pub from: DebtStatus,
    /// The status after.
    pub to: DebtStatus,
    /// The trigger spelling (`removal_test_failed`, `retired`,
    /// `model_version_change`, `profile_change`, `evidence_superseded`, …).
    pub trigger: String,
    /// The evidence the transition cites (the `ComparisonReport` ref).
    pub evidence_ref: Option<String>,
    /// `causes[]` — every observable that fired, spelled closed (S5.4:
    /// additive; empty on the Stage-3 callers).
    pub causes: Vec<String>,
}

impl DebtTransition {
    /// The `lifecycle.debt.status.changed` payload (the event class's
    /// registered members; `causes[]` rides the row — nothing silently
    /// folded, §5h.6 §2 `evaluate_debt`).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("debt_ref".into(), Json::str(&self.debt_ref));
        m.insert("from".into(), Json::str(self.from.name()));
        m.insert("to".into(), Json::str(self.to.name()));
        m.insert("trigger".into(), Json::str(&self.trigger));
        if let Some(e) = &self.evidence_ref {
            m.insert("evidence_ref".into(), Json::str(e));
        }
        if !self.causes.is_empty() {
            m.insert(
                "causes".into(),
                Json::Arr(self.causes.iter().map(|c| Json::str(c.clone())).collect()),
            );
        }
        Json::Obj(m)
    }
}

/// What `settle_removal_test` did to the record (§5h.6 §2 — the verdict's
/// record-facing half; the immutable `RemovalVerdict` is the other half).
#[derive(Debug, Clone, PartialEq)]
pub enum SettleEffect {
    /// `pass` — the rule is retirement-eligible; status unchanged until a
    /// human seal retires it (D-5/D7 — never automatic).
    RetirementEligible,
    /// `fail` — the report becomes a `comparison_report` evidence ref and
    /// the grade re-derives `confirmed`; an `expiring`/`expired` record
    /// returns to `active` (a fail verdict is evidence, never a
    /// punishment).
    Revalidated {
        /// The `ComparisonReport` ref the revalidation cites.
        evidence_ref: String,
    },
    /// `inconclusive` — the test reschedules under `DebtPolicy` (the
    /// `reason` rides the `RemovalVerdict.inconclusive{reason}` member).
    Reschedule,
}

/// `settle_removal_test(debt_ref, kind, report, prior_status) →
/// (RemovalVerdict, SettleEffect, [DebtTransition])` — the Stage-3 settle:
/// reads the `ComparisonReport`-backed result, produces the immutable
/// `RemovalVerdict` and the transitions the caller appends (§5h.6 §2).
///
/// `verdict`/`reason` are the report's settled outcome (`inconclusive`
/// carries the reason — the caller derives both from the report; this
/// function never re-judges it).
pub fn settle_removal_test(
    debt_ref: &str,
    kind: RemovalTestKind,
    report_ref: &str,
    verdict: Verdict,
    reason: Option<String>,
    settled_at: u64,
    prior_status: DebtStatus,
) -> (RemovalVerdict, SettleEffect, Vec<DebtTransition>) {
    let v = RemovalVerdict {
        debt_ref: debt_ref.to_string(),
        kind,
        verdict,
        reason: reason.clone(),
        report_ref: report_ref.to_string(),
        settled_at,
    };
    match verdict {
        // `pass` ⇒ eligible for retirement — the status is *unchanged*
        // until the human-sealed `retire` (ADR-0197 D7).
        Verdict::Pass => (v, SettleEffect::RetirementEligible, Vec::new()),
        // `fail` ⇒ `revalidated{evidence_ref = report}` — a still-needed
        // rule leaves `expiring`/`expired`; an `active`/`retired` record
        // records no status transition (retired is terminal).
        Verdict::Fail => {
            let transitions = match prior_status {
                DebtStatus::Expiring | DebtStatus::Expired => vec![DebtTransition {
                    debt_ref: debt_ref.to_string(),
                    from: prior_status,
                    to: DebtStatus::Active,
                    trigger: "removal_test_failed".into(),
                    evidence_ref: Some(report_ref.to_string()),
                    causes: vec!["removal_test_failed".into()],
                }],
                _ => Vec::new(),
            };
            (
                v,
                SettleEffect::Revalidated {
                    evidence_ref: report_ref.to_string(),
                },
                transitions,
            )
        }
        // `inconclusive` ⇒ reschedule under policy — no transition.
        Verdict::Inconclusive => (v, SettleEffect::Reschedule, Vec::new()),
    }
}

/// The `retire` refusals (§5h.6 §2's `retire` row; AC-R-2.9.6-4).
#[derive(Debug, Clone, PartialEq)]
pub enum RetireError {
    /// `RetirementNotEvidenced` — no `pass` `RemovalVerdict` on the debt
    /// (an `origin = evolution` diff removing a rule without one is
    /// refused at the gate; proposal is separated from deployment).
    RetirementNotEvidenced {
        /// The debt the removal targeted.
        debt_ref: String,
    },
    /// `decided_by.origin ≠ human` — a `RetirementRecord` is human-sealed
    /// by construction; a non-human `decided_by` never retires.
    NotHumanSealed {
        /// The debt the removal targeted.
        debt_ref: String,
    },
    /// `retired` is terminal — a second `retire` never rewrites history.
    AlreadyRetired {
        /// The debt the removal targeted.
        debt_ref: String,
    },
}

/// The `retire` outcome — the superseding version's debt-facing content:
/// `supersedes{reason: expiry}` plus the `→ retired` transition the caller
/// appends (`lifecycle.debt.status.changed{to: retired}`; the
/// `security.label.endorsed{basis: seal}` row is the seal call's own).
#[derive(Debug, Clone, PartialEq)]
pub struct RetirementOutcome {
    /// The retired debt.
    pub debt_ref: String,
    /// `supersedes.reason` — always `expiry` (the supersession reason the
    /// new version carries; §5h.6 `retire`).
    pub supersedes_reason: &'static str,
    /// The status transition.
    pub transition: DebtTransition,
}

/// `retire(debt_ref, status, verdicts, record)` — the retirement gate
/// (§5h.6 §2; AC-R-2.9.6-4): a `pass` verdict on the debt *and* a
/// human-sealed `RetirementRecord` admit the `→ retired` supersession;
/// anything else is a typed refusal (nothing deletes, nothing
/// auto-retires).
pub fn retire(
    debt_ref: &str,
    status: DebtStatus,
    verdicts: &[RemovalVerdict],
    record: &hh_hir::debt::RetirementRecord,
) -> Result<RetirementOutcome, RetireError> {
    if status == DebtStatus::Retired {
        return Err(RetireError::AlreadyRetired {
            debt_ref: debt_ref.to_string(),
        });
    }
    if !matches!(
        record.decided_by.origin,
        hh_provenance::Origin::Human { .. }
    ) {
        return Err(RetireError::NotHumanSealed {
            debt_ref: debt_ref.to_string(),
        });
    }
    if !verdicts
        .iter()
        .any(|v| v.debt_ref == debt_ref && v.verdict == Verdict::Pass)
    {
        return Err(RetireError::RetirementNotEvidenced {
            debt_ref: debt_ref.to_string(),
        });
    }
    Ok(RetirementOutcome {
        debt_ref: debt_ref.to_string(),
        supersedes_reason: "expiry",
        transition: DebtTransition {
            debt_ref: debt_ref.to_string(),
            from: status,
            to: DebtStatus::Retired,
            trigger: "retired".into(),
            evidence_ref: Some(record.removal_test_report_ref.clone()),
            causes: vec!["human_seal".into()],
        },
    })
}

/// `lifecycle.debt.expired_used{debt_ref, intent_ref}` — the payload a
/// bundle compiled under an `expired` record appends (§5h.6 §3's event
/// row; `classes.rs` registers the class). `intent_ref` names the recorded
/// intent that admitted the compile (`compile_for_expired` in
/// `hh_compiler::link` — absent intent refuses `link` outright).
pub fn expired_used_payload(debt_ref: &str, intent_ref: &str) -> Json {
    Json::obj([
        ("debt_ref", Json::str(debt_ref)),
        ("intent_ref", Json::str(intent_ref)),
    ])
}

/// The headline-exclusion rule (AC-R-2.9.6-5; ADR-0197 D9): a run whose
/// conditioned rule carried an `expired` debt record is excluded from a
/// headline `ComparisonReport` unless the `Design` declares dead-weight
/// purpose. The rule is data — the caller (the report generator / the
/// kernel's `expired_used` projection) supplies both flags.
pub fn headline_admitted(expired_used: bool, dead_weight_purpose: bool) -> bool {
    !expired_used || dead_weight_purpose
}

// ── S5.4: live evaluation, DebtPolicy, DebtIndex — R-2.9.6¹ §5h.6 §2–§6 ──────

/// `ProfileChangeKind` — the closed profile-change trigger family (§5h.6 §2;
/// fired on `model.profile.revised`/`rebound`/`retired` lifecycle events).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileChangeKind {
    /// A superseding L2 profile landed (auto-evaluates the profile-home debts).
    SupersededL2,
    /// The profile was retired (grace period applies — `grace_elapsed` on the
    /// observable).
    Retired,
    /// The bound profile's own `retirement_at` passed.
    PastRetirement,
    /// The binding re-sealed to a different profile revision.
    Rebound,
}

impl ProfileChangeKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ProfileChangeKind::SupersededL2 => "superseded_l2",
            ProfileChangeKind::Retired => "retired",
            ProfileChangeKind::PastRetirement => "past_retirement",
            ProfileChangeKind::Rebound => "rebound",
        }
    }

    /// Parse the spelling.
    pub fn parse(s: &str) -> Option<ProfileChangeKind> {
        match s {
            "superseded_l2" => Some(ProfileChangeKind::SupersededL2),
            "retired" => Some(ProfileChangeKind::Retired),
            "past_retirement" => Some(ProfileChangeKind::PastRetirement),
            "rebound" => Some(ProfileChangeKind::Rebound),
            _ => None,
        }
    }
}

/// `profile_change{kind, grace_elapsed, profile_ref}` — the profile-change
/// observable row (§5h.6 §2's "profile-change triggers").
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileChange {
    /// The change kind.
    pub kind: ProfileChangeKind,
    /// Whether `DebtPolicy.grace_period_ms` has elapsed since the change
    /// (the caller timestamps it — `DebtObservables` holds no clock).
    pub grace_elapsed: bool,
    /// The profile ref the change names.
    pub profile_ref: String,
}

/// `DebtObservables` — the closed observable set `evaluate_debt` reads
/// (§5h.6 §2). The per-home *beneficiary* observables ride
/// [`hh_compiler::expiry::ExpiryObservables`] (one source — CC1); the
/// manager-level families (profile-change, evidence-supersession, probation)
/// are the additional members here.
#[derive(Debug, Clone, Default)]
pub struct DebtObservables {
    /// The §05b observable row (the delegated half — `served_model_mismatch`,
    /// `compatibility_token_changed`, `fingerprint_drift`,
    /// `unresolved_beyond_grace`, `retirement_at_passed`, `probe_records`,
    /// `max_evidence_age_ms`, `experiment_non_inferior`, `revalidation`,
    /// `regression_drifted_rules`).
    pub expiry: hh_compiler::expiry::ExpiryObservables,
    /// `profile_change` — `None` when no profile lifecycle event fired.
    pub profile_change: Option<ProfileChange>,
    /// `evidence_superseded[]` — the evidence refs that resolved as
    /// superseded/revoked/expired since the last evaluation (the
    /// evidence-supersession trigger family).
    pub evidence_superseded: Vec<String>,
    /// The grade of `expiry.revalidation`'s fresh evidence (the caller
    /// grades it — `revalidated` on a `model_conditioned` record requires
    /// ≥ `evidenced`, §5h.6 §2's revalidation rule).
    pub revalidation_grade: Option<hh_ontology::debt::EvidenceGrade>,
}

/// `evaluate_debt(record, home, observables, registry_snapshot,
/// ledger_watermark, now, policy) → [Transition]` — the live all-home
/// evaluator (§5h.6 §2; S5.4). Pure records-in/transitions-out: the caller
/// supplies the observables (`registry_snapshot`/`ledger_watermark` are the
/// caller's projection — this fn reads already-extracted rows), and appends
/// `lifecycle.debt.status.changed` for each returned row.
///
/// Semantics (§5h.6 §2's evaluation block):
/// - `retired` is terminal — no transitions out.
/// - per-home triggers delegate to
///   [`hh_compiler::expiry::evaluate_debt_record`] (the §05b evaluator —
///   kinds `date`/`evidence_refresh_due`/`experiment_ref`/
///   `model_version_change`/`probe_failure` + the `revalidation` leg).
/// - the manager-level families merge in: `profile_change` (homes 1/8/10 and
///   any record whose `scope`/`expiry` names the profile — warn; a `retired`
///   profile past `grace_elapsed` is a hard transition), evidence
///   supersession (`evidence_refs` ∩ `evidence_superseded` ≠ ∅ → warn), and
///   probation (`hypothesized` grade past `hypothesized_max_age` → warn).
/// - triggers firing the same direction fold into one `DebtTransition` with
///   `causes[]` naming every firing observable; an `expired` transition
///   subsumes a simultaneous `expiring` one.
/// - idempotent: a record already in the target state emits no row for it.
/// - `revalidated` on a `model_conditioned` record requires
///   `revalidation_grade ≥ evidenced` (§5h.6 §2) — otherwise the leg is
///   suppressed.
pub fn evaluate_debt(
    debt_ref: &str,
    record: &hh_hir::records::AssumptionDebtRecord,
    home_id: Option<u8>,
    obs: &DebtObservables,
    now_ms: u64,
    policy: &DebtPolicy,
) -> Vec<DebtTransition> {
    if record.status == DebtStatus::Retired {
        return Vec::new();
    }
    let cur = record.status;
    let mut warn_causes: Vec<String> = Vec::new();
    let mut hard_causes: Vec<String> = Vec::new();
    let mut evidence_ref: Option<String> = None;
    let mut revalidated: Option<(String, Vec<String>)> = None;

    // ── (1) the delegated §05b half ─────────────────────────────────────────
    let view = profile_debt_view(record);
    for t in hh_compiler::expiry::evaluate_debt_record(
        &view,
        Some(&record.rule_id),
        None,
        &obs.expiry,
        now_ms,
    ) {
        match t.to {
            DebtStatus::Expiring => warn_causes.extend(t.causes.iter().cloned()),
            DebtStatus::Expired => hard_causes.extend(t.causes.iter().cloned()),
            DebtStatus::Active => {
                // The `revalidated` leg — gate model_conditioned on the fresh
                // evidence's grade (§5h.6 §2).
                let model_conditioned = matches!(
                    record.debt_class,
                    Some(hh_ontology::debt::DebtClass::ModelConditioned)
                );
                let grade_ok = !model_conditioned
                    || matches!(
                        obs.revalidation_grade,
                        Some(hh_ontology::debt::EvidenceGrade::Evidenced)
                            | Some(hh_ontology::debt::EvidenceGrade::Confirmed)
                    );
                if grade_ok {
                    revalidated = t
                        .evidence_ref
                        .clone()
                        .map(|e| (e, t.causes.clone()))
                        .or(revalidated);
                }
            }
            DebtStatus::Retired => {} // retired is human-sealed — never auto.
        }
        if evidence_ref.is_none() && t.to != DebtStatus::Active {
            evidence_ref = t.evidence_ref.clone();
        }
    }

    // ── (2) profile-change family (the manager-level trigger; fires for the
    // profile homes 2–5 — `profile_rule`/`model_profile`/`model_profile_ext`/
    // `fallback_profile`; `None` = caller asserts applicability) ────────────
    let profile_home = home_id.map(|h| matches!(h, 2..=5)).unwrap_or(true);
    if profile_home {
        if let Some(pc) = &obs.profile_change {
            let cause = format!("profile_change:{}", pc.kind.name());
            if pc.kind == ProfileChangeKind::Retired && pc.grace_elapsed {
                hard_causes.push(cause);
            } else {
                warn_causes.push(cause);
            }
            if evidence_ref.is_none() {
                evidence_ref = Some(pc.profile_ref.clone());
            }
        }
    }

    // ── (3) evidence supersession ───────────────────────────────────────────
    let superseded: Vec<String> = record
        .evidence_refs
        .iter()
        .filter(|r| obs.evidence_superseded.iter().any(|s| s == &r.reference))
        .map(|r| r.reference.clone())
        .collect();
    if !superseded.is_empty() {
        warn_causes.push("evidence_superseded".into());
        if evidence_ref.is_none() {
            evidence_ref = superseded.first().cloned();
        }
    }

    // ── (4) probation (hypothesized records past the probation bound) ───────
    if record.evidence_grade() == hh_ontology::debt::EvidenceGrade::Hypothesized {
        if let Some(created) = record.created_at {
            if created.saturating_add(policy.hypothesized_max_age_ms) <= now_ms {
                warn_causes.push("probation_overrun".into());
            }
        }
    }

    // ── (5) evidence refresh bound fallback: the record names
    // `evidence_refresh_due` without `expiry.params.evidence_max_age_ms` —
    // the policy's bound applies (the delegated evaluator reads params).
    if record.expiry_condition.kind == ExpiryKind::EvidenceRefreshDue
        && record
            .expiry
            .as_ref()
            .and_then(|e| e.params.evidence_max_age_ms)
            .is_none()
        && obs
            .expiry
            .max_evidence_age_ms
            .map(|a| a > policy.evidence_max_age_ms)
            .unwrap_or(false)
    {
        hard_causes.push(format!("{}:over", ExpiryKind::EvidenceRefreshDue.name()));
    }

    // ── merge: hard subsumes warn; same-direction causes fold ───────────────
    let mut out: Vec<DebtTransition> = Vec::new();
    if !hard_causes.is_empty() && cur != DebtStatus::Expired {
        let mut causes = hard_causes;
        causes.extend(warn_causes);
        causes.sort();
        causes.dedup();
        out.push(DebtTransition {
            debt_ref: debt_ref.to_string(),
            from: cur,
            to: DebtStatus::Expired,
            trigger: "expired".into(),
            evidence_ref: evidence_ref.clone(),
            causes,
        });
    } else if !warn_causes.is_empty() && cur == DebtStatus::Active {
        warn_causes.sort();
        warn_causes.dedup();
        out.push(DebtTransition {
            debt_ref: debt_ref.to_string(),
            from: cur,
            to: DebtStatus::Expiring,
            trigger: "expiring".into(),
            evidence_ref,
            causes: warn_causes,
        });
    }
    if let Some((eref, mut causes)) = revalidated {
        causes.sort();
        causes.dedup();
        out.push(DebtTransition {
            debt_ref: debt_ref.to_string(),
            from: cur,
            to: DebtStatus::Active,
            trigger: "revalidated".into(),
            evidence_ref: Some(eref),
            causes,
        });
    }
    out
}

/// The `ProfileDebtRecord` view of an `AssumptionDebtRecord` — the field
/// shapes are the `/1` vocabulary (CC1: one record family); the spellings
/// differ where the profile home records principal ids plainly.
fn profile_debt_view(
    r: &hh_hir::records::AssumptionDebtRecord,
) -> hh_compiler::profile::ProfileDebtRecord {
    hh_compiler::profile::ProfileDebtRecord {
        rule_id: r.rule_id.clone(),
        hypothesis: r
            .hypothesis
            .content
            .clone()
            .unwrap_or_else(|| r.hypothesis.content_hash.clone()),
        evidence_refs: r.evidence_refs.clone(),
        owner: r.owner.id.clone(),
        reach_via: r.owner.reach_via.clone(),
        expiry_condition: r.expiry_condition.clone(),
        removal_test_ref: r.removal_test_ref.clone(),
        removal_test: r.removal_test.clone(),
        status: r.status,
        debt_class: r.debt_class,
        hypothesis_typed: r.hypothesis_typed.clone(),
        scope: r.scope.clone(),
        expiry: r.expiry.clone(),
        runway_ms: r.runway_ms,
        revalidation: r.revalidation.clone(),
        created_at: r.created_at,
        supersedes: r.supersedes.clone(),
    }
}

/// `debt_incomplete` — the record-level required-members check against the
/// home's `required_fields` (§5h.6 §2's seal-time check; the record's own
/// `hh_hir::debt::validate_for_home` covers the `/1` required set — this
/// projects the *missing* list the refusal carries).
pub fn missing_required_fields(
    record: &hh_hir::records::AssumptionDebtRecord,
    home_id: Option<u8>,
    policy: &DebtPolicy,
) -> Vec<String> {
    let home = match home_id.and_then(|id| DEBT_HOMES.iter().find(|h| h.id == id)) {
        Some(h) => h,
        None => return Vec::new(),
    };
    let mut missing: Vec<String> = hh_ontology::debt::required_fields(home, policy)
        .iter()
        .filter(|f| !record.has_field(f))
        .cloned()
        .collect();
    missing.sort();
    missing
}

/// `DebtIndexEntry` — one input to [`debt_index`]: the registered record plus
/// the projections the index materializes (the `status.changed` event fold,
/// the removal-test state, `used_by`, `expired_used_runs`).
#[derive(Debug, Clone)]
pub struct DebtIndexEntry {
    /// `debt_ref.home` — the `DebtHomes/1` row id (`None` = unlisted —
    /// `UnknownDebtHome` territory; the row still indexes, marked).
    pub home: Option<u8>,
    /// `debt_ref.version_id` — the registry record version the debt rides.
    pub version_id: Option<String>,
    /// The registered `AssumptionDebtRecord` (status = stored status at seal).
    pub record: hh_hir::records::AssumptionDebtRecord,
    /// The `lifecycle.debt.status.changed` transitions for this `debt_ref`,
    /// in ledger order (the fold — CF-423).
    pub transitions: Vec<DebtTransition>,
    /// The removal-test state (the last bound report, when any).
    pub removal_test_state: Option<RemovalTestState>,
    /// `used_by[]` — the sealed-definition version ids the rule is bound in.
    pub used_by: Vec<String>,
    /// `expired_used_runs` — runs that executed while the debt was `expired`.
    pub expired_used_runs: u64,
}

/// `debt_index(entries) → [DebtIndexRow]` — the §5h.6 §3 index: a
/// materialized view — current status = stored status ⊕ the transition fold;
/// `last_trigger`/`staleness_reasons` name the latest fold step (rebuild
/// equality — the same entries always produce the same rows, and rebuilding
/// after N events equals the live fold).
pub fn debt_index(entries: &[DebtIndexEntry]) -> Vec<DebtIndexRow> {
    let mut rows: Vec<DebtIndexRow> = entries
        .iter()
        .map(|e| {
            let stored = e.record.status;
            let mut status = stored;
            let mut last_trigger: Option<String> = None;
            let mut staleness: Vec<String> = Vec::new();
            for t in &e.transitions {
                if t.to != status {
                    staleness.push(t.trigger.clone());
                }
                status = t.to;
                last_trigger = Some(t.trigger.clone());
            }
            staleness.sort();
            staleness.dedup();
            DebtIndexRow {
                rule_id: e.record.rule_id.clone(),
                owner: e.record.owner.clone(),
                status,
                removal_test_kind: e.record.removal_test.as_ref().map(|t| t.kind),
                expiry_condition: e.record.expiry_condition.clone(),
                created_at: e.record.created_at.unwrap_or(0),
                expiry_at: e
                    .record
                    .expiry
                    .as_ref()
                    .and_then(|x| x.params.until)
                    .or_else(|| {
                        (e.record.expiry_condition.kind == ExpiryKind::Date)
                            .then(|| e.record.expiry_condition.value.clone())
                            .flatten()
                            .and_then(|v| v.parse().ok())
                    }),
                home: e.home,
                version_id: e.version_id.clone(),
                debt_class: e.record.debt_class,
                stored_status: Some(stored),
                evidence_grade: Some(e.record.evidence_grade()),
                deficiency_class: e
                    .record
                    .hypothesis_typed
                    .as_ref()
                    .map(|h| h.deficiency_class.name().to_string()),
                last_trigger,
                removal_test_state: e.removal_test_state.clone(),
                used_by: e.used_by.clone(),
                expired_used_runs: e.expired_used_runs,
                staleness_reasons: staleness,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.home.unwrap_or(u8::MAX), &a.rule_id).cmp(&(b.home.unwrap_or(u8::MAX), &b.rule_id))
    });
    rows
}

/// `route_notices(report, index, policy) → {sink → [notice]}` — the
/// §5h.6 §6 notifier's routing half: each notice's owner `reach_via` sinks
/// intersected with the policy's declared `notice_sinks` (a notice whose
/// owner declares no reachable sink lands under `unrouted` — the
/// `OwnerUnreachable` surface, never silently dropped).
pub fn route_notices<'a>(
    notices: &[DebtNotice<'a>],
    policy: &DebtPolicy,
) -> BTreeMap<String, Vec<Json>> {
    let mut out: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    for n in notices {
        let sinks: Vec<String> = n
            .owner
            .reach_via
            .iter()
            .filter(|s| policy.notice_sinks.iter().any(|d| d == *s))
            .cloned()
            .collect();
        let payload = n.to_json();
        if sinks.is_empty() {
            out.entry("unrouted".into()).or_default().push(payload);
        } else {
            for s in sinks {
                out.entry(s).or_default().push(payload.clone());
            }
        }
    }
    out
}
