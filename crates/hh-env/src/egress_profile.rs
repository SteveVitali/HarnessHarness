//! The `declared_deps` cell-egress admission (R5.56; ADR-0421) — the
//! sanctioned form of the S4 egress supplement's scratch machinery
//! (`docs/build/runs/S4-EGRESS.md`, `hh-s4-egress-supplement/1`; the
//! scratch variant `/tmp/s4-helper-egress` is superseded by the first-class
//! `--network` flag this profile drives).
//!
//! A task that *declares* runtime egress deps (apt/pip/uv bootstrap,
//! `git clone`, `curl`/`wget`, hosted-model downloads, remote RPC) may be
//! admitted to run its `local_container` cell on `podman --network
//! bridge` — outbound NAT through the podman VM's slirp4netns. The
//! admission is honest on both records:
//!
//! - the attached `ContainmentPolicy` declares `net.mode = public` with
//!   the N4 `residual_channels[host_bridge]` entry (kind, statement,
//!   owner, closer) — a `public` mode without a declared residual still
//!   fails `PolicyError::PublicWithoutResidual`, and the fs/proc bounds
//!   (pids limit, `no-new-privileges`, `/work` mount) are untouched;
//! - the cell spec (`EnvironmentRecord.ext`) records the supplement's
//!   field spellings verbatim — `egress_profile: declared_deps`,
//!   `cell_network: bridge`, `policy_net_mode: public` — plus the
//!   declared deps the admission stood on. A bridge cell is never
//!   labelled `none`.
//!
//! `none` stays the default posture: only cells whose tasks declare the
//! profile are admitted; `mediated` keeps its own declared posture and is
//! never silently widened to bridge.

use hharness_containment::attach::AttachError;
use hharness_containment::policy::{ContainmentPolicy, NetMode, ResidualChannel, ResidualKind};
use hharness_hir::leaves::Text;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

use crate::errors::EnvError;
use crate::record::{EnvironmentClass, EnvironmentRecord};

/// The profile's record spelling — `egress_profile: declared_deps`.
pub const EGRESS_PROFILE_DECLARED_DEPS: &str = "declared_deps";
/// The admitted cell network — `cell_network: bridge`.
pub const CELL_NETWORK_BRIDGE: &str = "bridge";
/// The honest policy mode — `policy_net_mode: public`.
pub const POLICY_NET_MODE_PUBLIC: &str = "public";

/// `DeclaredDeps` — the declaration a `declared_deps` admission stands
/// on: the task's declared runtime egress deps plus the residual
/// channel's owner and closer (N4 requires the residual declared *and*
/// owned — an unowned residual fails readiness, T-LCD-05).
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredDeps {
    /// The task's declared runtime egress deps — non-empty; an empty
    /// list is an honest refusal (the cell keeps `--network none`).
    pub deps: Vec<String>,
    /// The residual channel's owner (recorded verbatim).
    pub owner: String,
    /// The residual channel's closer — the mechanism that closes the
    /// bridge at cell end (recorded verbatim).
    pub closer: String,
}

/// `podman_helper_args(container, image, workspace, policy)` — the
/// `local_container` half of the helper spawn argv (`--container/--image/
/// --workspace` plus the network flag the attached containment policy
/// selects): `net.mode = public` — the admitted `declared_deps`
/// profile — appends `--network bridge`; `none`/`mediated` leave the
/// flag absent so the helper's `none` default keeps the pre-variant argv
/// byte-identical (AC-R5.56-2; `mediated` is its own declared posture,
/// never silently widened).
pub fn podman_helper_args(
    container: &str,
    image: &str,
    workspace: &str,
    policy: &ContainmentPolicy,
) -> Vec<String> {
    let mut args = vec![
        "--container".to_string(),
        container.to_string(),
        "--image".to_string(),
        image.to_string(),
        "--workspace".to_string(),
        workspace.to_string(),
    ];
    if policy.net.mode == NetMode::Public {
        args.push("--network".to_string());
        args.push(CELL_NETWORK_BRIDGE.to_string());
    }
    args
}

/// `admit_declared_deps(record, policy, decl, at)` — the declared
/// admission: mutates `policy` to the honest `public` declaration and
/// stamps `record.ext` with the supplement's field spellings, or refuses
/// typed and leaves both untouched (the admission is transactional — a
/// refused profile never produces a half-labelled cell).
///
/// Refusals:
/// - `decl.deps` empty — no declared runtime egress deps:
///   `Unsupported{egress_profile.declared_deps}`; the cell stays `none`;
/// - `decl.owner`/`decl.closer` empty — an unowned/unclosed residual is
///   no residual: `Unsupported{egress_profile.declared_deps}`;
/// - `record.class` is not `LocalContainer` — the profile admits a
///   podman cell only: `Unsupported{egress_profile.declared_deps}`;
/// - the resulting policy fails `validate` (`PublicBelowPrincipal` when
///   the layer's authority is under principal; `PublicWithoutResidual`
///   can never fire here — the admission declares the residual itself,
///   but the validator is consulted, not bypassed):
///   `Attach(PolicyInvalid)` verbatim.
pub fn admit_declared_deps(
    record: &mut EnvironmentRecord,
    policy: &mut ContainmentPolicy,
    decl: &DeclaredDeps,
    at: u64,
) -> Result<(), EnvError> {
    if record.class != EnvironmentClass::LocalContainer {
        return Err(EnvError::Unsupported {
            capability: "egress_profile.declared_deps",
            detail: format!(
                "the declared_deps profile admits a local_container cell — \
                 {} has no podman network to bridge",
                record.class.as_str()
            ),
        });
    }
    if decl.deps.is_empty() {
        return Err(EnvError::Unsupported {
            capability: "egress_profile.declared_deps",
            detail: "no declared runtime egress deps — the cell keeps \
                     --network none (untrusted participants stay on the \
                     default posture)"
                .to_string(),
        });
    }
    if decl.owner.is_empty() || decl.closer.is_empty() {
        return Err(EnvError::Unsupported {
            capability: "egress_profile.declared_deps",
            detail: "the host_bridge residual requires a declared owner \
                     and closer (N4/T-LCD-05)"
                .to_string(),
        });
    }
    // Mutate a candidate, validate, then commit — a refused admission
    // leaves the policy byte-identical.
    let mut candidate = policy.clone();
    candidate.net.mode = NetMode::Public;
    candidate.residual_channels.push(ResidualChannel {
        kind: ResidualKind::HostBridge,
        statement: Text::new(
            format!(
                "egress_profile: declared_deps — podman --network bridge \
                 (outbound NAT via the podman VM's slirp4netns) admitted for \
                 the task's declared runtime deps [{}]; unmediated outbound \
                 connect/DNS for the cell's life. Scoped admission recorded \
                 per cell; not a silent confinement bypass.",
                decl.deps.join(", ")
            ),
            &decl.owner,
            ProvenanceRecord::kernel("hh-env/egress_profile", at),
        ),
        owner: decl.owner.clone(),
        closer: decl.closer.clone(),
    });
    candidate.compute_ids();
    candidate
        .validate()
        .map_err(|e| EnvError::Attach(AttachError::PolicyInvalid(e)))?;
    *policy = candidate;
    // The cell spec records the supplement's field spellings verbatim —
    // stamped only after the honest policy exists (a bridge cell is
    // never labelled `none`).
    record.ext.insert(
        "egress_profile".to_string(),
        Json::str(EGRESS_PROFILE_DECLARED_DEPS.to_string()),
    );
    record.ext.insert(
        "cell_network".to_string(),
        Json::str(CELL_NETWORK_BRIDGE.to_string()),
    );
    record.ext.insert(
        "policy_net_mode".to_string(),
        Json::str(POLICY_NET_MODE_PUBLIC.to_string()),
    );
    record.ext.insert(
        "declared_deps".to_string(),
        Json::Arr(decl.deps.iter().map(|d| Json::str(d.clone())).collect()),
    );
    Ok(())
}
