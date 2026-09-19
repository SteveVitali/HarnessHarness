//! The deterministic risk assessors (§5g.1 §2.3; ADR-0052 D4): `kernel_assessed`
//! is the max over, in order, (i) the static `EffectAttributes` of the
//! capability — [`hh_hir::risk::project_risk`], (ii) deterministic argument
//! analysis over the recorded [`AssessmentInputs`], (iv) a model `self_report` —
//! **raise-only** (a "LOW" claim never lowers a kernel "HIGH"; ADR-0031 §2).
//! (iii) hook admission is R-2.8.5's Stage-2 seam — hooks may only raise or
//! deny, so the raise-only `max_by_danger` fold is the one mechanism.
//!
//! **Unparseable or partially parseable ⇒ `unknown ⇒ {irreversible,
//! non_idempotent, external}`** — `RiskClass::UNKNOWN` (ADR-0212/OQ-133: the
//! partial-parse policy is ratified; the closed command-class list is not — the
//! assessor therefore takes the *classification result* as a recorded input and
//! never parses shell text itself).

use hh_hir::kinds::EffectAttributes;
use hh_ontology::risk::RiskClass;
use hh_wire::json::Json;

/// `Tri` — the honest three-valued assessor input: `yes`, `no`, `unknown`.
/// `Unknown` never satisfies a `yes`-condition of any Π row — an unverifiable
/// fact reads as the *dangerous* answer wherever a row conditions on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tri {
    /// Verified.
    Yes,
    /// Verified absent.
    No,
    /// Unverifiable at C0 — reads as the dangerous answer (Π) and raises
    /// `unknown` in the risk fold (2.3).
    #[default]
    Unknown,
}

impl Tri {
    /// `true` only for `Yes`.
    pub fn is_yes(self) -> bool {
        matches!(self, Tri::Yes)
    }
}

/// The command-classification result — the closed-grammar parse's *output*
/// (ADR-0212/OQ-133 defers the grammar itself; the monitor consumes the
/// recorded verdict, never the command text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseOutcome {
    /// The command parsed into a declared command class.
    Parsed,
    /// The command partially parsed — `unknown`.
    Partial,
    /// The command did not parse — `unknown`.
    Failed,
}

/// `secret_access` transport — the ADR-0059 D4 row discriminator. The broker
/// reuses this one sum as its `BindingMode` (CC7 — the channel's
/// `binding_modes` member spells these tags; §5g.3 R-2.8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretTransport {
    /// Broker-injected proxy (the secret never enters context).
    ProxyInjected,
    /// A minted, scoped credential.
    MintedScoped,
    /// A wrapped long-lived credential — deny unless a sealed `HarnessRule`
    /// names channel and destination.
    WrappedLongLived,
}

impl SecretTransport {
    /// The canonical spelling (§5g.3 §2 `BindingMode`).
    pub fn as_str(self) -> &'static str {
        match self {
            SecretTransport::ProxyInjected => "proxy_injected",
            SecretTransport::MintedScoped => "minted_scoped",
            SecretTransport::WrappedLongLived => "wrapped_long_lived",
        }
    }

    /// Parse the canonical spelling (`None` for an unknown tag — never coerced).
    pub fn parse(s: &str) -> Option<SecretTransport> {
        match s {
            "proxy_injected" => Some(SecretTransport::ProxyInjected),
            "minted_scoped" => Some(SecretTransport::MintedScoped),
            "wrapped_long_lived" => Some(SecretTransport::WrappedLongLived),
            _ => None,
        }
    }
}

/// `AssessmentInputs` — the *recorded* inputs the assessors and Π rows read
/// (§5g.1 §2.3 (ii): "assessors are executor-registered pure functions run by
/// the kernel at `resolve`; the writable-root and network-host inputs are
/// supplied by R-2.8.4"). At Stage 1 the caller (the executor/resolve path)
/// computes them; the monitor never reads the arguments' text itself — every
/// field is a closed value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssessmentInputs {
    /// Path canonicalisation succeeded and the canonical path is inside a
    /// declared writable root (`Tri::Unknown` = unparseable/hard-linked —
    /// treated as outside; AC-R-2.8.1-9).
    pub inside_writable_roots: Tri,
    /// The closed-grammar command classification.
    pub command_parse: Option<ParseOutcome>,
    /// The egress host is in the definition's allowlist (R-2.8.4 input).
    pub host_allowlisted: Tri,
    /// A compensation plan is registered for the effect.
    pub compensation_registered: Tri,
    /// The covering grant also covers the compensator (Π-4).
    pub compensator_covered: Tri,
    /// The `message_human` sole recipient is the run's principal.
    pub sole_recipient_principal: Tri,
    /// The `secret_access` transport, when the domain is `secret_access`.
    pub secret_transport: Option<SecretTransport>,
    /// The secret destination is inside the grant's scope.
    pub destination_in_scope: Tri,
    /// The `secret_access` destination is a model-visible sink (Π-10).
    pub model_visible_sink: Tri,
    /// The `spend` is over the soft threshold (Π-10).
    pub spend_over_soft: Tri,
    /// `spawn_process`: the requested child permission set is contained in the
    /// parent's (Π-9 / step 6).
    pub child_contained: Tri,
    /// A `definition`-issued pre-authorization handle covers the effect
    /// (ADR-0053 D5 — the unattended-`ask` exception).
    pub pre_authorized: Tri,
    /// The `memory_write` persistence scope class — `run|session|project|user`
    /// ordering for the domain row (`run` lowest).
    pub memory_scope: Option<MemoryScope>,
}

/// The `memory_write` persistence scope the domain row orders on
/// (`allow` ≤ `run`, `ask` at `session`, `deny` above — ADR-0052 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryScope {
    /// `run`.
    Run,
    /// `session`.
    Session,
    /// `project`.
    Project,
    /// `user`.
    User,
}

impl MemoryScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryScope::Run => "run",
            MemoryScope::Session => "session",
            MemoryScope::Project => "project",
            MemoryScope::User => "user",
        }
    }

    /// Parse the canonical spelling (`None` for an unknown tag — never coerced).
    pub fn parse(s: &str) -> Option<MemoryScope> {
        match s {
            "run" => Some(MemoryScope::Run),
            "session" => Some(MemoryScope::Session),
            "project" => Some(MemoryScope::Project),
            "user" => Some(MemoryScope::User),
            _ => None,
        }
    }
}

/// `kernel_assessed(declared, inputs)` — precedence step (i)+(ii): the static
/// projection, raised by deterministic argument analysis. `None` declared
/// attributes already project to `UNKNOWN` (`project_risk`); a failed or
/// partial command parse raises to `UNKNOWN`; a path outside (or unverifiably
/// inside) the writable roots raises the scope axis to `external` for the
/// `fs_*` domains — the caller records `inside_writable_roots` and this fold
/// applies it.
pub fn kernel_assessed(
    declared: Option<&EffectAttributes>,
    domain_is_fs_write: bool,
    inputs: &AssessmentInputs,
) -> RiskClass {
    let mut rc = hh_hir::risk::project_risk(declared);
    // (ii) argument analysis — raise only.
    if matches!(
        inputs.command_parse,
        Some(ParseOutcome::Partial | ParseOutcome::Failed)
    ) {
        rc = RiskClass::max_by_danger(rc, RiskClass::UNKNOWN);
    }
    if domain_is_fs_write && !inputs.inside_writable_roots.is_yes() {
        // Outside (or unverifiably inside) the writable roots — the scope axis
        // rises to `external`; never lowers.
        rc = RiskClass::max_by_danger(
            rc,
            RiskClass {
                reversibility: rc.reversibility,
                repeat_safety: rc.repeat_safety,
                scope: hh_ontology::risk::RiskScope::External,
            },
        );
    }
    rc
}

/// Step (iv) — the model `self_report` is raise-only:
/// `max_by_danger(kernel, report)` (ADR-0031 §2).
pub fn apply_self_report(kernel: RiskClass, self_report: Option<RiskClass>) -> RiskClass {
    match self_report {
        Some(r) => RiskClass::max_by_danger(kernel, r),
        None => kernel,
    }
}

/// The recorded-inputs JSON (the `assessment_inputs_ref` member content at
/// Stage 1 — a structured record, never text).
pub fn inputs_json(i: &AssessmentInputs) -> Json {
    let tri = |t: Tri| {
        Json::str(match t {
            Tri::Yes => "yes",
            Tri::No => "no",
            Tri::Unknown => "unknown",
        })
    };
    Json::obj([
        ("inside_writable_roots", tri(i.inside_writable_roots)),
        ("host_allowlisted", tri(i.host_allowlisted)),
        ("compensation_registered", tri(i.compensation_registered)),
        ("compensator_covered", tri(i.compensator_covered)),
        ("sole_recipient_principal", tri(i.sole_recipient_principal)),
        ("destination_in_scope", tri(i.destination_in_scope)),
        ("model_visible_sink", tri(i.model_visible_sink)),
        ("spend_over_soft", tri(i.spend_over_soft)),
        ("child_contained", tri(i.child_contained)),
        ("pre_authorized", tri(i.pre_authorized)),
    ])
}
