//! The surface server — a serial loopback HTTP/1.1 server over the
//! shared `hh-wire::http` codec. Routing is four routes total:
//! - `GET /` — the token-entry shell (public; carries no data),
//! - `GET /app.js` / `GET /app.css` — the shell's assets (public),
//! - `POST /api` — the one API route: `{op, params}` where `op` is an
//!   admitted canonical op ([`crate::ops`]) or `view.<id>`; the gate
//!   ([`crate::gate`]) runs before routing, the sink policy + scrub
//!   ([`crate::sink`] + `hh_secrets::redact`) before responding.
//!
//! Serial by construction — one connection at a time (the instrument's
//! human is its only caller).

use std::collections::BTreeMap;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};

use hh_embed_client_generated::{ClientError, NetClient};
use hh_secrets::DetectorSet;
use hh_wire::http::{read_request, split_target, write_response, RequestHead};
use hh_wire::json::Json;

use crate::gate::{hardened_headers, GateConfig};
use crate::ops;
use crate::session::Sessions;
use crate::sink;
use crate::ui;
use crate::views;

/// The API body's cap — a browser params envelope never exceeds this.
pub const MAX_API_BODY: usize = 256 * 1024;

/// One served response.
pub enum Served {
    /// A static asset — content-type + bytes (the shell is build-fixed;
    /// no scrub needed — it carries no data).
    Static(&'static str, &'static str),
    /// A JSON payload — canonical text, already sink-enforced + scrubbed.
    Json(u16, String),
    /// A bare status (head/body refusals — no body, P2's no-detail rule).
    Bare(u16),
}

fn write_served(served: Served, stream: &mut TcpStream) {
    match served {
        Served::Static(ct, body) => {
            let mut headers: Vec<(&str, &str)> = vec![("Content-Type", ct)];
            headers.extend(hardened_headers());
            let _ = write_response(stream, 200, &headers, body.as_bytes());
        }
        Served::Json(status, body) => {
            let mut headers: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
            headers.extend(hardened_headers());
            let _ = write_response(stream, status, &headers, body.as_bytes());
        }
        Served::Bare(status) => {
            let headers = hardened_headers();
            let _ = write_response(stream, status, &headers, b"");
        }
    }
}

/// Sink-enforce + scrub + serialize (AC-6(g): every response — data or
/// refusal — sweeps the detector set; the tombstone never echoes the
/// value it replaced).
fn js(status: u16, payload: Json, detectors: &DetectorSet) -> Served {
    let text = payload.to_canonical_string();
    let (scrubbed, _hits) = hh_secrets::redact(&text, detectors);
    Served::Json(status, scrubbed)
}

fn bare(s: u16) -> Served {
    Served::Json(s, String::new())
}

/// Route + serve one parsed request — the full gate→route→sink pipeline
/// on a parsed head/body (the battery tests call this directly, no
/// socket needed). `detectors` is the surface's scrub set — every served
/// payload's canonical text sweeps `hh_secrets::redact` before leaving
/// (the kernel masks at source, the surface sweeps at the wire — SV-3
/// defence in depth).
pub fn serve_request(
    gate: &GateConfig,
    svc: &mut Sessions,
    detectors: &DetectorSet,
    head: &RequestHead,
    body: &[u8],
) -> Served {
    let (path, _q) = split_target(&head.target);
    // The public shell — exactly the token-entry assets (P2's one
    // exception; nothing here is data).
    let public_shell = matches!(
        (head.method.as_str(), path),
        ("GET", "/") | ("GET", "/app.js") | ("GET", "/app.css")
    );
    if public_shell {
        if let Err(s) = gate.check(head, false, true) {
            return bare(s);
        }
        return match path {
            "/" => Served::Static("text/html; charset=utf-8", ui::INDEX),
            "/app.js" => Served::Static("text/javascript; charset=utf-8", ui::JS),
            _ => Served::Static("text/css; charset=utf-8", ui::CSS),
        };
    }
    let is_api = path == "/api";
    // The transport legs (host/origin/fetch-site/token — P2/P3/P9) run
    // before ANY body byte is interpreted and on every non-shell route:
    // a mis-origined or tokenless caller gets its refusal regardless of
    // what the body would have decoded to.
    if let Err(s) = gate.check(head, false, false) {
        return bare(s);
    }
    if !is_api {
        return js(
            404,
            Json::obj([("error", Json::str("not_found"))]),
            detectors,
        );
    }
    // POST /api only.
    if head.method != "POST" {
        return js(
            405,
            Json::obj([("error", Json::str("method_not_allowed"))]),
            detectors,
        );
    }
    // P4 pre-decode legs — the API admits only script-bearing JSON
    // requests: a simple HTML form (urlencoded body, no custom headers)
    // is refused here before any body byte is interpreted, mutating op
    // or not. 403 — a refused *request*, not a malformed one.
    let scripted = head
        .headers
        .get(crate::gate::SCRIPT_HEADER)
        .map(|v| v == crate::gate::SCRIPT_VALUE)
        == Some(true);
    let json_ct = head.headers.get("content-type").map(String::as_str) == Some("application/json");
    if !(scripted && json_ct) {
        return js(
            403,
            Json::obj([("error", Json::str("forbidden"))]),
            detectors,
        );
    }
    // Decode the envelope so the gate's write legs fire on the *declared
    // op's* class — a write op in a read-shaped request still runs the
    // mutation checks (never trust the method alone).
    let payload: Json = match hh_wire::json::parse(&String::from_utf8_lossy(body)) {
        Ok(j) => j,
        Err(_) => {
            return js(
                400,
                Json::obj([("error", Json::str("bad_json"))]),
                detectors,
            )
        }
    };
    let (op, params) = match &payload {
        Json::Obj(m) => (
            match m.get("op") {
                Some(Json::Str(s)) => s.clone(),
                _ => {
                    return js(
                        400,
                        Json::obj([("error", Json::str("bad_envelope"))]),
                        detectors,
                    )
                }
            },
            m.get("params")
                .cloned()
                .unwrap_or(Json::Obj(BTreeMap::new())),
        ),
        _ => {
            return js(
                400,
                Json::obj([("error", Json::str("bad_envelope"))]),
                detectors,
            )
        }
    };
    let is_view = op.starts_with("view.");
    let view_ok = is_view && views::VIEW_IDS.contains(&&op["view.".len()..]);
    let (op_class, run_scoped) = if is_view {
        if !view_ok {
            return js(403, ops::refused_payload(&op), detectors);
        }
        (ops::OpClass::Read, true)
    } else {
        match ops::classify(&op) {
            Some(c) => (c, ops::session_scoped(&op)),
            None => {
                return js(403, ops::refused_payload(&op), detectors);
            }
        }
    };
    // The write legs — the mutation legs the decode couldn't see until
    // the op classed (fetch-metadata + script/ct re-asserted).
    if let Err(s) = gate.check(head, op_class == ops::OpClass::Write, false) {
        return bare(s);
    }
    // The run scope: browser params name the run, never the session
    // (P12 — the surface opens/injects the session itself).
    let run_id = match &params {
        Json::Obj(m) => m.get("run_id").and_then(|r| match r {
            Json::Str(s) => Some(s.clone()),
            _ => None,
        }),
        _ => None,
    };
    let result = if is_view {
        views::view(svc, &op["view.".len()..], params)
    } else {
        // Every admitted op runs the injection path — session-free ops
        // (`lab.*`/`kernel.reproduce`) ignore `run_id`; a session-scoped
        // op that named none gets the kernel's own typed refusal (the
        // browser can never inject `session_id`/`responder`/`registrar`
        // past the strip — P12/CC2).
        let _ = run_scoped;
        svc.call_checked(run_id.as_deref().unwrap_or(""), &op, params)
    };
    match result {
        // P8 — the sink policy runs on every served payload (the scrub
        // inside `js` runs on the serialized text).
        Ok(r) => js(200, sink::enforce_object(&svc.sink, r), detectors),
        Err(e) => js(
            200,
            refused_enriched(svc, &op, run_id.as_deref(), &e),
            detectors,
        ),
    }
}

/// The refusal payload — the kernel's typed refusal verbatim, plus the
/// §7.2 §5.2 incoherent-fork enrichment (AC-R-2.11.2-11): a refused
/// `fork`/`branch.open` carries the canonical `coherent_fork_points`
/// result (the nearest coherent point is a projection of that list —
/// the surface never recomputes coherence).
fn refused_enriched(svc: &mut Sessions, op: &str, run_id: Option<&str>, e: &ClientError) -> Json {
    let mut payload = error_payload(e);
    let incoherent =
        matches!(e, ClientError::Rpc(ee) if ee.message.starts_with("fork_point_not_coherent"));
    if !incoherent || !matches!(op, "fork" | "branch.open") {
        return payload;
    }
    let Some(rid) = run_id else { return payload };
    let Ok(pts) = svc.call_for_run(rid, "coherent_fork_points", Json::Obj(BTreeMap::new())) else {
        return payload;
    };
    if let Json::Obj(m) = &mut payload {
        // The verbatim canonical result (points are ascending seqs);
        // `nearest_coherent` is the surface's selection — the greatest
        // listed point, the kernel's own boundary set.
        m.insert("coherent_fork_points".into(), pts.clone());
        if let Json::Obj(pm) = &pts {
            if let Some(Json::Arr(points)) = pm.get("points") {
                let nearest = points.iter().filter_map(|p| p.as_int()).max().unwrap_or(0);
                m.insert("nearest_coherent".into(), Json::Int(nearest));
            }
        }
    }
    payload
}

/// The kernel's typed refusal passes through verbatim (`{error: kind,
/// message}` — the browser renders the honest typed refusal; transport
/// errors degrade to `Unavailable`).
fn error_payload(e: &ClientError) -> Json {
    let mut m = BTreeMap::new();
    match e {
        ClientError::Rpc(ee) => {
            m.insert("error".into(), Json::str(&ee.kind));
            m.insert("message".into(), Json::str(&ee.message));
        }
        ClientError::Transport(t) => {
            m.insert("error".into(), Json::str("Unavailable"));
            m.insert("message".into(), Json::str(t));
        }
        other => {
            m.insert("error".into(), Json::str("Decode"));
            m.insert("message".into(), Json::str(format!("{other:?}")));
        }
    }
    Json::Obj(m)
}

/// The serial serve loop — bind + accept + handle, one connection at a
/// time. The caller prints the bound line to stderr before this call
/// (token delivery — never to the ledger, P9).
pub fn serve(
    listener: TcpListener,
    gate: GateConfig,
    mut svc: Sessions,
    detectors: DetectorSet,
) -> std::io::Result<()> {
    for conn in listener.incoming() {
        let mut stream: TcpStream = match conn {
            Ok(s) => s,
            Err(_) => continue,
        };
        let _ = stream.set_nodelay(true);
        let served = {
            let mut r = BufReader::new(&stream);
            match read_request(&mut r, MAX_API_BODY) {
                Ok(Some((head, body))) => serve_request(&gate, &mut svc, &detectors, &head, &body),
                Ok(None) => continue,
                Err(_) => Served::Bare(400),
            }
        };
        write_served(served, &mut stream);
    }
    Ok(())
}

/// [`serve`] bounded to `n` connections — the test/daemon variant the
/// battery uses to run the full socket path deterministically
/// (`Sessions` is `Send`: the surface owns it thread-locally).
pub fn serve_n(
    listener: TcpListener,
    gate: GateConfig,
    mut svc: Sessions,
    detectors: DetectorSet,
    n: usize,
) -> std::io::Result<usize> {
    let mut handled = 0usize;
    // `take(n)` yields at most n connections — the loop exits after the
    // nth without a trailing accept (an `incoming()` + early-break shape
    // would park on one more accept forever).
    for conn in listener.incoming().take(n) {
        let mut stream: TcpStream = match conn {
            Ok(s) => s,
            Err(_) => continue,
        };
        let _ = stream.set_nodelay(true);
        let served = {
            let mut r = BufReader::new(&stream);
            match read_request(&mut r, MAX_API_BODY) {
                Ok(Some((head, body))) => serve_request(&gate, &mut svc, &detectors, &head, &body),
                Ok(None) => continue,
                Err(_) => Served::Bare(400),
            }
        };
        write_served(served, &mut stream);
        handled += 1;
    }
    Ok(handled)
}

/// Build a `Sessions` from the kernel's bound address + token (the
/// binding (c) client half — P12: the surface's own bearer is the
/// kernel's token; the browser's is the surface's).
pub fn connect_kernel(addr: std::net::SocketAddr, token: &str) -> NetClient {
    NetClient::connect(addr, token)
}
