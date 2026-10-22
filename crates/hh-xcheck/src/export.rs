//! `export_bundle(dir)` — write the committed `hh-xcheck-bundle/1` replay
//! bundle. Deterministic and hermetic: every byte derives from the committed
//! corpora (never from a clock or a path), so `export` to a fresh directory
//! is byte-identical to the committed `fixtures/cross-impl/` — the test
//! battery pins that (CC7-style drift gate for the corpus package).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hh_hir::corpus::{document_corpus, missing_provenance_bytes, CORPUS_SEALED_AT};
use hh_identity::corpus::{golden_record_fixtures, golden_tree_fixtures, GoldenRecordInput};
use hh_identity::idp::address;
use hh_identity::rowkeys::IrRef;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_registry::corpus as registry_corpus;
use hh_registry::extension::plugin::{resolve_plugin_candidate, AdmissionContext, Requests};
use hh_wire::json::{parse, Json};
use hh_wire::sha256_hex;

use crate::trees::tree_manifest;
use crate::{XcheckError, ARMS, BUNDLE_FORMAT, PENDING_CELLS};

fn err(msg: impl Into<String>) -> XcheckError {
    XcheckError(msg.into())
}

/// The `CHECKER.md` contract — embedded so the exported bundle always
/// carries the same committed copy (one source, at the crate root).
const CHECKER_MD: &str = include_str!("../CHECKER.md");

/// The hh-registry plugin-manifest corpus (committed fixtures — the input
/// `DF-S1.27-1`'s foreign checker re-runs).
const PLUGIN_CORPUS_SRC: &str = "../hh-registry/tests/fixtures/plugin-manifests";

/// Write `bytes` at `dir.join(rel)`, creating parents.
fn w(dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), XcheckError> {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&p, bytes).map_err(XcheckError::from)
}

fn canon(j: &Json) -> Vec<u8> {
    j.to_canonical_string().into_bytes()
}

/// Export the whole bundle under `dir` (created as needed; `bundle.json`
/// written last with the file inventory).
pub fn export_bundle(dir: &Path) -> Result<(), XcheckError> {
    export_identity_trees(dir)?;
    export_identity_records(dir)?;
    export_ledger_transcript(dir)?;
    export_registry_snapshot(dir)?;
    export_plugin_manifests(dir)?;
    export_canonical_parse(dir)?;
    export_hir_documents(dir)?;
    export_ledger_scenarios(dir)?;
    w(dir, "CHECKER.md", CHECKER_MD.as_bytes())?;
    write_manifest(dir)
}

// ── identity-trees ──────────────────────────────────────────────────────────

fn export_identity_trees(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/identity-trees";
    let mut expected = BTreeMap::new();
    for (name, tree) in golden_tree_fixtures() {
        w(
            dir,
            &format!("{arm}/{name}.input.json"),
            &canon(&tree_manifest(&tree)),
        )?;
        w(
            dir,
            &format!("{arm}/{name}.canonical.json"),
            &canon(&tree.canonical_json()),
        )?;
        expected.insert(name.to_string(), Json::str(tree.address().id()));
    }
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── identity-records ────────────────────────────────────────────────────────

fn export_identity_records(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/identity-records";
    let mut expected = BTreeMap::new();
    for (name, input) in golden_record_fixtures() {
        let (input_json, exp) = match input {
            GoldenRecordInput::Blob { bytes, media_type } => (
                Json::obj([
                    ("arm", Json::str("blob")),
                    ("bytes_hex", Json::str(crate::hex::encode(bytes))),
                    ("media_type", Json::str(media_type)),
                ]),
                Json::obj([("version_id", Json::str(address(bytes, media_type).id()))]),
            ),
            GoldenRecordInput::Text { bytes } => (
                Json::obj([
                    ("arm", Json::str("text")),
                    ("bytes_hex", Json::str(crate::hex::encode(bytes))),
                ]),
                Json::obj([(
                    "version_id",
                    Json::str(hh_identity::idp::identify_text(bytes)),
                )]),
            ),
            GoldenRecordInput::Record(record) => {
                let id = hh_identity::record::identify(&record)
                    .map_err(|e| err(format!("corpus record {name} fails identify: {e:?}")))?;
                let semantic = if record.kind.has_semantic_projection() {
                    record.canonical_semantic()
                } else {
                    Json::Null
                };
                (
                    Json::obj([
                        ("arm", Json::str("record")),
                        ("canonical_full", record.canonical_full()),
                        ("canonical_semantic", semantic),
                        ("domain_tag", Json::str(record.kind.domain_tag())),
                    ]),
                    Json::obj([
                        ("version_id", Json::str(id.version_id)),
                        ("semantic_id", id.semantic_id.map_or(Json::Null, Json::str)),
                    ]),
                )
            }
        };
        w(
            dir,
            &format!("{arm}/{name}.input.json"),
            &canon(&input_json),
        )?;
        expected.insert(name.to_string(), exp);
    }
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── ledger-transcript ───────────────────────────────────────────────────────

/// The pinned golden head (`hh-ledger/tests/acceptance.rs::
/// golden_transcript_is_byte_stable`) — export refuses to proceed if the
/// transcript drifts, so a stale bundle can never be committed silently.
const GOLDEN_HEAD: &str = "sha256:e8c2c0a9ce754a4d26c4fe501d24bd71719bebbcacb46c909fc91a0275b45468";

/// The one fixed caller event of the golden transcript (same bytes as the
/// pinned test).
fn golden_event() -> Event {
    Event {
        event_id: "evt-fixed".to_string(),
        class: "context.observation.recorded".to_string(),
        ts: "2026-01-01T00:00:00.000Z".to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload: Json::str("golden"),
    }
}

fn tmpdir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hh-xcheck-{tag}-{}", std::process::id()))
}

fn export_ledger_transcript(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/ledger-transcript";
    let tmp = tmpdir("ledger");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let built = (|| -> Result<(Vec<u8>, Vec<Json>, String), XcheckError> {
        let mut s = Store::open_test(&tmp, 1_000).map_err(|e| err(format!("{e:?}")))?;
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "w")
            .map_err(|e| err(format!("{e:?}")))?;
        s.append(&run, &lease, vec![golden_event()])
            .map_err(|e| err(format!("{e:?}")))?;
        let head = s.head(&run).map_err(|e| err(format!("{e:?}")))?.hash;
        let wal = fs::read(tmp.join("runs").join(&run).join("events.wal"))?;
        let envs = wal_envelopes(&wal)?;
        Ok((wal, envs, head))
    })();
    let _ = fs::remove_dir_all(&tmp);
    let (wal, envs, head) = built?;
    if head != GOLDEN_HEAD {
        return Err(err(format!(
            "golden transcript drifted: head {head} != pinned {GOLDEN_HEAD} — \
             the ledger corpus changed; regenerate deliberately"
        )));
    }
    w(dir, &format!("{arm}/events.wal"), &wal)?;
    w(
        dir,
        &format!("{arm}/envelopes.json"),
        &canon(&Json::Arr(envs.clone())),
    )?;
    let hashes: Vec<Json> = envs
        .iter()
        .map(|e| Json::str(e.get("hash").and_then(Json::as_str).unwrap_or("")))
        .collect();
    w(
        dir,
        &format!("{arm}/scenario.json"),
        &canon(&Json::obj([
            ("clock_ms", Json::Int(1000)),
            ("id_source", Json::str("seq")),
            (
                "events",
                Json::Arr(vec![Json::obj([
                    ("class", Json::str("context.observation.recorded")),
                    ("event_id", Json::str("evt-fixed")),
                    ("payload", Json::str("golden")),
                    ("ts", Json::str("2026-01-01T00:00:00.000Z")),
                ])]),
            ),
            ("run_kind", Json::str("agent")),
            ("writer", Json::str("w")),
        ])),
    )?;
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::obj([
            ("event_count", Json::Int(envs.len() as i64)),
            ("event_hashes", Json::Arr(hashes)),
            ("head_hash", Json::str(head)),
            ("wal_sha256", Json::str(sha256_hex(&wal))),
        ])),
    )
}

// ── registry-snapshot ───────────────────────────────────────────────────────

fn export_registry_snapshot(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/registry-snapshot";
    let tmp = tmpdir("registry");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    /// `(registry.json bytes, snapshot_id, variant_version_ids, outputs bytes)`
    /// — the four products the temp store hands back.
    type RegistryProducts = (Vec<u8>, String, Vec<String>, Vec<u8>);
    let out = (|| -> Result<RegistryProducts, XcheckError> {
        let (snap, vids) = registry_corpus::build(&tmp).map_err(|e| err(format!("{e:?}")))?;
        let store =
            hh_registry::store::RegistryStore::open(&tmp, &registry_corpus::kernel_registrar())
                .map_err(|e| err(format!("{e:?}")))?;
        let outputs =
            registry_corpus::outputs(&store, &snap, &vids).map_err(|e| err(format!("{e:?}")))?;
        let log = fs::read(tmp.join("registry.json"))?;
        Ok((log, snap, vids, outputs))
    })();
    let _ = fs::remove_dir_all(&tmp);
    let (log, snap, vids, outputs) = out?;
    w(dir, &format!("{arm}/registry.json"), &log)?;
    w(
        dir,
        &format!("{arm}/scenario.json"),
        &canon(&Json::obj([
            ("snapshot_id", Json::str(snap)),
            (
                "variant_version_ids",
                Json::Arr(vids.iter().map(|v| Json::str(v.clone())).collect()),
            ),
        ])),
    )?;
    // The expected outputs verbatim — canonical bytes (the checker's byte
    // target), plus their digest for the answers file.
    w(dir, &format!("{arm}/expected-outputs.json"), &outputs)?;
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::obj([
            ("outputs_sha256", Json::str(sha256_hex(&outputs))),
            ("registry_sha256", Json::str(sha256_hex(&log))),
        ])),
    )
}

// ── plugin-manifests ────────────────────────────────────────────────────────

fn export_plugin_manifests(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/plugin-manifests";
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(PLUGIN_CORPUS_SRC);
    let tmp = tmpdir("plugin");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    registry_corpus::build(&tmp).map_err(|e| err(format!("corpus store: {e:?}")))?;
    let store = hh_registry::store::RegistryStore::open(&tmp, &registry_corpus::kernel_registrar())
        .map_err(|e| err(format!("{e:?}")))?;
    // The corpus-driver context: deny-everything requests cap (the same
    // admission posture `hh-registry/tests/plugin.rs::golden_manifest_corpus`
    // pins — a fixture with non-empty requests must fail RequestsExceedCap).
    let ctx = AdmissionContext {
        first_party: true,
        now: hh_plugin::EvalPoint::kernel(),
        requests_cap: Some(Requests::default()),
    };
    let mut expected = BTreeMap::new();
    for sub in ["valid", "invalid"] {
        let d = src.join(sub);
        let mut files: Vec<PathBuf> = fs::read_dir(&d)
            .map_err(XcheckError::from)?
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        for path in files {
            let fname = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| err("bad fixture name"))?
                .to_string();
            let rel = format!("{sub}/{fname}");
            let bytes = fs::read(&path)?;
            w(dir, &format!("{arm}/{rel}"), &bytes)?;
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let verdict = match resolve_plugin_candidate(&store, &bytes, &ctx) {
                    Ok(report) => format!("admit:{}", report.plugin_id),
                    Err(e) => format!("refuse:{}", e.kind_str()),
                };
                expected.insert(rel.clone(), Json::str(verdict));
            }
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── canonical-parse ─────────────────────────────────────────────────────────

/// One canonical-parse payload: an `hh-embed/1`-shaped JSON-RPC batch padded
/// to at least `target` canonical bytes. Deterministic — the same `seed`
/// always emits the same document.
fn parse_payload(seed: u64, target: usize) -> Vec<u8> {
    let build = |pad_len: usize| -> Vec<u8> {
        let mut frames = Vec::new();
        for i in 0..7u64 {
            let n = seed * 1000 + i;
            frames.push(Json::obj([
                ("id", Json::Int(n as i64)),
                ("jsonrpc", Json::str("2.0")),
                ("method", Json::str("lab.results.read")),
                (
                    "params",
                    Json::obj([
                        ("cursor", Json::str(format!("seq:{n}"))),
                        ("limit", Json::Int(50)),
                        ("padding", Json::str("x".repeat(pad_len / 7))),
                        (
                            "run_ref",
                            Json::str(format!("run-{seed:04}/{i:03}-café-☕")),
                        ),
                        (
                            "tags",
                            Json::Arr(vec![
                                Json::str("replay"),
                                Json::str(format!("batch-{seed}")),
                            ]),
                        ),
                    ]),
                ),
            ]));
        }
        Json::obj([("frames", Json::Arr(frames))])
            .to_canonical_string()
            .into_bytes()
    };
    let base = build(0);
    if base.len() >= target {
        return base;
    }
    // Close the gap deterministically: `pad_len / 7` pad bytes land in each of
    // the 7 frames, so request the shortfall plus the per-frame remainder.
    let need = target - base.len();
    let grown = build(need + 14);
    if grown.len() >= target {
        return grown;
    }
    build(need + 4096)
}

fn export_canonical_parse(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/canonical-parse";
    let mut expected = BTreeMap::new();
    // 16 KiB and 64 KiB bands — the DF-S0.3-2 far-side parse+hash corpus.
    let cases: Vec<(String, u64, usize)> = (0..4)
        .map(|i| (format!("frame-16k-{i}"), i as u64, 16 * 1024))
        .chain((0..2).map(|i| (format!("frame-64k-{i}"), 100 + i as u64, 64 * 1024)))
        .collect();
    for (name, seed, target) in cases {
        let bytes = parse_payload(seed, target);
        w(dir, &format!("{arm}/frames/{name}.json"), &bytes)?;
        expected.insert(
            name,
            Json::obj([
                ("bytes", Json::Int(bytes.len() as i64)),
                ("sha256", Json::str(sha256_hex(&bytes))),
            ]),
        );
    }
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── hir-documents ───────────────────────────────────────────────────────────

/// The `hir-documents` arm (R4.3 — DF-S1.2-2's "extend to the HIR/ledger
/// corpus" leg): each `hh_hir::corpus` case serializes as
/// `<name>.input.json`; the expected answer is the recorded replay
/// (`parse → validate → canonicalize → seal` at `CORPUS_SEALED_AT`). A
/// semantic drift in the pipeline changes `expected.json` and trips the
/// export-determinism battery — deliberate regen only.
fn export_hir_documents(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/hir-documents";
    let mut expected = BTreeMap::new();
    let mut inputs: Vec<(&'static str, Vec<u8>)> = document_corpus()
        .iter()
        .map(|(name, doc)| (*name, doc.canonical_bytes()))
        .collect();
    inputs.push(("missing-provenance", missing_provenance_bytes()));
    for (name, bytes) in inputs {
        let answer = crate::hir_replay::replay(&bytes, CORPUS_SEALED_AT);
        w(dir, &format!("{arm}/{name}.input.json"), &bytes)?;
        expected.insert(name.to_string(), answer);
    }
    w(
        dir,
        &format!("{arm}/scenario.json"),
        &canon(&Json::obj([(
            "sealed_at",
            Json::Int(CORPUS_SEALED_AT as i64),
        )])),
    )?;
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── ledger-scenarios ────────────────────────────────────────────────────────

/// A caller event for the WAL scenarios — `golden_event`'s shape with
/// caller-controlled id/class/parent/ts/payload.
fn scn_event(id: &str, class: &str, parent: &str, ts: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: ts.to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: parent.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

/// Decode a WAL's `k:"e"` envelopes (the `envelopes.json` member's content).
fn wal_envelopes(wal: &[u8]) -> Result<Vec<Json>, XcheckError> {
    let mut envs = Vec::new();
    for line in wal.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let j = parse(std::str::from_utf8(line).map_err(|e| err(format!("{e}")))?)
            .map_err(|e| err(format!("wal line parse: {e}")))?;
        if j.get("k").and_then(Json::as_str) == Some("e") {
            envs.push(j.get("v").cloned().unwrap_or(Json::Null));
        }
    }
    Ok(envs)
}

/// Build one scenario's run under a throwaway deterministic store and hand
/// back `(wal bytes, decoded e-frames, head_hash)`. `f` drives the store
/// past `open_run` — appends, blob puts, lease handovers.
fn build_scenario(
    tag: &str,
    f: impl FnOnce(&mut Store, &str, &Lease) -> Result<(), XcheckError>,
) -> Result<(Vec<u8>, Vec<Json>, String), XcheckError> {
    let tmp = tmpdir(tag);
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let built = (|| -> Result<(Vec<u8>, Vec<Json>, String), XcheckError> {
        let mut s = Store::open_test(&tmp, 1_000).map_err(|e| err(format!("{e:?}")))?;
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "w")
            .map_err(|e| err(format!("{e:?}")))?;
        f(&mut s, &run, &lease)?;
        let head = s.head(&run).map_err(|e| err(format!("{e:?}")))?.hash;
        let wal = fs::read(tmp.join("runs").join(&run).join("events.wal"))?;
        let envs = wal_envelopes(&wal)?;
        Ok((wal, envs, head))
    })();
    let _ = fs::remove_dir_all(&tmp);
    built
}

/// `causal-chain` — two append batches: a `parent_event_id` chain across
/// batches, a same-run `causes` ref, and `ir_refs`/`surface_ids` members.
fn scn_causal_chain(s: &mut Store, run: &str, lease: &Lease) -> Result<(), XcheckError> {
    let a = scn_event(
        "evt-chain-a",
        "context.observation.recorded",
        ROOT_EVENT,
        "2026-01-02T00:00:00.000Z",
        Json::str("chain-a"),
    );
    s.append(run, lease, vec![a])
        .map_err(|e| err(format!("{e:?}")))?;
    let mut b = scn_event(
        "evt-chain-b",
        "context.observation.recorded",
        "evt-chain-a",
        "2026-01-02T00:00:01.000Z",
        Json::obj([("detail", Json::str("caused by evt-chain-a"))]),
    );
    b.causes = vec![EventRef {
        run_id: run.to_string(),
        event_id: "evt-chain-a".to_string(),
    }];
    let mut c = scn_event(
        "evt-chain-c",
        "context.observation.recorded",
        "evt-chain-b",
        "2026-01-02T00:00:02.000Z",
        Json::obj([("detail", Json::str("ir_refs + surface_ids"))]),
    );
    c.ir_refs = vec![IrRef::new(
        format!("sha256:{}", sha256_hex(b"corpus-ir")),
        Some("corpus:doc".to_string()),
    )];
    c.surface_ids
        .insert("provider_native".to_string(), "native-007".to_string());
    s.append(run, lease, vec![b, c])
        .map_err(|e| err(format!("{e:?}")))?;
    Ok(())
}

/// `blob-refs` — `put_blob` two blobs (one empty), then events whose
/// `refs[]` carry the content addresses (a shared address across events).
fn scn_blob_refs(s: &mut Store, run: &str, lease: &Lease) -> Result<(), XcheckError> {
    let a1 = s
        .put_blob(b"blob one\n", "application/octet-stream")
        .map_err(|e| err(format!("{e:?}")))?;
    let a2 = s
        .put_blob(b"", "text/plain")
        .map_err(|e| err(format!("{e:?}")))?;
    let mut e1 = scn_event(
        "evt-blob",
        "context.observation.recorded",
        ROOT_EVENT,
        "2026-01-02T00:00:00.000Z",
        Json::obj([("note", Json::str("two blob refs"))]),
    );
    e1.refs = vec![a1.clone(), a2];
    let mut e2 = scn_event(
        "evt-blob-2",
        "context.observation.recorded",
        "evt-blob",
        "2026-01-02T00:00:01.000Z",
        Json::str("re-refs blob one"),
    );
    e2.refs = vec![a1];
    s.append(run, lease, vec![e1, e2])
        .map_err(|e| err(format!("{e:?}")))?;
    Ok(())
}

/// `writer-handover` — `w` releases its writer lease and `w2` takes over at
/// generation 2: `lifecycle.lease.released` + a second `lease.acquired`,
/// and caller envelopes stamped `lease_generation` 1 vs 2.
fn scn_writer_handover(s: &mut Store, run: &str, lease: &Lease) -> Result<(), XcheckError> {
    let e1 = scn_event(
        "evt-w1",
        "context.observation.recorded",
        ROOT_EVENT,
        "2026-01-02T00:00:00.000Z",
        Json::str("first writer"),
    );
    s.append(run, lease, vec![e1])
        .map_err(|e| err(format!("{e:?}")))?;
    s.release(lease, "handover")
        .map_err(|e| err(format!("{e:?}")))?;
    let lease2 = s
        .acquire_writer("w2", run, 60_000)
        .map_err(|e| err(format!("{e:?}")))?;
    let e2 = scn_event(
        "evt-w2",
        "context.observation.recorded",
        ROOT_EVENT,
        "2026-01-02T00:00:01.000Z",
        Json::str("second writer"),
    );
    s.append(run, &lease2, vec![e2])
        .map_err(|e| err(format!("{e:?}")))?;
    Ok(())
}

/// The `ledger-scenarios` arm (R4.3 — DF-S1.5-3's ledger-corpus leg):
/// additional transcript/WAL scenarios beyond the pinned golden transcript
/// — each case a `<case>/` dir of `events.wal` + `envelopes.json` +
/// `scenario.json`; `expected.json` answers per case plus informational
/// `<case>.event_count` / `<case>.wal_sha256` members.
fn export_ledger_scenarios(dir: &Path) -> Result<(), XcheckError> {
    let arm = "arms/ledger-scenarios";
    // The deterministic recipe every case records in `scenario.json`.
    let scn_json = |note: &str| {
        Json::obj([
            ("clock_ms", Json::Int(1000)),
            ("id_source", Json::str("seq")),
            ("note", Json::str(note)),
            ("run_kind", Json::str("agent")),
            ("writer", Json::str("w")),
        ])
    };
    /// One scenario row: `(case name, the store driver, the scenario.json
    /// member)`.
    type ScenarioRow = (
        &'static str,
        fn(&mut Store, &str, &Lease) -> Result<(), XcheckError>,
        Json,
    );
    let scenarios: Vec<ScenarioRow> = vec![
        (
            "causal-chain",
            scn_causal_chain,
            scn_json("two append batches; parent chain + a same-run cause + ir_refs/surface_ids"),
        ),
        (
            "blob-refs",
            scn_blob_refs,
            scn_json("put_blob x2 (one empty); refs[] across two events"),
        ),
        (
            "writer-handover",
            scn_writer_handover,
            scn_json("w releases; w2 takes over at lease generation 2"),
        ),
    ];
    let mut expected = BTreeMap::new();
    for (name, build, scenario_json) in scenarios {
        let (wal, envs, head) = build_scenario(name, build)?;
        let case_dir = format!("{arm}/{name}");
        w(dir, &format!("{case_dir}/events.wal"), &wal)?;
        w(
            dir,
            &format!("{case_dir}/envelopes.json"),
            &canon(&Json::Arr(envs.clone())),
        )?;
        w(
            dir,
            &format!("{case_dir}/scenario.json"),
            &canon(&scenario_json),
        )?;
        let hashes: Vec<Json> = envs
            .iter()
            .map(|e| Json::str(e.get("hash").and_then(Json::as_str).unwrap_or("")))
            .collect();
        expected.insert(
            name.to_string(),
            Json::obj([
                ("event_hashes", Json::Arr(hashes)),
                ("head_hash", Json::str(head)),
            ]),
        );
        expected.insert(format!("{name}.event_count"), Json::Int(envs.len() as i64));
        expected.insert(format!("{name}.wal_sha256"), Json::str(sha256_hex(&wal)));
    }
    w(
        dir,
        &format!("{arm}/expected.json"),
        &canon(&Json::Obj(expected)),
    )
}

// ── bundle.json ─────────────────────────────────────────────────────────────

fn write_manifest(dir: &Path) -> Result<(), XcheckError> {
    // The file inventory — every bundle member hashed (excluding bundle.json
    // itself). Sorted paths → deterministic.
    let mut files = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d)? {
            let e = e?;
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .map_err(|e| err(format!("{e}")))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel == "bundle.json" {
                    continue;
                }
                files.insert(
                    rel.clone(),
                    Json::str(format!("sha256:{}", sha256_hex(&fs::read(&p)?))),
                );
            }
        }
    }
    // CF-493 — `verified_by[]`: the candidate/evidence record, rendered
    // `{candidate, evidence_class, evidence_ref}` on both `arms[]` and
    // `pending_cells[]`. A pending cell's `verified_by[]` is the *partial*
    // discharge record — `status` stays `environment-pending`.
    let verified_by = |vs: &[crate::CandidateEvidence]| {
        Json::Arr(
            vs.iter()
                .map(|v| {
                    Json::obj([
                        ("candidate", Json::str(v.candidate)),
                        ("evidence_class", Json::str(v.evidence_class)),
                        ("evidence_ref", Json::str(v.evidence_ref)),
                    ])
                })
                .collect(),
        )
    };
    let arms = Json::Arr(
        ARMS.iter()
            .map(|a| {
                Json::obj([
                    ("dir", Json::str(a.dir)),
                    ("df", Json::str(a.df)),
                    ("id", Json::str(a.id)),
                    ("status", Json::str("e1-self-checked")),
                    ("verified_by", verified_by(a.verified_by)),
                ])
            })
            .collect(),
    );
    let pending = Json::Arr(
        PENDING_CELLS
            .iter()
            .map(|(cell, df, needs, by)| {
                Json::obj([
                    ("cell", Json::str(*cell)),
                    ("df", Json::str(*df)),
                    ("needs", Json::str(*needs)),
                    ("status", Json::str("environment-pending")),
                    ("verified_by", verified_by(by)),
                ])
            })
            .collect(),
    );
    w(
        dir,
        "bundle.json",
        &canon(&Json::obj([
            ("arms", arms),
            ("files", Json::Obj(files)),
            ("format", Json::str(BUNDLE_FORMAT)),
            ("generated_by", Json::str("E1")),
            ("pending_cells", pending),
        ])),
    )
}
