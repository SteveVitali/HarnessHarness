//! R4.9 acceptance battery — the four remaining foreign vocabularies
//! (DF-S4.2-2; §5h.3 §2; ADR-0370): each of `swebench_submission`,
//! `inspect_eval_log`, `in_toto_bundle`, `telemetry_trace` lifts a
//! committed sample fixture (`fixtures/foreign/<format>/`, provenance
//! recorded per directory) into a hosted `unverified` R0 bundle with
//! typed claim + loss records, and — all four being symmetric —
//! round-trips export→import byte-stably. `interchange_trajectory`
//! refuses `import` (`ForeignDirectionRefused` — OQ-030/ADR-0213),
//! `harbor_trial_dir` refuses `export`, unknown spellings never guess.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::codec::Decoded;
use hh_bundle::error::BundleError;
use hh_bundle::export::{export_target, PublicationPolicy};
use hh_bundle::import::lift_foreign;
use hh_bundle::manifest::BundlePolicy;
use hh_bundle::ForeignFormat;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-r49-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The committed sample for `format` as the lift's file map
/// (`fixtures/foreign/<format>/` — `PROVENANCE.md` rides as a member).
fn fixture_files(format: ForeignFormat) -> BTreeMap<String, Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/foreign")
        .join(format.as_str());
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            files.insert(
                p.file_name().unwrap().to_string_lossy().to_string(),
                std::fs::read(&p).unwrap(),
            );
        }
    }
    files
}

/// A store with a subject run (the export bundle's source) and a hosted
/// carrier run (the lift's subject).
struct Rig {
    store: Store,
    run: String,
    carrier: String,
    def_addr: String,
    def_bytes: Vec<u8>,
}

fn rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("r4.9"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, _l) = store.open_run(m, "r49.test").unwrap();
    let (carrier, _l2) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "r49.test")
        .unwrap();
    Rig {
        store,
        run,
        carrier,
        def_addr,
        def_bytes,
    }
}

fn decoded(rig: &Rig) -> Decoded {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &BundlePolicy::default(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("r49.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: &get,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
    })
    .unwrap();
    Decoded {
        manifest: a.manifest,
        members: a.members,
    }
}

fn lift(
    rig: &Rig,
    format: ForeignFormat,
    files: &BTreeMap<String, Vec<u8>>,
) -> hh_bundle::import::ForeignLift {
    lift_foreign(
        &rig.store,
        &rig.carrier,
        files,
        format,
        ProvenanceRecord::kernel("r49.test", 0).to_json(),
        "2026-01-01T00:00:00.000Z".into(),
        "sha256:kernel",
        0,
    )
    .unwrap()
}

fn arr<'a>(j: &'a Json, key: &str) -> &'a [Json] {
    match j.get(key) {
        Some(Json::Arr(a)) => a,
        _ => &[],
    }
}

fn roles(m: &hh_bundle::manifest::BundleManifest) -> Vec<&str> {
    m.claims.iter().map(|c| c.role.as_str()).collect()
}

fn loss_members(report: &Json) -> Vec<&str> {
    arr(report, "mapping_loss")
        .iter()
        .filter_map(|e| e.get("member").and_then(Json::as_str))
        .collect()
}

fn loss_entries(report: &Json) -> &[Json] {
    arr(report, "entries")
}

/// The hosted/unverified/R0 invariants every foreign lift must carry
/// (§5h.3 §2; AC-R-2.9.3-9) — asserted on every lift below.
fn assert_hosted_unverified(l: &hh_bundle::import::ForeignLift, format: ForeignFormat) {
    let m = &l.bundle.manifest;
    assert_eq!(m.participant_class, "hosted");
    assert_eq!(
        m.reproducibility
            .get("claimed_level")
            .and_then(Json::as_str),
        Some("R0")
    );
    assert!(m
        .unpinned
        .iter()
        .any(|u| u.role == "model" && u.reason == "foreign_only"));
    let prov = l.import_record.get("provenance").unwrap();
    assert_eq!(
        prov.get("authority").and_then(Json::as_str),
        Some("unverified")
    );
    assert_eq!(
        prov.get("origin")
            .and_then(|o| o.get("source_system"))
            .and_then(Json::as_str),
        Some(format.as_str())
    );
}

// ── the closed sum + typed refusals ───────────────────────────────────────

#[test]
fn foreign_format_is_a_closed_sum() {
    let spellings: Vec<&str> = ForeignFormat::ALL.iter().map(|f| f.as_str()).collect();
    assert_eq!(
        spellings,
        [
            "harbor_trial_dir",
            "harbor_job_dir",
            "interchange_trajectory",
            "swebench_submission",
            "inspect_eval_log",
            "in_toto_bundle",
            "telemetry_trace",
        ]
    );
    for f in ForeignFormat::ALL {
        assert_eq!(ForeignFormat::parse(f.as_str()), Some(f));
    }
    // `parse` never guesses — an outside spelling is `None` (the
    // callers then refuse `FormatUnknown`).
    for bad in [
        "swebench",
        "inspect_eval",
        "intoto",
        "otlp",
        "",
        "HARBOR_JOB_DIR",
    ] {
        assert!(ForeignFormat::parse(bad).is_none(), "{bad}");
    }
    // Direction table — `interchange_trajectory` is export-only
    // (OQ-030/ADR-0213), `harbor_trial_dir` import-only.
    assert!(!ForeignFormat::InterchangeTrajectory.lifts());
    assert!(ForeignFormat::InterchangeTrajectory.exports());
    assert!(!ForeignFormat::HarborTrialDir.exports());
    assert!(ForeignFormat::HarborTrialDir.lifts());
    for f in [
        ForeignFormat::HarborJobDir,
        ForeignFormat::SwebenchSubmission,
        ForeignFormat::InspectEvalLog,
        ForeignFormat::InTotoBundle,
        ForeignFormat::TelemetryTrace,
    ] {
        assert!(f.lifts() && f.exports(), "{}", f.as_str());
    }
}

#[test]
fn direction_refusals_are_typed_never_unknown() {
    let rig = rig("dirref");
    let files = fixture_files(ForeignFormat::SwebenchSubmission);
    // A known-but-export-only format at `import`.
    match lift_foreign(
        &rig.store,
        &rig.carrier,
        &files,
        ForeignFormat::InterchangeTrajectory,
        ProvenanceRecord::kernel("r49.test", 0).to_json(),
        "2026-01-01T00:00:00.000Z".into(),
        "sha256:kernel",
        0,
    ) {
        Err(BundleError::ForeignDirectionRefused { format, direction }) => {
            assert_eq!(
                (format.as_str(), direction),
                ("interchange_trajectory", "import")
            );
        }
        _ => panic!("expected ForeignDirectionRefused"),
    }
    // A known-but-import-only format at `export`.
    let d = decoded(&rig);
    match export_target(&d, "harbor_trial_dir", &PublicationPolicy::default()) {
        Err(BundleError::ForeignDirectionRefused { format, direction }) => {
            assert_eq!((format.as_str(), direction), ("harbor_trial_dir", "export"));
        }
        _ => panic!("expected ForeignDirectionRefused"),
    }
    // A spelling outside the sum is still `FormatUnknown`, never a guess.
    match export_target(&d, "bogus", &PublicationPolicy::default()) {
        Err(BundleError::FormatUnknown { .. }) => {}
        _ => panic!("expected FormatUnknown"),
    }
}

// ── per-format fixture lifts (claim/loss records asserted) ────────────────

#[test]
fn swebench_submission_lifts_typed_claims() {
    let rig = rig("swe-lift");
    let files = fixture_files(ForeignFormat::SwebenchSubmission);
    let l = lift(&rig, ForeignFormat::SwebenchSubmission, &files);
    assert_hosted_unverified(&l, ForeignFormat::SwebenchSubmission);
    let rs = roles(&l.bundle.manifest);
    // Every fixture file (artefact + PROVENANCE.md) is a verbatim member
    // with a digest claim.
    for f in ["predictions.jsonl", "PROVENANCE.md"] {
        assert!(l
            .bundle
            .manifest
            .members
            .iter()
            .any(|m| m.role == format!("foreign:{f}")));
        assert!(
            rs.contains(&format!("foreign_digest:{f}").as_str()),
            "{rs:?}"
        );
    }
    // The typed claims — one per prediction row.
    assert!(rs.contains(&"swebench.instance:django__django-11099"));
    assert!(rs.contains(&"swebench.instance:sympy__sympy-20590"));
    let c = l
        .bundle
        .manifest
        .claims
        .iter()
        .find(|c| c.role == "swebench.instance:django__django-11099")
        .unwrap();
    assert_eq!(
        c.value.get("model").and_then(Json::as_str),
        Some("hh-standin-model")
    );
    assert!(c
        .value
        .get("patch_digest")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
    // The claim's provenance is the import-minted `unverified` stamp —
    // a foreign fact is never re-asserted.
    assert_eq!(
        c.provenance.get("authority").and_then(Json::as_str),
        Some("unverified")
    );
    // The foreign model name is a `name_binding` (R-ID-4 — never a pin).
    assert!(l.bundle.manifest.name_bindings.iter().any(|b| {
        b.get("name").and_then(Json::as_str) == Some("hh-standin-model")
            && b.get("role").and_then(Json::as_str) == Some("swebench.model_name_or_path")
    }));
    // No conflicts: no declared digests in the fixture.
    assert!(arr(&l.mapping_report, "conflicts").is_empty());
}

#[test]
fn inspect_eval_log_lifts_header_scores_and_losses() {
    let rig = rig("ins-lift");
    let files = fixture_files(ForeignFormat::InspectEvalLog);
    let l = lift(&rig, ForeignFormat::InspectEvalLog, &files);
    assert_hosted_unverified(&l, ForeignFormat::InspectEvalLog);
    let rs = roles(&l.bundle.manifest);
    for role in [
        "inspect.version",
        "inspect.status",
        "inspect.task",
        "inspect.model",
        "inspect.sample:1",
        "inspect.score:accuracy",
    ] {
        assert!(rs.contains(&role), "missing claim {role}: {rs:?}");
    }
    assert_eq!(
        l.bundle
            .manifest
            .claims
            .iter()
            .find(|c| c.role == "inspect.task")
            .unwrap()
            .value
            .as_str(),
        Some("hh.standin.task")
    );
    // `plan`/`stats`/transcripts are named losses, never silently absent.
    let lm = loss_members(&l.mapping_report);
    assert!(lm.iter().any(|m| m.contains("plan")), "{lm:?}");
    assert!(lm.iter().any(|m| m.contains("stats")), "{lm:?}");
    assert!(lm.iter().any(|m| m.contains("transcript")), "{lm:?}");
}

#[test]
fn in_toto_bundle_lifts_subjects_and_unverified_signatures() {
    let rig = rig("int-lift");
    let files = fixture_files(ForeignFormat::InTotoBundle);
    let l = lift(&rig, ForeignFormat::InTotoBundle, &files);
    assert_hosted_unverified(&l, ForeignFormat::InTotoBundle);
    let rs = roles(&l.bundle.manifest);
    assert!(
        rs.contains(&"intoto.subject:standin-artefact.bin"),
        "{rs:?}"
    );
    assert!(
        rs.contains(&"intoto.subject:standin-manifest.json"),
        "{rs:?}"
    );
    assert!(rs
        .iter()
        .any(|r| r.starts_with("intoto.predicate:https://slsa.dev/provenance/v1")));
    // The signature is a claim — `present_unverified`, keyid recorded,
    // NEVER verified (no trust roots; authority stays `unverified`).
    let sig = l
        .bundle
        .manifest
        .claims
        .iter()
        .find(|c| c.role == "intoto.signature:0")
        .expect("signature claim minted");
    assert_eq!(
        sig.value.get("status").and_then(Json::as_str),
        Some("present_unverified")
    );
    assert_eq!(
        sig.value.get("keyid").and_then(Json::as_str),
        Some("standin-key-1")
    );
    assert_eq!(
        sig.provenance.get("authority").and_then(Json::as_str),
        Some("unverified")
    );
}

#[test]
fn telemetry_trace_lifts_services_traces_and_narrowed_loss() {
    let rig = rig("otl-lift");
    let files = fixture_files(ForeignFormat::TelemetryTrace);
    let l = lift(&rig, ForeignFormat::TelemetryTrace, &files);
    assert_hosted_unverified(&l, ForeignFormat::TelemetryTrace);
    let rs = roles(&l.bundle.manifest);
    assert!(rs.contains(&"otel.service:hh-standin-svc"), "{rs:?}");
    assert!(rs.iter().any(|r| r.starts_with("otel.trace:")), "{rs:?}");
    assert!(rs.contains(&"otel.span_count"));
    assert_eq!(
        l.bundle
            .manifest
            .claims
            .iter()
            .find(|c| c.role == "otel.span_count")
            .unwrap()
            .value
            .as_int(),
        Some(2)
    );
    // Span attributes are `narrowed` — the stream is a view, never the
    // event record.
    let lm = loss_members(&l.mapping_report);
    assert!(lm.iter().any(|m| m.contains("attributes")), "{lm:?}");
}

// ── export→import round-trips (symmetric formats are byte-stable) ────────

/// Export `decoded` to `format`, lift the artefact back, and assert the
/// byte-stability invariant: every artefact file is a `foreign:<path>`
/// member at the *same* content address (bytes preserved verbatim), the
/// declared digests verify (no `ForeignIntegrityMismatch`), and the
/// lifted bundle is hosted/unverified/R0.
fn assert_roundtrip(
    rig: &Rig,
    format: ForeignFormat,
) -> (
    hh_bundle::export::ExportOutcome,
    hh_bundle::import::ForeignLift,
) {
    let d = decoded(rig);
    let out = export_target(&d, format.as_str(), &PublicationPolicy::default()).unwrap();
    assert_eq!(out.granularity_ceiling, "product");
    let l = lift(rig, format, &out.files);
    assert_hosted_unverified(&l, format);
    for (path, bytes) in &out.files {
        let addr = hh_identity::address(bytes, "application/octet-stream").id();
        assert!(
            l.bundle
                .manifest
                .members
                .iter()
                .any(|m| m.role == format!("foreign:{path}") && m.address == addr),
            "{path} not preserved verbatim at {addr}"
        );
    }
    assert!(
        arr(&l.mapping_report, "conflicts").is_empty(),
        "declared digests must verify: {:?}",
        l.mapping_report
    );
    // The asymmetry/loss record is never empty on a foreign lowering.
    assert!(!loss_entries(&out.loss_report).is_empty());
    (out, l)
}

#[test]
fn swebench_submission_round_trips() {
    let rig = rig("swe-rt");
    let (out, l) = assert_roundtrip(&rig, ForeignFormat::SwebenchSubmission);
    let preds = std::str::from_utf8(&out.files["predictions.jsonl"]).unwrap();
    // One prediction row per subject run; the run id stands in as the
    // instance coordinate (no task_ref declared) — and the loss report
    // says so rather than fabricating a diff.
    assert!(preds.contains("\"instance_id\""));
    assert!(preds.contains(&rig.run));
    assert!(loss_entries(&out.loss_report)
        .iter()
        .any(|e| e.get("member").and_then(Json::as_str)
            == Some(&format!("model_patch:{}", rig.run))
            && e.get("class").and_then(Json::as_str) == Some("no_slot")));
    // And the claim round-trips: the artefact's own row re-derives the
    // `swebench.instance` claim on import.
    assert!(roles(&l.bundle.manifest).contains(&format!("swebench.instance:{}", rig.run).as_str()));
}

#[test]
fn inspect_eval_log_round_trips() {
    let rig = rig("ins-rt");
    let (out, l) = assert_roundtrip(&rig, ForeignFormat::InspectEvalLog);
    let doc =
        hh_wire::json::parse(std::str::from_utf8(&out.files["eval_log.json"]).unwrap()).unwrap();
    assert_eq!(doc.get("version").and_then(Json::as_int), Some(2));
    // The subject run is open in the rig — `started`, the honest map.
    assert_eq!(doc.get("status").and_then(Json::as_str), Some("started"));
    assert_eq!(
        doc.get("eval")
            .and_then(|e| e.get("run_id"))
            .and_then(Json::as_str),
        Some(rig.run.as_str())
    );
    // The absent sections are named losses — never zeroed-in.
    assert!(loss_entries(&out.loss_report)
        .iter()
        .any(|e| e.get("member").and_then(Json::as_str) == Some("plan|stats|reductions")));
    // Claim round-trip: the artefact's own header re-derives.
    assert!(roles(&l.bundle.manifest).contains(&"inspect.version"));
}

#[test]
fn in_toto_bundle_round_trips_unsigned() {
    let rig = rig("int-rt");
    let (out, l) = assert_roundtrip(&rig, ForeignFormat::InTotoBundle);
    let line = std::str::from_utf8(&out.files["attestation.intoto.jsonl"]).unwrap();
    let env = hh_wire::json::parse(line.trim()).unwrap();
    // Unsigned by construction — signing is attest()'s claim, never a
    // lowering's; the asymmetry is the named loss row.
    assert_eq!(arr(&env, "signatures").len(), 0);
    assert!(loss_entries(&out.loss_report)
        .iter()
        .any(
            |e| e.get("member").and_then(Json::as_str) == Some("signatures")
                && e.get("class").and_then(Json::as_str) == Some("no_slot")
        ));
    // The re-imported claims re-derive from the artefact itself — the
    // statement's subjects are the lifted bundle's own members.
    assert!(roles(&l.bundle.manifest)
        .iter()
        .any(|r| r.starts_with("intoto.subject:")));
}

#[test]
fn telemetry_trace_round_trips() {
    let rig = rig("otl-rt");
    let (out, l) = assert_roundtrip(&rig, ForeignFormat::TelemetryTrace);
    let doc =
        hh_wire::json::parse(std::str::from_utf8(&out.files["traces.json"]).unwrap()).unwrap();
    let rspans = arr(&doc, "resourceSpans");
    assert_eq!(rspans.len(), 1);
    let span_count: i64 = rspans
        .iter()
        .map(|rs| {
            arr(rs, "scopeSpans")
                .iter()
                .map(|ss| arr(ss, "spans").len() as i64)
                .sum::<i64>()
        })
        .sum();
    assert!(span_count > 0, "one span per ledger event");
    // The narrowing is named — the span stream cannot re-derive the
    // event set.
    assert!(loss_entries(&out.loss_report)
        .iter()
        .any(|e| e.get("member").and_then(Json::as_str)
            == Some(&format!("event_payloads:{}", rig.run))
            && e.get("class").and_then(Json::as_str) == Some("narrowed")));
    assert!(roles(&l.bundle.manifest).contains(&"otel.service:hh-bundle"));
    assert_eq!(
        l.bundle
            .manifest
            .claims
            .iter()
            .find(|c| c.role == "otel.span_count")
            .unwrap()
            .value
            .as_int(),
        Some(span_count)
    );
}

// ── malformed inputs record loss, never panic ─────────────────────────────

#[test]
fn malformed_artefact_rows_are_losses_not_panics() {
    let rig = rig("malformed");
    // A `.jsonl` file whose second line is not JSON, plus a prediction
    // row with an extra untyped field — both named losses, not a refusal.
    let files: BTreeMap<String, Vec<u8>> = [(
        "predictions.jsonl".to_string(),
        b"{\"instance_id\":\"a__b-1\",\"model_name_or_path\":\"m\",\"model_patch\":\"\",\"extra_field\":1}\nthis is not json\n"
            .to_vec(),
    )]
    .into_iter()
    .collect();
    let l = lift(&rig, ForeignFormat::SwebenchSubmission, &files);
    assert!(roles(&l.bundle.manifest).contains(&"swebench.instance:a__b-1"));
    let lm = loss_members(&l.mapping_report);
    assert!(lm.iter().any(|m| m.contains(":2")), "{lm:?}");
    assert!(lm.iter().any(|m| m.contains("extra_field")), "{lm:?}");

    // An in-toto bundle line that is not a DSSE envelope → loss row.
    let files2: BTreeMap<String, Vec<u8>> = [(
        "attestation.intoto.jsonl".to_string(),
        b"{\"payloadType\":\"application/vnd.in-toto+json\",\"payload\":\"!!!\",\"signatures\":[]}\n"
            .to_vec(),
    )]
    .into_iter()
    .collect();
    let l2 = lift(&rig, ForeignFormat::InTotoBundle, &files2);
    let lm2 = loss_members(&l2.mapping_report);
    assert!(
        lm2.iter().any(|m| m.contains("attestation.intoto.jsonl")),
        "{lm2:?}"
    );
}
