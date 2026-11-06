//! The verifier-bound admission (R5.57; ADR-0422) — the sanctioned
//! form of the scoped machinery admission that relaxes the enforced
//! `max_test_timeout_sec` decl for `swe-bench-astropy-2`'s verifier leg
//! so its real `run-tests.sh` bootstrap (`apt-get install gcc` +
//! `python -m venv` + `pip install -e .`) can run inside the declared
//! test window (`docs/build/runs/S4-EGRESS.md`,
//! `docs/build/reports/2026-11-05-s4-egress-cohort.json` —
//! `hh-s4-egress-supplement/1`; the ten
//! `inconclusive{verifier_unavailable:no_parseable_verdict}` slots).
//!
//! The bound path is the recorded scratch driver's verbatim
//! derivation: spec `max_test_timeout_sec` →
//! `test_deadline = min(declared, 900 s)` on the `tc-verdict` +
//! `tc-vendored` dispatches; the spec-gen default is `60` when the
//! spec carries no decl. This module is the one home of that
//! derivation and of the admission that relaxes it — a bound change
//! is always a *declared* admission, never a silent edit:
//!
//! - the admission is scoped to a named task — `record.ext["task_id"]`
//!   must equal `admission.task_id` verbatim or the spec refuses and
//!   keeps its enforced decl;
//! - the admission must name the *actually enforced* decl —
//!   `enforced_before_sec` must equal the value this spec currently
//!   enforces (the ext member, or the `60` spec-gen default when the
//!   member is absent). A mislabeled before refuses: the run ledger's
//!   stated `60` cannot be stamped onto a spec that enforced `360`
//!   (the durable `deadline: 360000` on the recorded dispatches is the
//!   evidence — the admission never presumes which decl bound a run);
//! - the admitted bound must relax and stay inside the frozen §9.2
//!   verifier envelope — `0 < enforced < admitted ≤ 900` (the driver
//!   cap). A larger need is a separately recorded admission, never a
//!   wider silent bound;
//! - on admission the spec field itself carries the relaxed decl —
//!   `ext["max_test_timeout_sec"] = admitted` — so the deadline
//!   derivation needs no second code path, and the verbatim record
//!   rides in `ext["verifier_bound_admission"]`
//!   (before → after, basis, scope, `prior_disposition`,
//!   `supplement_epoch`, authorization). A spec lacking the admission
//!   keeps the prior enforced decl byte-identically.

use hharness_wire::json::Json;

use crate::errors::EnvError;
use crate::record::EnvironmentRecord;

/// The spec surface field the enforced decl lives under — verbatim
/// the supplement's `max_test_timeout_sec` cell-spec member.
pub const SPEC_FIELD: &str = "max_test_timeout_sec";
/// The ext member the stamped admission record rides under.
pub const ADMISSION_FIELD: &str = "verifier_bound_admission";

/// The frozen §9.2 verifier envelope and the recorded driver cap —
/// `test_deadline = min(declared, 900 s)` on both verifier dispatches.
/// An admitted bound beyond this cap is a separately recorded
/// admission, never silently admitted.
pub const VERIFIER_BOUND_CAP_SEC: u64 = 900;

/// The recorded spec-gen default when a cell spec carries no
/// `max_test_timeout_sec` member (`jf("max_test_timeout_sec", 60.0)`
/// in the recorded driver machinery — S4-EGRESS).
pub const SPEC_GEN_DEFAULT_SEC: u64 = 60;

/// `VerifierBoundAdmission` — the declared relaxation: the named task
/// scope, the *actually enforced* before decl (verified against the
/// spec, never presumed), the admitted after, and the verbatim record
/// members (`basis`, `prior_disposition`, `supplement_epoch`,
/// `authorization`) the stamped record and the re-run's slot rows
/// carry.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifierBoundAdmission {
    /// The named task — the admission's entire scope (`record.ext
    /// ["task_id"]` must equal it verbatim).
    pub task_id: String,
    /// The *actually enforced* decl the admission relaxes — must equal
    /// the value this spec currently enforces (the ext member or the
    /// spec-gen default); a mislabeled before refuses.
    pub enforced_before_sec: u64,
    /// The admitted bound — must satisfy `enforced_before_sec <
    /// admitted_sec ≤ VERIFIER_BOUND_CAP_SEC`.
    pub admitted_sec: u64,
    /// The recorded basis verbatim (the inconclusive slot refs and
    /// disposition the admission stands on).
    pub basis: String,
    /// The rows' prior disposition verbatim — echoed on the re-run's
    /// slot rows (e.g. `inconclusive{verifier_unavailable:
    /// no_parseable_verdict}`).
    pub prior_disposition: String,
    /// The named supplement epoch the relaxation rides under (echoed
    /// on the re-run's slot rows verbatim).
    pub supplement_epoch: String,
    /// The verbatim authorization line the admission stands on.
    pub authorization: String,
    /// The scope statement recorded verbatim (e.g. "this task's
    /// verifier leg only — no other task, no other bound").
    pub scope: String,
}

/// `enforced_decl_sec(record)` — the decl this spec currently enforces:
/// `ext["max_test_timeout_sec"]` read as integer seconds (a `Json::Int`
/// verbatim, or a `Json::Str` carrying the task-cfg float spelling —
/// `"360.0"` reads `360`), or [`SPEC_GEN_DEFAULT_SEC`] when the member
/// is absent — the driver's `jf(…, 60.0)` default verbatim. A present
/// member of any other shape is malformed and refuses — an
/// unreadable decl never silently becomes `60`.
pub fn enforced_decl_sec(record: &EnvironmentRecord) -> Result<u64, EnvError> {
    match record.ext.get(SPEC_FIELD) {
        None => Ok(SPEC_GEN_DEFAULT_SEC),
        Some(Json::Int(i)) if *i >= 0 => Ok(*i as u64),
        Some(Json::Str(s)) => {
            let v: f64 = s.parse().map_err(|_| malformed_decl())?;
            if v.is_finite() && v >= 0.0 {
                Ok(v as u64)
            } else {
                Err(malformed_decl())
            }
        }
        Some(_) => Err(malformed_decl()),
    }
}

/// `verifier_deadline_ms(declared_sec)` — the sanctioned home of the
/// recorded driver derivation, verbatim: `test_deadline =
/// min(declared, 900 s)` in milliseconds for the `tc-verdict` and
/// `tc-vendored` dispatches. The cap binds every caller — admitted or
/// not, no decl exceeds the frozen §9.2 verifier envelope.
pub fn verifier_deadline_ms(declared_sec: u64) -> u64 {
    declared_sec.min(VERIFIER_BOUND_CAP_SEC) * 1000
}

/// `admit_verifier_bound(record, admission)` — the declared
/// relaxation. Transactional: every leg is checked before any stamp,
/// so a refused admission leaves the spec byte-identical (never a
/// half-admitted bound).
///
/// Refusals (`EnvError::Unsupported{verifier_bound.admission}`):
/// - `admission.task_id` empty, or `record.ext["task_id"]` absent —
///   the named-task scope cannot be verified;
/// - `record.ext["task_id"]` names a different task — that spec keeps
///   its enforced decl;
/// - `admission.enforced_before_sec` ≠ the spec's currently enforced
///   decl — the admission must name the *actually enforced* value
///   (the run ledger's stated `60` refuses on a spec that enforced
///   `360`);
/// - `admission.admitted_sec` is `0`, exceeds the
///   [`VERIFIER_BOUND_CAP_SEC`] envelope, or does not relax the
///   enforced decl (`admitted ≤ enforced` is not this admission);
/// - `basis`/`prior_disposition`/`supplement_epoch`/`authorization`/
///   `scope` empty — the record carries the verbatim members or
///   nothing is stamped.
///
/// On admission the spec field itself carries the relaxed decl —
/// `ext["max_test_timeout_sec"] = admitted` — so
/// [`verifier_deadline_ms`] yields the relaxed deadline with no second
/// code path, and `ext["verifier_bound_admission"]` records
/// before → after, basis, scope, `prior_disposition`,
/// `supplement_epoch`, the envelope check, and the authorization
/// verbatim.
pub fn admit_verifier_bound(
    record: &mut EnvironmentRecord,
    admission: &VerifierBoundAdmission,
) -> Result<(), EnvError> {
    if admission.task_id.is_empty() {
        return Err(unsupported(
            "the admission names no task — a bound relaxes only under a \
             named-task scope",
        ));
    }
    match record.ext.get("task_id") {
        Some(Json::Str(t)) if t == &admission.task_id => {}
        Some(Json::Str(t)) => {
            return Err(unsupported(&format!(
                "the admission names task `{}` but this spec is `{t}` — \
                 `{t}` keeps its enforced decl (no other task's bound moves)",
                admission.task_id
            )));
        }
        _ => {
            return Err(unsupported(
                "the spec carries no `task_id` member — the named-task \
                 scope cannot be verified (fail-closed)",
            ));
        }
    }
    let enforced = enforced_decl_sec(record)?;
    if admission.enforced_before_sec != enforced {
        return Err(unsupported(&format!(
            "the admission names enforced before `{}` s but this spec \
             enforces `{enforced}` s — the before must be the *actually \
             enforced* decl verbatim, never presumed",
            admission.enforced_before_sec
        )));
    }
    if admission.admitted_sec == 0 {
        return Err(unsupported(
            "the admitted bound is `0` — a verifier bound must be positive",
        ));
    }
    if admission.admitted_sec > VERIFIER_BOUND_CAP_SEC {
        return Err(unsupported(&format!(
            "the admitted bound `{}` s exceeds the frozen §9.2 verifier \
             envelope (≤{} s driver cap) — a larger need is a separately \
             recorded admission, never a silent wider bound",
            admission.admitted_sec, VERIFIER_BOUND_CAP_SEC
        )));
    }
    if admission.admitted_sec <= enforced {
        return Err(unsupported(&format!(
            "the admitted bound `{}` s does not relax the enforced `{enforced}` \
             s — this machinery admits a relaxation only",
            admission.admitted_sec
        )));
    }
    for (member, value) in [
        ("basis", &admission.basis),
        ("prior_disposition", &admission.prior_disposition),
        ("supplement_epoch", &admission.supplement_epoch),
        ("authorization", &admission.authorization),
        ("scope", &admission.scope),
    ] {
        if value.is_empty() {
            return Err(unsupported(&format!(
                "the admission's `{member}` member is empty — the record \
                 carries the verbatim members or nothing is stamped"
            )));
        }
    }
    // All legs passed — stamp. The spec field itself carries the
    // relaxed decl (deliverable: the cell-spec's declared bound IS the
    // admitted value), and the verbatim record rides alongside.
    record.ext.insert(
        SPEC_FIELD.to_string(),
        Json::Int(admission.admitted_sec as i64),
    );
    record.ext.insert(
        ADMISSION_FIELD.to_string(),
        Json::obj([
            ("task_id", Json::str(admission.task_id.clone())),
            (
                "enforced_before_sec",
                Json::Int(admission.enforced_before_sec as i64),
            ),
            ("admitted_sec", Json::Int(admission.admitted_sec as i64)),
            (
                "deadline_before_ms",
                Json::Int(verifier_deadline_ms(admission.enforced_before_sec) as i64),
            ),
            (
                "deadline_after_ms",
                Json::Int(verifier_deadline_ms(admission.admitted_sec) as i64),
            ),
            ("basis", Json::str(admission.basis.clone())),
            (
                "prior_disposition",
                Json::str(admission.prior_disposition.clone()),
            ),
            (
                "supplement_epoch",
                Json::str(admission.supplement_epoch.clone()),
            ),
            ("scope", Json::str(admission.scope.clone())),
            (
                "envelope_check",
                Json::str(format!(
                    "admitted {} s ≤ {} s driver cap = 15 min — inside the \
                     frozen §9.2 verifier envelope; a bound beyond the cap \
                     is a separately recorded admission",
                    admission.admitted_sec, VERIFIER_BOUND_CAP_SEC
                )),
            ),
            ("authorization", Json::str(admission.authorization.clone())),
        ]),
    );
    Ok(())
}

/// `slot_echo(record)` — the per-slot echo members the re-run's rows
/// carry verbatim under the admission's supplement epoch: the admitted
/// bound, `prior_disposition`, and `supplement_epoch` (R5.58 stamps
/// them on each of the ten re-executed rows; the original inconclusive
/// records stand untouched). A spec without a declared admission
/// echoes nothing — the echo exists only because the admission does.
pub fn slot_echo(record: &EnvironmentRecord) -> Vec<(&'static str, Json)> {
    let Some(adm) = record.ext.get(ADMISSION_FIELD) else {
        return Vec::new();
    };
    let mut echo = vec![];
    if let Some(v) = adm.get("admitted_sec") {
        echo.push(("verifier_bound_admitted", v.clone()));
    }
    if let Some(v) = adm.get("prior_disposition") {
        echo.push(("prior_disposition", v.clone()));
    }
    if let Some(v) = adm.get("supplement_epoch") {
        echo.push(("supplement_epoch", v.clone()));
    }
    echo
}

fn malformed_decl() -> EnvError {
    unsupported(
        "the spec's `max_test_timeout_sec` member is malformed — an \
         unreadable decl never silently becomes the default",
    )
}

fn unsupported(detail: &str) -> EnvError {
    EnvError::Unsupported {
        capability: "verifier_bound.admission",
        detail: detail.to_string(),
    }
}
