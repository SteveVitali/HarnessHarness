//! The op admission table (§7.2 §5; ADR-0302 D2) — the closed set of
//! `hh-embed/1` verbs the browser may invoke through `POST /api`. Every
//! unlisted op is `Refused{op_not_admitted}` at the gate — the browser
//! can never reach a session lifecycle verb (session management is the
//! surface's own, P12 — the surface opens the session; the browser
//! names the run).
//!
//! C2 (S5.7) admits the editing and time-travel write groups (§7.2
//! §5.2/V3/V4/V5/V6/V11/V12): the branch ops, the Lab write set
//! (`apply`/`publish`/`register`/`define`, `analyze`, experiment
//! lifecycle, `lab.serve`), and the console ops (`submit`/`cancel`/
//! `steer`/`stream_events`/`subscribe`). Every write carries the
//! surface's injected session + idempotency + provenance — never a
//! browser-supplied member (P12; `session.rs` owns injection).

/// Whether an admitted op is a mutation (the P4 legs — fetch metadata,
/// script header, JSON content-type — apply to these + to every
/// non-`GET` route).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// A canonical read — `read`/`project`/`query_rows`/`render` and the
    /// Lab view renders (§7.2 §5.1).
    Read,
    /// A declared write — `respond_permission`, `amend`,
    /// `kernel.reproduce`, the branch/Lab write set (§7.2 §5.2; every
    /// one lands a ledgered row with responder provenance + a request
    /// id — P12).
    Write,
}

/// The session-scope of an op: `true` when the canonical op takes a
/// `session_id` — the surface injects it (browser params never carry
/// one; the surface's attach/writer sessions are the only handles).
pub fn session_scoped(op: &str) -> bool {
    matches!(
        op,
        "read"
            | "head"
            | "project"
            | "account"
            | "describe"
            | "list_leases"
            | "lineage"
            | "get_artifact"
            | "audit_view"
            | "verify"
            | "prove_inclusion"
            | "prove_consistency"
            | "env.snapshot"
            | "respond_permission"
            | "amend"
            // C2 — the session-scoped writes + live streams (§7.2
            // V3/V12: `stream_events`/`subscribe` open under the
            // surface's attach session; the writes resume the writer
            // session).
            | "stream_events"
            | "subscribe"
            | "submit"
            | "cancel"
            | "steer"
            | "respond_elicitation"
            | "fork"
            | "navigate"
            | "rollback"
            | "replay"
            | "counterfactual"
            | "branch.open"
            | "branch.discard"
            | "branch.promote"
    )
}

/// Ops whose params carry a `registrar` provenance record — the
/// surface mints it (human principal, `PersistenceScope::User`,
/// attestation when the browser attests the plan) and strips any
/// browser-supplied copy (authority is conferred by the channel,
/// never read from the payload — CC2).
pub fn takes_registrar(op: &str) -> bool {
    matches!(
        op,
        "lab.assembly.apply"
            | "lab.assembly.adopt"
            | "lab.registry.register"
            | "lab.registry.publish"
    )
}

/// Ops whose params take the `responder` declaration (§7.2 P12 —
/// `responder_provenance` on the decided row; `amend` carries
/// `attestation`/`invocation`, not `responder` — the strict schema
/// refuses the extra member).
pub fn takes_responder(op: &str) -> bool {
    matches!(op, "respond_permission")
}

/// Ops whose params take the `invocation` audit record (the branch's
/// spawn-side record — `fork` mints it onto the child run; `amend`
/// mints `lifecycle.surface.invoked`).
pub fn takes_invocation(op: &str) -> bool {
    matches!(op, "fork" | "amend")
}

/// Ops whose schema carries `idempotency_key` — the surface mints one
/// (`web:<monotonic>`) on every write that accepts it (P12's request
/// id; ADR-0130). Branch/`lab.*`/subscribe writes are content-keyed or
/// strictly-schemad without it — never injected there.
pub fn takes_idempotency(op: &str) -> bool {
    matches!(
        op,
        "respond_permission"
            | "amend"
            | "submit"
            | "cancel"
            | "steer"
            | "respond_elicitation"
            | "fork"
            | "navigate"
            | "rollback"
            | "replay"
            | "counterfactual"
    )
}

/// The admitted op → its class. `None` = the browser may not call it
/// (`Refused{op_not_admitted}` — a typed refusal, never a silent drop).
///
/// Read set (§7.2 §5.1 — every view's data lands through these; the
/// catalogue is the *intersection* of the V1–V12 read list and the ops
/// the boundary serves):
///
/// - run records: `read`, `head`, `project`, `account`, `describe`,
///   `lineage`, `run_index`, `get_artifact`, `list_leases`,
///   `env.snapshot`, `stream_events`, `subscribe`;
/// - audit/verify: `audit_view`, `verify`, `prove_inclusion`,
///   `prove_consistency`, `coherent_fork_points`;
/// - Lab reads: `lab.results.*` queries/verifies/exports-row-reads,
///   `lab.leaderboard.{leaderboard,diff_snapshots,snapshots}`,
///   `lab.analysis.{render,diff_reports,power,component_targets,
///   attribution_design}`,
///   `lab.eval.{catalogue,compare,render_scorecard,loss_report}`,
///   `lab.assembly.{plan,explain,diff,drift,identity,validate_batch,
///   compile}`,
///   `lab.debt.{index,report}`,
///   `lab.experiment.{expand,next}`,
///   `lab.registry.{resolve,query,catalog,snapshot,lineage,sameness,
///   verify,publisher_claims,slot_choices,substitutable}`;
/// - kernel bundle reads: `kernel.bundle`, `kernel.validate`,
///   `kernel.check_completeness`, `kernel.status`, `kernel.lineage`,
///   `kernel.diff`, `kernel.audit_bundle`;
/// - fleet reads: `fleet.*` read group (S5.6).
///
/// Write set (§7.2 §5.2): `respond_permission`, `amend`,
/// `kernel.reproduce`; C2 — `submit`/`cancel`/`steer`/
/// `respond_elicitation`/`request_redaction`, the branch ops
/// (`fork`/`navigate`/`rollback`/`replay`/`counterfactual`/
/// `branch.{open,discard,promote}`), the Lab writes
/// (`lab.assembly.{assemble,apply,adopt}`, `lab.registry.{register,
/// publish}`, `lab.leaderboard.{define,publish,retract_entry}`,
/// `lab.experiment.{register,open_experiment,launch,claim,settle,
/// pause,resume,close}`, `lab.analysis.analyze`, `lab.serve`,
/// `lab.permission.{grant,revoke}_approver`, `lab.debt.evaluate`).
pub fn classify(op: &str) -> Option<OpClass> {
    Some(match op {
        "read"
        | "head"
        | "project"
        | "account"
        | "describe"
        | "list_leases"
        | "lineage"
        | "run_index"
        | "get_artifact"
        | "audit_view"
        | "verify"
        | "prove_inclusion"
        | "prove_consistency"
        | "env.snapshot"
        | "coherent_fork_points"
        | "stream_events" => OpClass::Read,
        "lab.results.get_row"
        | "lab.results.row_history"
        | "lab.results.query_rows"
        | "lab.results.cells"
        | "lab.results.distribution"
        | "lab.results.catalogue"
        | "lab.results.verify_row"
        | "lab.results.verify_snapshot"
        | "lab.results.verify_citation"
        | "lab.results.export_rows"
        | "lab.results.subscribe" => OpClass::Read,
        "lab.leaderboard.leaderboard"
        | "lab.leaderboard.diff_snapshots"
        | "lab.leaderboard.snapshots" => OpClass::Read,
        "lab.analysis.render"
        | "lab.analysis.diff_reports"
        | "lab.analysis.power"
        | "lab.analysis.component_targets"
        | "lab.analysis.attribution_design" => OpClass::Read,
        "lab.eval.catalogue"
        | "lab.eval.compare"
        | "lab.eval.render_scorecard"
        | "lab.eval.loss_report" => OpClass::Read,
        // V4 — the assembly editor's read arm: plan/explain/diff/
        // drift/identity/validate_batch and the plan-time compile
        // (the `lcd_report`/`opacity_report` source — a read by the
        // op's own contract; `assemble`/`apply`/`adopt` write).
        "lab.assembly.plan"
        | "lab.assembly.explain"
        | "lab.assembly.diff"
        | "lab.assembly.drift"
        | "lab.assembly.identity"
        | "lab.assembly.validate_batch"
        | "lab.assembly.compile" => OpClass::Read,
        // V4/V10 — the debt reads (`lab.debt.evaluate` mints transition
        // rows → Write, below).
        "lab.debt.index" | "lab.debt.report" => OpClass::Read,
        "lab.experiment.expand" | "lab.experiment.next" => OpClass::Read,
        "lab.registry.resolve"
        | "lab.registry.query"
        | "lab.registry.catalog"
        | "lab.registry.snapshot"
        | "lab.registry.lineage"
        | "lab.registry.sameness"
        | "lab.registry.verify"
        | "lab.registry.publisher_claims"
        | "lab.registry.slot_choices"
        | "lab.registry.substitutable" => OpClass::Read,
        "kernel.bundle"
        | "kernel.validate"
        | "kernel.check_completeness"
        | "kernel.status"
        | "kernel.lineage"
        | "kernel.diff"
        | "kernel.audit_bundle" => OpClass::Read,
        // S5.6 — the fleet read seam (§5i.1; ADR-0207 D4): FleetView,
        // the item reads, and the adapter capability/source-record
        // reads are canonical reads (R group). The W ops —
        // `webhook_ingress`, `reconcile`, `observe`, `dispatch` — stay
        // unadmitted: the browser surface never drives fleet writes.
        "fleet.fleet_view"
        | "fleet.work_item"
        | "fleet.list"
        | "fleet.source_capabilities"
        | "fleet.source_records" => OpClass::Read,
        // ── writes ─────────────────────────────────────────────────
        "respond_permission" | "amend" | "kernel.reproduce" => OpClass::Write,
        // V12 — the live console's control inputs (idempotent under
        // `idempotency_key`; `steer` applies where the control
        // strategy declares `steer_mode`) and the wakeup subscription
        // (a durable `control.wakeup.scheduled` row — §5a.4).
        "submit" | "cancel" | "steer" | "respond_elicitation" | "subscribe" => OpClass::Write,
        // V9's `request_redaction` stays unadmitted — the canonical
        // boundary does not yet serve it (an admitted-but-absent op
        // would only ever answer the kernel's unknown-op refusal; the
        // surface does not advertise a verb that cannot run).
        // V3 — the time-travel operations (ADR-0133–0135; Group W as
        // amended). `fork`'s incoherent cut is the typed
        // `fork_point_not_coherent` refusal with the nearest coherent
        // points offered surface-side (AC-R-2.11.2-11).
        "fork" | "navigate" | "rollback" | "replay" | "counterfactual" | "branch.open"
        | "branch.discard" | "branch.promote" => OpClass::Write,
        // V4 — HirDiff editing: `assemble` (the seal-mode dry run),
        // `apply` (publish under `registrar` + attestation on widening),
        // `adopt` (the drift-adoption diff).
        "lab.assembly.assemble" | "lab.assembly.apply" | "lab.assembly.adopt" => OpClass::Write,
        // V10 — registry publishes (through `apply`'s publish spec or
        // the direct verbs; widening is the registry's own gate —
        // ADR-0037).
        "lab.registry.register" | "lab.registry.publish" => OpClass::Write,
        // V7 — leaderboard writes (`define`, `publish` — an export with
        // its loss report, `retract_entry`).
        "lab.leaderboard.define" | "lab.leaderboard.publish" | "lab.leaderboard.retract_entry" => {
            OpClass::Write
        }
        // V5 — the experiment launcher lifecycle (no ad-hoc runs —
        // AC-K2-8: every launch is an ExperimentSpec).
        "lab.experiment.register"
        | "lab.experiment.open_experiment"
        | "lab.experiment.launch"
        | "lab.experiment.claim"
        | "lab.experiment.settle"
        | "lab.experiment.pause"
        | "lab.experiment.resume"
        | "lab.experiment.close" => OpClass::Write,
        // V6 — `analyze` requests (labelled preview unless a
        // pre-registration matches — the report carries it).
        "lab.analysis.analyze" => OpClass::Write,
        // V8 — the lease grants/revocations (ADR-0070 D6).
        "lab.permission.grant_approver" | "lab.permission.revoke_approver" => OpClass::Write,
        // The served-bundle verb (§7.3 `serve` — the connection-info
        // record, never a wire tool outside the bundle).
        "lab.serve" => OpClass::Write,
        // Debt evaluation emits lifecycle transition rows — a write.
        "lab.debt.evaluate" => OpClass::Write,
        _ => return None,
    })
}

/// The `Refused{reason: op_not_admitted}` the gate emits — the closed
/// spelling the browser's error panel and the ACs pin.
pub fn refused_payload(op: &str) -> hh_wire::json::Json {
    hh_wire::json::Json::obj([
        ("error", hh_wire::json::Json::str("Refused")),
        ("reason", hh_wire::json::Json::str("op_not_admitted")),
        ("op", hh_wire::json::Json::str(op)),
    ])
}
