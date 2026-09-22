//! The Group M/L bundle + measurement ops (S3.1; §7.4 Groups M/L;
//! R-2.9.3⁰ `bundle(kind = run)` / `validate_bundle` / `reproduce` /
//! `import`; R-2.11.3⁰'s `serve(bundle)` boundary half; ADR-0139…0141;
//! ADR-0176 D4).
//!
//! Boundary rules these honor:
//! - **CC9 matched budgets** — `kernel.reproduce` refuses
//!   `UnmatchedBudget` when the caller's `eval_budget` does not match the
//!   manifest's declared one; the report is a record, never a guess.
//! - **R-NOSIDE** — no handle id ever crosses the wire; bundle results
//!   carry content addresses and manifests, `lab.serve` returns the
//!   `stdio_launch` binding record + the launch descriptor the caller
//!   uses to spawn `hh-mcp-serve` (the stdio pair is the caller's).
//! - **Import is a lift** — `kernel.import` mints the receipt row under
//!   kernel provenance naming *coordinates only*; lifted member bytes
//!   keep `authority = unverified` (ADR-0141 D1 — no foreign fact is
//!   re-asserted as kernel truth).
//! - **Writer sessions only** for the appending ops (`emit_metric`,
//!   `kernel.bundle`, `kernel.import`); the read-shaped ops
//!   (`check_completeness`, `reproduce`, `lab.serve`) take no session —
//!   they are pure functions of the addressed bundle.

use std::collections::BTreeMap;
use std::path::Path;

use hh_bundle::assemble::{assemble, AssembleInputs, CompileOutcome};
use hh_bundle::codec::{decode_container, decode_dir, encode_dir, Decoded};
use hh_bundle::error::BundleError;
use hh_bundle::export::decode_page;
use hh_bundle::import::{lift, NATIVE_FORMAT};
use hh_bundle::levels;
use hh_bundle::manifest::{BundlePolicy, ReproLevel};
use hh_bundle::repro::{
    budget_limits_equal, fingerprint_drift, snapshot_fingerprint, ReproOutcome, ReproReport,
};
use hh_bundle::validate::check_completeness;
use hh_embed_schema::errors::EmbedError;
use hh_ledger::event::{Event, EventEnvelope, Producer, Scope};
use hh_ledger::manifest::RunManifest;
use hh_ontology::eval::{EvalError, MetricValue};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::service::{ledger_err, EmbedService};

/// The producer component the bundle rows stamp (`kernel:bundle` — the
/// §5h.3 record's producer).
const BUNDLE_COMPONENT: &str = "kernel:bundle";

/// The producer component the import receipt row stamps.
const IMPORT_COMPONENT: &str = "kernel:import";

/// `idp/1` blob-pool media type for `application/json` member bytes.
const MEDIA_JSON: &str = "application/json";

/// The `measurement.metric.emitted` class (§5h.1's Lab row — Group M's
/// write op mints it under the session's writer lease, byte-identical
/// to the path any client's write takes; AC-R-2.11.4-10).
const METRIC_EMITTED: &str = "measurement.metric.emitted";

fn missing(path: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: "missing".to_string(),
    }
}

fn str_at<'a>(params: &'a Json, op: &str, key: &str) -> Result<&'a str, EmbedError> {
    params
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| missing(&format!("{op}/{key}")))
}

/// The `bundle` argument — `{path: dir}` or `{container: file}` decoded
/// to `Decoded`. A path that is neither a readable directory nor an
/// HHB1 container is the typed `Refused{bundle_unreadable}`, never a
/// panic.
fn decode_bundle_arg(params: &Json, op: &str) -> Result<Decoded, EmbedError> {
    let dir = params.get("path").and_then(Json::as_str);
    let container = params.get("container").and_then(Json::as_str);
    match (dir, container) {
        (Some(d), None) => decode_dir(Path::new(d)).map_err(bundle_err),
        (None, Some(c)) => std::fs::read(c)
            .map_err(|e| EmbedError::Refused {
                reason: format!("bundle_unreadable: {e}"),
            })
            .and_then(|b| decode_container(&b).map_err(bundle_err)),
        (None, None) => Err(missing(&format!("{op}/path|container"))),
        (Some(_), Some(_)) => Err(EmbedError::SchemaViolation {
            path: format!("{op}/path"),
            code: "mutually_exclusive_with_container".to_string(),
        }),
    }
}

/// `BundleError` → the contract's closed sum — `Refused{reason}`
/// carrying the typed detail, never an `internal_error`.
fn bundle_err(e: BundleError) -> EmbedError {
    EmbedError::Refused {
        reason: format!("bundle: {e}"),
    }
}

/// Mint one kernel-origin `Event` for `run_id` — the same envelope shape
/// `hh_env::events::EventMinter` produces, stamped `kernel(hh-embed)`
/// (the boundary component — a Lab write is minted by the kernel, the
/// same as every client's; AC-R-2.11.4-10).
fn mint_embed_event(
    store: &hh_ledger::Store,
    run_id: &str,
    class: &str,
    payload: Json,
) -> Result<Event, EmbedError> {
    Ok(Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-embed"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).map_err(ledger_err)?,
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("hh-embed", store.now_ms())),
        content_kind: None,
        payload,
    })
}

/// `profile_binding` coordinates the sealed document pins — the compile
/// inputs' `profile_refs`. The walk is over the canonical document JSON
/// (a `profile_binding` object carries `profile_ref.profile`; the
/// `unbound`/`constraint` spellings contribute nothing).
fn pinned_profile_refs(sealed: &hh_hir::HirDocument) -> Vec<String> {
    fn walk(j: &Json, out: &mut Vec<String>) {
        if let Some(pr) = j.get("profile_binding").and_then(|b| b.get("profile_ref")) {
            if let Some(c) = pr.get("profile").and_then(Json::as_str) {
                out.push(c.to_string());
            }
        }
        match j {
            Json::Obj(m) => m.values().for_each(|v| walk(v, out)),
            Json::Arr(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut refs = Vec::new();
    if let Some(doc) = std::str::from_utf8(&sealed.canonical_bytes())
        .ok()
        .and_then(|t| hh_wire::json::parse(t).ok())
    {
        walk(&doc, &mut refs);
    }
    refs.sort();
    refs.dedup();
    refs
}

/// The `EnvironmentRecord` document the run executed under — read off
/// the run's `action.environment.declared` row (`{class, image,
/// version_id}` — the bundle's `environment` member shape). A run with
/// no environment carries `{}` (the member is absent → the R1/R2 basis
/// reports `Missing`, never a fabricated record).
fn environment_doc(store: &hh_ledger::Store, run_id: &str) -> Json {
    let events = match store.envelopes(run_id) {
        Ok(e) => e,
        Err(_) => return Json::obj([]),
    };
    for env in events {
        if env.class == "action.environment.declared" {
            let p = &env.payload;
            return Json::obj([
                ("class", p.get("class").cloned().unwrap_or(Json::Null)),
                ("image", p.get("image").cloned().unwrap_or(Json::Null)),
                (
                    "version_id",
                    p.get("environment_ref")
                        .and_then(|r| r.get("version_id"))
                        .cloned()
                        .unwrap_or(Json::Null),
                ),
            ]);
        }
    }
    Json::obj([])
}

impl EmbedService {
    /// The `ContractIdentity` every assembled bundle stamps (§5h.3
    /// `instrument.contract`; R-3.1: contracts ride in bundles).
    fn contract_identity(&self) -> Json {
        Json::obj([
            ("contract_major", Json::str("hh-embed/1")),
            ("schema_hash", Json::str(hh_embed_schema::schema_hash())),
            ("kernel_version_id", Json::str(self.kernel_version.clone())),
        ])
    }

    // ── Group M — measurement.emit_metric ────────────────────────────

    /// `measurement.emit_metric{session_id, metrics[], idempotency_key}`
    /// — append one `measurement.metric.emitted` row per `MetricValue`
    /// under the session's writer lease. `idempotency_key` is the §5g.6
    /// work-injecting key — a replayed call returns the recorded ack.
    pub(crate) fn emit_metric(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = str_at(params, "measurement.emit_metric", "session_id")?.to_string();
        let key = str_at(params, "measurement.emit_metric", "idempotency_key")?.to_string();
        let metrics = match params.get("metrics") {
            Some(Json::Arr(a)) => a.clone(),
            _ => return Err(missing("measurement.emit_metric/metrics")),
        };
        let s = self.writer_session(&session_id)?;
        if let Some(hit) = s.idem.get(&key) {
            return Ok(hit.clone());
        }
        let lease = s.lease.clone().ok_or(EmbedError::Refused {
            reason: "session_is_read_only".to_string(),
        })?;
        let run_id = s.run_id.clone();
        let mut values = Vec::with_capacity(metrics.len());
        for (i, m) in metrics.iter().enumerate() {
            values.push(MetricValue::from_json(m).map_err(|e| match e {
                EvalError::SchemaViolation { member, detail } => EmbedError::SchemaViolation {
                    path: format!("measurement.emit_metric/metrics[{i}]/{member}"),
                    code: detail,
                },
            })?);
        }
        let mut batch = Vec::with_capacity(values.len());
        for v in &values {
            batch.push(mint_embed_event(
                &self.store,
                &run_id,
                METRIC_EMITTED,
                v.to_json(),
            )?);
        }
        let range = self
            .store
            .append(&run_id, &lease, batch)
            .map_err(ledger_err)?;
        let out = Json::obj([
            ("emitted", Json::Int(values.len() as i64)),
            (
                "seq",
                Json::obj([
                    ("first", Json::Int(range.first as i64)),
                    ("last", Json::Int(range.last as i64)),
                ]),
            ),
        ]);
        self.session_mut(&session_id)?.idem.insert(key, out.clone());
        Ok(out)
    }

    // ── Group M — kernel.bundle ─────────────────────────────────────

    /// `kernel.bundle{run_id, kind?, policy?, deliver_sink?}` —
    /// `bundle(kind = run)` over the named run: assemble the
    /// `hh-bundle/1` manifest + members, deposit every member into the
    /// blob pool, append `measurement.experiment.bundle_assembled`, and
    /// — when `deliver_sink` names a directory — write the encoded
    /// bundle there and append `measurement.export.delivered` (the
    /// §5h.5 delivery row; DF-S1.14-4's append site, exercised here).
    ///
    /// Session-free like the other `kernel.*` derivations (R-BUNDLE-2):
    /// the appended rows ride the run's persisted writer-lease
    /// generation through `commit_kernel_row_for`, so a run whose writer
    /// session is already closed still exports. Determinism note: the
    /// manifest's `created_at` differs per call, so repeat calls append
    /// a new `bundle_assembled` row and yield a new `version_id` — each
    /// assembly is a distinct derivation (AC-R-2.9.3-2 is about the
    /// codec, not assembly).
    ///
    /// `kind` other than `run` is `Refused{kind_pending}` — corpus and
    /// profile bundles are later-stage kinds (ADR-0275 R-BUNDLE-1).
    pub(crate) fn kernel_bundle(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let run_id = str_at(params, "kernel.bundle", "run_id")?.to_string();
        let kind = params
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("run")
            .to_string();
        if kind != "run" {
            return Err(EmbedError::Refused {
                reason: format!("kind_pending:{kind}"),
            });
        }
        let policy = BundlePolicy::from_json(params.get("policy")).map_err(bundle_err)?;
        let deliver_sink = params
            .get("deliver_sink")
            .and_then(Json::as_str)
            .map(String::from);
        let run_manifest: RunManifest = self.store.manifest(&run_id).map_err(ledger_err)?.clone();

        // The compile output — `compiled_bundle` is the R1 gate. The
        // kernel re-runs `compile` over the persisted sealed bytes (the
        // same pipeline `hh-compile` wraps; the out-of-process
        // byte-parity AC is the compile's own). A compile failure
        // surfaces as the typed error — the bundle is not silently
        // degraded to R0.
        // A compile that cannot run (unresolvable profile, missing
        // artifact) does not sink the export — the bundle assembles
        // without a `compiled_bundle` member, `max_supported_level` caps
        // at R0, and the refusal rides in the boundary result's
        // `compile_error` (visible, never hidden; the R1 gate simply
        // finds no member). Hard failures of the *assembler* still
        // refuse.
        let mut compile_error = Json::Null;
        let compiled = match self.compile_for_bundle(&run_manifest) {
            Ok(c) => c,
            Err(e) => {
                compile_error = Json::str(format!("{e:?}"));
                None
            }
        };
        let environment = environment_doc(&self.store, &run_id);
        let budget = Json::Null;
        let kernel_prov = ProvenanceRecord::kernel(BUNDLE_COMPONENT, self.store.now_ms());
        let inputs = AssembleInputs {
            store: &self.store,
            run_id: &run_id,
            policy: &policy,
            created_at: hh_ledger::store::rfc3339_ms(self.store.now_ms()),
            producer: kernel_prov.to_json(),
            contract_identity: self.contract_identity(),
            kernel_version_id: self.kernel_version.clone(),
            instrument_dirty: std::env::var("HH_KERNEL_DIRTY").ok().as_deref() == Some("1"),
            artifact_bytes: &|addr: &str| self.artifact_bytes(addr).ok(),
            environment,
            compiled,
            model_snapshots: vec![],
            // §6.2's one-snapshot rule — the pinned registry snapshot on the
            // run's experiment binding (subject/experiment runs) or the
            // manifest's top-level pin (every other run kind — recorded at
            // `kernel.open` from the sealed definition's `resolved` member)
            // propagates onto every bundle.
            registry_snapshot_id: run_manifest
                .experiment
                .as_ref()
                .and_then(|b| b.registry_snapshot_id.clone())
                .or_else(|| run_manifest.registry_snapshot_id.clone()),
            variants: vec![],
            budget,
            profile: Json::str("none"),
            nondeterminism: vec![],
            participant_class: None,
        };
        let assembled = assemble(&inputs).map_err(bundle_err)?;

        // Deposit every present member into the blob pool (the bundle is
        // a view over the same content-addressed pool, never a second
        // store).
        for member in &assembled.manifest.members {
            if let Some(bytes) = assembled.members.get(&member.address) {
                self.store
                    .put_blob(bytes, &member.media_type)
                    .map_err(ledger_err)?;
            }
        }
        // The manifest itself is pool-addressable — `version_id` is the
        // coordinate `bundle show` / `run export` read back by.
        let manifest_bytes = assembled
            .manifest
            .to_json()
            .to_canonical_string()
            .into_bytes();
        self.store
            .put_blob(&manifest_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;

        // A named sink → the encoded directory form on disk + the
        // `measurement.export.delivered` delivery row (§5h.5's delivery
        // fact; the strict audit partition is `{sink_id, view_kind,
        // seq_range, content_classes, loss_report_ref}`).
        let mut delivered_seq = Json::Null;
        if let Some(sink) = &deliver_sink {
            encode_dir(Path::new(sink), &assembled.manifest, &assembled.members)
                .map_err(bundle_err)?;
            let head = self.store.head(&run_id).map_err(ledger_err)?;
            let payload = Json::obj([
                ("sink_id", Json::str(sink.clone())),
                ("view_kind", Json::str("bundle")),
                (
                    "seq_range",
                    Json::Arr(vec![Json::Int(0), Json::Int(head.seq as i64)]),
                ),
                (
                    "content_classes",
                    Json::Arr(vec![Json::str("structural"), Json::str("content")]),
                ),
                ("loss_report_ref", Json::Null),
            ]);
            let env = self
                .store
                .commit_kernel_row_for(
                    BUNDLE_COMPONENT,
                    &run_id,
                    "measurement.export.delivered",
                    payload,
                    vec![],
                    vec![],
                )
                .map_err(ledger_err)?;
            delivered_seq = Json::Int(env.seq as i64);
        }

        // `measurement.experiment.bundle_assembled` — the bundle
        // derivation is a kernel fact on the subject run.
        let manifest_json = assembled.manifest.to_json();
        let max_level = assembled
            .manifest
            .reproducibility
            .get("max_supported_level")
            .cloned()
            .unwrap_or(Json::Null);
        let assembled_payload = Json::obj([
            (
                "bundle_id",
                Json::str(assembled.manifest.version_id.clone()),
            ),
            (
                "version_id",
                Json::str(assembled.manifest.version_id.clone()),
            ),
            ("run_id", Json::str(run_id.clone())),
            ("kind", Json::str("run")),
            (
                "member_count",
                Json::Int(assembled.manifest.members.len() as i64),
            ),
            ("max_supported_level", max_level),
        ]);
        self.store
            .commit_kernel_row_for(
                BUNDLE_COMPONENT,
                &run_id,
                "measurement.experiment.bundle_assembled",
                assembled_payload,
                vec![],
                vec![],
            )
            .map_err(ledger_err)?;

        let out = Json::obj([
            ("schema", Json::str("hh-bundle-result/1")),
            ("manifest", manifest_json),
            ("report_rows", Json::Arr(assembled.report_rows.clone())),
            ("delivered_seq", delivered_seq),
            ("compile_error", compile_error),
        ]);
        Ok(out)
    }

    /// Re-run `compile` over the run's persisted sealed definition —
    /// the `compiled_bundle` member + the R1 derivation's recomputation
    /// input. Returns `None` only when the run carries no
    /// `harness_def_ref`; a compile *failure* is the typed error.
    fn compile_for_bundle(
        &self,
        run_manifest: &RunManifest,
    ) -> Result<Option<CompileOutcome>, EmbedError> {
        let Some(def_ref) = &run_manifest.harness_def_ref else {
            return Ok(None);
        };
        let bytes = self.artifact_bytes(def_ref)?;
        let kernel_prov = ProvenanceRecord::kernel(BUNDLE_COMPONENT, self.store.now_ms());
        let (sealed, _accept) = hh_compiler::accept_bytes(&bytes, &self.catalog, &kernel_prov)
            .map_err(|e| EmbedError::Refused {
                reason: format!("compile_accept: {e:?}"),
            })?;
        let profile_refs = pinned_profile_refs(&sealed.document);
        let bundle = hh_compiler::compile(
            &hh_compiler::CompileInputs {
                sealed,
                profile_refs,
                fallback_profile: None,
                targets: vec![],
                compile_for_expired: false,
            },
            &hh_compiler::profile::NoProfiles,
            &self.registry,
            &self.catalog,
            &kernel_prov,
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("compile: {e:?}"),
        })?;
        let doc = hh_compiler::schema::bundle_to_json(&bundle);
        Ok(Some(CompileOutcome {
            bundle_id: bundle.bundle_id.clone(),
            derivation_key: bundle.derivation_key.clone(),
            member_bytes: doc.to_canonical_string().into_bytes(),
            lcd_report: doc.get("lcd_report").cloned().unwrap_or(Json::Null),
            opacity_report: doc.get("opacity_report").cloned().unwrap_or(Json::Null),
        }))
    }

    // ── Group M — kernel.check_completeness ─────────────────────────

    /// `kernel.check_completeness{path | container}` — the staged
    /// `validate_bundle` S1/S2/S4/S7 gate over the decoded bundle
    /// (§5h.3 §2). A pure read: no session, no ledger write.
    pub(crate) fn kernel_check_completeness(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.check_completeness")?;
        let report = check_completeness(&decoded.manifest, &decoded.members);
        Ok(report.to_json())
    }

    // ── Group M — kernel.reproduce ──────────────────────────────────

    /// `kernel.reproduce{path | container, level, eval_budget?}` —
    /// `reproduce(bundle_ref, level)` (§5h.3 §2/§4; AC-R-2.9.3-3/4/6).
    /// The native Stage-3 driver re-derives what the bundle's own bytes
    /// decide offline:
    ///
    /// - **every level**: `manifest.version_id` recompute (the identity
    ///   leg of AC-4);
    /// - **R0**: the `ledger_export` pages' `lifecycle.run.*` rows fold
    ///   to the manifest's recorded `subject.status` (a mismatch is a
    ///   `lifecycle_status` drift row);
    /// - **R1**: + the sealed `definition` member re-`accept`s and
    ///   re-`compile`s; the recomputed `bundle_id` must equal the
    ///   `compiled_bundle` member's;
    /// - **R2**: + every exported envelope's hash recomputes and the
    ///   `prev_hash` chain links — the recorded execution re-derives
    ///   under the recorded seed (the live re-execution arm lands with
    ///   the experiment runtime; ADR-0275 R-REPRO-1);
    /// - **R3**: `eval_budget` must match `configuration.budget`
    ///   (`UnmatchedBudget` refusal — CC9); `model_fingerprint` drift is
    ///   computed between the manifest's declared snapshots and the
    ///   `model_snapshot` members'.
    pub(crate) fn kernel_reproduce(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.reproduce")?;
        let level_str = str_at(params, "kernel.reproduce", "level")?;
        let level = ReproLevel::parse(level_str).ok_or_else(|| EmbedError::SchemaViolation {
            path: "kernel.reproduce/level".to_string(),
            code: format!("unknown level `{level_str}`"),
        })?;
        let m = &decoded.manifest;

        // Decode the exported envelopes once — R0's fold, the R1 level
        // derivation's `replay_declared` input and R2's re-derivation
        // share them.
        let mut drift: Vec<Json> = Vec::new();
        let envelopes = self.exported_envelopes(&decoded, &mut drift);
        let replay_declared = envelopes.iter().any(|e| e.class == "control.decision");

        let (max, bases) = levels::derive(m, replay_declared);
        let basis_json: Vec<Json> = bases.iter().map(|b| b.to_json()).collect();
        let bundle_id = m.version_id.clone();
        let mk = |outcome: ReproOutcome,
                  refusal: Option<String>,
                  evidence: Json,
                  drift: Vec<Json>,
                  achieved: Option<ReproLevel>,
                  env_ok: bool,
                  model_ok: bool| {
            let mut r = ReproReport {
                bundle_id: bundle_id.clone(),
                requested_level: level.name().to_string(),
                achieved_level: achieved.map(|l| l.name().to_string()),
                outcome,
                refusal,
                evidence,
                drift,
                basis: basis_json.clone(),
                environment_ok: env_ok,
                model_ok,
                report_id: String::new(),
            };
            r.seal();
            Ok(r.to_json())
        };

        // Level gate — `ReproClaimUnsupported` (AC-3's "verify the
        // claimed level" obligation, evaluated against the *derived*
        // maximum, never the asserted one).
        if level > max {
            return mk(
                ReproOutcome::Refused,
                Some(format!(
                    "ReproClaimUnsupported{{requested {}, max_supported {}}}",
                    level.name(),
                    max.name()
                )),
                Json::obj([
                    ("oracle_diff", Json::Arr(vec![])),
                    ("fingerprints", Json::Arr(vec![])),
                    ("budget_match", Json::Null),
                ]),
                vec![],
                None,
                false,
                false,
            );
        }

        // R3's budget gate is a refusal, not a drift row (CC9).
        if level == ReproLevel::R3 {
            if let Some(req) = params.get("eval_budget") {
                let declared = m.configuration.get("budget").cloned().unwrap_or(Json::Null);
                if !budget_limits_equal(&declared, req) {
                    return mk(
                        ReproOutcome::Refused,
                        Some("UnmatchedBudget".to_string()),
                        Json::obj([
                            ("oracle_diff", Json::Arr(vec![])),
                            ("fingerprints", Json::Arr(vec![])),
                            ("budget_match", Json::Bool(false)),
                        ]),
                        vec![],
                        None,
                        false,
                        false,
                    );
                }
            }
        }

        let mut inconclusive = false;

        // Identity leg (every level): `version_id` recompute.
        if m.compute_id() != m.version_id {
            drift.push(Json::obj([
                ("kind", Json::str("manifest_version")),
                ("declared", Json::str(m.version_id.clone())),
                ("recomputed", Json::str(m.compute_id())),
            ]));
        }

        // R0 — lifecycle fold vs the recorded status.
        let derived_status = envelopes
            .iter()
            .filter(|e| e.class.starts_with("lifecycle.run."))
            .filter_map(|e| {
                e.payload
                    .get("status")
                    .and_then(Json::as_str)
                    .map(String::from)
                    .or_else(|| {
                        (e.class == "lifecycle.run.finished").then(|| "finished".to_string())
                    })
            })
            .next_back();
        if let Some(st) = &derived_status {
            if st != &m.subject.status {
                drift.push(Json::obj([
                    ("kind", Json::str("lifecycle_status")),
                    ("declared", Json::str(m.subject.status.clone())),
                    ("derived", Json::str(st.clone())),
                ]));
            }
        } else {
            inconclusive = true;
        }

        // R1 — recompile the sealed member; the recomputed
        // `CompiledBundle.bundle_id` must equal the member's.
        if level >= ReproLevel::R1 {
            let sealed_member = m
                .members
                .iter()
                .find(|mm| mm.role == levels::roles::DEFINITION)
                .and_then(|mm| decoded.members.get(&mm.address));
            let compiled_member = m
                .members
                .iter()
                .find(|mm| mm.role == levels::roles::COMPILED_BUNDLE)
                .and_then(|mm| decoded.members.get(&mm.address));
            match (sealed_member, compiled_member) {
                (Some(sealed_bytes), Some(compiled_bytes)) => {
                    let kernel_prov =
                        ProvenanceRecord::kernel(BUNDLE_COMPONENT, self.store.now_ms());
                    let repro =
                        hh_compiler::accept_bytes(sealed_bytes, &self.catalog, &kernel_prov)
                            .and_then(|(sealed, _)| {
                                let profile_refs = pinned_profile_refs(&sealed.document);
                                hh_compiler::compile(
                                    &hh_compiler::CompileInputs {
                                        sealed,
                                        profile_refs,
                                        fallback_profile: None,
                                        targets: vec![],
                                        compile_for_expired: false,
                                    },
                                    &hh_compiler::profile::NoProfiles,
                                    &self.registry,
                                    &self.catalog,
                                    &kernel_prov,
                                )
                            });
                    match repro {
                        Ok(recompiled) => {
                            let declared_id = std::str::from_utf8(compiled_bytes)
                                .ok()
                                .and_then(|t| hh_wire::json::parse(t).ok())
                                .and_then(|j| {
                                    j.get("bundle_id").and_then(Json::as_str).map(String::from)
                                })
                                .unwrap_or_default();
                            if recompiled.bundle_id != declared_id {
                                drift.push(Json::obj([
                                    ("kind", Json::str("compiled_bundle")),
                                    ("declared", Json::str(declared_id)),
                                    ("recomputed", Json::str(recompiled.bundle_id.clone())),
                                ]));
                            }
                        }
                        Err(e) => {
                            drift.push(Json::obj([
                                ("kind", Json::str("compile")),
                                ("detail", Json::str(format!("{e:?}"))),
                            ]));
                        }
                    }
                }
                _ => inconclusive = true,
            }
        }

        // R2 — re-derive the delivered rows: every envelope hash
        // recomputes and the chain links.
        if level >= ReproLevel::R2 {
            let mut prev = String::new();
            for env in &envelopes {
                if env.recompute_hash() != env.hash {
                    drift.push(Json::obj([
                        ("kind", Json::str("event_hash")),
                        ("event_id", Json::str(env.event_id.clone())),
                        ("seq", Json::Int(env.seq as i64)),
                    ]));
                }
                if !prev.is_empty() && env.prev_hash != prev {
                    drift.push(Json::obj([
                        ("kind", Json::str("hash_chain")),
                        ("event_id", Json::str(env.event_id.clone())),
                        ("seq", Json::Int(env.seq as i64)),
                    ]));
                }
                prev = env.hash.clone();
            }
        }

        // R3 — fingerprint drift between the manifest's declared
        // snapshots and the `model_snapshot` members' observed ones.
        let mut fingerprints: Vec<Json> = Vec::new();
        let mut model_ok = true;
        if level == ReproLevel::R3 {
            let mut observed: Vec<(String, String)> = Vec::new();
            for mm in &m.members {
                if mm.role == "model_snapshot" {
                    if let Some(bytes) = decoded.members.get(&mm.address) {
                        if let Some(j) = std::str::from_utf8(bytes)
                            .ok()
                            .and_then(|t| hh_wire::json::parse(t).ok())
                        {
                            if let Some(fp) = snapshot_fingerprint(&j) {
                                let model_ref = j
                                    .get("model_id")
                                    .and_then(Json::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                observed.push((model_ref.clone(), fp.clone()));
                                fingerprints.push(Json::obj([
                                    ("model_ref", Json::str(model_ref)),
                                    ("fingerprint", Json::str(fp)),
                                ]));
                            }
                        }
                    }
                }
            }
            let rows = fingerprint_drift(m, &observed);
            if !rows.is_empty() {
                model_ok = false;
                drift.extend(rows);
            }
        }

        let env_ok = m
            .members
            .iter()
            .any(|mm| mm.role == levels::roles::ENVIRONMENT);
        let outcome = if !drift.is_empty() {
            ReproOutcome::Drift
        } else if inconclusive {
            ReproOutcome::Inconclusive
        } else {
            ReproOutcome::Pass
        };
        mk(
            outcome,
            None,
            Json::obj([
                ("oracle_diff", Json::Arr(vec![])),
                ("fingerprints", Json::Arr(fingerprints)),
                (
                    "budget_match",
                    if level == ReproLevel::R3 {
                        Json::Bool(model_ok)
                    } else {
                        Json::Null
                    },
                ),
            ]),
            drift,
            Some(level),
            env_ok,
            model_ok,
        )
    }

    /// Parse the bundle's ledger pages into envelopes — one drift row
    /// per malformed page (never a panic; a page the manifest names but
    /// the tree does not carry is a missing-member drift row).
    fn exported_envelopes(&self, decoded: &Decoded, drift: &mut Vec<Json>) -> Vec<EventEnvelope> {
        let mut out = Vec::new();
        for (run, export) in &decoded.manifest.traces {
            for page_addr in &export.pages {
                match decoded.members.get(page_addr) {
                    Some(bytes) => match decode_page(bytes) {
                        Ok(evs) => out.extend(evs),
                        Err(e) => drift.push(Json::obj([
                            ("kind", Json::str("ledger_page")),
                            ("run_id", Json::str(run.clone())),
                            ("detail", Json::str(format!("{e}"))),
                        ])),
                    },
                    None => drift.push(Json::obj([
                        ("kind", Json::str("ledger_page_missing")),
                        ("run_id", Json::str(run.clone())),
                        ("address", Json::str(page_addr.clone())),
                    ])),
                }
            }
        }
        out.sort_by_key(|e| e.seq);
        out
    }

    // ── Group M — kernel.import ─────────────────────────────────────

    /// `kernel.import{path | container, holder?}` — lift a decoded
    /// `hh-bundle/1` bundle into a new run: member bytes join the blob
    /// pool, the lifted manifest opens the run (its
    /// `lifecycle.run.created` carries the `import` lineage record), the
    /// `lifecycle.run.imported` receipt row names the source coordinates
    /// (refs only — no foreign fact is re-asserted; ADR-0141 D1), and
    /// the `ImportRecord` + `MappingReport` ride into the pool.
    ///
    /// Session-free like the other `kernel.*` derivations
    /// (R-BUNDLE-2): the imported run is a *new* run the kernel opens
    /// and writes itself (`commit_kernel_row_for`); `holder` names the
    /// new run's lease holder for any later `open_session(resume)`.
    pub(crate) fn kernel_import(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let holder = params
            .get("holder")
            .and_then(Json::as_str)
            .unwrap_or("hh-cli")
            .to_string();
        let decoded = decode_bundle_arg(params, "kernel.import")?;
        // Only a complete bundle lifts — the staged gate's own verdict.
        let report = check_completeness(&decoded.manifest, &decoded.members);
        if !report.complete {
            return Err(EmbedError::Refused {
                reason: "bundle_incomplete".to_string(),
            });
        }
        let lift = lift(&decoded, NATIVE_FORMAT, self.store.now_ms()).map_err(bundle_err)?;

        // Blob pool deposit — member bytes land under their recorded
        // media types (the address recomputes identically).
        for (addr, bytes) in &lift.blobs {
            let media = decoded
                .manifest
                .members
                .iter()
                .find(|mm| mm.address == *addr)
                .map(|mm| mm.media_type.clone())
                .unwrap_or_else(|| MEDIA_JSON.to_string());
            self.store.put_blob(bytes, &media).map_err(ledger_err)?;
        }
        // The ImportRecord + MappingReport are records too — canonical
        // bytes into the pool so the receipt row's refs resolve.
        let record_bytes = lift.import_record.to_canonical_string().into_bytes();
        let record_addr = self
            .store
            .put_blob(&record_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;
        let mapping_bytes = lift.mapping_report.to_canonical_string().into_bytes();
        let mapping_addr = self
            .store
            .put_blob(&mapping_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;

        // Open the lifted run — `open_run` mints `lifecycle.run.created`
        // from the lifted manifest (which carries the `import` extra).
        let (new_run_id, _lease) = self
            .store
            .open_run(lift.run_manifest.clone(), &holder)
            .map_err(ledger_err)?;

        // The receipt rows — kernel minted (the kernel performed the
        // lift; the *coordinates* are the record). Lifted events carry
        // only refs/coordinates by `lift`'s construction.
        for ev in &lift.events {
            let refs = ev
                .refs
                .iter()
                .filter_map(|r| {
                    hh_identity::idp::parse_id(r)
                        .ok()
                        .map(|p| hh_identity::idp::ContentAddress {
                            idp: "idp/1",
                            algorithm: "sha256",
                            digest: p.digest_hex,
                            media_type: String::new(),
                            size: 0,
                        })
                })
                .collect::<Vec<_>>();
            let mut refs_all = refs;
            refs_all.push(record_addr.clone());
            refs_all.push(mapping_addr.clone());
            self.store
                .commit_kernel_row_for(
                    IMPORT_COMPONENT,
                    &new_run_id,
                    &ev.class,
                    ev.payload.clone(),
                    refs_all,
                    vec![],
                )
                .map_err(ledger_err)?;
        }

        Ok(Json::obj([
            ("schema", Json::str("hh-import-result/1")),
            ("run_id", Json::str(new_run_id)),
            ("mapping_report", lift.mapping_report.clone()),
        ]))
    }

    // ── Group L — lab.serve ─────────────────────────────────────────

    /// `lab.serve{path | container}` — the `serve(bundle)` boundary
    /// half (R-2.11.3⁰; ADR-0097 D7): decode the bundle, lower its
    /// `target:mcp` member into the `hh-mcp-artifact/1` record and
    /// return `{artifact, binding, launch}` — the `stdio_launch`
    /// CallerBinding (fixed to the test principal, R-3) plus the launch
    /// descriptor the caller uses to spawn `hh-mcp-serve` (the stdio
    /// pair is the caller's; the kernel never holds it).
    pub(crate) fn lab_serve(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "lab.serve")?;
        let member = decoded
            .manifest
            .members
            .iter()
            .find(|mm| mm.role == "target:mcp")
            .ok_or_else(|| EmbedError::Refused {
                reason: "no_target_mcp_member".to_string(),
            })?;
        let bytes = decoded
            .members
            .get(&member.address)
            .ok_or_else(|| EmbedError::Refused {
                reason: "target_mcp_member_absent".to_string(),
            })?;
        let artifact = hh_mcp::artifact::lower_mcp_target(
            bytes,
            &decoded.manifest.version_id,
            &decoded.manifest.version_id,
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("mcp_target: {e:?}"),
        })?;
        let artifact_json = artifact.to_json();
        let mut launch_args: Vec<String> = Vec::new();
        if let Some(p) = params.get("path").and_then(Json::as_str) {
            launch_args.push("--bundle".to_string());
            launch_args.push(p.to_string());
        } else if let Some(c) = params.get("container").and_then(Json::as_str) {
            launch_args.push("--container".to_string());
            launch_args.push(c.to_string());
        }
        Ok(Json::obj([
            ("schema", Json::str("hh-lab-serve/1")),
            ("artifact", artifact_json),
            ("binding", hh_mcp::stdio_launch_binding()),
            (
                "launch",
                Json::obj([
                    ("program", Json::str("hh-mcp-serve")),
                    (
                        "args",
                        Json::Arr(launch_args.iter().map(|a| Json::str(a.clone())).collect()),
                    ),
                    ("transport", Json::str("stdio")),
                ]),
            ),
        ]))
    }
}
