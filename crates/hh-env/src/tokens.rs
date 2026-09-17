//! `AttributionToken` — the kernel-minted capability that binds a capture item
//! to `(effect_id, attempt_no, env_handle_id)` (§5d.5 R-2.5.5 §2 token row;
//! ADR-0101 D2). The kernel's [`TokenMinter`] is the *only* minter (CF-213 —
//! the helper never mints); the helper/executor holds a [`TokenResolver`] — a
//! read-only view that resolves a presented token to its binding, never a
//! mint.
//!
//! - *Opaque*: `token = idp_id("attribution_token", seed ∥ counter ∥ binding)`
//!   — 256 bits under `idp/1`, unguessable without the kernel-held `seed`.
//! - *Single-use per attempt*: a token binds exactly one
//!   `(effect_id, attempt_no)`; it is `expire`d at the effect's terminal so a
//!   replayed token resolves `Unknown`.
//! - *Hash*: `hash = idp_id("attribution_token_hash", token)` — the recorded
//!   form (the token itself never enters a payload; only its hash does, so a
//!   leaked ledger row cannot be replayed as a capability).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use hh_identity::idp::{idp_digest, idp_id};

/// What a presented token resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// The token is live — its binding.
    Resolved {
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
        /// The environment the token was minted for.
        env_handle_id: String,
    },
    /// Unknown, expired or consumed — never a silent accept.
    Unknown,
}

/// The binding a token carries.
#[derive(Debug, Clone, PartialEq)]
struct TokenEntry {
    effect_id: String,
    attempt_no: u64,
    env_handle_id: String,
    /// `true` once the effect reached a terminal (expired — replays refuse).
    expired: bool,
}

/// The minted token record the dispatcher hands the helper/executor (the
/// *hash* is what lands in payloads; the token itself is the capability the
/// executor presents back).
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionToken {
    /// The opaque token (`idp/1` — `<algo>:<hex>`).
    pub token: String,
    /// The recorded hash (`idp_id("attribution_token_hash", token)`).
    pub hash: String,
    /// The effect.
    pub effect_id: String,
    /// The attempt.
    pub attempt_no: u64,
    /// The environment.
    pub env_handle_id: String,
}

impl AttributionToken {
    /// `hash(token)` — the recorded form (recomputable; the token is never
    /// stored).
    pub fn hash_of(token: &str) -> String {
        idp_id("attribution_token_hash", token.as_bytes())
    }
}

/// `TokenMinter` — the kernel's one minter. Holds the table mutably; hands out
/// `TokenResolver` clones (a shared `Rc<RefCell>` read view — the helper side
/// resolves but cannot mint).
pub struct TokenMinter {
    table: Rc<RefCell<BTreeMap<String, TokenEntry>>>,
    /// The kernel-held seed (derived from a kernel secret + run; never leaves
    /// the kernel).
    seed: [u8; 32],
    counter: u64,
}

impl TokenMinter {
    /// A minter seeded by `seed` (the kernel derives it; tests pass a fixed
    /// seed for determinism).
    pub fn new(seed: [u8; 32]) -> Self {
        TokenMinter {
            table: Rc::new(RefCell::new(BTreeMap::new())),
            seed,
            counter: 0,
        }
    }

    /// `mint(effect_id, attempt_no, env_handle_id)` — a fresh opaque token
    /// bound to the triple.
    pub fn mint(
        &mut self,
        effect_id: &str,
        attempt_no: u64,
        env_handle_id: &str,
    ) -> AttributionToken {
        self.counter += 1;
        let binding = format!("{effect_id}\u{1f}{attempt_no}\u{1f}{env_handle_id}");
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.seed);
        payload.push(0x1f);
        payload.extend_from_slice(&self.counter.to_le_bytes());
        payload.push(0x1f);
        payload.extend_from_slice(binding.as_bytes());
        let token = format!("sha256:{}", idp_digest("attribution_token", &payload));
        self.table.borrow_mut().insert(
            token.clone(),
            TokenEntry {
                effect_id: effect_id.to_string(),
                attempt_no,
                env_handle_id: env_handle_id.to_string(),
                expired: false,
            },
        );
        AttributionToken {
            hash: AttributionToken::hash_of(&token),
            token,
            effect_id: effect_id.to_string(),
            attempt_no,
            env_handle_id: env_handle_id.to_string(),
        }
    }

    /// `expire(effect_id, attempt_no)` — the effect reached a terminal; its
    /// tokens die (a replayed token resolves `Unknown`).
    pub fn expire(&mut self, effect_id: &str, attempt_no: u64) {
        for e in self.table.borrow_mut().values_mut() {
            if e.effect_id == effect_id && e.attempt_no == attempt_no {
                e.expired = true;
            }
        }
    }

    /// A `TokenResolver` — the read-only view the helper/executor holds.
    pub fn resolver(&self) -> TokenResolver {
        TokenResolver {
            table: Rc::clone(&self.table),
        }
    }
}

/// `TokenResolver` — the helper/executor's read view (resolve-only; no mint).
#[derive(Clone)]
pub struct TokenResolver {
    table: Rc<RefCell<BTreeMap<String, TokenEntry>>>,
}

impl TokenResolver {
    /// `resolve(token)` — the binding, or `Unknown` (expired/never minted).
    pub fn resolve(&self, token: &str) -> ResolveOutcome {
        match self.table.borrow().get(token) {
            Some(e) if !e.expired => ResolveOutcome::Resolved {
                effect_id: e.effect_id.clone(),
                attempt_no: e.attempt_no,
                env_handle_id: e.env_handle_id.clone(),
            },
            _ => ResolveOutcome::Unknown,
        }
    }

    /// The recorded hash for a presented token (for the `unattributed` marker
    /// — the resolver records the *hash* of an unresolvable signal so the
    /// audit names it without storing the capability).
    pub fn hash_of(token: &str) -> String {
        AttributionToken::hash_of(token)
    }
}
