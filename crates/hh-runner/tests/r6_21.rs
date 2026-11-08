//! R6.21 — SITE (b)+(d): the committed `hh-receipt-index/1` site
//! fixture validates under the landed closed codec, keeps the dataset
//! private (every trial renders `withheld{dataset_private}` — REQ-XF-132,
//! X-05), and refuses a malformed member. Offline fixtures only.

use std::path::PathBuf;

use hharness_runner::receipt_index::{validate_index, RECEIPT_INDEX_SCHEMA};
use hharness_wire::json::{parse, Json};

fn fixture() -> Json {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/site/fixtures/phase2-receipt-index.json");
    parse(&std::fs::read_to_string(p).expect("fixture readable")).expect("fixture parses")
}

#[test]
fn receipt_index_fixture_validates_private() {
    let doc = fixture();
    validate_index(&doc).expect("the committed index validates");

    let m = match &doc {
        Json::Obj(m) => m,
        _ => panic!("index is an object"),
    };
    assert_eq!(
        m.get("schema").and_then(Json::as_str),
        Some(RECEIPT_INDEX_SCHEMA)
    );
    let ds = match m.get("dataset") {
        Some(Json::Obj(d)) => d,
        _ => panic!("dataset is an object"),
    };
    assert_eq!(ds.get("visibility").and_then(Json::as_str), Some("private"));
    // private → no pinned revision; the viewer shows the sha256 and no link
    assert_eq!(ds.get("revision"), Some(&Json::Null));
    let attempts = match m.get("attempts") {
        Some(Json::Arr(a)) => a,
        _ => panic!("attempts is an array"),
    };
    assert_eq!(attempts.len(), 300, "10 tasks x 6 arms x 5 replicates");
    for a in attempts {
        let files = match a {
            Json::Obj(am) => match am.get("files") {
                Some(Json::Arr(f)) => f,
                _ => panic!("attempt.files is an array"),
            },
            _ => panic!("attempt is an object"),
        };
        assert!(
            files
                .iter()
                .any(|f| f.get("role").and_then(Json::as_str) == Some("transcript")),
            "every attempt indexes its transcript"
        );
    }
}

#[test]
fn receipt_index_fixture_refuses_mutations() {
    // a bad sha256 length refuses under the closed codec
    let mut doc = fixture();
    if let Json::Obj(m) = &mut doc {
        if let Some(Json::Arr(attempts)) = m.get_mut("attempts") {
            if let Some(Json::Obj(am)) = attempts.get_mut(0) {
                if let Some(Json::Arr(files)) = am.get_mut("files") {
                    if let Some(Json::Obj(fm)) = files.get_mut(0) {
                        fm.insert("sha256".into(), Json::str("deadbeef"));
                    }
                }
            }
        }
    }
    assert!(validate_index(&doc).is_err());

    // a public flip with a private revision stays shape-valid but is a
    // different site state — the codec only checks the closed sets
    let mut doc = fixture();
    if let Json::Obj(m) = &mut doc {
        if let Some(Json::Obj(ds)) = m.get_mut("dataset") {
            ds.insert("visibility".into(), Json::str("public"));
        }
    }
    validate_index(&doc).expect("public visibility is codec-valid");
}
