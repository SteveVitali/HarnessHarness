//! `hh-xcheck` — the cross-implementation replay/checker packaging (spec
//! §10.7; ticket **R2.21**; ADR-0353).
//!
//! The deferred cross-implementation cluster (BL-01; DF-S0.3-2, DF-S1.2-2,
//! DF-S1.5-3, DF-S1.8-1, DF-S1.27-1, the live cell of DF-S1.24-2, LT-03's
//! live-corpus arm) needs one thing before any foreign toolchain runs: the
//! committed golden corpora packaged as a **language-neutral replay bundle**
//! plus the checker contract a second implementation is held to. This crate
//! is that packaging — the landed, durable form of the throwaway S0.3b
//! cross-candidate machinery (`spikes/s0.3b-online-spike`).
//!
//! Three verbs:
//!
//! - [`export::export_bundle`] writes `hh-xcheck-bundle/1` — the corpus
//!   *inputs* (tree manifests, record preimages, the golden WAL transcript,
//!   the canonical registry log, the plugin-manifest fixtures, the
//!   16/64 KiB canonical-parse payloads), the `expected` answers, and a
//!   `CHECKER.md` contract (copied verbatim into the bundle so it travels).
//! - [`check::self_check`] replays the bundle through the **E1**
//!   implementation hermetically — manifest → canonical form → id, WAL →
//!   chain head, log → snapshot outputs, manifest bytes → verdicts — proving
//!   the packaging is sufficient and the recorded answers reproduce.
//! - [`check::verify_answers`] is the comparator: a foreign implementation's
//!   `answers.json` is diffed against `expected` case-by-case; a missing or
//!   disagreeing case fails by name — never a vacuous pass.
//!
//! **Honesty ceiling (CC4 + the R2.21 honesty floor):** the E1 self-check is
//! fixture-verified only. Nothing here is, or mints, a foreign-toolchain
//! result — the pending cells are enumerated in `bundle.json`
//! (`pending_cells[]`) with their gating prerequisite (`HUMAN-H3`).

pub mod check;
pub mod export;
pub mod hex;
pub mod trees;

/// The bundle format id — the `format` member of `bundle.json`.
pub const BUNDLE_FORMAT: &str = "hh-xcheck-bundle/1";

/// The replay arms in export order. Each arm carries its `DEFERRALS.md` row.
pub const ARMS: &[ArmSpec] = &[
    ArmSpec {
        id: "identity-trees",
        df: "DF-S1.2-2",
        dir: "arms/identity-trees",
    },
    ArmSpec {
        id: "identity-records",
        df: "DF-S1.2-2",
        dir: "arms/identity-records",
    },
    ArmSpec {
        id: "ledger-transcript",
        df: "DF-S1.5-3",
        dir: "arms/ledger-transcript",
    },
    ArmSpec {
        id: "registry-snapshot",
        df: "DF-S1.8-1",
        dir: "arms/registry-snapshot",
    },
    ArmSpec {
        id: "plugin-manifests",
        df: "DF-S1.27-1",
        dir: "arms/plugin-manifests",
    },
    ArmSpec {
        id: "canonical-parse",
        df: "DF-S0.3-2",
        dir: "arms/canonical-parse",
    },
];

/// One replay arm: id, the deferral row it discharges (foreign side), and
/// its directory inside the bundle.
pub struct ArmSpec {
    /// The arm id (`answers.json` / `bundle.json` key).
    pub id: &'static str,
    /// The `DEFERRALS.md` row this arm's foreign replay discharges.
    pub df: &'static str,
    /// The arm's directory inside the bundle.
    pub dir: &'static str,
}

/// The cells this ticket does **not** close — every one needs the
/// `HUMAN-H3` foreign-toolchain environment. Recorded in `bundle.json` as
/// `pending_cells[]` so the packaging itself carries the honesty record.
pub const PENDING_CELLS: &[(&str, &str)] = &[
    ("identity-trees.foreign-replay", "DF-S1.2-2"),
    ("identity-records.foreign-replay", "DF-S1.2-2"),
    ("ledger-transcript.foreign-replay", "DF-S1.5-3"),
    ("registry-snapshot.foreign-replay", "DF-S1.8-1"),
    ("plugin-manifests.foreign-checker", "DF-S1.27-1"),
    ("canonical-parse.foreign-timing", "DF-S0.3-2"),
    ("polyglot-ci.two-toolchain", "DF-S0.3-2"),
    ("bench-manifest.live-import", "DF-S1.24-2"),
    ("lt-03.live-corpus-arm", "DF-S1.13-3"),
];

/// The one error type — a tagged string (the `JsonError` precedent).
/// The harness is a build-time tool; a failure always names its cause.
#[derive(Debug)]
pub struct XcheckError(pub String);

impl std::fmt::Display for XcheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "xcheck: {}", self.0)
    }
}

impl std::error::Error for XcheckError {}

impl From<std::io::Error> for XcheckError {
    fn from(e: std::io::Error) -> XcheckError {
        XcheckError(format!("io: {e}"))
    }
}
