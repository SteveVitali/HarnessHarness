//! The scoped lease scopes (§5a.3; ADR-0131 §1–2 — `R-2.2.3` C1·S2): beside the
//! persisted `writer` lease, a run carries **`effect(id)`**, **`resource(key)`**,
//! **`environment(handle)`** and **`wakeup(subscription, occurrence)`** scopes —
//! one `lifecycle.lease.{acquired,renewed,released,fenced}` vocabulary, the
//! `scope` member naming which scope the row binds (ADR-0131 §1's "one lease
//! vocabulary").
//!
//! Scoped leases live **in the ledger** (the writer lease stays the only
//! lease-*file* — scoped scopes exist only under a live writer, so the WAL is
//! their authoritative store; CC3, no second store). They share the writer's
//! generation fence (ADR-0130 §3): a scoped record minted under writer
//! generation `g` is fenced the moment the writer generation moves past `g` —
//! no separate fence file, the fold computes it.
//!
//! **Liveness shortening (AC-R-2.2.3-5).** A takeover before `expires_at` is
//! admitted only when [`probe_holder`] reports the recorded holder *dead* on
//! this host — the process is gone (`kill -0` fails) or the pid was reused
//! (`start_id` mismatch). An off-host or opaque holder is `Unknown` and waits
//! for expiry — never a guess. A *suspended* run is exempt: suspension is a
//! declared pause, not a crash, so its live lease is never liveness-shortened
//! (§5a.3 suspension table).
//!
//! Holder spellings carrying probeable identity use
//! `host=<hostname>;pid=<pid>;start=<start-id>` `;`-separated members anywhere
//! in the holder string; a bare holder is `Opaque` (`probe` ⇒ `Unknown`).

use std::collections::BTreeMap;
use std::process::Command;

use hh_wire::json::Json;

use crate::errors::LedgerError;
use crate::event::Scope;
use crate::manifest::EventRef;
use crate::recovery::kernel_ev;
use crate::store::{Lease, Store};

/// The four scoped lease scopes (`writer` is the fifth — the persisted lease
/// file, unchanged). Spellings: `effect:<id>`, `resource:<key>`,
/// `environment:<handle>`, `wakeup:<subscription_id>:<occurrence_key>`
/// (ADR-0131 §1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeaseScope {
    /// The effect-lifecycle scope — an executor's claim on an effect.
    Effect(String),
    /// A `resource(key)` claim (e.g. `run_plan_id`, `device`, `workspace_root`
    /// — the §5a.3 row's examples).
    Resource(String),
    /// The environment-handle scope.
    Environment(String),
    /// The wakeup-claim scope — at most one `fire` per
    /// `(subscription_id, occurrence_key)` (W-1).
    Wakeup {
        /// The subscription.
        subscription_id: String,
        /// The occurrence.
        occurrence_key: String,
    },
}

impl LeaseScope {
    /// The `scope` member spelling on `lifecycle.lease.*` rows.
    pub fn spelling(&self) -> String {
        match self {
            LeaseScope::Effect(id) => format!("effect:{id}"),
            LeaseScope::Resource(k) => format!("resource:{k}"),
            LeaseScope::Environment(h) => format!("environment:{h}"),
            LeaseScope::Wakeup {
                subscription_id,
                occurrence_key,
            } => format!("wakeup:{subscription_id}:{occurrence_key}"),
        }
    }

    /// Parse a `scope` spelling (`None` on `writer` and unknown shapes).
    pub fn parse(s: &str) -> Option<LeaseScope> {
        let (kind, rest) = s.split_once(':')?;
        Some(match kind {
            "effect" => LeaseScope::Effect(rest.to_string()),
            "resource" => LeaseScope::Resource(rest.to_string()),
            "environment" => LeaseScope::Environment(rest.to_string()),
            "wakeup" => {
                let (sub, key) = rest.split_once(':')?;
                LeaseScope::Wakeup {
                    subscription_id: sub.to_string(),
                    occurrence_key: key.to_string(),
                }
            }
            _ => return None,
        })
    }
}

/// The liveness probe result (`probe_holder(lease) → …`; ADR-0131 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderProbe {
    /// The recorded holder process is live on this host.
    Alive,
    /// The holder process is gone, or the pid was reused (`start` mismatch).
    Dead,
    /// Unprobeable — an opaque or off-host holder: the takeover waits for
    /// `expires_at` (never a guess).
    Unknown,
}

impl HolderProbe {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HolderProbe::Alive => "alive",
            HolderProbe::Dead => "dead",
            HolderProbe::Unknown => "unknown",
        }
    }
}

/// A scoped lease record — the fold's view of the `lifecycle.lease.*` rows for
/// one non-writer scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedLease {
    /// The scope.
    pub scope: LeaseScope,
    /// The lease id.
    pub lease_id: String,
    /// The holder identity string.
    pub holder: String,
    /// The writer generation this lease was minted under — fenced when the
    /// writer generation moves past it (the one generation fence).
    pub generation: u64,
    /// Acquisition time (wall-ms).
    pub acquired_at_ms: u64,
    /// Expiry (wall-ms).
    pub expires_at_ms: u64,
    /// The last `progress_hint` carried on `renewed`.
    pub progress: Option<String>,
    /// Whether a `released`/`fenced` row closed the record.
    pub closed: bool,
}

/// Parse `key=value` members out of a holder string's `;`-separated suffix
/// (`holder[<k>=<v>;…]` — any position; a bare holder yields nothing).
fn holder_member(holder: &str, key: &str) -> Option<String> {
    holder
        .split(';')
        .filter_map(|m| m.trim().split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_string())
}

/// This host's name (for the off-host check — an off-host holder is never
/// probed, only expired).
pub fn local_hostname() -> Option<String> {
    Command::new("hostname")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A live `pid`'s start identity — `ps -o lstart=` output, whitespace-squashed
/// (the same command at both ends makes the string comparable; a pid-reuse
/// yields a different start string).
fn pid_start_id(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let squashed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.is_empty() {
        None
    } else {
        Some(squashed)
    }
}

/// `kill -0` liveness (unix — the dev/CI surface for this build).
fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `probe_holder(holder)` — parse the holder's identity members and probe:
/// `host` present and ≠ local ⇒ `Unknown`; no `pid` ⇒ `Unknown`; `kill -0`
/// fails ⇒ `Dead`; `start` recorded and mismatches the live pid's ⇒ `Dead`
/// (reuse); otherwise `Alive`.
pub fn probe_holder(holder: &str) -> HolderProbe {
    if let Some(host) = holder_member(holder, "host") {
        match local_hostname() {
            Some(local) if local == host => {}
            _ => return HolderProbe::Unknown, // off-host — never probed
        }
    }
    let Some(pid_s) = holder_member(holder, "pid") else {
        return HolderProbe::Unknown; // opaque holder
    };
    let Ok(pid) = pid_s.parse::<u32>() else {
        return HolderProbe::Unknown;
    };
    if !pid_alive(pid) {
        return HolderProbe::Dead;
    }
    if let Some(start) = holder_member(holder, "start") {
        match pid_start_id(pid) {
            Some(live) if live == start => {}
            _ => return HolderProbe::Dead, // reused pid — the start id moved
        }
    }
    HolderProbe::Alive
}

/// Compose a probeable holder spelling — `label;host=<local>;pid=<self>`
/// (callers append `;start=<…>` via [`holder_start_id`] when they want the
/// reuse check).
pub fn holder_spelling(label: &str, pid: u32) -> String {
    let host = local_hostname().unwrap_or_else(|| "unknown".to_string());
    match pid_start_id(pid) {
        Some(start) => format!("{label};host={host};pid={pid};start={start}"),
        None => format!("{label};host={host};pid={pid}"),
    }
}

impl Store {
    /// `scoped_lease(run, scope)` — the fold's current record for the scope,
    /// when one exists and is still live under the *current* writer generation
    /// (a record under a superseded generation is fenced — invisible here).
    pub fn scoped_lease(
        &self,
        run_id: &str,
        scope: &LeaseScope,
    ) -> Result<Option<ScopedLease>, LedgerError> {
        let st = self.run(run_id)?;
        Ok(st
            .scoped_leases
            .get(&scope.spelling())
            .filter(|l| !l.closed)
            .cloned())
    }

    /// `acquire(scope, holder, ttl)` — mint a scoped lease under the caller's
    /// writer lease (the `lifecycle.lease.acquired{scope}` row rides the normal
    /// append path — the writer fence is the lease check). An active scoped
    /// lease is `WouldBlock`; an expired/superseded one is taken over with the
    /// audited `fenced` row first.
    pub fn lease_acquire(
        &mut self,
        run_id: &str,
        writer: &Lease,
        scope: &LeaseScope,
        holder: &str,
        ttl_ms: u64,
    ) -> Result<ScopedLease, LedgerError> {
        self.tier_c1("lease_acquire")?;
        let now = self.now_ms();
        let cur_gen = writer.generation;
        if let Some(existing) = self.scoped_lease(run_id, scope)? {
            let fenced = existing.generation < cur_gen;
            if !fenced && now < existing.expires_at_ms {
                return Err(LedgerError::WouldBlock {
                    active_holder: existing.holder,
                });
            }
            // Expired (or generation-fenced) — audit the fence before the new
            // acquisition (the takeover rows are one atomic batch).
            let fence = kernel_ev(
                self,
                run_id,
                "lifecycle.lease.fenced",
                Scope::default(),
                Json::obj([
                    ("scope", Json::str(scope.spelling())),
                    ("lease_id", Json::str(&existing.lease_id)),
                    ("holder", Json::str(&existing.holder)),
                    ("generation", Json::Int(cur_gen as i64)),
                    ("stale_lease_id", Json::str(&existing.lease_id)),
                    ("stale_generation", Json::Int(existing.generation as i64)),
                    (
                        "reason",
                        Json::str(if fenced {
                            "writer_generation"
                        } else {
                            "expired"
                        }),
                    ),
                ]),
            )?;
            self.append(run_id, writer, vec![fence])?;
        }
        self.emit_scoped_acquire(run_id, writer, scope, holder, ttl_ms, now)
    }

    /// `takeover(scope, holder, ttl)` — like [`Store::lease_acquire`] but
    /// admits an *unexpired* live record when [`probe_holder`] reports the
    /// holder dead (liveness shortening; AC-R-2.2.3-5). A live or unprobeable
    /// holder is `WouldBlock` until `expires_at`.
    pub fn lease_takeover(
        &mut self,
        run_id: &str,
        writer: &Lease,
        scope: &LeaseScope,
        holder: &str,
        ttl_ms: u64,
    ) -> Result<ScopedLease, LedgerError> {
        self.tier_c1("lease_takeover")?;
        let now = self.now_ms();
        let cur_gen = writer.generation;
        // ADR-0131 §4 — a suspended run is exempt from liveness-based
        // takeover: its holders park by declaration, so a dead pid is not
        // evidence of abandonment (expiry takeover via `lease_acquire`
        // still applies — the exemption is the *shortening*, not the lease).
        let suspended = self.run(run_id)?.suspended;
        if let Some(existing) = self.scoped_lease(run_id, scope)? {
            let fenced = existing.generation < cur_gen;
            if !fenced && now < existing.expires_at_ms {
                match probe_holder(&existing.holder) {
                    HolderProbe::Dead if !suspended => {}
                    _ => {
                        return Err(LedgerError::WouldBlock {
                            active_holder: existing.holder,
                        })
                    }
                }
            }
            let fence = kernel_ev(
                self,
                run_id,
                "lifecycle.lease.fenced",
                Scope::default(),
                Json::obj([
                    ("scope", Json::str(scope.spelling())),
                    ("lease_id", Json::str(&existing.lease_id)),
                    ("holder", Json::str(&existing.holder)),
                    ("generation", Json::Int(cur_gen as i64)),
                    ("stale_lease_id", Json::str(&existing.lease_id)),
                    ("stale_generation", Json::Int(existing.generation as i64)),
                    (
                        "reason",
                        Json::str(if fenced {
                            "writer_generation"
                        } else if now < existing.expires_at_ms {
                            "liveness_probe"
                        } else {
                            "expired"
                        }),
                    ),
                ]),
            )?;
            self.append(run_id, writer, vec![fence])?;
        }
        self.emit_scoped_acquire(run_id, writer, scope, holder, ttl_ms, now)
    }

    /// The shared `lifecycle.lease.acquired{scope}` emit (acquire + takeover).
    fn emit_scoped_acquire(
        &mut self,
        run_id: &str,
        writer: &Lease,
        scope: &LeaseScope,
        holder: &str,
        ttl_ms: u64,
        now: u64,
    ) -> Result<ScopedLease, LedgerError> {
        let rec = ScopedLease {
            scope: scope.clone(),
            lease_id: self.alloc_id("lease"),
            holder: holder.to_string(),
            generation: writer.generation,
            acquired_at_ms: now,
            expires_at_ms: now + ttl_ms,
            progress: None,
            closed: false,
        };
        let ev = kernel_ev(
            self,
            run_id,
            "lifecycle.lease.acquired",
            Scope::default(),
            scoped_lease_payload(&rec, Json::Null),
        )?;
        self.append(run_id, writer, vec![ev])?;
        Ok(rec)
    }

    /// `renew(scoped, progress_hint?)` — extends the record's expiry; a
    /// released/fenced/stale token is `Fenced` (§5a.3 lease rules).
    pub fn lease_renew(
        &mut self,
        run_id: &str,
        writer: &Lease,
        scoped: &ScopedLease,
        progress: Option<&str>,
    ) -> Result<ScopedLease, LedgerError> {
        self.tier_c1("lease_renew")?;
        let now = self.now_ms();
        let cur = self
            .scoped_lease(run_id, &scoped.scope)?
            .ok_or_else(|| LedgerError::Fenced {
                lease_generation: scoped.generation,
                current_generation: writer.generation,
                detail: "scoped lease closed or unknown".into(),
            })?;
        if cur.lease_id != scoped.lease_id || cur.generation != scoped.generation {
            return Err(LedgerError::Fenced {
                lease_generation: scoped.generation,
                current_generation: cur.generation,
                detail: "superseded".into(),
            });
        }
        if now >= cur.expires_at_ms {
            return Err(LedgerError::Fenced {
                lease_generation: scoped.generation,
                current_generation: writer.generation,
                detail: "scoped lease expired; re-acquire".into(),
            });
        }
        let renewed = ScopedLease {
            expires_at_ms: now + (cur.expires_at_ms - cur.acquired_at_ms).max(1),
            progress: progress.map(str::to_string).or(cur.progress.clone()),
            ..cur
        };
        let mut extra = Json::Null;
        if let Some(p) = progress {
            extra = Json::obj([("progress", Json::str(p))]);
        }
        let ev = kernel_ev(
            self,
            run_id,
            "lifecycle.lease.renewed",
            Scope::default(),
            scoped_lease_payload(&renewed, extra),
        )?;
        self.append(run_id, writer, vec![ev])?;
        Ok(renewed)
    }

    /// `release(scoped, reason)` — closes the record (`lifecycle.lease.released`
    /// with the scope member).
    pub fn lease_release(
        &mut self,
        run_id: &str,
        writer: &Lease,
        scoped: &ScopedLease,
        reason: &str,
    ) -> Result<(), LedgerError> {
        self.tier_c1("lease_release")?;
        let cur = self
            .scoped_lease(run_id, &scoped.scope)?
            .ok_or_else(|| LedgerError::Fenced {
                lease_generation: scoped.generation,
                current_generation: writer.generation,
                detail: "scoped lease closed or unknown".into(),
            })?;
        if cur.lease_id != scoped.lease_id {
            return Err(LedgerError::Fenced {
                lease_generation: scoped.generation,
                current_generation: cur.generation,
                detail: "superseded".into(),
            });
        }
        let ev = kernel_ev(
            self,
            run_id,
            "lifecycle.lease.released",
            Scope::default(),
            scoped_lease_payload(&cur, Json::obj([("reason", Json::str(reason))])),
        )?;
        self.append(run_id, writer, vec![ev])?;
        Ok(())
    }
}

/// The `lifecycle.lease.*` payload for a scoped record (`scope` names the
/// scope — the writer rows' `scope: "writer"` default is unchanged).
fn scoped_lease_payload(rec: &ScopedLease, extra: Json) -> Json {
    let mut m = BTreeMap::from([
        ("scope".to_string(), Json::str(rec.scope.spelling())),
        ("lease_id".to_string(), Json::str(&rec.lease_id)),
        ("holder".to_string(), Json::str(&rec.holder)),
        ("generation".to_string(), Json::Int(rec.generation as i64)),
        (
            "acquired_at_ms".to_string(),
            Json::Int(rec.acquired_at_ms as i64),
        ),
        (
            "expires_at_ms".to_string(),
            Json::Int(rec.expires_at_ms as i64),
        ),
    ]);
    if let Json::Obj(extra) = extra {
        m.extend(extra);
    }
    Json::Obj(m)
}

/// The scoped-lease fold arm — `lifecycle.lease.{acquired,renewed,released,
/// fenced}` rows whose `scope` ≠ `writer` (§5a.3). Called from the rebuild and
/// commit paths (one fold — CC1).
pub(crate) fn fold_lease_row(
    scoped: &mut BTreeMap<String, ScopedLease>,
    env: &crate::event::EventEnvelope,
) {
    if !env.class.starts_with("lifecycle.lease.") {
        return;
    }
    let Some(sp) = env.payload.get("scope").and_then(Json::as_str) else {
        return;
    };
    if sp == "writer" {
        return; // the persisted lease file owns the writer scope
    }
    let Some(scope) = LeaseScope::parse(sp) else {
        return;
    };
    let p = &env.payload;
    match env.class.as_str() {
        "lifecycle.lease.acquired" => {
            scoped.insert(
                sp.to_string(),
                ScopedLease {
                    scope,
                    lease_id: str_member(p, "lease_id"),
                    holder: str_member(p, "holder"),
                    generation: int_member(p, "generation"),
                    acquired_at_ms: int_member(p, "acquired_at_ms"),
                    expires_at_ms: int_member(p, "expires_at_ms"),
                    progress: None,
                    closed: false,
                },
            );
        }
        "lifecycle.lease.renewed" => {
            if let Some(l) = scoped.get_mut(sp) {
                if l.lease_id == str_member(p, "lease_id") {
                    l.expires_at_ms = int_member(p, "expires_at_ms");
                    if let Some(prog) = p.get("progress").and_then(Json::as_str) {
                        l.progress = Some(prog.to_string());
                    }
                }
            }
        }
        "lifecycle.lease.released" | "lifecycle.lease.fenced" => {
            if let Some(l) = scoped.get_mut(sp) {
                // A `fenced`/`released` row names the record it closes.
                let names = p
                    .get("stale_lease_id")
                    .or_else(|| p.get("lease_id"))
                    .and_then(Json::as_str);
                if names.is_none() || names == Some(l.lease_id.as_str()) {
                    l.closed = true;
                }
            }
        }
        _ => {}
    }
}

fn str_member(p: &Json, k: &str) -> String {
    p.get(k)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn int_member(p: &Json, k: &str) -> u64 {
    p.get(k)
        .and_then(Json::as_int)
        .map(|n| n.max(0) as u64)
        .unwrap_or(0)
}

/// An `EventRef` helper for the `created_by` member shapes the wakeup rows
/// carry (`{run_id, event_id, seq}` — the subscribing event's coordinate).
pub(crate) fn event_ref_json(r: &EventRef) -> Json {
    Json::obj([
        ("run_id", Json::str(&r.run_id)),
        ("event_id", Json::str(&r.event_id)),
    ])
}
