//! The Wc6 H-allow repair leg (runner side): a verifier whose bootstrap
//! dies before its pytest leg writes no `/logs/verifier` file of its own
//! — its only evidence is the exec's captured stdout/stderr (apt `E:`/
//! `Err:` lines and `curl: (N)` diagnostics land on stderr). The drive
//! used to null-sink stderr and drop both streams, so a `reward.txt=0`
//! with no `ctrf.json` classified `decided{0}` + `infra_suspected` and
//! the packet rolled up `allowlist_gap{unknown}` — a real `reward=0` with
//! no recorded cause. The repaired drive records `test-stdout.txt`/
//! `test-stderr.txt` beside the reward bytes and feeds both to the §3.4
//! signature rule.

use std::collections::BTreeMap;
use std::path::Path;

use hharness_runner::cell::{run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan};
use hharness_runner::verdict::{self, MediatorRow};

/// A recording cell: no in-cell files ever exist (the frozen `test.sh`
/// writes only `reward.*`/`ctrf.json`, and here it wrote neither — a
/// bootstrap that never reached pytest); the verifier exec returns a
/// decided `reward.txt` via cp_out plus real stream bytes.
struct StreamsCell {
    copies: BTreeMap<String, Vec<u8>>,
}

impl StreamsCell {
    fn new() -> Self {
        let mut copies = BTreeMap::new();
        // `echo 0 > /logs/verifier/reward.txt` — the only file the dead
        // bootstrap still lands.
        copies.insert("/logs/verifier/reward.txt".to_string(), b"0\n".to_vec());
        StreamsCell { copies }
    }
}

impl CellOps for StreamsCell {
    fn present(&mut self, p: &str) -> Result<bool, CellError> {
        Ok(self.copies.contains_key(p))
    }
    fn ls_dir(&mut self, _p: &str) -> Result<Vec<String>, CellError> {
        Ok(vec![])
    }
    fn cp_in(&mut self, _s: &Path, _d: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn cp_out(&mut self, s: &str, d: &Path) -> Result<(), CellError> {
        match self.copies.get(s) {
            Some(b) => std::fs::write(d, b).map_err(|e| CellError {
                op: "cp_out",
                detail: e.to_string(),
            }),
            None => Err(CellError {
                op: "cp_out",
                detail: format!("no file {s}"),
            }),
        }
    }
    fn rm_glob(&mut self, _g: &str) -> Result<(), CellError> {
        Ok(())
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        if spec.argv == ["bash", "/tests/test.sh"] {
            // The dead bootstrap verbatim: apt failed through the
            // mediated proxy (the duplicate-Host 400s), uv never
            // installed, `uvx` never ran — `echo 0` still landed.
            return Ok(CellOutcome {
                exit_code: Some(0),
                timed_out: false,
                stdout: b"Reading package lists...\n/test.sh: line 19: uvx: command not found\n"
                    .to_vec(),
                stderr: b"E: Failed to fetch http://archive.ubuntu.com/ubuntu/pool/main/c/curl/curl_8.5.0_amd64.deb  400  Bad Request [IP: 127.0.0.1 3128]\n".to_vec(),
            });
        }
        Ok(CellOutcome {
            exit_code: Some(0),
            timed_out: false,
            stdout: vec![],
            stderr: vec![],
        })
    }
    fn terminate_group(&mut self, _p: &str) -> Result<(), CellError> {
        Ok(())
    }
}

#[test]
fn verify_streams_land_in_collect_and_classify_bootstrap() {
    let dir = std::env::temp_dir().join(format!("hh-hallow-cell-{}", std::process::id()));
    let staging = dir.join("staging");
    std::fs::create_dir_all(staging.join("solution")).unwrap();
    std::fs::create_dir_all(staging.join("tests")).unwrap();
    let collect = dir.join("collect");
    let plan = CellPlan {
        oracle_participant: false,
        workdir: "/app".to_string(),
        host_staging: staging,
        collect_dir: collect.clone(),
        agent_argv: vec!["bash".into(), "/solution/solve.sh".into()],
        agent_deadline_ms: None,
        agent_deadline_source: hharness_runner::cell::DeadlineSource::CfgDefault,
        verifier_argv: vec!["bash".into(), "/tests/test.sh".into()],
        verifier_deadline_ms: None,
        verifier_deadline_source: hharness_runner::cell::DeadlineSource::CfgDefault,
        max_grade_attempts: 1,
        logs_copyout: None,
    };
    let mut cell = StreamsCell::new();
    let report = run_cell(&mut cell, &plan).unwrap();
    let files = report.verify.unwrap().files;
    // The exec streams are the recorded evidence: stdout filled the
    // absent in-cell member; stderr rides beside it.
    let stdout = files.test_stdout.clone().unwrap();
    let stderr = files.test_stderr.clone().unwrap();
    assert!(String::from_utf8_lossy(&stdout).contains("uvx: command not found"));
    assert!(String::from_utf8_lossy(&stderr).contains("E: Failed to fetch"));
    assert!(collect.join("test-stdout.txt").exists());
    assert!(collect.join("test-stderr.txt").exists());
    // And the §3.4 signature rule now sees the real bootstrap evidence:
    // a decided 0 with no ctrf names the bootstrap, not `{unknown}`.
    let v = verdict::classify(
        &files,
        &[MediatorRow {
            phase: "verify".into(),
            decision: "deny".into(),
        }],
        false,
    );
    assert_eq!(
        v.classification,
        "verifier_unavailable:bootstrap:egress_denied"
    );
    let v2 = verdict::classify(&files, &[], false);
    assert_eq!(
        v2.classification,
        "verifier_unavailable:bootstrap:signature"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
