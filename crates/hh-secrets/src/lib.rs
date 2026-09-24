//! `hh-secrets` — the C0/Stage-1 **credential broker** slice (spec §5g.3,
//! R-2.8.3; ticket S1.13; ADR-0057/0058/0059).
//!
//! # What this crate is
//!
//! The kernel-side boundary every credential crosses: above the broker a secret
//! exists only as the value-free `SecretRef{name, description}`; below it the
//! broker holds the channel table, the binding table, the resolver SPI (the
//! vault boundary) and the mask set. The PDP/CDP split is the spec's own:
//! **the reference monitor decides** (`hh-monitor`'s `security.permission.*`
//! records — the `DecisionView` fold reads them), **the broker delivers** —
//! and never decides policy, never falls back to an environment value, and
//! fails closed when the vault or the audit writer is unavailable.
//!
//! # The SV invariants (§5g.3 §8) as they land here
//!
//! - **SV-1** — `mask_set` covers every live channel *and* every retained
//!   rotated revision; a source failure fails the set closed.
//! - **SV-2** — no record this crate emits can carry a value (typed fields are
//!   refs/revisions/codes/`provided`).
//! - **SV-3** — model-visible form is `SecretRef::render` = name + description.
//! - **SV-4** — `env_apply`/`env_snapshot`/`proc_env` project placeholders only;
//!   `env_sweep` is the verifier.
//! - **SV-5** — `used`/`denied`/`bound`/`revoked`/`rotated` are durable before
//!   the operation's result is visible (`AuditUnavailable` otherwise).
//! - **SV-6** — `expires_at` ⊆ run lifetime; `on_run_terminal` is the implicit
//!   revoke.
//! - **SV-7** — `grant` requires issuer authority ≥ `principal` and scope ⊆
//!   channel scope (attenuation, never widening).
//! - **SV-8** — `bind`/`mediate` refuse `DecisionMissing` without a covering
//!   `security.permission.decided{allow}` (AC-R-2.8.3-13).
//! - **SV-9** — broker/vault/ledger-writer unavailability ⇒ `Refused` — a
//!   terminal `SecretUnavailable` observation, never a hang or an env-value
//!   fallback.
//! - **SV-10** — placeholders are binding-scoped and unforgeable-minted
//!   (`mh_secret:v1:…:<idp/1-framed nonce>`); a copied placeholder resolves
//!   nothing (best-effort at Stage 1).
//!
//! # The Stage-1 cut
//!
//! Per ADR-0058 §e and the §5g.3 §9 stage map, the egress mediator's *wire*
//! half (`mediate`'s actual request rewriting), `mint`ed delivery,
//! snapshot/fork virtualisation and destination-path binding are Stage-2 rows
//! (DF-S1.12-1 → S2.4; the rows this ticket opens are in
//! `docs/tickets/DEFERRALS.md`). `leak_scan` is a **test battery** — a static/
//! contract verifier, never a live-path interceptor (ADR-0059 D2).

pub mod broker;
pub mod channel;
mod codec;
pub mod decisions;
pub mod defscan;
pub mod env;
pub mod errors;
pub mod events;
pub mod mask;
pub mod metrics;
pub mod patterns;
pub mod redact;
pub mod types;

pub use broker::{
    coord_key, path_is_ambiguous, BindRequest, CompositeResolver, CredentialBinding,
    CredentialBroker, Delivery, DenyAllResolver, LaunchEnvResolver, MediationOutcome,
    RequestDescriptor, RevokeOutcome, RevokeTarget, SecretSourceResolver, SourceError, StaticVault,
    COMPONENT,
};
pub use channel::{
    AuthCarrier, CredentialKind, DestinationBinding, SecretChannel, SecretChannelSpec,
    SecretSource, SenderConstraint,
};
pub use decisions::{covering_secret_access, fold, CoveringDecision, DecidedRow, DecisionView};
pub use defscan::{check_diff, scan_document, seal_checked};
pub use env::{
    env_apply, env_snapshot, env_sweep, is_inheritable, EnvBinding, EnvSpec, ProjectedEnv,
    KERNEL_NONINHERITABLE,
};
pub use errors::{BrokerError, CodecError, Refused, RefusedCode};
pub use mask::{MaskEntry, MaskSet};
pub use metrics::{registered_metrics, MetricCharge, MetricLevel, SecretMetric};
pub use patterns::{SecretPattern, REGISTERED_PATTERNS};
pub use redact::{
    detect, leak_scan, redact, Canary, DetectorKind, DetectorSet, Hit, Leak, RedactionTombstone,
    ScanTarget,
};
pub use types::{
    AccessClass, BindingState, KernelPurpose, Placeholder, ProvidedSecret, SecretFingerprint,
    SecretRef, SecretUnavailable,
};
