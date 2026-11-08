//! `l1_gate` — the L1 runner: legs L1.0/L1.1/L1.2, the replay-stable
//! prefix, first-divergence-fails semantics, the REQ-XD-104 equality
//! levels over `hh-l1-canon`-canonicalized upstream-bound requests,
//! and the `hh-l1-diff/1` + `hh-l1-conformance/1` records (manifest
//! 282/R6.28, K4; spec §7.4 REQ-XD-103/104/109; design/36 §1.2–§1.3).
//!
//! **L1 is a precondition gate, not parity evidence** — every record
//! carries the pinned label and `parity: "none"`; the landed
//! `parity_admission` refuses recorded model I/O as
//! `InauthenticEvidence` (evidence spec §4.4; AC-7).
//!
//! Leg semantics (spec §7.4 legs table; design/36 §1.2):
//! - **L1.0** — O twice against REPLAY(*S*) in fresh cells; defines
//!   the replay-stable prefix *s(S)*, the count of leading
//!   canonically-equal upstream-bound requests.
//! - **L1.1** — H with REPLAY(*S*) + the observation-replay executor
//!   fed O's run-a raw observations; the whole trajectory; **gate**,
//!   first divergence fails.
//! - **L1.2** — H with REPLAY(*S*) + live execution; **gate on
//!   requests 1…s(S)**: a divergence at `k ≤ s(S)` fails, `s(S)+1` is
//!   excluded (the first request that may carry a post-divergence
//!   observation), beyond is reported only.

use std::collections::BTreeMap;

use hharness_gateway::recording::RecordedCall;
use hharness_gateway::replay::{match_requests, MatchVerdict, RequestCanonicalizer};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::json_ext::JsonExt;
use crate::l1_drive::{L1Drive, SequenceReplay, ToyMini};
use crate::l1_obs::{err, L1Error, ObsReplay, RawObservation};

/// `hh-l1-diff/1` — the durable divergence record (REQ-XD-109).
pub const L1_DIFF_SCHEMA: &str = "hh-l1-diff/1";
/// `hh-l1-conformance/1` — the L1 result record (REQ-XD-109).
pub const L1_CONFORMANCE_SCHEMA: &str = "hh-l1-conformance/1";
/// The pinned label — verbatim, never paraphrased (X-08).
pub const L1_LABEL: &str = "L1 precondition gate — not parity evidence";

/// A participant identity carried by the records.
#[derive(Debug, Clone)]
pub struct L1Identity {
    /// The participant record ref.
    pub participant_ref: String,
    /// The version label.
    pub version: String,
    /// The artifact digest (`sha256:` spelling).
    pub digest: String,
}

fn identity_json(i: &L1Identity) -> Json {
    Json::obj([
        ("participant_ref", Json::str(&i.participant_ref)),
        ("version", Json::str(&i.version)),
        ("digest", Json::str(&i.digest)),
    ])
}

/// One observed divergence at one equality level.
#[derive(Debug, Clone, PartialEq)]
pub struct L1Divergence {
    /// The level (`request` | `action` | `request_count` |
    /// `termination` | `submission` | `extra_request` |
    /// `downstream` — downstream is a diagnostic, never a fail).
    pub level: &'static str,
    /// The 1-based request ordinal for per-request levels.
    pub turn: Option<usize>,
    /// The first difference path / member.
    pub path: String,
    /// The verbatim detail.
    pub detail: String,
    /// `true` when this divergence is gate-evaluated (L1.1: always;
    /// L1.2: `turn ≤ s(S)` for request-level; trajectory levels gate
    /// only on a full-length stable prefix).
    pub gated: bool,
}

fn div(
    level: &'static str,
    turn: Option<usize>,
    path: impl Into<String>,
    detail: impl Into<String>,
    gated: bool,
) -> L1Divergence {
    L1Divergence {
        level,
        turn,
        path: path.into(),
        detail: detail.into(),
        gated,
    }
}

/// `stable_prefix(canon, surface, a, b)` — L1.0's output *s(S)*: the
/// count of leading upstream-bound requests on which the two O drives
/// are canonically equal (method, path and C03-allowlisted
/// `content-type` also compared — C03's own member set).
pub fn stable_prefix(
    canon: &dyn RequestCanonicalizer,
    surface: &str,
    a: &L1Drive,
    b: &L1Drive,
) -> usize {
    let n = a.requests.len().min(b.requests.len());
    let mut s = 0usize;
    for k in 0..n {
        let (ra, rb) = (&a.requests[k], &b.requests[k]);
        let equal = ra.method == rb.method
            && ra.path == rb.path
            && ra.content_type == rb.content_type
            && !matches!(
                match_requests(canon, surface, &ra.upstream_body, &rb.upstream_body),
                Ok(MatchVerdict::Diverged { .. }) | Err(_)
            );
        if !equal {
            break;
        }
        s += 1;
    }
    s
}

/// `compare_drives(canon, surface, o, h, stable, total)` — the
/// REQ-XD-104 equality over the O reference drive and the H drive.
/// `stable` is *s(S)*: request-level divergences at `turn ≤ s` are
/// gated; at `turn == s + 1` reported `excluded` (never gated); beyond
/// reported. Trajectory levels (`request_count`, `termination`,
/// `submission`, `extra_request`) gate only when `stable == total`
/// (a partial prefix never evaluates the tail). `downstream`
/// divergences are diagnostics — `gated: false` always.
pub fn compare_drives(
    canon: &dyn RequestCanonicalizer,
    surface: &str,
    o: &L1Drive,
    h: &L1Drive,
    stable: usize,
    total: usize,
) -> Vec<L1Divergence> {
    let mut out = Vec::new();
    let n = o.requests.len().min(h.requests.len());
    for k in 0..n {
        let turn = k + 1;
        let (ro, rh) = (&o.requests[k], &h.requests[k]);
        // request level — canonicalized upstream-bound bytes + the
        // C03 members (method, path, allowlisted content-type).
        let gated = turn <= stable;
        let verdict = match_requests(canon, surface, &ro.upstream_body, &rh.upstream_body);
        match verdict {
            Ok(MatchVerdict::Exact) | Ok(MatchVerdict::CanonicalEqual) => {}
            Ok(MatchVerdict::Diverged { first_diff_path }) => out.push(div(
                "request",
                Some(turn),
                first_diff_path,
                "canonicalized upstream-bound requests differ",
                gated,
            )),
            Err(e) => out.push(div(
                "request",
                Some(turn),
                "canonicalizer",
                e.detail.clone(),
                gated,
            )),
        }
        if ro.method != rh.method || ro.path != rh.path || ro.content_type != rh.content_type {
            out.push(div(
                "request",
                Some(turn),
                "c03",
                "method/path/content-type differ",
                gated,
            ));
        }
        // action level — H's parsed actions vs O's executed list.
        let (oa, ha) = (
            o.executed_actions.get(k).cloned().unwrap_or_default(),
            h.parsed_actions.get(k).cloned().unwrap_or_default(),
        );
        if oa != ha {
            out.push(div(
                "action",
                Some(turn),
                "actions",
                format!(
                    "executed {} vs parsed {} actions differ",
                    oa.len(),
                    ha.len()
                ),
                gated,
            ));
        }
        // downstream diagnostic — never gated.
        match match_requests(canon, surface, &ro.body, &rh.body) {
            Ok(MatchVerdict::Diverged { first_diff_path }) => out.push(div(
                "downstream",
                Some(turn),
                first_diff_path,
                "downstream bytes differ (diagnostic only)",
                false,
            )),
            _ => {}
        }
    }
    // trajectory levels — gated only on a full-length prefix.
    let tg = stable >= total;
    if o.requests.len() != h.requests.len() {
        out.push(div(
            "request_count",
            None,
            "requests.len",
            format!("{} vs {}", o.requests.len(), h.requests.len()),
            tg,
        ));
    }
    if o.termination != h.termination {
        out.push(div(
            "termination",
            None,
            "termination",
            format!("{} vs {}", o.termination, h.termination),
            tg,
        ));
    }
    if o.submission != h.submission {
        out.push(div(
            "submission",
            None,
            "submission",
            "submission payloads differ",
            tg,
        ));
    }
    if h.extra_request {
        out.push(div(
            "extra_request",
            None,
            "extra_request",
            "a request past the sequence end is a fail",
            tg,
        ));
    }
    out
}

/// `L1Sequence` — one corpus sequence and its environment material:
/// the recorded calls and O's recorded raw observations for run *a*
/// (and run *b* — `None` = the same environment, a fully reproducible
/// cell, `s(S) = total`).
#[derive(Clone)]
pub struct L1Sequence {
    /// The sequence id.
    pub seq_id: String,
    /// The recorded calls (responses to serve; O reference bodies).
    pub calls: Vec<RecordedCall>,
    /// O's recorded raw observations for run *a* — also fed to H's
    /// observation-replay executor in L1.1.
    pub obs_a: Vec<RawObservation>,
    /// O's second-run environment (`None` = identical → `s = total`).
    pub obs_b: Option<Vec<RawObservation>>,
}

/// One leg's outcome.
#[derive(Debug, Clone)]
pub struct LegOutcome {
    /// `pass` | `fail`.
    pub result: &'static str,
    /// All divergences observed (gated + reported + excluded).
    pub divergences: Vec<L1Divergence>,
    /// The strict-endpoint first divergence `(turn, path)`, when the
    /// endpoint refused it.
    pub endpoint_first: Option<(usize, String)>,
    /// The captured H drive (kept for the diff record bodies).
    pub h_drive: Option<L1Drive>,
    /// The O reference drive.
    pub o_drive: Option<L1Drive>,
}

/// One sequence's three-leg outcome.
#[derive(Debug)]
pub struct SeqOutcome {
    /// The sequence id.
    pub seq_id: String,
    /// `s(S)` — L1.0's stable prefix.
    pub stable: usize,
    /// The reference request count.
    pub total: usize,
    /// L1.1.
    pub l1_1: LegOutcome,
    /// L1.2.
    pub l1_2: LegOutcome,
    /// The `hh-l1-diff/1` records emitted (one per gated divergence).
    pub diffs: Vec<Json>,
}

fn leg_result(divergences: &[L1Divergence]) -> &'static str {
    if divergences.iter().any(|d| d.gated) {
        "fail"
    } else {
        "pass"
    }
}

/// `run_sequence(seq, make_o, make_h, canon_id, cell_facts, surface,
/// recorded_at, h_id, o_id)` — orchestrate the three legs over
/// [`ToyMini`] drives and [`SequenceReplay`] endpoints: L1.0 computes
/// `s(S)`; L1.1 runs H over a strict matcher against the O run-a
/// reference with the observation-replay executor; L1.2 runs H
/// lenient with live execution, gated on requests `1…s(S)`. First
/// divergence fails — every gated divergence emits a durable
/// `hh-l1-diff/1`.
#[allow(clippy::too_many_arguments)]
pub fn run_sequence(
    seq: &L1Sequence,
    make_o: &dyn Fn(Box<dyn crate::l1_obs::L1Exec>) -> ToyMini,
    make_h: &dyn Fn(Box<dyn crate::l1_obs::L1Exec>) -> ToyMini,
    canon_id: &str,
    cell_facts: &[(String, String)],
    surface: &str,
    recorded_at: &str,
    h_id: &L1Identity,
    o_id: &L1Identity,
) -> SeqOutcome {
    let seq_id = seq.seq_id.clone();
    let canon = || {
        hharness_gateway::replay::canonicalizer_for(canon_id, cell_facts)
            .expect("declared canonicalizer")
    };
    // ── L1.0: O twice against REPLAY(S) in fresh cells ────────────
    let mut ep_a = SequenceReplay::new(seq.calls.clone());
    let mut o_a = make_o(Box::new(ObsReplay::new(seq.obs_a.clone())));
    let drive_a = o_a.drive(&mut ep_a);
    let mut ep_b = SequenceReplay::new(seq.calls.clone());
    let mut o_b = make_o(Box::new(ObsReplay::new(
        seq.obs_b.clone().unwrap_or_else(|| seq.obs_a.clone()),
    )));
    let drive_b = o_b.drive(&mut ep_b);
    let s = stable_prefix(canon().as_ref(), surface, &drive_a, &drive_b);
    let total = drive_a.requests.len().max(drive_b.requests.len());
    // The reference recording for the H legs: the recorded responses
    // with O run-a's upstream-bound bodies as the match reference.
    let mut ref_calls = seq.calls.clone();
    for (i, c) in ref_calls.iter_mut().enumerate() {
        if let Some(r) = drive_a.requests.get(i) {
            c.upstream_request = Some(r.upstream_body.clone());
        }
    }
    // ── L1.1: H over strict replay + observation-replay exec ──────
    let mut ep1 = SequenceReplay::new(ref_calls.clone()).with_matcher(canon(), surface, true);
    let mut h1 = make_h(Box::new(ObsReplay::new(seq.obs_a.clone())));
    let d1 = h1.drive(&mut ep1);
    let mut div1 = compare_drives(canon().as_ref(), surface, &drive_a, &d1, usize::MAX, total);
    if let Some((t, p)) = ep1.first_divergence() {
        if !div1
            .iter()
            .any(|d| d.turn == Some(t) && d.level == "request")
        {
            div1.push(div("request", Some(t), p, "strict endpoint refusal", true));
        }
    }
    let l1_1 = LegOutcome {
        result: leg_result(&div1),
        divergences: div1,
        endpoint_first: ep1.first_divergence(),
        h_drive: Some(d1),
        o_drive: None,
    };
    // ── L1.2: H lenient + live execution, gated on 1…s(S) ─────────
    let mut ep2 = SequenceReplay::new(ref_calls.clone()).with_matcher(canon(), surface, false);
    let mut h2 = make_h(Box::new(crate::l1_drive::LiveExec {
        timeout_ms: Some(10_000),
    }));
    let d2 = h2.drive(&mut ep2);
    let div2 = compare_drives(canon().as_ref(), surface, &drive_a, &d2, s, total);
    let l1_2 = LegOutcome {
        result: leg_result(&div2),
        divergences: div2,
        endpoint_first: ep2.first_divergence(),
        h_drive: Some(d2),
        o_drive: None,
    };
    // ── diff records: one durable record per gated divergence ─────
    let cmp = canon();
    let mut diffs = Vec::new();
    for (leg, outcome) in [("l1_1", &l1_1), ("l1_2", &l1_2)] {
        let (o_ref, h_ref) = (&drive_a, outcome.h_drive.as_ref().unwrap_or(&drive_a));
        for d in outcome.divergences.iter().filter(|d| d.gated) {
            let (oc, hc) = canon_pair(cmp.as_ref(), surface, o_ref, h_ref, d.turn);
            diffs.push(diff_record(
                &seq_id,
                leg,
                d,
                canon_id,
                o_id,
                h_id,
                &oc,
                &hc,
                recorded_at,
            ));
        }
    }
    SeqOutcome {
        seq_id,
        stable: s,
        total,
        l1_1,
        l1_2,
        diffs,
    }
}

fn canon_pair(
    canon: &dyn RequestCanonicalizer,
    surface: &str,
    o: &L1Drive,
    h: &L1Drive,
    turn: Option<usize>,
) -> (String, String) {
    let Some(t) = turn else {
        return (String::new(), String::new());
    };
    let cv = |d: &L1Drive| -> String {
        d.requests
            .get(t - 1)
            .and_then(|r| canon.canonicalize(surface, &r.upstream_body).ok())
            .map(|c| String::from_utf8_lossy(&c.bytes).into_owned())
            .unwrap_or_default()
    };
    (cv(o), cv(h))
}

/// `diff_record(…)` — the durable `hh-l1-diff/1` (design/36 §1.3):
/// sequence id, leg, turn `k`, both canonical bodies, the first-diff
/// path, the rule-set version, and both identity records.
#[allow(clippy::too_many_arguments)]
pub fn diff_record(
    seq_id: &str,
    leg: &str,
    d: &L1Divergence,
    canon_id: &str,
    o_id: &L1Identity,
    h_id: &L1Identity,
    o_canonical: &str,
    h_canonical: &str,
    recorded_at: &str,
) -> Json {
    let mut m = vec![
        ("schema", Json::str(L1_DIFF_SCHEMA)),
        ("label", Json::str(L1_LABEL)),
        ("parity", Json::str("none")),
        ("seq_id", Json::str(seq_id)),
        ("leg", Json::str(leg)),
        ("level", Json::str(d.level)),
        ("first_diff_path", Json::str(&d.path)),
        ("detail", Json::str(&d.detail)),
        ("canon_version", Json::str(canon_id)),
        ("o_identity", identity_json(o_id)),
        ("h_identity", identity_json(h_id)),
        ("o_canonical", Json::str(o_canonical)),
        ("h_canonical", Json::str(h_canonical)),
        ("recorded_at", Json::str(recorded_at)),
    ];
    if let Some(t) = d.turn {
        m.push(("k", Json::Int(t as i64)));
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// `l1_pass_id(h, canon_id, corpus_digest)` — the L1 pass id bound to
/// the exact H version/digest (REQ-XD-102's consumer pin).
pub fn l1_pass_id(h: &L1Identity, canon_id: &str, corpus_digest: &str) -> String {
    let seed = Json::obj([
        ("h_digest", Json::str(&h.digest)),
        ("h_version", Json::str(&h.version)),
        ("canon_version", Json::str(canon_id)),
        ("corpus_digest", Json::str(corpus_digest)),
        ("result", Json::str("pass")),
    ])
    .to_canonical_string();
    format!("l1-pass:sha256:{}", sha256_hex(seed.as_bytes()))
}

/// `conformance_record(…)` — the `hh-l1-conformance/1` publication:
/// result, coverage table, stable-prefix histogram, per-leg outcomes,
/// every diff record, and (on pass) the pass id bound to the exact H
/// version/digest (REQ-XD-109; contributes REQ-XD-102).
#[allow(clippy::too_many_arguments)]
pub fn conformance_record(
    seqs: &[&SeqOutcome],
    coverage: &Json,
    census_covered: usize,
    canon_id: &str,
    corpus_digest: &str,
    h_id: &L1Identity,
    o_id: &L1Identity,
    recorded_at: &str,
) -> Json {
    let result = if seqs
        .iter()
        .all(|s| s.l1_1.result == "pass" && s.l1_2.result == "pass")
    {
        "pass"
    } else {
        "fail"
    };
    let mut histogram: BTreeMap<String, i64> = BTreeMap::new();
    let mut legs = Vec::new();
    let mut diffs = Vec::new();
    for s in seqs {
        let bucket = if s.stable == s.total {
            "full".to_string()
        } else {
            format!("prefix:{}", s.stable)
        };
        *histogram.entry(bucket).or_insert(0) += 1;
        legs.push(Json::obj([
            ("seq_id", Json::str(&s.seq_id)),
            ("s", Json::Int(s.stable as i64)),
            ("total", Json::Int(s.total as i64)),
            (
                "l1_1",
                Json::obj([
                    ("result", Json::str(s.l1_1.result)),
                    (
                        "divergences",
                        Json::Int(s.l1_1.divergences.iter().filter(|d| d.gated).count() as i64),
                    ),
                ]),
            ),
            (
                "l1_2",
                Json::obj([
                    ("result", Json::str(s.l1_2.result)),
                    (
                        "divergences",
                        Json::Int(s.l1_2.divergences.iter().filter(|d| d.gated).count() as i64),
                    ),
                    (
                        "reported",
                        Json::Int(s.l1_2.divergences.iter().filter(|d| !d.gated).count() as i64),
                    ),
                ]),
            ),
        ]));
        diffs.extend(s.diffs.iter().cloned());
    }
    let mut m = vec![
        ("schema", Json::str(L1_CONFORMANCE_SCHEMA)),
        ("label", Json::str(L1_LABEL)),
        ("parity", Json::str("none")),
        ("result", Json::str(result)),
        ("h_identity", identity_json(h_id)),
        ("o_identity", identity_json(o_id)),
        ("canon_version", Json::str(canon_id)),
        ("corpus_digest", Json::str(corpus_digest)),
        (
            "coverage",
            Json::obj([
                ("branches", coverage.clone()),
                ("census_tasks", Json::Int(census_covered as i64)),
            ]),
        ),
        (
            "stable_prefix",
            Json::obj([
                (
                    "histogram",
                    Json::Obj(
                        histogram
                            .into_iter()
                            .map(|(k, v)| (k, Json::Int(v)))
                            .collect(),
                    ),
                ),
                ("sequences", Json::Int(seqs.len() as i64)),
            ]),
        ),
        ("legs", Json::Arr(legs)),
        ("diffs", Json::Arr(diffs)),
        ("recorded_at", Json::str(recorded_at)),
    ];
    if result == "pass" {
        m.push((
            "pass_id",
            Json::str(l1_pass_id(h_id, canon_id, corpus_digest)),
        ));
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// `check_l1_diff(rec)` — the record-level validator (REQ-XD-109's
/// schema test): schema, label, leg, levels and required members.
pub fn check_l1_diff(rec: &Json) -> Vec<String> {
    let mut out = Vec::new();
    let m = match rec.as_obj() {
        Some(m) => m,
        None => return vec!["record must be an object".into()],
    };
    if m.get("schema").and_then(Json::as_str) != Some(L1_DIFF_SCHEMA) {
        out.push("schema is not hh-l1-diff/1".into());
    }
    if m.get("label").and_then(Json::as_str) != Some(L1_LABEL) {
        out.push("label is not the pinned L1 phrase".into());
    }
    match m.get("leg").and_then(Json::as_str) {
        Some("l1_1") | Some("l1_2") => {}
        _ => out.push("leg must be l1_1|l1_2".into()),
    }
    match m.get("level").and_then(Json::as_str) {
        Some("request")
        | Some("action")
        | Some("request_count")
        | Some("termination")
        | Some("submission")
        | Some("extra_request")
        | Some("downstream") => {}
        _ => out.push("level outside the REQ-XD-104 set".into()),
    }
    for k in [
        "seq_id",
        "canon_version",
        "o_identity",
        "h_identity",
        "first_diff_path",
    ] {
        if m.get(k).is_none() {
            out.push(format!("{k} member absent"));
        }
    }
    if m.get("parity").and_then(Json::as_str) != Some("none") {
        out.push("parity must be none".into());
    }
    out
}

/// `check_l1_conformance(rec)` — the record-level validator: schema,
/// label, coverage table, stable-prefix histogram, pass-id presence
/// consistent with `result`, and `parity: none`.
pub fn check_l1_conformance(rec: &Json) -> Vec<String> {
    let mut out = Vec::new();
    let m = match rec.as_obj() {
        Some(m) => m,
        None => return vec!["record must be an object".into()],
    };
    if m.get("schema").and_then(Json::as_str) != Some(L1_CONFORMANCE_SCHEMA) {
        out.push("schema is not hh-l1-conformance/1".into());
    }
    if m.get("label").and_then(Json::as_str) != Some(L1_LABEL) {
        out.push("label is not the pinned L1 phrase".into());
    }
    if m.get("parity").and_then(Json::as_str) != Some("none") {
        out.push("parity must be none".into());
    }
    let result = m.get("result").and_then(Json::as_str);
    match (result, m.get("pass_id").and_then(Json::as_str)) {
        (Some("pass"), Some(pid)) if pid.starts_with("l1-pass:sha256:") => {}
        (Some("fail"), None) => {}
        (Some("pass"), _) => out.push("pass without an l1-pass id".into()),
        (Some("fail"), Some(_)) => out.push("fail carrying a pass id".into()),
        _ => out.push("result outside pass|fail".into()),
    }
    if m.get("coverage")
        .and_then(|c| c.get("branches"))
        .and_then(JsonExt::as_obj)
        .map(|b| {
            crate::l1_corpus::BM_BRANCHES
                .iter()
                .all(|(bm, _)| b.contains_key(*bm))
        })
        != Some(true)
    {
        out.push("coverage.branches must cover BM1–BM19".into());
    }
    if m.get("stable_prefix")
        .and_then(|s| s.get("histogram"))
        .and_then(JsonExt::as_obj)
        .is_none()
    {
        out.push("stable_prefix.histogram absent".into());
    }
    out
}

/// `refuse_parity_input(rec)` — an `hh-l1-*` record presented as
/// parity input refuses typed (L1 is never parity evidence; the
/// substantive refusal rides `parity_admission`'s
/// `InauthenticEvidence`, AC-7).
pub fn refuse_parity_input(rec: &Json) -> Result<(), L1Error> {
    match rec.get("schema").and_then(Json::as_str) {
        Some(L1_DIFF_SCHEMA) | Some(L1_CONFORMANCE_SCHEMA) => Err(err(
            "l1_not_parity",
            "an L1 record is a precondition gate, never parity evidence",
        )),
        _ => Ok(()),
    }
}
