//! # Extension supply-chain trust — the §5g.5 C0/Stage-1 slice (R-2.8.5⁰)
//!
//! The `ExtensionRecord`/`ExtensionTrustRecord` shapes, the `DeclaredSource` sum, the
//! resolve-time pin rule (`UnpinnedInSealedForm`), the location-neutral
//! `default_text_authority` minting table, and `TextHygieneReport` — the projection of
//! extension text into the canonical authority class set (§8.1 `AuthorityClass`). An
//! `ExtensionRecord` is a **registry-layer versioned record** (`RecordKind::Extension`),
//! never a HIR entity — the registry is its storage layer (CC10).
//!
//! What this module does NOT own (spec §5g.5 §9 + the decomposed manifest):
//! - the trust-root/signature install machinery (`trust_root_policy` exists; ADR-0210
//!   OQ-162 is open, C1 — Stage 1 works with hash pins);
//! - install procedures / UX / revocation propagation — S4.14a;
//! - the MCP discover/lift-and-pin driver and the `hh-mcp` bridge — S1.24/S2.5;
//! - event emission of the `security.extension.*` family (classes land in
//!   `hh-ledger::classes`; `loaded` already emits — S1.18; the rest land with their
//!   producers).

use std::collections::BTreeMap;

use hh_identity::idp::{self, ContentAddress};
use hh_identity::kinds::RecordKind as IdentityRecordKind;
use hh_identity::refs::VersionedRef;
use hh_provenance::authority::PersistenceScope;
use hh_provenance::origin::Origin;
use hh_provenance::record::{Attestation, AttestationAnchor, AttestationKind, ProvenanceRecord};
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::RegistryError;

/// The `PluginManifest/1` codec + admission gate (§8.4, R-2.12.2; S1.27).
pub mod plugin;

// ── ExtensionKind ─────────────────────────────────────────────────────────────

/// The `ExtensionKind` sum (§5g.5 §3): `skill | hook | plugin | mcp_server |
/// instruction_file | fetched_instruction | component_variant | <other>` —
/// namespaced string with the listed spellings reserved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtensionKind {
    /// An Agent Skills skill.
    Skill,
    /// A hook (a guard on a ControlBoundary — the hook *placement* extension kind).
    Hook,
    /// A plugin (a packaged set of extensions).
    Plugin,
    /// An MCP server.
    McpServer,
    /// An instruction file (CLAUDE.md/AGENTS.md class).
    InstructionFile,
    /// A fetched instruction (`fetch[instruction]` target — remote, untrusted).
    FetchedInstruction,
    /// A component variant (a registered class/variant — bridges to the component
    /// store).
    ComponentVariant,
    /// Any other namespaced kind (forward-compat member).
    Other(String),
}

impl ExtensionKind {
    /// The canonical spelling.
    pub fn as_str(&self) -> &str {
        match self {
            ExtensionKind::Skill => "skill",
            ExtensionKind::Hook => "hook",
            ExtensionKind::Plugin => "plugin",
            ExtensionKind::McpServer => "mcp_server",
            ExtensionKind::InstructionFile => "instruction_file",
            ExtensionKind::FetchedInstruction => "fetched_instruction",
            ExtensionKind::ComponentVariant => "component_variant",
            ExtensionKind::Other(s) => s.as_str(),
        }
    }

    /// Parse a spelling; unknown spellings round-trip through `Other`.
    pub fn parse(s: &str) -> ExtensionKind {
        match s {
            "skill" => ExtensionKind::Skill,
            "hook" => ExtensionKind::Hook,
            "plugin" => ExtensionKind::Plugin,
            "mcp_server" => ExtensionKind::McpServer,
            "instruction_file" => ExtensionKind::InstructionFile,
            "fetched_instruction" => ExtensionKind::FetchedInstruction,
            "component_variant" => ExtensionKind::ComponentVariant,
            other => ExtensionKind::Other(other.to_string()),
        }
    }
}

// ── DeclaredSource ────────────────────────────────────────────────────────────

/// `DeclaredSource` (§5g.5 §3) — the closed sum of extension sources:
/// `{directory_scan{root, scope}, marketplace{catalog_ref}, registry{name},
/// git{url, ref}, archive{url, sha}, mcp_endpoint{uri}, instruction_files{roots}}`.
/// Sources are **declared in the Assembly**, never discovered by implicit
/// directory-scan of the home/project directories (CF-079 — CC3).
#[derive(Debug, Clone, PartialEq)]
pub enum DeclaredSource {
    /// A declared directory scan `{root, scope}` — a "trusted folder" is a scope
    /// condition on this source, never an authority fact (L1).
    DirectoryScan {
        /// The declared root path.
        root: String,
        /// The persistence scope the root is declared under.
        scope: PersistenceScope,
    },
    /// A marketplace `{catalog_ref}`.
    Marketplace {
        /// The declared catalog reference.
        catalog_ref: String,
    },
    /// A registry `{name}`.
    Registry {
        /// The declared registry name.
        name: String,
    },
    /// A git source `{url, ref}`.
    Git {
        /// The clone URL.
        url: String,
        /// The declared ref (branch/tag/SHA selector — pinned at resolve).
        ref_: String,
    },
    /// An archive `{url, sha}`.
    Archive {
        /// The fetch URL.
        url: String,
        /// The declared SHA selector (pinned at resolve).
        sha: String,
    },
    /// An MCP endpoint `{uri}`.
    McpEndpoint {
        /// The endpoint URI.
        uri: String,
    },
    /// Instruction-file roots `{roots}` (the declared CLAUDE.md/AGENTS.md set —
    /// this is the *only* way instruction files reach the assembly).
    InstructionFiles {
        /// The declared roots.
        roots: Vec<String>,
    },
}

impl DeclaredSource {
    /// The canonical kind spelling.
    pub fn kind_str(&self) -> &'static str {
        match self {
            DeclaredSource::DirectoryScan { .. } => "directory_scan",
            DeclaredSource::Marketplace { .. } => "marketplace",
            DeclaredSource::Registry { .. } => "registry",
            DeclaredSource::Git { .. } => "git",
            DeclaredSource::Archive { .. } => "archive",
            DeclaredSource::McpEndpoint { .. } => "mcp_endpoint",
            DeclaredSource::InstructionFiles { .. } => "instruction_files",
        }
    }
}

// ── SourceLocator ─────────────────────────────────────────────────────────────

/// `SourceLocator` (§5g.5 §3): `{scheme, credential_free_uri, selector?,
/// resolved?, fetched_at?}` — the declared + resolved coordinates of an
/// extension's source. `selector`/`resolved`/`fetched_at` are `None` before
/// `resolve` (the resolver's job is to fill `resolved`/`fetched_at` and `content`;
/// L4: a selector surviving into a sealed form fails `UnpinnedInSealedForm`).
/// The URI is **credential-free** — a credential inside it is a CC3/secret-plane
/// violation (`RegistryError::SchemaViolation{path: "locator.credential_free_uri"}`).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocator {
    /// The locator scheme — the `DeclaredSource` kind spelling it resolves.
    pub scheme: String,
    /// The credential-free URI the candidate was discovered at.
    pub credential_free_uri: String,
    /// The declared selector (a version/ref selector — pinned at resolve).
    pub selector: Option<String>,
    /// The resolved coordinate (filled by `resolve`).
    pub resolved: Option<String>,
    /// The fetch stamp (the resolver's logical time).
    pub fetched_at: Option<u64>,
}

impl SourceLocator {
    /// True when the locator is pinned: a resolved coordinate + no surviving
    /// selector + a fetch stamp. L4 checks `is_pinned()` on every extension
    /// reference reaching a sealed form.
    pub fn is_pinned(&self) -> bool {
        self.selector.is_none() && self.resolved.is_some() && self.fetched_at.is_some()
    }

    /// The credential-free check (CC3): `userinfo` (`@` before the first `/` after
    /// the scheme) or a `password`/`token`/`key` query member is refused.
    pub fn validate_credential_free(&self, path: &str) -> Result<(), RegistryError> {
        let uri = &self.credential_free_uri;
        let authority = uri
            .split("://")
            .nth(1)
            .or_else(|| uri.split(':').next())
            .unwrap_or(uri);
        let before_path = authority.split('/').next().unwrap_or(authority);
        if before_path.contains('@') {
            return Err(RegistryError::SchemaViolation {
                path: format!("{path}.credential_free_uri"),
                detail: "credential userinfo in locator URI".to_string(),
            });
        }
        if let Some(query) = uri.split('?').nth(1) {
            for kv in query.split('&') {
                let k = kv.split('=').next().unwrap_or("").to_ascii_lowercase();
                if matches!(
                    k.as_str(),
                    "password" | "passwd" | "token" | "secret" | "key"
                ) {
                    return Err(RegistryError::SchemaViolation {
                        path: format!("{path}.credential_free_uri"),
                        detail: format!("credential-like query member `{k}` in locator URI"),
                    });
                }
            }
        }
        Ok(())
    }
}

// ── AttestationStatus ─────────────────────────────────────────────────────────

/// The attestation status the trust record carries (§5g.5 §3
/// `attestation_status: AttestationStatus`): the outcome of the kernel-side
/// `verify_attestation` — never self-declared (a payload's own claim is never
/// trusted; in-toto/SLSA signer–builder rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationStatus {
    /// A verified attestation of this kind (`signature` is the only route that
    /// raises `text_authority` to `definition`/`principal`; `pin`/`hash_chain`/
    /// `seal` are the hash-pin routes capped at `hash_only_ceiling`).
    Verified(AttestationKind),
    /// Verification failed.
    Failed,
    /// No attestation present.
    Missing,
}

impl AttestationStatus {
    /// The canonical spelling set.
    pub fn to_json(&self) -> Json {
        match self {
            AttestationStatus::Verified(k) => Json::obj([("verified", Json::str(k.as_str()))]),
            AttestationStatus::Failed => Json::str("failed"),
            AttestationStatus::Missing => Json::str("missing"),
        }
    }

    /// Decode.
    pub fn from_json(j: &Json, path: &str) -> Result<AttestationStatus, RegistryError> {
        match j {
            Json::Str(s) if s == "failed" => Ok(AttestationStatus::Failed),
            Json::Str(s) if s == "missing" => Ok(AttestationStatus::Missing),
            Json::Obj(m) => match m.get("verified").and_then(Json::as_str) {
                Some("seal") => Ok(AttestationStatus::Verified(AttestationKind::Seal)),
                Some("hash_chain") => Ok(AttestationStatus::Verified(AttestationKind::HashChain)),
                Some("signature") => Ok(AttestationStatus::Verified(AttestationKind::Signature)),
                Some("pin") => Ok(AttestationStatus::Verified(AttestationKind::Pin)),
                Some(other) => Err(RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: format!("unknown attestation kind {other}"),
                }),
                None => Err(RegistryError::SchemaViolation {
                    path: path.to_string(),
                    detail: "expected {verified: kind}".to_string(),
                }),
            },
            _ => Err(RegistryError::SchemaViolation {
                path: path.to_string(),
                detail: "expected attestation_status".to_string(),
            }),
        }
    }
}

// ── IsolationClass ────────────────────────────────────────────────────────────

/// The `IsolationClass` vocabulary (§5g.5 §3 `isolation` + ADR-0065 — the
/// placement-class vocabulary the trust record projects into containment
/// `Placement` rows at install): `{in_process, subprocess_confined, container,
/// remote, host_unconfined}`. The mechanism mapping to containment `Placement`
/// rows is R-2.8.4's (CF-291); this is the declared class, never an enforced
/// placement (isolation enforcement is Stage 2+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolationClass {
    /// Runs in the host process.
    InProcess,
    /// A confined subprocess.
    SubprocessConfined,
    /// A container.
    Container,
    /// A remote endpoint (e.g. a remote MCP server).
    Remote,
    /// Runs with the host's full ambient authority.
    HostUnconfined,
}

impl IsolationClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationClass::InProcess => "in_process",
            IsolationClass::SubprocessConfined => "subprocess_confined",
            IsolationClass::Container => "container",
            IsolationClass::Remote => "remote",
            IsolationClass::HostUnconfined => "host_unconfined",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<IsolationClass> {
        match s {
            "in_process" => Some(IsolationClass::InProcess),
            "subprocess_confined" => Some(IsolationClass::SubprocessConfined),
            "container" => Some(IsolationClass::Container),
            "remote" => Some(IsolationClass::Remote),
            "host_unconfined" => Some(IsolationClass::HostUnconfined),
            _ => None,
        }
    }
}

// ── TrustStatus ───────────────────────────────────────────────────────────────

/// The trust-record `status` sum (§5g.5 §3): `{resolved, sealed, quarantined,
/// revoked}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrustStatus {
    /// Resolved (pins filled, not yet sealed into a definition).
    Resolved,
    /// Sealed into a definition/run manifest.
    Sealed,
    /// Quarantined (admission pending review — the `foreign_import` treatment).
    Quarantined,
    /// Revoked (the trust anchor the record was verified under was revoked).
    Revoked,
}

impl TrustStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TrustStatus::Resolved => "resolved",
            TrustStatus::Sealed => "sealed",
            TrustStatus::Quarantined => "quarantined",
            TrustStatus::Revoked => "revoked",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<TrustStatus> {
        match s {
            "resolved" => Some(TrustStatus::Resolved),
            "sealed" => Some(TrustStatus::Sealed),
            "quarantined" => Some(TrustStatus::Quarantined),
            "revoked" => Some(TrustStatus::Revoked),
            _ => None,
        }
    }
}

// ── TextHygieneReport ─────────────────────────────────────────────────────────

/// The hygiene-finding kind (§5g.5 `TextHygieneReport` — "invisible/bidirectional/
/// tag code points and description–code mismatches").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HygieneFindingKind {
    /// A bidirectional control (RLO/LRO/PDF/embeddings/overrides, isolates).
    BidirectionalControl,
    /// An invisible codepoint (zero-widths, BOM/ZWNBSP, word joiners, soft hyphen).
    InvisibleCodepoint,
    /// A Unicode Tag codepoint (U+E0000–E007F).
    TagCodepoint,
    /// A description–code mismatch finding (what the text describes doesn't match
    /// the code it ships — an advisory scan outcome).
    DescriptionCodeMismatch,
}

impl HygieneFindingKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HygieneFindingKind::BidirectionalControl => "bidirectional_control",
            HygieneFindingKind::InvisibleCodepoint => "invisible_codepoint",
            HygieneFindingKind::TagCodepoint => "tag_codepoint",
            HygieneFindingKind::DescriptionCodeMismatch => "description_code_mismatch",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<HygieneFindingKind> {
        match s {
            "bidirectional_control" => Some(HygieneFindingKind::BidirectionalControl),
            "invisible_codepoint" => Some(HygieneFindingKind::InvisibleCodepoint),
            "tag_codepoint" => Some(HygieneFindingKind::TagCodepoint),
            "description_code_mismatch" => Some(HygieneFindingKind::DescriptionCodeMismatch),
            _ => None,
        }
    }
}

/// One `TextHygieneReport` finding: `{kind, offset, codepoint}` — the byte offset
/// and the offending codepoint's `U+XXXX` spelling (the report points at bytes;
/// it never rewrites them — advisory while preserving the original bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct HygieneFinding {
    /// The finding kind.
    pub kind: HygieneFindingKind,
    /// The byte offset of the codepoint in the scanned text.
    pub offset: u64,
    /// The `U+XXXX` spelling.
    pub codepoint: String,
}

/// `TextHygieneReport` (§5g.5): `{status ∈ {clean, flagged, unavailable},
/// findings[]}` — the **advisory** hygiene report on extension text; the
/// original bytes are always preserved (the report never rewrites the text).
/// `unavailable` means the scan couldn't run over this text (non-UTF-8 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct TextHygieneReport {
    /// The report status.
    pub status: HygieneStatus,
    /// The findings (empty for `clean`; non-empty for `flagged`).
    pub findings: Vec<HygieneFinding>,
}

/// The report status sum `{clean, flagged, unavailable}` — the same sum
/// `AdvisoryResult` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HygieneStatus {
    /// No findings.
    Clean,
    /// At least one finding.
    Flagged,
    /// The scan could not run over this text.
    Unavailable,
}

impl HygieneStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HygieneStatus::Clean => "clean",
            HygieneStatus::Flagged => "flagged",
            HygieneStatus::Unavailable => "unavailable",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<HygieneStatus> {
        match s {
            "clean" => Some(HygieneStatus::Clean),
            "flagged" => Some(HygieneStatus::Flagged),
            "unavailable" => Some(HygieneStatus::Unavailable),
            _ => None,
        }
    }
}

/// `AdvisoryResult` (§5g.5 `scans: AdvisoryResult[]`) — one advisory scan's
/// outcome `{scanner, status, detail?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryResult {
    /// The scanner identity (a component-ref coordinate).
    pub scanner: String,
    /// The outcome.
    pub status: HygieneStatus,
    /// Optional detail (findings summary — never the finding set itself, which
    /// lives in `text_hygiene`).
    pub detail: Option<String>,
}

/// True when `cp` is an invisible codepoint (zero-widths, word joiners, BOM
/// in-stream, soft hyphen, hangul fillers).
fn is_invisible(cp: u32) -> bool {
    matches!(
        cp,
        0x00AD // SOFT HYPHEN
        | 0x061C // ARABIC LETTER MARK (also a bidi control — classified bidi)
        | 0x180E // MONGOLIAN VOWEL SEPARATOR
        | 0x200B..=0x200F // ZWSP/ZWNJ/ZWJ + LRM/RLM (LRM/RLM reclassified bidi below)
        | 0x2028..=0x2029 // LINE/PARAGRAPH SEPARATOR
        | 0x2060..=0x2064 // WORD JOINER + invisible operators
        | 0x2065 // unassigned invisible
        | 0xFEFF // ZWNBSP / BOM in-stream
        | 0x115F | 0x1160 // HANGUL FILLERS
        | 0x3164 // HANGUL FILLER
        | 0xFFA0 // HALFWIDTH HANGUL FILLER
        | 0xFFF0..=0xFFF8 // unassigned invisible
        | 0xE0000..=0xE0FFF // (tag chars handled separately; this range is all invisible)
    )
}

/// True when `cp` is a bidirectional control.
fn is_bidi_control(cp: u32) -> bool {
    matches!(
        cp,
        0x061C // ARABIC LETTER MARK
        | 0x200E | 0x200F // LRM / RLM
        | 0x202A..=0x202E // LRE RLE PDF LRO RLO
        | 0x2066..=0x2069 // LRI RLI FSI PDI
    )
}

/// True when `cp` is a Unicode Tag codepoint (U+E0000–E007F, or the deprecated
/// language-tag range — the whole plane is treated as tag/invisible).
fn is_tag(cp: u32) -> bool {
    (0xE0000..=0xE007F).contains(&cp)
}

/// Scan `bytes` for invisible/bidirectional/tag codepoints → `TextHygieneReport`
/// (§5g.5; AC-R-2.8.5-11). **Advisory**: the report records `{kind, offset,
/// codepoint}` findings; the input bytes are never modified or normalized.
/// Non-UTF-8 input scans as `unavailable` (the report is explicit, never a silent
/// `clean` — CC3).
pub fn scan_text_hygiene(bytes: &[u8]) -> TextHygieneReport {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            return TextHygieneReport {
                status: HygieneStatus::Unavailable,
                findings: Vec::new(),
            }
        }
    };
    let mut findings = Vec::new();
    for (offset, ch) in text.char_indices() {
        let cp = ch as u32;
        let kind = if is_tag(cp) {
            Some(HygieneFindingKind::TagCodepoint)
        } else if is_bidi_control(cp) {
            Some(HygieneFindingKind::BidirectionalControl)
        } else if is_invisible(cp) {
            Some(HygieneFindingKind::InvisibleCodepoint)
        } else {
            None
        };
        if let Some(kind) = kind {
            findings.push(HygieneFinding {
                kind,
                offset: offset as u64,
                codepoint: format!("U+{cp:04X}"),
            });
        }
    }
    TextHygieneReport {
        status: if findings.is_empty() {
            HygieneStatus::Clean
        } else {
            HygieneStatus::Flagged
        },
        findings,
    }
}

// ── DeclaredClaim ─────────────────────────────────────────────────────────────

/// `DeclaredClaim` (§5g.5 §3 `declared_claims: DeclaredClaim[]` — the
/// machine-usable members from `allowed-tools`, tool annotations, and declared
/// permission manifests). L3: **claims are never grants** — this list is the
/// *only* place they live; the monitor never reads it, and a claim can never
/// appear in `grants` (that would be `LegCrossing`).
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredClaim {
    /// The claim kind (the manifest member it was lifted from).
    pub kind: DeclaredClaimKind,
    /// The claimed value (a machine-usable datum — a tool name, an annotation
    /// key, a declared permission class).
    pub value: Json,
}

/// The `DeclaredClaim` kind sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeclaredClaimKind {
    /// An `allowed-tools` claim.
    AllowedTools,
    /// A tool-annotation claim.
    ToolAnnotation,
    /// A permission-manifest claim.
    PermissionManifest,
    /// Any other declared claim.
    Other,
}

impl DeclaredClaimKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeclaredClaimKind::AllowedTools => "allowed_tools",
            DeclaredClaimKind::ToolAnnotation => "tool_annotation",
            DeclaredClaimKind::PermissionManifest => "permission_manifest",
            DeclaredClaimKind::Other => "other",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<DeclaredClaimKind> {
        match s {
            "allowed_tools" => Some(DeclaredClaimKind::AllowedTools),
            "tool_annotation" => Some(DeclaredClaimKind::ToolAnnotation),
            "permission_manifest" => Some(DeclaredClaimKind::PermissionManifest),
            "other" => Some(DeclaredClaimKind::Other),
            _ => None,
        }
    }
}

// ── ExtensionTrustRecord ─────────────────────────────────────────────────────

/// `ExtensionTrustRecord` (§5g.5 §3): the trust facts the resolver + verifier
/// computed — `text_authority` is the minted `AuthorityClass` (never a declared
/// one); `grants` are conferred `VersionedRef<Permission>` pins (a claim is never
/// a grant); `declared_claims` are the extension's own manifest claims (advisory
/// only, L3); `attestation_status` is the kernel-side verification outcome;
/// `scans`/`text_hygiene` are advisory reports; `surface_pin` carries the lifted
/// MCP `SurfaceDocument`'s ContentAddress when the surface was pinned.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionTrustRecord {
    /// The minted text authority (§2.2 `default_text_authority` — L1 refuses a
    /// value above the minted ceiling).
    pub text_authority: AuthorityClass,
    /// The verified code identities (`ContentAddress`/`SignerRef` coordinates).
    pub code_identity: Vec<String>,
    /// The declared isolation class.
    pub isolation: IsolationClass,
    /// The conferred grants — `VersionedRef<Permission>` pins only (a
    /// `declared_claim` can never appear here; a grant confers, never declares).
    pub grants: Vec<VersionedRef>,
    /// The extension's own manifest claims — advisory, never grants (L3).
    pub declared_claims: Vec<DeclaredClaim>,
    /// The verified attestations (kernel-verified, never self-declared).
    pub attestations: Vec<Attestation>,
    /// The verification outcome (the field `default_text_authority` reads).
    pub attestation_status: AttestationStatus,
    /// Advisory scan results.
    pub scans: Vec<AdvisoryResult>,
    /// The text-hygiene report (advisory; the bytes it scanned are preserved).
    pub text_hygiene: TextHygieneReport,
    /// The lifted MCP `SurfaceDocument`'s pin (remote endpoints, when the
    /// discovery output was lifted to a pinned `SurfaceDocument`).
    pub surface_pin: Option<ContentAddress>,
    /// Who installed the extension (identity coordinate — a model-authored
    /// install is an install procedure fact, Stage 4).
    pub installed_by: Option<String>,
    /// When the extension was installed (logical time).
    pub installed_at: Option<u64>,
    /// The persistence scope the extension is installed under.
    pub scope: PersistenceScope,
    /// The trust status.
    pub status: TrustStatus,
    /// The `human_review` record ref (C1 review routing — a *reference*, never a
    /// fact that raises `text_authority`: L2).
    pub review_ref: Option<String>,
}

impl ExtensionTrustRecord {
    /// The conservative Stage-1 default: no attestation, no grants, no claims,
    /// clean-scan-unknown hygiene, `resolved` status. `text_authority` is minted
    /// by [`default_text_authority`] — never set directly above it (L1).
    pub fn unresolved_default(scope: PersistenceScope) -> ExtensionTrustRecord {
        ExtensionTrustRecord {
            text_authority: AuthorityClass::External,
            code_identity: Vec::new(),
            isolation: IsolationClass::InProcess,
            grants: Vec::new(),
            declared_claims: Vec::new(),
            attestations: Vec::new(),
            attestation_status: AttestationStatus::Missing,
            scans: Vec::new(),
            text_hygiene: TextHygieneReport {
                status: HygieneStatus::Unavailable,
                findings: Vec::new(),
            },
            surface_pin: None,
            installed_by: None,
            installed_at: None,
            scope,
            status: TrustStatus::Resolved,
            review_ref: None,
        }
    }
}

// ── default_text_authority ────────────────────────────────────────────────────

/// The minting table (§5g.5 §2.2):
///
/// `default_text_authority(kind, origin, attestation_status) =`
/// - `attestation_status = verified(signature)` → `definition` (the only route
///   that reaches `definition`/`principal` for extension text — the pin
///   endorsement; `principal` only where the TrustRootPolicy says so — the C1
///   refinement keeps this table's shape).
/// - `attestation_status = verified(seal)` → `definition`, except
///   `mcp_server`/`fetched_instruction` → `external` (a sealed open-world
///   endpoint's live text is never definition).
/// - `attestation_status = verified(pin | hash_chain)` → `external` ∧
///   `R_text(origin)` (the `hash_only_ceiling` — a hash pin preserves integrity,
///   it never lifts authority).
/// - `attestation_status = missing` → `min(kind_floor, R_text(origin))` where
///   `kind_floor(mcp_server) = unverified` (a server outside the sealed
///   definition), `kind_floor(fetched_instruction) = external` with taint (the
///   label leg — `fetch[instruction]` returns `external + tainted` content), and
///   `kind_floor(·) = external` otherwise.
/// - `attestation_status = failed` → `unverified` (a failed verification lowers,
///   never launders).
///
/// `R_text` is `hh_provenance::origin::default_text_authority` — **the same**
/// minting table every other text-bearing pipeline runs (CC1 — this function
/// composes it, it never re-tables it). The function is **total** over
/// `(kind, origin, attestation_status)` and reads nothing else — the L1
/// `LocationElevation` check is that no locator/scope/directory/marketplace/
/// settings-layer/"trusted folder" fact ever reaches it (AC-R-2.8.5-2).
pub fn default_text_authority(
    kind: &ExtensionKind,
    origin: &Origin,
    attestation_status: &AttestationStatus,
) -> AuthorityClass {
    // The same R_text minting table the rest of the system runs (CC1) — origin
    // facts only, no attestation self-claim (`None`: our verified-fact status
    // already encodes the kernel-side verification outcome).
    let r_text = hh_provenance::origin::default_text_authority(origin, None);
    match attestation_status {
        AttestationStatus::Failed => AuthorityClass::Unverified,
        AttestationStatus::Verified(AttestationKind::Signature) => {
            // The pin endorsement — the only route to definition for extension
            // text. (`principal` requires the TrustRootPolicy's say — C1.)
            AuthorityClass::Definition
        }
        AttestationStatus::Verified(AttestationKind::Seal) => match kind {
            // A sealed *open-world* endpoint (MCP server, fetched instruction)
            // keeps its live text external — the seal covers the record, not
            // the server's live output (the open-world carve-out).
            ExtensionKind::McpServer | ExtensionKind::FetchedInstruction => {
                AuthorityClass::External.min(r_text)
            }
            _ => AuthorityClass::Definition,
        },
        AttestationStatus::Verified(AttestationKind::Pin)
        | AttestationStatus::Verified(AttestationKind::HashChain) => {
            // `hash_only_ceiling` — a hash pin never lifts authority.
            AuthorityClass::External.min(r_text)
        }
        AttestationStatus::Missing => {
            let kind_floor = match kind {
                ExtensionKind::McpServer => AuthorityClass::Unverified,
                _ => AuthorityClass::External,
            };
            kind_floor.min(r_text)
        }
    }
}

/// The `LocationElevation` check (L1): `text_authority` must equal the minted
/// ceiling — never above it (and a value *below* it is a refused quiet-widening-
/// in-reverse: the record must carry the minted value so two resolvers mint the
/// same record — determinism). Any mismatch fails `LocationElevation`.
pub fn check_text_authority(
    kind: &ExtensionKind,
    origin: &Origin,
    attestation_status: &AttestationStatus,
    text_authority: AuthorityClass,
) -> Result<(), ExtensionError> {
    let minted = default_text_authority(kind, origin, attestation_status);
    if text_authority != minted {
        return Err(ExtensionError::LocationElevation {
            detail: format!(
                "text_authority {} != minted {} (kind {}, attestation {:?})",
                text_authority.as_str(),
                minted.as_str(),
                kind.as_str(),
                attestation_status
            ),
        });
    }
    Ok(())
}

// ── ExtensionRecord ───────────────────────────────────────────────────────────

/// `ExtensionRecord` (§5g.5 §3): `{extension_id (version_id), kind, name,
/// content: ContentAddress, manifest, contributes, locator, trust, provenance,
/// ext}` — a **registry-layer versioned record** (`RecordKind::Extension`),
/// never a HIR entity. `extension_id` is the envelope's `version_id` (the body
/// never carries its own identity coordinate — the same treatment every other
/// registry record gets; `H(body)` would be circular otherwise).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRecord {
    /// The extension kind.
    pub kind: ExtensionKind,
    /// The extension name (the semantic coordinate).
    pub name: String,
    /// The content pin (the tree rule — the ContentAddress of the extension's
    /// payload; filled at `resolve`).
    pub content: ContentAddress,
    /// The extension's manifest (verbatim, schema-external — claims inside it
    /// live in `trust.declared_claims` after lifting).
    pub manifest: Json,
    /// The components this extension contributes (`VersionedRef`s).
    pub contributes: Vec<VersionedRef>,
    /// The declared+resolved source locator.
    pub locator: SourceLocator,
    /// The trust facts.
    pub trust: ExtensionTrustRecord,
    /// The provenance.
    pub provenance: ProvenanceRecord,
    /// The `ext` escape-hatch.
    pub ext: BTreeMap<String, Json>,
}

/// A *candidate* — what `discover` yields (a not-yet-resolved extension):
/// `{name, kind, locator, source}`. A candidate is not an `ExtensionRecord`
/// (no `content` yet — the resolver fills it).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The discovered name.
    pub name: String,
    /// The discovered kind.
    pub kind: ExtensionKind,
    /// The discovered locator (`resolved`/`fetched_at` empty).
    pub locator: SourceLocator,
    /// The declared source it was discovered under.
    pub source: DeclaredSource,
}

/// A sealed-form extension **reference** — the assembly-side member that L4
/// checks: `{name, kind, locator, content?}`. A reference is *pinned* when
/// `locator.is_pinned()` and `content` is present; anything less reaching a
/// sealed form fails `UnpinnedInSealedForm` (AC-R-2.8.5-1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRef {
    /// The extension name.
    pub name: String,
    /// The extension kind.
    pub kind: ExtensionKind,
    /// The locator (selector → resolved at `resolve`).
    pub locator: SourceLocator,
    /// The content pin (filled at `resolve`).
    pub content: Option<ContentAddress>,
    /// The pinned extension record (`version_id`, filled at `resolve`).
    pub extension_id: Option<String>,
}

impl ExtensionRef {
    /// L4's pinned test.
    pub fn is_pinned(&self) -> bool {
        self.content.is_some() && self.locator.is_pinned()
    }
}

/// The fetch outcome the resolver consumes (Stage-1 model: the fetcher is a
/// caller-provided function; `resolve_candidate` only *records* the outcome —
/// it never performs I/O itself, CC5).
#[derive(Debug, Clone, PartialEq)]
pub struct FetchOutcome {
    /// The resolved coordinate (the pinned selector — a commit SHA, a resolved
    /// version_id, a fetched URL).
    pub resolved: String,
    /// The content pin.
    pub content: ContentAddress,
    /// The fetch stamp (logical time).
    pub fetched_at: u64,
    /// The verified code identities (empty at Stage 1 for unsigned payloads).
    pub code_identity: Vec<String>,
    /// The source's own snapshot id (the `{registry_snapshot_id}` — the
    /// resolution's reproducibility anchor).
    pub source_snapshot: Option<String>,
    /// The lifted MCP `SurfaceDocument` pin (remote endpoints).
    pub surface_pin: Option<ContentAddress>,
}

/// `resolve` over a candidate: fills `locator.resolved`/`fetched_at`, sets
/// `content`/`code_identity`/`surface_pin` from the `FetchOutcome`, mints
/// `text_authority` via [`default_text_authority`] (never reads locator facts —
/// L1), scans `payload` for text hygiene (advisory), and stamps
/// `status = resolved`. `payload` is the fetched bytes for the hygiene scan —
/// the *record* keeps the pin, not the bytes (R3 — the store never loads
/// `implementation.content`; the scan is the resolver's one read).
pub fn resolve_candidate(
    candidate: &Candidate,
    outcome: &FetchOutcome,
    origin: Origin,
    payload: Option<&[u8]>,
    created_at: u64,
) -> ExtensionRecord {
    let attestation_status = AttestationStatus::Missing;
    let text_authority = default_text_authority(&candidate.kind, &origin, &attestation_status);
    let text_hygiene = payload.map(scan_text_hygiene).unwrap_or(TextHygieneReport {
        status: HygieneStatus::Unavailable,
        findings: Vec::new(),
    });
    let mut locator = candidate.locator.clone();
    locator.resolved = Some(outcome.resolved.clone());
    locator.fetched_at = Some(outcome.fetched_at);
    locator.selector = None; // the selector is consumed by the pin (L4)
    ExtensionRecord {
        kind: candidate.kind.clone(),
        name: candidate.name.clone(),
        content: outcome.content.clone(),
        manifest: Json::obj([]),
        contributes: Vec::new(),
        locator,
        trust: ExtensionTrustRecord {
            text_authority,
            code_identity: outcome.code_identity.clone(),
            isolation: IsolationClass::InProcess,
            grants: Vec::new(),
            declared_claims: Vec::new(),
            attestations: Vec::new(),
            attestation_status,
            scans: Vec::new(),
            text_hygiene,
            surface_pin: outcome.surface_pin.clone(),
            installed_by: None,
            installed_at: None,
            scope: PersistenceScope::Run,
            status: TrustStatus::Resolved,
            review_ref: None,
        },
        provenance: ProvenanceRecord::minted(origin, PersistenceScope::Run, created_at),
        ext: BTreeMap::new(),
    }
}

/// The L1–L3 record-level checks (`validate` — the admission gate; the
/// assembly-level L4 check is `ExtensionRef::is_pinned` + `validate_assembly`):
/// - **L1**: `text_authority` equals the minted ceiling (LocationElevation).
/// - **L2**: legs are independent — a `review_ref` never raises text authority
///   (checked by L1's minted-equality: review is not an input to the table);
///   `grants` entries must be `VersionedRef`s (typed, conferring) and a
///   `declared_claim` can never appear among them — structurally impossible
///   (the sums are disjoint).
/// - **L3**: claims are never grants — enforced by the disjoint sums; a record
///   whose `declared_claims` carry a `version_id`-shaped value is refused
///   (`LegCrossing` — a claim pretending to be a pin).
/// - `attestation_status = verified(k)` requires a matching `attestations[]`
///   member (the status is the *outcome* of verification, not a parallel claim).
pub fn validate_extension_record(r: &ExtensionRecord) -> Result<(), RegistryError> {
    let path = "extension";
    r.locator.validate_credential_free(path)?;
    if let Err(e) = check_text_authority(
        &r.kind,
        &r.provenance.origin,
        &r.trust.attestation_status,
        r.trust.text_authority,
    ) {
        return Err(RegistryError::SchemaViolation {
            path: format!("{path}.trust.text_authority"),
            detail: match e {
                ExtensionError::LocationElevation { detail } => {
                    format!("LocationElevation: {detail}")
                }
                other => format!("{other:?}"),
            },
        });
    }
    if let AttestationStatus::Verified(kind) = r.trust.attestation_status {
        if !r.trust.attestations.iter().any(|a| a.kind == kind) {
            return Err(RegistryError::SchemaViolation {
                path: format!("{path}.trust.attestation_status"),
                detail: format!(
                    "verified({}) claimed without a matching attestations[] member",
                    kind.as_str()
                ),
            });
        }
    }
    // L3: a claim carrying a pin-shaped value pretends to be a grant.
    for c in &r.trust.declared_claims {
        if let Json::Obj(m) = &c.value {
            if m.contains_key("version_id") || m.contains_key("grant") {
                return Err(RegistryError::SchemaViolation {
                    path: format!("{path}.trust.declared_claims"),
                    detail: "LegCrossing: a declared_claim may not carry a pin/grant shape"
                        .to_string(),
                });
            }
        }
    }
    Ok(())
}

/// The extension-trust errors (§5g.5 §6 — `UnpinnedInSealedForm`,
/// `LocationElevation`, `LegCrossing`; `NameCollision` is the registry
/// collision-policy error, already a `RegistryError`).
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionError {
    /// A selector survived into a sealed form, or `content` is missing
    /// (AC-R-2.8.5-1).
    UnpinnedInSealedForm {
        /// The detail.
        detail: String,
    },
    /// A locator/scope/directory/marketplace/settings-layer/"trusted folder"
    /// fact reached text-authority minting.
    LocationElevation {
        /// The detail.
        detail: String,
    },
    /// One leg tried to raise another (a claim pretending to be a grant, a
    /// review fact raising text authority, …).
    LegCrossing {
        /// The detail.
        detail: String,
    },
    /// Two declared sources/candidates yielded the same `name` in the same
    /// scope (the collision policy is `refuse`).
    NameCollision {
        /// The detail.
        detail: String,
    },
}

// ── Codecs (CC7 — one canonical encoding, `body_json`/`record_from_json`) ──────

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, RegistryError> {
    j.get(k).ok_or_else(|| RegistryError::SchemaViolation {
        path: format!("{path}.{k}"),
        detail: "missing member".to_string(),
    })
}

fn req_str(j: &Json, k: &str, path: &str) -> Result<String, RegistryError> {
    req(j, k, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected string".to_string(),
        })
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn req_arr<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a [Json], RegistryError> {
    match req(j, k, path)? {
        Json::Arr(a) => Ok(a.as_slice()),
        _ => Err(RegistryError::SchemaViolation {
            path: format!("{path}.{k}"),
            detail: "expected array".to_string(),
        }),
    }
}

fn scope_str(s: PersistenceScope) -> &'static str {
    match s {
        PersistenceScope::Definition => "definition",
        PersistenceScope::User => "user",
        PersistenceScope::Project => "project",
        PersistenceScope::Session => "session",
        PersistenceScope::Run => "run",
        PersistenceScope::Turn => "turn",
    }
}

fn scope_parse(s: &str) -> Option<PersistenceScope> {
    match s {
        "definition" => Some(PersistenceScope::Definition),
        "user" => Some(PersistenceScope::User),
        "project" => Some(PersistenceScope::Project),
        "session" => Some(PersistenceScope::Session),
        "run" => Some(PersistenceScope::Run),
        "turn" => Some(PersistenceScope::Turn),
        _ => None,
    }
}

fn locator_json(l: &SourceLocator) -> Json {
    let mut m = BTreeMap::new();
    m.insert("scheme".to_string(), Json::str(l.scheme.clone()));
    m.insert(
        "credential_free_uri".to_string(),
        Json::str(l.credential_free_uri.clone()),
    );
    if let Some(s) = &l.selector {
        m.insert("selector".to_string(), Json::str(s.clone()));
    }
    if let Some(s) = &l.resolved {
        m.insert("resolved".to_string(), Json::str(s.clone()));
    }
    if let Some(t) = l.fetched_at {
        m.insert("fetched_at".to_string(), Json::Int(t as i64));
    }
    Json::Obj(m)
}

fn locator_from_json(j: &Json, path: &str) -> Result<SourceLocator, RegistryError> {
    Ok(SourceLocator {
        scheme: req_str(j, "scheme", path)?,
        credential_free_uri: req_str(j, "credential_free_uri", path)?,
        selector: opt_str(j, "selector"),
        resolved: opt_str(j, "resolved"),
        fetched_at: j.get("fetched_at").and_then(Json::as_int).map(|i| i as u64),
    })
}

fn ca_json(c: &ContentAddress) -> Json {
    Json::obj([
        ("algorithm", Json::str(c.algorithm)),
        ("digest", Json::str(c.digest.clone())),
        ("media_type", Json::str(c.media_type.clone())),
        ("size", Json::Int(c.size as i64)),
    ])
}

fn ca_from_json(j: &Json, path: &str) -> Result<ContentAddress, RegistryError> {
    let algorithm = req_str(j, "algorithm", path)?;
    let digest = req_str(j, "digest", path)?;
    // The id format checks are the one canonical parser (CC1) — a malformed or
    // non-idp/1-tagged address is refused here, never halfway through a consumer.
    idp::parse_id(&format!("{algorithm}:{digest}")).map_err(|e| {
        RegistryError::SchemaViolation {
            path: path.to_string(),
            detail: format!("content address: {e:?}"),
        }
    })?;
    Ok(ContentAddress {
        idp: idp::IDP_1.idp_id,
        algorithm: idp::IDP_1.hash_algorithm,
        digest,
        media_type: req_str(j, "media_type", path)?,
        size: req(j, "size", path)?
            .as_int()
            .ok_or_else(|| RegistryError::SchemaViolation {
                path: format!("{path}.size"),
                detail: "expected int".to_string(),
            })? as u64,
    })
}

fn ref_json(r: &VersionedRef) -> Json {
    let mut m = BTreeMap::new();
    m.insert("kind".to_string(), Json::str(r.kind.domain_tag()));
    m.insert("version_id".to_string(), Json::str(r.version_id.clone()));
    if let Some(s) = &r.semantic_id {
        m.insert("semantic_id".to_string(), Json::str(s.clone()));
    }
    Json::Obj(m)
}

fn kind_from_domain_tag(tag: &str) -> Option<IdentityRecordKind> {
    use IdentityRecordKind as K;
    Some(match tag {
        "hir.node" => K::HirNode,
        "hir.edge" => K::HirEdge,
        "hir.text" => K::HirTextLeaf,
        "hir.definition" => K::SealedDefinition,
        "variant" => K::VariantRecord,
        "model_profile" => K::ModelProfile,
        "permission_policy" => K::PermissionPolicy,
        "budget" => K::Budget,
        "validator" => K::Validator,
        "task_data_manifest" => K::TaskDataManifest,
        "capability_declaration" => K::CapabilityDeclaration,
        "run_manifest" => K::RunManifest,
        "bundle_manifest" => K::BundleManifest,
        "name_binding" => K::NameBindingRecord,
        "identity_profile" => K::IdentityProfile,
        "extension" => K::ExtensionRecord,
        "registry" => K::RegistryRecord,
        "revocation" => K::RevocationRecord,
        "configuration" => K::Configuration,
        "configuration_version" => K::ConfigurationVersion,
        "containment_policy" => K::ContainmentPolicy,
        "secret.channel" => K::SecretChannel,
        "credential.binding" => K::CredentialBinding,
        "sink_policy" => K::SinkPolicy,
        "environment" => K::EnvironmentRecord,
        "tool_surface" => K::ToolSurface,
        "memory" => K::Memory,
        "memory_manifest" => K::MemoryManifest,
        _ => return None,
    })
}

fn ref_from_json(j: &Json, path: &str) -> Result<VersionedRef, RegistryError> {
    let kind = kind_from_domain_tag(&req_str(j, "kind", path)?).ok_or_else(|| {
        RegistryError::SchemaViolation {
            path: format!("{path}.kind"),
            detail: "unknown record kind".to_string(),
        }
    })?;
    let mut r = VersionedRef::pinned(
        kind,
        req_str(j, "version_id", path)?,
        ProvenanceRecord::kernel("kernel:registry", 0),
    );
    if let Some(s) = opt_str(j, "semantic_id") {
        r = r.with_semantic(s);
    }
    Ok(r)
}

fn attestation_json2(a: &Attestation) -> Json {
    let anchor = match &a.anchor {
        hh_provenance::record::AttestationAnchor::Signer(s) => {
            Json::obj([("signer", Json::str(s.clone()))])
        }
        hh_provenance::record::AttestationAnchor::Chain(c) => {
            Json::obj([("chain", Json::str(c.clone()))])
        }
    };
    Json::obj([
        ("kind", Json::str(a.kind.as_str())),
        ("subject_hash", Json::str(a.subject_hash.clone())),
        ("anchor", anchor),
        ("verified_by", Json::str(a.verified_by.clone())),
        ("verified_at", Json::Int(a.verified_at as i64)),
    ])
}

fn attestation_from_json(j: &Json, path: &str) -> Result<Attestation, RegistryError> {
    let kind = match req_str(j, "kind", path)?.as_str() {
        "seal" => AttestationKind::Seal,
        "hash_chain" => AttestationKind::HashChain,
        "signature" => AttestationKind::Signature,
        "pin" => AttestationKind::Pin,
        other => {
            return Err(RegistryError::SchemaViolation {
                path: format!("{path}.kind"),
                detail: format!("unknown attestation kind {other}"),
            })
        }
    };
    let anchor_j = req(j, "anchor", path)?;
    let anchor = if let Some(s) = anchor_j.get("signer").and_then(|v| v.as_str()) {
        AttestationAnchor::Signer(s.to_string())
    } else if let Some(c) = anchor_j.get("chain").and_then(|v| v.as_str()) {
        AttestationAnchor::Chain(c.to_string())
    } else {
        return Err(RegistryError::SchemaViolation {
            path: format!("{path}.anchor"),
            detail: "expected {signer} or {chain}".to_string(),
        });
    };
    Ok(Attestation {
        kind,
        subject_hash: req_str(j, "subject_hash", path)?,
        anchor,
        verified_by: req_str(j, "verified_by", path)?,
        verified_at: req(j, "verified_at", path)?.as_int().ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.verified_at"),
                detail: "expected int".to_string(),
            }
        })? as u64,
    })
}

fn hygiene_json(h: &TextHygieneReport) -> Json {
    Json::obj([
        ("status", Json::str(h.status.as_str())),
        (
            "findings",
            Json::Arr(
                h.findings
                    .iter()
                    .map(|f| {
                        Json::obj([
                            ("kind", Json::str(f.kind.as_str())),
                            ("offset", Json::Int(f.offset as i64)),
                            ("codepoint", Json::str(f.codepoint.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn hygiene_from_json(j: &Json, path: &str) -> Result<TextHygieneReport, RegistryError> {
    let status = HygieneStatus::parse(&req_str(j, "status", path)?).ok_or_else(|| {
        RegistryError::SchemaViolation {
            path: format!("{path}.status"),
            detail: "unknown hygiene status".to_string(),
        }
    })?;
    let mut findings = Vec::new();
    for f in req_arr(j, "findings", path)? {
        findings.push(HygieneFinding {
            kind: HygieneFindingKind::parse(&req_str(f, "kind", path)?).ok_or_else(|| {
                RegistryError::SchemaViolation {
                    path: format!("{path}.findings.kind"),
                    detail: "unknown finding kind".to_string(),
                }
            })?,
            offset: req(f, "offset", path)?.as_int().ok_or_else(|| {
                RegistryError::SchemaViolation {
                    path: format!("{path}.findings.offset"),
                    detail: "expected int".to_string(),
                }
            })? as u64,
            codepoint: req_str(f, "codepoint", path)?,
        });
    }
    Ok(TextHygieneReport { status, findings })
}

fn claim_json(c: &DeclaredClaim) -> Json {
    Json::obj([
        ("kind", Json::str(c.kind.as_str())),
        ("value", c.value.clone()),
    ])
}

fn claim_from_json(j: &Json, path: &str) -> Result<DeclaredClaim, RegistryError> {
    Ok(DeclaredClaim {
        kind: DeclaredClaimKind::parse(&req_str(j, "kind", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.kind"),
                detail: "unknown claim kind".to_string(),
            }
        })?,
        value: req(j, "value", path)?.clone(),
    })
}

fn scan_json(s: &AdvisoryResult) -> Json {
    let mut m = BTreeMap::new();
    m.insert("scanner".to_string(), Json::str(s.scanner.clone()));
    m.insert("status".to_string(), Json::str(s.status.as_str()));
    if let Some(d) = &s.detail {
        m.insert("detail".to_string(), Json::str(d.clone()));
    }
    Json::Obj(m)
}

fn scan_from_json(j: &Json, path: &str) -> Result<AdvisoryResult, RegistryError> {
    Ok(AdvisoryResult {
        scanner: req_str(j, "scanner", path)?,
        status: HygieneStatus::parse(&req_str(j, "status", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.status"),
                detail: "unknown hygiene status".to_string(),
            }
        })?,
        detail: opt_str(j, "detail"),
    })
}

fn trust_json(t: &ExtensionTrustRecord) -> Json {
    let mut m = BTreeMap::new();
    m.insert(
        "text_authority".to_string(),
        Json::str(t.text_authority.as_str()),
    );
    m.insert(
        "code_identity".to_string(),
        Json::Arr(t.code_identity.iter().map(Json::str).collect()),
    );
    m.insert("isolation".to_string(), Json::str(t.isolation.as_str()));
    m.insert(
        "grants".to_string(),
        Json::Arr(t.grants.iter().map(ref_json).collect()),
    );
    m.insert(
        "declared_claims".to_string(),
        Json::Arr(t.declared_claims.iter().map(claim_json).collect()),
    );
    m.insert(
        "attestations".to_string(),
        Json::Arr(t.attestations.iter().map(attestation_json2).collect()),
    );
    m.insert(
        "attestation_status".to_string(),
        t.attestation_status.to_json(),
    );
    m.insert(
        "scans".to_string(),
        Json::Arr(t.scans.iter().map(scan_json).collect()),
    );
    m.insert("text_hygiene".to_string(), hygiene_json(&t.text_hygiene));
    if let Some(p) = &t.surface_pin {
        m.insert("surface_pin".to_string(), ca_json(p));
    }
    if let Some(s) = &t.installed_by {
        m.insert("installed_by".to_string(), Json::str(s.clone()));
    }
    if let Some(t_) = t.installed_at {
        m.insert("installed_at".to_string(), Json::Int(t_ as i64));
    }
    m.insert("scope".to_string(), Json::str(scope_str(t.scope)));
    m.insert("status".to_string(), Json::str(t.status.as_str()));
    if let Some(r) = &t.review_ref {
        m.insert("review_ref".to_string(), Json::str(r.clone()));
    }
    Json::Obj(m)
}

fn trust_from_json(j: &Json, path: &str) -> Result<ExtensionTrustRecord, RegistryError> {
    let authority = match req_str(j, "text_authority", path)?.as_str() {
        "principal" => AuthorityClass::Principal,
        "definition" => AuthorityClass::Definition,
        "environment" => AuthorityClass::Environment,
        "external" => AuthorityClass::External,
        "unverified" => AuthorityClass::Unverified,
        other => {
            return Err(RegistryError::SchemaViolation {
                path: format!("{path}.text_authority"),
                detail: format!("unknown authority {other}"),
            })
        }
    };
    let mut grants = Vec::new();
    for g in req_arr(j, "grants", path)? {
        grants.push(ref_from_json(g, &format!("{path}.grants"))?);
    }
    let mut claims = Vec::new();
    for c in req_arr(j, "declared_claims", path)? {
        claims.push(claim_from_json(c, &format!("{path}.declared_claims"))?);
    }
    let mut attestations = Vec::new();
    for a in req_arr(j, "attestations", path)? {
        attestations.push(attestation_from_json(a, &format!("{path}.attestations"))?);
    }
    let mut scans = Vec::new();
    for s in req_arr(j, "scans", path)? {
        scans.push(scan_from_json(s, &format!("{path}.scans"))?);
    }
    Ok(ExtensionTrustRecord {
        text_authority: authority,
        code_identity: req_arr(j, "code_identity", path)?
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect(),
        isolation: IsolationClass::parse(&req_str(j, "isolation", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.isolation"),
                detail: "unknown isolation class".to_string(),
            }
        })?,
        grants,
        declared_claims: claims,
        attestations,
        attestation_status: AttestationStatus::from_json(
            req(j, "attestation_status", path)?,
            &format!("{path}.attestation_status"),
        )?,
        scans,
        text_hygiene: hygiene_from_json(
            req(j, "text_hygiene", path)?,
            &format!("{path}.text_hygiene"),
        )?,
        surface_pin: j
            .get("surface_pin")
            .map(|p| ca_from_json(p, &format!("{path}.surface_pin")))
            .transpose()?,
        installed_by: opt_str(j, "installed_by"),
        installed_at: j
            .get("installed_at")
            .and_then(Json::as_int)
            .map(|i| i as u64),
        scope: scope_parse(&req_str(j, "scope", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.scope"),
                detail: "unknown scope".to_string(),
            }
        })?,
        status: TrustStatus::parse(&req_str(j, "status", path)?).ok_or_else(|| {
            RegistryError::SchemaViolation {
                path: format!("{path}.status"),
                detail: "unknown trust status".to_string(),
            }
        })?,
        review_ref: opt_str(j, "review_ref"),
    })
}

/// The canonical `ExtensionRecord` body encoding.
pub fn extension_body_json(r: &ExtensionRecord) -> Json {
    let mut m = BTreeMap::new();
    m.insert("kind".to_string(), Json::str(r.kind.as_str()));
    m.insert("name".to_string(), Json::str(r.name.clone()));
    m.insert("content".to_string(), ca_json(&r.content));
    m.insert("manifest".to_string(), r.manifest.clone());
    m.insert(
        "contributes".to_string(),
        Json::Arr(r.contributes.iter().map(ref_json).collect()),
    );
    m.insert("locator".to_string(), locator_json(&r.locator));
    m.insert("trust".to_string(), trust_json(&r.trust));
    m.insert("provenance".to_string(), r.provenance.to_json());
    if !r.ext.is_empty() {
        m.insert("ext".to_string(), Json::Obj(r.ext.clone()));
    }
    Json::Obj(m)
}

/// Decode an `ExtensionRecord` body.
pub fn extension_from_json(j: &Json, path: &str) -> Result<ExtensionRecord, RegistryError> {
    let mut contributes = Vec::new();
    for c in req_arr(j, "contributes", path)? {
        contributes.push(ref_from_json(c, &format!("{path}.contributes"))?);
    }
    let ext = match j.get("ext") {
        Some(Json::Obj(m)) => m.clone(),
        _ => BTreeMap::new(),
    };
    Ok(ExtensionRecord {
        kind: ExtensionKind::parse(&req_str(j, "kind", path)?),
        name: req_str(j, "name", path)?,
        content: ca_from_json(req(j, "content", path)?, &format!("{path}.content"))?,
        manifest: req(j, "manifest", path)?.clone(),
        contributes,
        locator: locator_from_json(req(j, "locator", path)?, &format!("{path}.locator"))?,
        trust: trust_from_json(req(j, "trust", path)?, &format!("{path}.trust"))?,
        provenance: ProvenanceRecord::from_json(req(j, "provenance", path)?).map_err(|e| {
            RegistryError::SchemaViolation {
                path: format!("{path}.provenance"),
                detail: format!("{e:?}"),
            }
        })?,
        ext,
    })
}

/// `ExtensionRef` codec (the sealed-form reference — what an Assembly carries).
pub fn extension_ref_json(r: &ExtensionRef) -> Json {
    let mut m = BTreeMap::new();
    m.insert("name".to_string(), Json::str(r.name.clone()));
    m.insert("kind".to_string(), Json::str(r.kind.as_str()));
    m.insert("locator".to_string(), locator_json(&r.locator));
    if let Some(c) = &r.content {
        m.insert("content".to_string(), ca_json(c));
    }
    if let Some(e) = &r.extension_id {
        m.insert("extension_id".to_string(), Json::str(e.clone()));
    }
    Json::Obj(m)
}

/// Decode an `ExtensionRef`.
pub fn extension_ref_from_json(j: &Json, path: &str) -> Result<ExtensionRef, RegistryError> {
    Ok(ExtensionRef {
        name: req_str(j, "name", path)?,
        kind: ExtensionKind::parse(&req_str(j, "kind", path)?),
        locator: locator_from_json(req(j, "locator", path)?, &format!("{path}.locator"))?,
        content: j
            .get("content")
            .map(|c| ca_from_json(c, &format!("{path}.content")))
            .transpose()?,
        extension_id: opt_str(j, "extension_id"),
    })
}

/// `DeclaredSource` codec.
pub fn declared_source_json(s: &DeclaredSource) -> Json {
    match s {
        DeclaredSource::DirectoryScan { root, scope } => Json::obj([
            ("kind", Json::str("directory_scan")),
            ("root", Json::str(root.clone())),
            ("scope", Json::str(scope_str(*scope))),
        ]),
        DeclaredSource::Marketplace { catalog_ref } => Json::obj([
            ("kind", Json::str("marketplace")),
            ("catalog_ref", Json::str(catalog_ref.clone())),
        ]),
        DeclaredSource::Registry { name } => Json::obj([
            ("kind", Json::str("registry")),
            ("name", Json::str(name.clone())),
        ]),
        DeclaredSource::Git { url, ref_ } => Json::obj([
            ("kind", Json::str("git")),
            ("url", Json::str(url.clone())),
            ("ref", Json::str(ref_.clone())),
        ]),
        DeclaredSource::Archive { url, sha } => Json::obj([
            ("kind", Json::str("archive")),
            ("url", Json::str(url.clone())),
            ("sha", Json::str(sha.clone())),
        ]),
        DeclaredSource::McpEndpoint { uri } => Json::obj([
            ("kind", Json::str("mcp_endpoint")),
            ("uri", Json::str(uri.clone())),
        ]),
        DeclaredSource::InstructionFiles { roots } => Json::obj([
            ("kind", Json::str("instruction_files")),
            ("roots", Json::Arr(roots.iter().map(Json::str).collect())),
        ]),
    }
}

/// Decode a `DeclaredSource`.
pub fn declared_source_from_json(j: &Json, path: &str) -> Result<DeclaredSource, RegistryError> {
    match req_str(j, "kind", path)?.as_str() {
        "directory_scan" => Ok(DeclaredSource::DirectoryScan {
            root: req_str(j, "root", path)?,
            scope: scope_parse(&req_str(j, "scope", path)?).ok_or_else(|| {
                RegistryError::SchemaViolation {
                    path: format!("{path}.scope"),
                    detail: "unknown scope".to_string(),
                }
            })?,
        }),
        "marketplace" => Ok(DeclaredSource::Marketplace {
            catalog_ref: req_str(j, "catalog_ref", path)?,
        }),
        "registry" => Ok(DeclaredSource::Registry {
            name: req_str(j, "name", path)?,
        }),
        "git" => Ok(DeclaredSource::Git {
            url: req_str(j, "url", path)?,
            ref_: req_str(j, "ref", path)?,
        }),
        "archive" => Ok(DeclaredSource::Archive {
            url: req_str(j, "url", path)?,
            sha: req_str(j, "sha", path)?,
        }),
        "mcp_endpoint" => Ok(DeclaredSource::McpEndpoint {
            uri: req_str(j, "uri", path)?,
        }),
        "instruction_files" => Ok(DeclaredSource::InstructionFiles {
            roots: req_arr(j, "roots", path)?
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
        }),
        other => Err(RegistryError::SchemaViolation {
            path: format!("{path}.kind"),
            detail: format!("unknown declared-source kind {other}"),
        }),
    }
}

// ── trust_snapshot (§5g.5 §3; ADR-0064 D7; S3.11b) ────────────────────────────

/// One `trust_snapshot.extensions[]` entry — the bundle-member shape
/// (`{extension_id, content, trust_record, attestation_refs[], surface_pin?}` —
/// §5g.5 §3's `resolved_dependencies.extensions[]` row, refined by CF-145).
/// `trust_record` names the versioned coordinate of the trust facts: the
/// envelope's `trust_record_ref` when a separate trust-record pin exists,
/// else the extension record's own `version_id` (the record that carries the
/// embedded `ExtensionTrustRecord`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrustSnapshotEntry {
    /// The pinned extension record (`version_id`).
    pub extension_id: String,
    /// The content pin (`ContentAddress::id()` — `<algorithm>:<hex>`).
    pub content: String,
    /// The trust facts' version coordinate.
    pub trust_record: String,
    /// The verified attestations' subject coordinates.
    pub attestation_refs: Vec<String>,
    /// The lifted `SurfaceDocument` pin, when the surface was pinned.
    pub surface_pin: Option<String>,
}

impl TrustSnapshotEntry {
    /// The canonical JSON member.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("extension_id".into(), Json::str(self.extension_id.clone()));
        m.insert("content".into(), Json::str(self.content.clone()));
        m.insert("trust_record".into(), Json::str(self.trust_record.clone()));
        m.insert(
            "attestation_refs".into(),
            Json::Arr(self.attestation_refs.iter().map(Json::str).collect()),
        );
        if let Some(p) = &self.surface_pin {
            m.insert("surface_pin".into(), Json::str(p.clone()));
        }
        Json::Obj(m)
    }
}

/// `trust_snapshot(definition)` — the `{extensions[], policy version_id,
/// registry_snapshot_id}` projection the bundle's
/// `resolved_dependencies.extensions[]` carries (§5g.5 §3; ADR-0064 D7).
/// Pure over `(definition document, store, snapshot)` — the same inputs
/// re-derive the same snapshot (the AC-R-2.8.5-10 R1 property).
#[derive(Debug, Clone, PartialEq)]
pub struct TrustSnapshot {
    /// The bound extension entries, sorted by `extension_id` (canonical
    /// order — the fold is order-insensitive).
    pub extensions: Vec<TrustSnapshotEntry>,
    /// The `RegistryPolicy` digest the snapshot pins (`policy version_id`).
    pub policy_version_id: String,
    /// The registry snapshot the resolution ran under.
    pub registry_snapshot_id: String,
}

impl TrustSnapshot {
    /// The canonical JSON document.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "extensions",
                Json::Arr(self.extensions.iter().map(|e| e.to_json()).collect()),
            ),
            (
                "policy_version_id",
                Json::str(self.policy_version_id.clone()),
            ),
            (
                "registry_snapshot_id",
                Json::str(self.registry_snapshot_id.clone()),
            ),
        ])
    }

    /// The snapshot's content id (`idp/1` over the canonical document —
    /// re-derivation compares these).
    pub fn snapshot_id(&self) -> String {
        idp::idp_id(
            "registry.trust_snapshot",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

/// `trust_snapshot(store, definition, snapshot)` — project the definition's
/// resolved `assembly.extensions.refs[]` into the bundle-carrying snapshot.
/// Every ref's `extension_id` must resolve to an `extension` record in
/// `store` (a selector left in a sealed form or an unresolvable pin is
/// `Unresolved`/`UnknownVersion` — never silently skipped, CC3).
pub fn trust_snapshot(
    store: &crate::store::RegistryStore,
    definition: &Json,
    snapshot: &crate::records::RegistrySnapshot,
) -> Result<TrustSnapshot, RegistryError> {
    let refs = definition
        .get("assembly")
        .and_then(|a| a.get("extensions"))
        .and_then(|e| e.get("refs"))
        .and_then(|r| match r {
            Json::Arr(items) => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or(&[]);
    let mut extensions = Vec::with_capacity(refs.len());
    for (i, r) in refs.iter().enumerate() {
        let path = format!("assembly.extensions.refs[{i}]");
        let extension_id = r
            .get("extension_id")
            .and_then(Json::as_str)
            .ok_or_else(|| RegistryError::Unresolved {
                detail: format!("{path}.extension_id: unresolved ref in a sealed form"),
            })?
            .to_string();
        let (env, record) =
            store
                .get(&extension_id)
                .ok_or_else(|| RegistryError::UnknownVersion {
                    version_id: extension_id.clone(),
                })?;
        let crate::records::RegistryRecord::Extension(rec) = record else {
            return Err(RegistryError::KindMismatch {
                detail: format!(
                    "{path}: pin names a {} record, not an extension",
                    record.kind().domain_tag()
                ),
            });
        };
        extensions.push(TrustSnapshotEntry {
            extension_id: env.version_id.clone(),
            content: rec.content.id(),
            trust_record: env
                .trust_record_ref
                .clone()
                .unwrap_or_else(|| env.version_id.clone()),
            attestation_refs: rec
                .trust
                .attestations
                .iter()
                .map(|a| a.subject_hash.clone())
                .collect(),
            surface_pin: rec.trust.surface_pin.as_ref().map(|c| c.id()),
        });
    }
    extensions.sort_by(|a, b| a.extension_id.cmp(&b.extension_id));
    Ok(TrustSnapshot {
        extensions,
        policy_version_id: snapshot.policy_digest.clone(),
        registry_snapshot_id: snapshot.snapshot_id.clone(),
    })
}

#[cfg(test)]
mod tests;
