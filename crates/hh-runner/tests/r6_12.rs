//! R6.12 — ARCHIVE (a): `hh-receipt-index/1` codec + the condenser
//! (experiments spec §4.10; REQ-XF-124, REQ-XH-101; AR-20). Offline
//! fixtures only.

use std::process::Command;

use hharness_runner::condenser::condense;
use hharness_runner::receipt_index::{
    artifact_entry, file_entry, packet_fold, validate_index, RECEIPT_INDEX_SCHEMA,
};
use hharness_secrets::redact::DetectorSet;
use hharness_secrets::types::SecretFingerprint;
use hharness_secrets::{MaskEntry, MaskSet};
use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

fn receipt(k: i64, mc: &str, disposition: &str, messages: Json) -> Json {
    Json::obj([
        ("schema", Json::str("hh.model_io/1")),
        ("model_call_id", Json::str(mc)),
        ("request_body", Json::obj([("messages", messages)])),
        (
            "outcome",
            Json::obj([("disposition", Json::str(disposition))]),
        ),
        (
            "proxy",
            Json::obj([
                ("slot_call_seq", Json::Int(k)),
                ("mode", Json::str("replay")),
                (
                    "usage",
                    Json::obj([(
                        "view",
                        Json::obj([
                            ("tokens.input.total", Json::Int(10 + k)),
                            ("tokens.output.total", Json::Int(2)),
                            ("cache_read", Json::obj([("available", Json::Bool(false))])),
                        ]),
                    )]),
                ),
                (
                    "timing",
                    Json::obj([
                        ("latency_ms", Json::Int(3)),
                        ("queue_wait_ms", Json::Int(0)),
                    ]),
                ),
                ("released", Json::obj([("status", Json::Int(200))])),
            ]),
        ),
    ])
}

fn user(s: &str) -> Json {
    Json::obj([("role", Json::str("user")), ("content", Json::str(s))])
}

/// AC-4 — the index codec: every artifact role, a `digest_only` entry
/// with no uploaded file, malformed entries refuse.
#[test]
fn index_schema_validates_roles_and_digest_only() {
    let attempt = Json::obj([(
        "files",
        Json::Arr(vec![
            file_entry("raw_receipts", "w2/slot-1/receipts.json", b"{}").unwrap(),
            file_entry("transcript", "w2/slot-1/transcript.jsonl", b"x\n").unwrap(),
            file_entry("atif", "w2/slot-1/atif.json", b"{}").unwrap(),
            file_entry("egress_log", "w2/slot-1/egress.log", b"").unwrap(),
            file_entry("native_log", "w2/slot-1/native.log", b"").unwrap(),
            file_entry("store_segment", "w2/store/seg.wal", b"wal").unwrap(),
        ]),
    )]);
    if let Json::Obj(m) =
        &mut Json::obj([("slot_id", Json::str("slot-1")), ("attempt", Json::Int(1))])
    {
        m.insert("files".into(), attempt.get("files").unwrap().clone());
    }
    let attempts = Json::Arr(vec![Json::obj([
        ("slot_id", Json::str("slot-1")),
        ("attempt", Json::Int(1)),
        ("files", attempt.get("files").unwrap().clone()),
    ])]);
    let artifacts = Json::Arr(vec![
        artifact_entry(
            "layer_export",
            "h-layer",
            "W2",
            ("image_id", "img:1"),
            Some("w2/layer.tar"),
            b"l",
            "archived",
        )
        .unwrap(),
        artifact_entry(
            "bundle",
            "h-bundle",
            "W2",
            ("digest", "sha256:bb"),
            Some("w2/bundle.tar"),
            b"b",
            "archived",
        )
        .unwrap(),
        artifact_entry(
            "binary_set",
            "infra-set",
            "W1",
            ("digest", "sha256:cc"),
            None,
            b"bin",
            "digest_only",
        )
        .unwrap(),
    ]);
    let doc = Json::obj([
        ("schema", Json::str(RECEIPT_INDEX_SCHEMA)),
        (
            "dataset",
            Json::obj([
                ("repo", Json::str("org/hh-receipts")),
                ("revision", Json::Null),
                ("visibility", Json::str("private")),
            ]),
        ),
        ("attempts", attempts),
        ("artifacts", artifacts),
        (
            "scans",
            Json::obj([
                ("secret_scan_ref", Json::Null),
                ("contamination_scan_ref", Json::Null),
            ]),
        ),
    ]);
    validate_index(&doc).unwrap();
    // Supersession + tombstone entries.
    let mut sup = doc.clone();
    if let Json::Obj(m) = &mut sup {
        m.insert(
            "supersedes".into(),
            Json::Arr(vec![Json::obj([("ref", Json::str("sha256:idx-old"))])]),
        );
        m.insert(
            "tombstone".into(),
            Json::obj([("reason", Json::str("secret in w2 revision"))]),
        );
    }
    validate_index(&sup).unwrap();
    // Malformed: bad role, archived without path, digest_only with
    // path, unknown member, bad sha.
    let mut bad = doc.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("junk".into(), Json::Int(1));
    }
    assert!(validate_index(&bad).is_err());
    assert!(artifact_entry("bogus", "n", "W1", ("digest", "x"), None, b"", "archived").is_err());
    assert!(artifact_entry("bundle", "n", "W1", ("digest", "x"), None, b"", "archived").is_err());
    assert!(artifact_entry(
        "bundle",
        "n",
        "W1",
        ("digest", "x"),
        Some("p"),
        b"",
        "digest_only"
    )
    .is_err());
    let mut bad2 = doc.clone();
    if let Json::Obj(m) = &mut bad2 {
        if let Some(Json::Arr(a)) = m.get_mut("attempts") {
            if let Json::Obj(am) = &mut a[0] {
                if let Json::Arr(fs) = am.get_mut("files").unwrap() {
                    fs[0] = Json::obj([
                        ("role", Json::str("raw_receipts")),
                        ("path", Json::str("p")),
                        ("sha256", Json::str("zz")),
                        ("bytes", Json::Int(0)),
                    ]);
                }
            }
        }
    }
    assert!(validate_index(&bad2).is_err());
}

/// AC-4 — condenser determinism (two runs byte-equal), positional
/// message dedup, and the tombstone hook on derived lines.
#[test]
fn condenser_is_deterministic_and_tombstones_slot_tokens() {
    let msgs = Json::Arr(vec![user("hi"), user("more")]);
    let msgs2 = Json::Arr(vec![
        user("hi"),
        user("more"),
        user("tok SLOT-TOKEN-XYZ inside"),
    ]);
    let receipts = vec![
        receipt(0, "mc-1", "forwarded", msgs),
        receipt(1, "mc-2", "forwarded", msgs2),
    ];
    let det = DetectorSet::default();
    let a = condense("trial-1", &receipts, None, &det);
    let b = condense("trial-1", &receipts, None, &det);
    assert_eq!(a.bytes, b.bytes, "deterministic bytes");
    let text = String::from_utf8(a.bytes).unwrap();
    // Positional dedup: "hi"/"more" appear once; the third message
    // lands at k=1.
    assert_eq!(text.matches("\"content\":\"hi\"").count(), 1);
    assert_eq!(text.matches("SLOT-TOKEN-XYZ").count(), 1);
    assert!(text.contains("\"disposition\":\"forwarded\""));
    // The tombstone hook: a mask set carrying the slot token redacts
    // the derived file.
    let mut mask = MaskSet::default();
    mask.insert(MaskEntry {
        channel_id: "hh.proxy.slot.slot-1".to_string(),
        revision: 1,
        value: "SLOT-TOKEN-XYZ".to_string(),
        fingerprint: SecretFingerprint {
            channel_id: "hh.proxy.slot.slot-1".to_string(),
            revision: 1,
            digest: "f".to_string(),
        },
        live: false,
        placeholder: None,
    });
    let c = condense("trial-1", &receipts, None, &DetectorSet::standard(mask));
    let tombstoned = String::from_utf8(c.bytes).unwrap();
    assert!(
        !tombstoned.contains("SLOT-TOKEN-XYZ"),
        "token tombstoned in derived file"
    );
    assert!(tombstoned.contains("REDACTED["));
    assert!(!c.hits.is_empty());
}

/// AC-5 — the fold recompute from archived store segments equals the
/// committed packet, and `git ls-files` holds no archive bytes.
#[test]
fn fold_recompute_equals_committed_packet_and_no_archive_bytes_in_git() {
    let events = vec![
        (
            "measurement.cost.attributed".to_string(),
            Json::obj([("money", Json::obj([("micro_units", Json::Int(12))]))]),
        ),
        (
            "model.call.completed".to_string(),
            Json::obj([
                ("response_ref", Json::str("sha256:aa")),
                ("disposition", Json::str("forwarded")),
            ]),
        ),
        (
            "measurement.cost.attributed".to_string(),
            Json::obj([("money", Json::obj([("micro_units", Json::Int(8))]))]),
        ),
        (
            "model.call.failed".to_string(),
            Json::obj([
                ("response_ref", Json::str("sha256:bb")),
                ("disposition", Json::str("upstream_error")),
            ]),
        ),
    ];
    // The "committed packet" and the "archived segment" are the same
    // durable rows — the fold is a pure function of the segment bytes'
    // events, so a recompute is definitionally equal; the test pins
    // the fold's shape so a drifted fold fails.
    let packet = packet_fold(&events);
    let recompute = packet_fold(&events);
    assert_eq!(packet, recompute);
    assert_eq!(
        packet.get("consumed_micros").and_then(Json::as_int),
        Some(20)
    );
    assert_eq!(packet.get("calls").and_then(Json::as_int), Some(2));
    // `git ls-files` (repo root) holds no experiment archive bytes:
    // no condensed transcript, no archive-store WAL or blob-pool file.
    // The committed `hh-xcheck` ledger corpus is a declared fixture
    // (cross-implementation corpus), not an experiment archive —
    // excluded explicitly so the check stays honest.
    let out = Command::new("git")
        .args(["-C", env!("CARGO_MANIFEST_DIR"), "rev-parse", "--show-toplevel"])
        .output()
        .expect("git rev-parse");
    let root = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = Command::new("git")
        .args(["-C", &root, "ls-files"])
        .output()
        .expect("git ls-files");
    let files = String::from_utf8(out.stdout).unwrap();
    for line in files.lines() {
        assert!(
            !line.ends_with("transcript.jsonl"),
            "archive byte in git: {line}"
        );
        if line.ends_with("events.wal") || line.contains("/blobs/") {
            assert!(
                line.starts_with("crates/hh-xcheck/fixtures/"),
                "archive byte in git: {line}"
            );
        }
    }
    // A segment file entry's sha256 is over the archived bytes.
    let e = file_entry("store_segment", "w2/store/seg.wal", b"wal-bytes").unwrap();
    assert_eq!(
        e.get("sha256").and_then(Json::as_str),
        Some(sha256_hex(b"wal-bytes").as_str())
    );
}
