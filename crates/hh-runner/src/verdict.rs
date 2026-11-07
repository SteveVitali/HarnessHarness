//! The verdict channels (R6.9; VERDICT; REQ-XB-109…112): the TB 2.1
//! `harbor_reward_file/1` channel — an `OracleExecutor` through
//! `OracleRunner` with `shared` isolation (`tamper: n/a{no_detector}`),
//! reusing `grade`/`parse_typed_reward` unchanged — plus the §3.4
//! bootstrap classifier as a pure function of the recorded bytes and
//! the mediator rows, and the `tb10_pytest_log/1` channel confined to
//! the `terminal-bench-core==0.1.1` pin (the committed ANSI-strip and
//! PytestParser-equivalent repairs, REQ-XA-127).

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::disposition::SlotFlag;

/// The TB 2.1 channel spelling.
pub const HARBOR_CHANNEL: &str = "harbor_reward_file/1";
/// The TB 1.0 channel spelling.
pub const TB10_CHANNEL: &str = "tb10_pytest_log/1";
/// The only dataset pin `tb10_pytest_log/1` serves (REQ-XB-112).
pub const TB10_PIN: &str = "terminal-bench-core==0.1.1";
/// The frozen same-cell regrade bound (proposed 2 — REQ-XB-111).
pub const MAX_GRADE_ATTEMPTS: u32 = 2;

/// `RewardFileSet` — the verifier-side bytes copied out of the cell
/// (absent members are absent files, never empty placeholders).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RewardFileSet {
    /// `/logs/verifier/reward.json` bytes.
    pub reward_json: Option<Vec<u8>>,
    /// `/logs/verifier/reward.txt` bytes.
    pub reward_txt: Option<Vec<u8>>,
    /// `/logs/verifier/ctrf.json` bytes.
    pub ctrf_json: Option<Vec<u8>>,
    /// `/logs/verifier/test-stdout.txt` bytes.
    pub test_stdout: Option<Vec<u8>>,
}

impl RewardFileSet {
    /// The verbatim file digests for the record's `verdict.files`.
    pub fn files_map(&self) -> BTreeMap<String, Json> {
        let mut m = BTreeMap::new();
        for (name, bytes) in [
            ("reward.json", &self.reward_json),
            ("reward.txt", &self.reward_txt),
            ("ctrf.json", &self.ctrf_json),
            ("test-stdout.txt", &self.test_stdout),
        ] {
            if let Some(b) = bytes {
                m.insert(
                    name.to_string(),
                    Json::obj([
                        ("sha256", Json::str(hharness_wire::sha256::sha256_hex(b))),
                        ("bytes", Json::Int(b.len() as i64)),
                    ]),
                );
            }
        }
        m
    }
}

/// The `verifier_unavailable` causes the channel's parse rules emit
/// (§3.4 rules 1/2/4 — the typed, never-fabricated spellings).
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelOutcome {
    /// A decided reward (ppm of 1.0).
    Decided(u64),
    /// A typed verifier-unavailable cause.
    Unavailable(String),
}

/// `parse_reward_number(s)` — the §3.4 rule-2 decimal: a trimmed
/// decimal in `[0, 1]` with ≤ 6 fractional digits, mapped exactly to
/// ppm. Anything else is `None` (`reward_unparseable`).
fn parse_reward_number(s: &str) -> Option<u64> {
    let t = s.trim();
    let (int, frac) = match t.split_once('.') {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) || int.len() > 1 {
        return None;
    }
    if int != "0" && int != "1" {
        return None;
    }
    if int == "1" && !frac.is_empty() && !frac.bytes().all(|b| b == b'0') {
        return None; // 1.x is out of range unless frac is all zeros
    }
    if frac.len() > 6 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac_ppm: u64 = frac.bytes().fold(0u64, |a, b| a * 10 + (b - b'0') as u64)
        * 10u64.pow(6 - frac.len() as u32);
    if int == "1" {
        return if frac_ppm == 0 { Some(1_000_000) } else { None };
    }
    Some(frac_ppm)
}

/// `parse_reward_files(files)` — §3.4 rules 1–2: `reward.json`
/// preferred over `reward.txt`; the json must be an object with
/// exactly one numeric `reward` member; the txt must be the trimmed
/// decimal; an empty/absent file is `reward_missing`/`reward_unparseable`.
pub fn parse_reward_files(files: &RewardFileSet) -> ChannelOutcome {
    if let Some(bytes) = &files.reward_json {
        let text = String::from_utf8_lossy(bytes);
        match hharness_wire::json::parse(&text) {
            Ok(Json::Obj(m)) => {
                if m.len() != 1 || !m.contains_key("reward") {
                    return ChannelOutcome::Unavailable("reward_json_multi_key".to_string());
                }
                return match m.get("reward") {
                    Some(Json::Int(r)) if (0..=1_000_000).contains(r) => {
                        ChannelOutcome::Decided(*r as u64)
                    }
                    Some(Json::Int(_)) => {
                        ChannelOutcome::Unavailable("reward_unparseable".to_string())
                    }
                    // A numeric float member (Harbor writes `0.5` for
                    // partial rewards) — the canonical codec has no
                    // floats, so this arm is unreachable here; a
                    // float-bearing file fails canonical parse below.
                    _ => ChannelOutcome::Unavailable("reward_unparseable".to_string()),
                };
            }
            _ => return ChannelOutcome::Unavailable("reward_unparseable".to_string()),
        }
    }
    match &files.reward_txt {
        None => ChannelOutcome::Unavailable("reward_missing".to_string()),
        Some(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            if text.trim().is_empty() {
                return ChannelOutcome::Unavailable("reward_unparseable".to_string());
            }
            match parse_reward_number(&text) {
                Some(ppm) => ChannelOutcome::Decided(ppm),
                None => ChannelOutcome::Unavailable("reward_unparseable".to_string()),
            }
        }
    }
}

/// A mediator row in the declared fixture shape (the mediator lands in
/// R6.15; the classifier consumes rows, never the mediator itself).
#[derive(Debug, Clone, PartialEq)]
pub struct MediatorRow {
    /// The phase the row was recorded under (`verify` for the ST5 leg).
    pub phase: String,
    /// `allow | deny | error` — the recorded decision.
    pub decision: String,
}

/// The bootstrap signatures (§3.4/F7 — apt `E:` lines, `Temporary
/// failure resolving`, curl non-zero, uv fetch error). The list is
/// heuristic until the re-proof records real bootstrap output (F7).
pub const BOOTSTRAP_SIGNATURES: &[&str] = &[
    "Temporary failure resolving",
    "curl: (",
    "Failed to fetch",
    "error: Failed to download",
    "Failed to download",
];

/// `has_bootstrap_signature(stdout)` — a pre-pytest bootstrap failure
/// signature in `test-stdout.txt`: an apt `E:` line (leading, possibly
/// indented) or any declared needle.
pub fn has_bootstrap_signature(stdout: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stdout);
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("E: ") {
            return true;
        }
        for needle in BOOTSTRAP_SIGNATURES {
            if line.contains(needle) {
                return true;
            }
        }
    }
    false
}

/// `ctrf` verdict facts — `Some` only when `ctrf.json` parses and
/// names collected tests.
#[derive(Debug, Clone, PartialEq)]
pub struct CtrfFacts {
    /// Collected tests (`results.summary.tests`, or the `tests` array
    /// length when the summary is absent).
    pub tests: u64,
    /// Failures (`summary.failed`, or failed entries in `tests`).
    pub failed: u64,
}

/// `ctrf_facts(bytes)` — a `ctrf.json` with ≥ 1 collected test;
/// `None` for absent/unparseable/empty reports (the bootstrap rule's
/// "no parseable ctrf" leg).
pub fn ctrf_facts(bytes: Option<&[u8]>) -> Option<CtrfFacts> {
    let bytes = bytes?;
    let text = String::from_utf8_lossy(bytes);
    let j = hharness_wire::json::parse(&text).ok()?;
    let results = j.get("results")?;
    let (mut tests, mut failed) = (0i64, 0i64);
    if let Some(s) = results.get("summary") {
        tests = s.get("tests").and_then(Json::as_int).unwrap_or(0);
        failed = s.get("failed").and_then(Json::as_int).unwrap_or(0);
    }
    if tests == 0 {
        if let Some(Json::Arr(t)) = results.get("tests") {
            tests = t.len() as i64;
            failed = t
                .iter()
                .filter(|e| {
                    e.get("status")
                        .and_then(Json::as_str)
                        .map(|s| s == "failed")
                        .unwrap_or(false)
                })
                .count() as i64;
        }
    }
    if tests <= 0 {
        return None;
    }
    Some(CtrfFacts {
        tests: tests as u64,
        failed: failed.max(0) as u64,
    })
}

/// `ChannelVerdict` — the channel's §3.4 classification (the slot
/// record's `verdict` members).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelVerdict {
    /// The reward ppm when decided.
    pub reward_ppm: Option<u64>,
    /// `success` per the TB 2.1 default `reward_ppm > 0`.
    pub success: Option<bool>,
    /// The `verdict.classification` spelling — `decided` or the
    /// `verifier_unavailable:<cause>` tail.
    pub classification: String,
    /// The verdict flags (closed set).
    pub flags: Vec<SlotFlag>,
}

/// `classify(files, mediator_rows, verifier_timed_out)` — the §3.4
/// bootstrap classifier, a pure function of the recorded bytes and the
/// mediator rows (REQ-XB-110):
///
/// 1. a verifier deadline lapse ⇒ `verifier_unavailable:verifier_timeout`;
/// 2. parse rules 1–2 ⇒ a decided ppm or `verifier_unavailable:<parse>`;
/// 3. `ctrf.json` with collected tests ⇒ scored on the reward (a `reward
///    = 1` with ctrf failures flags `reward_ctrf_inconsistent`);
/// 4. `reward_ppm == 0` with no parseable ctrf ⇒
///    `verifier_unavailable:bootstrap:<egress_denied|upstream_error|
///    signature>` when the verify-phase rows or stdout carry evidence;
///    scored 0 flagged `infra_suspected:verifier_no_report` otherwise.
pub fn classify(
    files: &RewardFileSet,
    mediator_rows: &[MediatorRow],
    verifier_timed_out: bool,
) -> ChannelVerdict {
    let unavailable = |cause: &str| ChannelVerdict {
        reward_ppm: None,
        success: None,
        classification: format!("verifier_unavailable:{cause}"),
        flags: vec![],
    };
    if verifier_timed_out {
        return unavailable("verifier_timeout");
    }
    let ppm = match parse_reward_files(files) {
        ChannelOutcome::Decided(p) => p,
        ChannelOutcome::Unavailable(c) => return unavailable(&c),
    };
    let ctrf = ctrf_facts(files.ctrf_json.as_deref());
    let decided = |flags: Vec<SlotFlag>| ChannelVerdict {
        reward_ppm: Some(ppm),
        success: Some(ppm > 0),
        classification: "decided".to_string(),
        flags,
    };
    if let Some(c) = ctrf {
        if ppm >= 1_000_000 && c.failed > 0 {
            return decided(vec![SlotFlag::RewardCtrfInconsistent]);
        }
        return decided(vec![]);
    }
    if ppm == 0 {
        let verify = |d: &str| {
            mediator_rows
                .iter()
                .any(|r| r.phase == "verify" && r.decision == d)
        };
        if verify("deny") {
            return unavailable("bootstrap:egress_denied");
        }
        if verify("error") {
            return unavailable("bootstrap:upstream_error");
        }
        if files
            .test_stdout
            .as_deref()
            .map(has_bootstrap_signature)
            .unwrap_or(false)
        {
            return unavailable("bootstrap:signature");
        }
        return decided(vec![SlotFlag::InfraSuspectedVerifierNoReport]);
    }
    decided(vec![])
}

/// `HarborRewardExecutor` — `harbor_reward_file/1` as an
/// `OracleExecutor` (REQ-XB-109): `execute` reports the channel's
/// observation — decided rewards land as the canonical
/// `{"reward": <ppm>}` document `parse_typed_reward` reads unchanged;
/// unavailable causes land as the typed failure observation; the raw
/// bytes stay verbatim in `files` for the attempt record.
pub struct HarborRewardExecutor {
    /// The frozen oracle digest (content-pinned bytes).
    pub digest: String,
    /// The collected reward-side bytes.
    pub files: RewardFileSet,
    /// The verify-phase mediator rows (fixture-declared until R6.15).
    pub mediator_rows: Vec<MediatorRow>,
    /// Whether the verifier exec lapsed its deadline.
    pub verifier_timed_out: bool,
}

impl hharness_bench::oracle::OracleExecutor for HarborRewardExecutor {
    fn oracle_digest(&self) -> &str {
        &self.digest
    }

    fn execute(
        &self,
        _task: &hharness_lab::bench::TaskRecord,
        _submission: &hharness_bench::records::Submission,
    ) -> hharness_bench::oracle::OracleObservation {
        let verdict = classify(&self.files, &self.mediator_rows, self.verifier_timed_out);
        match verdict.reward_ppm {
            Some(ppm) => hharness_bench::oracle::OracleObservation {
                exit_code: Some(0),
                timed_out: false,
                stdout: Json::obj([("reward", Json::Int(ppm as i64))]).to_canonical_string(),
                stderr: String::new(),
                environment_error: None,
                wall_ms: 0,
            },
            None => hharness_bench::oracle::OracleObservation {
                exit_code: None,
                timed_out: self.verifier_timed_out,
                stdout: String::new(),
                stderr: format!("{}:{}", HARBOR_CHANNEL, verdict.classification),
                environment_error: Some(verdict.classification),
                wall_ms: 0,
            },
        }
    }
}

/// `select_channel(dataset_pin)` — the pin-confined channel choice
/// (REQ-XB-112): the TB 1.0 pin admits `tb10_pytest_log/1`; a TB 2.1
/// pin admits `harbor_reward_file/1`; any other combination refuses
/// typed — a TB 2.1 slot never selects the TB 1.0 channel.
pub fn select_channel(dataset_pin: &str) -> Result<&'static str, String> {
    if dataset_pin == TB10_PIN {
        return Ok(TB10_CHANNEL);
    }
    if dataset_pin.starts_with("terminal-bench-2-1@")
        || dataset_pin == "harbor-framework/terminal-bench-2-1@12b75656"
    {
        return Ok(HARBOR_CHANNEL);
    }
    Err(format!(
        "no verdict channel for dataset pin {dataset_pin} — \
         tb10_pytest_log/1 serves {TB10_PIN} only and refuses TB 2.1"
    ))
}

// ── tb10_pytest_log/1 — the committed scratch repairs (REQ-XA-127) ────

/// `strip_ansi(s)` — the committed form of the `s4-orch.py` repair set
/// (verbatim semantics): OSC (`ESC ] … BEL|ESC\`), CSI (`ESC [` params
/// intermediates final), the MISC set (`ESC (|) [0-2]`, `ESC [=>#]
/// [0-9]?`, `ESC [@-Z\\-_]`, `ESC [\^_] … BEL|ESC\`), remaining `ESC`
/// dropped, then each line keeps only its post-`\r` segment.
pub fn strip_ansi(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '\u{1b}' && i + 1 < b.len() {
            let c = b[i + 1];
            if c == ']' {
                // OSC: ESC ] [^BEL ESC]* (BEL | ESC \)
                let mut j = i + 2;
                while j < b.len() && b[j] != '\u{7}' && b[j] != '\u{1b}' {
                    j += 1;
                }
                if j < b.len() && b[j] == '\u{7}' {
                    i = j + 1;
                } else if j + 1 < b.len() && b[j] == '\u{1b}' && b[j + 1] == '\\' {
                    i = j + 2;
                } else {
                    i = j; // unterminated — the ESC-drop pass below eats the ESCs
                }
                continue;
            }
            if c == '[' {
                // CSI: ESC [ [0-?]* [ -/]* [@-~]
                let mut j = i + 2;
                while j < b.len() && ('0'..='?').contains(&b[j]) {
                    j += 1;
                }
                while j < b.len() && (' '..='/').contains(&b[j]) {
                    j += 1;
                }
                if j < b.len() && ('@'..='~').contains(&b[j]) {
                    i = j + 1;
                    continue;
                }
                i = j;
                continue;
            }
            if (c == '(' || c == ')') && i + 2 < b.len() && ('0'..='2').contains(&b[i + 2]) {
                i += 3;
                continue;
            }
            if matches!(c, '=' | '>' | '#') {
                i += if i + 2 < b.len() && b[i + 2].is_ascii_digit() {
                    3
                } else {
                    2
                };
                continue;
            }
            if ('@'..='Z').contains(&c) || ('\\'..='_').contains(&c) {
                i += 2;
                continue;
            }
            if c == '^' || c == '_' {
                let mut j = i + 2;
                while j < b.len() && b[j] != '\u{7}' && b[j] != '\u{1b}' {
                    j += 1;
                }
                if j < b.len() && b[j] == '\u{7}' {
                    i = j + 1;
                } else if j + 1 < b.len() && b[j] == '\u{1b}' && b[j + 1] == '\\' {
                    i = j + 2;
                } else {
                    i = j;
                }
                continue;
            }
        }
        if b[i] != '\u{1b}' {
            out.push(b[i]);
        }
        i += 1;
    }
    // The carriage-return leg: each line keeps its post-\r segment
    // (progress redraws overwrite, never concatenate).
    out.lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `parse_pytest_summary(content) → {test_name → passed|failed}` — a
/// short-summary parse equivalent to tb's `PytestParser` (terminal-
/// bench-core 0.1.1 `parsers/pytest_parser.py`): split on the
/// `=+ short test summary info =+` banner (case-insensitive), then per
/// line `<STATUS> <path>` where FAILED lines drop their ` - reason`
/// tail, the status must be a member, and the test name is the path's
/// last `::` segment. PASSED/XFAIL/SKIPPED map `passed`;
/// FAILED/XPASS/ERROR map `failed`. No banner is a typed error.
pub fn parse_pytest_summary(content: &str) -> Result<BTreeMap<String, String>, String> {
    let mut summary: Option<&str> = None;
    for (idx, line) in content.split_inclusive('\n').enumerate() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("short test summary info")
            && line
                .trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                .starts_with(|_: char| true)
            && line.starts_with(|c: char| c == '=')
        {
            let _ = idx;
            let pos: usize = content
                .split_inclusive('\n')
                .take(idx + 1)
                .map(|l| l.len())
                .sum();
            summary = Some(&content[pos..]);
        }
    }
    let summary = summary
        .ok_or_else(|| "No short test summary info found in the provided content.".to_string())?;
    const MEMBERS: &[(&str, &str)] = &[
        ("PASSED", "passed"),
        ("XFAIL", "passed"),
        ("SKIPPED", "passed"),
        ("FAILED", "failed"),
        ("XPASS", "failed"),
        ("ERROR", "failed"),
    ];
    let mut out = BTreeMap::new();
    for line in summary.lines() {
        let mut line = line;
        if line.starts_with("FAILED") {
            // drop the ` - reason` tail (split on " - ", rejoin all but last)
            if let Some(pos) = line.rfind(" - ") {
                line = &line[..pos];
            }
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let status = parts.next().unwrap_or("").trim().trim_matches(':');
        let Some(rest) = parts.next() else { continue };
        let Some((_, mapped)) = MEMBERS.iter().find(|(m, _)| *m == status) else {
            continue;
        };
        let name = rest.trim().split("::").last().unwrap_or("").trim();
        if !name.is_empty() {
            out.insert(name.to_string(), (*mapped).to_string());
        }
    }
    Ok(out)
}
