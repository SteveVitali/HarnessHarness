//! The hosted-child composition seam (§5e.3 T7; §5g.1; C-10; ADR-0164/0165;
//! AC-R-2.8.1-4, AC-R-2.1.5-6).
//!
//! `ChildProcess::Hosted(Ref<OpaqueProcess>)` names an *opaque* participant:
//! the kernel never resolves the record itself — resolution, session
//! mechanics and verb dispatch live behind [`HostedPlane`], the JSON verb
//! seam the hosting side (hh-hosting's `HostingService`, or a test double)
//! implements. The kernel's view is exactly three verbs — `open_child` at
//! spawn, `send_verb` while the child runs, `cancel` at the boundary.
//!
//! C-10's failure model is boundary-shaped: a hosted child fails through the
//! `cancel` verb, an *uncovered* call — one the participant's declared
//! capability vector does not cover, or a spelling the ABI does not admit —
//! is refused at the boundary as [`HostedPlaneError::UncoveredCall`] /
//! [`HostedPlaneError::UnknownVerb`]; undeclared capabilities are `unknown`
//! and never granted (ADR-0053 D-4). The kernel forwards typed refusals; it
//! never upgrades them.

use hh_wire::json::Json;

/// What `open_child` returns when the plane admits the hosted child.
#[derive(Debug, Clone, PartialEq)]
pub struct HostedOpen {
    /// The plane-side session ref — opaque to the kernel, the boundary
    /// handle every later `send_verb`/`cancel` names.
    pub session_ref: String,
    /// The resolved hosting mechanism (hh-ontology's spelling —
    /// `session_abi`/`model_boundary_intercept`/`container_installed`).
    /// Lands on the child manifest's `hosting_mechanism`.
    pub hosting_mechanism: String,
    /// The resolved capability-declaration record ref, when the plane
    /// resolves one (manifest `capability_declaration_ref`).
    pub capability_declaration_ref: Option<String>,
}

impl HostedOpen {
    /// The ledger-member form (`spawned.hosted`).
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("session_ref", Json::str(&self.session_ref)),
            ("hosting_mechanism", Json::str(&self.hosting_mechanism)),
        ];
        if let Some(r) = &self.capability_declaration_ref {
            m.push(("capability_declaration_ref", Json::str(r)));
        }
        Json::obj(m)
    }

    /// Decode the `spawned.hosted` member (the adopt-the-interrupted-spawn
    /// path rebuilds the manifest from the durable row, never from a second
    /// `open_child` — the plane session is already live).
    pub fn from_json(j: &Json) -> Option<HostedOpen> {
        Some(HostedOpen {
            session_ref: j.get("session_ref")?.as_str()?.to_string(),
            hosting_mechanism: j.get("hosting_mechanism")?.as_str()?.to_string(),
            capability_declaration_ref: j
                .get("capability_declaration_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }
}

/// The seam's typed refusal/error sum (boundary errors are typed on the
/// plane; the kernel maps them without reinterpretation).
#[derive(Debug, Clone, PartialEq)]
pub enum HostedPlaneError {
    /// A verb the participant's declared capability vector does not cover —
    /// refused at the boundary (AC-R-2.8.1-4's hosted clause; ADR-0053
    /// D-4's "undeclared ⇒ `unknown`, never granted").
    UncoveredCall {
        /// The refused verb.
        verb: String,
    },
    /// A spelling the Hosting ABI does not admit at all.
    UnknownVerb {
        /// The refused spelling.
        verb: String,
    },
    /// The spec's `process.hosted` ref did not resolve to a hosted
    /// `OpaqueProcess` — a definition error the kernel reports as
    /// `DefinitionUnresolvable`.
    ProcessUnresolvable {
        /// The human-readable cause.
        detail: String,
    },
    /// Any other plane-side failure (channel lost, boundary down).
    Plane {
        /// The human-readable cause.
        detail: String,
    },
}

impl std::fmt::Display for HostedPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostedPlaneError::UncoveredCall { verb } => {
                write!(f, "uncovered call refused at the boundary: {verb}")
            }
            HostedPlaneError::UnknownVerb { verb } => {
                write!(f, "verb not admitted by the Hosting ABI: {verb}")
            }
            HostedPlaneError::ProcessUnresolvable { detail } => {
                write!(f, "hosted process unresolvable: {detail}")
            }
            HostedPlaneError::Plane { detail } => write!(f, "hosted plane: {detail}"),
        }
    }
}

/// `HostedPlane` — the JSON seam a hosted child composes through. Object-
/// safe: `SpawnCtx` carries `Option<&mut dyn HostedPlane>` and a `hosted`
/// spec without one refuses `ModeUnsupported` — never a silent degrade to
/// native composition.
///
/// Implementations must make `open_child` **idempotent on `child_run_id`**
/// (carried inside `spec`): the kernel's atomicity guarantee ends at the
/// spawned row, and a retried spawn must land on the same session, never a
/// second one (KP-21 at the boundary).
pub trait HostedPlane {
    /// `open` — resolve the spec's `process.hosted` ref to the sealed
    /// `OpaqueProcess`, open the plane-side session under the delegated
    /// budget/permissions, and return the session handle data the kernel
    /// ledgers on `control.subagent.spawned{hosted{…}}` and the child
    /// manifest.
    ///
    /// `spec` is the `SubagentSpec`'s canonical JSON plus `child_run_id`
    /// (the deterministic `sub-<H(decision, H(spec))>` id — the plane's
    /// idempotency key).
    fn open_child(&mut self, spec: &Json) -> Result<HostedOpen, HostedPlaneError>;

    /// One verb send while the child runs. Coverage is the plane's call —
    /// an uncovered verb returns [`HostedPlaneError::UncoveredCall`], an
    /// ABI-foreign spelling [`HostedPlaneError::UnknownVerb`]; the kernel
    /// never decides capability itself.
    fn send_verb(
        &mut self,
        session_ref: &str,
        verb: &str,
        params: &Json,
    ) -> Result<Json, HostedPlaneError>;

    /// `cancel` — the C-10 failure verb: a hosted child is torn down at the
    /// boundary; nothing reaches inside the opaque process.
    fn cancel(&mut self, session_ref: &str) -> Result<Json, HostedPlaneError>;
}
