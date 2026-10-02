//! `hh-results` — the Stage-3 results store (spec §6.5, R-2.10.5⁰; ticket
//! S3.4b; ADR-0161/0162/0163).
//!
//! The results store is a **projection layer, never a store of record**
//! (§6.5 §1): everything it holds is `project()`-pure at a recorded
//! watermark set, carries a `view_hash`, and rebuilds to equal bytes from
//! the authoritative sources — the run ledgers, blobs, manifests, bundle
//! manifests, registry history, metric registry, and pricing snapshots. It
//! has no update or delete operation; rows supersede as immutable
//! `RowVersion`s under a fixed `RowKey`.
//!
//! Modules:
//!
//! - [`row`] — `ResultsRow/1` (`hh-results-row/1`) and its sections.
//! - [`watermark`] — `WatermarkSet`, the `{run → seq}` coverage pin.
//! - [`audit`] — `AuditRef` citations + `verify_citation` (hash match +
//!   Merkle inclusion + content-refs presence).
//! - [`scoring`] — `ScoringContext`, the pinned scoring inputs.
//! - [`version`] — `RowVersion` records + `DerivedReason`.
//! - [`projection`] — `project_row(run_id, until_seq?, scoring?)`.
//! - [`store`] — `ResultsStore`, the derived-state directory + every read.
//! - [`query`] — `QuerySpec` typed filters + pagination.
//! - [`cells`] — `build_cells` → `CellTable`, `distribution`.
//! - [`catalogue`] — `catalogue_refresh` → `BundleCatalogue` (header-only).
//! - [`annotations`] — `annotate` → the rebuildable annotation index.
//! - [`verify`] — `verify_row`'s per-check `VerifyReport`.
//! - [`leaderboard`] — the L1–L9 `leaderboard()` plus the C1 publication
//!   surface (`define_leaderboard`, snapshots, `diff_snapshots`,
//!   `verify_snapshot`, `retract_entry`, `publish_leaderboard`,
//!   `pin_set` — ADR-0163).
//! - [`disclosure`] — `DisclosureSummary`, the per-experiment disclosure
//!   counts (`arms_registered/published/restricted`, exclusions,
//!   analysis split — ADR-0162 D5).
//! - [`journal`] — `subscribe`, the durable results journal over the
//!   ledger subscription stream.
//! - [`export`] — `export_rows`, the lowering/export op with
//!   `LoweringLossReport` + `measurement.export.delivered`.
//! - [`error`] — the typed refusal set (§6.5 §2 errors column).

#![forbid(unsafe_code)]

pub mod annotations;
pub mod audit;
pub mod catalogue;
pub mod cells;
pub mod disclosure;
pub mod error;
pub mod export;
pub mod journal;
pub mod leaderboard;
pub mod projection;
pub mod query;
pub mod row;
pub mod scoring;
pub mod status;
pub mod store;
pub mod verify;
pub mod version;
pub mod watermark;

pub use audit::{AuditRef, CitationVerdict};
pub use catalogue::{BundleCatalogue, BundleCatalogueEntry, BundleStatus};
pub use cells::{CellTable, DistributionRef, TableCell};
pub use disclosure::{disclosure_summary, DisclosureSummary};
pub use error::ResultsError;
pub use export::{
    export_rows, round_trip_native, ExportOutcome, ExportTarget, LossEntry, LoweringLossReport,
};
pub use journal::{JournalEvent, JournalFilter, JournalKind, JournalSubscription};
pub use leaderboard::{
    DisclosurePolicy, LeaderboardDefinition, LeaderboardEntry, LeaderboardSnapshot,
    PublishedLeaderboard, SnapshotDiff,
};
pub use query::{Page, QuerySpec};
pub use row::{ResultsRow, RowKey, ROW_SCHEMA};
pub use scoring::ScoringContext;
pub use status::{
    fold as fold_status_book, load_book, set_status, BundleStatusRecord, StatusBook, STATUS_EVENT,
};
pub use store::{ResultsStore, VerifyVerdict};
pub use version::{DerivedReason, RowVersion};
pub use watermark::WatermarkSet;
