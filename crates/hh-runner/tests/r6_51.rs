//! R6.51 (manifest 321a) — the transitive verify-host derivation from
//! captured runtime evidence (DF-R6.45l-1's discharge): the
//! `hh-verify-host-derivation/1` fold over verify-phase-attributed
//! `security.egress.decided` deny rows + captured `denied_hosts`
//! observations; the retroactive smoke1 acceptance (exactly the three
//! runtime-observed hosts, no hand-listing); the honesty legs (an
//! agent-layer deny and an unattributable deny are never derived; an
//! empty input emits `n/a{no_runtime_evidence}`); determinism.

use std::collections::{BTreeMap, BTreeSet};

use hharness_runner::json_ext::JsonExt;
use hharness_runner::verify_hosts::{
    derive_verify_hosts, CellEvidence, ReportObservation, DERIVATION_SCHEMA, NA_NO_RUNTIME_EVIDENCE,
};
use hharness_wire::json::Json;

/// A WAL envelope line (`{"k":"e","v":{event}}`) — the committed
/// mediator-store spelling.
fn wal_line(class: &str, seq: i64, payload: Json) -> String {
    let ev = Json::obj([
        ("class", Json::str(class)),
        ("seq", Json::Int(seq)),
        ("event_id", Json::str(format!("evt-{seq}"))),
        ("payload", payload),
    ]);
    Json::obj([("k", Json::str("e")), ("v", ev)]).to_canonical_string()
}

/// A `security.egress.decided` deny payload in the committed member
/// shape (`host_norm`/`host_raw`/`policy_version_id` — the Wc4 repair's
/// member table).
fn deny(host: &str, vid: &str) -> Json {
    Json::obj([
        ("decided_by", Json::str("policy")),
        ("decision", Json::str("deny")),
        ("host_norm", Json::str(host)),
        ("host_raw", Json::str(host)),
        ("method", Json::str("CONNECT")),
        ("policy_version_id", Json::str(vid)),
        ("port", Json::Int(443)),
        ("protocol", Json::str("https_connect")),
        ("reason", Json::str("not_allowed")),
        ("source", Json::str("default_unmatched")),
    ])
}

const AGENT_VID: &str = "sha256:539369ec8570abdc0e19183f2d8d2ed4d28db50f8a70f3b0c92ed96f314e46b6";
const AGENT_VID_DIAG: &str =
    "sha256:1bc08ac0b0e1c8486da2eca5729570b87d0d6b39b3c3bb4e1aae48d3c6621231";
const VERIFY_VID: &str = "sha256:33035892412f67c74786c7f9060da8335d73b8f34c443d78aac13f55f0b957fb";
const VERIFY_VID_DIAG: &str =
    "sha256:cccb98c4f337d78a7a9de07867ad06dc5089a6c00820e681f60e28755fd0716e";

/// The recorded smoke1 deny topology over the pytorch-model-cli cells:
/// 7 `download.pytorch.org` denies under the agent layer, then each
/// `verify_rows` host denied under the verify layer, then a verify
/// allow (not a deny), then the WAL checkpoint row.
fn smoke1_wal(verify_rows: &[&str], verify_vid: &str, agent_vid: &str) -> Vec<u8> {
    let mut lines = vec![wal_line(
        "lifecycle.run.created",
        0,
        Json::obj([("run_id", Json::str("run"))]),
    )];
    let mut seq = 1;
    for _ in 0..7 {
        lines.push(wal_line(
            "security.egress.decided",
            seq,
            deny("download.pytorch.org", agent_vid),
        ));
        seq += 1;
    }
    for host in verify_rows {
        lines.push(wal_line(
            "security.egress.decided",
            seq,
            deny(host, verify_vid),
        ));
        seq += 1;
    }
    lines.push(wal_line(
        "security.egress.decided",
        seq,
        Json::obj([
            ("decision", Json::str("allow")),
            ("host_norm", Json::str("pypi.org")),
            ("policy_version_id", Json::str(verify_vid)),
        ]),
    ));
    lines.push(r#"{"k":"c","n":9}"#.to_string());
    lines.join("\n").into_bytes()
}

fn cell(cell_ref: &str, vid: &str, verify_rows: &[&str], agent_vid: &str) -> CellEvidence {
    CellEvidence {
        cell: cell_ref.to_string(),
        verify_version_id: Some(vid.to_string()),
        wal: Some(smoke1_wal(verify_rows, vid, agent_vid)),
        reports: vec![],
    }
}

fn smoke1_cells() -> Vec<CellEvidence> {
    let mut h_allow = cell(
        "pytorch-model-cli/H-allow/r0",
        VERIFY_VID,
        &["download-r2.pytorch.org"],
        AGENT_VID,
    );
    h_allow.reports.push(ReportObservation {
        kind: "hh-smoke/1.cells.denied_hosts".to_string(),
        denied_hosts: vec!["download-r2.pytorch.org".to_string()],
        observed_deny_hosts: vec![
            "download-r2.pytorch.org".to_string(),
            "download.pytorch.org".to_string(),
        ],
    });
    let mut diag = cell(
        "pytorch-model-cli/H-allow-diag/r0",
        VERIFY_VID_DIAG,
        &["ossci-datasets.s3.amazonaws.com", "yann.lecun.com"],
        AGENT_VID_DIAG,
    );
    diag.reports.push(ReportObservation {
        kind: "cell-report.denied_hosts".to_string(),
        denied_hosts: vec![
            "ossci-datasets.s3.amazonaws.com".to_string(),
            "yann.lecun.com".to_string(),
        ],
        observed_deny_hosts: vec![
            "download.pytorch.org".to_string(),
            "ossci-datasets.s3.amazonaws.com".to_string(),
            "yann.lecun.com".to_string(),
        ],
    });
    vec![h_allow, diag]
}

fn str_set(j: Option<&Json>) -> BTreeSet<String> {
    j.and_then(JsonExt::as_arr)
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn excluded_hosts(rec: &Json, key: &str) -> BTreeSet<String> {
    rec.get(key)
        .and_then(JsonExt::as_arr)
        .unwrap()
        .iter()
        .filter_map(|d| d.get("host").and_then(Json::as_str).map(str::to_string))
        .collect()
}

/// AC-1 + AC-2 — the fold derives exactly the three runtime-observed
/// hosts from the smoke1-shaped evidence; per-host provenance names
/// the cell, the deciding layer's `version_id` and the deny-row count,
/// corroborated by the captured record members. `candidates`/`hosts`
/// agree; the canonical output is byte-stable across emits and parses
/// back identically. No hand-listed host anywhere in the path.
#[test]
fn smoke1_retroactive_derivation_emits_exactly_the_three_hosts() {
    let sources = BTreeMap::from([("fixture".to_string(), "sha256:0".to_string())]);
    let rec = derive_verify_hosts("smoke1", &smoke1_cells(), &sources);
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(DERIVATION_SCHEMA)
    );
    let want: BTreeSet<String> = [
        "download-r2.pytorch.org",
        "ossci-datasets.s3.amazonaws.com",
        "yann.lecun.com",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(str_set(rec.get("hosts")), want);
    let cands = rec.get("candidates").unwrap().as_arr().unwrap();
    assert_eq!(
        cands
            .iter()
            .filter_map(|c| c.get("host").and_then(Json::as_str).map(str::to_string))
            .collect::<BTreeSet<_>>(),
        want
    );
    let by_host = |h: &str| {
        cands
            .iter()
            .find(|c| c.get("host").and_then(Json::as_str) == Some(h))
            .and_then(|c| c.get("provenance").and_then(JsonExt::as_arr))
            .unwrap()
    };
    let h_allow = by_host("download-r2.pytorch.org");
    assert!(h_allow.iter().any(|p| {
        p.get("kind").and_then(Json::as_str) == Some("security.egress.decided")
            && p.get("cell").and_then(Json::as_str) == Some("pytorch-model-cli/H-allow/r0")
            && p.get("policy_version_id").and_then(Json::as_str) == Some(VERIFY_VID)
            && p.get("deny_rows").and_then(Json::as_int) == Some(1)
    }));
    assert!(h_allow
        .iter()
        .any(|p| p.get("kind").and_then(Json::as_str) == Some("hh-smoke/1.cells.denied_hosts")));
    assert!(by_host("yann.lecun.com").iter().any(|p| {
        p.get("policy_version_id").and_then(Json::as_str) == Some(VERIFY_VID_DIAG)
            && p.get("deny_rows").and_then(Json::as_int) == Some(1)
    }));
    // AC-4: the agent-layer `download.pytorch.org` deny (the X-06
    // `allowlist_gap` class) is recorded in the excluded leg — never
    // derived.
    let nv = excluded_hosts(&rec, "non_verify_denies");
    assert!(nv.contains("download.pytorch.org") && nv.len() == 1);
    let entry = rec
        .get("non_verify_denies")
        .and_then(JsonExt::as_arr)
        .unwrap()
        .iter()
        .find(|d| d.get("host").and_then(Json::as_str) == Some("download.pytorch.org"))
        .unwrap();
    assert!(entry
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap()
        .contains("excluded"));
    let obs = entry.get("observations").unwrap().as_arr().unwrap();
    for vid in [AGENT_VID, AGENT_VID_DIAG] {
        assert!(obs
            .iter()
            .any(|o| o.get("policy_version_id").and_then(Json::as_str) == Some(vid)));
    }
    assert_eq!(rec.get("na"), Some(&Json::Null));
    // Determinism + canonical round-trip.
    let canon = rec.to_canonical_string();
    assert_eq!(
        derive_verify_hosts("smoke1", &smoke1_cells(), &sources).to_canonical_string(),
        canon
    );
    assert_eq!(
        hharness_wire::json::parse(&canon)
            .unwrap()
            .to_canonical_string(),
        canon
    );
}

/// AC-4 — an agent-phase-only deny is never a candidate: the host
/// stays in the excluded leg and the candidate set is empty (evidence
/// was seen, so `na` stays null).
#[test]
fn an_agent_layer_deny_is_never_derived() {
    let cells = vec![cell("task/H-allow/r0", VERIFY_VID, &[], AGENT_VID)];
    let rec = derive_verify_hosts("w", &cells, &BTreeMap::new());
    assert!(str_set(rec.get("hosts")).is_empty());
    assert!(excluded_hosts(&rec, "non_verify_denies").contains("download.pytorch.org"));
    assert_eq!(rec.get("na"), Some(&Json::Null));
}

/// AC-4 — a WAL deny with no verify-layer attribution key lands
/// `unattributed`, never a candidate; a deny whose row carries no
/// `policy_version_id` at all lands there too.
#[test]
fn an_unattributable_deny_is_never_derived() {
    // No verify version_id declared — rows cannot be phase-attributed.
    let mut c = cell(
        "task/H-allow/r0",
        VERIFY_VID,
        &["dataset.example.com"],
        AGENT_VID,
    );
    c.verify_version_id = None;
    let rec = derive_verify_hosts("w", &[c], &BTreeMap::new());
    assert!(str_set(rec.get("hosts")).is_empty());
    let un = excluded_hosts(&rec, "unattributed_denies");
    assert!(un.contains("dataset.example.com") && un.contains("download.pytorch.org"));

    // A verify vid declared but the row carries no policy stamp.
    let wal = wal_line(
        "security.egress.decided",
        0,
        Json::obj([
            ("decision", Json::str("deny")),
            ("host_norm", Json::str("mystery.example.com")),
        ]),
    ) + &format!("\n{}", r#"{"k":"c","n":1}"#);
    let c = CellEvidence {
        cell: "task/H-allow/r0".to_string(),
        verify_version_id: Some(VERIFY_VID.to_string()),
        wal: Some(wal.into_bytes()),
        reports: vec![],
    };
    let rec = derive_verify_hosts("w", &[c], &BTreeMap::new());
    assert!(str_set(rec.get("hosts")).is_empty());
    assert!(excluded_hosts(&rec, "unattributed_denies").contains("mystery.example.com"));
}

/// AC-4 — an empty evidence input emits the empty set +
/// `n/a{no_runtime_evidence}`, not a guess.
#[test]
fn an_empty_input_emits_the_typed_na() {
    let rec = derive_verify_hosts("w", &[], &BTreeMap::new());
    assert!(str_set(rec.get("hosts")).is_empty());
    assert_eq!(
        rec.get("na").and_then(Json::as_str),
        Some(NA_NO_RUNTIME_EVIDENCE)
    );
    // A cell with no evidence at all is also `n/a`.
    let rec = derive_verify_hosts("w", &[CellEvidence::default()], &BTreeMap::new());
    assert_eq!(
        rec.get("na").and_then(Json::as_str),
        Some(NA_NO_RUNTIME_EVIDENCE)
    );
}
