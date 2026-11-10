//! R6.38 (REQ-XE-116; AR-24; AC-3) — `marker_detected` over the
//! settled raw output: `line = first` + `require_returncode` equality.
//! BM12 is the marker-on-the-first-line hit; BM13 is the continue side
//! (non-first-line marker, wrong returncode). The predicate reads the
//! raw output bytes — never the model-view rendering, never an
//! executor-reported hint (the environment is never consulted).
//!
//! Fixture-verified — the dispatch-path mint of `submission_ref` is
//! exercised in `hh-embed`'s battery.

use hharness_env::dispatch::{marker_detected, SubmissionMarker, SubmitLine};

const M: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

fn marker() -> SubmissionMarker {
    SubmissionMarker {
        marker: M.to_string(),
        line: SubmitLine::First,
        require_returncode: 0,
    }
}

/// BM12 — the marker on the first line of the settled raw output with
/// the required `returncode` detects submission.
#[test]
fn bm12_marker_first_line_rc_zero_detects() {
    assert!(marker_detected(
        &marker(),
        &format!("{M}\nThe rest of the submission body\nmore lines"),
        Some(0)
    ));
    // The marker alone is a first line too.
    assert!(marker_detected(&marker(), M, Some(0)));
}

/// BM13 — a non-first-line marker continues (no detection), and a
/// first-line marker with the wrong `returncode` continues.
#[test]
fn bm13_non_first_line_or_wrong_rc_continues() {
    // Marker on the second line — the declared line is `first`.
    assert!(!marker_detected(
        &marker(),
        &format!("preamble\n{M}\nrest"),
        Some(0)
    ));
    // First-line marker, wrong returncode.
    assert!(!marker_detected(&marker(), M, Some(1)));
    // First-line marker, no settled returncode at all.
    assert!(!marker_detected(&marker(), M, None));
}

/// Detection is exact — the marker literal must equal the occupied
/// line, never a substring/prefix match.
#[test]
fn marker_is_exact_line_match() {
    assert!(!marker_detected(
        &marker(),
        &format!("{M} trailing"),
        Some(0)
    ));
    assert!(!marker_detected(&marker(), &format!("prefix {M}"), Some(0)));
    // Empty output never detects.
    assert!(!marker_detected(&marker(), "", Some(0)));
}

/// A different declared literal detects against its own output — the
/// marker is the declaration's data, never a kernel constant.
#[test]
fn marker_literal_is_declaration_data() {
    let m = SubmissionMarker {
        marker: "DONE".to_string(),
        line: SubmitLine::First,
        require_returncode: 42,
    };
    assert!(marker_detected(&m, "DONE\nbody", Some(42)));
    assert!(!marker_detected(&m, "DONE\nbody", Some(0)));
    assert!(!marker_detected(&m, &format!("{M}\nbody"), Some(42)));
}
