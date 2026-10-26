//! `hharness-telemetry` R3.5 acceptance — `surface_rejection_rate` computation
//! (ticket R3.5; BL-16 / DF-S1.17-1; AC-R-2.5.2-12's computation half):
//!
//! * The fold reads `action.tool.proposed` ∪ `action.tool.surface_rejected`
//!   over real durable envelopes — `surface_rejected ÷ model-emitted calls`
//!   per (profile, family); the env pattern (`proposed` then
//!   `surface_rejected` on one `tool_call_id`) dedupes to one call.
//! * `profile` joins `model.call.requested{profile_ref | model_ref
//!   .profile_ref}` by `model_call_id`; `family` is the payload member the
//!   emitters stamp from the binding's `family_id` — both read `unknown`
//!   absent, never fabricated.
//! * Empty denominators render `n/a{estimator_undefined}`; hosted
//!   participants render `n/a{class}` — never 0, never a proxy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_ledger::event::{Cursor, Direction, Event, EventEnvelope, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::participant::{Observability, ParticipantClass};
use hharness_telemetry::views::{metric_view, metric_view_scoped};
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";
const PPM: i64 = 1_000_000;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-telemetry-r35-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn read_all(s: &Store, run: &str) -> Vec<EventEnvelope> {
    s.read(run, Cursor::Seq(0), None, Direction::Fwd, 10_000)
        .unwrap()
        .events
}

fn declared_all() -> BTreeSet<Observability> {
    [
        Observability::Events,
        Observability::ModelIo,
        Observability::EndState,
        Observability::Ledger,
    ]
    .into_iter()
    .collect()
}

/// An audit-grade event (kernel producer + provenance — the class table's
/// `audit_grade` rule).
fn ev(id: &str, class: &str, scope: Scope, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope,
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
            "kernel:test",
            0,
        )),
        content_kind: None,
        payload,
    }
}

// The `turn ⊃ model_call ⊃ tool_call` chain — the ledger's scope rule
// refuses a `model_call_id` with no `turn_id`.
fn mc_scope(mc: &str) -> Scope {
    Scope {
        turn_id: Some("t-1".to_string()),
        model_call_id: Some(mc.to_string()),
        ..Default::default()
    }
}

fn tc_scope(mc: &str, tc: &str) -> Scope {
    Scope {
        turn_id: Some("t-1".to_string()),
        model_call_id: Some(mc.to_string()),
        tool_call_id: Some(tc.to_string()),
        ..Default::default()
    }
}

/// `lifecycle.turn.started` opens `t-1` — the fenced writer refuses
/// turn-scoped events whose turn never opened.
fn turn_started() -> Event {
    ev(
        "e-turn",
        "lifecycle.turn.started",
        Scope {
            turn_id: Some("t-1".to_string()),
            ..Default::default()
        },
        Json::obj([("turn_id", Json::str("t-1"))]),
    )
}

/// `model.call.requested` carrying the bound `profile_ref`.
fn requested(id: &str, mc: &str, profile: &str) -> Event {
    ev(
        id,
        "model.call.requested",
        mc_scope(mc),
        Json::obj([
            ("model_call_id", Json::str(mc)),
            ("profile_ref", Json::str(profile)),
        ]),
    )
}

/// `action.tool.proposed` — the driver/dispatch mint (family stamped off
/// the surface record; R3.5).
fn proposed(id: &str, mc: &str, tc: &str, family: Option<&str>) -> Event {
    ev(
        id,
        "action.tool.proposed",
        tc_scope(mc, tc),
        Json::obj([
            ("tool_call_id", Json::str(tc)),
            ("surface_id", Json::str(format!("surf:{tc}"))),
            ("loop_key", Json::str("lk")),
            ("family", family.map_or(Json::Null, Json::str)),
        ]),
    )
}

/// `action.tool.surface_rejected` — the parser/monitor SurfaceFailure row.
fn surface_rejected(id: &str, mc: &str, tc: &str, family: Option<&str>) -> Event {
    ev(
        id,
        "action.tool.surface_rejected",
        tc_scope(mc, tc),
        Json::obj([
            ("surface_id", Json::str(format!("surf:{tc}"))),
            ("binding_ref", Json::str(format!("surf:{tc}"))),
            ("failure_class", Json::str("unmapped_argument")),
            ("raw_call_hash", Json::str("sha256:call")),
            ("model_call_id", Json::str(mc)),
            ("rendering_ref", Json::Null),
            ("tool_call_id", Json::str(tc)),
            ("family", family.map_or(Json::Null, Json::str)),
        ]),
    )
}

fn metric_cell<'a>(v: &'a hharness_ledger::views::View, name: &str) -> &'a Json {
    v.payload
        .get("metrics")
        .and_then(|m| m.get(name))
        .unwrap_or_else(|| panic!("{name} in the view"))
}

// ── the fold — per-(profile, family) strata over real emitted rows ───────────

#[test]
fn r3_5_surface_rejection_rate_computes_per_profile_family() {
    let (mut s, run, lease) = open("srr");
    // mc-a under minimal-patch: tc-1 proposed clean; tc-2 the env pattern —
    // `proposed` opens the tool-call scope (the durable rule), then the
    // arg-eval refusal mints `surface_rejected` for the same call (the two
    // rows dedupe to one emitted call).
    // mc-b under a probed profile: tc-3 clean, tc-4 + tc-5 rejected.
    s.append(
        &run,
        &lease,
        vec![
            turn_started(),
            requested("e-req-a", "mc-a", "profile:minimal-patch@1.0"),
            proposed("e-p1", "mc-a", "tc-1", Some("native_fc")),
            proposed("e-p2", "mc-a", "tc-2", Some("native_fc")),
            surface_rejected("e-r2", "mc-a", "tc-2", Some("native_fc")),
            requested("e-req-b", "mc-b", "profile:probed@1.0"),
            proposed("e-p3", "mc-b", "tc-3", Some("split")),
            proposed("e-p4", "mc-b", "tc-4", Some("split")),
            surface_rejected("e-r4", "mc-b", "tc-4", Some("split")),
            proposed("e-p5", "mc-b", "tc-5", Some("split")),
            surface_rejected("e-r5", "mc-b", "tc-5", Some("split")),
        ],
    )
    .unwrap();
    let all = read_all(&s, &run);
    let v = metric_view(&run, &run, &declared_all(), &all, None);
    let cell = metric_cell(&v, "surface_rejection_rate");
    // 5 emitted calls, 3 rejected → 600_000 ppm.
    assert_eq!(cell.get("ppm"), Some(&Json::Int(3 * PPM / 5)));
    let strata = cell
        .get("by_profile_family")
        .expect("by_profile_family strata");
    assert_eq!(
        strata
            .get("profile:minimal-patch@1.0|native_fc")
            .and_then(Json::as_int),
        Some(PPM / 2),
        "mc-a stratum: 1 rejected of 2 emitted"
    );
    assert_eq!(
        strata
            .get("profile:probed@1.0|split")
            .and_then(Json::as_int),
        Some(2 * PPM / 3),
        "mc-b stratum: 2 rejected of 3 emitted"
    );
}

// ── the absent/unknown halves ────────────────────────────────────────────────

#[test]
fn r3_5_surface_rejection_rate_unknown_strata_honest() {
    let (mut s, run, lease) = open("srr-unk");
    // `model.call.requested` carries no `profile_ref` and the tool rows
    // carry no `family` member — both stratum axes read `unknown`, never
    // fabricated. `proposed` precedes `surface_rejected` on tc-2 (the
    // durable rule).
    s.append(
        &run,
        &lease,
        vec![
            turn_started(),
            ev(
                "e-req",
                "model.call.requested",
                mc_scope("mc-x"),
                Json::obj([("model_call_id", Json::str("mc-x"))]),
            ),
            ev(
                "e-p",
                "action.tool.proposed",
                tc_scope("mc-x", "tc-1"),
                Json::obj([
                    ("tool_call_id", Json::str("tc-1")),
                    ("surface_id", Json::str("surf:a")),
                    ("loop_key", Json::str("lk")),
                ]),
            ),
            ev(
                "e-p2",
                "action.tool.proposed",
                tc_scope("mc-x", "tc-2"),
                Json::obj([
                    ("tool_call_id", Json::str("tc-2")),
                    ("surface_id", Json::str("surf:b")),
                    ("loop_key", Json::str("lk")),
                ]),
            ),
            ev(
                "e-r",
                "action.tool.surface_rejected",
                tc_scope("mc-x", "tc-2"),
                Json::obj([
                    ("surface_id", Json::str("surf:b")),
                    ("binding_ref", Json::str("surf:b")),
                    ("failure_class", Json::str("unknown_surface")),
                    ("raw_call_hash", Json::str("sha256:call")),
                    ("model_call_id", Json::str("mc-x")),
                ]),
            ),
        ],
    )
    .unwrap();
    let all = read_all(&s, &run);
    let v = metric_view(&run, &run, &declared_all(), &all, None);
    let cell = metric_cell(&v, "surface_rejection_rate");
    assert_eq!(cell.get("ppm"), Some(&Json::Int(PPM / 2)));
    let strata = cell.get("by_profile_family").expect("strata");
    assert_eq!(
        strata.get("unknown|unknown").and_then(Json::as_int),
        Some(PPM / 2)
    );
}

#[test]
fn r3_5_surface_rejection_rate_empty_is_na_estimator() {
    let (s, run, _lease) = open("srr-empty");
    let all = read_all(&s, &run);
    let v = metric_view(&run, &run, &declared_all(), &all, None);
    let cell = metric_cell(&v, "surface_rejection_rate");
    assert_eq!(
        cell.get("na").and_then(Json::as_str),
        Some("estimator_undefined"),
        "no emitted calls — n/a, never 0"
    );
}

#[test]
fn r3_5_surface_rejection_rate_hosted_is_na_class() {
    let (mut s, run, lease) = open("srr-hosted");
    s.append(
        &run,
        &lease,
        vec![
            turn_started(),
            requested("e-req", "mc-a", "profile:p@1.0"),
            proposed("e-p", "mc-a", "tc-1", Some("native_fc")),
            proposed("e-p2", "mc-a", "tc-2", Some("native_fc")),
            surface_rejected("e-r", "mc-a", "tc-2", Some("native_fc")),
        ],
    )
    .unwrap();
    let all = read_all(&s, &run);
    let v = metric_view_scoped(
        &run,
        &run,
        ParticipantClass::Hosted,
        &declared_all(),
        &all,
        None,
    );
    let cell = metric_cell(&v, "surface_rejection_rate");
    assert_eq!(
        cell.get("na").and_then(Json::as_str),
        Some("class"),
        "a hosted participant reads n/a{{class}}, never the native ppm"
    );
}

// ── the `model_ref.profile_ref` join fallback ────────────────────────────────

#[test]
fn r3_5_surface_rejection_rate_joins_model_ref_profile() {
    let (mut s, run, lease) = open("srr-mref");
    s.append(
        &run,
        &lease,
        vec![
            turn_started(),
            ev(
                "e-req",
                "model.call.requested",
                mc_scope("mc-a"),
                Json::obj([
                    ("model_call_id", Json::str("mc-a")),
                    (
                        "model_ref",
                        Json::obj([
                            ("profile_ref", Json::str("profile:nested@1.0")),
                            ("provider_model_id", Json::str("m-1")),
                        ]),
                    ),
                ]),
            ),
            proposed("e-p", "mc-a", "tc-1", Some("native_fc")),
            surface_rejected("e-r", "mc-a", "tc-1", Some("native_fc")),
        ],
    )
    .unwrap();
    let all = read_all(&s, &run);
    let v = metric_view(&run, &run, &declared_all(), &all, None);
    let cell = metric_cell(&v, "surface_rejection_rate");
    assert_eq!(cell.get("ppm"), Some(&Json::Int(PPM)));
    let strata = cell.get("by_profile_family").expect("strata");
    assert_eq!(
        strata
            .get("profile:nested@1.0|native_fc")
            .and_then(Json::as_int),
        Some(PPM),
        "the stratum joins through model_ref.profile_ref"
    );
}
