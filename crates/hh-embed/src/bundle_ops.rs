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

use hh_bundle::assemble::{assemble, AssembleInputs, CompileOutcome, ExtensionInput};
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

/// A displayable error → `Refused` (the results-store/status surfaces
/// render as their typed detail).
fn refused(e: impl std::fmt::Display) -> EmbedError {
    EmbedError::Refused {
        reason: format!("{e}"),
    }
}

/// `Json::Arr` member access (no `as_arr` on `Json`).
fn arr_at<'a>(j: &'a Json, k: &str) -> Option<&'a Vec<Json>> {
    match j.get(k) {
        Some(Json::Arr(a)) => Some(a),
        _ => None,
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

    /// The definition's bound extensions as `resolved_dependencies
    /// .extensions[]` members — the `trust_snapshot` projection over the
    /// sealed definition's `assembly.extensions.refs[]` against the run's
    /// pinned registry snapshot (§5g.5 §3; ADR-0064 D7; S3.11b).
    /// `[]` only when the definition binds no extensions — a bound-but-
    /// unresolvable extension refuses, never silently drops (CC3).
    /// Each entry carries its payload bytes when the blob pool holds them
    /// (the assembler verifies the `content` digest against the member);
    /// absent bytes land `unpinned{not_captured}`, never a silent ref.
    fn extension_entries(&self, rm: &RunManifest) -> Result<Vec<ExtensionInput>, EmbedError> {
        let Some(def_ref) = rm.harness_def_ref.as_deref() else {
            return Ok(vec![]);
        };
        let def_doc = match self.artifact_bytes(def_ref) {
            Ok(bytes) => hh_wire::json::parse(std::str::from_utf8(&bytes).map_err(|_| {
                EmbedError::Refused {
                    reason: "extension_entries: sealed definition is not utf-8".into(),
                }
            })?)
            .map_err(|e| EmbedError::Refused {
                reason: format!("extension_entries: sealed definition parse: {e}"),
            })?,
            // The definition member is itself optional in the assembly path
            // (`MemberUnavailable` is the assembler's own refusal) — a read
            // miss here just means no definition bytes to project from.
            Err(_) => return Ok(vec![]),
        };
        let has_refs = def_doc
            .get("assembly")
            .and_then(|a| a.get("extensions"))
            .and_then(|e| e.get("refs"))
            .map(|r| matches!(r, Json::Arr(v) if !v.is_empty()))
            .unwrap_or(false);
        if !has_refs {
            return Ok(vec![]);
        }
        let Some(snap_id) = rm
            .experiment
            .as_ref()
            .and_then(|b| b.registry_snapshot_id.clone())
            .or_else(|| rm.registry_snapshot_id.clone())
        else {
            return Err(EmbedError::Refused {
                reason:
                    "extension_entries: definition binds extensions but the run pins no registry_snapshot_id"
                        .into(),
            });
        };
        let Some((_, hh_registry::records::RegistryRecord::Snapshot(snap))) =
            self.registry.get(&snap_id)
        else {
            return Err(EmbedError::Refused {
                reason: format!("extension_entries: registry snapshot {snap_id} is not held"),
            });
        };
        let ts = hh_registry::extension::trust_snapshot(&self.registry, &def_doc, snap).map_err(
            |e| EmbedError::Refused {
                reason: format!("extension_entries: trust_snapshot: {}", e.reason()),
            },
        )?;
        Ok(ts
            .extensions
            .iter()
            .map(|e| ExtensionInput {
                entry: e.to_json(),
                bytes: self.artifact_bytes(&e.content).ok(),
            })
            .collect())
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
    /// `kind ∈ {run, arm, experiment, lineage}` — the scoped kinds
    /// assemble over `contains[]` child bundles (S4.2; AC-R-2.9.3-10).
    /// Corpus and profile bundles remain `Refused{kind_pending}`
    /// (ADR-0275 R-BUNDLE-1).
    pub(crate) fn kernel_bundle(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let kind = params
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("run")
            .to_string();
        if kind != "run" {
            return self.kernel_bundle_scoped(params, &kind);
        }
        let run_id = str_at(params, "kernel.bundle", "run_id")?.to_string();
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
            // §5g.5 §3 `resolved_dependencies.extensions[]` — the
            // `trust_snapshot` projection over the sealed definition's
            // `assembly.extensions.refs[]` against the run's pinned registry
            // snapshot (S3.11b). A bound-but-unresolvable extension refuses,
            // never silently drops (CC3).
            extensions: self.extension_entries(&run_manifest)?,
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

    /// `kernel.bundle{kind ∈ {arm, experiment, lineage}}` — the scoped
    /// assembly (S4.2; §5h.3 §3's `experiment ⊃ arm ⊃ run` DAG).
    /// `contains[]` children arrive as `{path: dir}` or
    /// `{container: file}` decodes; each child's canonical manifest
    /// bytes deposit verbatim as a `contains:<bundle_id>` member (the
    /// child is never rewritten — I1). `subject_runs` defaults to the
    /// union of the children's `subject.run_ids` (an `arm`/`experiment`
    /// bundle covers ≥ 1 run; `ScopeInvalid` refuses otherwise).
    fn kernel_bundle_scoped(&mut self, params: &Json, kind: &str) -> Result<Json, EmbedError> {
        if !matches!(
            kind,
            hh_bundle::scoped::KIND_ARM
                | hh_bundle::scoped::KIND_EXPERIMENT
                | hh_bundle::scoped::KIND_LINEAGE
        ) {
            return Err(EmbedError::Refused {
                reason: format!("kind_pending:{kind}"),
            });
        }
        let policy = BundlePolicy::from_json(params.get("policy")).map_err(bundle_err)?;

        // Decode the contained bundles — each `contains` entry is the
        // child's own `{path | container}` decode.
        let mut children = Vec::new();
        if let Some(Json::Arr(cs)) = params.get("contains") {
            for c in cs {
                let decoded = decode_bundle_arg(c, "kernel.bundle/contains")?;
                children.push(hh_bundle::scoped::ChildBundle {
                    bundle_id: decoded.manifest.version_id.clone(),
                    kind: decoded.manifest.bundle_kind.clone(),
                    manifest_bytes: decoded
                        .manifest
                        .to_json()
                        .to_canonical_string()
                        .into_bytes(),
                });
            }
        }

        let mut subject_runs: Vec<String> = arr_at(params, "subject_runs")
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let row_payloads: Vec<(String, Vec<u8>)> = Vec::new();
        if subject_runs.is_empty() {
            // Derive the coverage from the children's subject sections —
            // the scope covers exactly what its members cover.
            if let Some(Json::Arr(cs)) = params.get("contains") {
                for c in cs {
                    let decoded = decode_bundle_arg(c, "kernel.bundle/contains")?;
                    for r in &decoded.manifest.subject.run_ids {
                        if !subject_runs.contains(r) {
                            subject_runs.push(r.clone());
                        }
                    }
                }
            }
        }

        let created_at = hh_ledger::store::rfc3339_ms(self.store.now_ms());
        let producer = ProvenanceRecord::kernel(BUNDLE_COMPONENT, self.store.now_ms()).to_json();
        let inputs = hh_bundle::scoped::ScopedInputs {
            store: &self.store,
            policy: &policy,
            created_at,
            producer,
            contract_identity: self.contract_identity(),
            kernel_version_id: self.kernel_version.clone(),
            instrument_dirty: std::env::var("HH_KERNEL_DIRTY").ok().as_deref() == Some("1"),
            kind: kind.to_string(),
            experiment_run_id: params
                .get("experiment_run_id")
                .and_then(Json::as_str)
                .map(String::from),
            arm: params.get("arm").cloned(),
            design: params.get("design").cloned(),
            pre_registration: params.get("pre_registration").cloned(),
            pre_registration_seq: params
                .get("pre_registration_seq")
                .and_then(Json::as_int)
                .map(|v| v as u64),
            first_plan_seq: params
                .get("first_plan_seq")
                .and_then(Json::as_int)
                .map(|v| v as u64),
            match_spec: params.get("match_spec").cloned(),
            search_budget: params.get("search_budget").cloned(),
            arms: arr_at(params, "arms").cloned().unwrap_or_default(),
            subject_runs: subject_runs.clone(),
            children,
            row_refs: arr_at(params, "row_refs")
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            row_payloads,
            participant_class: params
                .get("participant_class")
                .and_then(Json::as_str)
                .map(String::from),
        };
        let assembled = hh_bundle::scoped::assemble_scoped(&inputs).map_err(bundle_err)?;

        // Deposit every present member + the manifest — same pool rule
        // as the `run` path.
        for member in &assembled.manifest.members {
            if let Some(bytes) = assembled.members.get(&member.address) {
                self.store
                    .put_blob(bytes, &member.media_type)
                    .map_err(ledger_err)?;
            }
        }
        let manifest_bytes = assembled
            .manifest
            .to_json()
            .to_canonical_string()
            .into_bytes();
        self.store
            .put_blob(&manifest_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;

        // The derivation row lands on the experiment run when named,
        // else the first covered subject run — a lineage bundle over no
        // run still assembles (its record is the bundle itself).
        let record_run = params
            .get("experiment_run_id")
            .and_then(Json::as_str)
            .map(String::from)
            .or_else(|| subject_runs.first().cloned());
        if let Some(run) = &record_run {
            let payload = Json::obj([
                (
                    "bundle_id",
                    Json::str(assembled.manifest.version_id.clone()),
                ),
                (
                    "version_id",
                    Json::str(assembled.manifest.version_id.clone()),
                ),
                ("run_id", Json::str(run.clone())),
                ("kind", Json::str(kind)),
                (
                    "member_count",
                    Json::Int(assembled.manifest.members.len() as i64),
                ),
            ]);
            self.store
                .commit_kernel_row_for(
                    BUNDLE_COMPONENT,
                    run,
                    "measurement.experiment.bundle_assembled",
                    payload,
                    vec![],
                    vec![],
                )
                .map_err(ledger_err)?;
        }
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-result/1")),
            ("manifest", assembled.manifest.to_json()),
            ("report_rows", Json::Arr(assembled.report_rows.clone())),
        ]))
    }

    /// `kernel.validate{path | container, publication?}` — the staged
    /// `validate_bundle` gate over a decoded bundle. `publication =
    /// true` runs S1..S9 (the publication gate, with the scoped-kind
    /// `cross_section` checks); otherwise S1..S8 (the assembly gate).
    /// The report is the verdict — `status: invalid` returns the rows,
    /// never a refusal (refusals are for unreadable input).
    pub(crate) fn kernel_validate(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.validate")?;
        let publication = matches!(params.get("publication"), Some(Json::Bool(true)));
        let report = if publication {
            hh_bundle::validate::validate_publication(&decoded.manifest, &decoded.members)
        } else {
            hh_bundle::validate::validate(&decoded.manifest, &decoded.members)
        };
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-validation-result/1")),
            ("publication", Json::Bool(publication)),
            ("report", report.to_json()),
        ]))
    }

    /// `kernel.diff{left: {path | container}, right: {path | container}}`
    /// — the bundle-diff surface (§5h.3's `diff`: member deltas,
    /// configuration/level differences, `same_fact`/`same_result`, and
    /// the varied-factor report over the two manifests' factor sets).
    pub(crate) fn kernel_diff(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let left = params
            .get("left")
            .ok_or_else(|| missing("kernel.diff/left"))?;
        let right = params
            .get("right")
            .ok_or_else(|| missing("kernel.diff/right"))?;
        let a = decode_bundle_arg(left, "kernel.diff/left")?;
        let b = decode_bundle_arg(right, "kernel.diff/right")?;
        let diff = hh_bundle::diff::bundle_diff(&a, &b);
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-diff/1")),
            ("diff", diff.to_json()),
        ]))
    }

    /// `kernel.fetch{path | container, addresses[]}` — materialize the
    /// manifest's `status = fetch` members. The kernel's fetcher
    /// resolves bytes from the content-addressed blob pool (the same
    /// pool member deposit writes); a member whose bytes the pool does
    /// not hold reports `fetch_error:no_transport` — the kernel never
    /// fabricates payload bytes and never trusts a location without the
    /// `idp/1` digest re-check `fetch` performs. Materialized bytes
    /// deposit into the pool and the bundle's `fetch[]` intent is
    /// unchanged (the manifest is immutable — the reader's *view* gains
    /// bytes).
    pub(crate) fn kernel_fetch(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.fetch")?;
        let addresses: Vec<String> = arr_at(params, "addresses")
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                decoded
                    .manifest
                    .members
                    .iter()
                    .filter(|m| m.status == hh_bundle::manifest::MemberStatus::Fetch)
                    .map(|m| m.address.clone())
                    .collect()
            });
        let outcome = hh_bundle::fetch::fetch(
            &decoded,
            &addresses,
            Some(&|entry: &hh_bundle::manifest::FetchEntry| {
                // Pool-backed fetcher — `entry.address` is the
                // content address the bytes must recompute to.
                let parsed = hh_identity::idp::parse_id(&entry.address)
                    .map_err(|e| format!("bad_address:{e:?}"))?;
                self.store
                    .get_blob(&hh_identity::idp::ContentAddress {
                        idp: "idp/1",
                        algorithm: "sha256",
                        digest: parsed.digest_hex,
                        media_type: String::new(),
                        size: 0,
                    })
                    .map_err(|_| "no_transport:pool_miss".to_string())
            }),
        );
        // Deposit the verified materializations.
        for addr in &outcome.materialized {
            if let Some(bytes) = outcome.bytes.get(addr) {
                let media = decoded
                    .manifest
                    .members
                    .iter()
                    .find(|m| m.address == *addr)
                    .map(|m| m.media_type.clone())
                    .unwrap_or_else(|| MEDIA_JSON.to_string());
                self.store.put_blob(bytes, &media).map_err(ledger_err)?;
            }
        }
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-fetch/1")),
            ("outcome", outcome.to_json()),
        ]))
    }

    /// `kernel.export{path | container, target, policy?, dir}` — the
    /// export-target lowering (§5h.3 §6; AC-R-2.9.3-7). `target ∈
    /// {ledger_native, harbor_job_dir, interchange_trajectory}`; the
    /// `PublicationPolicy` gates member classes (withheld members land
    /// `no_slot` loss rows — never silently absent). `dir` receives the
    /// artefact's file tree; `measurement.export.delivered` rows land
    /// on the export's subject runs.
    pub(crate) fn kernel_export(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.export")?;
        let target = str_at(params, "kernel.export", "target")?;
        let policy = hh_bundle::export::PublicationPolicy::from_json(params.get("policy"))
            .map_err(bundle_err)?;
        let out =
            hh_bundle::export::export_target(&decoded, target, &policy).map_err(bundle_err)?;
        if let Some(dir) = params.get("dir").and_then(Json::as_str) {
            let root = Path::new(dir);
            std::fs::create_dir_all(root).map_err(|e| EmbedError::Refused {
                reason: format!("export_dir: {e}"),
            })?;
            for (rel, bytes) in &out.files {
                let path = root.join(rel);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| EmbedError::Refused {
                        reason: format!("export_dir: {e}"),
                    })?;
                }
                std::fs::write(&path, bytes).map_err(|e| EmbedError::Refused {
                    reason: format!("export_write: {e}"),
                })?;
            }
        }
        for run in &out.delivered_runs {
            let _ = self.store.commit_kernel_row_for(
                BUNDLE_COMPONENT,
                run,
                "measurement.export.delivered",
                Json::obj([
                    ("sink_id", Json::str(format!("target:{target}"))),
                    ("view_kind", Json::str("bundle_export")),
                    ("content_classes", Json::Arr(vec![Json::str("structural")])),
                    ("loss_report_ref", Json::Null),
                ]),
                vec![],
                vec![],
            );
        }
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-export/1")),
            ("artefact", Json::str(out.artefact)),
            (
                "files",
                Json::Arr(out.files.keys().map(|k| Json::str(k.clone())).collect()),
            ),
            ("loss_report", out.loss_report),
            ("granularity_ceiling", Json::str(out.granularity_ceiling)),
        ]))
    }

    /// `kernel.status{bundle_id}` — the status book read (rebuilt from
    /// the ledger's `bundle_status_changed` rows — the persisted book
    /// is a cache of this fold).
    pub(crate) fn kernel_status(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let bundle_id = str_at(params, "kernel.status", "bundle_id")?;
        let book = hh_results::status::fold(&self.store);
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-status/1")),
            ("bundle_id", Json::str(bundle_id)),
            ("status", Json::str(book.status(bundle_id).as_str())),
            (
                "history",
                Json::Arr(
                    book.history(bundle_id)
                        .iter()
                        .map(|r| r.to_json())
                        .collect(),
                ),
            ),
        ]))
    }

    /// `kernel.set_status{run_id, bundle_id, to, reason?, readers?,
    /// authority?, evidence?}` — the gated transition (§5h.3 §2's
    /// monotone vocabulary + evidence gates). `run_id` is the subject
    /// run the `bundle_status_changed` row lands on.
    pub(crate) fn kernel_set_status(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let run_id = str_at(params, "kernel.set_status", "run_id")?.to_string();
        let bundle_id = str_at(params, "kernel.set_status", "bundle_id")?.to_string();
        let to_s = str_at(params, "kernel.set_status", "to")?;
        let to =
            hh_results::BundleStatus::parse(to_s).ok_or_else(|| EmbedError::SchemaViolation {
                path: "kernel.set_status/to".to_string(),
                code: format!("unknown_status:{to_s}"),
            })?;
        let reason = params
            .get("reason")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let readers: Vec<String> = arr_at(params, "readers")
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let authority = params.get("authority").cloned().unwrap_or(Json::Null);
        let evidence = params.get("evidence").cloned().unwrap_or(Json::Null);
        let results = hh_results::store::ResultsStore::open(self.store.root().join("results"))
            .map_err(refused)?;
        let record = hh_results::status::set_status(
            &mut self.store,
            &results,
            &run_id,
            &bundle_id,
            to,
            &reason,
            readers,
            authority,
            evidence,
        )
        .map_err(refused)?;
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-status/1")),
            ("record", record.to_json()),
        ]))
    }

    /// `kernel.attest{path | container, attestation}` — verify a
    /// `BundleAttestation` against the decoded bundle. Signed
    /// attestations verify under the store's audit key resolver; an
    /// unsigned attestation is admitted as a *statement* (the gate is
    /// `set_status`'s, which requires the statement to exist).
    pub(crate) fn kernel_attest(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.attest")?;
        let att_j = params
            .get("attestation")
            .ok_or_else(|| missing("kernel.attest/attestation"))?;
        let attestation =
            hh_bundle::attestation::BundleAttestation::from_json(att_j).map_err(bundle_err)?;
        let empty = hh_ledger::audit::KeyTable::default();
        let resolver: &dyn hh_ledger::audit::AuditKeyResolver =
            self.store.audit_key_resolver().unwrap_or(&empty);
        if attestation.sig.is_some() && self.store.audit_key_resolver().is_none() {
            return Err(EmbedError::Refused {
                reason: "attest_no_key_resolver".to_string(),
            });
        }
        let doc =
            hh_bundle::attestation::attest(&decoded, &attestation, resolver).map_err(bundle_err)?;
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-attestation/1")),
            ("attestation", doc),
        ]))
    }

    /// `kernel.audit_bundle{path | container, known_bundles?}` — the
    /// member/link hygiene pass (`audit_bundle` — findings, never a
    /// mutated manifest).
    pub(crate) fn kernel_audit_bundle(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.audit_bundle")?;
        let known: std::collections::BTreeSet<String> = arr_at(params, "known_bundles")
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let report = hh_bundle::audit::audit_bundle(&decoded, &known);
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-audit/1")),
            ("bundle_id", Json::str(report.bundle_id)),
            ("ok", Json::Bool(report.ok)),
            ("findings", Json::Arr(report.findings)),
        ]))
    }

    /// `kernel.supersede{new: {path | container}, old: {path |
    /// container}, reason}` — stamp the `derived_from` edge on the
    /// successor manifest and recompute its `version_id` (§5h.3 §2;
    /// supersession by new `version_id` — `old` is never touched). The
    /// result carries the re-stamped manifest; the caller deposits
    /// status via `kernel.set_status{to: superseded, evidence.successor}`.
    pub(crate) fn kernel_supersede(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let new_p = params
            .get("new")
            .ok_or_else(|| missing("kernel.supersede/new"))?;
        let old_p = params
            .get("old")
            .ok_or_else(|| missing("kernel.supersede/old"))?;
        let reason = str_at(params, "kernel.supersede", "reason")?;
        let mut new = decode_bundle_arg(new_p, "kernel.supersede/new")?;
        let old = decode_bundle_arg(old_p, "kernel.supersede/old")?;
        hh_bundle::lifecycle::supersede_bundle(&mut new.manifest, &old.manifest, reason)
            .map_err(bundle_err)?;
        let manifest_bytes = new.manifest.to_json().to_canonical_string().into_bytes();
        self.store
            .put_blob(&manifest_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-result/1")),
            ("manifest", new.manifest.to_json()),
        ]))
    }

    /// `kernel.migrate{path | container, to_schema}` — the
    /// migration-as-supersession path (AC-R-2.9.3-13). Only
    /// `hh-bundle/1` exists — other spellings are `FormatUnknown`.
    pub(crate) fn kernel_migrate(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let decoded = decode_bundle_arg(params, "kernel.migrate")?;
        let to_schema = str_at(params, "kernel.migrate", "to_schema")?;
        let producer = ProvenanceRecord::kernel(BUNDLE_COMPONENT, self.store.now_ms()).to_json();
        let migrated = hh_bundle::lifecycle::migrate_bundle(
            &decoded,
            to_schema,
            producer,
            hh_ledger::store::rfc3339_ms(self.store.now_ms()),
        )
        .map_err(bundle_err)?;
        let manifest_bytes = migrated
            .manifest
            .to_json()
            .to_canonical_string()
            .into_bytes();
        self.store
            .put_blob(&manifest_bytes, MEDIA_JSON)
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-result/1")),
            ("manifest", migrated.manifest.to_json()),
        ]))
    }

    /// `kernel.lineage{bundles: [{path | container}...]}` — the
    /// supersession-chain edges over the supplied manifests
    /// (`{bundle_id, derived_from, reason}`), deterministic order.
    pub(crate) fn kernel_lineage(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let mut manifests = Vec::new();
        if let Some(Json::Arr(bs)) = params.get("bundles") {
            for b in bs {
                manifests.push(decode_bundle_arg(b, "kernel.lineage")?.manifest);
            }
        }
        let edges = hh_bundle::lifecycle::lineage_edges(&manifests);
        Ok(Json::obj([
            ("schema", Json::str("hh-bundle-lineage/1")),
            ("edges", Json::Arr(edges)),
        ]))
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
        // The validator set the plan binds — `resolved_dependencies
        // .validators[]` names it so an R2 verdict reports the set it
        // re-derived under (AC-R-2.12.1-13; S3.12).
        let validators = doc
            .get("runtime_plan")
            .and_then(|p| p.get("validators"))
            .and_then(|v| match v {
                Json::Arr(vs) => Some(vs.clone()),
                _ => None,
            })
            .unwrap_or_default()
            .iter()
            .filter_map(|b| b.get("validator").cloned())
            .collect();
        Ok(Some(CompileOutcome {
            bundle_id: bundle.bundle_id.clone(),
            derivation_key: bundle.derivation_key.clone(),
            member_bytes: doc.to_canonical_string().into_bytes(),
            lcd_report: doc.get("lcd_report").cloned().unwrap_or(Json::Null),
            opacity_report: doc.get("opacity_report").cloned().unwrap_or(Json::Null),
            validators,
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
            // AC-R-2.12.1-13 — `reproduce(bundle, R3, seeds[])`: every
            // seed arm carries `search_budget`/`eval_budget` equal to the
            // bundle's declared budgets, else `UnbudgetedArm{seed}`
            // (matched-budget-or-refuse — an arm the manifest can't
            // budget-match never enters the distribution).
            let declared_eval = m.configuration.get("budget").cloned().unwrap_or(Json::Null);
            let declared_search = m
                .results
                .get("search_budget")
                .or_else(|| m.configuration.get("search_budget"))
                .cloned()
                .unwrap_or(Json::Null);
            if let Some(Json::Arr(seeds)) = params.get("seeds") {
                for arm in seeds {
                    let seed_label = arm
                        .get("seed")
                        .and_then(Json::as_int)
                        .map(|s| s.to_string())
                        .or_else(|| arm.as_int().map(|s| s.to_string()))
                        .unwrap_or_else(|| "?".to_string());
                    let arm_eval = arm.get("eval_budget").cloned().unwrap_or(Json::Null);
                    let arm_search = arm.get("search_budget").cloned().unwrap_or(Json::Null);
                    let eval_ok = !matches!(arm_eval, Json::Null)
                        && budget_limits_equal(&declared_eval, &arm_eval);
                    let search_ok = match (&declared_search, &arm_search) {
                        (Json::Null, Json::Null) => true,
                        (Json::Null, _) | (_, Json::Null) => false,
                        _ => budget_limits_equal(&declared_search, &arm_search),
                    };
                    if !eval_ok || !search_ok {
                        return mk(
                            ReproOutcome::Refused,
                            Some(format!("UnbudgetedArm{{seed {seed_label}}}")),
                            Json::obj([
                                ("oracle_diff", Json::Arr(vec![])),
                                ("fingerprints", Json::Arr(vec![])),
                                ("budget_match", Json::Bool(false)),
                                ("distributions", Json::Arr(vec![])),
                            ]),
                            vec![],
                            None,
                            false,
                            false,
                        );
                    }
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
        // AC-R-2.12.1-13 — an R2+ verdict *names its validator set*:
        // `resolved_dependencies.validators[]` (the pinned refs the
        // compiled `RuntimePlan` bound — `[]` is the honestly empty set).
        let validators = m
            .resolved_dependencies
            .get("validators")
            .cloned()
            .unwrap_or(Json::Arr(vec![]));
        // …and reports *distributions*, never means alone: one row per
        // seed leg the caller declared (`{seed, outcome, drift}`), the
        // per-leg verdict — no aggregate column.
        let distributions = if level == ReproLevel::R3 {
            match params.get("seeds") {
                Some(Json::Arr(seeds)) => Json::Arr(
                    seeds
                        .iter()
                        .map(|arm| {
                            let seed = arm
                                .get("seed")
                                .cloned()
                                .or_else(|| arm.as_int().map(Json::Int))
                                .unwrap_or(Json::Null);
                            Json::obj([
                                ("seed", seed),
                                (
                                    "outcome",
                                    Json::str(if drift.is_empty() && model_ok {
                                        "pass"
                                    } else {
                                        "drift"
                                    }),
                                ),
                                ("drift", Json::Int(drift.len() as i64)),
                            ])
                        })
                        .collect(),
                ),
                _ => Json::Arr(vec![]),
            }
        } else {
            Json::Arr(vec![])
        };
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
                ("validators", validators),
                ("distributions", distributions),
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
        let format = params
            .get("format")
            .and_then(Json::as_str)
            .unwrap_or(NATIVE_FORMAT)
            .to_string();
        if format != NATIVE_FORMAT {
            return self.kernel_import_foreign(params, &format, &holder);
        }
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

    /// `kernel.import{path: dir, format ∈ {harbor_trial_dir,
    /// harbor_job_dir}, holder?}` — the foreign lift (§5h.3 §2;
    /// AC-R-2.9.3-9). The caller's directory walks into the file map;
    /// the kernel opens a hosted carrier run, `lift_foreign` stamps
    /// every member `authority = unverified` under `origin =
    /// import(format)` with the mandatory `ImportRecord` +
    /// `MappingReport` (declared foreign digests that fail the supplied
    /// bytes land as `ForeignIntegrityMismatch` conflicts — never
    /// coerced). Unknown formats are `Refused{FormatUnknown}`.
    fn kernel_import_foreign(
        &mut self,
        params: &Json,
        format: &str,
        holder: &str,
    ) -> Result<Json, EmbedError> {
        if !hh_bundle::import::FOREIGN_FORMATS.contains(&format) {
            return Err(bundle_err(BundleError::FormatUnknown {
                detail: format!(
                    "{format} — supported: {} + {:?}",
                    NATIVE_FORMAT,
                    hh_bundle::import::FOREIGN_FORMATS
                ),
            }));
        }
        let dir = params
            .get("path")
            .and_then(Json::as_str)
            .ok_or_else(|| missing("kernel.import/path"))?;
        // Walk the directory — relpath → bytes (dirs skipped; symlinks
        // not followed by `is_file`).
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut stack = vec![Path::new(dir).to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).map_err(|e| EmbedError::Refused {
                reason: format!("import_dir: {e}"),
            })? {
                let entry = entry.map_err(|e| EmbedError::Refused {
                    reason: format!("import_dir: {e}"),
                })?;
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file() {
                    let rel = p
                        .strip_prefix(dir)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .to_string();
                    files.insert(
                        rel,
                        std::fs::read(&p).map_err(|e| EmbedError::Refused {
                            reason: format!("import_read: {e}"),
                        })?,
                    );
                }
            }
        }
        // The carrier run — a hosted-class run the lift's subject binds.
        let (run_id, _lease) = self
            .store
            .open_run(
                hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
                holder,
            )
            .map_err(ledger_err)?;
        let lift = hh_bundle::import::lift_foreign(
            &self.store,
            &run_id,
            &files,
            format,
            ProvenanceRecord::kernel(IMPORT_COMPONENT, self.store.now_ms()).to_json(),
            hh_ledger::store::rfc3339_ms(self.store.now_ms()),
            &self.kernel_version,
            self.store.now_ms(),
        )
        .map_err(bundle_err)?;
        // Deposit member + record bytes into the pool.
        for member in &lift.bundle.manifest.members {
            if let Some(bytes) = lift.bundle.members.get(&member.address) {
                self.store
                    .put_blob(bytes, &member.media_type)
                    .map_err(ledger_err)?;
            }
        }
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
        // The receipt row — coordinates only (no foreign fact is
        // re-asserted as kernel truth; ADR-0141 D1).
        self.store
            .commit_kernel_row_for(
                IMPORT_COMPONENT,
                &run_id,
                "lifecycle.run.imported",
                Json::obj([
                    ("format", Json::str(format)),
                    (
                        "bundle_id",
                        Json::str(lift.bundle.manifest.version_id.clone()),
                    ),
                    ("kind", Json::str("run")),
                    ("participant_class", Json::str("hosted")),
                ]),
                vec![record_addr, mapping_addr],
                vec![],
            )
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("schema", Json::str("hh-import-result/1")),
            ("run_id", Json::str(run_id)),
            ("bundle_id", Json::str(lift.bundle.manifest.version_id)),
            ("mapping_report", lift.mapping_report),
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
