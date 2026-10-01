//! `foreign` — the C1 interchange leg (§5h.4 §2.1 `export`, §2.5 planes;
//! AC-R-2.9.4-10; ADR-0142 D3; ADR-0005).
//!
//! Two directions, both typed and both honest about loss:
//!
//! - `export(adapter, format, …)` lowers the run's adapter records into a
//!   foreign artifact (`harbor_trial_dir` — the trial directory the
//!   original runner regrades for parity). Every member with no foreign
//!   slot lands on the `LoweringLossReport`, never silently dropped
//!   (T-LCD-11).
//! - `import_result` lifts a foreign plane's result row (the
//!   Harness-Bench `harness_bench_result/1` shape) into an imported
//!   `product`-granularity row — foreign identity preserved verbatim,
//!   `observability_level` read from the artifact, `origin = import` /
//!   `authority = unverified` **always**: imported foreign facts are
//!   never upgraded to trusted/native authority (CC2, §5h.4 §6).

use std::collections::BTreeMap;

use hh_ontology::participant::Granularity;
use hh_wire::Json;

use crate::adapter::{AdapterError, BenchmarkAdapter, ExportFormat};
use crate::records::{BenchTask, Submission};

/// The interchange typed failures (refusals, never warnings).
#[derive(Debug, Clone, PartialEq)]
pub enum ForeignError {
    /// The foreign document failed schema shape (a required member
    /// missing or wrong-typed).
    Malformed {
        /// What failed.
        detail: String,
    },
    /// The document's `schema` member names a foreign format this crate
    /// does not consume.
    UnknownSchema {
        /// The refused schema spelling.
        schema: String,
    },
}

impl std::fmt::Display for ForeignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForeignError::Malformed { detail } => write!(f, "Malformed: {detail}"),
            ForeignError::UnknownSchema { schema } => {
                write!(f, "UnknownSchema({schema})")
            }
        }
    }
}

impl std::error::Error for ForeignError {}

/// `adapter_loss_entry/1` — one member that did not lower (or that the
/// import preserved verbatim without a typed home).
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// The member (dotted path).
    pub field: String,
    /// The loss reason (`no_slot` — the foreign form has no place for it;
    /// `foreign_only` — an import member with no typed home, preserved
    /// in `ext`).
    pub reason: String,
    /// What the member carried.
    pub detail: String,
}

/// `adapter_lowering_loss/1` — the `LoweringLossReport` of §5h.4 §2.1
/// `export → ForeignArtifact + LoweringLossReport` (the bench plane's
/// schema — one report shape per lowering boundary, CC7).
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// The lowering target (`adapter_export` | `harbor_trial_dir` |
    /// `harness_bench_result`).
    pub target: String,
    /// Whether the interchange was lossless.
    pub lossless: bool,
    /// Every dropped/preserved member.
    pub entries: Vec<LossEntry>,
}

impl LoweringLossReport {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("adapter_lowering_loss/1")),
            ("target", Json::str(&self.target)),
            ("lossless", Json::Bool(self.lossless)),
            (
                "entries",
                Json::Arr(
                    self.entries
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("field", Json::str(&e.field)),
                                ("reason", Json::str(&e.reason)),
                                ("detail", Json::str(&e.detail)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `foreign_artifact/1` — the export's lowered artifact plus its content
/// address (the `measurement.export.delivered` evidence input).
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignArtifact {
    /// The format it was lowered to.
    pub format: ExportFormat,
    /// The lowered body.
    pub body: Json,
    /// The body's `idp/1` content address.
    pub content_ref: String,
}

/// `export(adapter, format, task, submission, verdict, reward_ppm)` —
/// §5h.4 §2.1's `export(run, format) → ForeignArtifact +
/// LoweringLossReport`. The format must be declared on the adapter's
/// `export_formats[]` — an undeclared format refuses
/// `ExportFormatUnsupported` (never a silent lowering).
///
/// `adapter_export` emits the native replayable set (lossless).
/// `harbor_trial_dir` lowers to the foreign trial directory —
/// task/agent/verdict plus the submission payload; the `idp` identity,
/// artifact content addresses, apply evidence and provenance have no
/// foreign slot, and each lands on the loss report.
pub fn export(
    adapter: &impl BenchmarkAdapter,
    format: ExportFormat,
    task: &BenchTask,
    submission: &Submission,
    verdict: &Json,
    reward_ppm: i64,
) -> Result<(ForeignArtifact, LoweringLossReport), AdapterError> {
    adapter.declare().admits_export_format(format)?;
    match format {
        ExportFormat::AdapterExport => {
            let body = adapter.export(submission, verdict);
            let content_ref = hh_identity::idp_id(
                "bench.foreign_artifact",
                body.to_canonical_string().as_bytes(),
            );
            Ok((
                ForeignArtifact {
                    format,
                    body,
                    content_ref,
                },
                LoweringLossReport {
                    target: format.as_str().into(),
                    lossless: true,
                    entries: vec![],
                },
            ))
        }
        ExportFormat::HarborTrialDir => {
            let mut entries = Vec::new();
            // The members with no harbor-trial slot — listed, never
            // silently dropped (T-LCD-11).
            entries.push(LossEntry {
                field: "submission.submission_id".into(),
                reason: "no_slot".into(),
                detail: "the idp/1 submission identity has no foreign slot".into(),
            });
            if !submission.artifact_refs.is_empty() {
                entries.push(LossEntry {
                    field: "submission.artifact_refs".into(),
                    reason: "no_slot".into(),
                    detail: format!(
                        "{} content-addressed artifact refs have no foreign slot",
                        submission.artifact_refs.len()
                    ),
                });
            }
            if let Some(e) = &submission.apply_error {
                entries.push(LossEntry {
                    field: "submission.apply_error".into(),
                    reason: "no_slot".into(),
                    detail: format!("the typed apply-failure detail lowers nowhere: {e}"),
                });
            }
            entries.push(LossEntry {
                field: "task.provenance".into(),
                reason: "no_slot".into(),
                detail: "the task's provenance record has no foreign slot".into(),
            });
            entries.push(LossEntry {
                field: "verdict".into(),
                reason: "no_slot".into(),
                detail: "the verdict's evidence/detector members reduce to \
                         verifier_result.reward_ppm"
                    .into(),
            });
            let body = Json::obj([
                ("schema", Json::str("harbor_trial_dir/1")),
                ("format", Json::str(format.as_str())),
                (
                    "agent",
                    Json::obj([
                        ("name", Json::str("harnessharness")),
                        ("version", Json::str(adapter.declare().version_identity)),
                    ]),
                ),
                (
                    "suite",
                    Json::obj([
                        ("name", Json::str(task.foreign.name.clone())),
                        ("version", Json::str(task.foreign.version.clone())),
                        ("source_ref", Json::str(task.foreign.source_ref.clone())),
                    ]),
                ),
                (
                    "task",
                    Json::obj([
                        ("name", Json::str(task.foreign.name.clone())),
                        ("family", Json::str(task.family.name())),
                    ]),
                ),
                (
                    "verifier_result",
                    Json::obj([("reward_ppm", Json::Int(reward_ppm))]),
                ),
                (
                    "submission",
                    Json::obj([
                        ("payload_hex", Json::str(&submission.payload_hex)),
                        ("applied", Json::Bool(submission.applied)),
                    ]),
                ),
            ]);
            let content_ref = hh_identity::idp_id(
                "bench.foreign_artifact",
                body.to_canonical_string().as_bytes(),
            );
            Ok((
                ForeignArtifact {
                    format,
                    body,
                    content_ref,
                },
                LoweringLossReport {
                    target: format.as_str().into(),
                    lossless: false,
                    entries,
                },
            ))
        }
    }
}

/// `imported_result/1` — a foreign plane's result row lifted into the
/// results vocabulary (§5h.4 §2.5 (b)): `product`-granularity, foreign
/// identity verbatim, `observability_level` as the artifact declared it,
/// `origin = import` / `authority = unverified` — imported facts never
/// upgrade to trusted/native authority (CC2).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedResult {
    /// The foreign suite name.
    pub foreign_suite: String,
    /// The foreign task name.
    pub foreign_task: String,
    /// The foreign run/trial id.
    pub foreign_run_id: String,
    /// The reported `task_success` reward (ppm).
    pub reward_ppm: i64,
    /// The artifact's declared `observability_level` (`[]` = undeclared —
    /// unknown is never coerced; a ledger-observability cell on this row
    /// renders `n/a{observability}`).
    pub observability_level: Vec<String>,
    /// The comparison granularity the import is admissible at — always
    /// `product` (§5h.4 §2.5: imported rows are product-granularity only).
    pub granularity: Granularity,
    /// Foreign members with no typed home — preserved byte-for-byte
    /// (T-LCD-11; they never feed typed fields).
    pub foreign_preserved: BTreeMap<String, Json>,
}

impl ImportedResult {
    /// The canonical row (`imported_result/1`) — `origin`/`authority` are
    /// fixed members, never caller-supplied.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("imported_result/1"));
        m.insert("foreign_suite".into(), Json::str(&self.foreign_suite));
        m.insert("foreign_task".into(), Json::str(&self.foreign_task));
        m.insert("foreign_run_id".into(), Json::str(&self.foreign_run_id));
        m.insert("reward_ppm".into(), Json::Int(self.reward_ppm));
        m.insert(
            "observability_level".into(),
            Json::Arr(
                self.observability_level
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        );
        m.insert("granularity".into(), Json::str(self.granularity.as_str()));
        m.insert("origin".into(), Json::str("import"));
        m.insert("authority".into(), Json::str("unverified"));
        if !self.foreign_preserved.is_empty() {
            m.insert(
                "foreign_preserved".into(),
                Json::Obj(self.foreign_preserved.clone()),
            );
        }
        Json::Obj(m)
    }
}

/// `import_result(doc)` — lift a `harness_bench_result/1` document into an
/// `ImportedResult` plus its `LoweringLossReport` (§5h.4 §2.5 (b);
/// AC-R-2.9.4-10's import half). Members without a typed home are
/// preserved verbatim in `foreign_preserved` *and* listed on the loss
/// report — lossy interchange is explicit, never silent.
pub fn import_result(doc: &Json) -> Result<(ImportedResult, LoweringLossReport), ForeignError> {
    const KNOWN: &[&str] = &[
        "schema",
        "suite",
        "task",
        "run_id",
        "reward_ppm",
        "observability_level",
    ];
    let m = match doc {
        Json::Obj(m) => m,
        _ => {
            return Err(ForeignError::Malformed {
                detail: "not an object".into(),
            })
        }
    };
    let schema = m
        .get("schema")
        .and_then(Json::as_str)
        .ok_or_else(|| ForeignError::Malformed {
            detail: "missing `schema`".into(),
        })?;
    if schema != "harness_bench_result/1" {
        return Err(ForeignError::UnknownSchema {
            schema: schema.into(),
        });
    }
    let str_member = |k: &str| -> Result<String, ForeignError> {
        m.get(k)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| ForeignError::Malformed {
                detail: format!("`{k}` missing or not a string"),
            })
    };
    let reward_ppm = match m.get("reward_ppm") {
        Some(Json::Int(v)) if (0..=1_000_000).contains(v) => *v,
        _ => {
            return Err(ForeignError::Malformed {
                detail: "`reward_ppm` missing, non-integer, or outside [0, 1e6]".into(),
            })
        }
    };
    // `observability_level` — read as declared; absent stays absent
    // (T-LCD-07). An unrecognised spelling is preserved, not coerced.
    let observability_level = match m.get("observability_level") {
        None | Some(Json::Null) => vec![],
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ForeignError::Malformed {
                        detail: "observability_level member not a string".into(),
                    })
            })
            .collect::<Result<_, _>>()?,
        _ => {
            return Err(ForeignError::Malformed {
                detail: "`observability_level` not an array".into(),
            })
        }
    };
    let mut foreign_preserved = BTreeMap::new();
    let mut entries = Vec::new();
    for (k, v) in m {
        if !KNOWN.contains(&k.as_str()) {
            entries.push(LossEntry {
                field: k.clone(),
                reason: "foreign_only".into(),
                detail: "member has no typed home — preserved verbatim".into(),
            });
            foreign_preserved.insert(k.clone(), v.clone());
        }
    }
    Ok((
        ImportedResult {
            foreign_suite: str_member("suite")?,
            foreign_task: str_member("task")?,
            foreign_run_id: str_member("run_id")?,
            reward_ppm,
            observability_level,
            granularity: Granularity::ProductLevel,
            foreign_preserved,
        },
        LoweringLossReport {
            target: "harness_bench_result".into(),
            lossless: entries.is_empty(),
            entries,
        },
    ))
}
