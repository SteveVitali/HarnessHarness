//! `ForeignFormat` — the closed sum of foreign artefact vocabularies the
//! bundle plane admits at `import`/`export` (§5h.3 §2; DF-S4.2-2; ticket
//! R4.9; ADR-0370). `parse` is the one grammar (CC1): a spelling outside
//! the sum refuses `FormatUnknown`, never guesses; a known format asked
//! the wrong direction refuses `ForeignDirectionRefused`.
//!
//! Directions (the claim-set rulings this ticket owns — ADR-0370):
//! `harbor_trial_dir` lifts only (a foreign trial tree has no native
//! lowering); `interchange_trajectory` exports only (its import half
//! stays OQ-030-deferred, ADR-0213); the other five go both ways.
//!
//! Format references (fetched 2026-10-22; fixture `PROVENANCE.md` files
//! carry the same citations): `swebench_submission` — SWE-bench
//! predictions JSONL (`{"instance_id","model_name_or_path","model_patch"}`
//! per line; github.com/SWE-bench/SWE-bench `docs/guides/evaluation.md`);
//! `inspect_eval_log` — Inspect AI `EvalLog` JSON (`{version, status,
//! eval{task,model,created,…}, results{scores[]}, samples[]}`;
//! inspect.aisi.org.uk/eval-logs.html); `in_toto_bundle` — in-toto
//! attestation bundle, JSONL of DSSE envelopes (github.com/in-toto/
//! attestation `spec/v1/bundle.md`); `telemetry_trace` — OTLP/JSON
//! `ExportTraceServiceRequest` (opentelemetry.io OTLP 1.11.1 — hex
//! trace/span ids, unix-nanos times as strings).

use std::collections::BTreeMap;

use hh_wire::json::{self, Json};

use crate::error::BundleError;
use crate::export::MemberBytes;
use crate::manifest::{BundleManifest, Claim};

/// The closed foreign-format sum (§5h.3 §2's declared vocabularies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignFormat {
    /// A foreign trial directory tree (import only).
    HarborTrialDir,
    /// Our own harbor job-dir export shape (import + export).
    HarborJobDir,
    /// The ADR-0021 interchange trajectory (export only — the import
    /// half stays OQ-030-deferred, ADR-0213).
    InterchangeTrajectory,
    /// A SWE-bench predictions artefact (import + export).
    SwebenchSubmission,
    /// An Inspect AI eval log, `.json` spelling (import + export).
    InspectEvalLog,
    /// An in-toto attestation bundle (DSSE-envelope JSONL; signatures
    /// are claims, never verified — no trust roots are in scope).
    InTotoBundle,
    /// An OTLP/JSON trace export (import + export).
    TelemetryTrace,
}

impl ForeignFormat {
    /// Every member of the closed sum.
    pub const ALL: [ForeignFormat; 7] = [
        ForeignFormat::HarborTrialDir,
        ForeignFormat::HarborJobDir,
        ForeignFormat::InterchangeTrajectory,
        ForeignFormat::SwebenchSubmission,
        ForeignFormat::InspectEvalLog,
        ForeignFormat::InTotoBundle,
        ForeignFormat::TelemetryTrace,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HarborTrialDir => "harbor_trial_dir",
            Self::HarborJobDir => "harbor_job_dir",
            Self::InterchangeTrajectory => "interchange_trajectory",
            Self::SwebenchSubmission => "swebench_submission",
            Self::InspectEvalLog => "inspect_eval_log",
            Self::InTotoBundle => "in_toto_bundle",
            Self::TelemetryTrace => "telemetry_trace",
        }
    }

    /// Parse a canonical spelling — the one grammar (CC1). `None` on any
    /// other spelling; the caller refuses `FormatUnknown`, never guesses.
    pub fn parse(s: &str) -> Option<ForeignFormat> {
        Self::ALL.iter().copied().find(|f| f.as_str() == s)
    }

    /// Whether `import` lifts this format.
    pub fn lifts(self) -> bool {
        !matches!(self, Self::InterchangeTrajectory)
    }

    /// Whether `export` lowers to this format.
    pub fn exports(self) -> bool {
        !matches!(self, Self::HarborTrialDir)
    }

    /// The liftable spellings (the supported-formats list refusals print).
    pub fn liftable_spellings() -> Vec<&'static str> {
        Self::ALL
            .iter()
            .filter(|f| f.lifts())
            .map(|f| f.as_str())
            .collect()
    }
}

// ── lift-side claim extraction ────────────────────────────────────────────

/// What a format-aware lift finds inside the artefact: typed `Claim`s
/// (the format's own structure — never a re-asserted foreign fact),
/// `name_bindings` (foreign names; R-ID-4 keeps them out of pin slots),
/// `mapping_loss` entries, and mapping-report `notes`.
#[derive(Debug, Default)]
pub(crate) struct ForeignClaims {
    /// Typed claims minted from the artefact's structure.
    pub claims: Vec<Claim>,
    /// Foreign names (model handles, eval ids) — bindings, never pins.
    pub name_bindings: Vec<Json>,
    /// `mapping_loss` rows: `{member, class, detail}`.
    pub loss: Vec<Json>,
    /// Free-text notes on the mapping report.
    pub notes: Vec<String>,
}

fn loss_row(member: &str, class: &str, detail: &str) -> Json {
    Json::obj([
        ("member", Json::str(member)),
        ("address", Json::str("")),
        ("class", Json::str(class)),
        ("detail", Json::str(detail)),
    ])
}

fn claim(role: String, value: Json, prov: &Json) -> Claim {
    Claim {
        role,
        value,
        provenance: prov.clone(),
    }
}

fn parse_file(files: &BTreeMap<String, Vec<u8>>, path: &str) -> Option<Json> {
    std::str::from_utf8(files.get(path)?)
        .ok()
        .and_then(|t| json::parse_provider(t).ok())
}

/// `lift_claims(format, files, prov)` — the format-aware half of the
/// foreign lift: parse the artefact's own structure and mint the claims
/// it carries (all under the caller's `unverified` import provenance).
/// `harbor_*` formats have no typed structure beyond file digests —
/// the generic `foreign_digest`/`foreign_lock_digest` sweep
/// `lift_foreign` performs is their whole claim set.
pub(crate) fn lift_claims(
    format: ForeignFormat,
    files: &BTreeMap<String, Vec<u8>>,
    prov: &Json,
) -> ForeignClaims {
    match format {
        ForeignFormat::SwebenchSubmission => lift_swebench(files, prov),
        ForeignFormat::InspectEvalLog => lift_inspect(files, prov),
        ForeignFormat::InTotoBundle => lift_intoto(files, prov),
        ForeignFormat::TelemetryTrace => lift_otlp(files, prov),
        _ => ForeignClaims::default(),
    }
}

/// SWE-bench: lines (or a top-level array — the sb-cli list spelling)
/// carrying `instance_id` are prediction rows; other prediction fields
/// are named losses; a file with no prediction rows is a note, never
/// an error.
fn lift_swebench(files: &BTreeMap<String, Vec<u8>>, prov: &Json) -> ForeignClaims {
    let mut fc = ForeignClaims::default();
    for (path, bytes) in files {
        let name = path.rsplit('/').next().unwrap_or(path);
        if !(name.ends_with(".jsonl") || name.ends_with(".json")) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let mut rows: Vec<(String, Json)> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match json::parse_provider(line) {
                Ok(row) => rows.push((format!("{path}:{}", i + 1), row)),
                Err(_) if name.ends_with(".jsonl") => fc.loss.push(loss_row(
                    &format!("{path}:{}", i + 1),
                    "unparseable",
                    "predictions line is not JSON",
                )),
                Err(_) => {}
            }
        }
        if rows.is_empty() {
            if let Ok(Json::Arr(items)) = json::parse_provider(text.trim()) {
                for (i, item) in items.into_iter().enumerate() {
                    rows.push((format!("{path}[{i}]"), item));
                }
            }
        }
        if !rows
            .iter()
            .any(|(_, r)| r.get("instance_id").and_then(Json::as_str).is_some())
        {
            continue; // not a predictions artefact — some other file.
        }
        for (loc, row) in &rows {
            let Some(id) = row.get("instance_id").and_then(Json::as_str) else {
                continue;
            };
            let model = row
                .get("model_name_or_path")
                .and_then(Json::as_str)
                .unwrap_or("");
            let patch = row.get("model_patch").and_then(Json::as_str).unwrap_or("");
            fc.claims.push(claim(
                format!("swebench.instance:{id}"),
                Json::obj([
                    ("model", Json::str(model)),
                    ("patch_bytes", Json::Int(patch.len() as i64)),
                    (
                        "patch_digest",
                        Json::str(hh_identity::idp_id("blob", patch.as_bytes())),
                    ),
                    ("source", Json::str(loc.clone())),
                ]),
                prov,
            ));
            if !model.is_empty() {
                fc.name_bindings.push(Json::Obj(BTreeMap::from([
                    ("name".to_string(), Json::str(model)),
                    ("role".to_string(), Json::str("swebench.model_name_or_path")),
                ])));
            }
            if let Json::Obj(m) = row {
                for k in m.keys() {
                    if !matches!(
                        k.as_str(),
                        "instance_id" | "model_name_or_path" | "model_patch"
                    ) {
                        fc.loss.push(loss_row(
                            &format!("{loc}.{k}"),
                            "untyped_slot",
                            "prediction field with no native slot — the artefact member carries it verbatim",
                        ));
                    }
                }
            }
        }
    }
    if fc.claims.is_empty() {
        fc.notes
            .push("no swebench prediction rows — files lifted verbatim only".into());
    }
    fc
}

/// Inspect AI eval log (`.json` spelling): the header (`version`,
/// `status`, `eval{task,model}`), `results.scores[]` and per-sample
/// digests are the claims; `plan`/`stats`/`reductions`/sample
/// transcripts stay artefact-verbatim (`untyped_slot`/`narrowed` —
/// Inspect's transcript vocabulary is not our event set).
fn lift_inspect(files: &BTreeMap<String, Vec<u8>>, prov: &Json) -> ForeignClaims {
    let mut fc = ForeignClaims::default();
    for path in files.keys() {
        let Some(doc) = parse_file(files, path) else {
            continue;
        };
        let (Some(Json::Int(version)), Some(Json::Obj(eval))) =
            (doc.get("version"), doc.get("eval"))
        else {
            continue;
        };
        fc.claims
            .push(claim("inspect.version".into(), Json::Int(*version), prov));
        if let Some(s) = doc.get("status").and_then(Json::as_str) {
            fc.claims
                .push(claim("inspect.status".into(), Json::str(s), prov));
        }
        for (k, role) in [("task", "inspect.task"), ("model", "inspect.model")] {
            if let Some(v) = eval.get(k).and_then(Json::as_str) {
                fc.claims.push(claim(role.into(), Json::str(v), prov));
            }
        }
        if let Some(Json::Arr(samples)) = doc.get("samples") {
            for s in samples.iter().filter_map(|s| s.get("id")) {
                let label = s
                    .as_str()
                    .map(String::from)
                    .or_else(|| s.as_int().map(|i| i.to_string()))
                    .unwrap_or_default();
                fc.claims.push(claim(
                    format!("inspect.sample:{label}"),
                    Json::str(path.clone()),
                    prov,
                ));
            }
        }
        if let Some(Json::Arr(scores)) = doc.get("results").and_then(|r| r.get("scores")) {
            for s in scores {
                if let Some(name) = s.get("name").and_then(Json::as_str) {
                    fc.claims.push(claim(
                        format!("inspect.score:{name}"),
                        s.get("metrics").cloned().unwrap_or(Json::Null),
                        prov,
                    ));
                }
            }
        }
        for (k, d) in [
            ("plan", "solver plan — not claimable"),
            (
                "stats",
                "eval stats — foreign token accounting is not a TokenVector",
            ),
            ("reductions", "epoch reductions"),
            ("error", "eval error detail"),
        ] {
            if doc.get(k).is_some() {
                fc.loss
                    .push(loss_row(&format!("{path}.{k}"), "untyped_slot", d));
            }
        }
        if doc.get("samples").is_some() {
            fc.loss.push(loss_row(
                &format!("{path}.samples[].transcript"),
                "narrowed",
                "sample messages/events stay artefact-verbatim — no claim per transcript step",
            ));
        }
        return fc; // one eval log per artefact set.
    }
    fc.notes
        .push("no inspect eval log document found — files lifted verbatim only".into());
    fc
}

/// in-toto bundle: JSONL of DSSE envelopes, or a sigstore-shaped
/// `{mediaType, dsseEnvelope}` document. Subjects + predicate type are
/// the claims; signatures are *counted* with their keyids — never
/// verified (no trust roots are in scope; authority stays
/// `unverified`). A line that is not an envelope is a loss row.
fn lift_intoto(files: &BTreeMap<String, Vec<u8>>, prov: &Json) -> ForeignClaims {
    let mut fc = ForeignClaims::default();
    for (path, bytes) in files {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let mut envelopes: Vec<(String, Json)> = Vec::new();
        if let Some(doc) = parse_file(files, path) {
            if let Some(env) = doc.get("dsseEnvelope") {
                envelopes.push((format!("{path}.dsseEnvelope"), env.clone()));
            }
            if doc.get("verificationMaterial").is_some() {
                fc.loss.push(loss_row(
                    &format!("{path}.verificationMaterial"),
                    "no_slot",
                    "certificates/tlog entries are foreign trust material — carried verbatim, never adopted",
                ));
            }
        }
        if envelopes.is_empty() {
            for (i, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match json::parse_provider(line) {
                    Ok(env) if env.get("payload").and_then(Json::as_str).is_some() => {
                        envelopes.push((format!("{path}:{}", i + 1), env));
                    }
                    _ => fc.loss.push(loss_row(
                        &format!("{path}:{}", i + 1),
                        "unparseable",
                        "bundle line is not a DSSE envelope",
                    )),
                }
            }
        }
        for (loc, env) in &envelopes {
            let Some(stmt) = env
                .get("payload")
                .and_then(Json::as_str)
                .and_then(b64_decode)
                .and_then(|b| String::from_utf8(b).ok())
                .and_then(|t| json::parse_provider(&t).ok())
            else {
                fc.loss.push(loss_row(
                    loc,
                    "unparseable",
                    "envelope payload does not decode to a JSON statement",
                ));
                continue;
            };
            if let Some(Json::Arr(subjects)) = stmt.get("subject") {
                for s in subjects {
                    if let (Some(name), Some(digest)) =
                        (s.get("name").and_then(Json::as_str), s.get("digest"))
                    {
                        fc.claims.push(claim(
                            format!("intoto.subject:{name}"),
                            Json::obj([
                                ("digest", digest.clone()),
                                ("source", Json::str(loc.clone())),
                            ]),
                            prov,
                        ));
                    }
                }
            }
            if let Some(pt) = stmt.get("predicateType").and_then(Json::as_str) {
                fc.claims.push(claim(
                    format!("intoto.predicate:{pt}"),
                    Json::str(loc.clone()),
                    prov,
                ));
            }
            if let Some(Json::Arr(sigs)) = env.get("signatures") {
                // A signature is a claim — keyid recorded, never verified
                // (no trust roots; authority stays `unverified`).
                for (i, s) in sigs.iter().enumerate() {
                    fc.claims.push(claim(
                        format!("intoto.signature:{i}"),
                        Json::obj([
                            ("keyid", s.get("keyid").cloned().unwrap_or(Json::Null)),
                            ("status", Json::str("present_unverified")),
                            ("source", Json::str(loc.clone())),
                        ]),
                        prov,
                    ));
                }
            }
        }
    }
    if fc.claims.is_empty() {
        fc.notes
            .push("no DSSE envelopes found — files lifted verbatim only".into());
    }
    fc
}

/// OTLP/JSON trace: `service.name` attributes, distinct trace ids and
/// the span count are the claims; span attributes/events/links and
/// non-trace resource sections are `narrowed`/`untyped_slot` losses
/// (the span stream is a view, never the event record).
fn lift_otlp(files: &BTreeMap<String, Vec<u8>>, prov: &Json) -> ForeignClaims {
    let mut fc = ForeignClaims::default();
    for path in files.keys() {
        let Some(doc) = parse_file(files, path) else {
            continue;
        };
        let Some(Json::Arr(rspans)) = doc.get("resourceSpans") else {
            continue;
        };
        let mut span_count = 0i64;
        let mut traces: std::collections::BTreeSet<String> = Default::default();
        let mut attrs_present = false;
        for rs in rspans {
            if let Some(Json::Arr(attrs)) = rs.get("resource").and_then(|r| r.get("attributes")) {
                for a in attrs {
                    if a.get("key").and_then(Json::as_str) == Some("service.name") {
                        if let Some(v) = a
                            .get("value")
                            .and_then(|v| v.get("stringValue"))
                            .and_then(Json::as_str)
                        {
                            fc.claims.push(claim(
                                format!("otel.service:{v}"),
                                Json::str(path.clone()),
                                prov,
                            ));
                        }
                    }
                }
            }
            if let Some(Json::Arr(scopes)) = rs.get("scopeSpans") {
                for spans in scopes.iter().filter_map(|ss| ss.get("spans")) {
                    if let Json::Arr(spans) = spans {
                        for sp in spans {
                            span_count += 1;
                            if let Some(t) = sp.get("traceId").and_then(Json::as_str) {
                                traces.insert(t.to_string());
                            }
                            attrs_present |= sp.get("attributes").is_some()
                                || sp.get("events").is_some()
                                || sp.get("links").is_some();
                        }
                    }
                }
            }
        }
        for t in &traces {
            fc.claims.push(claim(
                format!("otel.trace:{t}"),
                Json::str(path.clone()),
                prov,
            ));
        }
        fc.claims
            .push(claim("otel.span_count".into(), Json::Int(span_count), prov));
        if attrs_present {
            fc.loss.push(loss_row(
                &format!("{path}.spans[].attributes|events|links"),
                "narrowed",
                "span attributes/events/links stay artefact-verbatim — no claim per member",
            ));
        }
        for k in ["resourceLogs", "resourceMetrics"] {
            if doc.get(k).is_some() {
                fc.loss.push(loss_row(
                    &format!("{path}.{k}"),
                    "untyped_slot",
                    "non-trace OTLP section — carried verbatim, no typed claim",
                ));
            }
        }
        return fc; // one trace document per artefact set.
    }
    fc.notes
        .push("no OTLP resourceSpans document found — files lifted verbatim only".into());
    fc
}

// ── export arms ───────────────────────────────────────────────────────────

/// Decode every subject run's ledger pages → `run_id → envelopes`.
fn events_of(
    manifest: &BundleManifest,
    members: &MemberBytes,
) -> Result<BTreeMap<String, Vec<hh_ledger::event::EventEnvelope>>, BundleError> {
    let mut out = BTreeMap::new();
    for (run, export) in &manifest.traces {
        let mut evs = Vec::new();
        for page_addr in &export.pages {
            if let Some(bytes) = members.get(page_addr) {
                evs.extend(crate::export::decode_page(bytes)?);
            }
        }
        out.insert(run.clone(), evs);
    }
    Ok(out)
}

/// The member sweep every foreign export ends with: ledger pages/trees
/// are `narrowed` (the format reduces the trace), anything else the
/// format does not express is `untyped_slot` — never silently absent.
fn member_loss_sweep(manifest: &BundleManifest, loss: &mut Vec<Json>, detail: &str) {
    for m in &manifest.members {
        if loss
            .iter()
            .any(|e| e.get("member").and_then(Json::as_str) == Some(m.role.as_str()))
        {
            continue;
        }
        let class = if m.role.starts_with("ledger_page:") || m.role.starts_with("ledger_tree:") {
            "narrowed"
        } else {
            "untyped_slot"
        };
        loss.push(Json::obj([
            ("member", Json::str(m.role.clone())),
            ("address", Json::str(m.address.clone())),
            ("class", Json::str(class)),
            ("detail", Json::str(detail)),
        ]));
    }
}

/// `manifest.json` + `digests.json` — the self-describing trailer every
/// symmetric export ships (a re-import verifies the digests and reads
/// the manifest member as the foreign manifest record).
fn trailer(manifest: &BundleManifest, files: &mut BTreeMap<String, Vec<u8>>) {
    files.insert(
        "manifest.json".into(),
        manifest.to_json().to_canonical_string().into_bytes(),
    );
    let digests = crate::fetch::digest_index(files);
    files.insert(
        "digests.json".into(),
        Json::Obj(
            digests
                .iter()
                .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                .collect(),
        )
        .to_canonical_string()
        .into_bytes(),
    );
}

/// The bundle's model-snapshot display name ("" when unpinned — callers
/// substitute the bundle id and record the loss).
fn model_name(manifest: &BundleManifest) -> String {
    manifest
        .model
        .get("snapshots")
        .and_then(|s| match s {
            Json::Arr(a) => a.first(),
            _ => None,
        })
        .and_then(|s| {
            s.get("name")
                .or_else(|| s.get("version_id"))
                .and_then(Json::as_str)
        })
        .unwrap_or("")
        .to_string()
}

/// The run's declared `task_ref.task_id` off its `lifecycle.run.created`
/// payload (`None` when the run declares none).
fn task_id_of(evs: &[hh_ledger::event::EventEnvelope]) -> Option<String> {
    evs.iter()
        .find(|e| e.class == "lifecycle.run.created")
        .and_then(|e| e.payload.get("task_ref"))
        .and_then(|t| t.get("task_id"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

/// `export_arm(format, manifest, members, files, loss)` — dispatch the
/// four R4.9 foreign lowerings (the harbor/interchange arms stay inline
/// in `export.rs`; a non-exportable format is `ForeignDirectionRefused`,
/// never a silent skip).
pub(crate) fn export_arm(
    format: ForeignFormat,
    manifest: &BundleManifest,
    members: &MemberBytes,
    files: &mut BTreeMap<String, Vec<u8>>,
    loss: &mut Vec<Json>,
) -> Result<(), BundleError> {
    match format {
        ForeignFormat::SwebenchSubmission => export_swebench(manifest, members, files, loss),
        ForeignFormat::InspectEvalLog => export_inspect(manifest, members, files, loss),
        ForeignFormat::InTotoBundle => export_intoto(manifest, members, files, loss),
        ForeignFormat::TelemetryTrace => export_telemetry(manifest, members, files, loss),
        other => Err(BundleError::ForeignDirectionRefused {
            format: other.as_str().to_string(),
            direction: "export",
        }),
    }
}

/// The `swebench_submission` arm — `predictions.jsonl` (one row per
/// subject run). `instance_id` is the run's `task_ref.task_id` when
/// declared, else the run id with a `narrowed` loss row; `model_patch`
/// is the newest recorded `patch`/`diff` payload field, else the empty
/// string the schema reads as *skip* — with a `no_slot` row, never a
/// fabricated diff.
fn export_swebench(
    manifest: &BundleManifest,
    members: &MemberBytes,
    files: &mut BTreeMap<String, Vec<u8>>,
    loss: &mut Vec<Json>,
) -> Result<(), BundleError> {
    let events = events_of(manifest, members)?;
    let model = model_name(manifest);
    let model = if model.is_empty() {
        loss.push(loss_row(
            "model_name_or_path",
            "no_slot",
            "the bundle pins no model snapshot name — the row carries the bundle id",
        ));
        format!("hh-bundle:{}", manifest.version_id)
    } else {
        model
    };
    let mut lines = String::new();
    for run in &manifest.subject.run_ids {
        let evs = events.get(run).cloned().unwrap_or_default();
        let instance = match task_id_of(&evs) {
            Some(t) => t,
            None => {
                loss.push(loss_row(
                    &format!("instance_id:{run}"),
                    "narrowed",
                    "the run declares no task_ref — the run id stands in as the instance coordinate",
                ));
                run.clone()
            }
        };
        let patch = evs
            .iter()
            .rev()
            .find_map(|e| {
                ["model_patch", "patch", "diff", "patch_text"]
                    .iter()
                    .find_map(|k| e.payload.get(k).and_then(Json::as_str))
            })
            .unwrap_or("")
            .to_string();
        if patch.is_empty() {
            loss.push(loss_row(
                &format!("model_patch:{run}"),
                "no_slot",
                "no recorded patch/diff member on the trace — the empty patch is the format's documented skip marker, never a fabricated diff",
            ));
        }
        let row = Json::obj([
            ("instance_id", Json::str(instance)),
            ("model_name_or_path", Json::str(model.clone())),
            ("model_patch", Json::str(patch)),
        ]);
        lines.push_str(&row.to_canonical_string());
        lines.push('\n');
    }
    files.insert("predictions.jsonl".into(), lines.into_bytes());
    for run in &manifest.subject.run_ids {
        loss.push(loss_row(
            &format!("trace:{run}"),
            "narrowed",
            "the event trace reduces to the patch field — the submission schema carries no trajectory slot",
        ));
    }
    member_loss_sweep(
        manifest,
        loss,
        "no swebench_submission slot — the artefact carries predictions + manifest only",
    );
    trailer(manifest, files);
    Ok(())
}

/// The `inspect_eval_log` arm — `eval_log.json` (the `.json` EvalLog
/// spelling). Scores come from `verification.validator.verdict` /
/// `measurement.metric.emitted` events only — never a fabricated metric.
fn export_inspect(
    manifest: &BundleManifest,
    members: &MemberBytes,
    files: &mut BTreeMap<String, Vec<u8>>,
    loss: &mut Vec<Json>,
) -> Result<(), BundleError> {
    let events = events_of(manifest, members)?;
    let first_run = manifest
        .subject
        .run_ids
        .first()
        .cloned()
        .unwrap_or_default();
    let task = events
        .get(&first_run)
        .map(|e| task_id_of(e))
        .unwrap_or(None);
    if task.is_none() {
        loss.push(loss_row(
            "eval.task",
            "no_slot",
            "no task_ref on the subject run — eval.task carries the bundle id",
        ));
    }
    let model = model_name(manifest);
    let model = if model.is_empty() {
        loss.push(loss_row(
            "eval.model",
            "no_slot",
            "the bundle pins no model snapshot name — eval.model carries the bundle id",
        ));
        format!("hh-bundle:{}", manifest.version_id)
    } else {
        model
    };
    let status = match manifest.subject.status.as_str() {
        "finished" => "success",
        "open" => "started",
        _ => "error",
    };
    if status == "error" {
        loss.push(loss_row(
            "status",
            "narrowed",
            "subject.status maps onto inspect's 4-status vocabulary — non-finished lowers to `error`",
        ));
    }
    let mut samples = Vec::new();
    let mut score_rows = Vec::new();
    for (run, evs) in &events {
        let mut scores = BTreeMap::new();
        for e in evs {
            let (name, value) = match e.class.as_str() {
                "verification.validator.verdict" => (
                    e.payload
                        .get("validator")
                        .and_then(Json::as_str)
                        .unwrap_or("verdict")
                        .to_string(),
                    e.payload.get("verdict").cloned().unwrap_or(Json::Null),
                ),
                "measurement.metric.emitted" => {
                    match e.payload.get("name").and_then(Json::as_str) {
                        Some(n) => (
                            n.to_string(),
                            e.payload.get("value").cloned().unwrap_or(Json::Null),
                        ),
                        None => continue,
                    }
                }
                _ => continue,
            };
            scores.insert(name.clone(), Json::obj([("value", value.clone())]));
            score_rows.push(Json::obj([
                ("name", Json::str(name)),
                ("run_id", Json::str(run.clone())),
                ("value", value),
            ]));
        }
        samples.push(Json::obj([
            ("id", Json::str(run.clone())),
            ("epoch", Json::Int(1)),
            ("scores", Json::Obj(scores)),
            (
                "metadata",
                Json::obj([
                    ("hh.run_id", Json::str(run.clone())),
                    ("hh.bundle_id", Json::str(manifest.version_id.clone())),
                ]),
            ),
        ]));
    }
    let doc = Json::obj([
        ("version", Json::Int(2)),
        ("status", Json::str(status)),
        (
            "eval",
            Json::obj([
                ("eval_id", Json::str(manifest.version_id.clone())),
                ("run_id", Json::str(first_run)),
                (
                    "task",
                    Json::str(task.unwrap_or_else(|| manifest.version_id.clone())),
                ),
                ("model", Json::str(model)),
                ("created", Json::str(manifest.created_at.clone())),
            ]),
        ),
        (
            "results",
            Json::obj([
                ("total_samples", Json::Int(samples.len() as i64)),
                ("completed_samples", Json::Int(samples.len() as i64)),
                ("scores", Json::Arr(score_rows)),
            ]),
        ),
        ("samples", Json::Arr(samples)),
    ]);
    files.insert(
        "eval_log.json".into(),
        doc.to_canonical_string().into_bytes(),
    );
    loss.push(loss_row(
        "samples[].transcript",
        "narrowed",
        "samples carry id+scores+metadata only — message transcripts are not fabricated",
    ));
    loss.push(loss_row(
        "plan|stats|reductions",
        "no_slot",
        "the bundle has no solver-plan/usage/reduction sections — absent, never zeroed",
    ));
    member_loss_sweep(
        manifest,
        loss,
        "no inspect_eval_log slot — the artefact carries the eval log + manifest only",
    );
    trailer(manifest, files);
    Ok(())
}

/// The `in_toto_bundle` arm — `attestation.intoto.jsonl`, one DSSE
/// envelope wrapping an in-toto `Statement/v1` whose `subject[]` is the
/// bundle plus every present member (`{name: role, digest: sha256}`).
/// The envelope ships **unsigned** (`signatures: []`): signing is
/// `attest()`'s claim — a lowering never fabricates a signature, and
/// the loss report names the asymmetry.
fn export_intoto(
    manifest: &BundleManifest,
    _members: &MemberBytes,
    files: &mut BTreeMap<String, Vec<u8>>,
    loss: &mut Vec<Json>,
) -> Result<(), BundleError> {
    let mut subjects = vec![Json::obj([
        ("name", Json::str(manifest.version_id.clone())),
        (
            "digest",
            Json::obj([("sha256", Json::str(bare_sha256(&manifest.version_id)))]),
        ),
    ])];
    for m in &manifest.members {
        if m.status != crate::manifest::MemberStatus::Present {
            loss.push(loss_row(
                &m.role,
                "no_slot",
                "a non-present member cannot be named as a statement subject",
            ));
            continue;
        }
        subjects.push(Json::obj([
            ("name", Json::str(m.role.clone())),
            (
                "digest",
                Json::obj([("sha256", Json::str(bare_sha256(&m.address)))]),
            ),
        ]));
    }
    let statement = Json::obj([
        ("_type", Json::str("https://in-toto.io/Statement/v1")),
        ("subject", Json::Arr(subjects)),
        (
            "predicateType",
            Json::str("https://harnessharness.dev/predicate/bundle-manifest/v1"),
        ),
        (
            "predicate",
            Json::obj([
                ("bundle_id", Json::str(manifest.version_id.clone())),
                ("bundle_kind", Json::str(manifest.bundle_kind.clone())),
                ("created_at", Json::str(manifest.created_at.clone())),
                ("producer", manifest.producer.clone()),
                (
                    "participant_class",
                    Json::str(manifest.participant_class.clone()),
                ),
            ]),
        ),
    ]);
    let envelope = Json::obj([
        ("payloadType", Json::str("application/vnd.in-toto+json")),
        (
            "payload",
            Json::str(b64_encode(statement.to_canonical_string().as_bytes())),
        ),
        ("signatures", Json::Arr(vec![])),
    ]);
    let mut line = envelope.to_canonical_string();
    line.push('\n');
    files.insert("attestation.intoto.jsonl".into(), line.into_bytes());
    loss.push(loss_row(
        "signatures",
        "no_slot",
        "unsigned — signing is attest()'s claim, never a lowering; a foreign signature lifts back as an unverified claim (the format's one asymmetric channel)",
    ));
    loss.push(loss_row(
        "member_bytes",
        "narrowed",
        "member payloads stay behind their subject digests — the statement attests bytes it does not carry",
    ));
    trailer(manifest, files);
    Ok(())
}

/// The `telemetry_trace` arm — `traces.json`, an OTLP/JSON
/// `ExportTraceServiceRequest`: one `resourceSpans` per subject run,
/// one span per ledger event. Span/trace ids are deterministic
/// digest-derived hex (the same event always lowers to the same ids);
/// event payloads narrow to `hh.*` span attributes — the span stream
/// cannot re-derive the event set, and the loss row says so.
fn export_telemetry(
    manifest: &BundleManifest,
    members: &MemberBytes,
    files: &mut BTreeMap<String, Vec<u8>>,
    loss: &mut Vec<Json>,
) -> Result<(), BundleError> {
    let events = events_of(manifest, members)?;
    let attr = |k: &str, v: Json| {
        Json::obj([
            ("key", Json::str(k)),
            (
                "value",
                match v {
                    Json::Int(i) => Json::obj([("intValue", Json::str(i.to_string()))]),
                    other => Json::obj([("stringValue", other)]),
                },
            ),
        ])
    };
    let mut resource_spans = Vec::new();
    let mut time_loss = false;
    for (run, evs) in &events {
        let trace_id = hh_identity::idp_digest("otel.trace", run.as_bytes());
        let mut spans = Vec::new();
        for e in evs {
            let mut span = BTreeMap::from([
                ("traceId".to_string(), Json::str(&trace_id[..32])),
                (
                    "spanId".to_string(),
                    Json::str(&hh_identity::idp_digest("otel.span", e.event_id.as_bytes())[..16]),
                ),
                ("name".to_string(), Json::str(e.class.clone())),
                ("kind".to_string(), Json::Int(1)), // SPAN_KIND_INTERNAL
                (
                    "attributes".to_string(),
                    Json::Arr(vec![
                        attr("hh.event.class", Json::str(e.class.clone())),
                        attr("hh.seq", Json::Int(e.seq as i64)),
                        attr("hh.event_id", Json::str(e.event_id.clone())),
                        attr("hh.ts", Json::str(e.ts.clone())),
                    ]),
                ),
                ("status".to_string(), Json::obj([("code", Json::Int(1))])),
            ]);
            if let Some(ns) = rfc3339_unix_nanos(&e.ts) {
                span.insert("startTimeUnixNano".into(), Json::str(ns.clone()));
                span.insert("endTimeUnixNano".into(), Json::str(ns));
            } else {
                time_loss = true;
            }
            spans.push(Json::Obj(span));
        }
        resource_spans.push(Json::obj([
            (
                "resource",
                Json::obj([(
                    "attributes",
                    Json::Arr(vec![
                        attr("service.name", Json::str("hh-bundle")),
                        attr("hh.bundle.id", Json::str(manifest.version_id.clone())),
                        attr("hh.run_id", Json::str(run.clone())),
                    ]),
                )]),
            ),
            (
                "scopeSpans",
                Json::Arr(vec![Json::obj([
                    ("scope", Json::obj([("name", Json::str("hh-ledger"))])),
                    ("spans", Json::Arr(spans)),
                ])]),
            ),
        ]));
        loss.push(loss_row(
            &format!("event_payloads:{run}"),
            "narrowed",
            "event payloads/provenance narrow to hh.* span attributes — the span stream cannot re-derive the event set",
        ));
    }
    if time_loss {
        loss.push(loss_row(
            "span_times",
            "narrowed",
            "an event ts that is not RFC-3339 UTC-ms lowers without span times",
        ));
    }
    files.insert(
        "traces.json".into(),
        Json::obj([("resourceSpans", Json::Arr(resource_spans))])
            .to_canonical_string()
            .into_bytes(),
    );
    member_loss_sweep(
        manifest,
        loss,
        "no telemetry_trace slot — the artefact carries the span set + manifest only",
    );
    trailer(manifest, files);
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────

/// `sha256:<hex>` → `<hex>` (in-toto digest fields carry the bare hex;
/// the idp spelling stays the canonical one everywhere else).
fn bare_sha256(id: &str) -> String {
    id.rsplit(':').next().unwrap_or(id).to_string()
}

/// RFC 4648 base64 (the DSSE `payload` spelling) — the workspace has no
/// base64 codec; this is the one (CC1), private to the foreign arms.
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |acc, b| (acc << 8) | *b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| B64.iter().position(|&b| b == c).map(|p| p as u32);
    let clean: Vec<u8> = s.bytes().filter(|b| !b" \t\r\n".contains(b)).collect();
    if clean.is_empty() {
        return Some(vec![]);
    }
    // '=' is padding — legal only in the final two slots.
    let pad = clean.iter().rev().take_while(|&&b| b == b'=').count();
    if !clean.len().is_multiple_of(4) || pad > 2 || clean[..clean.len() - pad].contains(&b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= (if c == b'=' { 0 } else { val(c)? }) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// `YYYY-MM-DDTHH:MM:SS[.fff…]Z` → unix nanos as a decimal string
/// (OTLP's int64-as-string). `None` on any other shape — the caller
/// records the loss, never fabricates a timestamp.
pub(crate) fn rfc3339_unix_nanos(ts: &str) -> Option<String> {
    let b = ts.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || (b[10] != b'T' && b[10] != b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { ts.get(a..z)?.parse().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let rest = &ts[19..];
    let (frac_ns, tail) = match rest.strip_prefix('.') {
        Some(f) => {
            let digits: String = f.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return None;
            }
            let ns =
                digits.parse::<u64>().ok()? * 10u64.pow(9u32.saturating_sub(digits.len() as u32));
            (ns, &f[digits.len()..])
        }
        None => (0, rest),
    };
    if tail != "Z" {
        return None; // offsets are out of the envelope's grammar (always Z).
    }
    let days = days_from_civil(y, mo, d);
    let nanos = ((days * 86_400 + h * 3_600 + mi * 60 + s) as u64) * 1_000_000_000 + frac_ns;
    Some(nanos.to_string())
}

/// Days since 1970-01-01 for a civil date (Hinnant's algorithm — the
/// inverse of `hh_ledger::store::rfc3339_ms`'s civil-from-days).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = doy + 365 * yoe + yoe / 4 - yoe / 100;
    era * 146_097 + doe - 719_468
}
