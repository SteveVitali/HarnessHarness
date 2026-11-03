//! `rights` — the §6.4 rights/losses/quotas/recipient-handoff
//! validation owner (REQ-F-006/207/208; AC-6-3; R5.27).
//!
//! Per-item bindings ride `rights-record/1` inputs
//! (`{member, source, version, license, consent, recipients[], uses[],
//! owner, decision, expiry}` — unknown rights are withheld, never
//! inferred). [`admit_handoff`] evaluates a [`HandoffPlan`] against the
//! decoded bundle under the §6.4 scope ceilings and emits the
//! `idp/1`-sealed `handoff-report/1`; [`sanitize_replace`] emits the
//! sealed `rights-supersession/1` for a cleared sanitized replacement
//! — a new identity with supersession and dependent claims
//! recalculated, never native-hash/R1 retention. Departures emit
//! `rights-tombstone/1` and disable dependent procedures; a bare
//! delete marker is not deletion. Records-in/records-out — no public
//! RPC (Packet C C.4); durable/transport/secret primitives stay in
//! their own owners (`hh-ledger`, `fetch`/`trust`, `hh-secrets`).

use hharness_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::manifest::MemberStatus;
use crate::trust::{self, FetchRefusal, FetchTrustPolicy, LocationLeg};

/// The per-item rights binding (`{member, source, version, license,
/// consent, recipients[], uses[], owner, decision, expiry?}`).
pub const RIGHTS_RECORD_SCHEMA: &str = "rights-record/1";
/// The sealed handoff verdict record.
pub const HANDOFF_REPORT_SCHEMA: &str = "handoff-report/1";
/// The sealed sanitized-replacement record.
pub const SUPERSESSION_SCHEMA: &str = "rights-supersession/1";
/// The departure status record.
pub const TOMBSTONE_SCHEMA: &str = "rights-tombstone/1";

/// The closed `RightsRefused` code set (§6.4 refusals).
pub mod code {
    /// An item with no rights record or `decision: unknown`.
    pub const UNKNOWN_RIGHTS: &str = "unknown_rights";
    /// A `decision: withheld` / defaulted-held-out / status-loss item.
    pub const WITHHELD: &str = "withheld";
    /// Secret or credential material in handoff bytes.
    pub const SECRET_MATERIAL: &str = "secret_material";
    /// A declared sensitive/low-entropy identifier in handoff bytes.
    pub const SENSITIVE_IDENTIFIER: &str = "sensitive_identifier";
    /// Fetch absent/unadmitted/unsupported/unsigned-authorization.
    pub const UNAUTHORIZED_FETCH: &str = "unauthorized_fetch";
    /// A declared fetch or rights expiry already past.
    pub const FETCH_EXPIRED: &str = "fetch_expired";
    /// A §6.4 scope ceiling breach (asset/package/wave/envelope/grant).
    pub const QUOTA_BREACH: &str = "quota_breach";
    /// write/delete/list rights or >1 h object/version links.
    pub const GRANT_OVERREACH: &str = "grant_overreach";
    /// A deletion/GC claim whose only evidence is a status marker.
    pub const MARKER_NOT_DELETION: &str = "marker_not_deletion";
    /// Controlled/access closure asserted as public closure.
    pub const SCOPE_MISMATCH: &str = "scope_mismatch";
    /// Optional hosting/S3/account/provisioning needs its own
    /// operator decision — never implied by handoff admission.
    pub const SEPARATE_AUTHORITY: &str = "separate_authority";
    /// A restricted item the recipient binding does not name.
    pub const UNAUTHORIZED_RECIPIENT: &str = "unauthorized_recipient";
    /// A required commitment binding absent (contact/region/date).
    pub const MISSING_INPUT: &str = "missing_input";
    /// A commitment date recorded later than the evaluation clock.
    pub const FUTURE_DATE: &str = "future_date";
    /// The sanitized bytes equal the original — no new identity.
    pub const UNCHANGED_BYTES: &str = "unchanged_bytes";
    /// Original↔derivative linkage without a permitted mapping.
    pub const UNPERMITTED_MAPPING: &str = "unpermitted_mapping";
    /// A malformed record/plan member.
    pub const MALFORMED: &str = "malformed";
}

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// The §6.4 scope table — each scope carries its own ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffScope {
    /// Public review package — 512 MiB/asset, 2 GiB compressed,
    /// 8 GiB expanded, 10 GiB first-wave aggregate.
    PublicReview,
    /// Controlled review staging — ≤20 GiB selected originals.
    ControlledReview,
    /// Named-maintainer read-only access — ≤30 days, links ≤1 h,
    /// never write/delete/list rights.
    AccessGrant,
    /// Full archive — declared envelope ≤200 GiB (quote separately;
    /// breach stops/requotes, never discards to fit).
    FullArchive,
    /// Recipient prebuilt procedure — ≤2 GiB download, 8 GiB
    /// expansion, $0 provider fees.
    RecipientProcedure,
    /// Optional versioned hosting (private S3 etc.) — admission is a
    /// separate operator decision, always refused here.
    OptionalHosting,
}

impl HandoffScope {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::PublicReview => "public_review",
            Self::ControlledReview => "controlled_review",
            Self::AccessGrant => "access_grant",
            Self::FullArchive => "full_archive",
            Self::RecipientProcedure => "recipient_procedure",
            Self::OptionalHosting => "optional_hosting",
        }
    }

    /// Parse; `None` on anything else (the sum is closed).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "public_review" => Self::PublicReview,
            "controlled_review" => Self::ControlledReview,
            "access_grant" => Self::AccessGrant,
            "full_archive" => Self::FullArchive,
            "recipient_procedure" => Self::RecipientProcedure,
            "optional_hosting" => Self::OptionalHosting,
            _ => return None,
        })
    }

    /// `{asset, compressed, expanded, first_wave}` byte ceilings.
    fn ceilings(self) -> [u64; 4] {
        match self {
            Self::PublicReview => [512 * MIB, 2 * GIB, 8 * GIB, 10 * GIB],
            Self::ControlledReview => [20 * GIB; 4],
            Self::AccessGrant => [512 * MIB, 2 * GIB, 8 * GIB, 10 * GIB],
            Self::FullArchive => [200 * GIB; 4],
            Self::RecipientProcedure => [2 * GIB, 2 * GIB, 8 * GIB, 8 * GIB],
            Self::OptionalHosting => [0; 4],
        }
    }
}

/// The per-item `decision` sum — `unknown` is withheld, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightsDecision {
    /// Cleared for the declared recipients/uses.
    Release,
    /// Restricted recipients only (controlled/access closure — never
    /// public closure).
    Restricted,
    /// Explicitly withheld.
    Withheld,
    /// Unrecorded basis — withheld (unknown rights = withheld).
    Unknown,
}

impl RightsDecision {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Restricted => "restricted",
            Self::Withheld => "withheld",
            Self::Unknown => "unknown",
        }
    }

    /// Parse; `None` on anything else (the sum is closed).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "release" => Self::Release,
            "restricted" => Self::Restricted,
            "withheld" => Self::Withheld,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// One `rights-record/1` input — the item's binding evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct RightsRecord {
    /// The member content address this record binds.
    pub member: String,
    /// Item source/version provenance.
    pub source: String,
    /// The item version label.
    pub version: String,
    /// License basis evidence (`""` = none recorded).
    pub license: String,
    /// Consent basis evidence (`""` = none recorded).
    pub consent: String,
    /// Named permitted recipients.
    pub recipients: Vec<String>,
    /// Named permitted uses.
    pub uses: Vec<String>,
    /// The rights owner/decision-maker.
    pub owner: String,
    /// The recorded decision.
    pub decision: RightsDecision,
    /// Item rights expiry (RFC 3339) — shorter rights override
    /// retention honestly.
    pub expiry: Option<String>,
}

impl RightsRecord {
    /// Canonical `rights-record/1` JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(RIGHTS_RECORD_SCHEMA)),
            ("member", Json::str(self.member.clone())),
            ("source", Json::str(self.source.clone())),
            ("version", Json::str(self.version.clone())),
            ("license", Json::str(self.license.clone())),
            ("consent", Json::str(self.consent.clone())),
            (
                "recipients",
                Json::Arr(self.recipients.iter().map(Json::str).collect()),
            ),
            ("uses", Json::Arr(self.uses.iter().map(Json::str).collect())),
            ("owner", Json::str(self.owner.clone())),
            ("decision", Json::str(self.decision.name())),
            (
                "expiry",
                self.expiry
                    .as_ref()
                    .map_or(Json::Null, |e| Json::str(e.clone())),
            ),
        ])
    }
}

/// An expiry/GC/deletion the plan declares — a real departure needs a
/// disposition, an actual date and evidence; a bare marker refuses.
#[derive(Debug, Clone, PartialEq)]
pub struct Departure {
    /// The departing member's address.
    pub member: String,
    /// `expired` | `deleted` | `gc` — anything else is a marker, not a
    /// departure.
    pub disposition: String,
    /// When the departure actually occurred (RFC 3339, ≤ now).
    pub occurred_at: String,
    /// The deletion/expiry evidence reference (a marker alone is not
    /// deletion evidence).
    pub evidence: String,
}

/// The handoff plan under evaluation — AC-6-3's pre-commitment
/// binding: rights/owner/contact/host/region/quotas/actual
/// dates/full-archive envelope recorded before collection or
/// distribution.
pub struct HandoffPlan<'a> {
    /// The §6.4 scope row.
    pub scope: HandoffScope,
    /// The member addresses entering the handoff.
    pub items: Vec<String>,
    /// The per-item rights bindings (one per item maximum).
    pub rights: Vec<RightsRecord>,
    /// Departures declared at handoff (expiry/GC/deletion).
    pub departures: Vec<Departure>,
    /// Named maintainer/private contact (required).
    pub contact: String,
    /// The staging host — `""` = bounded local staging; an
    /// S3/account/provisioning locator is `separate_authority`.
    pub host: String,
    /// Recorded region (required).
    pub region: String,
    /// The named recipient the plan is bound to.
    pub recipient: String,
    /// When the quote/bind actually happened (RFC 3339, ≤ now).
    pub quoted_at: String,
    /// When the owner decision actually happened (RFC 3339, ≤ now).
    pub decided_at: String,
    /// The recorded review deadline (RFC 3339; may be future).
    pub review_before: String,
    /// Read-only grant days (access_grant; ≤30).
    pub grant_days: u64,
    /// Object/version link lifetime in ms (access_grant; ≤1 h).
    pub link_expiry_ms: u64,
    /// Write/delete/list rights requested — always `grant_overreach`.
    pub write_delete_list: bool,
    /// Declared sensitive/low-entropy identifiers — member bytes
    /// carrying one refuse `sensitive_identifier`.
    pub restricted_identifiers: Vec<String>,
    /// Declared package compressed bytes (≤ scope ceiling).
    pub compressed_bytes: u64,
    /// Declared package expanded bytes (≤ scope ceiling).
    pub expanded_bytes: u64,
    /// Declared first-wave aggregate bytes (≤ scope ceiling).
    pub first_wave_bytes: u64,
    /// The declared remote-fetch trust policy (fetch members only).
    pub trust: Option<&'a FetchTrustPolicy>,
}

/// A departure's role→dependent-procedure map (§6.3 input roles;
/// `verify` stages every member, so it always binds).
fn procedures_for(role: &str) -> Vec<&'static str> {
    let mut p = vec!["verify"];
    if role.starts_with("ledger_page") || role.starts_with("trace") {
        p.push("replay");
    }
    match role {
        "definition" => p.extend(["replay", "fresh"]),
        "compiled_bundle" => p.push("replay"),
        "rows" | "analysis" => p.push("recompute"),
        "model" | "instrument" | "configuration" | "resolved_dependencies" => {
            p.extend(["fresh", "recompute"])
        }
        _ => {}
    }
    p
}

/// A `sha256:`-class sensitive-id scan plus the admitted secret
/// detector over member bytes (utf8-lossy — binary members carry no
/// textual identifiers).
fn scan_bytes(bytes: &[u8], restricted: &[String]) -> Result<(), BundleError> {
    let text = String::from_utf8_lossy(bytes);
    let detectors = hharness_secrets::DetectorSet::standard(hharness_secrets::MaskSet::default());
    if let Some(hit) = hharness_secrets::detect(&text, &detectors)
        .iter()
        .find(|h| h.tombstone.is_some())
    {
        return Err(refused(
            code::SECRET_MATERIAL,
            format!("member bytes match detector {}", hit.detector.as_str()),
        ));
    }
    for id in restricted {
        if !id.is_empty() && text.contains(id.as_str()) {
            return Err(refused(
                code::SENSITIVE_IDENTIFIER,
                "member bytes carry a restricted identifier",
            ));
        }
    }
    Ok(())
}

fn refused(c: &'static str, detail: impl Into<String>) -> BundleError {
    BundleError::RightsRefused {
        code: c,
        detail: detail.into(),
    }
}

fn row(m: &str, decision: &str, detail: &str) -> Json {
    Json::obj([
        ("member", Json::str(m)),
        ("decision", Json::str(decision)),
        ("detail", Json::str(detail)),
    ])
}

/// `rfc3339 → ms`; unparseable → `None`.
fn ts_ms(s: &str) -> Option<u64> {
    crate::foreign::rfc3339_unix_nanos(s)?
        .parse::<u128>()
        .ok()
        .map(|ns| (ns / 1_000_000) as u64)
}

/// The sealed `handoff-report/1` verdict record.
#[derive(Debug, Clone, PartialEq)]
pub struct HandoffReport {
    /// The subject bundle's `version_id`.
    pub bundle_id: String,
    /// The evaluated scope.
    pub scope: &'static str,
    /// The bound recipient.
    pub recipient: String,
    /// The recorded evaluation timestamp (RFC 3339).
    pub checked_at: String,
    /// `admitted` | `withheld` | `refused`.
    pub verdict: &'static str,
    /// Per-item `{member, decision, detail}` disposition rows.
    pub items: Vec<Json>,
    /// `rights-tombstone/1` rows the departures emitted.
    pub tombstones: Vec<Json>,
    /// Procedures the departures disabled.
    pub disabled_procedures: Vec<String>,
    /// `{ceiling, observed}` quota record.
    pub quotas: Json,
    /// `{declared_until, effective_until, override}` retention record.
    pub retention: Json,
    /// `idp/1` over the report minus `report_id`.
    pub report_id: String,
}

impl HandoffReport {
    /// Canonical JSON (report_id stamped).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(HANDOFF_REPORT_SCHEMA)),
            ("bundle_id", Json::str(self.bundle_id.clone())),
            ("scope", Json::str(self.scope)),
            ("recipient", Json::str(self.recipient.clone())),
            ("checked_at", Json::str(self.checked_at.clone())),
            ("verdict", Json::str(self.verdict)),
            ("items", Json::Arr(self.items.clone())),
            ("tombstones", Json::Arr(self.tombstones.clone())),
            (
                "disabled_procedures",
                Json::Arr(self.disabled_procedures.iter().map(Json::str).collect()),
            ),
            ("quotas", self.quotas.clone()),
            ("retention", self.retention.clone()),
            ("report_id", Json::str(self.report_id.clone())),
        ])
    }

    /// Stamp `report_id` (`idp/1` — the `AdmissionCard` seal rule).
    pub fn seal(&mut self) {
        self.report_id = String::new();
        let mut doc = match self.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        doc.remove("report_id");
        self.report_id = hharness_identity::idp_id(
            "bundle.handoff",
            Json::Obj(doc).to_canonical_string().as_bytes(),
        );
    }
}

/// `admit_handoff(decoded, plan, now_ms)` — evaluate the plan's items,
/// departures and quotas under its scope ceilings at `now_ms`
/// (deterministic — the caller's store clock). Per-item outcomes are
/// withheld/refused rows inside the report; plan-level refusals (S3
/// authority, marker-only deletion, dates, quota breaches, sensitive
/// content) are typed `RightsRefused` errors — a secret-bearing or
/// over-quota handoff never produces a verdict.
pub fn admit_handoff(
    decoded: &Decoded,
    plan: &HandoffPlan<'_>,
    now_ms: u64,
) -> Result<HandoffReport, BundleError> {
    if plan.scope == HandoffScope::OptionalHosting
        || ["s3:", "account:", "http://", "https://"]
            .iter()
            .any(|p| plan.host.starts_with(p))
    {
        return Err(refused(
            code::SEPARATE_AUTHORITY,
            "optional hosting/account/provisioning needs its own operator decision",
        ));
    }
    if plan.contact.is_empty() || plan.region.is_empty() {
        return Err(refused(code::MISSING_INPUT, "contact/region unbound"));
    }
    for (name, ts) in [
        ("quoted_at", &plan.quoted_at),
        ("decided_at", &plan.decided_at),
    ] {
        match ts_ms(ts) {
            None => return Err(refused(code::MISSING_INPUT, format!("{name} unparseable"))),
            Some(ms) if ms > now_ms => {
                return Err(refused(code::FUTURE_DATE, format!("{name} after now")))
            }
            _ => {}
        }
    }
    if ts_ms(&plan.review_before).is_none() {
        return Err(refused(code::MISSING_INPUT, "review_before unparseable"));
    }
    if plan.scope == HandoffScope::AccessGrant {
        if plan.grant_days > 30 || plan.link_expiry_ms > 3_600_000 {
            return Err(refused(code::QUOTA_BREACH, "grant/link ceiling"));
        }
        if plan.write_delete_list {
            return Err(refused(code::GRANT_OVERREACH, "write/delete/list rights"));
        }
    }
    let checked_at = hharness_ledger::store::rfc3339_ms(now_ms);
    let [asset_cap, compressed_cap, expanded_cap, wave_cap] = plan.scope.ceilings();
    for (name, observed, cap) in [
        ("compressed", plan.compressed_bytes, compressed_cap),
        ("expanded", plan.expanded_bytes, expanded_cap),
        ("first_wave", plan.first_wave_bytes, wave_cap),
    ] {
        if observed > cap {
            return Err(refused(
                code::QUOTA_BREACH,
                format!("{name} {observed} > {cap}"),
            ));
        }
    }
    let rights_of = |member: &str| plan.rights.iter().find(|r| r.member == member);
    let mut items = Vec::new();
    let mut any_withheld = false;
    let mut any_refused = false;
    let mut earliest_expiry: Option<u64> = None;
    for member in &plan.items {
        let m = match decoded
            .manifest
            .members
            .iter()
            .find(|mm| mm.address == *member)
        {
            Some(m) => m,
            None => {
                items.push(row(member, "withheld", "unknown_member"));
                any_withheld = true;
                continue;
            }
        };
        let rec = match rights_of(member) {
            Some(r) => r,
            None => {
                items.push(row(member, "withheld", code::UNKNOWN_RIGHTS));
                any_withheld = true;
                continue;
            }
        };
        if rec.owner.is_empty() || rec.source.is_empty() {
            return Err(refused(
                code::MISSING_INPUT,
                format!("{member} rights owner/source"),
            ));
        }
        let mut fail = |decision: &str, detail: &str, hard: bool| {
            items.push(row(member, decision, detail));
            if hard {
                any_refused = true;
            } else {
                any_withheld = true;
            }
        };
        match rec.decision {
            RightsDecision::Unknown => {
                fail("withheld", code::UNKNOWN_RIGHTS, false);
                continue;
            }
            RightsDecision::Withheld => {
                fail("withheld", code::WITHHELD, false);
                continue;
            }
            RightsDecision::Restricted => {
                if plan.scope == HandoffScope::PublicReview {
                    fail("refused", code::SCOPE_MISMATCH, true);
                    continue;
                }
                if !rec.recipients.iter().any(|r| r == &plan.recipient) {
                    fail("withheld", code::UNAUTHORIZED_RECIPIENT, false);
                    continue;
                }
            }
            RightsDecision::Release => {
                if plan.scope != HandoffScope::PublicReview
                    && !rec.recipients.is_empty()
                    && !rec.recipients.iter().any(|r| r == &plan.recipient)
                {
                    fail("withheld", code::UNAUTHORIZED_RECIPIENT, false);
                    continue;
                }
            }
        }
        if let Some(e) = &rec.expiry {
            match ts_ms(e) {
                Some(ms) if ms <= now_ms => {
                    fail("withheld", code::FETCH_EXPIRED, false);
                    continue;
                }
                Some(ms) => {
                    earliest_expiry = Some(earliest_expiry.map_or(ms, |e| e.min(ms)));
                }
                None => {
                    fail("withheld", code::MALFORMED, false);
                    continue;
                }
            }
        }
        // Held-out solutions/oracle/model I/O default restricted —
        // without recorded license/consent evidence they withhold.
        let held_out = ["solution", "oracle", "model_io"]
            .iter()
            .any(|h| m.role.contains(h));
        if held_out && rec.license.is_empty() && rec.consent.is_empty() {
            fail("withheld", code::WITHHELD, false);
            continue;
        }
        if m.size > asset_cap {
            return Err(refused(
                code::QUOTA_BREACH,
                format!("{member} {} > {asset_cap}", m.size),
            ));
        }
        match m.status {
            MemberStatus::Present => match decoded.members.get(member) {
                Some(bytes) => scan_bytes(bytes, &plan.restricted_identifiers)?,
                None => {
                    fail("withheld", "missing_bytes", false);
                    continue;
                }
            },
            MemberStatus::Fetch => {
                let entry = match decoded.manifest.fetch.iter().find(|e| e.address == *member) {
                    Some(e) => e,
                    None => {
                        fail("refused", code::UNAUTHORIZED_FETCH, true);
                        continue;
                    }
                };
                let mut bad = false;
                for loc in &entry.locations {
                    let c = match trust::admit(plan.trust, entry, loc, now_ms) {
                        Ok(LocationLeg::Pool) | Ok(LocationLeg::HttpsGet { .. }) => continue,
                        Ok(LocationLeg::Unsupported { .. }) => code::UNAUTHORIZED_FETCH,
                        Err(FetchRefusal::LocationExpired) => code::FETCH_EXPIRED,
                        Err(_) => code::UNAUTHORIZED_FETCH,
                    };
                    fail("refused", c, true);
                    bad = true;
                    break;
                }
                if bad {
                    continue;
                }
            }
            MemberStatus::Redacted | MemberStatus::Gc => {
                fail("withheld", m.status.name(), false);
                continue;
            }
        }
        items.push(row(member, rec.decision.name(), "admitted"));
    }
    let mut tombstones = Vec::new();
    let mut disabled: Vec<String> = Vec::new();
    for d in &plan.departures {
        let member = decoded
            .manifest
            .members
            .iter()
            .find(|mm| mm.address == d.member);
        let real = matches!(d.disposition.as_str(), "expired" | "deleted" | "gc")
            && !d.evidence.is_empty()
            && ts_ms(&d.occurred_at).is_some_and(|ms| ms <= now_ms);
        if !real || member.is_none() {
            return Err(refused(
                code::MARKER_NOT_DELETION,
                format!("{} — a marker is not deletion", d.member),
            ));
        }
        let role = member.unwrap().role.clone();
        for p in procedures_for(&role) {
            if !disabled.iter().any(|x| x == p) {
                disabled.push(p.to_string());
            }
        }
        tombstones.push(Json::obj([
            ("schema", Json::str(TOMBSTONE_SCHEMA)),
            ("bundle_id", Json::str(decoded.manifest.version_id.clone())),
            ("member", Json::str(d.member.clone())),
            ("disposition", Json::str(d.disposition.clone())),
            ("occurred_at", Json::str(d.occurred_at.clone())),
            ("evidence", Json::str(d.evidence.clone())),
        ]));
    }
    let verdict = if any_refused {
        "refused"
    } else if any_withheld {
        "withheld"
    } else {
        "admitted"
    };
    let review_ms = ts_ms(&plan.review_before).unwrap_or(0);
    let effective = earliest_expiry.map_or(review_ms, |e| e.min(review_ms));
    let mut report = HandoffReport {
        bundle_id: decoded.manifest.version_id.clone(),
        scope: plan.scope.name(),
        recipient: plan.recipient.clone(),
        checked_at,
        verdict,
        items,
        tombstones,
        disabled_procedures: disabled,
        quotas: Json::obj([
            ("asset_ceiling", Json::Int(asset_cap as i64)),
            ("compressed_ceiling", Json::Int(compressed_cap as i64)),
            ("expanded_ceiling", Json::Int(expanded_cap as i64)),
            ("first_wave_ceiling", Json::Int(wave_cap as i64)),
            ("compressed", Json::Int(plan.compressed_bytes as i64)),
            ("expanded", Json::Int(plan.expanded_bytes as i64)),
            ("first_wave", Json::Int(plan.first_wave_bytes as i64)),
        ]),
        retention: Json::obj([
            ("declared_until", Json::str(plan.review_before.clone())),
            (
                "effective_until",
                Json::str(hharness_ledger::store::rfc3339_ms(effective)),
            ),
            (
                "override",
                Json::str(if earliest_expiry.is_some_and(|e| e < review_ms) {
                    "shorter_rights"
                } else {
                    "none"
                }),
            ),
        ]),
        report_id: String::new(),
    };
    report.seal();
    Ok(report)
}

/// The cleared sanitization declaration — `kind`/`version` name the
/// transform; `mapping_permitted` is the original↔derivative linkage
/// permission (retained only when permitted); `procedures` names the
/// dependent procedures the per-procedure loss is computed over.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizeTransform {
    /// The transform kind (`pseudonymize`/`redact`/`aggregate`/…).
    pub kind: String,
    /// The transform version.
    pub version: String,
    /// Whether the original→derivative mapping is permitted.
    pub mapping_permitted: bool,
    /// Dependent procedures the exact per-procedure loss binds.
    pub procedures: Vec<String>,
    /// Sensitive identifiers the replacement must not carry.
    pub restricted_identifiers: Vec<String>,
}

/// The sanitized-replacement outcome — new identity + supersession.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizedItem {
    /// The original member's address.
    pub original: String,
    /// The replacement's content address (new identity — changed
    /// bytes never retain native hashes/R1).
    pub replacement: String,
    /// The replacement media type (the original member's).
    pub media_type: String,
    /// `{field, from, to}` dependent-claim recalculation rows.
    pub dependent_claims: Vec<Json>,
    /// `{procedure, loss, r_levels}` exact per-procedure losses.
    pub losses: Vec<Json>,
    /// The sealed `rights-supersession/1` record.
    pub supersession: Json,
}

/// `sanitize_replace(decoded, member, new_bytes, transform)` — the
/// cleared sanitized replacement. `new_bytes` must differ (a new
/// identity is minted); scans run on the replacement bytes before the
/// supersession emits.
pub fn sanitize_replace(
    decoded: &Decoded,
    member: &str,
    new_bytes: &[u8],
    transform: &SanitizeTransform,
) -> Result<SanitizedItem, BundleError> {
    let m = decoded
        .manifest
        .members
        .iter()
        .find(|mm| mm.address == member)
        .ok_or_else(|| refused(code::MALFORMED, format!("{member} unknown")))?;
    if transform.kind.is_empty() || transform.version.is_empty() {
        return Err(refused(code::MISSING_INPUT, "transform kind/version"));
    }
    if !transform.mapping_permitted {
        return Err(refused(code::UNPERMITTED_MAPPING, member));
    }
    scan_bytes(new_bytes, &transform.restricted_identifiers)?;
    let replacement = hharness_identity::address(new_bytes, m.media_type.clone()).id();
    if replacement == member {
        return Err(refused(code::UNCHANGED_BYTES, member));
    }
    let mut dependent_claims = Vec::new();
    walk_refs(
        &decoded.manifest.to_json(),
        member,
        "",
        &mut dependent_claims,
        &replacement,
    );
    let losses: Vec<Json> = transform
        .procedures
        .iter()
        .map(|p| {
            Json::obj([
                ("procedure", Json::str(p.clone())),
                ("loss", Json::str("native_identity")),
                ("r_levels", Json::str("R0")),
            ])
        })
        .collect();
    let mut sup = Json::obj([
        ("schema", Json::str(SUPERSESSION_SCHEMA)),
        ("bundle_id", Json::str(decoded.manifest.version_id.clone())),
        ("original", Json::str(member)),
        ("replacement", Json::str(replacement.clone())),
        (
            "transform",
            Json::obj([
                ("kind", Json::str(transform.kind.clone())),
                ("version", Json::str(transform.version.clone())),
                ("mapping", Json::str("permitted")),
            ]),
        ),
        ("dependent_claims", Json::Arr(dependent_claims.clone())),
        ("losses", Json::Arr(losses.clone())),
        ("supersession_id", Json::str("")),
    ]);
    if let Json::Obj(mut o) = sup.clone() {
        o.remove("supersession_id");
        let id = hharness_identity::idp_id(
            "bundle.supersession",
            Json::Obj(o).to_canonical_string().as_bytes(),
        );
        if let Json::Obj(ref mut o2) = sup {
            o2.insert("supersession_id".into(), Json::str(id));
        }
    }
    Ok(SanitizedItem {
        original: member.to_string(),
        replacement,
        media_type: m.media_type.clone(),
        dependent_claims,
        losses,
        supersession: sup,
    })
}

/// Walk the manifest document for string fields equal to `old`,
/// recording `{field, from, to}` recalculation rows.
fn walk_refs(j: &Json, old: &str, path: &str, out: &mut Vec<Json>, new: &str) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                walk_refs(v, old, &format!("{path}.{k}"), out, new);
            }
        }
        Json::Arr(a) => {
            for (i, v) in a.iter().enumerate() {
                walk_refs(v, old, &format!("{path}[{i}]"), out, new);
            }
        }
        Json::Str(s) if s == old => out.push(Json::obj([
            ("field", Json::str(path.trim_start_matches('.'))),
            ("from", Json::str(old)),
            ("to", Json::str(new)),
        ])),
        _ => {}
    }
}
