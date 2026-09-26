//! The §8.4 contract surface (ADR-0180 D3; ADR-0182 D1/D2; owner §08.4 per the
//! §8.5 shared-schema index):
//!
//! - [`ContractRef`] — `{kind ∈ {class_contract, record_kind, dialect,
//!   protocol_binding}, id, version_range}`: the **only** form a dependency may
//!   take (X1). Anything not reachable through one is an internal.
//! - [`ContractVersionPolicy`] — `{contract, supported[], sunset:
//!   map<version, date|stage>, additive_only}`: the `hh/`-namespace MUST-data
//!   the resolver checks against (carried on `RegistryPolicy` —
//!   `contract_version_policies`; a dedicated record *kind* would be a
//!   `registry/2` dialect bump — ADR-0262).
//! - [`check_compatibility`] — `requires` ∩ `supported` − past-sunset, run
//!   inside `resolve` **before** any executable is fetched, unpacked or
//!   launched; every failure is a typed [`ContractIncompatible`{ref,
//!   offered_range, supported_range, sunset?}].
//! - [`negotiate_version`] — the highest-common-version rule `hello` uses at
//!   Stage 2 (a plugin offering two contract versions negotiates the highest
//!   common one — AC-2's pure half).
//!
//! Additive contract changes never bump `contract_version` (they are tri-state
//! capabilities negotiated at `hello` — T-LCD-07); a breaking change is a new
//! `contract_version` with a sunset window during which both are supported.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use hh_wire::json::Json;

// ── ContractRef ───────────────────────────────────────────────────────────────

/// `ContractRef.kind` — the closed four-member sum (ADR-0182 D2). Growth is a
/// `plugin_abi`/manifest dialect bump, never an open string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractRefKind {
    /// A component-class contract (`ClassRecord.contract_version`).
    ClassContract,
    /// A registry record kind (`registry/1` kinds only — `RecordKind` growth is
    /// a `registry/2` bump).
    RecordKind,
    /// A record/document dialect (`hir/1`, `registry/1`, `idp/1`, …).
    Dialect,
    /// A protocol binding (`plugin_abi/1`, `hh-embed/1`, …).
    ProtocolBinding,
}

impl ContractRefKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContractRefKind::ClassContract => "class_contract",
            ContractRefKind::RecordKind => "record_kind",
            ContractRefKind::Dialect => "dialect",
            ContractRefKind::ProtocolBinding => "protocol_binding",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<ContractRefKind> {
        Some(match s {
            "class_contract" => ContractRefKind::ClassContract,
            "record_kind" => ContractRefKind::RecordKind,
            "dialect" => ContractRefKind::Dialect,
            "protocol_binding" => ContractRefKind::ProtocolBinding,
            _ => return None,
        })
    }
}

/// `ContractRef{kind, id, version_range}` (§8.4 §3) — the only dependency form
/// (X1). A reference names a *contract* — a class contract, a record kind, a
/// dialect or a protocol binding — never an implementation, a process, a file
/// or a private record. `id` spellings: class ids (`control_strategy`), the
/// registry/1 kind spellings (`variant`, `extension`, …), dialect names
/// (`hir/1`), binding names (`plugin_abi/1`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContractRef {
    /// The contract kind.
    pub kind: ContractRefKind,
    /// The contract identifier.
    pub id: String,
    /// The required version range ([`VersionRange`] grammar).
    pub version_range: String,
}

impl ContractRef {
    /// Construct a `ContractRef`.
    pub fn new(kind: ContractRefKind, id: impl Into<String>, range: impl Into<String>) -> Self {
        ContractRef {
            kind,
            id: id.into(),
            version_range: range.into(),
        }
    }

    /// `dialect:<name>` convenience.
    pub fn dialect(id: impl Into<String>, range: impl Into<String>) -> Self {
        Self::new(ContractRefKind::Dialect, id, range)
    }

    /// `record_kind:<kind>` convenience.
    pub fn record_kind(id: impl Into<String>, range: impl Into<String>) -> Self {
        Self::new(ContractRefKind::RecordKind, id, range)
    }

    /// `protocol_binding:<name>` convenience.
    pub fn protocol_binding(id: impl Into<String>, range: impl Into<String>) -> Self {
        Self::new(ContractRefKind::ProtocolBinding, id, range)
    }

    /// `class_contract:<class_id>` convenience.
    pub fn class_contract(id: impl Into<String>, range: impl Into<String>) -> Self {
        Self::new(ContractRefKind::ClassContract, id, range)
    }

    /// The canonical spelling `kind:id` (identity coordinate for diagnostics
    /// and the DAG report).
    pub fn label(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }

    /// The canonical JSON member `{kind, id, version_range}`.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.as_str())),
            ("id", Json::str(self.id.clone())),
            ("version_range", Json::str(self.version_range.clone())),
        ])
    }

    /// Decode a `ContractRef` member. Strict: unknown members and unknown kinds
    /// fail.
    pub fn from_json(j: &Json, path: &str) -> Result<ContractRef, ContractRefError> {
        let bad = |detail: &str| ContractRefError::SchemaViolation {
            path: path.to_string(),
            detail: detail.to_string(),
        };
        let Json::Obj(m) = j else {
            return Err(bad("expected object"));
        };
        for k in m.keys() {
            if !matches!(k.as_str(), "kind" | "id" | "version_range" | "ext") {
                return Err(bad(&format!("unknown member `{k}`")));
            }
        }
        let kind_s = m
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("`kind` missing or not a string"))?;
        let kind = ContractRefKind::parse(kind_s).ok_or_else(|| {
            bad(&format!(
                "`kind` `{kind_s}` is not in the closed ContractRef kind sum"
            ))
        })?;
        let id = m
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("`id` missing or not a string"))?;
        if id.is_empty() {
            return Err(bad("`id` is empty"));
        }
        let version_range = m
            .get("version_range")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("`version_range` missing or not a string"))?;
        VersionRange::parse(version_range)
            .ok_or_else(|| bad(&format!("`version_range` `{version_range}` is malformed")))?;
        Ok(ContractRef {
            kind,
            id: id.to_string(),
            version_range: version_range.to_string(),
        })
    }
}

/// A `ContractRef` decode failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractRefError {
    /// The member is not schema-valid.
    SchemaViolation {
        /// The member path.
        path: String,
        /// What was wrong.
        detail: String,
    },
}

// ── VersionRange ─────────────────────────────────────────────────────────────

/// The ONE contract-version range grammar (CC1 — the same grammar
/// `registry/1`'s `contract_range` uses): `*` or empty (any), `>=v`, `<=v`,
/// `lo-hi` (inclusive), or an exact dotted-numeric version.
///
/// Versions compare as dot-separated numerals, shorter-is-less on equal
/// prefixes (`semver_cmp` — the same ordering `resolve` uses for labels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRange {
    /// Any version.
    Any,
    /// `>= v`.
    AtLeast(String),
    /// `<= v`.
    AtMost(String),
    /// `lo-hi` inclusive.
    Between(String, String),
    /// Exactly `v`.
    Exact(String),
}

impl VersionRange {
    /// Parse the grammar; `None` on a malformed range.
    pub fn parse(s: &str) -> Option<VersionRange> {
        let s = s.trim();
        if s.is_empty() || s == "*" {
            return Some(VersionRange::Any);
        }
        if let Some(v) = s.strip_prefix(">=") {
            return valid_version(v).then(|| VersionRange::AtLeast(v.to_string()));
        }
        if let Some(v) = s.strip_prefix("<=") {
            return valid_version(v).then(|| VersionRange::AtMost(v.to_string()));
        }
        if let Some((lo, hi)) = s.split_once('-') {
            return (valid_version(lo) && valid_version(hi))
                .then(|| VersionRange::Between(lo.to_string(), hi.to_string()));
        }
        valid_version(s).then(|| VersionRange::Exact(s.to_string()))
    }

    /// Whether `version` satisfies the range.
    pub fn covers(&self, version: &str) -> bool {
        match self {
            VersionRange::Any => true,
            VersionRange::AtLeast(lo) => version_cmp(version, lo) != Ordering::Less,
            VersionRange::AtMost(hi) => version_cmp(version, hi) != Ordering::Greater,
            VersionRange::Between(lo, hi) => {
                version_cmp(version, lo) != Ordering::Less
                    && version_cmp(version, hi) != Ordering::Greater
            }
            VersionRange::Exact(v) => version_cmp(version, v) == Ordering::Equal,
        }
    }
}

/// The free-function form `registry/1`'s `contract_range` check delegates to
/// (CC1 — one range grammar). Semantics identical to the pre-S1.27 store copy:
/// `*`/empty covers all; `v` exact; `a-b` inclusive bounds; `>=v`/`<=v` open
/// bounds; versions compare as numeric tuples (unparseable segments compare as
/// 0 — deterministic, never a parse failure at check time).
pub fn range_covers(range: &str, version: &str) -> bool {
    let range = range.trim();
    if range == "*" || range.is_empty() {
        return true;
    }
    if let Some(v) = range.strip_prefix(">=") {
        return version_cmp(version, v.trim()) != Ordering::Less;
    }
    if let Some(v) = range.strip_prefix("<=") {
        return version_cmp(version, v.trim()) != Ordering::Greater;
    }
    if let Some((a, b)) = range.split_once('-') {
        return version_cmp(version, a.trim()) != Ordering::Less
            && version_cmp(version, b.trim()) != Ordering::Greater;
    }
    version_cmp(version, range) == Ordering::Equal
}

/// Whether `v` is a dotted-numeric version (`1`, `1.0`, `v2.1.3` — the `v`
/// prefix is tolerated, matching the registry's loose compare).
fn valid_version(v: &str) -> bool {
    let v = v.trim().trim_start_matches('v');
    !v.is_empty()
        && v.split('.')
            .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit()))
}

/// Dot-separated numeric compare — `v`-prefix tolerated, unparseable segments
/// compare as 0, shorter-is-less on equal prefixes (the same ordering the
/// registry store applies to `contract_range`s and the assembly resolver to
/// `version_label`s).
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    parse(a).cmp(&parse(b))
}

/// `hello`-time negotiation (AC-2, pure half): the highest version in
/// `supported` that `range` covers and that is not past `sunset` at `now`.
/// Deterministic (dotted-numeric order, ties impossible — versions are unique).
pub fn negotiate_version(
    range: &str,
    supported: &[String],
    sunset: &BTreeMap<String, SunsetBound>,
    now: &EvalPoint,
) -> Option<String> {
    supported
        .iter()
        .filter(|v| {
            VersionRange::parse(range)
                .map(|r| r.covers(v))
                .unwrap_or(false)
        })
        .filter(|v| !sunset.get(*v).map(|b| b.past(now)).unwrap_or(false))
        .max_by(|a, b| version_cmp(a, b))
        .cloned()
}

// ── ContractVersionPolicy ─────────────────────────────────────────────────────

/// A `sunset` bound (§8.4 §3 `sunset: map<version, date | stage>`): either an
/// ISO date (`YYYY-MM-DD`) or a build stage (`stage:N`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SunsetBound {
    /// A calendar date — the version is unsupported once `now.date > bound`.
    Date(String),
    /// A build stage — the version is unsupported once `now.stage > bound`.
    Stage(u8),
}

impl SunsetBound {
    /// Parse a bound spelling (`YYYY-MM-DD` or `stage:N`).
    pub fn parse(s: &str) -> Option<SunsetBound> {
        if let Some(n) = s.strip_prefix("stage:") {
            return n.parse::<u8>().ok().map(SunsetBound::Stage);
        }
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 3
            && parts[0].len() == 4
            && parts[1].len() == 2
            && parts[2].len() == 2
            && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Some(SunsetBound::Date(s.to_string()));
        }
        None
    }

    /// The canonical spelling.
    pub fn as_str(&self) -> String {
        match self {
            SunsetBound::Date(d) => d.clone(),
            SunsetBound::Stage(n) => format!("stage:{n}"),
        }
    }

    /// Whether the bound is in the past at `now`. A `Date` bound evaluates
    /// only when the evaluation point carries a date — an unevaluable bound is
    /// *not* treated as past (the dated refusal needs a date; a caller that
    /// cannot supply one cannot prove "past" — ADR-0262).
    pub fn past(&self, now: &EvalPoint) -> bool {
        match self {
            SunsetBound::Date(d) => now
                .date
                .as_deref()
                .map(|nd| nd > d.as_str())
                .unwrap_or(false),
            SunsetBound::Stage(n) => now.stage > *n,
        }
    }
}

/// The evaluation point a sunset is checked against (`{date?, stage}` — the
/// stage is always known; the date is caller-supplied because `resolve` has no
/// wall clock).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalPoint {
    /// The evaluation date (`YYYY-MM-DD`), when the caller carries one.
    pub date: Option<String>,
    /// The current build stage.
    pub stage: u8,
}

impl EvalPoint {
    /// The kernel's own evaluation point: the current build stage, no date
    /// (stage bounds still evaluate — the kernel's own policies use them).
    pub fn kernel() -> EvalPoint {
        EvalPoint {
            date: None,
            stage: CURRENT_STAGE,
        }
    }
}

/// The build stage this workspace implements.
pub const CURRENT_STAGE: u8 = 1;

/// `ContractVersionPolicy{contract, supported[], sunset, additive_only}`
/// (§8.4 §3; ADR-0180 D3). A `hh/`-namespace MUST-data record — carried on
/// `RegistryPolicy.contract_version_policies` (a dedicated `RecordKind` would
/// be a `registry/2` dialect bump — ADR-0262). `additive_only` pins the
/// negotiation discipline: within a `contract` id only additive changes are
/// admissible — a breaking change is a NEW contract version id
/// (`plugin_abi/2`, `registry/2`), never an in-place edit.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractVersionPolicy {
    /// The contract this policy governs (`kind` + `id` are the key;
    /// `version_range` is informational).
    pub contract: ContractRef,
    /// The versions this kernel supports (e.g. `["1"]`, or `["1","2"]` inside a
    /// sunset window).
    pub supported: Vec<String>,
    /// Per-version sunset bounds (`date | stage`).
    pub sunset: BTreeMap<String, SunsetBound>,
    /// Whether only additive changes are admissible within this contract
    /// (always true at Stage 1 — T-LCD-07).
    pub additive_only: bool,
}

impl ContractVersionPolicy {
    /// The canonical JSON member.
    pub fn to_json(&self) -> Json {
        let sunset: BTreeMap<String, Json> = self
            .sunset
            .iter()
            .map(|(v, b)| (v.clone(), Json::str(b.as_str())))
            .collect();
        Json::Obj(BTreeMap::from([
            ("contract".to_string(), self.contract.to_json()),
            (
                "supported".to_string(),
                Json::Arr(
                    self.supported
                        .iter()
                        .map(|v| Json::str(v.clone()))
                        .collect(),
                ),
            ),
            ("sunset".to_string(), Json::Obj(sunset)),
            ("additive_only".to_string(), Json::Bool(self.additive_only)),
        ]))
    }

    /// Decode a policy member (strict — unknown members fail).
    pub fn from_json(j: &Json, path: &str) -> Result<ContractVersionPolicy, ContractRefError> {
        let bad = |detail: String| ContractRefError::SchemaViolation {
            path: path.to_string(),
            detail,
        };
        let Json::Obj(m) = j else {
            return Err(bad("expected object".to_string()));
        };
        for k in m.keys() {
            if !matches!(
                k.as_str(),
                "contract" | "supported" | "sunset" | "additive_only" | "ext"
            ) {
                return Err(bad(format!("unknown member `{k}`")));
            }
        }
        let contract = ContractRef::from_json(
            m.get("contract")
                .ok_or_else(|| bad("`contract` missing".to_string()))?,
            &format!("{path}.contract"),
        )?;
        let supported: Vec<String> = match m.get("supported") {
            Some(Json::Arr(a)) => {
                let mut out = Vec::new();
                for (i, v) in a.iter().enumerate() {
                    let s = v
                        .as_str()
                        .ok_or_else(|| bad(format!("supported[{i}] is not a string")))?;
                    if !valid_version(s) {
                        return Err(bad(format!("supported[{i}] `{s}` is not a version")));
                    }
                    out.push(s.to_string());
                }
                out
            }
            _ => return Err(bad("`supported` missing or not an array".to_string())),
        };
        let mut sunset = BTreeMap::new();
        if let Some(Json::Obj(sm)) = m.get("sunset") {
            for (v, b) in sm {
                let s = b
                    .as_str()
                    .ok_or_else(|| bad(format!("sunset.{v} is not a string")))?;
                let bound = SunsetBound::parse(s).ok_or_else(|| {
                    bad(format!("sunset.{v} `{s}` is not `YYYY-MM-DD` or `stage:N`"))
                })?;
                sunset.insert(v.clone(), bound);
            }
        }
        let additive_only = match m.get("additive_only") {
            Some(Json::Bool(b)) => *b,
            None => true,
            _ => return Err(bad("`additive_only` is not a bool".to_string())),
        };
        Ok(ContractVersionPolicy {
            contract,
            supported,
            sunset,
            additive_only,
        })
    }
}

/// The kernel's own `hh/`-namespace contract policies (the Stage-1 contract
/// set): the dialects this build writes and the two boundary bindings.
/// `plugin_abi/1` and `hh-embed/1` are declared now (the schemas are the
/// contract — execution lands with the host at Stage 2 / the embed service at
/// S1.25). `hh-hosting/1` is deliberately absent — the Hosting ABI is a
/// different binding (CF-208) and no `ContractRef` may name it (AC-12; a
/// `requires` naming it finds no policy → `ContractIncompatible`).
pub fn kernel_contract_policies() -> Vec<ContractVersionPolicy> {
    let mut out = Vec::new();
    for id in ["hir/1", "idp/1", "registry/1", "hh-experiment/1"] {
        out.push(ContractVersionPolicy {
            contract: ContractRef::dialect(id, "*"),
            supported: vec!["1".to_string()],
            sunset: BTreeMap::new(),
            additive_only: true,
        });
    }
    for id in ["plugin_abi/1", "hh-embed/1"] {
        out.push(ContractVersionPolicy {
            contract: ContractRef::protocol_binding(id, "*"),
            supported: vec!["1".to_string()],
            sunset: BTreeMap::new(),
            additive_only: true,
        });
    }
    out
}

// ── check_compatibility ───────────────────────────────────────────────────────

/// A class contract as the catalog reports it: the registered
/// `ClassRecord.contract_version` (the supported set is the singleton —
/// contract evolution registers a new `ClassRecord` *version*) plus an
/// optional sunset a `class_contract:<id>` policy may attach.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassContract {
    /// The class id.
    pub class_id: String,
    /// The contract version this `ClassRecord` version declares.
    pub contract_version: String,
    /// The class's defining tier (`ClassRecord.tier` — ADR-0182 D1).
    pub tier: u8,
    /// The β decision points the class's variants may occupy.
    pub decision_points: Vec<String>,
}

/// The typed incompatibility (§8.4 §2 `ContractIncompatible{ref,
/// offered_range, supported_range, sunset?}`).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractIncompatible {
    /// The contract that failed.
    pub contract: ContractRef,
    /// The range the plugin offered/required.
    pub offered_range: String,
    /// The versions the kernel supports (rendered; empty when the contract is
    /// unknown to every policy).
    pub supported_range: String,
    /// The sunset bound that expired, when the failure is a sunset refusal —
    /// the *dated reason* (e.g. `"2027-01-01"` / `"stage:2"`).
    pub sunset: Option<String>,
}

impl ContractIncompatible {
    /// The one-line rendering carried in diagnostics/`RegistryError` detail.
    pub fn detail(&self) -> String {
        let mut s = format!(
            "{} offered {} vs supported [{}]",
            self.contract.label(),
            self.offered_range,
            self.supported_range
        );
        if let Some(b) = &self.sunset {
            s.push_str(&format!("; sunset {b}"));
        }
        s
    }
}

/// The successful result: every required contract with its negotiated
/// (highest common) version — what `hello` records in
/// `negotiated.contract_versions_chosen` at Stage 2.
#[derive(Debug, Clone, PartialEq)]
pub struct Negotiation {
    /// `(contract label, chosen version)` in contract-declaration order.
    pub chosen: Vec<(ContractRef, String)>,
}

/// `check_compatibility(requires, policies, class_catalog) → Negotiation |
/// [ContractIncompatible]` — §8.4 §2. Every `requires` ref (the manifest's
/// `hir_dialect`/`registry_dialect`/`plugin_abi` ranges materialised as
/// `ContractRef`s plus `requires.contracts[]`) and every contribution
/// `contract_range` must intersect a `supported` version not past its sunset.
/// The report is **complete** — every incompatibility is collected, in
/// declaration order, before refusal (the resolver never half-admits).
///
/// Contract resolution: `class_contract` refs consult the class catalog (the
/// registered `ClassRecord.contract_version`, plus a `class_contract:<id>`
/// policy's sunset when one exists); every other kind consults `policies`
/// (the kernel table ∪ `RegistryPolicy.contract_version_policies`). An unknown
/// contract — no policy, no class — is `ContractIncompatible` with an empty
/// `supported_range` (that is how a `hh-hosting/1` reference fails — AC-12).
pub fn check_compatibility(
    requires: &[ContractRef],
    policies: &[ContractVersionPolicy],
    classes: &[ClassContract],
    now: &EvalPoint,
) -> Result<Negotiation, Vec<ContractIncompatible>> {
    let mut chosen = Vec::new();
    let mut failures = Vec::new();
    for r in requires {
        // The supported set + sunset map for this contract.
        let (supported, sunset): (Vec<String>, BTreeMap<String, SunsetBound>) =
            if r.kind == ContractRefKind::ClassContract {
                let sunset = policies
                    .iter()
                    .find(|p| p.contract.kind == r.kind && p.contract.id == r.id)
                    .map(|p| p.sunset.clone())
                    .unwrap_or_default();
                match classes.iter().find(|c| c.class_id == r.id) {
                    Some(c) => (vec![c.contract_version.clone()], sunset),
                    None => (Vec::new(), sunset),
                }
            } else {
                match policies
                    .iter()
                    .find(|p| p.contract.kind == r.kind && p.contract.id == r.id)
                {
                    Some(p) => (p.supported.clone(), p.sunset.clone()),
                    None => (Vec::new(), BTreeMap::new()),
                }
            };
        let supported_render = supported.join(",");
        match negotiate_version(&r.version_range, &supported, &sunset, now) {
            Some(v) => chosen.push((r.clone(), v)),
            None => {
                // Sunset or no intersection? If some in-range supported version
                // is excluded only because its bound is past, this is the dated
                // sunset refusal.
                let expired = supported
                    .iter()
                    .filter(|v| {
                        VersionRange::parse(&r.version_range)
                            .map(|rg| rg.covers(v))
                            .unwrap_or(false)
                    })
                    .filter_map(|v| sunset.get(v))
                    .filter(|b| b.past(now))
                    .map(|b| b.as_str())
                    .max();
                failures.push(ContractIncompatible {
                    contract: r.clone(),
                    offered_range: r.version_range.clone(),
                    supported_range: supported_render,
                    sunset: expired,
                });
            }
        }
    }
    if failures.is_empty() {
        Ok(Negotiation { chosen })
    } else {
        Err(failures)
    }
}

// ── defining tiers (X2) ───────────────────────────────────────────────────────

/// The *defining tier* of a contract — what the spec-DAG check (X2) compares
/// against a depender's tier ("a C(n) item's `depends_on` names only contracts
/// whose defining tier is ≤ n").
///
/// - `dialect:*`, `record_kind:*`, `protocol_binding:{plugin_abi/1,
///   hh-embed/1}` are C0 — the kernel substrate contracts.
/// - `protocol_binding:hh-hosting/1` is C2 — the hosted binding; a C0/C1
///   depender naming it is a tier violation (and an AC-12 violation besides).
/// - `class_contract:<id>` takes the class's own declared tier
///   (`ClassRecord.tier` — "a `ClassRecord`'s tier is the tier that defines
///   the contract").
/// - anything else is `None` — the report lists it in `unknown_refs` (a
///   dependency nameable by no contract is caught, never silently ignored —
///   CC3/CC5).
pub fn contract_defining_tier(r: &ContractRef, classes: &[ClassContract]) -> Option<u8> {
    match r.kind {
        ContractRefKind::Dialect | ContractRefKind::RecordKind => Some(0),
        ContractRefKind::ProtocolBinding => match r.id.as_str() {
            "hh-hosting/1" => Some(2),
            _ => Some(0),
        },
        ContractRefKind::ClassContract => {
            classes.iter().find(|c| c.class_id == r.id).map(|c| c.tier)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(id: &str, supported: &[&str]) -> ContractVersionPolicy {
        ContractVersionPolicy {
            contract: ContractRef::protocol_binding(id, "*"),
            supported: supported.iter().map(|s| s.to_string()).collect(),
            sunset: BTreeMap::new(),
            additive_only: true,
        }
    }

    #[test]
    fn range_grammar() {
        assert!(range_covers("*", "9.9"));
        assert!(range_covers("", "1.0"));
        assert!(range_covers(">=1.0", "1.4"));
        assert!(!range_covers(">=1.0", "0.9"));
        assert!(range_covers("<=1.5", "1.5"));
        assert!(!range_covers("<=1.5", "1.6"));
        assert!(range_covers("1.0-1.9", "1.4"));
        assert!(!range_covers("1.0-1.9", "2.0"));
        assert!(range_covers("1.0", "1.0"));
        assert!(!range_covers("1.0", "1.0.1"));
        assert!(!range_covers("nonsense", "1.0"));
        assert_eq!(VersionRange::parse("1-a"), None);
    }

    #[test]
    fn negotiate_picks_highest_common() {
        // AC-2's pure half: two offered contract versions → highest common.
        let sunset = BTreeMap::new();
        let now = EvalPoint::kernel();
        assert_eq!(
            negotiate_version("1-2", &["1".into(), "2".into()], &sunset, &now),
            Some("2".to_string())
        );
        assert_eq!(
            negotiate_version(">=3", &["1".into(), "2".into()], &sunset, &now),
            None
        );
        // A past-sunset version is not negotiated.
        let mut sunset = BTreeMap::new();
        sunset.insert("2".to_string(), SunsetBound::Stage(0));
        assert_eq!(
            negotiate_version("1-2", &["1".into(), "2".into()], &sunset, &now),
            Some("1".to_string())
        );
    }

    #[test]
    fn compat_ok_and_negotiates() {
        let policies = vec![policy("plugin_abi/1", &["1", "2"])];
        let now = EvalPoint::kernel();
        let req = vec![ContractRef::protocol_binding("plugin_abi/1", ">=1")];
        let n = check_compatibility(&req, &policies, &[], &now).unwrap();
        assert_eq!(n.chosen[0].1, "2");
    }

    #[test]
    fn compat_miss_is_typed() {
        let policies = vec![policy("plugin_abi/1", &["1"])];
        let now = EvalPoint::kernel();
        let req = vec![ContractRef::protocol_binding("plugin_abi/1", ">=5")];
        let err = check_compatibility(&req, &policies, &[], &now).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].contract.id, "plugin_abi/1");
        assert_eq!(err[0].offered_range, ">=5");
        assert_eq!(err[0].supported_range, "1");
        assert!(err[0].sunset.is_none());
    }

    #[test]
    fn compat_unknown_contract_is_typed() {
        // AC-12: a `hh-hosting/1` ref finds no policy → ContractIncompatible.
        let now = EvalPoint::kernel();
        let req = vec![ContractRef::protocol_binding("hh-hosting/1", "*")];
        let err = check_compatibility(&req, &kernel_contract_policies(), &[], &now).unwrap_err();
        assert_eq!(err[0].contract.id, "hh-hosting/1");
        assert_eq!(err[0].supported_range, "");
    }

    #[test]
    fn compat_sunset_is_dated() {
        let mut p = policy("plugin_abi/1", &["1"]);
        p.sunset
            .insert("1".to_string(), SunsetBound::Date("2026-01-01".into()));
        let now = EvalPoint {
            date: Some("2026-09-18".to_string()),
            stage: 1,
        };
        let req = vec![ContractRef::protocol_binding("plugin_abi/1", "1")];
        let err = check_compatibility(&req, &[p], &[], &now).unwrap_err();
        assert_eq!(err[0].sunset.as_deref(), Some("2026-01-01"));
        assert!(err[0].detail().contains("sunset 2026-01-01"));
        // Before the date it negotiates.
        let earlier = EvalPoint {
            date: Some("2025-01-01".to_string()),
            stage: 1,
        };
        assert!(check_compatibility(&req, &kernel_contract_policies(), &[], &earlier).is_ok());
    }

    #[test]
    fn compat_class_contract_uses_catalog() {
        let classes = vec![ClassContract {
            class_id: "control_strategy".to_string(),
            contract_version: "1.0".to_string(),
            tier: 0,
            decision_points: vec![],
        }];
        let now = EvalPoint::kernel();
        let ok = vec![ContractRef::class_contract("control_strategy", ">=1.0")];
        assert!(check_compatibility(&ok, &[], &classes, &now).is_ok());
        let miss = vec![ContractRef::class_contract("control_strategy", ">=2.0")];
        assert!(check_compatibility(&miss, &[], &classes, &now).is_err());
        let unknown = vec![ContractRef::class_contract("nope", "*")];
        assert!(check_compatibility(&unknown, &[], &classes, &now).is_err());
    }

    #[test]
    fn contract_ref_round_trip() {
        let r = ContractRef::record_kind("extension", "1-2");
        let j = r.to_json();
        assert_eq!(ContractRef::from_json(&j, "/x"), Ok(r));
        assert!(ContractRef::from_json(
            &Json::obj([
                ("kind", Json::str("weird")),
                ("id", Json::str("x")),
                ("version_range", Json::str("*"))
            ]),
            "/x"
        )
        .is_err());
        assert!(ContractRef::from_json(
            &Json::obj([
                ("kind", Json::str("dialect")),
                ("id", Json::str("hir/1")),
                ("version_range", Json::str("*")),
                ("nope", Json::Null)
            ]),
            "/x"
        )
        .is_err());
    }

    #[test]
    fn policy_round_trip() {
        let mut p = policy("plugin_abi/1", &["1"]);
        p.sunset.insert("1".to_string(), SunsetBound::Stage(4));
        let j = p.to_json();
        assert_eq!(ContractVersionPolicy::from_json(&j, "/p"), Ok(p));
    }
}
