// SPDX-License-Identifier: Apache-2.0
//
//! S4.13 (R-2.2.2²; AC-R-2.2.2-9) — the hosted five-status → effect
//! lifecycle mapping (spec §5a.2; ADR-0030 (d); T-LCD-06/-07).
//!
//! A hosted participant's tool-call status stream maps onto the ADR-0030
//! lifecycle with **no native field** — the participant's own vocabulary
//! is projected, never amended:
//!
//! | hosted status | lifecycle row |
//! |---|---|
//! | `pending` | `action.effect.intended` |
//! | `in_progress` | `action.effect.committed` |
//! | `completed` | `action.effect.observed{outcome: applied}` |
//! | `failed` | `action.effect.observed{outcome: not_applied}` |
//! | `cancelled` | `action.effect.unknown{cause: cancelled}` |
//! | *undeclared* | `action.effect.unknown{cause: executor_error, status_raw}` |
//!
//! Undeclared spellings report `unknown` — **never** `read_only`, never a
//! guessed terminal (an unmapped word the kernel cannot verify is an
//! executor-error-class unknown; the raw spelling is preserved on the
//! member for the audit trail — CC3).
//!
//! The mapping is *presentation*: hosted ingestion keeps the
//! `lifecycle.hosted.*`/`action.tool.*` rows it already mints (the Lab's
//! observable vocabulary, kernel-minted); this table is the analysis-side
//! projection a hosted run's effect ledger renders through — the
//! participant never mints `action.effect.*` rows itself
//! (`hosting_ops`'s observational-class gate holds: the participant's
//! claim is lifted, its authority never widens — CC2).

use hh_wire::json::Json;

/// The lifecycle target one hosted status maps to (the closed sum —
/// `unknown` covers every undeclared spelling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedStatusMap {
    /// `pending → action.effect.intended`.
    Intended,
    /// `in_progress → action.effect.committed`.
    Committed,
    /// `completed → action.effect.observed{outcome: applied}`.
    ObservedApplied,
    /// `failed → action.effect.observed{outcome: not_applied}`.
    ObservedNotApplied,
    /// `cancelled → action.effect.unknown{cause: cancelled}`.
    UnknownCancelled,
    /// *undeclared* → `action.effect.unknown{cause: executor_error}` —
    /// never `read_only`, never a fabricated terminal (AC-R-2.2.2-9).
    UnknownUndeclared,
}

impl HostedStatusMap {
    /// The `action.effect.*` class the mapping emits.
    pub fn class(self) -> &'static str {
        match self {
            HostedStatusMap::Intended => "action.effect.intended",
            HostedStatusMap::Committed => "action.effect.committed",
            HostedStatusMap::ObservedApplied | HostedStatusMap::ObservedNotApplied => {
                "action.effect.observed"
            }
            HostedStatusMap::UnknownCancelled | HostedStatusMap::UnknownUndeclared => {
                "action.effect.unknown"
            }
        }
    }

    /// The payload members the mapping contributes (`{outcome}` on
    /// `observed`, `{cause}` on `unknown`; the member set matches the
    /// class's §5a.2 audit shape — the mapping adds *no native field*).
    pub fn members(self) -> Vec<(&'static str, Json)> {
        match self {
            HostedStatusMap::Intended | HostedStatusMap::Committed => Vec::new(),
            HostedStatusMap::ObservedApplied => {
                vec![("outcome", Json::str("applied"))]
            }
            HostedStatusMap::ObservedNotApplied => {
                vec![("outcome", Json::str("not_applied"))]
            }
            HostedStatusMap::UnknownCancelled => vec![("cause", Json::str("cancelled"))],
            HostedStatusMap::UnknownUndeclared => {
                vec![("cause", Json::str("executor_error"))]
            }
        }
    }
}

/// `map_tool_call_status(status)` — the five-status table (spec §5a.2's
/// hosted applicability clause). An undeclared spelling maps to
/// [`HostedStatusMap::UnknownUndeclared`] — reported `unknown`, never
/// `read_only`.
pub fn map_tool_call_status(status: &str) -> HostedStatusMap {
    match status {
        "pending" => HostedStatusMap::Intended,
        "in_progress" => HostedStatusMap::Committed,
        "completed" => HostedStatusMap::ObservedApplied,
        "failed" => HostedStatusMap::ObservedNotApplied,
        "cancelled" => HostedStatusMap::UnknownCancelled,
        _ => HostedStatusMap::UnknownUndeclared,
    }
}

/// `hosted_lifecycle_payload(status, status_raw?)` — the mapped payload:
/// the mapping members plus `status_raw` preserving the participant's own
/// spelling (the audit trail; absent for the five declared values).
pub fn hosted_lifecycle_payload(status: &str) -> Json {
    let map = map_tool_call_status(status);
    let mut members = map.members();
    if matches!(map, HostedStatusMap::UnknownUndeclared) {
        members.push(("status_raw", Json::str(status.to_string())));
    }
    Json::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-R-2.2.2-9 — the five-state status stream maps onto the
    /// lifecycle with no native field; undeclared spellings report
    /// `unknown`, never `read_only`.
    #[test]
    fn ac_r_2_2_2_9_five_status_stream_maps_onto_the_lifecycle() {
        let cases = [
            ("pending", "action.effect.intended"),
            ("in_progress", "action.effect.committed"),
            ("completed", "action.effect.observed"),
            ("failed", "action.effect.observed"),
            ("cancelled", "action.effect.unknown"),
        ];
        for (status, class) in cases {
            assert_eq!(map_tool_call_status(status).class(), class, "{status}");
        }
        assert_eq!(
            map_tool_call_status("completed").members(),
            vec![("outcome", Json::str("applied"))]
        );
        assert_eq!(
            map_tool_call_status("failed").members(),
            vec![("outcome", Json::str("not_applied"))]
        );
        assert_eq!(
            map_tool_call_status("cancelled").members(),
            vec![("cause", Json::str("cancelled"))]
        );
    }

    /// AC-R-2.2.2-9 (tail) — an undeclared spelling lands `unknown`
    /// (`executor_error` cause, raw preserved), never `read_only`, never
    /// a fabricated terminal.
    #[test]
    fn ac_r_2_2_2_9_undeclared_status_is_unknown_never_read_only() {
        for raw in ["completed_v2", "_vendor_done", "", "read_only"] {
            let map = map_tool_call_status(raw);
            assert_eq!(map, HostedStatusMap::UnknownUndeclared, "{raw:?}");
            assert_eq!(map.class(), "action.effect.unknown");
            let payload = hosted_lifecycle_payload(raw);
            assert_eq!(
                payload.get("cause"),
                Some(&Json::str("executor_error")),
                "{raw:?}"
            );
            assert_eq!(
                payload.get("status_raw"),
                Some(&Json::str(raw.to_string())),
                "{raw:?}"
            );
            // The tail clause verbatim — even the literal `read_only`
            // spelling maps to `unknown`.
            assert_ne!(map.class(), "action.effect.refused");
        }
    }
}
