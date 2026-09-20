//! `validate_bundle` / `check_completeness` — the staged validation pass
//! (§5h.3 §2/§4; AC-R-2.9.3-3). Never fail-fast: every stage runs, every
//! check reports its own row, and `complete` is the S1∧S2∧S4∧S7 fold
//! (`check_completeness` is exactly that subset — same engine, filtered).
//! The report itself is content-addressed (`digest` = `idp/1` over the
//! report minus `digest`).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::error::BundleError;
use crate::export::{decode_page, MemberBytes};
use crate::levels::{self, roles};
use crate::manifest::{BundleManifest, LedgerExport, MemberStatus, ReproLevel};

/// A check's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// The check holds.
    Pass,
    /// The check fails — the stage reports `fail` and `complete` folds it.
    Fail,
    /// The member the check names is listed but not materialized
    /// (`status = fetch`, a `manifest_only`/`detached` bundle) — reported,
    /// never a failure.
    Unmaterialized,
    /// A warning — reported, never a failure (`instrument_dirty`).
    Warn,
}

impl CheckStatus {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Fail => "fail",
            CheckStatus::Unmaterialized => "unmaterialized",
            CheckStatus::Warn => "warn",
        }
    }
}

/// One check row — `{check, status, code?, detail?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckRow {
    /// The check name.
    pub check: String,
    /// The verdict.
    pub status: CheckStatus,
    /// The spec's failure code (`hash_mismatch`, `missing_member`, …).
    pub code: Option<String>,
    /// Human-facing detail.
    pub detail: Option<String>,
}

impl CheckRow {
    fn pass(check: impl Into<String>) -> CheckRow {
        CheckRow {
            check: check.into(),
            status: CheckStatus::Pass,
            code: None,
            detail: None,
        }
    }
    fn fail(check: impl Into<String>, code: &str, detail: impl Into<String>) -> CheckRow {
        CheckRow {
            check: check.into(),
            status: CheckStatus::Fail,
            code: Some(code.to_string()),
            detail: Some(detail.into()),
        }
    }
    fn unmaterialized(check: impl Into<String>, detail: impl Into<String>) -> CheckRow {
        CheckRow {
            check: check.into(),
            status: CheckStatus::Unmaterialized,
            code: None,
            detail: Some(detail.into()),
        }
    }
    fn warn(check: impl Into<String>, code: &str, detail: impl Into<String>) -> CheckRow {
        CheckRow {
            check: check.into(),
            status: CheckStatus::Warn,
            code: Some(code.to_string()),
            detail: Some(detail.into()),
        }
    }
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("check".into(), Json::str(self.check.clone()));
        m.insert("status".into(), Json::str(self.status.name()));
        if let Some(c) = &self.code {
            m.insert("code".into(), Json::str(c.clone()));
        }
        if let Some(d) = &self.detail {
            m.insert("detail".into(), Json::str(d.clone()));
        }
        Json::Obj(m)
    }
}

/// One stage — `{stage, name, checks[]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct StageReport {
    /// S1..S9 (S9 is publication-time; not produced by `validate`).
    pub stage: u8,
    /// The stage name (`completeness`, `present_members_verify`, …).
    pub name: &'static str,
    /// The check rows — every check runs (no fail-fast).
    pub checks: Vec<CheckRow>,
}

impl StageReport {
    /// Whether the stage has no `fail` rows.
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("stage", Json::Int(self.stage as i64)),
            ("name", Json::str(self.name)),
            (
                "checks",
                Json::Arr(self.checks.iter().map(|c| c.to_json()).collect()),
            ),
        ])
    }
}

/// `BundleValidationReport` — `{schema, bundle_id, bundle_kind,
/// participant_class, max_supported_level, complete, complete_except[],
/// stages[{stage, checks[]}], findings[], digest}`.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleValidationReport {
    /// `version_id` of the manifest under validation.
    pub bundle_id: String,
    /// `run` at this stage.
    pub bundle_kind: String,
    /// `native | hosted`.
    pub participant_class: String,
    /// The declared (or derived) max level.
    pub max_supported_level: String,
    /// S1∧S2∧S4∧S7.
    pub complete: bool,
    /// The stage names among S1/S2/S4/S7 that failed.
    pub complete_except: Vec<String>,
    /// The per-stage rows.
    pub stages: Vec<StageReport>,
    /// Non-fatal diagnostics (`instrument_dirty`, basis detail, …).
    pub findings: Vec<Json>,
    /// `idp/1` over the report minus this member.
    pub digest: String,
}

/// The report's schema id.
pub const REPORT_SCHEMA: &str = "hh-bundle-validation/1";

impl BundleValidationReport {
    /// Canonical JSON (digest stamped).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(REPORT_SCHEMA));
        m.insert("bundle_id".into(), Json::str(self.bundle_id.clone()));
        m.insert("bundle_kind".into(), Json::str(self.bundle_kind.clone()));
        m.insert(
            "participant_class".into(),
            Json::str(self.participant_class.clone()),
        );
        m.insert(
            "max_supported_level".into(),
            Json::str(self.max_supported_level.clone()),
        );
        m.insert("complete".into(), Json::Bool(self.complete));
        m.insert(
            "complete_except".into(),
            Json::Arr(
                self.complete_except
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        );
        m.insert(
            "stages".into(),
            Json::Arr(self.stages.iter().map(|s| s.to_json()).collect()),
        );
        m.insert("findings".into(), Json::Arr(self.findings.clone()));
        m.insert("digest".into(), Json::str(self.digest.clone()));
        Json::Obj(m)
    }
    /// Stamp `digest` = `idp/1` over `canonical(report − digest)`.
    pub fn seal(&mut self) {
        self.digest = String::new();
        let mut doc = match self.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        doc.remove("digest");
        self.digest = hh_identity::idp_id(
            "bundle.validation",
            Json::Obj(doc).to_canonical_string().as_bytes(),
        );
    }
}

/// A credentialed locator check (R-ID-7): a URI with a userinfo component
/// or a credential-ish query parameter is never a legal `fetch` location.
fn credentialed(locator: &str) -> bool {
    let lower = locator.to_lowercase();
    if let Some(rest) = lower.split("://").nth(1) {
        let authority = rest.split('/').next().unwrap_or("");
        if authority.contains('@') {
            return true;
        }
    }
    for key in ["token=", "key=", "sig=", "signature=", "password=", "secret="] {
        if lower.contains(key) {
            return true;
        }
    }
    false
}

/// Secret-scan a byte payload (tombstone-bearing hits only — a
/// `placeholder_passthrough` mark is the safe form).
fn has_secret(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let detectors = hh_secrets::DetectorSet::standard(hh_secrets::MaskSet::default());
    hh_secrets::detect(text, &detectors)
        .iter()
        .find(|h| h.tombstone.is_some())
        .map(|h| h.detector.as_str().to_string())
}

/// The member addresses a manifest *requires* (S1's required set).
fn required_roles() -> &'static [&'static str] {
    &[
        roles::SUBJECT,
        roles::DEFINITION,
        roles::INSTRUMENT,
        roles::MODEL,
        roles::CONFIGURATION,
        roles::RESOLVED_DEPENDENCIES,
        roles::RESULTS,
        roles::ENVIRONMENT,
        roles::NONDETERMINISM,
    ]
}

/// Decode every `LedgerExport` page the manifest names — returns the
/// `(run → envelopes)` map plus a replay flag, or the first decode error
/// as a check detail.
fn decode_traces(
    manifest: &BundleManifest,
    members: &MemberBytes,
    checks: &mut Vec<CheckRow>,
) -> (BTreeMap<String, Vec<hh_ledger::event::EventEnvelope>>, bool) {
    let mut out: BTreeMap<String, Vec<hh_ledger::event::EventEnvelope>> = BTreeMap::new();
    let mut replay = false;
    for (run, export) in &manifest.traces {
        let mut events = Vec::new();
        for page_addr in &export.pages {
            match members.get(page_addr) {
                Some(bytes) => match decode_page(bytes) {
                    Ok(evs) => {
                        replay |= evs.iter().any(|e| e.class == "control.decision");
                        events.extend(evs);
                    }
                    Err(e) => checks.push(CheckRow::fail(
                        format!("traces.{run}.pages"),
                        "page_mismatch",
                        format!("{page_addr}: {e}"),
                    )),
                },
                None => checks.push(CheckRow::unmaterialized(
                    format!("traces.{run}.pages"),
                    format!("{page_addr} not materialized"),
                )),
            }
        }
        out.insert(run.clone(), events);
    }
    (out, replay)
}

/// Recompute a `LedgerExport`'s internal anchors (S2).
fn verify_export(
    export: &LedgerExport,
    events: &[hh_ledger::event::EventEnvelope],
    members: &MemberBytes,
    checks: &mut Vec<CheckRow>,
) {
    let run = &export.run_id;
    // Tree recomputation — the one tree rule.
    let tree = LedgerExport::tree_address(&export.pages);
    if tree != export.tree {
        checks.push(CheckRow::fail(
            format!("traces.{run}.tree"),
            "page_mismatch",
            format!("tree {tree} ≠ declared {}", export.tree),
        ));
    } else {
        checks.push(CheckRow::pass(format!("traces.{run}.tree")));
    }
    // Envelope hash + chain integrity, in materialized order.
    let mut prev_ok = true;
    let mut last: Option<&hh_ledger::event::EventEnvelope> = None;
    for (i, ev) in events.iter().enumerate() {
        if ev.recompute_hash() != ev.hash {
            checks.push(CheckRow::fail(
                format!("traces.{run}.events[{i}]"),
                "hash_mismatch",
                format!("seq {} recomputes to a different hash", ev.seq),
            ));
            prev_ok = false;
        }
        if let Some(prev) = last {
            if ev.seq != prev.seq + 1 || ev.prev_hash != prev.hash {
                checks.push(CheckRow::fail(
                    format!("traces.{run}.events[{i}]"),
                    "page_mismatch",
                    format!("seq {} does not continue seq {}", ev.seq, prev.seq),
                ));
                prev_ok = false;
            }
        }
        last = Some(ev);
    }
    if prev_ok && !events.is_empty() {
        checks.push(CheckRow::pass(format!("traces.{run}.events")));
    }
    // The export's head = the last event's coordinate.
    if let Some(last) = last {
        let head_seq = export.head.get("seq").and_then(Json::as_int).unwrap_or(-1);
        let head_hash = export
            .head
            .get("hash")
            .and_then(Json::as_str)
            .unwrap_or("");
        if last.seq as i64 != head_seq || last.hash != head_hash {
            checks.push(CheckRow::fail(
                format!("traces.{run}.head"),
                "head_mismatch",
                format!(
                    "last event ({},{}) ≠ head ({head_seq},{head_hash})",
                    last.seq, last.hash
                ),
            ));
        } else {
            checks.push(CheckRow::pass(format!("traces.{run}.head")));
        }
    }
    // Blob index: present blobs must materialize or fail — `blob_index`
    // rows are informational when the blob isn't a bundle member.
    for b in &export.blob_index {
        if b.status == "present" && !members.contains_key(&b.address) {
            checks.push(CheckRow::unmaterialized(
                format!("traces.{run}.blob_index"),
                format!("blob {} not carried", b.address),
            ));
        }
    }
}

/// `validate_bundle` — the full S1..S8 pass (S9 is publication-time).
pub fn validate(manifest: &BundleManifest, members: &MemberBytes) -> BundleValidationReport {
    validate_stages(manifest, members, &[1, 2, 3, 4, 5, 6, 7, 8])
}

/// `check_completeness` — the S1/S2/S4/S7 subset (§5h.3 §4 note; AC-3).
pub fn check_completeness(
    manifest: &BundleManifest,
    members: &MemberBytes,
) -> BundleValidationReport {
    validate_stages(manifest, members, &[1, 2, 4, 7])
}

/// The engine — runs the requested stages, folds `complete`.
fn validate_stages(
    manifest: &BundleManifest,
    members: &MemberBytes,
    wanted: &[u8],
) -> BundleValidationReport {
    let mut stages: Vec<StageReport> = Vec::new();
    let mut findings: Vec<Json> = Vec::new();
    let want = |s: u8| wanted.contains(&s);

    // ── S1 completeness ──────────────────────────────────────────────
    let mut s1 = Vec::new();
    for role in required_roles() {
        match manifest.members.iter().find(|m| m.role == *role) {
            Some(_) => s1.push(CheckRow::pass(format!("member:{role}"))),
            None => s1.push(CheckRow::fail(
                format!("member:{role}"),
                "absent_required_member",
                format!("required role {role} is not a member"),
            )),
        }
    }
    // Every subject run needs its ledger tree + at least one page ref.
    for run in &manifest.subject.run_ids {
        match manifest.traces.get(run) {
            Some(t) if !t.pages.is_empty() => {
                s1.push(CheckRow::pass(format!("traces:{run}")))
            }
            _ => s1.push(CheckRow::fail(
                format!("traces:{run}"),
                "absent_required_member",
                format!("subject run {run} has no LedgerExport pages"),
            )),
        }
    }
    // Present members must carry bytes; fetch members are unmaterialized.
    for m in &manifest.members {
        match m.status {
            MemberStatus::Present => {
                if members.contains_key(&m.address) {
                    s1.push(CheckRow::pass(format!("present:{}", m.role)));
                } else {
                    s1.push(CheckRow::fail(
                        format!("present:{}", m.role),
                        "missing_member",
                        format!("{} claims present, carries no bytes", m.address),
                    ));
                }
            }
            MemberStatus::Fetch => s1.push(CheckRow::unmaterialized(
                format!("present:{}", m.role),
                format!("{} is fetch-only", m.address),
            )),
            MemberStatus::Redacted | MemberStatus::Gc => s1.push(CheckRow::unmaterialized(
                format!("present:{}", m.role),
                format!("{} is {}", m.address, m.status.name()),
            )),
        }
    }
    if want(1) {
        stages.push(StageReport {
            stage: 1,
            name: "completeness",
            checks: s1.clone(),
        });
    }

    // ── S2 present_members_verify ────────────────────────────────────
    let mut s2 = Vec::new();
    for m in &manifest.members {
        let Some(bytes) = members.get(&m.address) else {
            continue; // unmaterialized — S1 reported it.
        };
        // Member bytes are minted under the `blob` idp domain
        // (`hh_identity::address` — the media type is a member field,
        // never part of the digest).
        let recomputed = hh_identity::idp_id("blob", bytes);
        if recomputed != m.address {
            s2.push(CheckRow::fail(
                format!("member:{}", m.address),
                "hash_mismatch",
                format!("{} bytes do not recompute to the member ref", m.role),
            ));
            continue;
        }
        if m.size != 0 && bytes.len() as u64 != m.size {
            s2.push(CheckRow::fail(
                format!("member:{}", m.address),
                "bad_size",
                format!("{} bytes ≠ declared {}", bytes.len(), m.size),
            ));
        } else {
            s2.push(CheckRow::pass(format!("member:{}", m.role)));
        }
    }
    // The manifest's own identity — the tree rule over the preimage.
    let recomputed_id = manifest.compute_id();
    if manifest.version_id.is_empty() || recomputed_id != manifest.version_id {
        s2.push(CheckRow::fail(
            "manifest.version_id",
            "version_id_mismatch",
            format!("recomputed {recomputed_id} ≠ declared {}", manifest.version_id),
        ));
    } else {
        s2.push(CheckRow::pass("manifest.version_id"));
    }
    // LedgerExport internals (needs decoded pages — shared with S4).
    let (events_by_run, replay_declared) = decode_traces(manifest, members, &mut s2);
    for export in manifest.traces.values() {
        verify_export(
            export,
            events_by_run.get(&export.run_id).cloned().unwrap_or_default().as_slice(),
            members,
            &mut s2,
        );
    }
    if want(2) {
        stages.push(StageReport {
            stage: 2,
            name: "present_members_verify",
            checks: s2.clone(),
        });
    }

    // ── S3 completeness_closure ──────────────────────────────────────
    let mut s3 = Vec::new();
    let member_addrs: std::collections::BTreeSet<&str> = manifest
        .members
        .iter()
        .map(|m| m.address.as_str())
        .collect();
    for r in levels::section_member_refs(manifest) {
        if member_addrs.contains(r.as_str()) || r == manifest.version_id {
            s3.push(CheckRow::pass(format!("ref:{r}")));
        } else {
            s3.push(CheckRow::fail(
                format!("ref:{r}"),
                "missing_member",
                "manifest reference resolves to no member",
            ));
        }
    }
    // A fetch-status member needs a fetch[] entry; a present member must
    // not have one (that would lie about materialization).
    for m in &manifest.members {
        let has_fetch = manifest.fetch.iter().any(|f| f.address == m.address);
        if m.status == MemberStatus::Fetch && !has_fetch {
            s3.push(CheckRow::fail(
                format!("fetch:{}", m.address),
                "missing_member",
                "fetch-status member has no fetch[] entry",
            ));
        }
    }
    if want(3) {
        stages.push(StageReport {
            stage: 3,
            name: "completeness_closure",
            checks: s3,
        });
    }

    // ── S4 level_coherence ───────────────────────────────────────────
    let mut s4 = Vec::new();
    let (derived, basis) = levels::derive(manifest, replay_declared);
    match levels::declared_max(manifest) {
        Some(declared) if declared > derived => s4.push(CheckRow::fail(
            "max_supported_level",
            "unsupported_level",
            format!("declared {} > derived {}", declared.name(), derived.name()),
        )),
        Some(declared) => s4.push(CheckRow::pass(format!(
            "max_supported_level:{}",
            declared.name()
        ))),
        None => s4.push(CheckRow::fail(
            "max_supported_level",
            "unsupported_level",
            "reproducibility.max_supported_level absent",
        )),
    }
    if let (Some(claimed), Some(max)) =
        (levels::declared_claimed(manifest), levels::declared_max(manifest))
    {
        if claimed > max {
            s4.push(CheckRow::fail(
                "claimed_level",
                "unsupported_level",
                format!("claimed {} > max_supported {}", claimed.name(), max.name()),
            ));
        }
    }
    // Declared basis rows must name resolvable members.
    for (id, sat) in levels::declared_basis(manifest) {
        if let Some(a) = sat.get("member").and_then(Json::as_str) {
            if !member_addrs.contains(a) && a != manifest.version_id.as_str() {
                s4.push(CheckRow::fail(
                    format!("basis:{id}"),
                    "manifest_reference_unresolved",
                    format!("satisfied_by member {a} is not a bundle member"),
                ));
            }
        }
    }
    // Report the derived basis as findings (the contract is the report,
    // not the fail-fast).
    findings.push(Json::obj([
        ("kind", Json::str("derived_basis")),
        ("max_supported_level", Json::str(derived.name())),
        (
            "basis",
            Json::Arr(basis.iter().map(|b| b.to_json()).collect()),
        ),
    ]));
    if want(4) {
        stages.push(StageReport {
            stage: 4,
            name: "level_coherence",
            checks: s4,
        });
    }

    // ── S5 dirty_unknown ─────────────────────────────────────────────
    if want(5) {
        let mut s5 = Vec::new();
        match manifest.instrument.get("dirty") {
            Some(Json::Bool(true)) => {
                s5.push(CheckRow::warn(
                    "instrument.dirty",
                    "instrument_dirty",
                    "the producing workspace was dirty — level capped at R0",
                ));
                findings.push(Json::obj([
                    ("kind", Json::str("instrument_dirty")),
                    ("detail", Json::str("dirty ⇒ R0")),
                ]));
            }
            Some(Json::Bool(false)) => s5.push(CheckRow::pass("instrument.dirty")),
            _ => s5.push(CheckRow::fail(
                "instrument.dirty",
                "dirty_version_present",
                "instrument.dirty is absent or non-boolean — never unknown",
            )),
        }
        stages.push(StageReport {
            stage: 5,
            name: "dirty_unknown",
            checks: s5,
        });
    }

    // ── S6 lineage_integrity ─────────────────────────────────────────
    if want(6) {
        let mut s6 = Vec::new();
        // The lineage chain must end at the subject run at its head.
        let subject_ok = manifest
            .subject
            .lineage
            .last()
            .and_then(|l| {
                let run = l.get("run_id").and_then(Json::as_str)?;
                let seq = l.get("up_to_seq").and_then(Json::as_int)?;
                let hash = l.get("head_hash").and_then(Json::as_str)?;
                let head = manifest.subject.heads.get(run)?;
                let ok = manifest.subject.run_ids.contains(&run.to_string())
                    && head.get("seq").and_then(Json::as_int) == Some(seq)
                    && head.get("hash").and_then(Json::as_str) == Some(hash);
                Some(ok)
            })
            .unwrap_or(false);
        if subject_ok {
            s6.push(CheckRow::pass("subject.lineage"));
        } else {
            s6.push(CheckRow::fail(
                "subject.lineage",
                "lineage_hash_mismatch",
                "lineage does not terminate at the subject head",
            ));
        }
        // Export heads agree with subject heads.
        for (run, export) in &manifest.traces {
            let sh = manifest.subject.heads.get(run);
            match sh {
                Some(h)
                    if h.get("hash") == export.head.get("hash")
                        && h.get("seq") == export.head.get("seq") =>
                {
                    s6.push(CheckRow::pass(format!("traces.{run}.head")));
                }
                _ => s6.push(CheckRow::fail(
                    format!("traces.{run}.head"),
                    "head_mismatch",
                    "LedgerExport.head ≠ subject.heads",
                )),
            }
        }
        stages.push(StageReport {
            stage: 6,
            name: "lineage_integrity",
            checks: s6,
        });
    }

    // ── S7 hosting_boundary ──────────────────────────────────────────
    if want(7) {
        let mut s7 = Vec::new();
        if manifest.participant_class == "hosted" {
            for c in &manifest.claims {
                let auth = c
                    .provenance
                    .get("authority")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if auth == "principal" {
                    s7.push(CheckRow::pass(format!("claim:{}", c.role)));
                } else {
                    s7.push(CheckRow::fail(
                        format!("claim:{}", c.role),
                        "auth_fail",
                        format!("hosted claim carries authority {auth} ≠ principal"),
                    ));
                }
            }
            s7.push(CheckRow::pass("participant_class:hosted"));
        } else {
            s7.push(CheckRow::pass("participant_class:native"));
        }
        stages.push(StageReport {
            stage: 7,
            name: "hosting_boundary",
            checks: s7,
        });
    }

    // ── S8 secret_material ───────────────────────────────────────────
    if want(8) {
        let mut s8 = Vec::new();
        for (addr, bytes) in members {
            if let Some(det) = has_secret(bytes) {
                s8.push(CheckRow::fail(
                    format!("member:{addr}"),
                    "secret_or_credentialed_locator",
                    format!("member payload matches detector {det}"),
                ));
            }
        }
        for f in &manifest.fetch {
            for loc in &f.locations {
                if credentialed(loc) {
                    s8.push(CheckRow::fail(
                        format!("fetch:{}", f.address),
                        "secret_or_credentialed_locator",
                        "fetch location carries credentials",
                    ));
                }
            }
        }
        if s8.is_empty() {
            s8.push(CheckRow::pass("secret_scan"));
        }
        stages.push(StageReport {
            stage: 8,
            name: "secret_material",
            checks: s8,
        });
    }

    // ── fold ─────────────────────────────────────────────────────────
    let completeness_names = ["completeness", "present_members_verify", "level_coherence", "hosting_boundary"];
    let mut complete_except = Vec::new();
    for s in &stages {
        if completeness_names.contains(&s.name) && !s.ok() {
            complete_except.push(s.name.to_string());
        }
    }
    // When a completeness stage wasn't run (check_completeness never
    // skips one), completeness only folds the stages that ran — S1..S8
    // always include the four.
    let mut report = BundleValidationReport {
        bundle_id: manifest.version_id.clone(),
        bundle_kind: manifest.bundle_kind.clone(),
        participant_class: manifest.participant_class.clone(),
        max_supported_level: levels::declared_max(manifest)
            .or(Some(derived))
            .map(|l| l.name().to_string())
            .unwrap_or_else(|| ReproLevel::R0.name().to_string()),
        complete: complete_except.is_empty(),
        complete_except,
        stages,
        findings,
        digest: String::new(),
    };
    report.seal();
    report
}

/// A `validate_bundle`/`check_completeness` refusal — a manifest that
/// isn't `hh-bundle/1` at all. The staged engine reports on real
/// manifests only.
pub fn ensure_manifest(j: &Json) -> Result<BundleManifest, BundleError> {
    BundleManifest::from_json(j)
}
