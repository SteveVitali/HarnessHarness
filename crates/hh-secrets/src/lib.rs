//! `hharness-secrets` — the C0/Stage-1 **credential broker** slice (spec §5g.3,
//! R-2.8.3; ticket S1.13; ADR-0057/0058/0059).
//!
//! # What this crate is
//!
//! The kernel-side boundary every credential crosses: above the broker a secret
//! exists only as the value-free `SecretRef{name, description}`; below it the
//! broker holds the channel table, the binding table, the resolver SPI (the
//! vault boundary) and the mask set. The PDP/CDP split is the spec's own:
//! **the reference monitor decides** (`hharness-monitor`'s `security.permission.*`
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
//! # The Stage-2 additions (S2.4; ADR-0266)
//!
//! - **`mint`** — the `minted_scoped` verb: a kernel-keyed
//!   `mh_mint:v1:<binding>:<exp>:<mac>` token carrying no secret material,
//!   verified by [`CredentialBroker::verify_minted`] (audience + expiry +
//!   binding-liveness — LT-05). `used` is durable before the token is visible.
//! - **`mediate_sentinel` / `drain_claim`** — the mediator-side path: a
//!   `mh_secret:` spelling resolves to its binding (foreign ⇒ `out_of_scope`),
//!   `mediate` runs the refusal order, and `drain_claim` drains the staged
//!   value consume-once onto the wire.
//! - **`RequestDescriptor.env_handle`** — the anti-laundering check (a
//!   sentinel is bound to one env; ADR-0266 D3).
//! - **`DestinationBinding.revocation_path` + `RevokeOutcome.intents`** —
//!   destination-side revocation dispatch (LT-05's wrapped half).
//! - **`virtualize_for_fork`** — LT-09: a fork's snapshot gets fresh
//!   placeholders bound to fresh bindings on the fork's env handle.
//! - The **mask set** covers minted spellings too (a dead token on a durable
//!   surface is still a leak).
//!
//! # The R3.7 additions (ADR-0359/0360 — the HUMAN-H5 ruling)
//!
//! - **`dpop`** — `SenderConstraint::Dpop` verifies: `bind` checks a
//!   proof-of-possession over `dpop::bind_preimage` and pins
//!   `sender_key`; `mediate`/`mediate_sentinel`, `mint`/`verify_minted`
//!   and `deliver_wrapped` require a bound-key proof over their verb
//!   preimages — every failure mode is still
//!   `Refused{sender_constraint_unmet}`. The verifier is `crate::dpop`,
//!   the one construction (CC1) over the ADR-admitted `ring =0.17.14`
//!   — the workspace's single external runtime crate. Fixture-verified:
//!   real signatures generated at test time, never live-attested.
//! - **`aead`** — R3.8's `RingBlobCipher`: the workspace's one blob-pool
//!   AEAD (`ring::aead::AES_256_GCM` + `ring::rand` — ADR-0359's reserved
//!   scope consumed; ADR-0361). `hharness-ledger` installs it through the
//!   `BlobCipher` custody seam; key material resolves through the broker,
//!   never enters ledger state (`n/a{custody}` — OQ-170).
//! - **`deliver_wrapped` / `probe_wrapped`** — the `wrapped_long_lived`
//!   delivery half: `security.credential.wrapped{compensation_plan_id}`
//!   registers the compensating revocation (ADR-0032's `prepare` hook at
//!   the credential plane) durable-before-visible in the same append as
//!   `used`; `probe_wrapped` folds to `applied | not_applied |
//!   undeterminable` (a revoked wrap probes `not_applied` — LT-05).
//!
//! `leak_scan` is a **test battery** — a static/contract verifier, never a
//! live-path interceptor (ADR-0059 D2).

pub mod aead;
pub mod broker;
pub mod channel;
mod codec;
pub mod decisions;
pub mod defscan;
pub mod dpop;
pub mod encoded;
pub mod env;
pub mod errors;
pub mod events;
pub mod mask;
pub mod metrics;
pub mod patterns;
pub mod redact;
pub mod signer;
pub mod types;

pub use aead::{RingBlobCipher, BLOB_AEAD_ALG};
pub use broker::{
    coord_key, path_is_ambiguous, BindRequest, CompositeResolver, CredentialBinding,
    CredentialBroker, Delivery, DenyAllResolver, ForkVirtualization, LaunchEnvResolver,
    MediationOutcome, MintedToken, MintedVerdict, RequestDescriptor, RevokeIntent, RevokeOutcome,
    RevokeTarget, SecretSourceResolver, SourceError, StaticVault, WrapDeliveryRequest, WrapProbe,
    WrapRecord, WrappedDelivery, COMPONENT,
};
pub use channel::{
    AuthCarrier, CredentialKind, DestinationBinding, SecretChannel, SecretChannelSpec,
    SecretSource, SenderConstraint,
};
pub use decisions::{covering_secret_access, fold, CoveringDecision, DecidedRow, DecisionView};
pub use defscan::{check_diff, scan_document, seal_checked};
pub use dpop::{DpopAlg, DpopProof};
pub use env::{
    env_apply, env_snapshot, env_sweep, is_inheritable, EnvBinding, EnvSpec, ProjectedEnv,
    KERNEL_NONINHERITABLE,
};
pub use errors::{BrokerError, CodecError, Refused, RefusedCode};
pub use mask::{MaskEntry, MaskSet};
pub use metrics::{
    registered_metrics, SECRET_VALUE_IN_ENVIRONMENT, SECRET_VALUE_IN_LEDGER,
    SECRET_VALUE_IN_MODEL_CONTEXT, SECRET_VALUE_IN_REPORT,
};
pub use patterns::{SecretPattern, REGISTERED_PATTERNS};
pub use redact::{
    detect, leak_scan, redact, Canary, DetectorKind, DetectorSet, Hit, Leak, RedactionTombstone,
    ScanTarget,
};
pub use signer::{
    signer_key_id, signer_key_path, CustodyError, CustodySourceResolver, LedgerSignerCustody,
    LedgerSignerKey, LedgerSignerResolver, SIGNER_ACTIVE_FILE, SIGNER_CHANNEL_ID, SIGNER_FILE_MODE,
    SIGNER_FILE_PREFIX, SIGNER_VAULT_REF,
};
pub use types::{
    AccessClass, BindingState, KernelPurpose, Placeholder, ProvidedSecret, SecretFingerprint,
    SecretRef, SecretUnavailable,
};
