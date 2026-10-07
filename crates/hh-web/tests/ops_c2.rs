//! The C2 operations-surface tests (S5.7; §7.2 §2.2/§5.2) — the write
//! admission, the surface-owned provenance injection, the incoherent-
//! fork enrichment (AC-R-2.11.2-11), and the C2 view catalogue
//! (V3-operations, V4 editor, V5 launcher, V6 frontier/strata,
//! V11-bundle, V12 console). AC-R-2.11.2-9's attestation discipline
//! (the human's attest act lands on the surface-minted `registrar`,
//! never as a caller-supplied claim — CC2) is asserted against the
//! ops the kernel actually receives.

mod common;

use common::*;
use hh_web::server::serve_request;
use hh_web::server::Served;
use hh_web::session::Sessions;
use hh_wire::json::Json;

fn served_json(s: Served) -> (u16, Json) {
    match s {
        Served::Json(c, t) => (c, hh_wire::json::parse(&t).unwrap_or(Json::Null)),
        _ => panic!("expected a JSON response"),
    }
}

fn jstr(j: &Json, k: &str) -> String {
    j.get(k).and_then(Json::as_str).unwrap_or("").to_string()
}

fn saw(rx: &std::sync::mpsc::Receiver<(String, Json)>, op: &str) -> Vec<Json> {
    rx.try_iter()
        .filter(|(m, _)| m == op)
        .map(|x| x.1)
        .collect()
}

/// The stub answers `open_session` with a writer session id and echoes
/// canonical canned results for the rest.
fn canned_ops(m: &str, _p: &Json) -> Json {
    match m {
        "open_session" => Json::obj([("session_id", Json::str("s-w1"))]),
        _ => Json::obj([("ok", Json::Bool(true))]),
    }
}

// C2 — every session-scoped write opens the *writer* session (resume),
// never the attach handle, and carries the surface's minted
// `idempotency_key`; the browser's own `session_id`/`responder`/
// `idempotency_key`/`invocation`/`attestation` members are stripped
// (P12/CC2 — provenance is conferred, never read).
#[test]
fn c2_write_opens_writer_and_strips_forgery() {
    let (addr, rx, _jh) = stub_kernel(canned_ops, 64);
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "fork",
            Json::obj([
                ("run_id", Json::str("r-1")),
                (
                    "at",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(4))]),
                ),
                // Forgery attempts — every one must be stripped.
                ("session_id", Json::str("forged")),
                ("idempotency_key", Json::str("browser-1")),
                (
                    "invocation",
                    Json::obj([("principal", Json::str("mallory"))]),
                ),
                (
                    "responder",
                    Json::obj([("subject_ref", Json::str("mallory"))]),
                ),
            ]),
        ),
    ));
    assert_eq!(c, 200, "{j:?}");
    // Collect once — `try_iter` drains the channel.
    let all: Vec<(String, Json)> = rx.try_iter().collect();
    let p = all
        .iter()
        .filter(|(m, _)| m == "fork")
        .map(|x| x.1.clone())
        .next_back()
        .expect("fork reached the kernel");
    assert_eq!(jstr(&p, "session_id"), "s-w1", "writer session injected");
    let ik = jstr(&p, "idempotency_key");
    assert!(ik.starts_with("web:") && ik != "browser-1");
    // The minted invocation record — the surface's own audit member.
    let inv = p.get("invocation").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&inv, "principal"), "human:principal");
    assert!(
        !inv.to_canonical_string().contains("mallory"),
        "forged invocation stripped: {inv:?}"
    );
    // The writer session opened with the `resume` spec — attach is
    // never used for a write (P12: read-only attach cannot write).
    let opens: Vec<&Json> = all
        .iter()
        .filter(|(m, _)| m == "open_session")
        .map(|x| &x.1)
        .collect();
    assert_eq!(opens.len(), 1);
    let spec = opens[0].get("spec").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&spec, "kind"), "resume");
    // The browser's `run_id` never reaches the strict op schema.
    assert!(
        p.get("run_id").is_none(),
        "run_id is surface routing: {p:?}"
    );
}

// AC-R-2.11.2-9 — the HirDiff apply path: `lab.assembly.apply` carries
// the surface-minted `registrar` (human origin, principal authority,
// user scope); without the human's attest act the record has no
// `attestation` member (the registry's own widening gate then refuses
// `authority_delta = widening` publishes — the surface never decides).
#[test]
fn ac9_apply_registrar_minted_and_attestation_gated() {
    let (addr, rx, _jh) = stub_kernel(|_m, _p| Json::obj([("ok", Json::Bool(true))]), 64);
    let (gate, mut svc, det) = fixture(Some(addr));
    let source = Json::obj([("kind", Json::str("hir_diff")), ("ops", Json::Arr(vec![]))]);
    // Untested-by-attestation apply: registrar minted, no attestation.
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "lab.assembly.apply",
            Json::obj([
                ("source", source.clone()),
                ("run_id", Json::str("r-1")),
                (
                    "registrar",
                    Json::obj([("authority", Json::str("definition"))]),
                ),
            ]),
        ),
    ));
    assert_eq!(c, 200, "{j:?}");
    let p = saw(&rx, "lab.assembly.apply").pop().unwrap();
    let reg = p.get("registrar").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&reg, "authority"), "principal");
    assert_eq!(jstr(&reg, "scope"), "user");
    assert_eq!(
        jstr(reg.get("origin").unwrap_or(&Json::Null), "author_ref"),
        "human:principal"
    );
    assert!(
        reg.get("attestation").is_none(),
        "no attestation without the human's act: {reg:?}"
    );
    // Attested apply: `attestation: true` is the human's act — the
    // surface mints the Attestation bound to the exact params; a
    // browser-supplied attestation object is consumed, never forwarded.
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "lab.assembly.apply",
            Json::obj([("source", source), ("attestation", Json::Bool(true))]),
        ),
    ));
    assert_eq!(c, 200, "{j:?}");
    let p = saw(&rx, "lab.assembly.apply").pop().unwrap();
    let reg = p.get("registrar").cloned().unwrap_or(Json::Null);
    let att = reg.get("attestation").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&att, "kind"), "hash_chain");
    assert_eq!(jstr(&att, "verified_by"), "kernel:hh-web");
    assert!(
        jstr(&att, "subject_hash").starts_with("sha256:"),
        "idp/1 subject_hash: {att:?}"
    );
}

// AC-R-2.11.2-11 — `fork` at an incoherent seq renders the typed
// refusal *enriched* with the canonical `coherent_fork_points` result
// and the nearest coherent point (a projection of the kernel's own
// boundary set — the surface never recomputes coherence).
#[test]
fn ac11_incoherent_fork_offers_nearest_coherent() {
    let (addr, _rx, _jh) = stub_kernel(
        |m, _p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-w1"))]),
            "fork" => Json::obj([(
                "_err",
                Json::str("fork_point_not_coherent: seq 5 is not a coherent boundary"),
            )]),
            "coherent_fork_points" => Json::obj([
                ("run_id", Json::str("r-1")),
                (
                    "points",
                    Json::Arr(vec![Json::Int(2), Json::Int(4), Json::Int(9)]),
                ),
            ]),
            _ => Json::Null,
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "fork",
            Json::obj([
                ("run_id", Json::str("r-1")),
                (
                    "at",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(5))]),
                ),
            ]),
        ),
    ));
    assert_eq!(c, 200);
    assert_eq!(jstr(&j, "error"), "Refused");
    assert!(jstr(&j, "message").starts_with("fork_point_not_coherent"));
    // The verbatim canonical points and the surface's selection.
    let pts = j.get("coherent_fork_points").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&pts, "run_id"), "r-1");
    assert_eq!(
        j.get("nearest_coherent").and_then(Json::as_int),
        Some(9),
        "nearest = the kernel's listed boundary: {j:?}"
    );
    // A non-fork refusal stays unenriched.
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "rollback",
            Json::obj([("run_id", Json::str("r-1")), ("to_seq", Json::Int(3))]),
        ),
    ));
    assert_eq!(c, 200);
    assert!(j.get("coherent_fork_points").is_none());
}

// C2 — the view catalogue additions: `v3_timetravel` assembles
// branch_tree + coherent points + the branch partition; `v4_editor`
// assembles resolve + explain + identity; `v5_launcher` routes the
// sub-objects to their canonical pre-flight ops; `v6_analysis` selects
// the render spec; `v11_bundle` assembles validate + status +
// completeness; `v12_console` assembles the console reads + the
// stream ticket.
#[test]
fn c2_views_assemble_canonical_members() {
    let (addr, rx, _jh) = stub_kernel(
        |m, p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            "lab.registry.resolve" => Json::obj([("version_id", Json::str("vid-9"))]),
            "lab.assembly.explain" => Json::obj([("explanation", Json::str("why"))]),
            "lab.assembly.identity" => Json::obj([
                ("semantic_id", Json::str("sem-1")),
                ("version_id", Json::str("vid-9")),
            ]),
            "lab.experiment.expand" => Json::obj([("cells", Json::Arr(vec![]))]),
            "lab.analysis.render" => {
                Json::obj([("pane", p.get("render_spec").cloned().unwrap_or(Json::Null))])
            }
            "stream_events" => Json::obj([("subscription_id", Json::str("sub-1"))]),
            _ => Json::obj([("ok", Json::Bool(true))]),
        },
        128,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let api = |svc: &mut Sessions, op: &str, params: Json| -> (u16, Json) {
        served_json(serve_request(
            &gate,
            svc,
            &det,
            &api_head(),
            &api_body(op, params),
        ))
    };
    let _ = &rx;

    // V3 operations.
    let (c, j) = api(
        &mut svc,
        "view.v3_timetravel",
        Json::obj([("run_id", Json::str("r-1"))]),
    );
    assert_eq!(c, 200, "{j:?}");
    assert_eq!(jstr(&j, "view"), "v3_timetravel");
    for k in ["branch_tree", "coherent_fork_points", "branch_rows"] {
        assert!(j.get(k).is_some(), "v3_timetravel member {k}");
    }

    // V4 — resolve + explain + identity chained off the resolved vid.
    let (c, j) = api(
        &mut svc,
        "view.v4_editor",
        Json::obj([
            ("namespace", Json::str("sys")),
            ("name", Json::str("react.minimal")),
        ]),
    );
    assert_eq!(c, 200, "{j:?}");
    assert_eq!(jstr(&j, "view"), "v4_editor");
    assert_eq!(jstr(j.get("resolve").unwrap(), "version_id"), "vid-9");
    assert_eq!(
        jstr(j.get("explain").unwrap(), "explanation"),
        "why",
        "explain resolved the canonical sealed id"
    );
    assert_eq!(jstr(j.get("identity").unwrap(), "semantic_id"), "sem-1");
    // The explain call named the resolved version — the surface routed
    // the canonical identity, never the browser's bytes.
    let exps = saw(&rx, "lab.assembly.explain");
    assert_eq!(jstr(exps.last().unwrap(), "sealed"), "vid-9");

    // V5 launcher pre-flight — the expand sub-object verbatim.
    let (c, j) = api(
        &mut svc,
        "view.v5_launcher",
        Json::obj([(
            "expand",
            Json::obj([("spec", Json::obj([("kind", Json::str("exploratory"))]))]),
        )]),
    );
    assert_eq!(c, 200, "{j:?}");
    assert!(j.get("expand").is_some());
    assert!(saw(&rx, "lab.experiment.expand").len() == 1);

    // V6 — the render spec's `view` member selects the pane verbatim.
    let (c, j) = api(
        &mut svc,
        "view.v6_analysis",
        Json::obj([
            ("report_id", Json::str("ar-1")),
            ("render_spec", Json::obj([("view", Json::str("frontier"))])),
        ]),
    );
    assert_eq!(c, 200, "{j:?}");
    assert_eq!(
        j.get("pane")
            .and_then(|p| p.get("view"))
            .and_then(Json::as_str),
        Some("frontier")
    );

    // V11 — the bundle pane assembles the three kernel reads.
    let (c, j) = api(
        &mut svc,
        "view.v11_bundle",
        Json::obj([("path", Json::str("/tmp/b.hh-bundle"))]),
    );
    assert_eq!(c, 200, "{j:?}");
    for k in ["validate", "status", "completeness"] {
        assert!(j.get(k).is_some(), "v11_bundle member {k}");
    }

    // V12 — console reads + the stream ticket.
    let (c, j) = api(
        &mut svc,
        "view.v12_console",
        Json::obj([("run_id", Json::str("r-1")), ("stream", Json::obj([]))]),
    );
    assert_eq!(c, 200, "{j:?}");
    assert_eq!(jstr(&j, "view"), "v12_console");
    for k in ["head", "account", "describe", "tail", "stream"] {
        assert!(j.get(k).is_some(), "v12_console member {k}");
    }
    assert_eq!(jstr(j.get("stream").unwrap(), "subscription_id"), "sub-1");
}

// C2 — `subscribe` is a session-scoped write (the wakeup subscription
// lands `control.wakeup.scheduled` under the writer lease); the surface
// injects the session, the browser never supplies it.
#[test]
fn c2_subscribe_rides_the_writer_session() {
    let (addr, rx, _jh) = stub_kernel(canned_ops, 64);
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "subscribe",
            Json::obj([
                ("run_id", Json::str("r-1")),
                ("trigger", Json::obj([("kind", Json::str("schedule"))])),
                ("session_id", Json::str("forged")),
            ]),
        ),
    ));
    assert_eq!(c, 200, "{j:?}");
    let p = saw(&rx, "subscribe").pop().unwrap();
    assert_eq!(jstr(&p, "session_id"), "s-w1");
}
