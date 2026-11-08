//! `runner_check` — the §5.7 per-harness runner check, first-pass
//! form (manifest 279/R6.26; REQ-XC-325, REQ-XC-326, REQ-XH-031;
//! design/34 §1.8 + F14 + hint 8).
//!
//! R-replay ($0) compares the O (Harbor `skip_install`) and H cells on
//! the same layer and recording: (1) configuration equality — argv
//! with secrets elided, env-name set and non-secret values,
//! config-file sha256s, image digest, detected version; (2) canonical
//! downstream request-sequence equality modulo declared masks through
//! R6.12's canonicalizer (`hh_gateway::replay` — the one canonicalizer,
//! CC1; this module adds only the declared-mask step the spec names).
//! Every record is `hh-runner-check/1` carrying the pinned label and
//! `parity: none` — it is refused as L2 parity input (REQ-XC-325) and
//! R-live is `n/a{first_pass:r_live_not_built}` (§10.1.6; RA-14 — the
//! paid descriptive check is not built).
//!
//! REQ-XD-112's pair-equality predicate is R6.46's — this module only
//! fixes and asserts the O-side participant-record *members*
//! (REQ-XH-031: the frozen original layer, the same proxy, VM and
//! verifier-channel members as H — member presence, never the
//! equality predicate), plus the R-replay-before-G-L1 precedence.

use std::collections::BTreeMap;

use hharness_gateway::replay::{
    canonicalizer_for, match_requests, MatchVerdict, RequestCanonicalizer,
};
use hharness_wire::json::{parse_provider, Json};

use crate::admission::MINI_SWE_AGENT;
use crate::json_ext::JsonExt;

/// `hh-runner-check/1` — the runner-check record.
pub const RUNNER_CHECK_SCHEMA: &str = "hh-runner-check/1";
/// The pinned label phrase (REQ-XC-325 — verbatim, never paraphrased).
pub const RUNNER_CHECK_LABEL: &str = "per-harness runner check — fidelity check, `parity: none`";
/// The $0 kind (the only kind the first pass builds).
pub const KIND_R_REPLAY: &str = "r_replay";
/// R-live's typed absence — absent machinery is `n/a`, never silent.
pub const R_LIVE_NA: &str = "n/a{first_pass:r_live_not_built}";
/// The phase-3 O participant members REQ-XH-031 fixes (the frozen
/// original layer digest plus the members O shares with H).
pub const PHASE3_O_MEMBERS: [&str; 4] = ["layer_digest", "proxy", "vm", "verifier_channel"];

/// A typed runner-check refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerCheckError {
    /// The refusal spelling.
    pub detail: String,
}
fn err(d: impl Into<String>) -> RunnerCheckError {
    RunnerCheckError { detail: d.into() }
}
impl std::fmt::Display for RunnerCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runner-check: {}", self.detail)
    }
}
impl std::error::Error for RunnerCheckError {}

/// One surface's facts for the R-replay comparison — extracted from
/// the slot/job records by the caller (the comparator is pure over
/// facts; it never re-reads a store).
#[derive(Debug, Clone, PartialEq)]
pub struct SideFacts {
    /// The agent exec argv with secrets elided at the source.
    pub argv: Vec<String>,
    /// The cell env `NAME → value`; a secret member's value is the
    /// elision tombstone (`<secret:…>`), never the material.
    pub env: BTreeMap<String, String>,
    /// `config file path → sha256` for every rendered config file.
    pub config_sha256s: BTreeMap<String, String>,
    /// The image digest the surface ran.
    pub image_digest: String,
    /// The detected harness version.
    pub detected_version: String,
    /// The downstream request bodies in call order (`slot_call_seq`).
    pub downstream_requests: Vec<Vec<u8>>,
}

/// One observed difference.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    /// The leg (`config.argv`|`config.env`|`config.files`|
    /// `config.image_digest`|`config.detected_version`|`requests`).
    pub leg: &'static str,
    /// The request index for `requests` divergences.
    pub k: Option<i64>,
    /// The member path / name.
    pub path: String,
    /// The verbatim detail.
    pub detail: String,
}

/// A declared-divergence register row — a divergence passes only when
/// one row covers it (`leg` exact, `k` absent-or-equal, `path` a
/// dotted-path prefix of the observed path).
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredDivergence {
    /// The leg.
    pub leg: String,
    /// The request index (`None` = any).
    pub k: Option<i64>,
    /// The declared path (prefix match on the observed path).
    pub path: String,
    /// The recorded reason.
    pub reason: String,
}

fn div(
    leg: &'static str,
    k: Option<i64>,
    path: impl Into<String>,
    detail: impl Into<String>,
) -> Divergence {
    Divergence {
        leg,
        k,
        path: path.into(),
        detail: detail.into(),
    }
}

fn elided(v: &str) -> bool {
    v.starts_with("<secret") || v.starts_with("provided:")
}

/// `compare_config(o, h)` — configuration equality: argv (elided at
/// the source), env-name set and non-secret values, config-file
/// sha256s, image digest, detected version.
pub fn compare_config(o: &SideFacts, h: &SideFacts) -> Vec<Divergence> {
    let mut out = Vec::new();
    if o.argv.len() != h.argv.len() {
        out.push(div(
            "config.argv",
            None,
            "length",
            format!("argv length {} vs {}", o.argv.len(), h.argv.len()),
        ));
    } else {
        for (i, (a, b)) in o.argv.iter().zip(h.argv.iter()).enumerate() {
            if a != b {
                out.push(div(
                    "config.argv",
                    None,
                    format!("[{i}]"),
                    "argv member differs",
                ));
            }
        }
    }
    // env: name set, then non-secret values (an elided member compares
    // by elision flag only — the material never lands in a record).
    for (name, ov) in &o.env {
        match h.env.get(name) {
            None => out.push(div("config.env", None, name, "env name absent on H")),
            Some(hv) => {
                if elided(ov) || elided(hv) {
                    if elided(ov) != elided(hv) {
                        out.push(div("config.env", None, name, "elision flag differs"));
                    }
                } else if ov != hv {
                    out.push(div("config.env", None, name, "non-secret value differs"));
                }
            }
        }
    }
    for name in h.env.keys() {
        if !o.env.contains_key(name) {
            out.push(div("config.env", None, name, "env name absent on O"));
        }
    }
    for (path, ov) in &o.config_sha256s {
        match h.config_sha256s.get(path) {
            None => out.push(div("config.files", None, path, "config file absent on H")),
            Some(hv) if ov != hv => out.push(div("config.files", None, path, "sha256 differs")),
            _ => {}
        }
    }
    for path in h.config_sha256s.keys() {
        if !o.config_sha256s.contains_key(path) {
            out.push(div("config.files", None, path, "config file absent on O"));
        }
    }
    if o.image_digest != h.image_digest {
        out.push(div(
            "config.image_digest",
            None,
            "image_digest",
            "digests differ",
        ));
    }
    if o.detected_version != h.detected_version {
        out.push(div(
            "config.detected_version",
            None,
            "detected_version",
            format!("{} vs {}", o.detected_version, h.detected_version),
        ));
    }
    out
}

/// A mask matches a member's dotted path when equal or a dotted suffix
/// (`"id"` drops every `….id` leaf; `"messages.0.id"` only that path).
fn mask_matches(mask: &str, path: &str) -> bool {
    path == mask || path.ends_with(&format!(".{mask}"))
}

/// `mask_body(body, masks)` — parse the request bytes at the provider
/// boundary and drop every member whose dotted path a declared mask
/// covers; the masked doc re-serializes at provider form for the
/// canonicalizer. A non-JSON body is a typed refusal.
pub fn mask_body(body: &[u8], masks: &[String]) -> Result<Vec<u8>, RunnerCheckError> {
    let mut doc = parse_provider(&String::from_utf8_lossy(body))
        .map_err(|e| err(format!("mask: request body is not JSON: {e}")))?;
    fn drop_masked(j: &mut Json, prefix: &str, masks: &[String]) {
        match j {
            Json::Obj(m) => {
                let keys: Vec<String> = m.keys().cloned().collect();
                for k in keys {
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    if masks.iter().any(|mk| mask_matches(mk, &path)) {
                        m.remove(&k);
                    } else if let Some(v) = m.get_mut(&k) {
                        drop_masked(v, &path, masks);
                    }
                }
            }
            Json::Arr(a) => {
                for (i, v) in a.iter_mut().enumerate() {
                    drop_masked(v, &format!("{prefix}.{i}"), masks);
                }
            }
            _ => {}
        }
    }
    drop_masked(&mut doc, "", masks);
    Ok(doc.to_canonical_string().into_bytes())
}

/// `compare_requests(canon, surface, o, h, masks)` — canonical
/// downstream request-sequence equality modulo the declared masks via
/// R6.12's canonicalizer: `mask_body` then [`match_requests`] per call;
/// a canonicalizer refusal on either side records as a divergence at
/// that call, never a silent pass.
pub fn compare_requests(
    canon: &dyn RequestCanonicalizer,
    surface: &str,
    o: &SideFacts,
    h: &SideFacts,
    masks: &[String],
) -> Vec<Divergence> {
    let mut out = Vec::new();
    if o.downstream_requests.len() != h.downstream_requests.len() {
        out.push(div(
            "requests",
            None,
            "length",
            format!(
                "request sequence length {} vs {}",
                o.downstream_requests.len(),
                h.downstream_requests.len()
            ),
        ));
        return out;
    }
    for (k, (ob, hb)) in o
        .downstream_requests
        .iter()
        .zip(h.downstream_requests.iter())
        .enumerate()
    {
        let (mo, mh) = match (mask_body(ob, masks), mask_body(hb, masks)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                out.push(div("requests", Some(k as i64), "mask", e.detail.clone()));
                continue;
            }
        };
        match match_requests(canon, surface, &mo, &mh) {
            Ok(MatchVerdict::Exact) | Ok(MatchVerdict::CanonicalEqual) => {}
            Ok(MatchVerdict::Diverged { first_diff_path }) => {
                out.push(div(
                    "requests",
                    Some(k as i64),
                    first_diff_path,
                    "canonical requests differ",
                ));
            }
            Err(e) => {
                out.push(div(
                    "requests",
                    Some(k as i64),
                    "canonicalizer",
                    e.detail.clone(),
                ));
            }
        }
    }
    out
}

/// `covered(d, register)` — a divergence passes only when a register
/// row covers it.
fn covered(d: &Divergence, register: &[DeclaredDivergence]) -> bool {
    register.iter().any(|r| {
        r.leg == d.leg
            && (r.k.is_none() || r.k == d.k)
            && (d.path == r.path
                || d.path.starts_with(&format!("{}.", r.path))
                || d.path.starts_with(&format!("{}[", r.path)))
    })
}

/// `build_record(harness, pin, image_identity, o_ref, h_ref, surface,
/// masks, declared, recorded_at, o, h)` — run both legs and emit the
/// `hh-runner-check/1` record. `result` is `pass` iff every divergence
/// is register-covered (an empty divergence list passes).
#[allow(clippy::too_many_arguments)]
pub fn build_record(
    harness: &str,
    pin: &str,
    image_identity: &str,
    o_ref: &str,
    h_ref: &str,
    surface: &str,
    masks: &[String],
    declared: &[DeclaredDivergence],
    recorded_at: &str,
    o: &SideFacts,
    h: &SideFacts,
) -> Result<Json, RunnerCheckError> {
    let canon = canonicalizer_for("json_canonical", &[]).map_err(|e| err(e.detail.clone()))?;
    let mut divergences = compare_config(o, h);
    divergences.extend(compare_requests(canon.as_ref(), surface, o, h, masks));
    let unregistered: Vec<&Divergence> = divergences
        .iter()
        .filter(|d| !covered(d, declared))
        .collect();
    let result = if unregistered.is_empty() {
        "pass"
    } else {
        "divergent"
    };
    let div_json = |d: &Divergence| {
        let mut m = vec![
            ("leg", Json::str(d.leg)),
            ("path", Json::str(&d.path)),
            ("detail", Json::str(&d.detail)),
        ];
        if let Some(k) = d.k {
            m.push(("k", Json::Int(k)));
        }
        Json::obj(m)
    };
    Ok(Json::obj([
        ("schema", Json::str(RUNNER_CHECK_SCHEMA)),
        ("label", Json::str(RUNNER_CHECK_LABEL)),
        ("parity", Json::str("none")),
        ("kind", Json::str(KIND_R_REPLAY)),
        ("harness", Json::str(harness)),
        ("pin", Json::str(pin)),
        ("image_identity", Json::str(image_identity)),
        (
            "cells",
            Json::obj([
                (
                    "o",
                    Json::obj([("ref", Json::str(o_ref)), ("surface", Json::str("o"))]),
                ),
                (
                    "h",
                    Json::obj([("ref", Json::str(h_ref)), ("surface", Json::str("h"))]),
                ),
            ]),
        ),
        (
            "requests",
            Json::obj([
                ("canonicalizer", Json::str(canon.id())),
                (
                    "masks",
                    Json::Arr(masks.iter().map(|m| Json::str(m.clone())).collect()),
                ),
                ("calls", Json::Int(o.downstream_requests.len() as i64)),
            ]),
        ),
        (
            "declared_divergences",
            Json::Arr(
                declared
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("leg", Json::str(&r.leg)),
                            ("path", Json::str(&r.path)),
                            ("reason", Json::str(&r.reason)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "divergences",
            Json::Arr(divergences.iter().map(div_json).collect()),
        ),
        (
            "unregistered_divergences",
            Json::Arr(unregistered.iter().map(|d| div_json(d)).collect()),
        ),
        ("result", Json::str(result)),
        ("r_live", Json::str(R_LIVE_NA)),
        ("recorded_at", Json::str(recorded_at)),
    ]))
}

/// A typed validator failure over a `hh-runner-check/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckFailure {
    /// The failed leg.
    pub check: &'static str,
    /// The verbatim detail.
    pub detail: String,
}
fn fail(check: &'static str, detail: impl Into<String>) -> CheckFailure {
    CheckFailure {
        check,
        detail: detail.into(),
    }
}

/// `check_runner_check(rec)` — the record-level validator: the pinned
/// label phrase verbatim, `parity: none`, R-live's typed `n/a`, and a
/// `result` consistent with `unregistered_divergences`.
pub fn check_runner_check(rec: &Json) -> Vec<CheckFailure> {
    let mut out = Vec::new();
    let m = match rec.as_obj() {
        Some(m) => m,
        None => return vec![fail("record", "record must be an object")],
    };
    if m.get("schema").and_then(Json::as_str) != Some(RUNNER_CHECK_SCHEMA) {
        out.push(fail("schema", "schema is not hh-runner-check/1"));
    }
    if m.get("label").and_then(Json::as_str) != Some(RUNNER_CHECK_LABEL) {
        out.push(fail(
            "label",
            "label is not the pinned phrase — REQ-XC-325 requires verbatim",
        ));
    }
    if m.get("parity").and_then(Json::as_str) != Some("none") {
        out.push(fail("parity", "parity must be none"));
    }
    match m.get("r_live").and_then(Json::as_str) {
        Some(s) if s.starts_with("n/a{") => {}
        _ => out.push(fail(
            "r_live",
            "r_live must be typed n/a — the first pass builds none",
        )),
    }
    match m.get("image_identity").and_then(Json::as_str) {
        Some("image-identical") | Some("not-image-identical") => {}
        _ => out.push(fail(
            "image_identity",
            "image_identity outside the REQ-XC-306 set",
        )),
    }
    let unreg_len = m
        .get("unregistered_divergences")
        .and_then(Json::as_arr)
        .map(|a| a.len())
        .unwrap_or(usize::MAX);
    match (m.get("result").and_then(Json::as_str), unreg_len) {
        (Some("pass"), 0) => {}
        (Some("divergent"), n) if n > 0 => {}
        (Some(r), _) => out.push(fail(
            "result",
            format!("result {r} inconsistent with {unreg_len} unregistered divergences"),
        )),
        (None, _) => out.push(fail("result", "result member absent")),
    }
    out
}

/// `refuse_l2_input(rec)` — a `hh-runner-check/1` record is refused as
/// L2 parity input (REQ-XC-325: `parity: none` by construction; two
/// stochastic runs share no randomness, so verdict agreement cannot
/// establish parity). Other schemas pass through — this is the
/// runner-check's own refusal, not an admission of foreign records.
pub fn refuse_l2_input(rec: &Json) -> Result<(), RunnerCheckError> {
    if rec.get("schema").and_then(Json::as_str) == Some(RUNNER_CHECK_SCHEMA) {
        return Err(err(
            "refused{l2_parity_input}: a parity:none runner check is never L2 parity input",
        ));
    }
    Ok(())
}

/// `check_phase3_o_members(o_rec)` — REQ-XH-031's composition leg,
/// member presence only (the REQ-XD-112 equality predicate is R6.46's
/// — never recomputed here): the phase-3 O participant record must
/// carry the frozen original `layer_digest` (a `sha256:` digest, never
/// `pending{…}`) and the `proxy`, `vm` and `verifier_channel` members
/// the phase-2 hosting path shares with H.
pub fn check_phase3_o_members(o_rec: &Json) -> Vec<CheckFailure> {
    let mut out = Vec::new();
    let m = match o_rec.as_obj() {
        Some(m) => m,
        None => return vec![fail("composition", "participant record must be an object")],
    };
    for k in PHASE3_O_MEMBERS {
        match m.get(k) {
            Some(Json::Str(s)) if !s.is_empty() && !s.starts_with("pending{") => {}
            Some(_) => out.push(fail("composition", format!("{k} is pending or empty"))),
            None => out.push(fail("composition", format!("{k} member absent"))),
        }
    }
    match m.get("layer_digest").and_then(Json::as_str) {
        Some(s) if s.starts_with("sha256:") => {}
        Some(_) => out.push(fail("layer_digest", "layer_digest is not a sha256: digest")),
        None => {}
    }
    out
}

/// `check_r_replay_precedence(records, gl1)` — REQ-XH-031's ordering:
/// original mini-swe-agent's `hh-runner-check/1` `r_replay` record
/// must exist and be dated before the G-L1 readout. A missing record
/// or one at/after the readout refuses typed.
pub fn check_r_replay_precedence(records: &[Json], gl1: &Json) -> Result<(), RunnerCheckError> {
    let gl1_at = gl1
        .get("recorded_at")
        .and_then(Json::as_str)
        .ok_or_else(|| err("gl1 readout carries no recorded_at"))?;
    let rc = records.iter().find(|r| {
        r.get("schema").and_then(Json::as_str) == Some(RUNNER_CHECK_SCHEMA)
            && r.get("kind").and_then(Json::as_str) == Some(KIND_R_REPLAY)
            && r.get("harness").and_then(Json::as_str) == Some(MINI_SWE_AGENT)
    });
    let rc = rc.ok_or_else(|| {
        err("refused{r_replay_missing}: no r_replay hh-runner-check/1 for original mini-swe-agent")
    })?;
    let at = rc
        .get("recorded_at")
        .and_then(Json::as_str)
        .ok_or_else(|| err("r_replay record carries no recorded_at"))?;
    if at >= gl1_at {
        return Err(err(format!(
            "refused{{r_replay_not_before_gl1}}: {at} ≥ G-L1 {gl1_at}"
        )));
    }
    Ok(())
}
