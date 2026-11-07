//! The $0 targeted probe driver (REQ-XA-127; §2.3) for the two
//! records-in discordances — `grid-pattern-transform` and
//! `swe-bench-langcodes` (declared-workdir suspect). This row ships the
//! driver and its fixture run only; the probe itself is executed in W1
//! by R6.32's fleshing where the two tasks' images and verifier
//! fetches exist — the record here never claims execution.

use std::collections::BTreeMap;

use hharness_wire::json::Json;

/// The two §2.3 discordance tasks the probe covers.
pub const PROBE_TASKS: [&str; 2] = ["grid-pattern-transform", "swe-bench-langcodes"];
/// The driver record schema.
pub const PROBE_SCHEMA: &str = "hh-targeted-probe/1";

/// `probe_driver(driver_commit)` — the `$0` targeted-probe driver
/// packet: the two tasks, the candidate causes verbatim from §2.3,
/// and the honest execution label `owed{row:R6.32,window:W1}` — the
/// executed probe record is owed to R6.32 and is never claimed here.
pub fn probe_driver(driver_commit: &str) -> Json {
    let mut tasks = Vec::new();
    for (task, cause) in [
        ("grid-pattern-transform", "undiagnosed"),
        (
            "swe-bench-langcodes",
            "probable cause: declared-workdir defect (WORKDIR \
             /app/langcodes, repaired only in the repairs epoch)",
        ),
    ] {
        tasks.push(Json::obj([
            ("task_id", Json::str(task)),
            ("candidate_cause", Json::str(cause)),
        ]));
    }
    let mut m = BTreeMap::new();
    m.insert("schema".to_string(), Json::str(PROBE_SCHEMA));
    m.insert("tasks".to_string(), Json::Arr(tasks));
    m.insert(
        "basis".to_string(),
        Json::str(
            "§2.3 basis-A discordances; a $0 targeted probe belongs to \
             the runner's acceptance (REQ-XA-127), not a new tranche",
        ),
    );
    m.insert("driver_commit".to_string(), Json::str(driver_commit));
    m.insert(
        "probe_execution".to_string(),
        Json::str("owed{row:R6.32,window:W1}"),
    );
    Json::Obj(m)
}
