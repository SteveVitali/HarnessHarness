//! CAP5.2 — the composed execution/methods/recipient verification
//! battery (docs/tickets/239_CAP5.2__cap2.md; AC-1 positive, AC-2
//! negative): the four committed `example-card/1` records driven through
//! the landed `hharness::examples` runner (adoption → effects →
//! seal/finish → grade/settle → populations/methods → receipt →
//! clean-recipient), the AC-1 stage fold, the sixteen residual legs
//! carried verbatim with owed-work homes, and the access-validation
//! probe. Fails-without: `hharness::composed` does not exist at base
//! `a5bf3176` (compile red).
//!
//! Scratch stores only — offline scripted-lane execution; USD 0.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use hharness::composed::{run_composed, ComposedReport};
use hharness_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-cap5-2-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The real committed tree's files a scratch repo copy needs — cards,
/// pinned inputs, the residual packets and the home rows. `drop_file`
/// withholds one member (the missing-byte control).
fn stage_scratch_repo(root: &Path, drop_file: Option<&str>, forge: Option<(&str, &str, &str)>) {
    let r = repo();
    let copy = |rel: &str| {
        let dst = root.join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        let mut bytes = std::fs::read(r.join(rel)).unwrap();
        if let Some((f, from, to)) = forge {
            if rel == f {
                let text = String::from_utf8(bytes).unwrap();
                bytes = text.replacen(from, to, 1).into_bytes();
                assert_ne!(bytes, std::fs::read(r.join(rel)).unwrap());
            }
        }
        std::fs::write(&dst, bytes).unwrap();
    };
    for c in ["ex-01", "ex-02", "ex-03", "ex-04"] {
        copy(&format!("scripts/examples/{c}.json"));
    }
    copy("scripts/examples/ex-02-fixture.json");
    copy("clients/ts/fixture-definition.json");
    copy("docs/tickets/DEFERRALS.md");
    copy("docs/tickets/241_GATE-G17__accept.md");
    for e in std::fs::read_dir(r.join("docs/build/reports")).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().to_string();
        if name.starts_with("2026-11-") && name.contains("-r5.") && name.ends_with(".json") {
            if Some(name.as_str()) == drop_file {
                continue;
            }
            copy(&format!("docs/build/reports/{name}"));
        }
    }
}

static REPORT: OnceLock<ComposedReport> = OnceLock::new();

fn shared() -> &'static ComposedReport {
    REPORT.get_or_init(|| run_composed(&repo(), &tmp("shared").join("work")).unwrap())
}

fn residual<'a>(r: &'a ComposedReport, claim: &str) -> &'a hharness::composed::Residual {
    r.residuals
        .iter()
        .find(|x| x.claim == claim)
        .unwrap_or_else(|| panic!("residual {claim} absent"))
}

#[test]
fn composed_path_verifies() {
    let r = shared();
    assert_eq!(r.disposition, "executed", "{:?}", r.to_json());
    // The AC-1 spine — every stage has member checks and folds green.
    assert_eq!(r.stages.len(), 8);
    for (stage, verdict, members) in &r.stages {
        assert!(!members.is_empty(), "{stage} has no checks");
        assert_eq!(*verdict, "verified", "{stage}: {members:?}");
    }
    assert!(r.unmapped.is_empty(), "unmapped checks: {:?}", r.unmapped);
    // Counts derived from emitted details — 20 subjects, closed receipts.
    assert_eq!(r.counts.get("cards").and_then(Json::as_int), Some(4));
    assert_eq!(
        r.counts.get("subjects_settled").and_then(Json::as_int),
        Some(20)
    );
    assert!(
        r.counts
            .get("receipts_closed")
            .and_then(Json::as_int)
            .unwrap_or(0)
            >= 2
    );
    assert_eq!(r.counts.get("checks_fail").and_then(Json::as_int), Some(0));
    assert!(
        r.counts
            .get("checks_expected_refusal")
            .and_then(Json::as_int)
            .unwrap_or(0)
            >= 6
    );
    // Sixteen residual legs — every expected packet recorded verbatim.
    assert_eq!(r.residuals.len(), 16);
    for l in &r.residuals {
        assert_eq!(l.state, "recorded", "{}: {:?}", l.file, l.state);
        assert!(l.sha256.starts_with("sha256:"), "{}", l.file);
        assert_eq!(l.home_resolved, "resolved", "{}: home {}", l.file, l.home);
    }
    // The access record — a permitted probe, honest either way.
    let probe = r.access.get("backend").unwrap();
    let result = probe.get("result").and_then(Json::as_str).unwrap();
    assert!(
        result.starts_with("podman version")
            || result.starts_with("absent:")
            || result.starts_with("error:"),
        "unhonest probe result: {result}"
    );
}

#[test]
fn composed_packet_json_is_honest() {
    let r = shared();
    let j = r.to_json();
    assert_eq!(
        j.get("version").and_then(Json::as_str),
        Some("hh-cap5-composed/1")
    );
    assert_eq!(
        j.get("live_authorization").and_then(Json::as_str),
        Some("pending{not_claimed}")
    );
    assert_eq!(j.get("spend_usd").and_then(Json::as_int), Some(0));
    assert_eq!(
        j.get("disposition").and_then(Json::as_str),
        Some("executed")
    );
    let id = j.get("packet_id").and_then(Json::as_str).unwrap();
    assert!(
        id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );
    // packet_id recomputes over the canonical record minus itself.
    let mut m = j.clone();
    if let Json::Obj(ref mut mm) = m {
        mm.remove("packet_id");
    }
    let want =
        hharness::identity::idp::idp_digest("cap5_composed", m.to_canonical_string().as_bytes());
    assert_eq!(want, id);
    // The emitted text reparses byte-for-byte.
    hharness::wire::json::parse(&j.to_canonical_string()).unwrap();
}

#[test]
fn withheld_residual_never_closes() {
    let r = shared();
    // failed S1 — verbatim, pending, owed to DF-CAP5.1-1.
    let s1 = residual(r, "s1_useful");
    assert_eq!(s1.disposition, "failed");
    assert_eq!(s1.closure, "pending");
    assert_eq!(s1.home, "DF-CAP5.1-1");
    assert_eq!(s1.version, "hh-s1-useful/1");
    // The withheld stages stay withheld — never closed by the
    // composed fixture run.
    for (claim, disp) in [
        ("s3_parity", "withheld"),
        ("s4_confirm", "withheld"),
        ("outside_recipient", "withheld"),
        ("formative", "withheld"),
        ("publication", "withheld"),
        ("value_followup", "withheld"),
        ("plugin_exercise", "unselected"),
        ("interchange", "unselected"),
        ("backend_preflight", "inconclusive"),
    ] {
        let l = residual(r, claim);
        assert_eq!(l.disposition, disp, "{claim}");
        assert_eq!(l.closure, "pending", "{claim} closed?");
    }
    // Landed executed records are `recorded`, never `closed` — the
    // composed packet distinguishes landed evidence from a met claim.
    for claim in ["ops_live_second", "activation", "pilot", "study_report"] {
        assert_eq!(residual(r, claim).closure, "recorded", "{claim}");
    }
    // The committed S1 packet's own withheld claim is byte-verbatim.
    let s1_text =
        std::fs::read_to_string(repo().join("docs/build/reports").join(&s1.file)).unwrap();
    assert!(s1_text.contains("withheld{no_independent_correct_artifact}"));
}

/// AC-2: an inaccessible required byte blocks the dependent composed
/// claim — the residual leg records `blocked{inaccessible:…}` and the
/// packet disposition fails, never silently skips the leg.
#[test]
fn missing_residual_byte_blocks() {
    let root = tmp("missbyte");
    stage_scratch_repo(&root, Some("2026-11-04-r5.49-study-report.json"), None);
    let r = run_composed(&root, &root.join("work")).unwrap();
    let l = residual(&r, "study_report");
    assert!(l.state.starts_with("blocked{inaccessible"), "{:?}", l.state);
    assert_eq!(l.sha256, "none");
    assert_eq!(l.closure, "pending");
    assert_eq!(r.disposition, "failed", "{:?}", r.to_json());
    // The composed path itself stayed green — only the missing byte
    // blocks the packet verdict.
    for (stage, verdict, _) in &r.stages {
        assert_eq!(*verdict, "verified", "{stage}");
    }
}

/// AC-2: a tampered pinned artifact refuses `stale_pin` before any
/// execution — the forged S1 packet never reaches the composed fold.
#[test]
fn tampered_pinned_packet_refuses() {
    let root = tmp("tamper");
    stage_scratch_repo(
        &root,
        None,
        Some((
            "docs/build/reports/2026-11-04-r5.43-s1-useful.json",
            "\"verdict\":\"verified\"",
            "\"verdict\":\"forged\"",
        )),
    );
    let e = run_composed(&root, &root.join("work")).unwrap_err();
    assert!(format!("{e}").contains("stale_pin"), "{e}");
}

/// AC-2: the fixture labels and honest limitations ride every card —
/// a fixture pass never closes an actual S1/S4/R/P predicate.
#[test]
fn fixture_labels_retained() {
    let r = shared();
    for c in &r.cards {
        let label = c
            .card
            .get("fixture_label")
            .and_then(Json::as_str)
            .unwrap_or("");
        assert!(
            label.contains("fixture") || label.contains("scripted"),
            "{label}"
        );
        let limits = c.card.get("limitations");
        assert!(
            matches!(limits, Some(Json::Arr(a)) if !a.is_empty()),
            "{:?}",
            c.case
        );
    }
    // The packet carries the same posture top-level.
    let j = r.to_json();
    let Json::Arr(card_jsons) = j.get("cards").cloned().unwrap_or(Json::Arr(vec![])) else {
        panic!("cards not an array")
    };
    for c in &card_jsons {
        let label = c
            .get("card")
            .and_then(|x| x.get("fixture_label"))
            .and_then(Json::as_str)
            .unwrap_or("");
        assert!(label.contains("fixture") || label.contains("scripted"));
    }
}

/// Emitted packet writer — the committed artifact is produced by this
/// exact path, never hand-written (the R5.4x emit convention).
#[test]
fn composed_emit() {
    let r = shared();
    if let Ok(out) = std::env::var("HH_CAP5_COMPOSED_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo().join(out)
        };
        std::fs::write(&out, format!("{}\n", r.to_json().to_canonical_string())).unwrap();
        eprintln!("emitted {} disposition={}", out.display(), r.disposition);
    }
}
