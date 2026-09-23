//! The typed refusal set for the results store (§6.5 §2 — "never silently
//! skipped"). Variants are named for the spec's error column where the spec
//! names them (`RunNotDurable`, `RegistryVersionUnknown`,
//! `OverlayNotTargeting`, `Tampered`, `UnknownKey`, `WatermarkAhead`,
//! `MissingMatchSpec`, `NoChange`, `UnknownField`, `UnknownCursor`); the few
//! additions (`NotSubjectRun`, `UnknownVersion`, `DesignNotFound`, `Missing`,
//! `BundleDecode`, `LeaderboardRefused`, `Store`, `Schema`) are recorded in
//! the S3.4b decision record.

use std::fmt;

use hh_ledger::errors::{LedgerError, MissingReason};

/// The results-store error — every variant a typed refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultsError {
    /// A ledger-side failure (manifest read, event scan, verify) — the typed
    /// refusal is preserved, never flattened to a string.
    Ledger(LedgerError),
    /// An io failure on the derived-state directory (rows/heads/catalogue).
    Store {
        /// The io detail.
        detail: String,
    },
    /// A stored/derived record failed strict decode (unknown member,
    /// ill-typed field, bad enum spelling).
    Schema {
        /// The offending member.
        member: String,
        /// What is wrong.
        detail: String,
    },
    /// `project_row`/`build_cells` named a run that is not a `run_kind =
    /// agent` subject row (experiment runs are the *ledger* the store
    /// validates — they never become result rows; §6.5 §2.3).
    NotSubjectRun {
        /// The run.
        run_id: String,
        /// Its `run_kind`.
        run_kind: String,
    },
    /// `project_row(run, until_seq)` where `until_seq` is beyond the durable
    /// head — the requested prefix is not yet durable (§6.5 §2.1).
    RunNotDurable {
        /// The run.
        run_id: String,
        /// The requested seq.
        until_seq: u64,
        /// The durable head.
        head: u64,
    },
    /// `scoring.metric_registry_version` names a registry this build does not
    /// carry (the Stage-3 registry is the compiled-in `hh-eval` catalogue —
    /// one version; a pin to anything else is the refusal).
    RegistryVersionUnknown {
        /// The requested version.
        found: String,
        /// The version this build carries.
        known: String,
    },
    /// An overlay run in `scoring.overlay_runs` produces no verdict and no
    /// `causes[]` link targeting the subject run — `OverlayNotTargeting(run)`
    /// (ADR-0161 D2: a regrade must cite its subject by `audit_ref`/`causes`).
    OverlayNotTargeting {
        /// The overlay run.
        overlay_run_id: String,
        /// The subject it failed to target.
        subject_run_id: String,
    },
    /// A source chain failed verification — the tamper report lifts
    /// `LedgerError::Tampered` into the results vocabulary.
    Tampered {
        /// The first failing seq.
        at_seq: u64,
        /// The tamper class spelling.
        kind: String,
    },
    /// `verify_row` failed — §6.5's `AttestationError::Stale`: a consumer
    /// (leaderboard admission, export) treats the row as stale, never as
    /// silently absent.
    AttestationStale {
        /// The row selector.
        key: String,
        /// The first failing check.
        detail: String,
    },
    /// `get_row`/`row_history` named a key the store does not hold.
    UnknownKey {
        /// The selector.
        key: String,
    },
    /// `get_row(version_id)`/`verify_row` named a version the store does not
    /// hold.
    UnknownVersion {
        /// The version id.
        version_id: String,
    },
    /// A read `at`/`until_seq` named a seq beyond the run's durable head
    /// (`WatermarkAhead` — §6.5 §2.2 errors column).
    WatermarkAhead {
        /// The run.
        run_id: String,
        /// The requested seq.
        seq: u64,
        /// The durable head (`None` = the store does not hold the run).
        head: Option<u64>,
    },
    /// `query_rows` filtered on a field the declared field set does not
    /// carry (`UnknownField` — filters are typed, never free text).
    UnknownField {
        /// The offending field.
        field: String,
    },
    /// `query_rows`/`catalogue` paged from a cursor that does not resolve.
    UnknownCursor {
        /// The offending cursor.
        cursor: String,
    },
    /// `build_cells` over a design whose arms lack `MatchSpec` on a matched
    /// kind (`MissingMatchSpec` — the E-1 refusal surfacing at the results
    /// plane; §6.5 §2.1).
    MissingMatchSpec {
        /// The offending arm.
        arm: String,
    },
    /// `rescore` produced the identical version — `NoChange` (ADR-0161 D2:
    /// identical scoring is refused, never a silent re-append).
    NoChange {
        /// The fixed row key.
        key: String,
    },
    /// A named bundle's manifest bytes are absent (GC/redacted or never
    /// deposited) — `Missing{reason}` (ADR-0068 R3).
    Missing {
        /// The address.
        address: String,
        /// The reason class.
        reason: MissingReason,
    },
    /// A bundle manifest blob decoded but is not a `hh-bundle/1` record.
    BundleDecode {
        /// The bundle id.
        bundle_id: String,
        /// The decode detail.
        detail: String,
    },
    /// `build_cells`/`cells` named an experiment run that is not
    /// `run_kind = experiment`, or a `design_ref` with no registered
    /// experiment.
    DesignNotFound {
        /// The offending reference.
        reference: String,
    },
    /// A leaderboard view refused a cross-stratum rank — the rank column is
    /// `n/a{class|observability}` for some entry (L2: cross-class rank only
    /// on columns applicable to every entry).
    LeaderboardRefused {
        /// Why.
        detail: String,
    },
}

impl From<LedgerError> for ResultsError {
    fn from(e: LedgerError) -> ResultsError {
        match e {
            LedgerError::Tampered(t) => ResultsError::Tampered {
                at_seq: t.at_seq,
                kind: t.kind.as_str().to_string(),
            },
            other => ResultsError::Ledger(other),
        }
    }
}

impl fmt::Display for ResultsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ResultsError {}
