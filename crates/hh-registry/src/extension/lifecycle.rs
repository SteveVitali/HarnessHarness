//! `extension::lifecycle` — the C1 extension lifecycle over the live
//! `TrustRootPolicy` set (S4.14a; spec §6.2/§8.1; R-2.8.5 AC-4..9).
//!
//! The chain `discover → resolve → review → seal → activate → update →
//! revoke → audit` is a set of *pure* ops over records plus the store's
//! live-policy view ([`TrustView`]) — none of these functions performs I/O
//! (CC5: fetch is a caller-supplied `FetchOutcome`; the ledger write is the
//! caller's fenced `append`).
//!
//! - `discover` enforces the union `allowed_sources` — a source outside
//!   every live policy is `SourceNotAllowed`; the gate is fail-open only
//!   when **no** live policy declares the set.
//! - `resolve_extension` runs [`super::resolve_candidate`] then verifies
//!   the payload attestations under the live anchors: subject-bound
//!   (`subject_hash` must equal the content id), `required_predicates`
//!   covered by the *verified* kind set, `max_age` freshness (`StalePin`
//!   on expiry), `require_signature_for` ⇒ quarantine when no verified
//!   `signature`, hash-only attestations capped by `hash_only_ceiling`
//!   (a ceiling breach quarantines — the record's `text_authority` stays
//!   the minted value so `check_text_authority` still holds).
//! - `review`/`approve_seal_diff` — the review doc (install/exec plan,
//!   surface delta, advisory scans) and the `approval` endorsement whose
//!   subject is the seal `HirDiff` (AC-7's second half; `basis_ref` is
//!   the `permission_id`).
//! - `update_assessment` — the `LabelRegression` listing + the widening
//!   verdict over a `(superseded, successor)` extension pair (the store's
//!   `publish` then enforces `origin = human` + attestation + MAJOR
//!   label).
//! - `install_plan`/`install_completed` — the model-install Procedure
//!   (AC-7): `install_requested` is emitted before any fetch, the result
//!   mints `≤ external` authority with `{model, source,
//!   instructing_content}` taint, and a requested grant outside the
//!   proposer's own grants fails `AuthorityWidening`.
//! - `surface_document`/`check_surface`/`drift_decision` — the MCP
//!   `hh-mcp-listing/1` lift to a `HirDocument`, the `HirDiff`-carrying
//!   `SurfaceDrift` evidence record (AC-4), and the
//!   `notify|pause_surface|terminate` delivery decision.
//! - `propagate_revocation` — the run-side plan a revocation produces:
//!   `security.extension.revoked`, server terminations, dropped surface
//!   tools, grants → deny, in-flight effects → `action.effect.unknown`,
//!   and the `depends_on_revoked` result annotation (AC-8).
//! - R2.20 (DF-S1.23-2 / BL-28) — the producer legs the emitters left
//!   open: `scan_declared_sources` enumerates declared sources into
//!   `ExtensionRef`s (the `discover(sources, policy)` enumeration half —
//!   allowlist, containment, credential-free, merge-policy); the L3
//!   claim lift (`lift_declared_claims`) and `enforce_leg_boundaries`
//!   (L1–L3 incl. the delegate-grant `LegCrossing`) run inside
//!   `resolve`/`install`; `scanner_policy` consumes the minted
//!   `TextHygieneReport` (`deny` → `ScanDenied`, `quarantine` →
//!   `quarantined` + the durable row); and `seal_extension` /
//!   `activate_extension` mint `security.extension.{sealed, loaded}`
//!   (the check_surface convention — payloads ride the result, the
//!   caller appends).

use std::collections::BTreeSet;

use hh_hir::diff::{ops_between, DiffClassification, DiffDerivation, HirDiff};
use hh_hir::document::DefinitionVersionRef;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::KindRecord;
use hh_hir::refs::RunRef;
use hh_identity::idp::idp_id;
use hh_provenance::endorse::endorse;
use hh_provenance::{
    verify_attestation, Attestation, AttestationKind, AuthorityClass, ContentKind,
    EndorsementBasis, EndorsementError, Label, LabelEndorsed, Origin, ProvenanceRecord, TaintTag,
    TrustedAnchors,
};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::import;
use crate::records::ModelInstallRule;

use super::{
    default_text_authority, lift_declared_claims, validate_extension_record, AttestationStatus,
    Candidate, DeclaredSource, ExtensionKind, ExtensionRecord, ExtensionRef, FetchOutcome,
    HygieneStatus, MergePolicy, SourceLocator, TrustStatus,
};

// ── TrustView ─────────────────────────────────────────────────────────────

/// The resolved trust posture: the union/intersection projections of every
/// *live* `TrustRootPolicy` (`resolved`/`sealed` admission, not revoked).
/// A `TrustView` built over an empty policy set is the fail-open default —
/// `allowed_sources` empty ⇒ no gate; `accepted_signers` empty ⇒ nothing
/// verifies (fail-closed where verification is the *lift*, fail-open where
/// absence of policy simply means "no rule declared").
#[derive(Debug, Clone, PartialEq)]
pub struct TrustView {
    /// Union of `allowed_sources` over live policies (empty ⇒ fail-open).
    pub allowed_sources: BTreeSet<String>,
    /// Union of `accepted_signers` — the `verify_attestation` signer set.
    pub accepted_signers: BTreeSet<String>,
    /// Union of `required_predicates` — attestation-kind spellings the
    /// *verified* set must cover (C1: the `Attestation` record carries no
    /// predicate member at this slice, so the predicate gate reads verified
    /// kinds — `signature` in the set requires a verified signature).
    pub required_predicates: BTreeSet<String>,
    /// Union of `require_signature_for` spellings (record kinds + extension
    /// kinds — `store.signature_required` folds this at `register` too).
    pub require_signature_for: BTreeSet<String>,
    /// Lowest `hash_only_ceiling` over live policies (`external` default).
    pub hash_only_ceiling: AuthorityClass,
    /// Lowest non-`None` `max_age` over live policies (strictest wins).
    pub max_age: Option<u64>,
    /// The strictest `model_install` posture over live policies
    /// (`deny > ask > allow_attenuated`).
    pub model_install: ModelInstallRule,
    /// The strictest `scanner_policy` posture over live policies — how the
    /// minted `TextHygieneReport` is *consumed* at resolve/install (R2.20;
    /// `deny > quarantine > advisory`; an undeclared posture set is
    /// `Advisory` — no rule declared; an unrecognised *declared* spelling
    /// fails closed as `Deny`).
    pub scanner_policy: ScannerPolicy,
    /// The live policies' `version_id`s — the member set a signer-attributed
    /// `mark_stale_derived` names on policy supersession.
    pub live_policy_ids: Vec<String>,
}

impl TrustView {
    /// The `TrustedAnchors` for `verify_attestation` (accepted signers only
    /// — no chain heads at C1, matching the store's projection).
    pub fn anchors(&self) -> TrustedAnchors {
        TrustedAnchors {
            signers: self.accepted_signers.clone(),
            chain_heads: BTreeSet::new(),
        }
    }

    /// Whether `source` may publish under this view — fail-open only when
    /// no live policy declared `allowed_sources` (the union is empty).
    /// Membership matches the source's kind spelling or a
    /// `kind:coordinate` spelling (`git:https://…`).
    pub fn source_allowed(&self, source: &DeclaredSource) -> bool {
        if self.allowed_sources.is_empty() {
            return true;
        }
        let kind = source.kind_str();
        if self.allowed_sources.contains(kind) {
            return true;
        }
        let coordinate = match source {
            DeclaredSource::DirectoryScan { root, .. } => format!("{kind}:{root}"),
            DeclaredSource::Marketplace { catalog_ref } => format!("{kind}:{catalog_ref}"),
            DeclaredSource::Registry { name } => format!("{kind}:{name}"),
            DeclaredSource::Git { url, .. } => format!("{kind}:{url}"),
            DeclaredSource::Archive { url, .. } => format!("{kind}:{url}"),
            DeclaredSource::McpEndpoint { uri } => format!("{kind}:{uri}"),
            DeclaredSource::InstructionFiles { roots } => format!("{kind}:{}", roots.join(",")),
        };
        self.allowed_sources.contains(&coordinate)
    }

    /// Whether an extension of `kind_spelling` resolves signature-required.
    pub fn signature_required(&self, kind_spelling: &str) -> bool {
        self.require_signature_for.contains(kind_spelling)
    }
}

/// `deny > ask > allow_attenuated` — the strictest posture wins under a
/// union of live policies (a union of permissive policies never loosens a
/// stricter one).
pub fn strictest_install(a: ModelInstallRule, b: ModelInstallRule) -> ModelInstallRule {
    let rank = |r: ModelInstallRule| match r {
        ModelInstallRule::Deny => 0,
        ModelInstallRule::Ask => 1,
        ModelInstallRule::AllowAttenuated => 2,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// The `scanner_policy` posture a `TrustRootPolicy` declares (R2.20 — the
/// member existed since S4.1 as data; this slice makes it the consumption
/// rule for the `TextHygieneReport` the resolver mints). `advisory` is the
/// pre-R2.20 posture — the report mints and surfaces at `review` but never
/// gates; `quarantine` lands a flagged/unavailable report `quarantined`
/// (the record registers; the admission verdict is the durable row);
/// `deny` refuses the resolve/install with `ScanDenied` (nothing mints —
/// the refusal is the record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScannerPolicy {
    /// Advisory — mint and surface; never gate.
    Advisory,
    /// Flagged/unavailable → `TrustStatus::Quarantined` + the
    /// `security.extension.quarantined` payload.
    Quarantine,
    /// Flagged/unavailable → `RegistryError::ScanDenied`.
    Deny,
}

impl ScannerPolicy {
    /// Parse a declared spelling. An unrecognised spelling is `Deny` —
    /// a malformed trust rule fails closed, never silently advisory.
    pub fn parse(s: &str) -> ScannerPolicy {
        match s {
            "advisory" | "off" => ScannerPolicy::Advisory,
            "quarantine" => ScannerPolicy::Quarantine,
            "deny" => ScannerPolicy::Deny,
            _ => ScannerPolicy::Deny,
        }
    }

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScannerPolicy::Advisory => "advisory",
            ScannerPolicy::Quarantine => "quarantine",
            ScannerPolicy::Deny => "deny",
        }
    }
}

/// `deny > quarantine > advisory` — the strictest scanner posture wins
/// under a union of live policies (same rule as [`strictest_install`]).
pub fn strictest_scan(a: ScannerPolicy, b: ScannerPolicy) -> ScannerPolicy {
    a.max(b) // enum order Advisory < Quarantine < Deny — the max is strictest
}

/// The proposer's identity coordinate — `origin.tag()` names the *class*;
/// `proposer_ref` names the coordinate (`model:<ref>`-shaped).
fn origin_ref(o: &Origin) -> String {
    match o {
        Origin::Human { author_ref, .. } => format!("human:{author_ref}"),
        Origin::Model { model_ref, .. } => format!("model:{model_ref}"),
        Origin::Tool { capability, .. } => format!("tool:{capability}"),
        Origin::Evolution { candidate_id, .. } => format!("evolution:{candidate_id}"),
        Origin::Import { source_system, .. } => format!("import:{source_system}"),
        Origin::Migration { from_dialect } => format!("migration:{from_dialect}"),
        Origin::Kernel { component_ref } => format!("kernel:{component_ref}"),
        Origin::Participant {
            participant_ref, ..
        } => format!("participant:{participant_ref}"),
        Origin::Cache { entry_ref } => format!("cache:{entry_ref}"),
    }
}

// ── discover ─────────────────────────────────────────────────────────────

/// `discover(declared) → candidates` — the allowlist gate (§6.2): every
/// candidate's `DeclaredSource` must be a member of the union
/// `allowed_sources` when any live policy declared the set. A refused
/// source fails `SourceNotAllowed` — the discovery never silently drops
/// (a drop would hide the refusal; the error is the record).
pub fn discover(
    candidates: &[Candidate],
    view: &TrustView,
) -> Result<Vec<Candidate>, RegistryError> {
    let mut out = Vec::new();
    for c in candidates {
        if !view.source_allowed(&c.source) {
            return Err(RegistryError::SourceNotAllowed {
                source: format!("{}:{}", c.source.kind_str(), c.locator.credential_free_uri),
            });
        }
        out.push(c.clone());
    }
    Ok(out)
}

// ── declared-source scanners (R2.20 — the `discover` enumeration half) ──────

/// One member of a declared source's listing — the scanner's input row.
/// CC5: the enumeration itself is caller-supplied (the filesystem walk, the
/// marketplace listing, the endpoint inventory are I/O the pure layer never
/// performs — the same seam `FetchOutcome` occupies); the scanner's job is
/// the *trust* leg: source allowlist, containment under the declared roots,
/// credential-free locators, and the merge-policy fold into `ExtensionRef`s.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanEntry {
    /// The discovered extension name.
    pub name: String,
    /// The discovered kind.
    pub kind: ExtensionKind,
    /// The member payload the source's enumeration supplied —
    /// `path` for filesystem-shaped sources (`directory_scan`: relative
    /// under `root`; `instruction_files`: the full path, contained under a
    /// declared root), `uri`/`version` for remote sources.
    pub member: Json,
}

/// Containment: a filesystem-shaped member `path` must be relative and free
/// of `..` components — an escape from the declared root(s) is `ScanDenied`.
fn relative_member_path(entry: &ScanEntry) -> Result<&str, RegistryError> {
    let p = entry
        .member
        .get("path")
        .and_then(Json::as_str)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: format!("scan.{}.path", entry.name),
            detail: "filesystem source member requires a `path` string".to_string(),
        })?;
    if p.starts_with('/')
        || p.starts_with('~')
        || p.split('/').any(|c| c == "..")
        || p.contains("://")
    {
        return Err(RegistryError::ScanDenied {
            detail: format!(
                "member `{}` path `{p}` escapes the declared source root",
                entry.name
            ),
        });
    }
    Ok(p)
}

/// `scan_entry` — one listing member under one declared source → one
/// `ExtensionRef` (`locator.scheme = source.kind_str()` so the assembly's
/// declared-source coverage check sees it; `selector`/`resolved`/`content`
/// unset — resolve pins, the scanner only enumerates).
fn scan_entry(source: &DeclaredSource, entry: &ScanEntry) -> Result<ExtensionRef, RegistryError> {
    let member = &entry.member;
    let uri_member = member.get("uri").and_then(Json::as_str);
    let version = member.get("version").and_then(Json::as_str);
    let (uri, selector) = match source {
        DeclaredSource::DirectoryScan { root, .. } => {
            let rel = relative_member_path(entry)?;
            (format!("{}/{}", root.trim_end_matches('/'), rel), None)
        }
        DeclaredSource::InstructionFiles { roots } => {
            let full = entry
                .member
                .get("path")
                .and_then(Json::as_str)
                .ok_or_else(|| RegistryError::SchemaViolation {
                    path: format!("scan.{}.path", entry.name),
                    detail: "instruction_files member requires a `path` string".to_string(),
                })?;
            if full.split('/').any(|c| c == "..")
                || !roots.iter().any(|r| {
                    full == r.trim_end_matches('/')
                        || full.starts_with(&format!("{}/", r.trim_end_matches('/')))
                })
            {
                return Err(RegistryError::ScanDenied {
                    detail: format!(
                        "instruction file `{full}` is outside the declared roots {roots:?}"
                    ),
                });
            }
            (full.to_string(), None)
        }
        DeclaredSource::Git { url, ref_ } => (
            url.clone(),
            version.or(Some(ref_.as_str())).map(str::to_string),
        ),
        DeclaredSource::Registry { name } => (
            uri_member
                .map(str::to_string)
                .unwrap_or_else(|| format!("{name}/{}", entry.name)),
            version.map(str::to_string),
        ),
        DeclaredSource::Marketplace { catalog_ref } => (
            uri_member
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}/{}", catalog_ref.trim_end_matches('/'), entry.name)),
            version.map(str::to_string),
        ),
        DeclaredSource::Archive { url, sha } => {
            let uri = match member.get("path").and_then(Json::as_str) {
                Some(p) => format!("{url}#{p}"),
                None => url.clone(),
            };
            (uri, Some(sha.clone()))
        }
        DeclaredSource::McpEndpoint { uri } => (
            uri_member
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}/{}", uri.trim_end_matches('/'), entry.name)),
            version.map(str::to_string),
        ),
    };
    let locator = SourceLocator {
        scheme: source.kind_str().to_string(),
        credential_free_uri: uri,
        selector,
        resolved: None,
        fetched_at: None,
    };
    locator.validate_credential_free(&format!("scan.{}", entry.name))?;
    Ok(ExtensionRef {
        name: entry.name.clone(),
        kind: entry.kind.clone(),
        locator,
        content: None,
        extension_id: None,
    })
}

/// `scan_declared_sources(scans, view, merge_policy)` — the declared-source
/// enumeration (§5g.5 §2 `discover(sources, policy) → [Candidate]`'s producer
/// half): each `(DeclaredSource, listing)` pair yields `ExtensionRef`s, the
/// `allowed_sources` allowlist gates the source (`SourceNotAllowed`), member
/// paths are contained under the declared roots (`ScanDenied`), locators are
/// credential-free, and `merge_policy` resolves `(name, kind)` collisions —
/// `exact_only` refuses `NameCollision`, `disjoint` admits the disjoint
/// namespaces (each ref keeps its source's locator).
pub fn scan_declared_sources(
    scans: &[(DeclaredSource, Vec<ScanEntry>)],
    view: &TrustView,
    merge_policy: MergePolicy,
) -> Result<Vec<ExtensionRef>, RegistryError> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for (source, entries) in scans {
        if !view.source_allowed(source) {
            return Err(RegistryError::SourceNotAllowed {
                source: source.kind_str().to_string(),
            });
        }
        for e in entries {
            let r = scan_entry(source, e)?;
            let key = (r.name.clone(), r.kind.as_str().to_string());
            if merge_policy == MergePolicy::ExactOnly && !seen.insert(key) {
                return Err(RegistryError::NameCollision {
                    namespace: "extension".to_string(),
                    name: format!("{}:{}", r.kind.as_str(), r.name),
                });
            }
            out.push(r);
        }
    }
    Ok(out)
}

/// `enforce_leg_boundaries(record)` — the L1–L3 runtime check the
/// resolve/install/seal/activate paths all run on the record (R2.20 —
/// DF-S1.23-2's "beyond the record-validation halves" leg):
/// [`validate_extension_record`] covers L1 (minted `text_authority`
/// equality — LocationElevation), credential-free locators, the
/// attestation-status/attestations consistency check, and the L3
/// claims-never-carry-grant-shapes rule; this wrapper adds the **L2
/// conferral check** — a record minted under a delegate origin
/// (`model:`/`evolution:`/`participant:`/`tool:`/`import:`/`cache:`/
/// `migration:`) or installed by a delegate (`installed_by` names a
/// delegate coordinate) carries **no conferred grants**: grants confer
/// only through a `Permission` with `issuer.authority ≥ principal`
/// (§5g.5 §2.3), and a delegate-origin record is attenuated, never
/// conferring (AC-R-2.8.5-7's install result is grants-empty).
pub fn enforce_leg_boundaries(record: &ExtensionRecord) -> Result<(), RegistryError> {
    validate_extension_record(record)?;
    if record.trust.grants.is_empty() {
        return Ok(());
    }
    let delegate_origin = matches!(
        record.provenance.origin,
        Origin::Model { .. }
            | Origin::Tool { .. }
            | Origin::Evolution { .. }
            | Origin::Import { .. }
            | Origin::Migration { .. }
            | Origin::Participant { .. }
            | Origin::Cache { .. }
    );
    let delegate_installer = record
        .trust
        .installed_by
        .as_deref()
        .map(|i| {
            [
                "model:",
                "tool:",
                "evolution:",
                "import:",
                "migration:",
                "participant:",
                "cache:",
            ]
            .iter()
            .any(|p| i.starts_with(p))
        })
        .unwrap_or(false);
    if delegate_origin || delegate_installer {
        return Err(RegistryError::SchemaViolation {
            path: "extension.trust.grants".to_string(),
            detail: format!(
                "LegCrossing: a {}-origin record (installed_by {:?}) carries {} conferred \
                 grant(s) — grants confer only through a Permission with \
                 issuer.authority ≥ principal, never through a delegate leg",
                record.provenance.origin.tag(),
                record.trust.installed_by,
                record.trust.grants.len()
            ),
        });
    }
    Ok(())
}

/// `security.extension.resolved` — minted when `resolve_extension` produces
/// a register-ready record (the resolve leg's durable row; R2.20). Refs and
/// spellings only — content-free (CC2).
fn resolved_payload(record: &ExtensionRecord) -> Json {
    Json::obj([
        ("class", Json::str("security.extension.resolved")),
        ("extension_name", Json::str(record.name.clone())),
        ("kind", Json::str(record.kind.as_str())),
        ("content", Json::str(record.content.id())),
        (
            "resolved",
            record
                .locator
                .resolved
                .clone()
                .map_or(Json::Null, Json::str),
        ),
        ("source", Json::str(record.locator.scheme.clone())),
        (
            "attestation_status",
            record.trust.attestation_status.to_json(),
        ),
        (
            "text_authority",
            Json::str(record.trust.text_authority.as_str()),
        ),
        (
            "text_hygiene",
            Json::str(record.trust.text_hygiene.status.as_str()),
        ),
        ("status", Json::str(record.trust.status.as_str())),
    ])
}

/// `security.extension.quarantined` — the durable quarantine verdict row
/// (CC2: quarantine decisions mint durable rows). `reasons` carries the
/// typed spellings (`signature_required`, `hash_only_ceiling`,
/// `scanner_flagged`, `scanner_unavailable`).
fn quarantined_payload(record: &ExtensionRecord, reasons: &[&str]) -> Json {
    Json::obj([
        ("class", Json::str("security.extension.quarantined")),
        ("extension_name", Json::str(record.name.clone())),
        ("kind", Json::str(record.kind.as_str())),
        ("content", Json::str(record.content.id())),
        (
            "hygiene",
            Json::str(record.trust.text_hygiene.status.as_str()),
        ),
        (
            "reasons",
            Json::Arr(reasons.iter().map(|r| Json::str(*r)).collect()),
        ),
    ])
}

// ── resolve ───────────────────────────────────────────────────────────────

/// The `resolve_extension` outcome: the minted `ExtensionRecord` plus the
/// trust verdicts a caller (or a test) reads. `quarantined` is derived —
/// `signature_required && !verified(signature)` — matching the admission
/// the store forces at `register`.
#[derive(Debug, Clone)]
pub struct ResolveReport {
    /// The resolved record (register-ready).
    pub record: ExtensionRecord,
    /// The verified attestation kinds (`{"signature"}`, `{"pin"}`, …).
    pub verified_kinds: BTreeSet<String>,
    /// `signature_required && !verified(signature)` — the AC-6 quarantine,
    /// or a `scanner_policy = quarantine` verdict on a flagged/unavailable
    /// `TextHygieneReport` (R2.20).
    pub quarantined: bool,
    /// Advisory notes (e.g. `hash_only_ceiling` applied).
    pub notes: Vec<String>,
    /// The minted `security.extension.*` payloads in lifecycle order —
    /// `resolved`, then `quarantined` when the verdict lands (R2.20; the
    /// caller appends them through the fenced writer, the `check_surface`
    /// convention — the boundary mints evidence, never pretends appended).
    pub events: Vec<Json>,
}

/// `resolve_extension` (§6.2; AC-R-2.8.5-6): the C1 resolver — the Stage-1
/// [`super::resolve_candidate`] mint plus the trust-policy layer:
///
/// 1. `allowed_sources` — a source outside the union is `SourceNotAllowed`.
/// 2. Attestation verification — each candidate attestation must be
///    self-consistent, `Signature|Pin|HashChain`-kinded, `subject_hash ==
///    content.id()`, and anchor-clean under the live `TrustedAnchors`;
///    a failing one never enters `attestations[]`.
/// 3. `required_predicates` — every live-policy predicate spelling must
///    appear among the *verified* kinds; an uncovered predicate refuses
///    `AttestationFailed` (the attestation does not verify *under this
///    policy*).
/// 4. `max_age` — a signature-required record whose freshest signature
///    attestation is older than the bound refuses `StalePin`.
/// 5. `require_signature_for` — a matching kind with no verified
///    `signature` mints `quarantined` (the record registers; the
///    admission axis lands `quarantined` at `register` — AC-6).
/// 6. `hash_only_ceiling` — a non-signature verification above the
///    policy ceiling quarantines the record (the ceiling is a policy
///    verdict, not a label edit — `text_authority` stays minted).
#[allow(clippy::too_many_arguments)] // the resolve inputs are the call's arity.
pub fn resolve_extension(
    candidate: &Candidate,
    outcome: &FetchOutcome,
    attestations: &[Attestation],
    payload: Option<&[u8]>,
    manifest: Option<&Json>,
    origin: Origin,
    view: &TrustView,
    now: u64,
) -> Result<ResolveReport, RegistryError> {
    if !view.source_allowed(&candidate.source) {
        return Err(RegistryError::SourceNotAllowed {
            source: format!(
                "{}:{}",
                candidate.source.kind_str(),
                candidate.locator.credential_free_uri
            ),
        });
    }
    let content_id = outcome.content.id();
    let anchors = view.anchors();
    let mut verified_kinds = BTreeSet::new();
    let mut kept = Vec::new();
    for att in attestations {
        // Subject-binding: the attestation covers *this* content pin —
        // an attestation over anything else is not evidence for this
        // candidate (CC3; never a transferable claim).
        if att.subject_hash != content_id {
            continue;
        }
        if !matches!(
            att.kind,
            AttestationKind::Signature | AttestationKind::Pin | AttestationKind::HashChain
        ) {
            continue;
        }
        if verify_attestation(att, &anchors).is_err() {
            continue;
        }
        kept.push(att.clone());
        verified_kinds.insert(att.kind.as_str().to_string());
    }
    // Predicate coverage — over the verified set.
    for p in &view.required_predicates {
        if !verified_kinds.contains(p) {
            return Err(RegistryError::AttestationFailed {
                detail: format!("required predicate `{p}` not covered by a verified attestation"),
            });
        }
    }
    let signature_required = view.signature_required(candidate.kind.as_str());
    let signed = verified_kinds.contains("signature");
    // Freshness — a signature-required record's freshest signature
    // attestation past `max_age` is `StalePin`, not a silent drop.
    if signature_required && signed {
        if let Some(max_age) = view.max_age {
            let freshest = kept
                .iter()
                .filter(|a| a.kind == AttestationKind::Signature)
                .map(|a| a.verified_at)
                .max()
                .unwrap_or(0);
            if now.saturating_sub(freshest) > max_age {
                return Err(RegistryError::StalePin {
                    version_id: candidate.name.clone(),
                    detail: format!("signature attestation older than max_age {max_age}ms"),
                });
            }
        }
    }
    let status = if signed {
        AttestationStatus::Verified(AttestationKind::Signature)
    } else if verified_kinds.contains("pin") {
        AttestationStatus::Verified(AttestationKind::Pin)
    } else if verified_kinds.contains("hash_chain") {
        AttestationStatus::Verified(AttestationKind::HashChain)
    } else if attestations.is_empty() || kept.is_empty() {
        if attestations.is_empty() {
            AttestationStatus::Missing
        } else {
            AttestationStatus::Failed
        }
    } else {
        AttestationStatus::Failed
    };
    let mut record = super::resolve_candidate(candidate, outcome, origin, payload, now);
    record.trust.attestation_status = status.clone();
    record.trust.attestations = kept;
    record.trust.text_authority =
        default_text_authority(&candidate.kind, &record.provenance.origin, &status);
    // The manifest + the L3 claim lift (R2.20): `allowed-tools`, tool
    // annotations and permission-manifest members become `declared_claims`
    // — the only legal home; a pin/grant-shaped claim value is `LegCrossing`
    // here, at runtime, before the record exists.
    if let Some(m) = manifest {
        record.trust.declared_claims = lift_declared_claims(m)?;
        record.manifest = m.clone();
    }
    let mut notes = Vec::new();
    let mut reasons: Vec<&str> = Vec::new();
    // `hash_only_ceiling` — a non-signature verification conferring above
    // the policy ceiling lands quarantined (a hash pin is integrity,
    // never endorsement — the ceiling breach is a policy verdict).
    if signature_required && !signed {
        reasons.push("signature_required");
    }
    if !signed && record.trust.text_authority > view.hash_only_ceiling {
        reasons.push("hash_only_ceiling");
        notes.push(
            "hash_only_ceiling breach — non-signature verification confers at most the policy ceiling"
                .to_string(),
        );
    }
    // The `TextHygieneReport` consumer (R2.20): the strictest live
    // `scanner_policy` acts on the minted report — `deny` refuses
    // `ScanDenied`, `quarantine` lands `quarantined` (an `unavailable`
    // report is never `clean` — both consume), `advisory` records the read.
    let hygiene = record.trust.text_hygiene.status;
    if hygiene != HygieneStatus::Clean {
        match view.scanner_policy {
            ScannerPolicy::Deny => {
                return Err(RegistryError::ScanDenied {
                    detail: format!(
                        "scanner_policy = deny — `{}` text_hygiene = {}",
                        candidate.name,
                        hygiene.as_str()
                    ),
                });
            }
            ScannerPolicy::Quarantine => {
                reasons.push(match hygiene {
                    HygieneStatus::Flagged => "scanner_flagged",
                    _ => "scanner_unavailable",
                });
                notes.push(format!(
                    "scanner_policy = quarantine — text_hygiene = {}",
                    hygiene.as_str()
                ));
            }
            ScannerPolicy::Advisory => {
                notes.push(format!(
                    "text_hygiene = {} (advisory — report surfaces at review)",
                    hygiene.as_str()
                ));
            }
        }
    }
    let quarantined = !reasons.is_empty();
    if quarantined {
        record.trust.status = TrustStatus::Quarantined;
    }
    // L1–L3 on the minted record before it leaves resolve (R2.20).
    enforce_leg_boundaries(&record)?;
    let mut events = vec![resolved_payload(&record)];
    if quarantined {
        events.push(quarantined_payload(&record, &reasons));
    }
    Ok(ResolveReport {
        record,
        verified_kinds,
        quarantined,
        notes,
        events,
    })
}

// ── review / approve_seal_diff ────────────────────────────────────────────

/// `review(record, previous?) → doc` — the SEP-1024 review document: the
/// install/exec plan lifted from the manifest, the surface-pin delta
/// against `previous` (the doc-level diff is `check_surface`'s), the
/// advisory scans and hygiene, and the minted `text_authority`/
/// `attestation_status`. The doc is canonical Json — `review` decides
/// nothing (advisory; T2).
pub fn review(record: &ExtensionRecord, previous: Option<&ExtensionRecord>) -> Json {
    let mut plan_members = vec![
        ("kind".to_string(), Json::str(record.kind.as_str())),
        ("name".to_string(), Json::str(record.name.clone())),
        ("content".to_string(), Json::str(record.content.id())),
        (
            "locator".to_string(),
            Json::obj([
                ("scheme", Json::str(record.locator.scheme.clone())),
                ("uri", Json::str(record.locator.credential_free_uri.clone())),
                (
                    "resolved",
                    record
                        .locator
                        .resolved
                        .clone()
                        .map_or(Json::Null, Json::str),
                ),
            ]),
        ),
        (
            "isolation".to_string(),
            Json::str(record.trust.isolation.as_str()),
        ),
        (
            "grants".to_string(),
            Json::Arr(
                record
                    .trust
                    .grants
                    .iter()
                    .map(|g| Json::str(g.version_id.clone()))
                    .collect(),
            ),
        ),
        (
            "declared_claims".to_string(),
            Json::Arr(
                record
                    .trust
                    .declared_claims
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("kind", Json::str(c.kind.as_str())),
                            ("value", c.value.clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
    ];
    // The executable plan — for a plugin manifest the contribution set and
    // the `requests` ceiling (the review's "what would run" half).
    if let Ok(manifest) = super::plugin::manifest_from_json(&record.manifest, "manifest") {
        plan_members.push((
            "executables".to_string(),
            Json::Arr(
                manifest
                    .contributions
                    .iter()
                    .filter(|c| c.executable.is_some())
                    .map(|c| {
                        let exe = c.executable.as_ref().unwrap();
                        Json::obj([
                            ("kind", Json::str(c.kind.as_str())),
                            ("code_pointer", Json::str(exe.code_pointer.id())),
                            ("isolation", Json::str(exe.isolation.as_str())),
                            (
                                "placement",
                                exe.placement_preference
                                    .map_or(Json::Null, |p| Json::str(p.as_str())),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ));
        plan_members.push((
            "requested_env_keys".to_string(),
            Json::Arr(
                manifest
                    .requests
                    .env_keys
                    .iter()
                    .map(|k| Json::str(k.clone()))
                    .collect(),
            ),
        ));
        plan_members.push(("manifest_id".to_string(), Json::str(manifest.identity.id())));
    }
    let surface_delta = match previous {
        Some(prev) => Json::obj([
            (
                "previous_pin",
                prev.trust
                    .surface_pin
                    .as_ref()
                    .map(|p| Json::str(p.id()))
                    .unwrap_or(Json::Null),
            ),
            (
                "current_pin",
                record
                    .trust
                    .surface_pin
                    .as_ref()
                    .map(|p| Json::str(p.id()))
                    .unwrap_or(Json::Null),
            ),
            (
                "pin_changed",
                Json::Bool(prev.trust.surface_pin != record.trust.surface_pin),
            ),
        ]),
        None => Json::Null,
    };
    Json::obj([
        ("schema", Json::str("hh-extension-review/1")),
        ("extension", Json::str(record.name.clone())),
        ("content", Json::str(record.content.id())),
        (
            "text_authority",
            Json::str(record.trust.text_authority.as_str()),
        ),
        (
            "attestation_status",
            record.trust.attestation_status.to_json(),
        ),
        ("status", Json::str(record.trust.status.as_str())),
        (
            "install_plan",
            Json::Obj(plan_members.into_iter().collect()),
        ),
        ("surface_delta", surface_delta),
        (
            "text_hygiene",
            Json::obj([
                (
                    "status",
                    Json::str(match record.trust.text_hygiene.status {
                        HygieneStatus::Clean => "clean",
                        HygieneStatus::Flagged => "flagged",
                        HygieneStatus::Unavailable => "unavailable",
                    }),
                ),
                (
                    "findings",
                    Json::Int(record.trust.text_hygiene.findings.len() as i64),
                ),
            ]),
        ),
    ])
}

/// `approve_seal_diff(diff_prov, diff_ref, endorser, permission_id, to)` —
/// the `approval` endorsement whose subject is the `HirDiff` that
/// adds/updates the extension (§6.2 §8.1; ADR-0064 D3). The approval
/// endorses the *edit-application intent* (`ContentKind::EffectIntent`),
/// `basis_ref` is the `permission_id`; the label rises `from` the diff's
/// minted label `to` the approved level — one endorsement of one edit; it
/// never raises `text_authority` (that is `pin`'s lane).
pub fn approve_seal_diff(
    diff_prov: &ProvenanceRecord,
    diff_ref: &str,
    endorser: &ProvenanceRecord,
    permission_id: &str,
    to_authority: AuthorityClass,
) -> Result<LabelEndorsed, EndorsementError> {
    let from = diff_prov.label();
    let to = Label {
        authority: to_authority,
        taint: from.taint.clone(),
        readers: from.readers.clone(),
    };
    endorse(
        diff_prov,
        diff_ref,
        &to,
        endorser,
        EndorsementBasis::Approval,
        Some(permission_id.to_string()),
        ContentKind::EffectIntent,
    )
}

// ── update / label regressions ────────────────────────────────────────────

/// A `LabelRegression` — a successor field strictly weaker than the
/// superseded record's (§6.2 `update`-mode review evidence; never a silent
/// drop).
#[derive(Debug, Clone, PartialEq)]
pub struct LabelRegression {
    /// The field.
    pub field: String,
    /// The superseded value.
    pub was: String,
    /// The successor value.
    pub now: String,
}

/// The update assessment over a `(superseded, successor)` pair: the
/// `extension_widening` verdict (grant/hook growth — `publish` enforces
/// `origin = human` + attestation + MAJOR label on `widening`) plus the
/// `LabelRegression` listing (authority/status/pin downgrades — warnings,
/// never silent).
#[derive(Debug, Clone)]
pub struct UpdateAssessment {
    /// `extension_widening(old, new)`.
    pub widening: bool,
    /// The label regressions.
    pub regressions: Vec<LabelRegression>,
    /// `widening` ⇒ `origin = human` + attestation (the publish gate).
    pub requires_human: bool,
}

/// Rank an `AttestationStatus` for regression comparison.
fn status_rank(s: &AttestationStatus) -> u8 {
    match s {
        AttestationStatus::Verified(AttestationKind::Signature) => 4,
        AttestationStatus::Verified(AttestationKind::Seal) => 3,
        AttestationStatus::Verified(AttestationKind::Pin) => 2,
        AttestationStatus::Verified(AttestationKind::HashChain) => 1,
        AttestationStatus::Missing | AttestationStatus::Failed => 0,
    }
}

/// `update_assessment(old, new)` — the supersession review read (§8.1:
/// widened grants demand `origin = human` + attestation; label
/// regressions are listed as warnings, never silently accepted).
pub fn update_assessment(old: &ExtensionRecord, new: &ExtensionRecord) -> UpdateAssessment {
    let widening = crate::store::extension_widening(old, new);
    let mut regressions = Vec::new();
    if new.trust.text_authority < old.trust.text_authority {
        regressions.push(LabelRegression {
            field: "trust.text_authority".to_string(),
            was: old.trust.text_authority.as_str().to_string(),
            now: new.trust.text_authority.as_str().to_string(),
        });
    }
    if status_rank(&new.trust.attestation_status) < status_rank(&old.trust.attestation_status) {
        regressions.push(LabelRegression {
            field: "trust.attestation_status".to_string(),
            was: format!("{:?}", old.trust.attestation_status),
            now: format!("{:?}", new.trust.attestation_status),
        });
    }
    if old.trust.surface_pin.is_some() && new.trust.surface_pin.is_none() {
        regressions.push(LabelRegression {
            field: "trust.surface_pin".to_string(),
            was: "pinned".to_string(),
            now: "unpinned".to_string(),
        });
    }
    if new.trust.isolation > old.trust.isolation {
        regressions.push(LabelRegression {
            field: "trust.isolation".to_string(),
            was: old.trust.isolation.as_str().to_string(),
            now: new.trust.isolation.as_str().to_string(),
        });
    }
    UpdateAssessment {
        widening,
        regressions,
        requires_human: widening,
    }
}

// ── model-install Procedure (AC-R-2.8.5-7) ────────────────────────────────

/// The proposer's context at install: its provenance (the authority floor,
/// the `installed_by` record) and its own conferred grant set — the
/// ceiling the install request may never exceed.
#[derive(Debug, Clone)]
pub struct ProposerContext {
    /// The proposer's `ProvenanceRecord` (a model/agent origin).
    pub provenance: ProvenanceRecord,
    /// The proposer's granted effect-domain spellings (`domain:scope`).
    pub grants: BTreeSet<String>,
}

/// The install plan — `install_plan`'s product: the
/// `security.extension.install_requested` payload (emitted *before* any
/// fetch), the Π effect descriptor, and whether the resolved
/// `permission_id` is required before `install_completed` may mint.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// The `security.extension.install_requested` payload.
    pub install_requested: Json,
    /// The install effect descriptor `{capability, args, risk_class}` —
    /// the Π proposal (`approval` endorses *this*, never the content).
    pub effect_descriptor: Json,
    /// Whether a resolved `permission_id` is required before
    /// `install_completed` may mint the record (`model_install = ask`).
    pub requires_approval: bool,
    /// The grants the install requests (checked ⊆ proposer's).
    pub requested_grants: BTreeSet<String>,
}

/// `install_plan(candidate, outcome, requested_grants, proposer, view)` —
/// the model-instruction install plan (AC-7). Emits the
/// `security.extension.install_requested` payload *before* any fetch —
/// `outcome` carries the declared content expectation; nothing is
/// fetched here.
///
/// - `model_install = deny` → `ModelInstallDenied`.
/// - `requested_grants ⊄ proposer.grants` → `AuthorityWidening` (a model
///   instruction can never widen authority — the hard refusal).
/// - `model_install = ask` → `requires_approval` (the install rides Π as
///   an ordinary effect proposal — the edit itself is `auto_L2`-class:
///   at most the proposer's authority, and tainted).
pub fn install_plan(
    candidate: &Candidate,
    outcome: &FetchOutcome,
    requested_grants: BTreeSet<String>,
    proposer: &ProposerContext,
    view: &TrustView,
) -> Result<InstallPlan, RegistryError> {
    if view.model_install == ModelInstallRule::Deny {
        return Err(RegistryError::ModelInstallDenied {
            detail: format!(
                "model_install = deny — `install` of `{}` refused",
                candidate.name
            ),
        });
    }
    // The widening gate — every requested grant must already be the
    // proposer's (the install is a subset projection, never a conferral).
    for g in &requested_grants {
        if !proposer.grants.contains(g) {
            return Err(RegistryError::AuthorityWidening {
                detail: format!("install requests grant `{g}` outside the proposer's grant set"),
            });
        }
    }
    let install_requested = Json::obj([
        ("class", Json::str("security.extension.install_requested")),
        ("extension_name", Json::str(candidate.name.clone())),
        ("kind", Json::str(candidate.kind.as_str())),
        (
            "proposer",
            Json::str(origin_ref(&proposer.provenance.origin)),
        ),
        (
            "proposer_authority",
            Json::str(proposer.provenance.authority.as_str()),
        ),
        ("content", Json::str(outcome.content.id())),
        ("resolved", Json::str(outcome.resolved.clone())),
        (
            "requested_grants",
            Json::Arr(
                requested_grants
                    .iter()
                    .map(|g| Json::str(g.clone()))
                    .collect(),
            ),
        ),
    ]);
    let effect_descriptor = Json::obj([
        (
            "capability",
            Json::str(format!("extension.install/{}", candidate.kind.as_str())),
        ),
        (
            "args",
            Json::obj([
                ("name", Json::str(candidate.name.clone())),
                ("content", Json::str(outcome.content.id())),
                ("resolved", Json::str(outcome.resolved.clone())),
            ]),
        ),
        (
            "risk_class",
            Json::str(if view.model_install == ModelInstallRule::Ask {
                "irreversible"
            } else {
                "reversible"
            }),
        ),
    ]);
    Ok(InstallPlan {
        install_requested,
        effect_descriptor,
        requires_approval: view.model_install == ModelInstallRule::Ask,
        requested_grants,
    })
}

/// The `install_completed` product — the attenuated+tainted record plus
/// every `security.extension.*` payload the install minted, in lifecycle
/// order (`resolved`, `quarantined?`, `install_completed`).
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// The installed `ExtensionRecord`.
    pub record: ExtensionRecord,
    /// The minted event payloads (the caller appends through the run's
    /// fenced writer — evidence, never a pretended append).
    pub events: Vec<Json>,
}

/// `install_completed(candidate, outcome, attestations, payload, manifest,
/// proposer, view, approved, now)` — mint the installed `ExtensionRecord`:
/// `min(external, proposer)` authority under the proposer's own model
/// origin (the record's `provenance.origin` is the proposer's — honest
/// derivation, never an import disguise), tainted `{model, source,
/// instructing_content}` (`participant:<proposer>` + `import:<source>`
/// + `import:instruction` — the closed `TaintTag` sum's honest
///   spellings), `installed_by` = the proposer's coordinate.
///
/// `approved` is the resolved `permission_id` when the plan required
/// approval — `install_completed` refuses when `model_install = ask`
/// and none is presented. A verified `signature` attestation is recorded
/// as evidence but cannot lift a model-instructed install above
/// `external` (`pin` is the definition lane, never install's).
#[allow(clippy::too_many_arguments)] // the install record's members are the call's arity.
pub fn install_completed(
    candidate: &Candidate,
    outcome: &FetchOutcome,
    attestations: &[Attestation],
    payload: Option<&[u8]>,
    manifest: Option<&Json>,
    proposer: &ProposerContext,
    view: &TrustView,
    approved: Option<&str>,
    now: u64,
) -> Result<InstallOutcome, RegistryError> {
    let plan = install_plan(candidate, outcome, BTreeSet::new(), proposer, view)?;
    if plan.requires_approval && approved.is_none() {
        return Err(RegistryError::SchemaViolation {
            path: "install.approved".to_string(),
            detail: "model_install = ask — install_completed requires a resolved permission_id"
                .to_string(),
        });
    }
    // The record mints under the proposer's own origin (a model install is
    // model-authored content — `resolve_extension` mints `external` text
    // authority for it).
    let report = resolve_extension(
        candidate,
        outcome,
        attestations,
        payload,
        manifest,
        proposer.provenance.origin.clone(),
        view,
        now,
    )?;
    let mut record = report.record;
    let mut events = report.events;
    // A signature verification is evidence, not elevation, on this path:
    // degrade the status to the strongest non-signature verified kind so
    // the minted `text_authority` is the hash-pin floor (`external`) —
    // minted-equality (`check_text_authority`) then holds at `register`.
    if record.trust.attestation_status == AttestationStatus::Verified(AttestationKind::Signature) {
        record.trust.attestation_status = if report.verified_kinds.contains("pin") {
            AttestationStatus::Verified(AttestationKind::Pin)
        } else if report.verified_kinds.contains("hash_chain") {
            AttestationStatus::Verified(AttestationKind::HashChain)
        } else {
            AttestationStatus::Missing
        };
        record.trust.text_authority = default_text_authority(
            &candidate.kind,
            &record.provenance.origin,
            &record.trust.attestation_status,
        );
    }
    // Taint ⇒ `authority ≤ external` on the record's provenance label —
    // the *derived* post-taint authority is `min(origin_minted, external)`
    // (the taint rule, applied at mint, never a post-hoc declaration).
    record.provenance.taint.insert(TaintTag::Participant {
        participant: origin_ref(&proposer.provenance.origin),
    });
    record.provenance.taint.insert(TaintTag::Import {
        source_system: candidate.source.kind_str().to_string(),
    });
    record.provenance.taint.insert(TaintTag::Import {
        source_system: "instruction".to_string(),
    });
    record.provenance.authority = record.provenance.authority.min(AuthorityClass::External);
    record.trust.installed_by = Some(origin_ref(&proposer.provenance.origin));
    record.trust.installed_at = Some(now);
    // The attenuated+tainted record re-clears the leg boundary before the
    // completion row mints (L2: a delegate-origin record confers nothing —
    // `grants` is empty on this path by construction, the check is the teeth
    // for every conferral leg that lands after this slice).
    enforce_leg_boundaries(&record)?;
    let install_completed_event = Json::obj([
        ("class", Json::str("security.extension.install_completed")),
        ("extension_name", Json::str(record.name.clone())),
        ("content", Json::str(record.content.id())),
        (
            "installed_by",
            Json::str(origin_ref(&proposer.provenance.origin)),
        ),
        ("approved", approved.map_or(Json::Null, Json::str)),
        (
            "text_authority",
            Json::str(record.trust.text_authority.as_str()),
        ),
    ]);
    events.push(install_completed_event);
    Ok(InstallOutcome { record, events })
}

// ── surface pin + drift (AC-R-2.8.5-4) ────────────────────────────────────

/// `SurfaceDriftPolicy` — the sealed definition's continuation rule under
/// a changed MCP surface (§6.2 `check_surface`; distinct from
/// `TrustRootPolicy.drift_policy`, which governs the *signer set*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceDriftPolicy {
    /// Report `security.extension.drift`; the changed tools are suppressed
    /// and the run continues.
    Notify,
    /// Report drift; the *whole surface* pauses (no tool from it delivers)
    /// until a human re-pins.
    PauseSurface,
    /// Report drift; the lane/run terminates (the surface is untrusted
    /// until re-pinned).
    Terminate,
}

impl SurfaceDriftPolicy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SurfaceDriftPolicy::Notify => "notify",
            SurfaceDriftPolicy::PauseSurface => "pause_surface",
            SurfaceDriftPolicy::Terminate => "terminate",
        }
    }
    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<SurfaceDriftPolicy> {
        match s {
            "notify" => Some(SurfaceDriftPolicy::Notify),
            "pause_surface" => Some(SurfaceDriftPolicy::PauseSurface),
            "terminate" => Some(SurfaceDriftPolicy::Terminate),
            _ => None,
        }
    }
}

/// Lift an `hh-mcp-listing/1` document to a `HirDocument` — one
/// `ToolCapability` node per wire tool (`import::lift_tool`'s honest
/// lift); the root ref is `pinned("surface:<server_ref>", listing_hash)`
/// (the listing hash is the pin's anchor — ADR-0088 D3). This is the
/// `SurfaceDocument` the `surface_pin` ContentAddress covers.
pub fn surface_document(doc: &Json) -> Result<HirDocument, RegistryError> {
    let (server_ref, tools) = import::parse_listing(doc)?;
    let listing_hash = doc
        .get("listing_hash")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let root = hh_hir::refs::Ref::pinned(format!("surface:{server_ref}"), listing_hash);
    let mut hir = HirDocument::new(root);
    for (i, wire) in tools.iter().enumerate() {
        let (rec, _carried) = import::lift_tool(&server_ref, wire, i as u64);
        let mut node = Node::new(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(rec),
            import::import_prov(i as u64),
        );
        node.ext.insert(
            "surface.name".to_string(),
            wire.get("name")
                .and_then(Json::as_str)
                .map_or(Json::Null, Json::str),
        );
        hir.nodes.push(node);
    }
    Ok(hir)
}

/// The `check_surface` drift evidence — a `HirDiff` over the lifted
/// surface pair plus the per-name delta and the
/// `security.extension.drift` payload (AC-4: the change is a HirDiff,
/// never a string diff).
#[derive(Debug)]
pub struct SurfaceDrift {
    /// The extension/server ref.
    pub extension_id: String,
    /// The pinned document's `listing_hash`.
    pub pinned_listing_hash: String,
    /// The live document's `listing_hash`.
    pub live_listing_hash: String,
    /// The `HirDiff` (`base` = pinned, `target` = live).
    pub diff: HirDiff,
    /// Tools added since the pin.
    pub added: Vec<String>,
    /// Tools removed since the pin.
    pub removed: Vec<String>,
    /// Tools whose `listing_hash` changed.
    pub changed: Vec<String>,
    /// The `security.extension.drift` event payload (content-free — refs
    /// and spellings only).
    pub event: Json,
}

/// `check_surface(pinned, live, observer, extension_id, run_ref)` — the
/// pin-vs-live surface check. `Ok(listing_hash)` on an unchanged surface;
/// `Err(SurfaceDrift)` when the `HirDiff` is non-empty or the listing
/// hash moved. The diff is computed by the gate-free `ops_between` read —
/// drift is *evidence*, not an applied edit (§3.1.7's gates protect
/// `apply`, not the diff's existence).
#[allow(clippy::result_large_err)] // drift carries the full diff — it is the evidence.
pub fn check_surface(
    pinned: &Json,
    live: &Json,
    observer: &ProvenanceRecord,
    extension_id: &str,
    run_ref: Option<&str>,
) -> Result<String, SurfaceDrift> {
    let base_doc = match surface_document(pinned) {
        Ok(d) => d,
        Err(e) => return Err(SurfaceDrift::malformed(extension_id, pinned, live, e)),
    };
    let live_doc = match surface_document(live) {
        Ok(d) => d,
        Err(e) => return Err(SurfaceDrift::malformed(extension_id, pinned, live, e)),
    };
    let pinned_hash = pinned
        .get("listing_hash")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let live_hash = live
        .get("listing_hash")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let (ops, classification) = ops_between(&base_doc, &live_doc);
    if ops.is_empty() && pinned_hash == live_hash {
        return Ok(live_hash);
    }
    let (added, removed, changed) = import::listing_delta_names(pinned, live);
    let diff = HirDiff {
        base: DefinitionVersionRef {
            semantic_id: format!("surface:{extension_id}"),
            version_id: pinned_hash.clone(),
        },
        target: DefinitionVersionRef {
            semantic_id: format!("surface:{extension_id}"),
            version_id: live_hash.clone(),
        },
        dialect: "HIR/1".to_string(),
        ops,
        classification: classification.clone(),
        provenance: observer.clone(),
        derivation: DiffDerivation {
            hypothesis: None,
            trajectories: run_ref
                .map(|r| vec![RunRef { run: r.to_string() }])
                .unwrap_or_default(),
            candidate_id: None,
        },
    };
    let diff_ref = idp_id("surface.drift.diff", diff.canonical_bytes().as_slice());
    let event = Json::obj([
        ("class", Json::str("security.extension.drift")),
        ("extension_id", Json::str(extension_id.to_string())),
        ("pinned_listing_hash", Json::str(pinned_hash.clone())),
        ("live_listing_hash", Json::str(live_hash.clone())),
        ("diff_ref", Json::str(diff_ref)),
        (
            "classification",
            Json::obj([
                (
                    "semantic_ops",
                    Json::Int(classification.semantic_ops as i64),
                ),
                ("surface_ops", Json::Int(classification.surface_ops as i64)),
                (
                    "authority_delta",
                    Json::str(match classification.authority_delta {
                        hh_hir::diff::AuthorityDelta::None => "none",
                        hh_hir::diff::AuthorityDelta::Narrowing => "narrowing",
                        hh_hir::diff::AuthorityDelta::Widening => "widening",
                    }),
                ),
            ]),
        ),
        (
            "changed_tools",
            Json::Arr(changed.iter().map(|t| Json::str(t.clone())).collect()),
        ),
        (
            "added_tools",
            Json::Arr(added.iter().map(|t| Json::str(t.clone())).collect()),
        ),
        (
            "removed_tools",
            Json::Arr(removed.iter().map(|t| Json::str(t.clone())).collect()),
        ),
        ("checked_by", Json::str(observer.origin.tag())),
    ]);
    Err(SurfaceDrift {
        extension_id: extension_id.to_string(),
        pinned_listing_hash: pinned_hash,
        live_listing_hash: live_hash,
        diff,
        added,
        removed,
        changed,
        event,
    })
}

impl SurfaceDrift {
    /// A malformed lift is still drift evidence — the surface changed
    /// shape (or was never a listing); the record carries the parse
    /// failure, `added/removed/changed` empty.
    fn malformed(extension_id: &str, pinned: &Json, live: &Json, e: RegistryError) -> SurfaceDrift {
        let pinned_hash = pinned
            .get("listing_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let live_hash = live
            .get("listing_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let diff = HirDiff {
            base: DefinitionVersionRef {
                semantic_id: format!("surface:{extension_id}"),
                version_id: pinned_hash.clone(),
            },
            target: DefinitionVersionRef {
                semantic_id: format!("surface:{extension_id}"),
                version_id: live_hash.clone(),
            },
            dialect: "HIR/1".to_string(),
            ops: Vec::new(),
            classification: DiffClassification {
                semantic_ops: 0,
                surface_ops: 0,
                provenance_only_ops: 0,
                ext_ops: 0,
                authority_delta: hh_hir::diff::AuthorityDelta::None,
                budget_delta: hh_hir::diff::Delta::None,
                validity_delta: hh_hir::diff::Delta::None,
                coordination_delta: hh_hir::diff::Delta::None,
                touches_conditioned_rules: Vec::new(),
            },
            provenance: ProvenanceRecord::minted(
                Origin::kernel("registry.check_surface"),
                hh_provenance::PersistenceScope::Run,
                0,
            ),
            derivation: DiffDerivation::default(),
        };
        SurfaceDrift {
            extension_id: extension_id.to_string(),
            pinned_listing_hash: pinned_hash,
            live_listing_hash: live_hash,
            diff,
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            event: Json::obj([
                ("class", Json::str("security.extension.drift")),
                ("extension_id", Json::str(extension_id.to_string())),
                ("malformed", Json::str(format!("{e}"))),
            ]),
        }
    }
}

/// The delivery decision for a drifted surface — the drift policy's
/// continuation rule applied to the live tool set.
#[derive(Debug, Clone)]
pub struct DriftDecision {
    /// Tool names suppressed from delivery.
    pub suppress: Vec<String>,
    /// Whether the surface pauses entirely.
    pub pause_surface: bool,
    /// Whether the lane/run terminates.
    pub terminate: bool,
    /// The policy applied.
    pub policy: SurfaceDriftPolicy,
}

/// `drift_decision(drift, policy, live_tools)` — `notify` suppresses the
/// changed *and* added tools (the pin still covers the unchanged set; an
/// added tool is unreviewed new surface — never delivered before
/// re-pin); `pause_surface` suppresses every tool the live doc serves;
/// `terminate` suppresses everything and the lane dies.
pub fn drift_decision(
    drift: &SurfaceDrift,
    policy: SurfaceDriftPolicy,
    live_tools: &[String],
) -> DriftDecision {
    match policy {
        SurfaceDriftPolicy::Notify => {
            let mut suppress = drift.changed.clone();
            suppress.extend(drift.added.iter().cloned());
            suppress.sort();
            suppress.dedup();
            DriftDecision {
                suppress,
                pause_surface: false,
                terminate: false,
                policy,
            }
        }
        SurfaceDriftPolicy::PauseSurface => DriftDecision {
            suppress: live_tools.to_vec(),
            pause_surface: true,
            terminate: false,
            policy,
        },
        SurfaceDriftPolicy::Terminate => DriftDecision {
            suppress: live_tools.to_vec(),
            pause_surface: true,
            terminate: true,
            policy,
        },
    }
}

// ── revocation propagation (AC-R-2.8.5-8) ─────────────────────────────────

/// The run-side view `propagate_revocation` reads — the live state the
/// revocation must reach (all by `version_id` — names are never parsed).
#[derive(Debug, Clone, Default)]
pub struct RunExtensionView {
    /// Running server/variant lanes bound to extension version ids
    /// (`lane_ref → version_id`).
    pub servers: Vec<(String, String)>,
    /// Delivered surfaces (`version_id → tool names delivered from it`).
    pub surfaces: Vec<(String, Vec<String>)>,
    /// Live grants minted from the extension (`grant_id → version_id`).
    pub grants: Vec<(String, String)>,
    /// In-flight effects (`effect_id → version_id`).
    pub in_flight: Vec<(String, String)>,
    /// Result rows citing extension members (`result_ref → member vids`).
    pub results: Vec<(String, Vec<String>)>,
}

/// The propagation plan a revocation produces — every member is an
/// *instruction* the caller enforces (append the events through the run's
/// fenced writer; terminate the lanes; drop the tools; the grant rows are
/// what `resolve(execute)` then refuses `Stale`/`Revoked`).
#[derive(Debug, Clone)]
pub struct RevocationPropagation {
    /// The `security.extension.revoked` payload.
    pub revoked_event: Json,
    /// Lanes to terminate (`lane_ref`s).
    pub terminations: Vec<String>,
    /// Tool names removed from the next assembled context.
    pub dropped_surface_tools: Vec<String>,
    /// Grant ids whose resolution is now deny.
    pub denied_grants: Vec<String>,
    /// `action.effect.unknown` payloads for the in-flight effects.
    pub unknown_effects: Vec<Json>,
    /// Result refs annotated `depends_on_revoked = true` (their member
    /// sets reach the revoked version).
    pub depends_on_revoked: Vec<String>,
    /// The stale-derived dependants (`mark_stale_derived` targets —
    /// versions whose declared `depends_on` reaches the revoked member,
    /// or whose signer attestation a policy supersession dropped).
    pub stale_dependants: Vec<String>,
}

/// `propagate_revocation(revoked_vid, reason, stale_dependants, view,
/// checked_by)` — the AC-8 plan. `stale_dependants` is the store-side
/// derived set (`stale_for` over the revoked member's dependants — the
/// caller supplies it so the pure op never reaches into the store).
pub fn propagate_revocation(
    revoked_version_id: &str,
    reason: &str,
    stale_dependants: &[String],
    view: &RunExtensionView,
    checked_by: &ProvenanceRecord,
) -> RevocationPropagation {
    let affected: BTreeSet<&str> = std::iter::once(revoked_version_id)
        .chain(stale_dependants.iter().map(|s| s.as_str()))
        .collect();
    let terminations: Vec<String> = view
        .servers
        .iter()
        .filter(|(_, v)| affected.contains(v.as_str()))
        .map(|(lane, _)| lane.clone())
        .collect();
    let mut dropped_surface_tools: Vec<String> = Vec::new();
    for (v, tools) in &view.surfaces {
        if affected.contains(v.as_str()) {
            dropped_surface_tools.extend(tools.iter().cloned());
        }
    }
    dropped_surface_tools.sort();
    dropped_surface_tools.dedup();
    let denied_grants: Vec<String> = view
        .grants
        .iter()
        .filter(|(_, v)| affected.contains(v.as_str()))
        .map(|(g, _)| g.clone())
        .collect();
    let unknown_effects: Vec<Json> = view
        .in_flight
        .iter()
        .filter(|(_, v)| affected.contains(v.as_str()))
        .map(|(eid, _)| {
            Json::obj([
                ("class", Json::str("action.effect.unknown")),
                ("effect_id", Json::str(eid.clone())),
                ("cause", Json::str("extension_revoked")),
                ("revoked", Json::str(revoked_version_id)),
            ])
        })
        .collect();
    let depends_on_revoked: Vec<String> = view
        .results
        .iter()
        .filter(|(_, members)| members.iter().any(|m| affected.contains(m.as_str())))
        .map(|(r, _)| r.clone())
        .collect();
    let revoked_event = Json::obj([
        ("class", Json::str("security.extension.revoked")),
        ("extension_id", Json::str(revoked_version_id)),
        ("reason", Json::str(reason)),
        ("checked_by", Json::str(checked_by.origin.tag())),
        (
            "terminated_lanes",
            Json::Arr(terminations.iter().map(|t| Json::str(t.clone())).collect()),
        ),
        (
            "dropped_tools",
            Json::Arr(
                dropped_surface_tools
                    .iter()
                    .map(|t| Json::str(t.clone()))
                    .collect(),
            ),
        ),
        (
            "denied_grants",
            Json::Arr(denied_grants.iter().map(|g| Json::str(g.clone())).collect()),
        ),
        ("unknown_effects", Json::Int(unknown_effects.len() as i64)),
        (
            "stale_dependants",
            Json::Arr(
                stale_dependants
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        ),
    ]);
    RevocationPropagation {
        revoked_event,
        terminations,
        dropped_surface_tools,
        denied_grants,
        unknown_effects,
        depends_on_revoked,
        stale_dependants: stale_dependants.to_vec(),
    }
}

// ── seal / activate (R2.20 — the `sealed`/`loaded` producers) ───────────────

/// The `seal_extension` product — the sealed record plus the durable
/// `security.extension.sealed{extension_id, trust_snapshot_ref}` payload
/// (§5g.5 §2 `seal` row; R2.20).
#[derive(Debug, Clone)]
pub struct SealOutcome {
    /// The sealed `ExtensionRecord` (`trust.status = sealed`; the
    /// `trust_snapshot_ref` is recorded under `ext` — a coordinate, never
    /// an admission input).
    pub record: ExtensionRecord,
    /// The minted `security.extension.sealed` payload.
    pub sealed_event: Json,
}

/// `seal_extension(record, extension_id, trust_snapshot_ref, authority_cap)` —
/// the record-level seal gate (§5g.5 §2 `seal` row): `status ≠ resolved`
/// refuses (a quarantined/revoked/loaded record never seals — the durable
/// status row is the verdict), the locator must be pinned (L4 — no selector
/// survives into a sealed form), the leg boundaries re-run (L1–L3), and
/// `grants ⊄ authority_cap` is `AuthorityWidening` (the cap is the
/// definition's declared ceiling — a grant outside it never seals). On
/// success the record's `trust.status` lifts to `Sealed` and the
/// `security.extension.sealed` payload mints — the caller appends it through
/// the run's fenced writer.
pub fn seal_extension(
    record: &ExtensionRecord,
    extension_id: &str,
    trust_snapshot_ref: &str,
    authority_cap: &BTreeSet<String>,
) -> Result<SealOutcome, RegistryError> {
    if record.trust.status != TrustStatus::Resolved {
        return Err(RegistryError::SchemaViolation {
            path: "extension.trust.status".to_string(),
            detail: format!(
                "seal requires status = resolved (found {}) — a quarantined/revoked \
                 record never seals",
                record.trust.status.as_str()
            ),
        });
    }
    if !record.locator.is_pinned() {
        return Err(RegistryError::SchemaViolation {
            path: "extension.locator".to_string(),
            detail: "UnpinnedInSealedForm: seal requires a pinned locator (resolved + fetched_at, \
                 no surviving selector)"
                .to_string(),
        });
    }
    enforce_leg_boundaries(record)?;
    for g in &record.trust.grants {
        if !authority_cap.contains(&g.version_id) {
            return Err(RegistryError::AuthorityWidening {
                detail: format!(
                    "seal: grant `{}` is outside the declared authority_cap",
                    g.version_id
                ),
            });
        }
    }
    let mut sealed = record.clone();
    sealed.trust.status = TrustStatus::Sealed;
    sealed.ext.insert(
        "trust_snapshot_ref".to_string(),
        Json::str(trust_snapshot_ref),
    );
    let sealed_event = Json::obj([
        ("class", Json::str("security.extension.sealed")),
        ("extension_id", Json::str(extension_id.to_string())),
        ("extension_name", Json::str(record.name.clone())),
        ("content", Json::str(record.content.id())),
        (
            "trust_snapshot_ref",
            Json::str(trust_snapshot_ref.to_string()),
        ),
        (
            "grants",
            Json::Arr(
                record
                    .trust
                    .grants
                    .iter()
                    .map(|g| Json::str(g.version_id.clone()))
                    .collect(),
            ),
        ),
    ]);
    Ok(SealOutcome {
        record: sealed,
        sealed_event,
    })
}

/// The `activate_extension` product — the `security.extension.loaded`
/// payload (minted only when the run may load the extension), the surface
/// drift evidence and the delivery decision when a pinned surface moved.
#[derive(Debug)]
pub struct Activation {
    /// The `security.extension.loaded{extension_id, isolation, pin_check}`
    /// payload — `None` when the drift decision suppresses the load
    /// (`pause_surface`/`terminate`) or the record is unpinned.
    pub loaded_event: Option<Json>,
    /// The `SurfaceDrift` evidence when the live listing moved (the
    /// `security.extension.drift` payload rides it).
    pub drift: Option<SurfaceDrift>,
    /// The delivery decision over the drifted surface.
    pub decision: Option<DriftDecision>,
    /// The `pin_check` verdict the loaded event reports
    /// (`pinned` | `drifted`).
    pub pin_check: String,
}

/// `activate_extension(record, extension_id, pinned_listing, live_listing,
/// drift_policy, observer, run_ref)` — the run-time activation leg (§5g.5 §2
/// `activate` row; R2.20):
///
/// - `status ∈ {resolved, sealed}` — `revoked` is `Revoked`;
///   `quarantined` refuses (a quarantined record never loads — the
///   quarantine row is the verdict).
/// - L4 pin check — a locator without `resolved`/`fetched_at` or a missing
///   `content` pin never activates (`UnpinnedInSealedForm`).
/// - `pinned_listing`/`live_listing` — when the record carries a
///   `surface_pin` and the caller supplies both listings, the pin-vs-live
///   `check_surface` runs and `drift_policy` decides delivery: `notify`
///   loads with the changed/added tools suppressed; `pause_surface`/
///   `terminate` mint no `loaded` row (the drift row + decision are the
///   evidence — the surface delivers nothing).
/// - the minted `security.extension.loaded` carries `{extension_id,
///   isolation, pin_check}` — the Rule-O pin attestation the run's
///   producer obligations resolve against.
#[allow(clippy::too_many_arguments)] // the activation inputs are the call's arity.
pub fn activate_extension(
    record: &ExtensionRecord,
    extension_id: &str,
    pinned_listing: Option<&Json>,
    live_listing: Option<&Json>,
    drift_policy: SurfaceDriftPolicy,
    observer: &ProvenanceRecord,
    run_ref: Option<&str>,
) -> Result<Activation, RegistryError> {
    match record.trust.status {
        TrustStatus::Resolved | TrustStatus::Sealed => {}
        TrustStatus::Revoked => {
            return Err(RegistryError::Revoked {
                version_id: extension_id.to_string(),
                reason: "extension revoked — a revoked record never activates".to_string(),
            });
        }
        TrustStatus::Quarantined => {
            return Err(RegistryError::SchemaViolation {
                path: "extension.trust.status".to_string(),
                detail:
                    "a quarantined extension never activates (the quarantine row is the verdict)"
                        .to_string(),
            });
        }
    }
    if !record.locator.is_pinned() {
        return Err(RegistryError::SchemaViolation {
            path: "extension.locator".to_string(),
            detail: "UnpinnedInSealedForm: activate requires a pinned locator — a selector or \
                 unfetched coordinate never loads"
                .to_string(),
        });
    }
    enforce_leg_boundaries(record)?;
    let mut pin_check = "pinned".to_string();
    let mut drift = None;
    let mut decision = None;
    let mut admit = true;
    if record.trust.surface_pin.is_some() {
        if let (Some(pinned), Some(live)) = (pinned_listing, live_listing) {
            match check_surface(pinned, live, observer, extension_id, run_ref) {
                Ok(_hash) => {}
                Err(d) => {
                    let tools: Vec<String> = crate::import::parse_listing(live)
                        .map(|(_, ts)| {
                            ts.iter()
                                .filter_map(|t| {
                                    t.get("name")
                                        .or_else(|| t.get("tool").and_then(|tt| tt.get("name")))
                                        .and_then(Json::as_str)
                                        .map(str::to_string)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let dec = drift_decision(&d, drift_policy, &tools);
                    pin_check = "drifted".to_string();
                    admit = !(dec.pause_surface || dec.terminate);
                    drift = Some(d);
                    decision = Some(dec);
                }
            }
        }
    }
    let loaded_event = if admit {
        Some(Json::obj([
            ("class", Json::str("security.extension.loaded")),
            ("extension_id", Json::str(extension_id.to_string())),
            ("extension_name", Json::str(record.name.clone())),
            ("isolation", Json::str(record.trust.isolation.as_str())),
            ("pin_check", Json::str(pin_check.clone())),
            // The pin attestation (§5g.5 `loaded{…, pin attestation}` —
            // Rule O resolves `producer.component_variant_ref` against it):
            // the content pin + the resolved locator coordinate the run
            // loaded against.
            ("content", Json::str(record.content.id())),
            (
                "resolved",
                record
                    .locator
                    .resolved
                    .clone()
                    .map_or(Json::Null, Json::str),
            ),
            // Honest surface verdict — `clean` (re-check ran, no drift),
            // `drifted` (the drift decision admitted under `notify`), or
            // `not_supplied` (the record pins a surface but the caller
            // offered no live listing — the re-check did not run; never
            // report a check that did not happen).
            (
                "surface_check",
                Json::str(if record.trust.surface_pin.is_none() {
                    "no_surface_pin"
                } else if drift.is_some() {
                    "drifted"
                } else if pinned_listing.is_some() && live_listing.is_some() {
                    "clean"
                } else {
                    "not_supplied"
                }),
            ),
            ("checked_by", Json::str(observer.origin.tag())),
        ]))
    } else {
        None
    };
    Ok(Activation {
        loaded_event,
        drift,
        decision,
        pin_check,
    })
}
