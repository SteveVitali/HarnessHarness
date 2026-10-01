//! The C1 op admission table (§7.2 §5; ADR-0302 D2) — the closed set of
//! `hh-embed/1` verbs the browser may invoke through `POST /api`. Every
//! unlisted op is `Refused{op_not_admitted}` at the gate — the browser can
//! never reach `submit`/`fork`/`rollback`/`navigate`/`register`/`publish`
//! or any session lifecycle verb (session management is the surface's
//! own, P12 — the surface opens the session; the browser names the run).

/// Whether an admitted op is a mutation (the P4 legs — fetch metadata,
/// script header, JSON content-type — apply to these + to every
/// non-`GET` route).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// A canonical read — `read`/`project`/`query_rows`/`render` and the
    /// Lab view renders (§7.2 §5.1).
    Read,
    /// A declared write — `respond_permission`, `amend`,
    /// `kernel.reproduce` (§7.2 §5.2; every one lands a ledgered row with
    /// responder provenance + a request id — P12).
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
    )
}

/// The C1 admitted op → its class. `None` = the browser may not call it
/// (`Refused{op_not_admitted}` — a typed refusal, never a silent drop).
///
/// Read set (§7.2 §5.1 — every view's data lands through these; the
/// catalogue is the *intersection* of the V1–V11 read list and the ops
/// the boundary serves):
///
/// - run records: `read`, `head`, `project`, `account`, `describe`,
///   `lineage`, `run_index`, `get_artifact`, `list_leases`,
///   `env.snapshot`;
/// - audit/verify: `audit_view`, `verify`, `prove_inclusion`,
///   `prove_consistency`, `coherent_fork_points` (experimental — the
///   honest typed refusal when the gate is off);
/// - Lab reads: `lab.results.*` queries/verifies/exports-row-reads,
///   `lab.leaderboard.{leaderboard,diff_snapshots,snapshots}`,
///   `lab.analysis.{render,diff_reports,power}`,
///   `lab.eval.{catalogue,compare,render_scorecard,loss_report}`,
///   `lab.registry.{resolve,query,catalog,snapshot,lineage,sameness,
///   verify,publisher_claims,slot_choices,substitutable}`;
/// - kernel bundle reads: `kernel.bundle`, `kernel.validate`,
///   `kernel.check_completeness`, `kernel.status`, `kernel.lineage`,
///   `kernel.diff`, `kernel.audit_bundle`.
///
/// Write set (§7.2 §5.2): `respond_permission`, `amend` (the kernel's
/// own target table admits only what the stage allows — `budget`),
/// `kernel.reproduce`.
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
        | "coherent_fork_points" => OpClass::Read,
        "lab.results.get_row"
        | "lab.results.row_history"
        | "lab.results.query_rows"
        | "lab.results.cells"
        | "lab.results.distribution"
        | "lab.results.catalogue"
        | "lab.results.verify_row"
        | "lab.results.verify_snapshot"
        | "lab.results.verify_citation"
        | "lab.results.export_rows" => OpClass::Read,
        "lab.leaderboard.leaderboard" | "lab.leaderboard.diff_snapshots" => OpClass::Read,
        "lab.analysis.render" | "lab.analysis.diff_reports" | "lab.analysis.power" => OpClass::Read,
        "lab.eval.catalogue"
        | "lab.eval.compare"
        | "lab.eval.render_scorecard"
        | "lab.eval.loss_report" => OpClass::Read,
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
        "respond_permission" | "amend" | "kernel.reproduce" => OpClass::Write,
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
