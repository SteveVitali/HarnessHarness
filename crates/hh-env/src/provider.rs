// SPDX-License-Identifier: Apache-2.0
//
//! S4.13 (R-2.2.5¹) — the `provider_hosted`/`remote_*` adapter seam.
//!
//! A provider class is provisionable *only* through a registered
//! [`ProviderAdapter`] — the kernel never embeds a provider driver (CC10):
//! `EnvDriver::provision` resolves `record.class` → adapter →
//! `adapter.provision(...)`; the returned [`ProviderHandle`] is recorded on
//! the `EnvHandle` (and on the `provisioned` payload) so suspend/resume/
//! teardown/meters route back through the same adapter. Without an adapter
//! the class refuses `EnvError::Unsupported` — the class sum is declared,
//! the mechanism is opt-in.
//!
//! The adapter *reports*; the kernel *decides* (I-3 — a meter or state hint
//! is a claim folded into the kernel's own lifecycle rows, never an
//! authority).

use std::collections::BTreeMap;

use crate::errors::EnvError;
use crate::record::EnvironmentClass;

/// The provider's handle for one provisioned environment — the kernel
/// records it verbatim (an opaque provider-side identity + the meters the
/// adapter declared at provision).
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderHandle {
    /// The adapter's own id for the environment (opaque to the kernel).
    pub remote_id: String,
    /// The adapter's region/zone claim, when reported.
    pub region: Option<String>,
    /// Adapter-reported extras (opaque; recorded verbatim).
    pub extra: BTreeMap<String, hh_wire::json::Json>,
}

/// What `provision` hands the adapter — the environment record's resolved
/// identity plus the handles the adapter may need (image ref, limits).
#[derive(Debug, Clone)]
pub struct ProvisionRequest {
    /// The `environment_ref` — `(semantic_id, version_id)`.
    pub environment_ref: (String, String),
    /// The resolved image tag/digest (canonical).
    pub image_ref: String,
    /// The kernel-allocated env handle id (the adapter may record it; it is
    /// never authority on the provider side).
    pub env_handle_id: String,
    /// The owning run.
    pub run_id: String,
}

/// A provider meter sample — `{name, value}` pairs the adapter reports;
/// `action.environment.meters_sampled` folds them into the kernel's own
/// meter payload under `provider_meters` (R-2.2.5¹ provider meters —
/// durable + auditable).
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderMeter {
    /// The declared meter name (must be in the class's
    /// `EnvCapabilityDeclaration::meters` — undeclared names are dropped,
    /// recorded as `unreported[]` on the sample row).
    pub name: String,
    /// The measured value (provider units are the adapter's declaration).
    pub value: i64,
    /// The unit spelling (`ms`, `bytes`, `calls`, `usd_millicents`, …).
    pub unit: String,
}

/// `ProviderAdapter` — the registered driver for a provider class
/// (`remote_ephemeral` | `remote_persistent` | `provider_hosted`). All
/// methods are best-effort reports: failures surface as `EnvError` and the
/// kernel records them; nothing the adapter returns is an authority.
pub trait ProviderAdapter {
    /// The class this adapter drives (one adapter per class per driver).
    fn class(&self) -> EnvironmentClass;

    /// `provision(req)` — bring the remote environment up; the returned
    /// `ProviderHandle` is recorded on the `EnvHandle`.
    fn provision(&mut self, req: &ProvisionRequest) -> Result<ProviderHandle, EnvError>;

    /// `suspend(handle)` — checkpoint/park the remote environment
    /// (`memory`/`fs_only` posture per the class declaration).
    fn suspend(&mut self, handle: &ProviderHandle) -> Result<(), EnvError>;

    /// `resume(handle)` — bring the suspended environment back.
    fn resume(&mut self, handle: &ProviderHandle) -> Result<(), EnvError>;

    /// `teardown(handle)` — destroy the remote environment.
    /// `remote_persistent` survives `detach` but not `teardown` — the
    /// adapter owns the distinction.
    fn teardown(&mut self, handle: &ProviderHandle) -> Result<(), EnvError>;

    /// `meters(handle)` — the provider's current meter sample (reserved/
    /// active/suspended clocks the provider reports beside the kernel's
    /// own).
    fn meters(&mut self, handle: &ProviderHandle) -> Result<Vec<ProviderMeter>, EnvError> {
        let _ = handle;
        Ok(Vec::new())
    }
}

impl std::fmt::Debug for dyn ProviderAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProviderAdapter({})", self.class().as_str())
    }
}
