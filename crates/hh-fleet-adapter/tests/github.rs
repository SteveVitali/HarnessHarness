//! `GithubTracker` acceptance cells (R4.7; DF-S5.6-1; AC-R-2.12.6-9):
//! the `hh.fleet.github_source/1` document drives the six
//! `work_source_adapter` ops — the capability declaration (tri-state,
//! never coerced), `list`/`get` over the bound `issues[]` snapshot
//! normalized through the one `hharness_fleet::github` mapping, `suspended`
//! (declared set + `repo_archived`), `activate_run` (open issues only),
//! verbatim `occurrences`, strict-decode refusals, and the `dispatch`
//! wire codec round-trip.

use hharness_fleet::capabilities::CapState;
use hharness_fleet_adapter::abi;
use hharness_fleet_adapter::github::GithubTracker;
use hharness_fleet_adapter::tracker::{self, TrackerLogic};
use hharness_wire::json::Json;

/// The recorded GitHub Issues shape (fixture — `tests/fixtures/`).
fn issues() -> Vec<Json> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/github-issues.json"
    ))
    .unwrap();
    match hharness_wire::json::parse(&text).unwrap() {
        Json::Obj(o) => match o.get("issues") {
            Some(Json::Arr(a)) => a.clone(),
            _ => panic!("fixture issues[] required"),
        },
        _ => panic!("fixture must be an object"),
    }
}

fn doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.github_source/1")),
        (
            "github",
            Json::obj([
                ("repo", Json::str("acme/sandbox")),
                ("source_id", Json::str("github")),
                ("owner_map", Json::obj([("octocat", Json::str("alice"))])),
                ("owner_default", Json::str("ops")),
            ]),
        ),
        ("issues", Json::Arr(issues())),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("gh-occ-1")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("github.issue")),
                (
                    "item",
                    Json::obj([
                        ("item_id", Json::str("gh-acme-sandbox-7")),
                        ("title", Json::str("Wire the webhook")),
                        (
                            "source",
                            Json::obj([
                                ("source_id", Json::str("github")),
                                ("kind", Json::str("github.issue")),
                                ("native_id", Json::str("7")),
                                ("state", Json::str("open")),
                            ]),
                        ),
                        ("idempotency_key", Json::str("idem:gh-7")),
                        ("owner", Json::str("alice")),
                    ]),
                ),
                ("observed_at_ms", Json::Int(1_000)),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
    ])
}

fn tracker() -> GithubTracker {
    GithubTracker::from_doc(doc()).unwrap()
}

fn unwrap(doc: Json) -> Json {
    let env = abi::wrap_output(&doc);
    abi::unwrap_output(&env).unwrap()
}

#[test]
fn github_tracker_capabilities_declare_the_honest_set() {
    let mut t = tracker();
    let caps = t.capabilities();
    assert_eq!(
        caps.get("push_delivery_id").and_then(Json::as_str),
        Some("declared"),
        "X-GitHub-Delivery is a durable delivery id"
    );
    assert_eq!(
        caps.get("poll").and_then(Json::as_str),
        Some("declared"),
        "the issues snapshot reads serve list/get"
    );
    assert_eq!(
        caps.get("native_write_tools").and_then(Json::as_str),
        Some("declared_absent"),
        "R4.7 is the read+ingress half — no write verbs"
    );
    assert_eq!(
        caps.get("assignee_mapping").and_then(Json::as_str),
        Some("declared"),
        "owner_map is the assignee→principal mapping"
    );
    // `probe` members resolve `unknown` only — a declared member is
    // never downgraded (T-LCD-07).
    let mut d = doc();
    if let Json::Obj(ref mut o) = d {
        o.insert(
            "capabilities".into(),
            Json::obj([("poll", Json::str("unknown"))]),
        );
        o.insert("probe".into(), Json::obj([("poll", Json::Bool(true))]));
    }
    let mut t2 = GithubTracker::from_doc(d).unwrap();
    assert_eq!(
        t2.capabilities().get("poll").and_then(Json::as_str),
        Some("probed")
    );
}

#[test]
fn github_tracker_list_get_normalize_the_bound_snapshot() {
    let mut t = tracker();
    // list — all issues normalized through `issue_record`.
    let all = t.list(&[]);
    let Json::Arr(recs) = &all else {
        panic!("list must be an array")
    };
    assert_eq!(recs.len(), 2);
    assert_eq!(
        recs[0].get("native_id").and_then(Json::as_str),
        Some("7"),
        "the issue number is the native id"
    );
    assert_eq!(recs[0].get("state").and_then(Json::as_str), Some("open"));
    // list{states} filters on the GitHub state spelling.
    let open = t.list(&["open".to_string()]);
    let Json::Arr(open) = &open else {
        panic!("list must be an array")
    };
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].get("native_id").and_then(Json::as_str), Some("7"));
    // get{native_ids} selects by issue number.
    let got = t.get(&["3".to_string(), "404".to_string()]);
    let Json::Arr(got) = &got else {
        panic!("get must be an array")
    };
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get("state").and_then(Json::as_str), Some("closed"));
}

#[test]
fn github_tracker_activate_run_and_suspended() {
    let mut t = tracker();
    // Derived runnable set — open issues only (`gh-<repo>-<n>`).
    let cand = vec![
        "gh-acme-sandbox-7".to_string(),
        "gh-acme-sandbox-3".to_string(),
        "other".to_string(),
    ];
    let Json::Arr(runnable) = t.activate_run(&cand) else {
        panic!("activate_run must be an array")
    };
    assert_eq!(runnable, vec![Json::str("gh-acme-sandbox-7")]);
    // An explicit `activate_run` member wins (the declared set rule the
    // fixture adapter serves — one shape, two lanes).
    let mut d = doc();
    if let Json::Obj(ref mut o) = d {
        o.insert(
            "activate_run".into(),
            Json::Arr(vec![Json::str("gh-acme-sandbox-3")]),
        );
    }
    let mut t2 = GithubTracker::from_doc(d).unwrap();
    let Json::Arr(runnable2) = t2.activate_run(&cand) else {
        panic!("activate_run must be an array")
    };
    assert_eq!(runnable2, vec![Json::str("gh-acme-sandbox-3")]);
    // suspended — the declared member and the `repo_archived` GitHub
    // suspension signal.
    assert_eq!(t.suspended("github"), Json::Bool(false));
    let mut d3 = doc();
    if let Json::Obj(ref mut o) = d3 {
        if let Json::Obj(ref mut g) = o.get_mut("github").unwrap() {
            g.insert("repo_archived".into(), Json::Bool(true));
        }
    }
    let mut t3 = GithubTracker::from_doc(d3).unwrap();
    assert_eq!(
        t3.suspended("github"),
        Json::Bool(true),
        "an archived repository suspends its source"
    );
    assert_eq!(t3.suspended("other"), Json::Bool(false));
}

#[test]
fn github_tracker_occurrences_are_verbatim_and_since_filtered() {
    let mut t = tracker();
    let Json::Arr(occs) = t.occurrences(None) else {
        panic!("occurrences must be an array")
    };
    assert_eq!(occs.len(), 1);
    assert_eq!(
        occs[0].get("occurrence_id").and_then(Json::as_str),
        Some("gh-occ-1")
    );
    // `since` is a source-side cursor — the wire answers identically on
    // replay (durable dedup owns idempotence).
    let Json::Arr(fresh) = t.occurrences(Some(2_000)) else {
        panic!("occurrences must be an array")
    };
    assert!(fresh.is_empty(), "occurrence observed_at 1000 < since");
}

#[test]
fn github_tracker_dispatch_round_trip_and_refusals() {
    let mut t = tracker();
    // The ONE wire codec: `list` over the dispatch boundary wraps and
    // unwraps losslessly.
    let out = tracker::dispatch(
        &mut t,
        "list",
        &[Json::obj([(
            "states",
            Json::Arr(vec![Json::str("closed")]),
        )])],
    )
    .unwrap();
    let listed = abi::unwrap_output(&out[0]).unwrap();
    let Json::Arr(recs) = &listed else {
        panic!("list doc must be an array")
    };
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].get("native_id").and_then(Json::as_str), Some("3"));
    // An unknown op is the ABI's typed refusal — never a panic.
    assert!(tracker::dispatch(&mut t, "nope", &[]).is_err());
    // Strict doc decode — an unknown member and a foreign schema both
    // refuse closed.
    let mut bad = doc();
    if let Json::Obj(ref mut o) = bad {
        o.insert("extra".into(), Json::Null);
    }
    assert!(GithubTracker::from_doc(bad).is_err());
    let wrong_schema = Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        (
            "github",
            Json::obj([
                ("repo", Json::str("acme/sandbox")),
                ("source_id", Json::str("github")),
            ]),
        ),
    ]);
    assert!(GithubTracker::from_doc(wrong_schema).is_err());
    // The envelope round-trips the capability record byte-exact.
    let caps = unwrap(t.capabilities());
    assert_eq!(
        caps.get("push_delivery_id").and_then(Json::as_str),
        Some("declared")
    );
    assert_eq!(
        CapState::Declared.as_str(),
        "declared",
        "the wire spelling is the canonical one"
    );
}
