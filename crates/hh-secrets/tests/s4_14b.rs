//! S4.14b acceptance — the LT-07 subagent arm of AC-R-2.8.3-7 (§5g.3 §8;
//! ADR-0057/0058 SV-7): a *delegated child* asking the broker for a
//! channel scope outside the parent's grant is `ScopeExceedsChannel` —
//! credentials ⊆ the parent's placeholders, never widened at the boundary
//! (C-5). The hosted arms of LT-07 live in `hh-hosting/tests/s4_14b.rs`
//! (`credential_supply` declared-only bindings).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_hir::records::GrantConstraints;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::ProvenanceRecord;

use hh_secrets::*;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-secrets-s414b-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn github_spec() -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::ApiKey,
        source: SecretSource::OperatorVault {
            vault_ref: "vault:github".into(),
        },
        destinations: vec![DestinationBinding {
            scheme: "https".into(),
            host_pattern: "api.github.com".into(),
            port: None,
            path_prefix: None,
            revocation_path: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
        }],
        allowed_env_names: Some(["GH_API_TOKEN".into()].into_iter().collect()),
        delivery_modes: [hh_monitor::assess::SecretTransport::ProxyInjected]
            .into_iter()
            .collect(),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: Default::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "GitHub API token".into(),
    }
}

/// LT-07 (AC-R-2.8.3-7): a subagent's credential request outside the
/// parent's grant is `ScopeExceedsChannel`. The check is the same one
/// `grant` always runs — `scope ⊆ channel.destinations` — but the LT-07
/// row pins it against a *child-scope* holder: delegation never widens a
/// credential's reach past what the parent's channel declared.
#[test]
fn lt07_subagent_scope_outside_parent_grant_is_scope_exceeds_channel() {
    let mut store = Store::open_test(dir("lt07"), 1_000).unwrap();
    let (run_id, _lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let mut vault = StaticVault::default();
    let source = SecretSource::OperatorVault {
        vault_ref: "vault:github".into(),
    };
    vault.put(&source, "ghp_lt07_0123456789abcdefghij0123456");
    let mut broker = CredentialBroker::new(Box::new(vault), "test-fp-key");
    broker
        .register_channel(
            "github",
            github_spec(),
            ProvenanceRecord::kernel("kernel:test", 1_000),
        )
        .unwrap();

    // The child's ask: `api.github.com` covered by the channel — but the
    // child asks for a destination the parent's grant never covered.
    let r = broker.grant(
        &run_id,
        "sub:child-run",
        "github",
        &["evil.example.com".into()].into_iter().collect(),
        GrantConstraints::default(),
        &hh_hir::records::Issuer {
            authority: AuthorityClass::Principal,
            reference: "principal:test".into(),
        },
    );
    match r {
        Err(BrokerError::ScopeExceedsChannel { channel_id, scope }) => {
            assert_eq!(channel_id, "github");
            assert_eq!(scope, "evil.example.com");
        }
        other => panic!("expected ScopeExceedsChannel, got {other:?}"),
    }
    // The in-scope half of the same channel still grants — attenuation
    // narrows, it does not forbid.
    assert!(broker
        .grant(
            &run_id,
            "sub:child-run",
            "github",
            &["api.github.com".into()].into_iter().collect(),
            GrantConstraints::default(),
            &hh_hir::records::Issuer {
                authority: AuthorityClass::Principal,
                reference: "principal:test".into(),
            },
        )
        .is_ok());
    // And a child-scoped issuer (delegate authority) never confers at all.
    match broker.grant(
        &run_id,
        "sub:child-run",
        "github",
        &["api.github.com".into()].into_iter().collect(),
        GrantConstraints::default(),
        &hh_hir::records::Issuer {
            authority: AuthorityClass::Delegate,
            reference: "sub:child-run".into(),
        },
    ) {
        Err(BrokerError::IssuerAuthorityInsufficient { .. }) => {}
        other => panic!("expected IssuerAuthorityInsufficient, got {other:?}"),
    }
}
