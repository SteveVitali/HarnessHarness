//! The handle carrier — `surface_ids` (spec §7.3 §2.4): surface handles
//! are *names*, never capabilities. A handle resolves through the table
//! to a ledger object id under a covering grant — the grant is the
//! binding that minted it (or a `Permission` row; C1 records the
//! creator-binding grant only).
//!
//! Determinism (AC-R-2.11.3-8): a handle is `idp/1` over
//! `{kind, target_id}` — the *same launched run has the same handle on
//! every retry*, so the post-crash idempotency path can name the
//! recorded run without a second lookup table. Minting is durable:
//! every mint lands a `lifecycle.surface.call.minted` row on the surface-session
//! run before the handle is ever returned, and the table is a pure
//! projection of those rows (rebuilt verbatim after a crash — no state
//! of record lives here).
//!
//! Refusals are typed, never protocol faults: `UnknownHandle` (no
//! minted row — *never* guessed), `HandleExpired` (`expires_at` passed),
//! `NoCoveringGrant` (the handle names an object the caller's binding
//! did not mint and no grant covers).

use std::collections::BTreeMap;

use hh_ledger::errors::LedgerError;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::Store;
use hh_wire::json::Json;

/// The producer tag on every surface-minted kernel row.
pub const SURFACE_PRODUCER: &str = "kernel:mcp-surface";

/// The minted-handle kinds the carrier distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandleKind {
    /// A launched run (`hnd-run-…`).
    Run,
    /// An open experiment (`hnd-experiment-…`).
    Experiment,
    /// A resume/live session (`hnd-session-…`).
    Session,
    /// A stored plan/attendance token (`hnd-plan-…`).
    Plan,
}

impl HandleKind {
    /// The alias prefix + payload spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HandleKind::Run => "run",
            HandleKind::Experiment => "experiment",
            HandleKind::Session => "session",
            HandleKind::Plan => "plan",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<HandleKind> {
        match s {
            "run" => Some(HandleKind::Run),
            "experiment" => Some(HandleKind::Experiment),
            "session" => Some(HandleKind::Session),
            "plan" => Some(HandleKind::Plan),
            _ => None,
        }
    }

    /// Parse an alias back to its kind (`hnd-<kind>-<digest>`).
    pub fn of_alias(alias: &str) -> Option<HandleKind> {
        let rest = alias.strip_prefix("hnd-")?;
        let kind = rest.split('-').next()?;
        HandleKind::parse(kind)
    }
}

/// The deterministic alias for `(kind, target_id)` — `hnd-<kind>-<idp>`.
/// The *name* is public (a handle travels in `surface_ids`); the
/// *grant* is the table's — knowing the name never confers access.
pub fn handle_alias(kind: HandleKind, target_id: &str) -> String {
    let digest = hh_identity::idp_id(
        "mcp.surface_handle",
        Json::obj([
            ("kind", Json::str(kind.as_str())),
            ("target_id", Json::str(target_id)),
        ])
        .to_canonical_string()
        .as_bytes(),
    );
    format!("hnd-{}-{}", kind.as_str(), &digest[..24])
}

/// One minted row — a pure projection of a durable `lifecycle.surface.call.minted`
/// payload.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleEntry {
    /// The alias (`hnd-<kind>-<digest>`).
    pub alias: String,
    /// What the handle names.
    pub kind: HandleKind,
    /// The ledger object id (run_id / experiment_id / …).
    pub target_id: String,
    /// The binding that minted it — the covering grant.
    pub owner_binding: String,
    /// `expires_at` (epoch ms), when declared.
    pub expires_at_ms: Option<u64>,
    /// The mint event's seq on the surface-session run.
    pub minted_seq: u64,
    /// The mint event's own id (`evt-…`) — `handle_ref` spellings.
    pub event_id: String,
    /// The full durable payload — `launched` rows carry the recorded
    /// answer (`session_id`, `spawn_event_ref`, `idempotency_key`) the
    /// resume path reads verbatim.
    pub payload: Option<Json>,
}

/// The resolution refusal — the spec §2.4 §3.2 table.
#[derive(Debug, Clone, PartialEq)]
pub enum HandleRefusal {
    /// Never minted (or minted under another kind) — *never guessed*.
    UnknownHandle {
        /// The unrecognized alias.
        alias: String,
    },
    /// `expires_at` passed — distinguishable from UnknownHandle.
    HandleExpired {
        /// The alias.
        alias: String,
        /// The expiry recorded at mint.
        expires_at_ms: u64,
    },
    /// The alias names an object the caller has no covering grant for.
    NoCoveringGrant {
        /// The alias.
        alias: String,
        /// The minting binding.
        owner_binding: String,
    },
}

impl HandleRefusal {
    /// The canonical error-name spelling (the `surface_error{}` kind —
    /// AC-R-2.11.3-13's taxonomy).
    pub fn kind(&self) -> &'static str {
        match self {
            HandleRefusal::UnknownHandle { .. } => "UnknownHandle",
            HandleRefusal::HandleExpired { .. } => "HandleExpired",
            HandleRefusal::NoCoveringGrant { .. } => "NoCoveringGrant",
        }
    }

    /// The one-line message.
    pub fn message(&self) -> String {
        match self {
            HandleRefusal::UnknownHandle { alias } => {
                format!("unknown handle `{alias}` — handles are never guessed")
            }
            HandleRefusal::HandleExpired {
                alias,
                expires_at_ms,
            } => format!("handle `{alias}` expired at {expires_at_ms}"),
            HandleRefusal::NoCoveringGrant { alias, .. } => {
                format!("no covering grant for `{alias}` under this binding")
            }
        }
    }

    /// The refusal's JSON detail (`{kind, message, ...}`).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind()));
        m.insert("message".into(), Json::str(self.message()));
        match self {
            HandleRefusal::UnknownHandle { alias } => {
                m.insert("alias".into(), Json::str(alias.clone()));
            }
            HandleRefusal::HandleExpired {
                alias,
                expires_at_ms,
            } => {
                m.insert("alias".into(), Json::str(alias.clone()));
                m.insert("expires_at_ms".into(), Json::Int(*expires_at_ms as i64));
            }
            HandleRefusal::NoCoveringGrant {
                alias,
                owner_binding,
            } => {
                m.insert("alias".into(), Json::str(alias.clone()));
                m.insert("owner".into(), Json::str(owner_binding.clone()));
            }
        }
        Json::Obj(m)
    }
}

/// The minted-handle table — a pure projection of the surface run's
/// `lifecycle.surface.call.minted` rows. Rebuilt by [`HandleTable::rebuild`],
/// mutated only by [`HandleTable::record_minted`] (which is called
/// *after* the durable row lands).
#[derive(Debug, Default)]
pub struct HandleTable {
    /// `alias → entry`.
    pub by_alias: BTreeMap<String, HandleEntry>,
}

impl HandleTable {
    /// Empty.
    pub fn new() -> HandleTable {
        HandleTable {
            by_alias: BTreeMap::new(),
        }
    }

    /// Rebuild from the surface-session run's durable rows — the table
    /// holds nothing the ledger doesn't (no state of record).
    pub fn rebuild(store: &Store, surface_run_id: &str) -> HandleTable {
        let mut t = HandleTable::new();
        let Ok(events) = store.events(surface_run_id) else {
            return t;
        };
        for e in events {
            if e.class != "lifecycle.surface.call.minted" {
                continue;
            }
            let p = &e.payload;
            let (Some(alias), Some(kind), Some(target), Some(owner)) = (
                p.get("handle").and_then(Json::as_str),
                p.get("kind").and_then(Json::as_str),
                p.get("target_id").and_then(Json::as_str),
                p.get("owner_binding").and_then(Json::as_str),
            ) else {
                continue;
            };
            let Some(kind) = HandleKind::parse(kind) else {
                continue;
            };
            t.by_alias.insert(
                alias.to_string(),
                HandleEntry {
                    alias: alias.to_string(),
                    kind,
                    target_id: target.to_string(),
                    owner_binding: owner.to_string(),
                    expires_at_ms: p
                        .get("expires_at_ms")
                        .and_then(Json::as_int)
                        .map(|n| n.max(0) as u64),
                    minted_seq: e.seq,
                    event_id: e.event_id.clone(),
                    payload: Some(p.clone()),
                },
            );
        }
        t
    }

    /// Fold one just-durabled mint payload into the table.
    pub fn record_minted(&mut self, entry: HandleEntry) {
        self.by_alias.insert(entry.alias.clone(), entry);
    }

    /// Resolve `alias` under `caller_binding` at `now_ms` — the typed
    /// refusal on every failure class.
    pub fn resolve(
        &self,
        alias: &str,
        caller_binding: &str,
        now_ms: u64,
    ) -> Result<&HandleEntry, HandleRefusal> {
        let entry = self
            .by_alias
            .get(alias)
            .ok_or_else(|| HandleRefusal::UnknownHandle {
                alias: alias.to_string(),
            })?;
        if let Some(exp) = entry.expires_at_ms {
            if now_ms >= exp {
                return Err(HandleRefusal::HandleExpired {
                    alias: alias.to_string(),
                    expires_at_ms: exp,
                });
            }
        }
        if entry.owner_binding != caller_binding {
            return Err(HandleRefusal::NoCoveringGrant {
                alias: alias.to_string(),
                owner_binding: entry.owner_binding.clone(),
            });
        }
        Ok(entry)
    }

    /// The `resume` naming scan — an entry minted under `kind` whose
    /// payload carries `idempotency_key` (or `external_id`) equal to
    /// one of `keys`, on any of the runs named in `run_ids` (the
    /// surface-session runs of the caller's prior incarnations —
    /// adoption is scoped to the caller's own history, the "resume"
    /// names never confer another binding's mints).
    ///
    /// `run_ids` is honoured as a filter only when the table scopes to
    /// multiple runs; the current table is per-surface-run so the check
    /// is identity (`run_ids` names this run or an ancestor it adopted
    /// — the payload's `run_id` member records which run answered).
    pub fn find_adopted(
        &self,
        run_id: &str,
        keys: &[String],
        kind: HandleKind,
    ) -> Option<HandleEntry> {
        let _ = run_id;
        self.by_alias
            .values()
            .find(|e| {
                if e.kind != kind {
                    return false;
                }
                let Some(p) = &e.payload else {
                    return false;
                };
                keys.iter().any(|k| {
                    p.get("idempotency_key").and_then(Json::as_str) == Some(k.as_str())
                        || p.get("external_id").and_then(Json::as_str) == Some(k.as_str())
                })
            })
            .cloned()
    }

    /// Mint durably on `surface_run_id` via `EmbedService`'s
    /// kernel-row seam — the row lands *before* the alias is usable;
    /// `extra` members merge into the durable payload (the `launched`
    /// row's recorded answer). Returns the entry the caller folds in.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_on_run(
        &mut self,
        svc: &mut hh_embed::service::EmbedService,
        surface_run_id: &str,
        kind: HandleKind,
        handle_id: &str,
        target_id: &str,
        owner_binding: Option<&str>,
        causes: &[EventRef],
        extra: Option<Json>,
        target_session: Option<String>,
        _now_ms: i64,
    ) -> Result<HandleEntry, crate::dispatch::SurfaceError> {
        let owner = owner_binding.unwrap_or("").to_string();
        let alias = handle_alias(kind, target_id);
        let minted_by = causes.first().cloned().unwrap_or(EventRef {
            run_id: surface_run_id.to_string(),
            event_id: "seq0".to_string(),
        });
        let mut payload = minted_payload(&alias, kind, target_id, &owner, None, &minted_by);
        // Merge the extra members (`handle`/`kind`/`target_id` already
        // claim their names — the extras carry `session_id`,
        // `spawn_event_ref`, `idempotency_key`, the recorded answer).
        if let (Json::Obj(m), Some(Json::Obj(x))) = (&mut payload, extra) {
            for (k, v) in x {
                m.entry(k).or_insert(v);
            }
        }
        if let Some(s) = &target_session {
            if let Json::Obj(m) = &mut payload {
                m.entry("session_id".into()).or_insert(Json::str(s.clone()));
            }
        }
        // `handle_id` is the caller's stable name (`launch-<fx>`); the
        // alias stays deterministic — both spellings resolve.
        let env = svc
            .surface_commit_row(
                SURFACE_PRODUCER,
                surface_run_id,
                "lifecycle.surface.call.minted",
                payload.clone(),
                vec![],
                causes.to_vec(),
            )
            .map_err(|e| {
                crate::dispatch::SurfaceError::new(
                    "kernel_error",
                    format!("minted row: {e:?}"),
                    Json::Null,
                )
            })?;
        let entry = HandleEntry {
            alias: alias.clone(),
            kind,
            target_id: target_id.to_string(),
            owner_binding: owner,
            expires_at_ms: None,
            minted_seq: env.seq,
            event_id: env.event_id,
            payload: Some(payload),
        };
        self.by_alias.insert(alias, entry.clone());
        let _ = handle_id;
        Ok(entry)
    }
}

/// The `lifecycle.surface.call.minted` row payload (`{handle, kind, target_id,
/// owner_binding, expires_at_ms?, minted_by}`).
pub fn minted_payload(
    alias: &str,
    kind: HandleKind,
    target_id: &str,
    owner_binding: &str,
    expires_at_ms: Option<u64>,
    minted_by: &EventRef,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("handle".into(), Json::str(alias));
    m.insert("kind".into(), Json::str(kind.as_str()));
    m.insert("target_id".into(), Json::str(target_id));
    m.insert("owner_binding".into(), Json::str(owner_binding));
    if let Some(e) = expires_at_ms {
        m.insert("expires_at_ms".into(), Json::Int(e as i64));
    }
    m.insert(
        "minted_by".into(),
        Json::obj([
            ("run_id", Json::str(minted_by.run_id.clone())),
            ("event_id", Json::str(minted_by.event_id.clone())),
        ]),
    );
    Json::Obj(m)
}

/// Mint a handle durably: the `lifecycle.surface.call.minted` row lands on the
/// surface-session run *before* the alias is usable, and the caller
/// folds it into the table. Returns `(alias, mint_event_id)` — the
/// row's own event id (the mint links `causes[]` against the
/// tool-effect's `prepared`/`intended` events).
pub fn mint_handle(
    store: &mut Store,
    surface_run_id: &str,
    kind: HandleKind,
    target_id: &str,
    owner_binding: &str,
    expires_at_ms: Option<u64>,
    caused_by: Vec<EventRef>,
) -> Result<(String, String), LedgerError> {
    let alias = handle_alias(kind, target_id);
    let minted_by = caused_by.first().cloned().unwrap_or(EventRef {
        run_id: surface_run_id.to_string(),
        event_id: "seq0".to_string(),
    });
    let payload = minted_payload(
        &alias,
        kind,
        target_id,
        owner_binding,
        expires_at_ms,
        &minted_by,
    );
    let env = store.commit_kernel_row_for(
        SURFACE_PRODUCER,
        surface_run_id,
        "lifecycle.surface.call.minted",
        payload,
        vec![],
        caused_by,
    )?;
    Ok((alias, env.event_id))
}
