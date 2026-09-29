//! The invocation plane (§7.1 §2.4–2.5; ADR-0168 D1, ADR-0169 D2/D4/D5):
//! the `AttendanceDeclaration` the CLI writes into the manifest, the
//! `InvocationRecord` minted durable as `lifecycle.surface.invoked`, the
//! derived `idempotency_key`, the stdin-typing rule and the pre-ledger
//! `invocation_error` checks.
//!
//! These are the CLI's *surface* duties — the facts a run needs from an
//! operator are **set and recorded** here and implemented nowhere else
//! (ADR-0168). None of this touches kernel semantics.

use hh_embed_client_generated::{
    AttendanceDeclaration, AttendanceSource, AttendanceValue, InvocationRecord, OutputFormat,
};
use hh_wire::json::Json;

/// A pre-ledger invocation failure — `invocation_error`, emitted before
/// any boundary operation runs and never opening a run (§7.1 derivation
/// row 1). `code`/`path`/`remedy` mirror the `AssemblyDiagnostic` shape
/// (`InvocationError{code, path?, remedy}`, ADR-0169 D6).
#[derive(Debug, Clone, PartialEq)]
pub struct InvocationError {
    /// The refusal code.
    pub code: String,
    /// The offending flag/member, when one is named.
    pub path: Option<String>,
    /// The remedy hint.
    pub remedy: String,
}

impl InvocationError {
    /// A refusal with no named member.
    pub fn new(code: &str, remedy: &str) -> InvocationError {
        InvocationError {
            code: code.into(),
            path: None,
            remedy: remedy.into(),
        }
    }
    /// A refusal naming the offending flag/member.
    pub fn at(code: &str, path: &str, remedy: &str) -> InvocationError {
        InvocationError {
            code: code.into(),
            path: Some(path.into()),
            remedy: remedy.into(),
        }
    }
    /// The `InvocationError` record for the `json` result channel.
    pub fn to_json(&self) -> Json {
        let mut m: Vec<(&'static str, Json)> = vec![
            ("code", Json::str(self.code.clone())),
            ("remedy", Json::str(self.remedy.clone())),
        ];
        if let Some(p) = &self.path {
            m.push(("path", Json::str(p.clone())));
        }
        Json::obj(m)
    }
}

/// The TTY facts the inference reads (injected — `std::io::IsTerminal`
/// at the binary, fixtures in tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct Tty {
    /// stdin is a terminal.
    pub stdin: bool,
    /// stdout is a terminal.
    pub stdout: bool,
    /// stderr is a terminal.
    pub stderr: bool,
}

/// Resolve the `AttendanceDeclaration` (ADR-0168 D1):
/// - `--attendance <v>` ⇒ `declared(v)`;
/// - `--no-input` ⇒ `forced(unattended)` (a `--no-input`-class flag);
/// - otherwise `tty_inferred` — `interactive` only when the terminal
///   channels are real TTYs (stdin for answers, stdout+stderr for the
///   prompts and result); `interactive` *declared* with no TTY is M-1 —
///   `invocation_error`, never a hang on a pipe.
pub fn resolve_attendance(
    declared: Option<AttendanceValue>,
    no_input: bool,
    tty: Tty,
) -> Result<AttendanceDeclaration, InvocationError> {
    if let Some(v) = declared {
        if no_input {
            return Err(InvocationError::at(
                "flag_conflict",
                "--no-input",
                "--no-input forces unattended; drop it or drop --attendance",
            ));
        }
        if v == AttendanceValue::Interactive && !(tty.stdin && tty.stderr) {
            // M-1 — never a hang on a pipe.
            return Err(InvocationError::at(
                "interactive_requires_tty",
                "--attendance",
                "interactive attendance needs a real terminal (stdin and stderr); \
                 use --attendance async or run under a TTY",
            ));
        }
        return Ok(AttendanceDeclaration {
            value: v,
            source: AttendanceSource::Declared,
        });
    }
    if no_input {
        return Ok(AttendanceDeclaration {
            value: AttendanceValue::Unattended,
            source: AttendanceSource::Forced,
        });
    }
    let value = if tty.stdin && tty.stdout && tty.stderr {
        AttendanceValue::Interactive
    } else {
        AttendanceValue::Unattended
    };
    Ok(AttendanceDeclaration {
        value,
        source: AttendanceSource::TtyInferred,
    })
}

/// Resolve the `OutputFormat` (ADR-0169 D2): explicit flag wins; the
/// default is `human` on a TTY, else `jsonl` for streaming commands and
/// `json` for single-record commands. `streaming` names the command's
/// class (`run start`, `run events` stream; the rest answer one record).
pub fn resolve_format(
    flag: Option<OutputFormat>,
    streaming: bool,
    stdout_tty: bool,
) -> Result<OutputFormat, InvocationError> {
    if let Some(f) = flag {
        return Ok(f);
    }
    if stdout_tty {
        return Ok(OutputFormat::Human);
    }
    Ok(if streaming {
        OutputFormat::Jsonl
    } else {
        OutputFormat::Json
    })
}

/// Parse the `--format` spelling (the closed sum — an unknown spelling is
/// an `invocation_error`, not a silent default).
pub fn parse_format(s: &str) -> Result<OutputFormat, InvocationError> {
    match s {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        "jsonl" => Ok(OutputFormat::Jsonl),
        other => Err(InvocationError::at(
            "unknown_output_format",
            "--format",
            &format!("unknown format {other:?}; expected human|json|jsonl"),
        )),
    }
}

/// Parse the `--attendance` spelling.
pub fn parse_attendance(s: &str) -> Result<AttendanceValue, InvocationError> {
    match s {
        "interactive" => Ok(AttendanceValue::Interactive),
        "async" => Ok(AttendanceValue::Async),
        "unattended" => Ok(AttendanceValue::Unattended),
        other => Err(InvocationError::at(
            "unknown_attendance",
            "--attendance",
            &format!("unknown attendance {other:?}; expected interactive|async|unattended"),
        )),
    }
}

/// Build the `InvocationRecord` (§7.1 §3; ADR-0169 D5) and derive the
/// `idempotency_key = H(idp ∥ "cli" ∥ canonical(InvocationRecord))` when
/// the caller did not supply one. The key is derived over the record
/// with the member empty, then filled — the canonical derivation the
/// boundary's replay table keys on.
#[allow(clippy::too_many_arguments)]
pub fn build_invocation(
    argv: &[String],
    cwd_ref: &str,
    principal: &str,
    attendance: AttendanceDeclaration,
    output_format: OutputFormat,
    stdin_digest: Option<String>,
    instrument_record: Json,
    supplied_key: Option<String>,
) -> InvocationRecord {
    let mut rec = InvocationRecord {
        argv_canonical: argv.to_vec(),
        cwd_ref: cwd_ref.to_string(),
        principal: principal.to_string(),
        attendance,
        output_format,
        stdin_digest,
        overrides_layer_id: None,
        instrument_record,
        idempotency_key: String::new(),
    };
    rec.idempotency_key = supplied_key.unwrap_or_else(|| {
        let canonical = rec.to_json().to_canonical_string();
        hh_identity::address(format!("cli{canonical}").as_bytes(), "application/json").id()
    });
    rec
}

/// The `stdin_digest` — `sha256` of the consumed stdin bytes (the record
/// pins what was read, never the bytes).
pub fn stdin_digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        hh_wire::sha256::sha256_hex(&[b"stdin".as_slice(), bytes].concat())
    )
}

/// The typed-input rule (ADR-0169 D4): prompt text a human types at a
/// TTY enters as `Text{authority = principal, origin = human}`; piped
/// stdin enters as a separate `Text{authority = external,
/// origin = import}` — never `principal` (the rules-file backdoor row of
/// the failure table).
#[derive(Debug, Clone, PartialEq)]
pub struct InputBlock {
    /// The `submit.input[]` block.
    pub block: Json,
}

/// A text input block with its typed provenance.
pub fn text_block(text: &str, authority: &str, origin: &str) -> Json {
    Json::obj([
        ("kind", Json::str("text")),
        ("text", Json::str(text.to_string())),
        ("authority", Json::str(authority)),
        ("origin", Json::str(origin)),
    ])
}

/// `MissingBudget` — `run start` refused before any operation when
/// neither the definition, a preset nor a flag supplies a `Budget`
/// (ADR-0168 D5; there is no "unlimited" spelling). The definition-side
/// check is a shallow member scan — argument mapping, not semantics
/// (K-1): a `Budget`-kind node or an agent `budget` member satisfies it.
pub fn missing_budget(definition: &Json, flag_budget: bool) -> Result<(), InvocationError> {
    if flag_budget {
        return Ok(());
    }
    let supplied = match definition.get("nodes") {
        Some(Json::Arr(nodes)) => nodes.iter().any(|n| {
            n.get("kind").and_then(Json::as_str) == Some("Budget")
                || n.get("semantic")
                    .and_then(|s| s.get("budget"))
                    .map(|b| !matches!(b, Json::Null))
                    .unwrap_or(false)
        }),
        _ => false,
    };
    if supplied {
        Ok(())
    } else {
        Err(InvocationError::new(
            "missing_budget",
            "no Budget from the definition or a flag; supply --budget DIM=CAP[,DIM=CAP…]",
        ))
    }
}

/// `BypassWithoutContainment` — the bypass preset is accepted only when
/// the manifest can record `containment.enforcement_evidence = enforced`
/// from the environment handle (ADR-0168 D3; AC-R-2.11.1-7 — refused
/// before any run opens). At Stage 2 the kernel-provisioned `local_host`
/// binding supplies that evidence (the reference backend's attach report
/// mints probed/reported evidence for every relied-on field group —
/// [`crate::presets::environment_supplies_containment_evidence`]); every
/// other binding — a `ref` the surface cannot inspect, an unknown class —
/// fails closed here, and `open_session` re-checks kernel-side (the
/// surface gate is UX, never the authority).
pub fn bypass_without_containment(bypass: bool, environment: &Json) -> Result<(), InvocationError> {
    if bypass && !crate::presets::environment_supplies_containment_evidence(environment) {
        Err(InvocationError::at(
            "bypass_without_containment",
            "--bypass",
            "bypass requires an environment binding whose containment carries \
             kernel-minted enforcement_evidence (local_host); the requested \
             binding supplies none",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_interactive_without_tty_is_invocation_error() {
        let tty = Tty {
            stdin: false,
            stdout: true,
            stderr: true,
        };
        let r = resolve_attendance(Some(AttendanceValue::Interactive), false, tty);
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().code, "interactive_requires_tty");
    }

    #[test]
    fn no_input_forces_unattended() {
        let r = resolve_attendance(None, true, Tty::default()).unwrap();
        assert_eq!(r.value, AttendanceValue::Unattended);
        assert_eq!(r.source, AttendanceSource::Forced);
    }

    #[test]
    fn no_tty_infers_unattended() {
        let r = resolve_attendance(None, false, Tty::default()).unwrap();
        assert_eq!(r.value, AttendanceValue::Unattended);
        assert_eq!(r.source, AttendanceSource::TtyInferred);
    }

    #[test]
    fn full_tty_infers_interactive() {
        let tty = Tty {
            stdin: true,
            stdout: true,
            stderr: true,
        };
        let r = resolve_attendance(None, false, tty).unwrap();
        assert_eq!(r.value, AttendanceValue::Interactive);
        assert_eq!(r.source, AttendanceSource::TtyInferred);
    }

    #[test]
    fn no_input_plus_attendance_is_flag_conflict() {
        let r = resolve_attendance(Some(AttendanceValue::Async), true, Tty::default());
        assert_eq!(r.unwrap_err().code, "flag_conflict");
    }

    #[test]
    fn format_defaults_follow_the_tty() {
        assert_eq!(
            resolve_format(None, true, false).unwrap(),
            OutputFormat::Jsonl
        );
        assert_eq!(
            resolve_format(None, false, false).unwrap(),
            OutputFormat::Json
        );
        assert_eq!(
            resolve_format(None, true, true).unwrap(),
            OutputFormat::Human
        );
        assert!(parse_format("yaml").is_err());
    }

    #[test]
    fn idempotency_key_is_derived_deterministically() {
        let att = AttendanceDeclaration {
            value: AttendanceValue::Unattended,
            source: AttendanceSource::TtyInferred,
        };
        let a = build_invocation(
            &["run".into(), "start".into()],
            "cwd:/w",
            "principal:u",
            att.clone(),
            OutputFormat::Jsonl,
            None,
            Json::obj([]),
            None,
        );
        let b = build_invocation(
            &["run".into(), "start".into()],
            "cwd:/w",
            "principal:u",
            att,
            OutputFormat::Jsonl,
            None,
            Json::obj([]),
            None,
        );
        assert_eq!(a.idempotency_key, b.idempotency_key);
        assert!(a.idempotency_key.starts_with("sha256:"));
        let c = build_invocation(
            &["run".into(), "start".into()],
            "cwd:/w",
            "principal:u",
            AttendanceDeclaration {
                value: AttendanceValue::Unattended,
                source: AttendanceSource::TtyInferred,
            },
            OutputFormat::Jsonl,
            None,
            Json::obj([]),
            Some("supplied".into()),
        );
        assert_eq!(c.idempotency_key, "supplied");
    }

    #[test]
    fn missing_budget_reads_the_definition() {
        let with = Json::obj([(
            "nodes",
            Json::Arr(vec![Json::obj([("kind", Json::str("Budget"))])]),
        )]);
        assert!(missing_budget(&with, false).is_ok());
        let without = Json::obj([("nodes", Json::Arr(vec![]))]);
        assert_eq!(
            missing_budget(&without, false).unwrap_err().code,
            "missing_budget"
        );
        assert!(missing_budget(&without, true).is_ok());
    }
}
