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

use hharness_hosting::proxy::lane::{ProxyLane, Reservation};
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
        _ => Reservation::Unsupported,
    };
    if get("lane_id").is_empty()
        || get("endpoint_ref").is_empty()
        || get("upstream_host").is_empty()
        || get("model").is_empty()
    {
        return Err("lane requires lane_id/endpoint_ref/upstream_host/model".into());
    }
    Ok(ProxyLane::program(
        &get("lane_id"),
        &get("endpoint_ref"),
        &get("upstream_host"),
        &get("model"),
        reservation,
    ))
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
        root_cap_micros: doc
            .get("root_cap_micros")
            .and_then(Json::as_int)
            .unwrap_or(0),
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
