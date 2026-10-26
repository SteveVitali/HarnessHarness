//! `self_check` — replay the bundle through the **E1** implementation
//! hermetically — and `verify_answers` — the comparator a foreign
//! implementation's `answers.json` is checked against.
//!
//! `self_check` is the CC7 leg: it proves the packaging is sufficient — that
//! the serialized *inputs* (never the crate-side corpus constructors) fold
//! through the identity/ledger/registry/plugin primitives to exactly the
//! committed `expected` answers. `verify_answers` is the ticket's
//! cross-implementation leg: it fails per-case on a disagreeing or missing
//! answer — a vacuous green is impossible.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use hharness_identity::idp::{address, identify_text, idp_id};
use hharness_ledger::ids::GENESIS_HASH;
use hharness_ledger::schema::event_hash;
use hharness_registry::extension::plugin::{resolve_plugin_candidate, AdmissionContext, Requests};
use hharness_wire::json::{parse, Json};
use hharness_wire::sha256_hex;

use crate::trees::manifest_to_tree;
use crate::XcheckError;

fn err(msg: impl Into<String>) -> XcheckError {
    XcheckError(msg.into())
}

/// One case outcome: the arm, the case name, and `None` when green /
/// `Some(detail)` when it failed.
#[derive(Debug)]
pub struct CaseResult {
    /// The arm id (`identity-trees`, `ledger-transcript`, …).
    pub arm: String,
    /// The case (fixture name, `head_hash`, a fixture's relative path…).
    pub case: String,
    /// `Ok(())` green; `Err(detail)` the named failure.
    pub outcome: Result<(), String>,
}

/// The full report — one `CaseResult` per replayed case.
#[derive(Debug)]
pub struct CheckReport {
    /// Per-case outcomes in replay order.
    pub results: Vec<CaseResult>,
}

impl CheckReport {
    /// Green when every case passed.
    pub fn is_green(&self) -> bool {
        self.results.iter().all(|r| r.outcome.is_ok())
    }
    /// `(pass, fail)` counts.
    pub fn counts(&self) -> (usize, usize) {
        let pass = self.results.iter().filter(|r| r.outcome.is_ok()).count();
        (pass, self.results.len() - pass)
    }
    /// The failing cases (arm, case, detail) — empty when green.
    pub fn failures(&self) -> Vec<(&str, &str, &str)> {
        self.results
            .iter()
            .filter_map(|r| {
                r.outcome
                    .as_ref()
                    .err()
                    .map(|d| (r.arm.as_str(), r.case.as_str(), d.as_str()))
            })
            .collect()
    }
}

fn push(results: &mut Vec<CaseResult>, arm: &str, case: impl Into<String>, r: Result<(), String>) {
    results.push(CaseResult {
        arm: arm.to_string(),
        case: case.into(),
        outcome: r,
    });
}

fn expect_map(dir: &Path, arm: &str) -> Result<BTreeMap<String, Json>, XcheckError> {
    let p = dir.join(arm).join("expected.json");
    let j = parse(&String::from_utf8_lossy(&fs::read(&p)?))
        .map_err(|e| err(format!("{}: {e}", p.display())))?;
    match j {
        Json::Obj(m) => Ok(m),
        _ => Err(err(format!(
            "{}: expected.json must be an object",
            p.display()
        ))),
    }
}

/// Replay `dir` (a bundle root) through the E1 implementation — every arm,
/// every case. Fails fast only on unreadable structure; per-case results
/// collect in the report.
pub fn self_check(dir: &Path) -> Result<CheckReport, XcheckError> {
    let mut results = Vec::new();
    check_identity_trees(dir, &mut results)?;
    check_identity_records(dir, &mut results)?;
    check_ledger_transcript(dir, &mut results)?;
    check_registry_snapshot(dir, &mut results)?;
    check_plugin_manifests(dir, &mut results)?;
    check_canonical_parse(dir, &mut results)?;
    check_hir_documents(dir, &mut results)?;
    check_ledger_scenarios(dir, &mut results)?;
    Ok(CheckReport { results })
}

fn check_identity_trees(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/identity-trees";
    let expected = expect_map(dir, arm)?;
    let mut inputs: Vec<String> = fs::read_dir(dir.join(arm))?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| n.ends_with(".input.json"))
        .collect();
    inputs.sort();
    for fname in inputs {
        let name = fname.trim_end_matches(".input.json").to_string();
        let res = (|| -> Result<(), String> {
            let input_bytes = fs::read(dir.join(arm).join(&fname)).map_err(|e| format!("{e}"))?;
            let manifest = parse(&String::from_utf8_lossy(&input_bytes))
                .map_err(|e| format!("input parse: {e}"))?;
            let tree = manifest_to_tree(&manifest)?;
            // The serialized canonical form must be the fixture's bytes —
            // this is the cross-implementation contract's strongest leg:
            // a foreign `tree/1` fold emits the same canonical document.
            let canonical = tree.canonical_json().to_canonical_string();
            let committed = fs::read(dir.join(arm).join(format!("{name}.canonical.json")))
                .map_err(|e| format!("{e}"))?;
            if canonical.as_bytes() != committed.as_slice() {
                return Err("canonical form differs from committed fixture".to_string());
            }
            let id = tree.address().id();
            match expected.get(&name) {
                Some(Json::Str(want)) if *want == id => Ok(()),
                Some(_) => Err(format!("address {id} ≠ expected {expected:?}")),
                None => Err("no expected entry".to_string()),
            }
        })();
        push(out, "identity-trees", name, res);
    }
    Ok(())
}

fn check_identity_records(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/identity-records";
    let expected = expect_map(dir, arm)?;
    let mut inputs: Vec<String> = fs::read_dir(dir.join(arm))?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| n.ends_with(".input.json"))
        .collect();
    inputs.sort();
    for fname in inputs {
        let name = fname.trim_end_matches(".input.json").to_string();
        let res = (|| -> Result<(), String> {
            let bytes = fs::read(dir.join(arm).join(&fname)).map_err(|e| format!("{e}"))?;
            let input =
                parse(&String::from_utf8_lossy(&bytes)).map_err(|e| format!("input parse: {e}"))?;
            let want = expected
                .get(&name)
                .ok_or_else(|| "no expected entry".to_string())?;
            let arm_tag = input
                .get("arm")
                .and_then(Json::as_str)
                .ok_or_else(|| "missing arm".to_string())?;
            let want_version = want.get("version_id").and_then(Json::as_str).unwrap_or("");
            match arm_tag {
                "blob" => {
                    let bytes = crate::hex::decode(
                        input
                            .get("bytes_hex")
                            .and_then(Json::as_str)
                            .ok_or("missing bytes_hex")?,
                    )?;
                    let media = input
                        .get("media_type")
                        .and_then(Json::as_str)
                        .unwrap_or("application/octet-stream");
                    let got = address(&bytes, media).id();
                    (got == want_version)
                        .then_some(())
                        .ok_or(format!("{got} ≠ {want_version}"))
                }
                "text" => {
                    let bytes = crate::hex::decode(
                        input
                            .get("bytes_hex")
                            .and_then(Json::as_str)
                            .ok_or("missing bytes_hex")?,
                    )?;
                    let got = identify_text(&bytes);
                    (got == want_version)
                        .then_some(())
                        .ok_or(format!("{got} ≠ {want_version}"))
                }
                "record" => {
                    let tag = input
                        .get("domain_tag")
                        .and_then(Json::as_str)
                        .ok_or("missing domain_tag")?;
                    let full = input
                        .get("canonical_full")
                        .ok_or("missing canonical_full")?;
                    let got_version = idp_id(tag, full.to_canonical_string().as_bytes());
                    if got_version != want_version {
                        return Err(format!("version {got_version} ≠ {want_version}"));
                    }
                    let want_semantic = want.get("semantic_id").cloned().unwrap_or(Json::Null);
                    let got_semantic = match input.get("canonical_semantic") {
                        Some(Json::Null) | None => Json::Null,
                        Some(sem) => Json::str(idp_id(
                            &format!("{tag}#semantic"),
                            sem.to_canonical_string().as_bytes(),
                        )),
                    };
                    (got_semantic == want_semantic).then_some(()).ok_or(format!(
                        "semantic {} ≠ {}",
                        got_semantic.to_canonical_string(),
                        want_semantic.to_canonical_string()
                    ))
                }
                other => Err(format!("unknown record arm tag {other}")),
            }
        })();
        push(out, "identity-records", name, res);
    }
    Ok(())
}

/// The per-WAL replay both ledger arms share (CC1 — one fold): every
/// frame must be canonical JSON; every `k:"e"` frame's `v`, minus `hash`,
/// re-hashes under the `ledger.event` domain against the folded
/// `prev_hash`. `prefix` namespaces the per-frame case names
/// (`"frame[i]"` for the transcript arm, `"<case>:frame[i]"` for the
/// scenario arm). Returns `(folded head hash, recorded e-frame hashes in
/// order)`.
fn fold_wal_frames(
    wal: &[u8],
    arm: &str,
    prefix: &str,
    out: &mut Vec<CaseResult>,
) -> (String, Vec<String>) {
    let mut prev = GENESIS_HASH.to_string();
    let mut recorded_hashes = Vec::new();
    let mut seq = 0u64;
    for line in wal.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let case = format!("{prefix}frame[{seq}]");
        let res = (|| -> Result<(), String> {
            let text = std::str::from_utf8(line).map_err(|e| format!("{e}"))?;
            let frame = parse(text).map_err(|e| format!("frame parse: {e}"))?;
            // The committed WAL bytes must themselves be canonical JSON —
            // the foreign replay decodes them under the same rule.
            if frame.to_canonical_string().as_bytes() != line {
                return Err("WAL frame is not canonical JSON".to_string());
            }
            // Commit frames (`k:"c"`) interleave with event frames: they
            // carry no hash of their own — the canonical-bytes check above
            // is their replay leg. The `prev` fold only advances on `e`.
            if frame.get("k").and_then(Json::as_str) != Some("e") {
                if frame.get("k").and_then(Json::as_str) == Some("c") {
                    return Ok(());
                }
                return Err("unknown frame kind".to_string());
            }
            let env = frame
                .get("v")
                .cloned()
                .ok_or_else(|| "missing v".to_string())?;
            let Json::Obj(m) = env.clone() else {
                return Err("envelope not an object".to_string());
            };
            let recorded = m
                .get("hash")
                .and_then(Json::as_str)
                .ok_or_else(|| "envelope missing hash".to_string())?
                .to_string();
            let mut preimage = m;
            preimage.remove("hash");
            let got = event_hash(&Json::Obj(preimage), &prev);
            if got != recorded {
                return Err(format!("recomputed {got} ≠ recorded {recorded}"));
            }
            prev = recorded.clone();
            recorded_hashes.push(recorded);
            Ok(())
        })();
        push(out, arm, case, res);
        seq += 1;
    }
    (prev, recorded_hashes)
}

fn check_ledger_transcript(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/ledger-transcript";
    let expected = expect_map(dir, arm)?;
    let wal = fs::read(dir.join(arm).join("events.wal"))?;
    if Json::str(sha256_hex(&wal)) != *expected.get("wal_sha256").unwrap_or(&Json::Null) {
        push(
            out,
            "ledger-transcript",
            "wal_sha256",
            Err("WAL bytes drifted from expected".to_string()),
        );
        return Ok(());
    }
    push(out, "ledger-transcript", "wal_sha256", Ok(()));
    let (prev, _hashes) = fold_wal_frames(&wal, "ledger-transcript", "", out);
    let want_head = expected
        .get("head_hash")
        .and_then(Json::as_str)
        .unwrap_or("");
    push(
        out,
        "ledger-transcript",
        "head_hash",
        if prev == want_head {
            Ok(())
        } else {
            Err(format!("folded head {prev} ≠ expected {want_head}"))
        },
    );
    Ok(())
}

/// The `ledger-scenarios` arm (R4.3 / DF-S1.5-3): every `<case>/` dir's
/// `events.wal` replays through the shared fold; `expected.json` answers
/// per case (`{event_hashes, head_hash}`) plus the informational
/// `<case>.event_count` / `<case>.wal_sha256` members.
fn check_ledger_scenarios(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/ledger-scenarios";
    let expected = expect_map(dir, arm)?;
    let mut cases: Vec<String> = fs::read_dir(dir.join(arm))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    cases.sort();
    for case in cases {
        let wal = match fs::read(dir.join(arm).join(&case).join("events.wal")) {
            Ok(w) => w,
            Err(e) => {
                push(
                    out,
                    "ledger-scenarios",
                    format!("{case}:events.wal"),
                    Err(format!("{e}")),
                );
                continue;
            }
        };
        let want_wal = expected
            .get(&format!("{case}.wal_sha256"))
            .and_then(Json::as_str)
            .unwrap_or("");
        let got_wal = sha256_hex(&wal);
        push(
            out,
            "ledger-scenarios",
            format!("{case}:wal_sha256"),
            if got_wal == want_wal {
                Ok(())
            } else {
                Err(format!("wal sha256 {got_wal} ≠ expected {want_wal}"))
            },
        );
        let (prev, hashes) = fold_wal_frames(&wal, "ledger-scenarios", &format!("{case}:"), out);
        let want = expected.get(&case).cloned().unwrap_or(Json::Null);
        let want_head = want.get("head_hash").and_then(Json::as_str).unwrap_or("");
        push(
            out,
            "ledger-scenarios",
            format!("{case}:head_hash"),
            if prev == want_head {
                Ok(())
            } else {
                Err(format!("folded head {prev} ≠ expected {want_head}"))
            },
        );
        let want_hashes = want.get("event_hashes").cloned().unwrap_or(Json::Null);
        let got_hashes = Json::Arr(hashes.iter().map(|h| Json::str(h.clone())).collect());
        push(
            out,
            "ledger-scenarios",
            format!("{case}:event_hashes"),
            if got_hashes == want_hashes {
                Ok(())
            } else {
                Err(format!(
                    "event_hashes {} ≠ expected {}",
                    got_hashes.to_canonical_string(),
                    want_hashes.to_canonical_string()
                ))
            },
        );
    }
    Ok(())
}

/// The `hir-documents` arm (R4.3 / DF-S1.2-2's HIR-corpus leg): every
/// `<name>.input.json` replays through `hir_replay` (the one pipeline
/// spelling) and compares member-for-member to `expected.json`; a
/// parse-ok input must re-canonicalize to its own committed bytes.
fn check_hir_documents(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/hir-documents";
    let expected = expect_map(dir, arm)?;
    let scenario = fs::read(dir.join(arm).join("scenario.json")).map_err(XcheckError::from)?;
    let sealed_at = parse(&String::from_utf8_lossy(&scenario))
        .map_err(|e| err(format!("scenario.json parse: {e}")))?
        .get("sealed_at")
        .and_then(|j| match j {
            Json::Int(n) => Some(*n as u64),
            _ => None,
        })
        .ok_or_else(|| err("scenario.json missing sealed_at"))?;
    let mut inputs: Vec<String> = fs::read_dir(dir.join(arm))?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| n.ends_with(".input.json"))
        .collect();
    inputs.sort();
    for fname in inputs {
        let name = fname.trim_end_matches(".input.json").to_string();
        let res = (|| -> Result<(), String> {
            let bytes = fs::read(dir.join(arm).join(&fname)).map_err(|e| format!("{e}"))?;
            let answer = crate::hir_replay::replay(&bytes, sealed_at);
            // A parse-ok input IS the canonical document — re-canonicalizing
            // must reproduce the committed bytes (the foreign fold's pin).
            if answer.get("parse").and_then(Json::as_str) == Some("ok") {
                let input_sha = sha256_hex(&bytes);
                if answer.get("canonical_sha256").and_then(Json::as_str) != Some(input_sha.as_str())
                {
                    return Err("canonicalized bytes differ from the committed input".to_string());
                }
            }
            let want = expected
                .get(&name)
                .ok_or_else(|| "no expected entry".to_string())?;
            (answer == *want).then_some(()).ok_or_else(|| {
                format!(
                    "replay {} ≠ expected {}",
                    answer.to_canonical_string(),
                    want.to_canonical_string()
                )
            })
        })();
        push(out, "hir-documents", name, res);
    }
    Ok(())
}

fn check_registry_snapshot(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/registry-snapshot";
    let expected = expect_map(dir, arm)?;
    let res = (|| -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("hh-xcheck-reg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|e| format!("{e}"))?;
        let inner = (|| -> Result<(), String> {
            fs::write(
                tmp.join("registry.json"),
                fs::read(dir.join(arm).join("registry.json")).map_err(|e| format!("{e}"))?,
            )
            .map_err(|e| format!("{e}"))?;
            let store = hharness_registry::store::RegistryStore::open(
                &tmp,
                &hharness_registry::corpus::kernel_registrar(),
            )
            .map_err(|e| format!("{e:?}"))?;
            let scenario = parse(&String::from_utf8_lossy(
                &fs::read(dir.join(arm).join("scenario.json")).map_err(|e| format!("{e}"))?,
            ))
            .map_err(|e| format!("scenario parse: {e}"))?;
            let snap = scenario
                .get("snapshot_id")
                .and_then(Json::as_str)
                .ok_or("missing snapshot_id")?
                .to_string();
            let vids: Vec<String> = match scenario.get("variant_version_ids") {
                Some(Json::Arr(vs)) => vs
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            };
            let outputs = hharness_registry::corpus::outputs(&store, &snap, &vids)
                .map_err(|e| format!("{e:?}"))?;
            let committed = fs::read(dir.join(arm).join("expected-outputs.json"))
                .map_err(|e| format!("{e}"))?;
            if outputs != committed {
                return Err("corpus outputs differ from committed fixture".to_string());
            }
            let want = expected
                .get("outputs_sha256")
                .and_then(Json::as_str)
                .unwrap_or("");
            let got = sha256_hex(&outputs);
            (got == want)
                .then_some(())
                .ok_or(format!("outputs sha256 {got} ≠ {want}"))
        })();
        let _ = fs::remove_dir_all(&tmp);
        inner
    })();
    push(out, "registry-snapshot", "outputs", res);
    Ok(())
}

fn check_plugin_manifests(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/plugin-manifests";
    let expected = expect_map(dir, arm)?;
    let res = (|| -> Result<hharness_registry::store::RegistryStore, String> {
        let tmp = std::env::temp_dir().join(format!("hh-xcheck-plug-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|e| format!("{e}"))?;
        hharness_registry::corpus::build(&tmp).map_err(|e| format!("{e:?}"))?;
        hharness_registry::store::RegistryStore::open(
            &tmp,
            &hharness_registry::corpus::kernel_registrar(),
        )
        .map_err(|e| format!("{e:?}"))
        // NB: the tempdir intentionally persists for the store's lifetime —
        // cleaned by the OS; a fresh export never reads it.
    })();
    let store = match res {
        Ok(s) => s,
        Err(e) => {
            push(out, "plugin-manifests", "store", Err(e));
            return Ok(());
        }
    };
    let ctx = AdmissionContext {
        first_party: true,
        now: hharness_plugin::EvalPoint::kernel(),
        requests_cap: Some(Requests::default()),
    };
    for (rel, want) in &expected {
        let res = (|| -> Result<(), String> {
            let bytes =
                fs::read(dir.join(arm).join(rel)).map_err(|e| format!("read {rel}: {e}"))?;
            let got = match resolve_plugin_candidate(&store, &bytes, &ctx) {
                Ok(report) => format!("admit:{}", report.plugin_id),
                Err(e) => format!("refuse:{}", e.kind_str()),
            };
            let want = want.as_str().unwrap_or("");
            (got == want)
                .then_some(())
                .ok_or(format!("verdict {got} ≠ expected {want}"))
        })();
        push(out, "plugin-manifests", rel.clone(), res);
    }
    Ok(())
}

fn check_canonical_parse(dir: &Path, out: &mut Vec<CaseResult>) -> Result<(), XcheckError> {
    let arm = "arms/canonical-parse";
    let expected = expect_map(dir, arm)?;
    let mut frames: Vec<String> = fs::read_dir(dir.join(arm).join("frames"))?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    frames.sort();
    for fname in frames {
        let name = fname.trim_end_matches(".json").to_string();
        let res = (|| -> Result<(), String> {
            let bytes =
                fs::read(dir.join(arm).join("frames").join(&fname)).map_err(|e| format!("{e}"))?;
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("utf-8: {e}"))?;
            let j = parse(text).map_err(|e| format!("parse: {e}"))?;
            // Round-trip: the committed bytes ARE the canonical form —
            // a foreign codec must decode them and re-emit byte-identically.
            if j.to_canonical_string().as_bytes() != bytes.as_slice() {
                return Err("payload is not self-canonical".to_string());
            }
            let want = expected.get(&name).ok_or("no expected entry")?;
            let want_sha = want.get("sha256").and_then(Json::as_str).unwrap_or("");
            let got = sha256_hex(&bytes);
            (got == want_sha)
                .then_some(())
                .ok_or(format!("sha256 {got} ≠ {want_sha}"))
        })();
        push(out, "canonical-parse", name, res);
    }
    Ok(())
}

// ── foreign-answer verification ─────────────────────────────────────────────

/// The `answers.json` contract (documented in `CHECKER.md`):
/// `{ "candidate": "<E-id>", "arms": { <arm-id>: { <case>: <answer> } } }` —
/// each `<answer>` is the value `expected.json` pins (a digest string, a
/// verdict string, an `{version_id,semantic_id}` pair). Timing members a
/// foreign checker adds (`parse_ns`, …) are carried, not gated.
///
/// The `expected.json` members a foreign checker never answers: bundle-
/// integrity digests and informational members — `registry_sha256`,
/// `wal_sha256`, `event_count`, and `ledger-scenarios`'s per-case
/// `<case>.wal_sha256` / `<case>.event_count` spellings (R4.3). One rule
/// (CC1) — `verify_answers` and the test battery's answer-builder share it.
pub fn is_informational(arm_id: &str, case: &str) -> bool {
    matches!(
        (arm_id, case),
        ("registry-snapshot", "registry_sha256")
            | ("ledger-transcript", "wal_sha256")
            | ("ledger-transcript", "event_count")
    ) || (arm_id == "ledger-scenarios"
        && (case.ends_with(".wal_sha256") || case.ends_with(".event_count")))
}

/// Compare `answers` against the bundle's `expected` per case. A missing or
/// disagreeing case fails by name — never a vacuous pass. `candidate` is
/// echoed for the record.
pub fn verify_answers(dir: &Path, answers: &Json) -> Result<CheckReport, XcheckError> {
    let mut results = Vec::new();
    let Json::Obj(arms) = answers.get("arms").cloned().unwrap_or(Json::Null) else {
        return Err(err("answers must carry arms{}"));
    };
    for spec in crate::ARMS {
        let expected = expect_map(dir, spec.dir)?;
        let given = arms
            .get(spec.id)
            .cloned()
            .unwrap_or(Json::Obj(BTreeMap::new()));
        for (case, want) in &expected {
            if is_informational(spec.id, case) {
                continue;
            }
            let res = match given.get(case) {
                None => Err("missing answer".to_string()),
                Some(got) => {
                    // Object expectations compare member-wise (a foreign
                    // answer may add timing members; `bytes` is a bundle
                    // fact, not an answer); scalars compare exactly.
                    if let Json::Obj(wm) = want {
                        wm.iter()
                            .filter(|(k, _)| !(spec.id == "canonical-parse" && *k == "bytes"))
                            .find(|(k, v)| got.get(k) != Some(*v))
                            .map(|(k, v)| {
                                format!(
                                    "member {k}: got {} ≠ expected {}",
                                    got.get(k)
                                        .map(Json::to_canonical_string)
                                        .unwrap_or_else(|| "<absent>".to_string()),
                                    v.to_canonical_string()
                                )
                            })
                            .map_or(Ok(()), Err)
                    } else if got == want {
                        Ok(())
                    } else {
                        Err(format!(
                            "answer {} ≠ expected {}",
                            got.to_canonical_string(),
                            want.to_canonical_string()
                        ))
                    }
                }
            };
            push(&mut results, spec.id, case.clone(), res);
        }
    }
    Ok(CheckReport { results })
}
