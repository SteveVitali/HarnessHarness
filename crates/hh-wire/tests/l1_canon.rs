//! `hh-l1-canon/1` golden + mutation battery (R6.10 PROXY-CORE-b;
//! REQ-XC-208). Every rule carries a positive masked-difference case
//! AND an adjacent-field mutation proving the mask does not bleed —
//! the contract's per-rule shape.

use std::collections::BTreeMap;

use hharness_wire::json::{parse_preserve, PJson};
use hharness_wire::l1_canon::{L1Canon, L1_CANON_VERSION};

fn pj(s: &str) -> PJson {
    parse_preserve(s).unwrap()
}

fn canon() -> L1Canon {
    L1Canon::v1(&[
        ("kernel".into(), "5.15.0-fixture".into()),
        ("machine".into(), "arm64".into()),
        ("sysname".into(), "Linux".into()),
        ("release".into(), "5.15.0-fixture-generic".into()),
    ])
}

#[test]
fn c01_key_order_and_whitespace_mask() {
    let a = pj("{\"b\":1, \"a\":2}");
    let b = pj("{ \"a\": 2,\n\t\"b\": 1 }");
    let v = canon().compare(&a, &b);
    assert!(v.equal, "canonical reserialization masks key order");
    // mutation: a different value at an adjacent key fails
    let c = pj("{\"a\":2,\"b\":3}");
    assert!(!canon().compare(&a, &c).equal);
}

#[test]
fn c02_admitted_numeric_equality_and_boundaries() {
    let a = pj("{\"seed\":40,\"top_k\":3}");
    let b = pj("{\"seed\":40.0,\"top_k\":3}");
    let v = canon().compare(&a, &b);
    assert!(v.equal, "integral float ≡ int at admitted members");
    // mutation 1: a non-integral float is NOT masked
    let c = pj("{\"seed\":40.5,\"top_k\":3}");
    assert!(!canon().compare(&a, &c).equal);
    // mutation 2: an admitted-looking member on the never list
    // (temperature) does NOT mask — `0.7` vs `0.7` equal is fine,
    // `0.7` vs `1` must fail as unmasked (it would fail anyway, but
    // the never list also forbids `0.7` ≡ `0.7` collapsing — a pinned
    // value difference must surface).
    let d = pj("{\"temperature\":1,\"seed\":40}");
    let e = pj("{\"temperature\":1.0,\"seed\":40}");
    let v2 = canon().compare(&d, &e);
    assert!(!v2.equal, "temperature is never C02-masked");
    // mutation 3: max_tokens same guard
    let f = pj("{\"max_tokens\":512}");
    let g = pj("{\"max_tokens\":512.0}");
    assert!(!canon().compare(&f, &g).equal);
}

#[test]
fn c03_header_allowlist_drops_secret_and_telemetry() {
    let canon = canon();
    let mut h = BTreeMap::new();
    h.insert("content-type".to_string(), "application/json".to_string());
    h.insert("authorization".to_string(), "Bearer secret".to_string());
    h.insert("x-api-key".to_string(), "key".to_string());
    h.insert("user-agent".to_string(), "probe/1".to_string());
    h.insert("x-request-id".to_string(), "r1".to_string());
    h.insert("traceparent".to_string(), "t".to_string());
    let view = canon.header_view(&h);
    assert_eq!(
        view,
        vec![("content-type".to_string(), "application/json".to_string())]
    );
    // mutation: an allowlisted member still compares
    let mut h2 = h.clone();
    h2.insert("content-type".to_string(), "text/plain".to_string());
    assert_ne!(canon.header_view(&h), canon.header_view(&h2));
}

#[test]
fn c04_uname_substitutes_only_recorded_facts() {
    let a = pj("{\"observation\":{\"uname\":\"arm64\"}}");
    let b = pj("{\"observation\":{\"uname\":\"x86_64\"}}");
    // a substitutes the recorded fact; b is unrecorded — unmasked diff
    let v = canon().compare(&a, &b);
    assert!(!v.equal);
    let c = pj("{\"observation\":{\"uname\":\"arm64\"}}");
    let d = pj("{\"observation\":{\"uname\":\"arm64\"}}");
    assert!(canon().compare(&c, &d).equal);
    // a recorded fact at a NON-uname member is not masked
    let e = pj("{\"model\":\"arm64\"}");
    let f = pj("{\"model\":\"aarch64\"}");
    assert!(!canon().compare(&e, &f).equal);
}

#[test]
fn c05_stream_members_drop_and_record_mode() {
    let a = pj("{\"messages\":[],\"stream\":true,\"stream_options\":{\"x\":1}}");
    let b = pj("{\"messages\":[]}");
    let v = canon().compare(&a, &b);
    assert!(v.equal);
    assert_eq!(v.masked.iter().filter(|m| m.rule == "c05").count(), 2);
    assert_eq!(canon().view(&a).transport_mode, "streaming");
    assert_eq!(canon().view(&b).transport_mode, "buffered");
    // mutation: stream_options CONTENT never reaches compare — but a
    // non-stream member does
    let c = pj("{\"messages\":[1]}");
    assert!(!canon().compare(&a, &c).equal);
}

#[test]
fn c06_null_optional_assistant_members_absent() {
    let a = pj("{\"role\":\"assistant\",\"content\":\"hi\",\"tool_calls\":null,\"refusal\":null}");
    let b = pj("{\"role\":\"assistant\",\"content\":\"hi\"}");
    let v = canon().compare(&a, &b);
    assert!(v.equal, "null optional members ≡ absent");
    // mutation: non-null content at the same member is unmasked
    let c = pj("{\"role\":\"assistant\",\"content\":\"hi\",\"tool_calls\":[{\"id\":\"1\"}]}");
    assert!(!canon().compare(&a, &c).equal);
    // mutation: an undeclared member's null does NOT mask
    let d = pj("{\"role\":\"assistant\",\"content\":\"hi\",\"other\":null}");
    assert!(!canon().compare(&a, &d).equal);
}

#[test]
fn c07_terminus_session_fields_drop() {
    let a = pj("{\"messages\":[],\"session_id\":\"s-1\",\"tmux_session\":\"t-1\"}");
    let b = pj("{\"messages\":[]}");
    let v = canon().compare(&a, &b);
    assert!(v.equal);
    // mutation: a non-session id field does not mask
    let c = pj("{\"messages\":[],\"call_id\":\"s-1\"}");
    assert!(!canon().compare(&b, &c).equal);
}

#[test]
fn never_normalized_fields_stay_exact() {
    let canon = canon();
    for (a, b) in [
        ("{\"model\":\"m-1\"}", "{\"model\":\"m-2\"}"),
        ("{\"max_tokens\":100}", "{\"max_tokens\":101}"),
        (
            "{\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
            "{\"messages\":[{\"role\":\"user\",\"content\":\"hi \"}]}",
        ),
        (
            "{\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\"}]}",
            "{\"tool_calls\":[{\"id\":\"call_2\",\"type\":\"function\"}]}",
        ),
        (
            "{\"reasoning\":\"because\"}",
            "{\"reasoning\":\"because.\"}",
        ),
    ] {
        let v = canon.compare(&pj(a), &pj(b));
        assert!(!v.equal, "{a} vs {b} must not mask");
        assert!(v.first_unmasked.is_some());
    }
}

#[test]
fn version_stamp() {
    assert_eq!(L1_CANON_VERSION, "hh-l1-canon/1");
}
