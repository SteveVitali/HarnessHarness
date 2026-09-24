//! S1.7 integration tests — the §5a.2 effect & transaction model (R-2.2.2) and
//! the `R-2.2.3⁰ᵃ` durability slice against the real `Store`.
//!
//! Named acceptance criteria:
//! - AC-R-2.2.2-3  — no dangling intent (cancellation/timeout/crash ⇒ terminal
//!   or explicit `unknown` + escalation path).
//! - AC-R-2.2.2-5  — classification monotonicity; `unknown` is conservative.
//! - AC-R-2.2.2-7  — the idempotency key is stable across restart and attempts,
//!   different for changed semantic arguments.
//! - AC-R-2.2.2-10 — the write-ahead barrier: `committed` ≺ dispatch; a
//!   `tool.started` naming an uncommitted non-`read_only` effect is refused.
//! - AC-R-2.2.3-2  — checkpoint/restore is deterministic and ledger-derived.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_ledger::effect::{idempotency_key, CommitOutcome, EffectPhase};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::{LedgerError, ViewKind};
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-effect-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let (s, run, lease, _) = open_at(tag, 1_000);
    (s, run, lease)
}

/// `open` with a caller-held [`ManualClock`] — lease expiry (crash ⇒ takeover)
/// tests advance it past the writer TTL.
fn open_at(tag: &str, ms: u64) -> (Store, String, Lease, hh_ledger::ids::ManualClock) {
    let clock = hh_ledger::ids::ManualClock::at(ms);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        Some(Box::new(hh_ledger::ids::SeqIds::new())),
        hh_ledger::store::DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease, clock)
}

fn risk(rev: &str, rs: &str, scope: &str) -> Json {
    Json::obj([
        ("reversibility", Json::str(rev)),
        ("repeat_safety", Json::str(rs)),
        ("scope", Json::str(scope)),
    ])
}

const READ_ONLY: fn() -> Json = || risk("read_only", "idempotent", "workspace_local");

fn irreversible() -> Json {
    risk("irreversible", "non_idempotent", "external")
}

fn reversible() -> Json {
    risk("reversible", "non_idempotent", "workspace_local")
}

/// An `action.effect.*` row — audit-grade ⇒ kernel producer + provenance.
fn eff(id: &str, class: &str, effect_id: &str, payload: Json, fencing: u64) -> Event {
    let mut payload = payload;
    if fencing > 0 {
        if let Json::Obj(m) = &mut payload {
            m.insert("fencing_token".to_string(), Json::Int(fencing as i64));
        }
    }
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: if class.starts_with("action.effect.") {
                Some(effect_id.to_string())
            } else {
                None
            },
            ..Scope::default()
        },
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn intended(id: &str, effect_id: &str, rc: Json, extra: Json) -> Event {
    let mut m = BTreeMap::from([
        ("effect_id".to_string(), Json::str(effect_id)),
        ("effective_risk_class".to_string(), rc),
    ]);
    if let Json::Obj(x) = extra {
        m.extend(x);
    }
    eff(id, "action.effect.intended", effect_id, Json::Obj(m), 0)
}

fn authorized(id: &str, effect_id: &str, rc: Json) -> Event {
    eff(
        id,
        "action.effect.authorized",
        effect_id,
        Json::obj([("effective_risk_class", rc)]),
        0,
    )
}

fn prepared(id: &str, effect_id: &str, key: &str) -> Event {
    prepared_x(id, effect_id, key, Json::Null)
}

fn prepared_x(id: &str, effect_id: &str, key: &str, extra: Json) -> Event {
    let mut m = BTreeMap::from([("idempotency_key".to_string(), Json::str(key))]);
    if let Json::Obj(x) = extra {
        m.extend(x);
    }
    eff(id, "action.effect.prepared", effect_id, Json::Obj(m), 0)
}

/// A `security.permission.decided` row — the complete-mediation pre-record
/// (ADR-0052 D6; §5g.1 I-H7) every `committed` must be preceded by.
fn decided(id: &str, effect_id: &str, attempt: i64, decision: &str) -> Event {
    let mut ev = eff(
        id,
        "security.permission.decided",
        effect_id,
        Json::obj([
            ("effect_id", Json::str(effect_id)),
            ("attempt_no", Json::Int(attempt)),
            ("decision", Json::str(decision)),
        ]),
        0,
    );
    // The class carries the effect in scope too (decision_effect reads either).
    ev.scope.effect_id = Some(effect_id.to_string());
    ev
}

fn allow(id: &str, effect_id: &str, attempt: i64) -> Event {
    decided(id, effect_id, attempt, "allow")
}

fn committed(id: &str, effect_id: &str, attempt: i64, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.committed",
        effect_id,
        Json::obj([("attempt_no", Json::Int(attempt))]),
        fencing,
    )
}

fn observed(id: &str, effect_id: &str, attempt: i64, outcome: &str, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.observed",
        effect_id,
        Json::obj([
            ("attempt_no", Json::Int(attempt)),
            ("outcome", Json::str(outcome)),
        ]),
        fencing,
    )
}

fn unknown(id: &str, effect_id: &str, cause: &str, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.unknown",
        effect_id,
        Json::obj([("cause", Json::str(cause))]),
        fencing,
    )
}

fn probed(id: &str, effect_id: &str, verdict: &str, fencing: u64) -> Event {
    eff(
        id,
        "action.effect.probed",
        effect_id,
        Json::obj([("verdict", Json::str(verdict))]),
        fencing,
    )
}

fn tool_started(id: &str, effect_id: &str) -> Event {
    // `action.tool.started` is audit-grade (§5g.6 §3) — the kernel writes it
    // (Rule P); the executor's dispatch only *causes* it.
    let mut e = eff(
        id,
        "action.tool.started",
        effect_id,
        Json::obj([("tool_call_id", Json::str("tc-1"))]),
        0,
    );
    e.scope.effect_id = Some(effect_id.to_string());
    e
}

/// Drive an effect to `prepared` (intended → authorized → prepared).
fn to_prepared(s: &mut Store, run: &str, lease: &Lease, eid: &str, rc: Json) {
    to_prepared_x(s, run, lease, eid, rc, Json::Null)
}

/// `to_prepared` with extra `prepared` fields (compensable ⇒
/// `compensation_plan_id`; reversible ⇒ `baseline_ref`).
fn to_prepared_x(
    s: &mut Store,
    run: &str,
    lease: &Lease,
    eid: &str,
    rc: Json,
    prepared_extra: Json,
) {
    s.append(
        run,
        lease,
        vec![
            intended(&format!("{eid}-i"), eid, rc.clone(), Json::Null),
            authorized(&format!("{eid}-a"), eid, rc),
            prepared_x(&format!("{eid}-p"), eid, "k1", prepared_extra),
        ],
    )
    .unwrap();
}

// ── AC-R-2.2.2-10 — the write-ahead barrier ─────────────────────────────

#[test]
fn ac_2_2_2_10_tool_started_requires_committed_for_non_read_only() {
    let (mut s, run, lease) = open("i1");
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    // Dispatch before the write-ahead record → NotCommitted.
    let e = s
        .append(&run, &lease, vec![tool_started("t1", "e1")])
        .unwrap_err();
    assert!(matches!(e, LedgerError::NotCommitted { .. }), "{e:?}");
    // The durable commit lands; dispatch is admitted.
    s.append(
        &run,
        &lease,
        vec![
            allow("a1", "e1", 1),
            committed("c1", "e1", 1, lease.generation),
        ],
    )
    .unwrap();
    s.append(&run, &lease, vec![tool_started("t2", "e1")])
        .unwrap();
    // A read_only effect is exempt — no commit is ever required.
    to_prepared(&mut s, &run, &lease, "e-ro", READ_ONLY());
    s.append(&run, &lease, vec![tool_started("t3", "e-ro")])
        .unwrap();
}

#[test]
fn ac_2_2_2_10_commit_token_binds_attempt_and_generation() {
    let (mut s, run, lease, clock) = open_at("tok", 1_000);
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    // Complete mediation: the gate decision precedes the write-ahead commit.
    s.append(&run, &lease, vec![allow("a1", "e1", 1)]).unwrap();
    // commit_effect is idempotent — a repeat returns the same token.
    let out = s.commit_effect(&run, &lease, "e1", None).unwrap();
    let CommitOutcome::Committed(tok) = out else {
        panic!("expected Committed, got {out:?}")
    };
    s.validate_commit_token(&run, &tok).unwrap();
    let out2 = s.commit_effect(&run, &lease, "e1", None).unwrap();
    assert!(matches!(out2, CommitOutcome::AlreadyCommitted(t) if t == tok));
    // A fabricated token does not validate.
    let mut bogus = tok.clone();
    bogus.commit_event_id = "evt-fake".into();
    let e = s.validate_commit_token(&run, &bogus).unwrap_err();
    assert!(matches!(e, LedgerError::NotCommitted { .. }), "{e:?}");
    // A takeover fences the token's generation — writer-a's lease has expired.
    clock.advance(61_000);
    let new_lease = s.acquire_writer("writer-b", &run, 60_000).unwrap();
    let e = s.validate_commit_token(&run, &tok).unwrap_err();
    assert!(matches!(e, LedgerError::Fenced { .. }), "{e:?}");
    let _ = new_lease;
}

// ── AC-R-2.2.2-5 — classification monotonicity ──────────────────────────

#[test]
fn ac_2_2_2_5_risk_never_lowers() {
    let (mut s, run, lease) = open("mono");
    s.append(
        &run,
        &lease,
        vec![intended("i1", "e1", irreversible(), Json::Null)],
    )
    .unwrap();
    // A decision claiming read_only lowers the recorded class → refused.
    let e = s
        .append(&run, &lease, vec![authorized("a1", "e1", READ_ONLY())])
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
    // Raising is legal — the effective class takes the maximum.
    s.append(&run, &lease, vec![authorized("a2", "e1", irreversible())])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.risk_class.reversibility.name(), "irreversible");
    // Declared > effective at `intended` is refused (self-report can't lower).
    let e = s
        .append(
            &run,
            &lease,
            vec![intended(
                "i2",
                "e2",
                READ_ONLY(),
                Json::obj([("declared_risk_class", irreversible())]),
            )],
        )
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
    // An undeclared intent is the conservative UNKNOWN.
    let e = s
        .append(
            &run,
            &lease,
            vec![intended("i3", "e3", Json::Null, Json::Null)],
        )
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
}

// ── AC-R-2.2.2-7 — stable idempotency key ───────────────────────────────

#[test]
fn ac_2_2_2_7_idempotency_key_stable_across_restart() {
    let d = dir("idem");
    let (mut s, run, lease) = {
        let mut s = Store::open_test(&d, 1_000).unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        (s, run, lease)
    };
    s.append(
        &run,
        &lease,
        vec![intended(
            "i1",
            "e1",
            irreversible(),
            Json::obj([
                ("capability_id", Json::str("cap:fs.write")),
                ("capability_version", Json::str("v3")),
                ("args_canonical_hash", Json::str("sha256:aaa")),
            ]),
        )],
    )
    .unwrap();
    let expected = idempotency_key(&run, "e1", "sha256:aaa", "v3");
    s.append(
        &run,
        &lease,
        vec![
            authorized("a1", "e1", irreversible()),
            prepared("p1", "e1", &expected),
        ],
    )
    .unwrap();
    // A wrong key is refused at `prepared` — the key is derived, never minted.
    to_prepared_x(
        &mut s,
        &run,
        &lease,
        "e2",
        reversible(),
        Json::obj([("baseline_ref", Json::str("snap-1"))]),
    );
    let e = s
        .append(
            &run,
            &lease,
            vec![intended(
                "i3",
                "e3",
                irreversible(),
                Json::obj([
                    ("capability_version", Json::str("v3")),
                    ("args_canonical_hash", Json::str("sha256:aaa")),
                ]),
            )],
        )
        .and_then(|_| {
            s.append(
                &run,
                &lease,
                vec![
                    authorized("a3", "e3", irreversible()),
                    prepared("p3", "e3", "idp:not-the-key"),
                ],
            )
        })
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
    // Restart: rebuild derives the same fold — the key survives.
    drop(s);
    let s2 = Store::open_test(&d, 1_000).unwrap();
    let f = s2.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.idempotency_key.as_deref(), Some(expected.as_str()));
    assert_eq!(f.phase, EffectPhase::Prepared);
}

// ── AC-R-2.2.2-3 — no dangling intent ───────────────────────────────────

#[test]
fn ac_2_2_2_3_lifecycle_closure_and_no_redispatch() {
    let (mut s, run, lease) = open("nodangle");
    let gen = lease.generation;
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    // `observed{partial}` is non-terminal — the scope stays open.
    s.append(&run, &lease, vec![observed("o1", "e1", 1, "partial", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert!(!f.is_terminal());
    // Exactly one `observed` per attempt — a duplicate is refused.
    let e = s
        .append(&run, &lease, vec![observed("o2", "e1", 1, "applied", gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::AlreadyObserved { .. }), "{e:?}");
    // A crash-like `unknown` then probe cycle; `undeterminable` stays open.
    s.append(&run, &lease, vec![unknown("u1", "e1", "worker_lost", gen)])
        .unwrap();
    s.append(
        &run,
        &lease,
        vec![probed("pb1", "e1", "undeterminable", gen)],
    )
    .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.phase, EffectPhase::Unknown);
    assert_eq!(f.probe_count, 1);
    // Irreversible + non_idempotent: no re-commit from unknown (invariant 4).
    let e = s
        .append(&run, &lease, vec![committed("c2", "e1", 2, gen)])
        .unwrap_err();
    assert!(
        matches!(e, LedgerError::BadEffectTransition { .. }),
        "{e:?}"
    );
    // The probe decides not_applied — the effect settles `reverted`-eligible…
    s.append(&run, &lease, vec![probed("pb2", "e1", "not_applied", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert!(f.is_terminal());
}

#[test]
fn ac_2_2_2_3_retry_from_unknown_needs_idempotent_or_probed_not_applied() {
    let (mut s, run, lease) = open("retry4");
    let gen = lease.generation;
    // Idempotent + compensable: redispatch after `unknown` is legal.
    to_prepared_x(
        &mut s,
        &run,
        &lease,
        "e1",
        risk("compensable", "idempotent", "external"),
        Json::obj([("compensation_plan_id", Json::str("plan-1"))]),
    );
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![unknown("u1", "e1", "timeout", gen)])
        .unwrap();
    s.append(
        &run,
        &lease,
        vec![allow("a2", "e1", 2), committed("c2", "e1", 2, gen)],
    )
    .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.attempt_no, 2);
    // The same attempt can never be re-committed (no redispatch of attempt n).
    s.append(&run, &lease, vec![unknown("u2", "e1", "timeout", gen)])
        .unwrap();
    let e = s
        .append(&run, &lease, vec![committed("c3", "e1", 2, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
}

#[test]
fn not_applied_probe_leaves_retryable_effect_open_for_retry() {
    let (mut s, run, lease) = open("retry-na");
    let gen = lease.generation;
    // Compensable + non-idempotent: a probe returning `not_applied` is the
    // attempt's `observed` — but the *effect* stays open for
    // `committed{attempt_no + 1}` under the same idempotency key (invariant 4;
    // the compensable delivery row — ADR-0238 §1).
    to_prepared_x(
        &mut s,
        &run,
        &lease,
        "e1",
        risk("compensable", "non_idempotent", "external"),
        Json::obj([("compensation_plan_id", Json::str("plan-1"))]),
    );
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![unknown("u1", "e1", "timeout", gen)])
        .unwrap();
    s.append(&run, &lease, vec![probed("p1", "e1", "not_applied", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.phase, EffectPhase::Observed);
    assert!(
        !f.is_terminal(),
        "retryable not_applied is not effect-terminal"
    );
    // The effect scope is still open — the retry commits in-scope.
    s.append(
        &run,
        &lease,
        vec![allow("a2", "e1", 2), committed("c2", "e1", 2, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![observed("o2", "e1", 2, "applied", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert!(f.is_terminal());

    // `irreversible` forecloses: `not_applied` is terminal, no retry ever.
    to_prepared(&mut s, &run, &lease, "e2", irreversible());
    s.append(
        &run,
        &lease,
        vec![allow("a3", "e2", 1), committed("c3", "e2", 1, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![unknown("u2", "e2", "timeout", gen)])
        .unwrap();
    s.append(&run, &lease, vec![probed("p2", "e2", "not_applied", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e2").unwrap().unwrap();
    assert!(f.is_terminal(), "irreversible not_applied is terminal");
    let e = s
        .append(&run, &lease, vec![committed("c4", "e2", 2, gen)])
        .unwrap_err();
    // Terminal ⇒ the scope closed with the probe; the retry is refused either
    // as a closed scope or as a bad transition, never admitted.
    assert!(
        matches!(
            e,
            LedgerError::BadEffectTransition { .. } | LedgerError::ScopeNotOpen { .. }
        ),
        "{e:?}"
    );
    // Non-idempotent *without* a probe: a direct `observed{not_applied}` is
    // terminal too (invariant 4 — redispatch needs idempotent or a probe).
    to_prepared_x(
        &mut s,
        &run,
        &lease,
        "e3",
        risk("compensable", "non_idempotent", "external"),
        Json::obj([("compensation_plan_id", Json::str("plan-2"))]),
    );
    s.append(
        &run,
        &lease,
        vec![allow("a5", "e3", 1), committed("c5", "e3", 1, gen)],
    )
    .unwrap();
    s.append(
        &run,
        &lease,
        vec![observed("o3", "e3", 1, "not_applied", gen)],
    )
    .unwrap();
    let f = s.effect_fold(&run, "e3").unwrap().unwrap();
    assert!(f.is_terminal());
    let e = s
        .append(&run, &lease, vec![committed("c6", "e3", 2, gen)])
        .unwrap_err();
    assert!(
        matches!(
            e,
            LedgerError::BadEffectTransition { .. } | LedgerError::ScopeNotOpen { .. }
        ),
        "{e:?}"
    );
}

#[test]
fn deferred_holds_until_promotion_or_discard() {
    let (mut s, run, lease) = open("defer");
    let gen = lease.generation;
    to_prepared_x(
        &mut s,
        &run,
        &lease,
        "e1",
        reversible(),
        Json::obj([("baseline_ref", Json::str("snap-1"))]),
    );
    s.append(
        &run,
        &lease,
        vec![eff(
            "d1",
            "action.effect.deferred",
            "e1",
            Json::obj([
                ("reason", Json::str("speculative_branch")),
                ("branch_id", Json::str("br-1")),
            ]),
            0,
        )],
    )
    .unwrap();
    // From `deferred` only `committed` (post-promotion) or `refused` — never
    // `observed` directly.
    let e = s
        .append(&run, &lease, vec![observed("o1", "e1", 1, "applied", gen)])
        .unwrap_err();
    assert!(
        matches!(e, LedgerError::BadEffectTransition { .. }),
        "{e:?}"
    );
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![observed("o2", "e1", 1, "applied", gen)])
        .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert!(f.is_terminal());
}

#[test]
fn abandoned_requires_an_escalation_ref() {
    let (mut s, run, lease) = open("abandon");
    let gen = lease.generation;
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    s.append(&run, &lease, vec![unknown("u1", "e1", "worker_lost", gen)])
        .unwrap();
    // `abandoned` without an escalation → refused (always escalated).
    let e = s
        .append(
            &run,
            &lease,
            vec![eff(
                "ab1",
                "action.effect.abandoned",
                "e1",
                Json::obj([("reason", Json::str("unresolvable"))]),
                gen,
            )],
        )
        .unwrap_err();
    assert!(matches!(e, LedgerError::SchemaViolation { .. }), "{e:?}");
    // With a real `lifecycle.escalation.raised` ref it settles.
    s.append(
        &run,
        &lease,
        vec![eff(
            "esc1",
            "lifecycle.escalation.raised",
            "e1",
            Json::obj([("kind", Json::str("effect_abandoned"))]),
            0,
        )],
    )
    .unwrap();
    s.append(
        &run,
        &lease,
        vec![eff(
            "ab2",
            "action.effect.abandoned",
            "e1",
            Json::obj([
                ("reason", Json::str("unresolvable")),
                ("escalation_ref", Json::str("esc1")),
            ]),
            gen,
        )],
    )
    .unwrap();
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.phase, EffectPhase::Abandoned);
}

// ── AC-R-2.2.3-2 — checkpoint + restore are ledger-derived ──────────────

fn model_call_open(s: &mut Store, run: &str, lease: &Lease, turn: &str, mc: &str, n: usize) {
    let mut req = eff(
        &format!("mc-req-{n}"),
        "model.call.requested",
        "unused",
        Json::obj([("model", Json::str("m1"))]),
        0,
    );
    req.scope = Scope {
        turn_id: Some(turn.to_string()),
        model_call_id: Some(mc.to_string()),
        ..Scope::default()
    };
    // `model.call.requested` opens the model_call scope inside a turn — open the
    // turn first.
    s.append(
        run,
        lease,
        vec![
            {
                let mut t = eff(
                    &format!("turn-{n}"),
                    "lifecycle.turn.started",
                    "unused",
                    Json::obj([("turn_no", Json::Int(n as i64))]),
                    0,
                );
                t.scope = Scope {
                    turn_id: Some(turn.to_string()),
                    ..Scope::default()
                };
                t
            },
            req,
        ],
    )
    .unwrap();
}

#[test]
fn ac_2_2_3_2_restore_fences_marks_unknown_and_audits() {
    let d = dir("restore");
    let clock = hh_ledger::ids::ManualClock::at(1_000);
    let (mut s, run, lease) = {
        let mut s = Store::open_with(
            &d,
            Box::new(clock.clone()),
            Some(Box::new(hh_ledger::ids::SeqIds::new())),
            hh_ledger::store::DEFAULT_BLOB_MAX_BYTES,
        )
        .unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        (s, run, lease)
    };
    let gen = lease.generation;
    // A committed non-read_only effect + an open model call + a read_only
    // prepared effect + a pending permission — the crash tableau.
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(
        &run,
        &lease,
        vec![allow("a1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    to_prepared(&mut s, &run, &lease, "e-ro", READ_ONLY());
    model_call_open(&mut s, &run, &lease, "turn-1", "mc-1", 1);
    s.append(
        &run,
        &lease,
        vec![eff(
            "perm1",
            "security.permission.requested",
            "unused",
            Json::obj([("permission_id", Json::str("perm-1"))]),
            0,
        )],
    )
    .unwrap();
    // Checkpoint is a pure fold — rebuild and compare byte-for-byte.
    let cp1 = s.project(&run, ViewKind::Checkpoint, None).unwrap();
    let s2 = Store::open_test(&d, 1_000).unwrap();
    let cp2 = s2.project(&run, ViewKind::Checkpoint, None).unwrap();
    assert_eq!(
        cp1.view_hash, cp2.view_hash,
        "checkpoint must rebuild identically"
    );
    drop(s2);

    // Crash + takeover restore — writer-a's lease has expired.
    clock.advance(61_000);
    let rep = s.restore(&run, "writer-b", 60_000).unwrap();
    assert!(rep.generation > gen);
    assert_eq!(rep.unknowned, vec!["e1".to_string()]);
    assert_eq!(rep.reprepared(), &["e-ro".to_string()]);
    assert_eq!(rep.failed_model_calls, vec!["mc-1".to_string()]);
    assert_eq!(rep.pending_permissions, vec!["perm-1".to_string()]);
    // The stale writer is fenced — its next append fails and the fence is audited.
    let e = s
        .append(&run, &lease, vec![committed("cx", "e1", 2, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::Fenced { .. }), "{e:?}");
    // Recovery rows are durable facts.
    let events = s.events(&run).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "lifecycle.run.resumed" && e.event_id == rep.resumed_event_id));
    assert!(events.iter().any(|e| e.class == "model.call.failed"
        && e.payload.get("cause").and_then(Json::as_str) == Some("worker_lost")));
    // The committed effect is now `unknown` with a live probe timer.
    let f = s.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.phase, EffectPhase::Unknown);
    let due = s.retry_due(&run, s.now_ms() + 2_000).unwrap();
    assert!(due.iter().any(|t| t.scope_id == "e1" && t.kind == "probe"));
    assert!(due
        .iter()
        .any(|t| t.scope_id == "mc-1" && t.kind == "model_call"));
    // A second restore is idempotent — no duplicate unknown rows, timers kept.
    clock.advance(61_000);
    let rep2 = s.restore(&run, "writer-c", 60_000).unwrap();
    assert!(rep2.unknowned.is_empty());
    assert!(rep2.failed_model_calls.is_empty());
}

#[test]
fn ac_2_2_3_2_retry_due_is_a_pure_fold() {
    let (mut s, run, lease, clock) = open_at("retrydue", 1_000);
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(
        &run,
        &lease,
        vec![
            allow("a1", "e1", 1),
            committed("c1", "e1", 1, lease.generation),
        ],
    )
    .unwrap();
    // Restore takes over (writer-a expired); the report hands back the live
    // writer lease.
    clock.advance(61_000);
    let rep = s.restore(&run, "writer-b", 60_000).unwrap();
    // The probe timer is not due before its not_before.
    let now = s.now_ms();
    let early = s.retry_due(&run, now).unwrap();
    assert!(early.is_empty(), "not_before is in the future: {early:?}");
    // …and is due after it.
    let due = s.retry_due(&run, now + 2_000).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].scope_id, "e1");
    // Consuming it (`fired` — a kernel-origin row under the live lease) removes
    // it from the due set.
    let sched = due[0].schedule_event_id.clone();
    let fired = Event {
        event_id: "rf1".into(),
        class: "control.retry.fired".into(),
        ts: s.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: s.head_event_id(&run).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel(
            "kernel:test",
            s.now_ms(),
        )),
        content_kind: None,
        payload: Json::obj([("schedule_event_id", Json::str(&sched))]),
    };
    s.append(&run, &rep.lease, vec![fired]).unwrap();
    assert!(s.retry_due(&run, now + 2_000).unwrap().is_empty());
}

// ── Complete mediation (§5g.1 I-H7; ADR-0052 D6; S1.11) ─────────────────
// `action.effect.committed` requires a preceding same-effect/same-attempt
// `security.permission.decided{decision = allow}`; exactly one final
// allow/deny per attempt cycle.

#[test]
fn mediation_committed_without_allow_is_undecided() {
    let (mut s, run, lease) = open("med-undecided");
    let gen = lease.generation;
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    // No `decided` row at all → Undecided.
    let e = s
        .append(&run, &lease, vec![committed("c1", "e1", 1, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::Undecided { .. }), "{e:?}");
    // A `deny` decision does not open the gate.
    s.append(&run, &lease, vec![decided("d1", "e1", 1, "deny")])
        .unwrap();
    let e = s
        .append(&run, &lease, vec![committed("c1", "e1", 1, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::Undecided { .. }), "{e:?}");
    // `ask` is non-final — the gate stays closed.
    // (a final deny already closed attempt 1; a fresh attempt needs its own
    // allow.)
}

#[test]
fn mediation_duplicate_final_decision_refused() {
    let (mut s, run, lease) = open("med-dup");
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(&run, &lease, vec![allow("d1", "e1", 1)]).unwrap();
    // A second final decision for the same (effect, attempt) — refused, even
    // for a different verdict.
    let e = s
        .append(&run, &lease, vec![decided("d2", "e1", 1, "deny")])
        .unwrap_err();
    assert!(matches!(e, LedgerError::DuplicateDecision { .. }), "{e:?}");
    let e = s
        .append(&run, &lease, vec![allow("d3", "e1", 1)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::DuplicateDecision { .. }), "{e:?}");
    // A different attempt cycle has its own decision slot.
    s.append(&run, &lease, vec![decided("d4", "e1", 2, "deny")])
        .unwrap();
}

#[test]
fn mediation_decision_binds_effect_and_attempt() {
    let (mut s, run, lease) = open("med-bind");
    let gen = lease.generation;
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    to_prepared(&mut s, &run, &lease, "e2", irreversible());
    // An `allow` for a *different* effect does not open e1's gate.
    s.append(&run, &lease, vec![allow("d1", "e2", 1)]).unwrap();
    let e = s
        .append(&run, &lease, vec![committed("c1", "e1", 1, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::Undecided { .. }), "{e:?}");
    // An `allow` for a *different attempt* does not open e1's attempt-1 gate.
    s.append(&run, &lease, vec![allow("d2", "e1", 2)]).unwrap();
    let e = s
        .append(&run, &lease, vec![committed("c1", "e1", 1, gen)])
        .unwrap_err();
    assert!(matches!(e, LedgerError::Undecided { .. }), "{e:?}");
    // The right (effect, attempt) pair opens it.
    s.append(
        &run,
        &lease,
        vec![allow("d3", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
}

#[test]
fn mediation_allow_decision_survives_rebuild() {
    let d = dir("med-rebuild");
    let (mut s, run, lease) = {
        let mut s = Store::open_test(&d, 1_000).unwrap();
        let (run, lease) = s
            .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
            .unwrap();
        (s, run, lease)
    };
    let gen = lease.generation;
    to_prepared(&mut s, &run, &lease, "e1", irreversible());
    s.append(
        &run,
        &lease,
        vec![allow("d1", "e1", 1), committed("c1", "e1", 1, gen)],
    )
    .unwrap();
    drop(s);
    // Rebuild: the decision fold lands — a `committed` for attempt 1 after
    // restart is refused `DuplicateDecision`-adjacent (the fold knows the
    // attempt closed) — and the effect is committed.
    let clock2 = hh_ledger::ids::ManualClock::at(1_000);
    let mut s2 = Store::open_with(
        &d,
        Box::new(clock2.clone()),
        Some(Box::new(hh_ledger::ids::SeqIds::new())),
        hh_ledger::store::DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let f = s2.effect_fold(&run, "e1").unwrap().unwrap();
    assert_eq!(f.phase, EffectPhase::Committed);
    // A second final decision for the committed attempt is refused post-
    // rebuild (the decision fold rebuilt from the durable log). Writer-a's
    // lease expired while the store was down — writer-b takes over.
    clock2.advance(61_000);
    let lease2 = s2.acquire_writer("writer-b", &run, 60_000).unwrap();
    let e = s2
        .append(&run, &lease2, vec![decided("d2", "e1", 1, "deny")])
        .unwrap_err();
    assert!(matches!(e, LedgerError::DuplicateDecision { .. }), "{e:?}");
}
