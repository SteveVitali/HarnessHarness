//! Wc4 (R6.45d-lineage) repair battery — the O-surface's fifth
//! machinery defect: H-allow cells `exited{0}` →
//! `verifier_unavailable:reward_missing` (Wc2 + Wc3 verbatim).
//!
//! Two coupled machinery legs, both exercised fails-without:
//!
//! 1. `run_cell` never ensured the verifier log dir — the reward sink
//!    — exists before the verifier exec. On the happy path the
//!    ctrf plugin's `--ctrf /logs/verifier/ctrf.json` creates it; on
//!    the `allowlist_mediated` bootstrap-deny path pytest never runs,
//!    the trailing `echo … > /logs/verifier/reward.txt` fails on the
//!    absent dir, and a completed verifier records `reward_missing`.
//!    The ST5 leg now `mkdir -p /logs/verifier` like `/tests`.
//! 2. `denied_hosts` folded a `host` member the decided rows never
//!    carry (`host_norm`/`host_raw` are the committed members) and
//!    could not attribute phase: every deny row in the run counted as
//!    verifier evidence. The fold now matches the real members and
//!    attributes by `policy_version_id` — the verify layer's
//!    `version_id` — so `denied_hosts` records the verify-phase
//!    denies and the §3.4 classifier lands
//!    `bootstrap:egress_denied`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::cell::{run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan};
use hharness_runner::verdict::{self, MediatorRow};
use hharness_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r645d-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `DeniedBootstrapCell` — models the recorded H-allow cell: the
/// verifier runs to completion but its bootstrap dies before pytest
/// (denied egress), so `reward.txt` can be written only when the
/// lifecycle created `/logs/verifier`. `mkdir` execs record the dir
/// in `fs`; the verifier exec produces `reward.txt` iff the sink
/// exists — exactly the in-cell reality the Wc2/Wc3 harvests show.
struct DeniedBootstrapCell {
    fs: std::collections::BTreeSet<String>,
    copies: BTreeMap<String, Vec<u8>>,
    log: Vec<String>,
}

impl CellOps for DeniedBootstrapCell {
    fn present(&mut self, path: &str) -> Result<bool, CellError> {
        Ok(self.fs.contains(path))
    }
    fn ls_dir(&mut self, _path: &str) -> Result<Vec<String>, CellError> {
        Ok(self.fs.iter().cloned().collect())
    }
    fn cp_in(&mut self, _host_src: &Path, dst: &str) -> Result<(), CellError> {
        self.log.push(format!("cp_in ->{dst}"));
        self.fs.insert(dst.to_string());
        Ok(())
    }
    fn cp_out(&mut self, src: &str, dst: &Path) -> Result<(), CellError> {
        self.log.push(format!("cp_out {src}"));
        match self.copies.get(src) {
            Some(b) => {
                std::fs::write(dst, b).unwrap();
                Ok(())
            }
            None => Err(CellError {
                op: "cp_out",
                detail: format!("{src} absent"),
            }),
        }
    }
    fn rm_glob(&mut self, glob: &str) -> Result<(), CellError> {
        self.log.push(format!("rm {glob}"));
        Ok(())
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        self.log.push(format!("exec {}", spec.argv.join(" ")));
        if spec.argv.first().map(String::as_str) == Some("mkdir") {
            if let Some(dir) = spec.argv.last() {
                self.fs.insert(dir.clone());
            }
            return Ok(CellOutcome {
                exit_code: Some(0),
                timed_out: false,
                stdout: vec![],
            });
        }
        if spec.argv == ["bash", "/tests/test.sh"] {
            // The denied-bootstrap verifier: it ran its course (apt
            // denied → no uv → pytest never ran → nothing created
            // /logs/verifier), then `echo 0 > /logs/verifier/reward.txt`
            // — which lands iff the dir exists (the recorded defect:
            // on the unfixed drive it did not).
            if self.fs.contains("/logs/verifier") {
                self.copies
                    .insert("/logs/verifier/reward.txt".into(), b"0".to_vec());
            }
        }
        Ok(CellOutcome {
            exit_code: Some(0),
            timed_out: false,
            stdout: vec![],
        })
    }
    fn terminate_group(&mut self, _pattern: &str) -> Result<(), CellError> {
        Ok(())
    }
}

fn plan(dir: &Path) -> CellPlan {
    std::fs::create_dir_all(dir.join("staging/solution")).unwrap();
    std::fs::create_dir_all(dir.join("staging/tests")).unwrap();
    CellPlan {
        oracle_participant: true,
        workdir: "/app".to_string(),
        host_staging: dir.join("staging"),
        collect_dir: dir.join("collect"),
        agent_argv: vec!["bash".to_string(), "/solution/solve.sh".to_string()],
        agent_deadline_ms: Some(12_000_000),
        verifier_argv: vec!["bash".to_string(), "/tests/test.sh".to_string()],
        verifier_deadline_ms: Some(12_000_000),
        max_grade_attempts: 2,
        logs_copyout: None,
    }
}

/// The recorded defect's head leg: `exited{0}` → a findable reward —
/// the lifecycle creates `/logs/verifier` before the verifier exec,
/// so the denied-bootstrap verifier's trailing `reward.txt` write
/// lands and copy-out collects it (one verify attempt, not two).
#[test]
fn completed_cell_produces_a_findable_reward() {
    let dir = tmp("reward");
    let mut cell = DeniedBootstrapCell {
        fs: Default::default(),
        copies: Default::default(),
        log: vec![],
    };
    let report = run_cell(&mut cell, &plan(&dir)).unwrap();
    // The verifier ran under a reward sink that exists.
    let mkdir = cell
        .log
        .iter()
        .position(|l| l == "exec mkdir -p /logs/verifier")
        .expect("the ST5 leg creates the verifier log dir");
    let verify = cell
        .log
        .iter()
        .position(|l| l == "exec bash /tests/test.sh")
        .unwrap();
    assert!(mkdir < verify, "the sink exists before the verifier exec");
    // The reward lands and is collected — not `reward_missing`.
    let files = &report.verify.as_ref().unwrap().files;
    assert_eq!(files.reward_txt.as_deref(), Some(b"0".as_slice()));
    assert!(dir.join("collect/reward.txt").is_file());
    assert_eq!(
        report.verify_attempts, 1,
        "a decided reward ends the regrade"
    );
    // And the channel classifies it — with the verify-phase deny rows
    // the typed cause is `bootstrap:egress_denied`, never
    // `verifier_unavailable:reward_missing`.
    let deny = MediatorRow {
        phase: "verify".into(),
        decision: "deny".into(),
    };
    let v = verdict::classify(files, &[deny], false);
    assert_eq!(
        v.classification, "verifier_unavailable:bootstrap:egress_denied",
        "the recorded cell's honest verdict (a reward produced under a denied bootstrap)"
    );
    // Without deny evidence the same bytes are a decided-0 — still a
    // produced reward, flagged `infra_suspected`, not `reward_missing`.
    let v = verdict::classify(files, &[], false);
    assert_eq!(v.classification, "decided");
    assert_eq!(v.reward_ppm, Some(0));
}

/// The deny-evidence fold: decided rows carry `host_norm`/`host_raw`
/// and `policy_version_id` (the decided layer) — a `host`-only walk
/// records `denied_hosts: []` over live denies (the recorded Wc/Wc2/
/// Wc3 mis-signature) and non-verify-layer denies are not verifier
/// evidence.
#[test]
fn denied_hosts_reads_the_real_members_and_attributes_verify() {
    let decided = |vid: &str, decision: &str| {
        Json::obj([
            ("decision", Json::str(decision)),
            ("host_norm", Json::str("archive.ubuntu.com")),
            ("host_raw", Json::str("archive.ubuntu.com")),
            ("policy_version_id", Json::str(vid)),
            ("reason", Json::str("not_allowed")),
            ("source", Json::str("default_unmatched")),
        ])
    };
    let rows: Vec<(&str, Json)> = vec![
        (
            "security.egress.requested",
            decided("sha256:verify", "deny"),
        ),
        ("security.egress.decided", decided("sha256:verify", "deny")),
        ("security.egress.decided", decided("sha256:verify", "deny")),
        // an agent-layer deny — decided under a different policy
        // version, never verifier-bootstrap evidence
        ("security.egress.decided", decided("sha256:agent", "deny")),
        // an allow under the verify layer — not a deny
        ("security.egress.decided", decided("sha256:verify", "allow")),
        // an unrelated class carrying a deny-looking member
        ("action.effect.intended", decided("sha256:verify", "deny")),
    ];
    let iter = rows.iter().map(|(c, p)| (*c, p));
    let hosts = verdict::denied_hosts(iter, Some("sha256:verify"));
    assert_eq!(hosts, vec!["archive.ubuntu.com".to_string()]);
    // The recorded defect: `host`-only matching would find nothing
    // here — the committed payload carries host_norm/host_raw.
    let iter = rows.iter().map(|(c, p)| (*c, p));
    let all = verdict::denied_hosts(iter, None);
    assert_eq!(
        all,
        vec!["archive.ubuntu.com".to_string()],
        "unscoped, both layers' denies collect — the scoped call is the attribution leg"
    );
}

/// The end-to-end seam: `denied_hosts` over the recorded Wc3 payload
/// shape → the `MediatorRow`s `h_run_once` mints → the channel's
/// `bootstrap:egress_denied` for a produced zero reward.
#[test]
fn the_recorded_signature_resolves_to_bootstrap_egress_denied() {
    // The verbatim Wc3 decided-row members (harvest
    // torch-tensor-parallelism/H-allow/r0 mediator-store).
    let payload = Json::obj([
        ("checked_addrs", Json::Arr(vec![])),
        ("decided_by", Json::str("policy")),
        ("decision", Json::str("deny")),
        ("effect_id", Json::str("eff-run")),
        ("env_handle", Json::str("env-run")),
        ("host_norm", Json::str("security.ubuntu.com")),
        ("host_raw", Json::str("security.ubuntu.com")),
        ("latency_ms", Json::Int(6)),
        ("method", Json::str("GET")),
        ("policy_version_id", Json::str("sha256:verify")),
        ("port", Json::Int(80)),
        ("protocol", Json::str("http")),
        ("reason", Json::str("not_allowed")),
        ("source", Json::str("default_unmatched")),
    ]);
    let events = [("security.egress.decided", &payload)];
    let denied = verdict::denied_hosts(events.iter().map(|(c, p)| (*c, *p)), Some("sha256:verify"));
    assert_eq!(denied, vec!["security.ubuntu.com".to_string()]);
    let rows: Vec<MediatorRow> = denied
        .iter()
        .map(|_| MediatorRow {
            phase: "verify".to_string(),
            decision: "deny".to_string(),
        })
        .collect();
    let files = verdict::RewardFileSet {
        reward_txt: Some(b"0".to_vec()),
        ..Default::default()
    };
    let v = verdict::classify(&files, &rows, false);
    assert_eq!(
        v.classification,
        "verifier_unavailable:bootstrap:egress_denied"
    );
}
