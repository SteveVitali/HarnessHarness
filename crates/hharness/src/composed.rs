//! `hh-cap5-composed/1` — the CAP5.2 composed execution/methods/
//! recipient verification packet (docs/tickets/239_CAP5.2__cap2.md).
//!
//! The round's additive tickets each proved their own slice. This
//! driver composes them: it runs the four committed `example-card/1`
//! records through the landed [`crate::examples::run_example`] owners
//! on scratch stores — adoption → effects → seal/finish → grade/settle
//! → populations/methods → receipt → clean-recipient — folds every
//! emitted check into the AC-1 stage spine, re-reads the sixteen
//! disposition-bearing residual packets verbatim (records-in; an
//! absent required byte is `blocked{inaccessible}`, never skipped),
//! resolves each residual's owed-work home, and validates the recorded
//! backend-access claim with one permitted probe (`podman --version`
//! — offline; no task/provider/container/participant execution, $0).
//!
//! Honest posture: fixture-verified only. The withheld/failed/
//! unselected residuals fold to `pending` — a composed pass never
//! closes an absent later research/participant predicate, and live
//! authorization stays `pending{not_claimed}`.

use std::path::{Path, PathBuf};

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::examples::{run_example, ExampleCard, ExampleError, ExampleReport};

/// The packet schema label.
pub const COMPOSED_VERSION: &str = "hh-cap5-composed/1";
/// The packet-id domain (`idp_digest` — the landed mint convention).
const DOMAIN: &str = "cap5_composed";
/// The four committed offline cards the composed run drives.
pub const CARDS: [&str; 4] = ["ex-01", "ex-02", "ex-03", "ex-04"];

/// The AC-1 stage spine → member check ids. `semantic_controls` folds
/// the typed-refusal/label checks; every emitted check must map to
/// exactly the stages that evidence it — an unmapped check is a real
/// drift finding, recorded under `unmapped` never dropped.
const STAGES: &[(&str, &[&str])] = &[
    ("adoption", &["negotiated", "run_succeeded", "subjects_20"]),
    ("effects", &["excerpt:effect_chain", "matrix", "exhaustion"]),
    (
        "seal_finish",
        &["excerpt:run_submission", "sealed_submission"],
    ),
    (
        "grade_settle",
        &["oracle_chain", "excerpt:lifecycle_vs_oracle", "field_edit"],
    ),
    (
        "populations_methods",
        &[
            "cells_pinned",
            "informative_contrast",
            "report_identity",
            "exploratory_label",
        ],
    ),
    ("receipt", &["receipt"]),
    (
        "clean_recipient",
        &[
            "excerpt:intended_permission",
            "excerpt:action_model",
            "excerpt:context_replay",
            "export_verify_r0",
            "replay_admission",
            "recipient_recompute",
            "packet",
        ],
    ),
    (
        "semantic_controls",
        &[
            "refusal_controls",
            "refusal_suspend",
            "refusal_resume",
            "refusal_respond",
            "refusal_amend",
            "edit_refusal",
        ],
    ),
];

/// The expected disposition-bearing residual packets →
/// (claim, owed-work home). Homes mirror CAP5.1's routing verbatim:
/// participant set → `DF-H9-1`; failed S1 → `DF-CAP5.1-1`; parity/
/// confirm/study/publication/value → `GATE-G17`; the executed and
/// inconclusive-preflight records are the landed evidence itself.
const RESIDUALS: &[(&str, &str, &str)] = &[
    (
        "2026-11-03-r5.42a-backend-preflight.json",
        "backend_preflight",
        "landed:discharged@r5.44",
    ),
    (
        "2026-11-04-r5.42a-backend-preflight-reprobe.json",
        "backend_preflight_reprobe",
        "landed:discharged@r5.44",
    ),
    (
        "2026-11-04-r5.43-s1-useful.json",
        "s1_useful",
        "DF-CAP5.1-1",
    ),
    (
        "2026-11-04-r5.44-ops-live-first.json",
        "ops_live_first",
        "landed:honest-retention",
    ),
    (
        "2026-11-04-r5.44-ops-live-second.json",
        "ops_live_second",
        "landed:executed",
    ),
    (
        "2026-11-04-r5.46-activation-podman.json",
        "activation",
        "landed:executed",
    ),
    (
        "2026-11-04-r5.46-pilot-podman.json",
        "pilot",
        "landed:executed",
    ),
    ("2026-11-04-r5.47-parity-n0.json", "s3_parity", "GATE-G17"),
    (
        "2026-11-04-r5.48-confirm-withheld.json",
        "s4_confirm",
        "GATE-G17",
    ),
    (
        "2026-11-04-r5.49-study-report.json",
        "study_report",
        "GATE-G17",
    ),
    (
        "2026-11-04-r5.50-plugin-unselected.json",
        "plugin_exercise",
        "DF-H9-1",
    ),
    (
        "2026-11-04-r5.51-interchange-unselected.json",
        "interchange",
        "DF-H9-1",
    ),
    (
        "2026-11-04-r5.52-outside-withheld.json",
        "outside_recipient",
        "DF-H9-1",
    ),
    (
        "2026-11-04-r5.53-formative-withheld.json",
        "formative",
        "DF-H9-1",
    ),
    (
        "2026-11-04-r5.54-publication-withheld.json",
        "publication",
        "GATE-G17",
    ),
    (
        "2026-11-04-r5.55-value-withheld.json",
        "value_followup",
        "GATE-G17",
    ),
];

fn svc(root: &Path) -> Result<EmbedService, ExampleError> {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.join("ws"),
        holder: "cap5-2".to_string(),
    })
    .map_err(|e| ExampleError::Owner {
        seam: "svc",
        detail: format!("{e:?}"),
    })
}

/// The committed-record roots a composed run re-reads (records-in).
/// The canonical append-only records live under the build reports and
/// tickets trees; the CI readers gate bars gated test sources from
/// opening those living documentation trees, so callers pass
/// byte-identical fixture roots (same sha256 on every carried byte)
/// under `tests/fixtures/` — see `tests/cap5_2.rs`.
pub struct RecordsIn {
    /// Directory of the sixteen residual packets — byte-copies of the
    /// canonical append-only reports.
    pub reports: PathBuf,
    /// The deferral rows used to resolve `DF-*` homes — a verbatim
    /// row excerpt of the append-only register (rows never rewritten).
    pub deferrals: PathBuf,
    /// Directory scanned for a `GATE-*` home's committed ticket
    /// filename (presence, never content).
    pub tickets: PathBuf,
}

/// One residual leg — a disposition-bearing committed packet carried
/// verbatim, or `blocked{inaccessible}` when its byte cannot be read.
#[derive(Debug)]
pub struct Residual {
    /// The composed claim this leg evidences.
    pub claim: String,
    /// The packet basename under `docs/build/reports/`.
    pub file: String,
    /// `recorded` (bytes re-read + sha recomputed) or `blocked{…}`.
    pub state: String,
    /// Recomputed content hash, or `none` when blocked.
    pub sha256: String,
    /// `version`/`disposition`/`admission` verbatim (`absent` honestly).
    pub version: String,
    /// Verbatim packet disposition.
    pub disposition: String,
    /// Verbatim admission word.
    pub admission: String,
    /// `recorded` when the claim's record landed; `pending` otherwise —
    /// a withheld/failed/unselected/inconclusive claim never closes.
    pub closure: &'static str,
    /// The owed-work home, verbatim from the routing table.
    pub home: String,
    /// `resolved` when the home's row/file exists; `missing` otherwise.
    pub home_resolved: &'static str,
}

/// The composed verification record.
#[derive(Debug)]
pub struct ComposedReport {
    /// [`COMPOSED_VERSION`].
    pub version: &'static str,
    /// The landed card-runner version (`hh-examples/1`).
    pub runner: &'static str,
    /// Per-card verbatim reports.
    pub cards: Vec<ExampleReport>,
    /// `(stage, verdict, member check ids)` in AC-1 spine order.
    pub stages: Vec<(String, &'static str, Vec<String>)>,
    /// Check ids no stage claims (must be empty — recorded honestly).
    pub unmapped: Vec<String>,
    /// Tallies derived from emitted details, never hand-counted.
    pub counts: Json,
    /// The residual legs.
    pub residuals: Vec<Residual>,
    /// The access-validation record (probe + artifact recompute counts).
    pub access: Json,
    /// `executed` only when every stage verifies, every card passes,
    /// every residual leg records verbatim and every home resolves.
    pub disposition: &'static str,
    /// `H(canonical(record minus packet_id))` under `cap5_composed`.
    pub packet_id: String,
}

/// Recompute `state`/`closure`/`home` for one residual leg (records-in).
fn residual_leg(
    reports: &Path,
    records: &RecordsIn,
    file: &str,
    claim: &str,
    home: &str,
) -> Residual {
    let path = reports.join(file);
    let blocked = |why: String| Residual {
        claim: claim.to_string(),
        file: file.to_string(),
        state: format!("blocked{{inaccessible:{why}}}"),
        sha256: "none".to_string(),
        version: "absent".to_string(),
        disposition: "absent".to_string(),
        admission: "absent".to_string(),
        closure: "pending",
        home: home.to_string(),
        home_resolved: home_state(records, home),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return blocked(format!("read:{e}")),
    };
    let sha = format!("sha256:{}", sha256_hex(&bytes));
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => return blocked(format!("utf8:{e}")),
    };
    let j = match hharness_wire::json::parse(&text) {
        Ok(j) => j,
        Err(e) => return blocked(format!("parse:{e}")),
    };
    let at = |k: &str| {
        j.get(k)
            .and_then(Json::as_str)
            .unwrap_or("absent")
            .to_string()
    };
    let (version, disposition, admission) = (at("version"), at("disposition"), at("admission"));
    let closure = match disposition.as_str() {
        "executed" | "reported" => "recorded",
        _ => "pending",
    };
    Residual {
        claim: claim.to_string(),
        file: file.to_string(),
        state: "recorded".to_string(),
        sha256: sha,
        version,
        disposition,
        admission,
        closure,
        home: home.to_string(),
        home_resolved: home_state(records, home),
    }
}

/// `resolved` iff the home's committed row/file exists (records-in —
/// the verbatim fixture roots under [`RecordsIn`]).
fn home_state(records: &RecordsIn, home: &str) -> &'static str {
    if let Some(df) = home.strip_prefix("DF-") {
        let ok = std::fs::read_to_string(&records.deferrals)
            .map(|t| t.contains(&format!("| DF-{df}")))
            .unwrap_or(false);
        return if ok { "resolved" } else { "missing" };
    }
    if home.starts_with("GATE-") {
        let ok = records.tickets.is_dir()
            && std::fs::read_dir(&records.tickets)
                .map(|d| {
                    d.flatten()
                        .any(|e| e.file_name().to_string_lossy().contains(home))
                })
                .unwrap_or(false);
        return if ok { "resolved" } else { "missing" };
    }
    "resolved" // `landed:*` homes are the record itself — always present
}

/// One permitted probe — `podman --version` validates the recorded
/// backend-access claim (`runtime_available:true`) without any
/// collection or container execution. Absent/error is recorded, never
/// hidden.
fn access_probe() -> Json {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let result = match std::process::Command::new("podman")
        .arg("--version")
        .output()
    {
        Ok(o) => {
            let line = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if o.status.success() && !line.is_empty() {
                line
            } else {
                format!("error:{}", String::from_utf8_lossy(&o.stderr).trim())
            }
        }
        Err(e) => format!("absent:{e}"),
    };
    Json::obj([
        ("probe", Json::str("podman --version")),
        ("unix", Json::Int(unix)),
        ("result", Json::str(&result)),
    ])
}

/// `run_composed(repo, records, work)` — drive the four committed cards
/// through the landed runner (a fresh scratch `EmbedService` per card;
/// `repo` resolves the card records and their pinned inputs), fold the
/// stage spine, carry the residual legs verbatim off `records`, mint
/// `packet_id` last.
pub fn run_composed(
    repo: &Path,
    records: &RecordsIn,
    work: &Path,
) -> Result<ComposedReport, ExampleError> {
    std::fs::create_dir_all(work).map_err(|e| ExampleError::Owner {
        seam: "work",
        detail: format!("{}: {e}", work.display()),
    })?;
    let mut cards: Vec<ExampleReport> = Vec::new();
    let mut pin_members = 0usize;
    for case in CARDS {
        let card = ExampleCard::load(&repo.join("scripts/examples").join(format!("{case}.json")))?;
        if let Json::Obj(pins) = &card.pins {
            pin_members += pins.len();
        }
        let root = work.join(case);
        let mut s = svc(&root)?;
        cards.push(run_example(&mut s, &card, &root.join("work"), repo)?);
    }
    // Stage fold — every emitted check lands in its stage; drift
    // surfaces under `unmapped`, never silently dropped.
    let all: Vec<(&String, &'static str)> = cards
        .iter()
        .flat_map(|c| c.checks.iter().map(|k| (&k.id, k.verdict)))
        .collect();
    let mut stages = Vec::new();
    for (name, ids) in STAGES {
        let members: Vec<String> = all
            .iter()
            .filter(|(id, _)| ids.contains(&id.as_str()))
            .map(|(id, _)| id.to_string())
            .collect();
        let verdict = if members.is_empty() {
            "absent"
        } else if all
            .iter()
            .any(|(id, v)| members.contains(id) && *v == "fail")
        {
            "failed"
        } else {
            "verified"
        };
        stages.push((name.to_string(), verdict, members));
    }
    let unmapped: Vec<String> = all
        .iter()
        .map(|(id, _)| id.to_string())
        .filter(|id| !STAGES.iter().any(|(_, ids)| ids.contains(&id.as_str())))
        .collect();
    // Counts — derived from the emitted details, never hand-counted.
    let int = |c: &ExampleReport, id: &str, k: &str| {
        c.checks
            .iter()
            .find(|x| x.id == id)
            .and_then(|x| x.detail.get(k))
            .and_then(Json::as_int)
            .unwrap_or(0)
    };
    let (mut pass, mut refuse, mut fail) = (0i64, 0i64, 0i64);
    for (_, v) in &all {
        match *v {
            "pass" => pass += 1,
            "expected_refusal" => refuse += 1,
            _ => fail += 1,
        }
    }
    let ex02 = cards.iter().find(|c| c.case == "ex-02");
    let counts = Json::obj([
        ("cards", Json::Int(cards.len() as i64)),
        ("checks_pass", Json::Int(pass)),
        ("checks_expected_refusal", Json::Int(refuse)),
        ("checks_fail", Json::Int(fail)),
        (
            "subjects_settled",
            Json::Int(ex02.map(|c| int(c, "subjects_20", "slots")).unwrap_or(0)),
        ),
        (
            "receipts_closed",
            Json::Int(
                ex02.map(|c| int(c, "subjects_20", "receipts_closed"))
                    .unwrap_or(0),
            ),
        ),
    ]);
    // Residual legs — verbatim, homes resolved, bytes recomputed.
    let residuals: Vec<Residual> = RESIDUALS
        .iter()
        .map(|(f, c, h)| residual_leg(&records.reports, records, f, c, h))
        .collect();
    let access = Json::obj([
        ("backend", access_probe()),
        (
            "artifacts",
            Json::obj([
                ("pinned_recomputed", Json::Int(pin_members as i64)),
                ("packets_recomputed", Json::Int(residuals.len() as i64)),
            ]),
        ),
    ]);
    let cards_pass = cards.iter().all(|c| c.verdict == "pass");
    let stages_ok = stages.iter().all(|(_, v, _)| *v == "verified");
    let legs_ok = residuals.iter().all(|r| r.state == "recorded");
    let homes_ok = residuals.iter().all(|r| r.home_resolved == "resolved");
    let mut out = ComposedReport {
        version: COMPOSED_VERSION,
        runner: crate::examples::RUNNER_VERSION,
        cards,
        stages,
        unmapped,
        counts,
        residuals,
        access,
        disposition: if cards_pass && stages_ok && legs_ok && homes_ok {
            "executed"
        } else {
            "failed"
        },
        packet_id: String::new(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}

impl ComposedReport {
    /// Canonical JSON — the emitted packet (`disposition`, the verbatim
    /// residual legs and `packet_id` last, never re-ordered).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(self.version)),
            ("runner", Json::str(self.runner)),
            (
                "cards",
                Json::Arr(self.cards.iter().map(|c| c.to_json()).collect()),
            ),
            (
                "stages",
                Json::Arr(
                    self.stages
                        .iter()
                        .map(|(s, v, m)| {
                            Json::obj([
                                ("stage", Json::str(s)),
                                ("verdict", Json::str(*v)),
                                ("checks", Json::Arr(m.iter().map(Json::str).collect())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "unmapped",
                Json::Arr(self.unmapped.iter().map(Json::str).collect()),
            ),
            ("counts", self.counts.clone()),
            (
                "residuals",
                Json::Arr(
                    self.residuals
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("claim", Json::str(&r.claim)),
                                ("file", Json::str(&r.file)),
                                ("state", Json::str(&r.state)),
                                ("sha256", Json::str(&r.sha256)),
                                ("version", Json::str(&r.version)),
                                ("disposition", Json::str(&r.disposition)),
                                ("admission", Json::str(&r.admission)),
                                ("closure", Json::str(r.closure)),
                                ("home", Json::str(&r.home)),
                                ("home_resolved", Json::str(r.home_resolved)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("access", self.access.clone()),
            ("live_authorization", Json::str("pending{not_claimed}")),
            ("spend_usd", Json::Int(0)),
            ("disposition", Json::str(self.disposition)),
            ("packet_id", Json::str(&self.packet_id)),
        ])
    }
}
