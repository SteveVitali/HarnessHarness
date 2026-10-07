//! The containment/egress security metric folds (spec §5g.4 §6's process
//! metrics; DF-S1.12-4's runtime-emission half; R2.9b; ADR-0341 D7).
//!
//! The four declarations live in [`crate::catalogue`]; this module is the
//! fold that produces their values from a run's *projected* ledger rows —
//! the same `FactRow` view [`LedgerFacts`] reads. `hh-eval` reads ledger
//! data, never containment/env code (CC5/CC6 — the fold composes over the
//! recorded rows wherever a run produced them).
//!
//! Fold definitions (owned here; the run ledger records the decision):
//!
//! - `egress.ask_rate` — `security.egress.decided{decision = ask}` rows /
//!   all `security.egress.decided` rows, ppm. `n/a` on a run with no
//!   mediated egress.
//! - `containment.violation_rate` — `security.containment.violated` rows /
//!   `security.containment.applied` rows, ppm. `n/a` on a run that never
//!   attached a policy.
//! - `proxy.hijack_attempt_rate` — `security.containment.applied` rows
//!   whose `lowering_loss_fields` carries a proxy-surface tag
//!   (`net.upstream_proxy`, or `proc.env` — a proxy variable surface that
//!   cannot count as mediation; §5g.4 §5's "proxy env vars mistaken for
//!   mediation" row) / all `applied` rows, ppm. The fold reads the
//!   *declared-field spellings* the applied row carries — the blob is
//!   never resolved (the report stays content-free at the event).
//! - `session.revoked_continuation_denied` — a count, not a rate: the
//!   number of `security.credential.denied{reason = revoked}` rows — a
//!   revoked binding/lease that a continuation tried anyway and the
//!   boundary refused. Emitted when the credential machinery ran (any
//!   `security.credential.*` row); `n/a{not_run}` otherwise.
//!
//! [`LedgerFacts`]: crate::facts::LedgerFacts

use hh_wire::Json;

use crate::facts::FactRow;
use crate::stats::PPM;

/// `p.member` as a string slice.
fn ms<'a>(p: &'a Json, member: &str) -> Option<&'a str> {
    p.get(member).and_then(Json::as_str)
}

/// The proxy-surface field spellings a `lowering_loss_fields` member can
/// carry (the `proxy_env_is_not_mediation` family — `hh-containment`'s
/// closed reason tags name these declared fields; ADR-0341 D4/D5).
const PROXY_SURFACE_FIELDS: &[&str] = &["net.upstream_proxy", "proc.env"];

/// The fold's typed result — one member per declared metric, `Option`
/// where the run can fail to produce a value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecurityMetrics {
    /// Asked egress decisions / all egress decisions (ppm).
    pub egress_ask_rate: Option<i64>,
    /// Containment violations / applied policies (ppm).
    pub containment_violation_rate: Option<i64>,
    /// Applied policies carrying a proxy-surface loss / all applied (ppm).
    pub proxy_hijack_attempt_rate: Option<i64>,
    /// Revoked continuations refused (a count).
    pub session_revoked_continuation_denied: Option<i64>,
}

/// `fold(rows)` — the one-pass deterministic computation over the run's
/// `security.*` rows.
pub fn fold(rows: &[FactRow]) -> SecurityMetrics {
    let mut m = SecurityMetrics::default();

    // ── egress.ask_rate ─────────────────────────────────────────────────
    let decided: Vec<&FactRow> = rows
        .iter()
        .filter(|r| r.class == "security.egress.decided")
        .collect();
    if !decided.is_empty() {
        let asked = decided
            .iter()
            .filter(|r| ms(&r.payload, "decision") == Some("ask"))
            .count() as i64;
        m.egress_ask_rate = Some(asked * PPM / decided.len() as i64);
    }

    // ── containment.violation_rate ──────────────────────────────────────
    let applied: Vec<&FactRow> = rows
        .iter()
        .filter(|r| r.class == "security.containment.applied")
        .collect();
    if !applied.is_empty() {
        let violated = rows
            .iter()
            .filter(|r| r.class == "security.containment.violated")
            .count() as i64;
        m.containment_violation_rate = Some(violated * PPM / applied.len() as i64);
    }

    // ── proxy.hijack_attempt_rate ───────────────────────────────────────
    if !applied.is_empty() {
        let attempts = applied
            .iter()
            .filter(|r| match r.payload.get("lowering_loss_fields") {
                Some(Json::Arr(fields)) => fields.iter().any(|f| {
                    f.as_str()
                        .map(|s| PROXY_SURFACE_FIELDS.contains(&s))
                        .unwrap_or(false)
                }),
                _ => false,
            })
            .count() as i64;
        m.proxy_hijack_attempt_rate = Some(attempts * PPM / applied.len() as i64);
    }

    // ── session.revoked_continuation_denied ─────────────────────────────
    let credential_rows = rows
        .iter()
        .any(|r| r.class.starts_with("security.credential."));
    if credential_rows {
        let denied = rows
            .iter()
            .filter(|r| {
                r.class == "security.credential.denied"
                    && ms(&r.payload, "reason") == Some("revoked")
            })
            .count() as i64;
        m.session_revoked_continuation_denied = Some(denied);
    }

    m
}

/// The fold's canonical emission — `(metric, Option<value>)` rows in
/// declaration order. `None` is a genuine non-value: the caller renders the
/// typed `n/a{not_run}` (never 0, never a coerced `unknown`).
pub fn emit(m: &SecurityMetrics) -> Vec<(&'static str, Option<i64>)> {
    vec![
        ("egress.ask_rate", m.egress_ask_rate),
        ("containment.violation_rate", m.containment_violation_rate),
        ("proxy.hijack_attempt_rate", m.proxy_hijack_attempt_rate),
        (
            "session.revoked_continuation_denied",
            m.session_revoked_continuation_denied,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(seq: u64, class: &str, payload: Json) -> FactRow {
        FactRow {
            seq,
            event_id: None,
            class: class.into(),
            payload,
        }
    }

    #[test]
    fn ask_rate_folds_decided_rows() {
        let rows = vec![
            row(
                1,
                "security.egress.decided",
                Json::obj([("decision", Json::str("allow"))]),
            ),
            row(
                2,
                "security.egress.decided",
                Json::obj([("decision", Json::str("ask"))]),
            ),
            row(
                3,
                "security.egress.decided",
                Json::obj([("decision", Json::str("ask"))]),
            ),
            row(
                4,
                "security.egress.decided",
                Json::obj([("decision", Json::str("deny"))]),
            ),
        ];
        let m = fold(&rows);
        assert_eq!(m.egress_ask_rate, Some(PPM / 2));
    }

    #[test]
    fn violation_rate_folds_against_applied() {
        let rows = vec![
            row(1, "security.containment.applied", Json::obj([])),
            row(2, "security.containment.applied", Json::obj([])),
            row(3, "security.containment.violated", Json::obj([])),
        ];
        let m = fold(&rows);
        assert_eq!(m.containment_violation_rate, Some(PPM / 2));
    }

    #[test]
    fn proxy_attempt_reads_declared_field_tags() {
        let applied = |fields: Vec<&str>| {
            row(
                1,
                "security.containment.applied",
                Json::obj([(
                    "lowering_loss_fields",
                    Json::Arr(fields.into_iter().map(Json::str).collect()),
                )]),
            )
        };
        let rows = vec![
            applied(vec!["proc.env"]),
            applied(vec!["resources.cpu_ms"]),
            applied(vec!["net.upstream_proxy"]),
            applied(vec![]),
        ];
        let m = fold(&rows);
        assert_eq!(m.proxy_hijack_attempt_rate, Some(PPM / 2));
    }

    #[test]
    fn revoked_denial_is_a_count() {
        let denied = |reason: &str| {
            row(
                1,
                "security.credential.denied",
                Json::obj([("reason", Json::str(reason))]),
            )
        };
        let rows = vec![denied("revoked"), denied("revoked"), denied("policy")];
        let m = fold(&rows);
        assert_eq!(m.session_revoked_continuation_denied, Some(2));
    }

    #[test]
    fn empty_run_is_na_never_zero() {
        let m = fold(&[]);
        assert!(emit(&m).iter().all(|(_, v)| v.is_none()));
    }
}
