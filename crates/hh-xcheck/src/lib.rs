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
pub mod hir_replay;
pub mod trees;

/// The bundle format id — the `format` member of `bundle.json`.
pub const BUNDLE_FORMAT: &str = "hh-xcheck-bundle/1";

/// The replay arms in export order. Each arm carries its `DEFERRALS.md` row.
pub const ARMS: &[ArmSpec] = &[
    ArmSpec {
        id: "identity-trees",
        df: "DF-S1.2-2",
        dir: "arms/identity-trees",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    ArmSpec {
        id: "identity-records",
        df: "DF-S1.2-2",
        dir: "arms/identity-records",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    ArmSpec {
        id: "ledger-transcript",
        df: "DF-S1.5-3",
        dir: "arms/ledger-transcript",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    ArmSpec {
        id: "registry-snapshot",
        df: "DF-S1.8-1",
        dir: "arms/registry-snapshot",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    ArmSpec {
        id: "plugin-manifests",
        df: "DF-S1.27-1",
        dir: "arms/plugin-manifests",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    ArmSpec {
        id: "canonical-parse",
        df: "DF-S0.3-2",
        dir: "arms/canonical-parse",
        verified_by: &[E1_CHECK, E2_RUN, E3_RUN],
    },
    // R4.3 extension arms — DF-S1.2-2's "extend to the HIR/ledger corpus"
    // leg: HIR/1 documents on the HIR side, additional transcript/WAL
    // scenarios on the ledger side (DF-S1.5-3). The committed
    // `foreign/e2-python` tree cannot answer them (CF-493 records the
    // honest gap — E3's `verified_by[]` only).
    ArmSpec {
        id: "hir-documents",
        df: "DF-S1.2-2",
        dir: "arms/hir-documents",
        verified_by: &[E1_CHECK, E3_RUN],
    },
    ArmSpec {
        id: "ledger-scenarios",
        df: "DF-S1.5-3",
        dir: "arms/ledger-scenarios",
        verified_by: &[E1_CHECK, E3_RUN],
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
    /// The candidates whose recorded evidence verifies this arm —
    /// `verified_by[]` on the `bundle.json` arm member (CF-493).
    pub verified_by: &'static [CandidateEvidence],
}

/// One candidate's discharge evidence — `verified_by[]` renders
/// `{candidate, evidence_class, evidence_ref}` on `arms[]` and
/// `pending_cells[]` (CF-493): the E1 self-check is `fixture-verified`;
/// a foreign toolchain's recorded replay is `foreign-run-verified` — a
/// distinct class *below* the in-tree fixture-verified ceiling, never
/// conflated with it. A pending cell's `verified_by[]` records partial
/// discharge honestly — `status` stays `environment-pending`.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateEvidence {
    /// The candidate id (`E1` in-tree; `E2-candidate`/`E3-candidate`
    /// foreign).
    pub candidate: &'static str,
    /// The evidence class (`fixture-verified` | `foreign-run-verified`).
    pub evidence_class: &'static str,
    /// The evidence ref — the run's citation (the committed tree + its
    /// recorded replay), never a bare claim.
    pub evidence_ref: &'static str,
}

/// The E1 self-check evidence every exported arm carries.
pub const E1_CHECK: CandidateEvidence = CandidateEvidence {
    candidate: "E1",
    evidence_class: "fixture-verified",
    evidence_ref: "hh-xcheck selfcheck",
};
/// The E2 foreign replay (python3) — PR #136, `hh-xcheck verify` 49/49
/// (2026-10-14); the committed `foreign/e2-python` tree answers only the
/// six original arms.
pub const E2_RUN: CandidateEvidence = CandidateEvidence {
    candidate: "E2-candidate",
    evidence_class: "foreign-run-verified",
    evidence_ref: "foreign/e2-python — PR #136 (49/49)",
};
/// The E3 foreign replay (node ≥ 22; R4.4/R4.4a/R4.4b) — `hh-xcheck
/// verify` 57/57 over all eight arms.
pub const E3_RUN: CandidateEvidence = CandidateEvidence {
    candidate: "E3-candidate",
    evidence_class: "foreign-run-verified",
    evidence_ref: "foreign/e3-typescript — R4.4b (57/57)",
};

/// The cells the bundle does **not** close — `(cell, df, needs,
/// verified_by)` quadruples (CF-493). The `needs` member names the gating
/// prerequisite: `HUMAN-H3` is the foreign-toolchain environment;
/// `e2-tree arm extension` is the honest record (CF-493 vocabulary —
/// `status` stays `environment-pending`, never a new member) that the
/// committed `foreign/e2-python` tree answers only the six original
/// arms — the R4.3 arms' E2 replay awaits the e2-tree extension R4.4a
/// may land, their E3 replay the E3 toolchain. `verified_by` records a
/// cell's *partial* discharge honestly — the recorded foreign replay is
/// evidence for the cell, never its closure: `status` stays
/// `environment-pending` while the named prerequisite stands. Recorded
/// in `bundle.json` as `pending_cells[]` so the packaging itself
/// carries the honesty record.
pub const PENDING_CELLS: &[(&str, &str, &str, &[CandidateEvidence])] = &[
    (
        "identity-trees.foreign-replay",
        "DF-S1.2-2",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    (
        "identity-records.foreign-replay",
        "DF-S1.2-2",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    (
        "ledger-transcript.foreign-replay",
        "DF-S1.5-3",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    (
        "registry-snapshot.foreign-replay",
        "DF-S1.8-1",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    (
        "plugin-manifests.foreign-checker",
        "DF-S1.27-1",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    (
        "canonical-parse.foreign-timing",
        "DF-S0.3-2",
        "HUMAN-H3",
        &[E2_RUN, E3_RUN],
    ),
    ("polyglot-ci.two-toolchain", "DF-S0.3-2", "HUMAN-H3", &[]),
    // R4.6 discharged `bench-manifest.live-import` (DF-S1.24-2) and
    // `lt-03.live-corpus-arm` (DF-S1.13-3) — both executed live under
    // H3's "Toolchains + online corpus" authorization; the
    // `hh-live-evidence/1` records are `…/live/2026-10-21-lt03-live.json`
    // and `…-bench-manifest-live.json`.
    (
        "hir-documents.e2-replay",
        "DF-S1.2-2",
        "e2-tree arm extension",
        &[],
    ),
    (
        "hir-documents.e3-replay",
        "DF-S1.2-2",
        "HUMAN-H3",
        &[E3_RUN],
    ),
    (
        "ledger-scenarios.e2-replay",
        "DF-S1.5-3",
        "e2-tree arm extension",
        &[],
    ),
    (
        "ledger-scenarios.e3-replay",
        "DF-S1.5-3",
        "HUMAN-H3",
        &[E3_RUN],
    ),
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
