//! R7.3 — the gen-2 `transcript_render` set decodes through
//! `decode_model_lane` (manifest 329): the committed gen-2
//! `hh-p4-levels/1` record's sealed documents carry the complete
//! replay-fidelity set the ws1 authored doc declared (in the admitted
//! canonical spellings — the G-L1 rerun-4 repair), and the strict
//! decoder still refuses unknown members, out-of-domain values and
//! malformed shapes — the gen-2 repair authors the canonical
//! spellings precisely because the lane decode never accepts the
//! authored-doc's historical aliases (`keep`, `on_format_error`,
//! `text_only_turns`, `tool_args` — `bind_hir` normalizes those at
//! bind time; the lane sees canonical only).
//! Offline fixture — committed records only, no provider call.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use hharness_embed::gateway_model::decode_model_lane;
use hharness_wire::json::{parse, Json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const G2_LEVELS: &str = "docs/build/reports/2026-11-21-fp-p4-levels.json";

/// The committed gen-2 record's V1 sealed doc — a content-addressed
/// artifact carrying the repaired `transcript_render`.
fn committed_transcript_render() -> Json {
    let lvl = parse(&std::fs::read_to_string(root().join(G2_LEVELS)).expect("read gen-2 levels"))
        .unwrap();
    lvl.get("levels")
        .and_then(|ls| match ls {
            Json::Arr(a) => a
                .iter()
                .find(|l| l.get("level_id").and_then(Json::as_str) == Some("V1"))
                .cloned(),
            _ => None,
        })
        .and_then(|l| {
            l.get("sealed")
                .and_then(|s| s.get("document"))
                .and_then(|d| d.get("assembly"))
                .and_then(|a| a.get("slots"))
                .and_then(|s| s.get("model_port"))
                .and_then(|mp| mp.get("one"))
                .and_then(|o| o.get("params"))
                .and_then(|p| p.get("transcript_render"))
                .cloned()
        })
        .expect("V1 sealed doc carries transcript_render")
}

/// A decodable lane params object wrapping a candidate
/// `transcript_render` — the required lane members are inert
/// fixtures; the refusal/accept surface under test is the
/// `transcript_render` member alone.
fn lane_params(tr: Json) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str("http://127.0.0.1:1"));
    m.insert("provider_model_id".to_string(), Json::str("m"));
    m.insert("transcript_render".to_string(), tr);
    Json::Obj(m)
}

/// Mutate one member of the committed set (or add an unknown one).
fn mutated(f: impl FnOnce(&mut std::collections::BTreeMap<String, Json>)) -> Json {
    let tr = committed_transcript_render();
    let Json::Obj(mut m) = tr else {
        panic!("transcript_render is an object")
    };
    f(&mut m);
    Json::Obj(m)
}

// ── AC-1: the committed gen-2 set decodes — every member at its
//          declared value, the swept leaf bound ─────────────────────────

#[test]
fn ac1_committed_gen2_set_decodes() {
    let tr = committed_transcript_render();
    let lane = decode_model_lane(&lane_params(tr.clone()), "profile:test")
        .unwrap_or_else(|e| panic!("the committed gen-2 set decodes: {e:?}"));
    let r = &lane.transcript_render;
    assert!(r.replay_content, "assistant_replay.content verbatim");
    assert!(r.replay_reasoning, "assistant_replay.reasoning verbatim");
    assert!(r.provider_ids, "id_normalisation provider_verbatim");
    assert!(r.format_error_omit, "format_error_turn omit");
    assert!(!r.text_only_replay, "text_only_turn drop");
    assert!(r.verbatim_args, "tool_arguments verbatim");
    // The committed member set is exactly the ws1 set.
    let Json::Obj(m) = tr else { panic!() };
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "assistant_replay",
            "format_error_turn",
            "id_normalisation",
            "text_only_turn",
            "tool_arguments"
        ]
    );
}

// ── AC-2: the strict decoder still refuses — unknown members,
//          out-of-domain values, malformed shapes ──────────────────────

#[test]
fn ac2_unknown_member_refuses() {
    let tr = mutated(|m| {
        m.insert("provider_extra".to_string(), Json::str("x"));
    });
    let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
    assert!(format!("{e:?}").contains("provider_extra"), "{e:?}");
}

#[test]
fn ac2_out_of_domain_value_refuses() {
    for (k, v) in [
        ("format_error_turn", "drop_everything"),
        ("id_normalisation", "rewritten"),
        ("tool_arguments", "parsed"),
        ("text_only_turn", "verbatim"),
    ] {
        let tr = mutated(|m| {
            m.insert(k.to_string(), Json::str(v));
        });
        let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
        assert!(format!("{e:?}").contains(k), "{k}={v}: {e:?}");
    }
    // assistant_replay leaf values refuse too.
    let tr = mutated(|m| {
        m.insert(
            "assistant_replay".to_string(),
            Json::obj([("content", Json::str("summarize"))]),
        );
    });
    let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
    assert!(format!("{e:?}").contains("content"), "{e:?}");
}

#[test]
fn ac2_malformed_shapes_refuse() {
    // transcript_render itself must be an object.
    let e = decode_model_lane(&lane_params(Json::str("verbatim")), "p").unwrap_err();
    assert!(format!("{e:?}").contains("transcript_render"), "{e:?}");
    // assistant_replay must be an object.
    let tr = mutated(|m| {
        m.insert("assistant_replay".to_string(), Json::str("verbatim"));
    });
    let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
    assert!(format!("{e:?}").contains("assistant_replay"), "{e:?}");
}

#[test]
fn ac2_historical_authored_spellings_refuse() {
    // The ws1 authored doc's pre-`bind_hir` spellings never reach the
    // lane — the sealed doc carries canonical members only; feeding
    // the historical spellings to the strict decoder refuses. This is
    // why the gen-2 repair authors canonical members directly.
    for k in ["on_format_error", "text_only_turns", "tool_args"] {
        let tr = mutated(|m| {
            m.insert(k.to_string(), Json::str("drop"));
        });
        let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
        assert!(format!("{e:?}").contains(k), "{k}: {e:?}");
    }
    // `content: keep` — the authored alias for `verbatim` — refuses.
    let tr = mutated(|m| {
        m.insert(
            "assistant_replay".to_string(),
            Json::obj([("content", Json::str("keep"))]),
        );
    });
    let e = decode_model_lane(&lane_params(tr), "p").unwrap_err();
    assert!(format!("{e:?}").contains("content"), "{e:?}");
}
