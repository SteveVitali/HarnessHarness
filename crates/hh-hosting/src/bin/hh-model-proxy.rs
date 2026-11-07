//! `hh-model-proxy` — the model-mediation proxy binary (experiments
//! spec §4.1; AR-08; REQ-XC-101). The runner spawns it and drives it
//! over a Json control channel on stdio: one op per line in, one
//! `{ok|error}` reply per line out.
//!
//! The process is single-threaded at the `Proxy` (`Store` is not
//! `Send`): a stdin-reader thread forwards control lines over an
//! `mpsc` channel, and `Proxy::run` on the main thread interleaves
//! control ops with the accept loop — one writer, one speaker.
//!
//! Ops: `configure{store_root, run_id, listen, lane, credential_env?,
//! root_cap_micros}` (exactly once, first op — the reply carries the
//! bound `listen`), `register_slot{slot_id, effort?, slice_micros}`,
//! `revoke_slot{slot_id}`, `status`, `stop`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use hharness_hosting::proxy::budget::ProxyAuthorization;
use hharness_hosting::proxy::lane::{ProxyLane, Reservation, ReservationFormula};
use hharness_hosting::proxy::{Proxy, ProxyConfig};
use hharness_wire::json::{parse, Json};

fn err(detail: &str) -> Json {
    Json::obj([("error", Json::obj([("detail", Json::str(detail))]))])
}

fn reply(w: &mut impl Write, j: &Json) {
    let _ = writeln!(w, "{}", j.to_canonical_string());
    let _ = w.flush();
}

fn lane_from_json(j: &Json) -> Result<ProxyLane, String> {
    let get = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let reservation = match j.get("reservation") {
        Some(r) if r.get("kind").and_then(Json::as_str) == Some("fixture_bound") => {
            Reservation::FixtureBound {
                micros: r.get("micros").and_then(Json::as_int).unwrap_or(0),
            }
        }
        Some(r) if r.get("kind").and_then(Json::as_str) == Some("worst_case_formula") => {
            let mut f = ReservationFormula::default();
            if let Some(v) = r.get("template_overhead_tokens").and_then(Json::as_int) {
                f.template_overhead_tokens = v;
            }
            if let Some(v) = r.get("max_output_bound").and_then(Json::as_int) {
                f.max_output_bound = v;
            }
            if let Some(v) = r.get("declared_body_bytes").and_then(Json::as_int) {
                f.declared_body_bytes = v;
            }
            f.cached_share_ppm = r.get("cached_share_ppm").and_then(Json::as_int);
            if let Some(v) = r.get("max_tokens_policy").and_then(Json::as_str) {
                f.max_tokens_policy = v.to_string();
            }
            Reservation::Formula(f)
        }
        _ => Reservation::Unsupported,
    };
    if get("lane_id").is_empty()
        || get("endpoint_ref").is_empty()
        || get("upstream_host").is_empty()
        || get("model").is_empty()
    {
        return Err("lane requires lane_id/endpoint_ref/upstream_host/model".into());
    }
    let mut lane = ProxyLane::program(
        &get("lane_id"),
        &get("endpoint_ref"),
        &get("upstream_host"),
        &get("model"),
        reservation,
    );
    if let Some(v) = j.get("heartbeat_ms").and_then(Json::as_int) {
        lane.heartbeat_ms = v as u64;
    }
    lane.nonstream_read_deadline_ms = j
        .get("nonstream_read_deadline_ms")
        .and_then(Json::as_int)
        .map(|v| v as u64);
    if let Some(Json::Arr(v)) = j.get("served_model_expected") {
        lane.served_model_expected = v
            .iter()
            .filter_map(Json::as_str)
            .map(|s| s.to_string())
            .collect();
    }
    lane.catalog_base = j
        .get("catalog_base")
        .and_then(Json::as_str)
        .map(|s| s.to_string());
    Ok(lane)
}

fn auth_from_json(j: &Json) -> Result<ProxyAuthorization, String> {
    let get_s = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let get_i = |k: &str| j.get(k).and_then(Json::as_int).unwrap_or(0);
    let mut prior = Vec::new();
    if let Some(Json::Arr(ws)) = j.get("prior_windows") {
        for w in ws {
            prior.push(hharness_hosting::proxy::budget::WindowSpend {
                window_id: w
                    .get("window_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .into(),
                consumed_micros: w.get("consumed_micros").and_then(Json::as_int).unwrap_or(0),
                orphan_calls: w.get("orphan_calls").and_then(Json::as_int).unwrap_or(0) as u64,
                orphan_bounded_hi_micros: w
                    .get("orphan_bounded_hi_micros")
                    .and_then(Json::as_int)
                    .unwrap_or(0),
                harvested: matches!(w.get("harvested"), Some(Json::Bool(true))),
                verified: matches!(w.get("verified"), Some(Json::Bool(true))),
            });
        }
    }
    Ok(ProxyAuthorization {
        verbatim: get_s("verbatim"),
        gate_record_ref: get_s("gate_record_ref"),
        authorized_micros: get_i("authorized_micros"),
        available_credit_micros: get_i("available_credit_micros"),
        m_ppm: get_i("m_ppm"),
        prior_windows: prior,
    })
}

fn main() {
    // Forward stdin lines into a channel — `run` drains it on the
    // owning thread while the accept loop ticks between ops.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(std::io::stdin()).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });
    // The configure handshake is synchronous: read the first line on
    // the main thread (it is `configure` or the run refuses to start),
    // configure the proxy, reply, then hand the rest of the channel to
    // `run`.
    let mut out = std::io::stdout();
    let first = rx
        .recv()
        .unwrap_or_else(|_| "{\"op\":\"stop\"}".to_string());
    let doc = match parse(first.trim()) {
        Ok(d) => d,
        Err(e) => {
            reply(&mut out, &err(&format!("parse: {e}")));
            return;
        }
    };
    if doc.get("op").and_then(Json::as_str) != Some("configure") {
        reply(&mut out, &err("first op must be configure"));
        return;
    }
    let lane = match doc.get("lane").map(lane_from_json) {
        Some(Ok(l)) => l,
        Some(Err(e)) => {
            reply(&mut out, &err(&e));
            return;
        }
        None => {
            reply(&mut out, &err("configure.lane missing"));
            return;
        }
    };
    let cfg = ProxyConfig {
        store_root: PathBuf::from(doc.get("store_root").and_then(Json::as_str).unwrap_or("")),
        run_id: doc
            .get("run_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .into(),
        lane,
        listen: doc
            .get("listen")
            .and_then(Json::as_str)
            .unwrap_or("127.0.0.1:0")
            .into(),
        credential_env: doc
            .get("credential_env")
            .and_then(Json::as_str)
            .map(|s| s.to_string()),
        authorization: match doc.get("authorization").map(auth_from_json) {
            Some(Ok(a)) => a,
            Some(Err(e)) => {
                reply(&mut out, &err(&e));
                return;
            }
            None => {
                reply(&mut out, &err("configure.authorization missing"));
                return;
            }
        },
    };
    match Proxy::configure(cfg) {
        Ok((mut p, addr)) => {
            reply(
                &mut out,
                &Json::obj([("ok", Json::obj([("listen", Json::str(addr.to_string()))]))]),
            );
            p.run(&rx, &mut out);
        }
        Err(e) => reply(&mut out, &err(&format!("{e}"))),
    }
}
