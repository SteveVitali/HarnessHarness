//! Adapter C — the container-installed adapter (§6.6 §5; R-2.10.6 C2;
//! ADR-0164/0166).
//!
//! The participant runs as an installed process inside the Lab's
//! environment: the verbs are `install` / `run` / `kill` / `snapshot`
//! and the only observable surface is **`end_state`** plus whatever the
//! per-format **lifters** can reconstruct from the native logs at close.
//!
//! Honesty rules:
//!
//! * `observability_level = {end_state}` — a container-installed row can
//!   never claim `events`/`model_io`/`ledger`.
//! * Every reconstructed row is `mediation: unobserved`,
//!   `event_channel: log`, `origin: adapter` — the Lab scraped a log, it
//!   did not observe a channel; `mediated`/`observed` never appear on
//!   this mechanism.
//! * The reconstruction runs at `close` — a synthesized terminal is
//!   appended when no lifted row terminates the session; the native log
//!   rows themselves ride `lifecycle.hosted.native_record{raw_ref}` with
//!   mandatory provenance (ADR-0035 D4).

use std::collections::BTreeMap;

use hh_provenance::AuthorityClass;
use hh_wire::Json;

use crate::adapter_a::LiftedObservation;
use crate::events::{EventChannel, HostedOrigin, Mediation};
use crate::records::{AdapterRecord, ProcessPlacement};

/// The adapter-C id (`hh.adapter.c` — the container-installed adapter).
pub const ADAPTER_C_ID: &str = "hh.adapter.c";

/// The `end_state` snapshot `snapshot()` returns — the container's
/// terminal view (`exit_code`, the artefact list the participant left,
/// the reported outcome the log tail declares, when legible).
#[derive(Debug, Clone, PartialEq)]
pub struct EndStateSnapshot {
    /// The container exit code (`None` = still running at snapshot).
    pub exit_code: Option<i64>,
    /// The artefact paths/files the snapshot captured.
    pub artifacts: Vec<String>,
    /// The log-tail outcome line, when one parses.
    pub outcome: Option<Json>,
    /// The raw snapshot ref (`raw_ref` material — never inlined-opaque).
    pub raw_ref: Option<String>,
}

/// The container driver — records-in/records-out (the environment
/// plane's verbs are supplies; the adapter owns no handle).
pub trait ContainerDriver {
    /// `install(image_ref)` — place the participant image.
    fn install(&mut self, image_ref: &str) -> Result<String, String>;
    /// `run(container, spec)` — start the workload; the opaque container
    /// id the driver mints is its own.
    fn run(&mut self, container: &str, spec: &Json) -> Result<(), String>;
    /// `kill(container)` — terminate.
    fn kill(&mut self, container: &str) -> Result<(), String>;
    /// `snapshot(container)` — the end-state capture.
    fn snapshot(&mut self, container: &str) -> Result<EndStateSnapshot, String>;
    /// The native log rows since `watermark` — the lifters' input.
    fn log_rows(&mut self, container: &str, from_seq: u64) -> Vec<Json>;
}

/// A per-format log lifter — `kind_map` maps the log row's `type`/`kind`
/// field to a hosted kind; rows with no mapping fall to
/// `lifecycle.hosted.native_record` (preserved, never dropped — I-3).
#[derive(Debug, Clone, PartialEq)]
pub struct LogLifter {
    /// The format id (`anthropic_jsonl`, `otlp_json`, `stderr_lines`, …).
    pub format: String,
    /// `{log kind → hosted kind}` (e.g. `assistant → turn.output`).
    pub kind_map: BTreeMap<String, String>,
}

/// The lifter table Adapter C carries (the `AdapterRecord`'s
/// `ext.lifters` member lists the registered format ids).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LifterTable {
    /// `format → lifter`.
    pub formats: BTreeMap<String, LogLifter>,
}

impl LifterTable {
    /// The `stderr_lines` fallback lifter — always present: a log row no
    /// declared format claims lifts as a `native_record` (I-3).
    pub fn fallback() -> LogLifter {
        LogLifter {
            format: "stderr_lines".to_string(),
            kind_map: BTreeMap::new(),
        }
    }

    /// Lift one log row under `format` — `None` when the format is not
    /// registered (the caller falls to the fallback).
    fn lift_row(&self, format: &str, row: &Json) -> LiftedObservation {
        let lifted = self.formats.get(format).and_then(|l| {
            let k = row
                .get("type")
                .or_else(|| row.get("kind"))
                .and_then(Json::as_str)?;
            l.kind_map.get(k).map(|hk| (hk.clone(), k.to_string()))
        });
        let (kind, log_kind) = match lifted {
            Some((hk, lk)) => (hk, Some(lk)),
            None => (
                "lifecycle.hosted.native_record".to_string(),
                row.get("type")
                    .or_else(|| row.get("kind"))
                    .and_then(Json::as_str)
                    .map(str::to_string),
            ),
        };
        let mut payload = match row.clone() {
            Json::Obj(m) => m,
            other => {
                let mut m = BTreeMap::new();
                m.insert("row".into(), other);
                m
            }
        };
        payload.insert("source_format".into(), Json::str(format));
        if let Some(lk) = log_kind {
            payload.insert("native_kind".into(), Json::str(lk));
        }
        LiftedObservation {
            kind,
            payload: Json::Obj(payload),
            origin: HostedOrigin::Adapter,
            authority: AuthorityClass::Unverified,
            mediation: Mediation::Unobserved,
            event_channel: EventChannel::Log,
            raw_ref: None,
            ext: BTreeMap::new(),
        }
    }
}

/// Adapter C.
pub struct AdapterC {
    driver: Box<dyn ContainerDriver>,
    record: AdapterRecord,
    /// The per-format lifter table (the `end_state`-only surface's
    /// reconstruction engine).
    pub lifters: LifterTable,
    /// The container's declared log format (the participant's own).
    pub log_format: String,
    /// The log seq watermark the reconstruction consumed.
    log_watermark: u64,
    /// The container id `install` minted.
    container: Option<String>,
    /// A per-adapter monotone counter for `raw_ref` correlation.
    observation_seq: u64,
}

impl AdapterC {
    /// Build over a driver + record + lifter table. `log_format` names
    /// the participant's native log format (unknown formats lift through
    /// the `stderr_lines` fallback — never dropped).
    pub fn new(
        driver: Box<dyn ContainerDriver>,
        record: AdapterRecord,
        lifters: LifterTable,
        log_format: &str,
    ) -> AdapterC {
        AdapterC {
            driver,
            record,
            lifters,
            log_format: log_format.to_string(),
            log_watermark: 0,
            container: None,
            observation_seq: 0,
        }
    }

    /// The adapter's registry record.
    pub fn record(&self) -> &AdapterRecord {
        &self.record
    }

    fn next_raw(&mut self, tag: &str) -> String {
        self.observation_seq += 1;
        format!(
            "{}:{}:{}",
            self.record.adapter_id, tag, self.observation_seq
        )
    }

    fn stamped(&mut self, mut obs: LiftedObservation, tag: &str) -> LiftedObservation {
        obs.raw_ref = Some(self.next_raw(tag));
        obs
    }

    /// `install(image_ref)` — the placement verb.
    pub fn install(&mut self, image_ref: &str) -> Result<LiftedObservation, String> {
        let cid = self.driver.install(image_ref)?;
        self.container = Some(cid.clone());
        Ok(self.stamped(
            LiftedObservation {
                kind: "lifecycle.hosted.attached".to_string(),
                payload: Json::obj([
                    ("container", Json::str(&cid)),
                    ("image_ref", Json::str(image_ref)),
                    ("mechanism", Json::str("container_installed")),
                ]),
                origin: HostedOrigin::Adapter,
                authority: AuthorityClass::Environment,
                mediation: Mediation::Observed,
                event_channel: EventChannel::Handle,
                raw_ref: None,
                ext: BTreeMap::new(),
            },
            "install",
        ))
    }

    /// `run(spec)` — start the workload.
    pub fn run(&mut self, spec: &Json) -> Result<LiftedObservation, String> {
        let cid = self
            .container
            .clone()
            .ok_or_else(|| "not installed".to_string())?;
        self.driver.run(&cid, spec)?;
        Ok(self.stamped(
            LiftedObservation {
                kind: "lifecycle.hosted.run_started".to_string(),
                payload: Json::obj([("container", Json::str(&cid))]),
                origin: HostedOrigin::Adapter,
                authority: AuthorityClass::Environment,
                mediation: Mediation::Observed,
                event_channel: EventChannel::Handle,
                raw_ref: None,
                ext: BTreeMap::new(),
            },
            "run",
        ))
    }

    /// `kill()` — terminate the container.
    pub fn kill(&mut self) -> Result<LiftedObservation, String> {
        let cid = self
            .container
            .clone()
            .ok_or_else(|| "not installed".to_string())?;
        self.driver.kill(&cid)?;
        Ok(self.stamped(
            LiftedObservation {
                kind: "lifecycle.hosted.detached".to_string(),
                payload: Json::obj([
                    ("container", Json::str(&cid)),
                    ("reason", Json::str("kill")),
                ]),
                origin: HostedOrigin::Adapter,
                authority: AuthorityClass::Environment,
                mediation: Mediation::Observed,
                event_channel: EventChannel::Handle,
                raw_ref: None,
                ext: BTreeMap::new(),
            },
            "kill",
        ))
    }

    /// `snapshot()` — the `end_state` capture: the only observability
    /// surface this mechanism claims (the snapshot itself is the run's
    /// end-state evidence).
    pub fn snapshot(&mut self) -> Result<(EndStateSnapshot, LiftedObservation), String> {
        let cid = self
            .container
            .clone()
            .ok_or_else(|| "not installed".to_string())?;
        let snap = self.driver.snapshot(&cid)?;
        let obs = self.stamped(
            LiftedObservation {
                kind: "session.finished".to_string(),
                payload: Json::obj([
                    ("container", Json::str(&cid)),
                    (
                        "exit_code",
                        snap.exit_code.map(Json::Int).unwrap_or(Json::Null),
                    ),
                    (
                        "artifacts",
                        Json::Arr(snap.artifacts.iter().map(Json::str).collect()),
                    ),
                    ("outcome", snap.outcome.clone().unwrap_or(Json::Null)),
                ]),
                origin: HostedOrigin::Adapter,
                authority: AuthorityClass::Environment,
                mediation: Mediation::Observed,
                event_channel: EventChannel::Handle,
                raw_ref: None,
                ext: BTreeMap::new(),
            },
            "snapshot",
        );
        Ok((snap, obs))
    }

    /// `close` — drain the remaining log rows through the per-format
    /// lifters and reconstruct (`unobserved`, `log` channel, `adapter`
    /// origin — the Lab saw no channel, §6.6 §5); a synthesized
    /// `session.finished` terminal lands when nothing lifted terminates
    /// the session (the service's `ensure_terminal` path the adapter
    /// mirrors at the lift boundary).
    pub fn close_and_reconstruct(&mut self) -> Result<Vec<LiftedObservation>, String> {
        let cid = self
            .container
            .clone()
            .ok_or_else(|| "not installed".to_string())?;
        let from = self.log_watermark + 1;
        let rows = self.driver.log_rows(&cid, from);
        let mut out = Vec::new();
        let mut max_seq = self.log_watermark;
        for row in rows {
            if let Some(Json::Int(s)) = row.get("seq") {
                if *s > 0 && *s as u64 > max_seq {
                    max_seq = *s as u64;
                }
            }
            let fmt = if self.lifters.formats.contains_key(&self.log_format) {
                self.log_format.clone()
            } else {
                "stderr_lines".to_string()
            };
            let obs = self.lifters.lift_row(&fmt, &row);
            out.push(self.stamped(obs, "log"));
        }
        self.log_watermark = max_seq;
        // The synthesized terminal — a reconstructed session that never
        // emitted a terminal row still closes (the Lab synthesizes, never
        // drops; `unobserved`, adapter origin).
        let terminated = out.iter().any(|o| o.kind == "session.finished");
        if !terminated {
            out.push(self.stamped(
                LiftedObservation {
                    kind: "session.finished".to_string(),
                    payload: Json::obj([
                        ("container", Json::str(&cid)),
                        ("synthesized", Json::Bool(true)),
                        ("reason", Json::str("close")),
                    ]),
                    origin: HostedOrigin::Adapter,
                    authority: AuthorityClass::Unverified,
                    mediation: Mediation::Unobserved,
                    event_channel: EventChannel::Log,
                    raw_ref: None,
                    ext: BTreeMap::new(),
                },
                "terminal",
            ));
        }
        Ok(out)
    }
}

/// The Adapter-C `AdapterRecord` — `container_installed`,
/// `in_environment` placement, `end_state`-only defaults, the registered
/// lifter format ids on `ext.lifters`.
pub fn adapter_c_record(
    adapter_version: &str,
    lifters: &LifterTable,
    debt: hh_hir::records::AssumptionDebtRecord,
) -> AdapterRecord {
    let mut defaults = BTreeMap::new();
    for (dim, v) in [
        ("end_state", "supported"),
        ("streaming", "unsupported"),
        ("interrupt", "unsupported"),
        ("steer", "unsupported"),
        ("resume_cold", "unsupported"),
        ("resume_warm", "unsupported"),
        ("coordinate_model", "unsupported"),
        ("usage_reporting", "unknown"),
    ] {
        defaults.insert(dim.to_string(), Json::str(v));
    }
    let mut ext = BTreeMap::new();
    ext.insert(
        "lifters".to_string(),
        Json::Arr(lifters.formats.keys().map(Json::str).collect()),
    );
    AdapterRecord {
        adapter_id: ADAPTER_C_ID.to_string(),
        version_id: adapter_version.to_string(),
        hosting_mechanism: hh_ontology::participant::HostingMechanism::ContainerInstalled,
        participant_selector: Json::obj([("mechanism", Json::str("container_installed"))]),
        declaration_defaults: defaults,
        placement_supported: [ProcessPlacement::InEnvironment].into_iter().collect(),
        lowering_table_ref: None,
        loss_report_ref: None,
        debt,
        ext,
    }
}
