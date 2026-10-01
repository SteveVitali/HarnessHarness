//! The Stage-3 Lab nouns and the bundle/run import-export commands
//! (§7.1 verb table; R-2.11.1 C1/Stage-3 row; CF-484 moved
//! `bundle create/validate` + `run export/import` here from Stage 2).
//!
//! Every command is a thin boundary forwarder — one named `hh-embed/1`
//! operation per verb (K-1/K-2, N-3: Lab nouns add no semantics). Verbs
//! whose owning Group L/M op is registered but `stage_pending` (the
//! Lab *services* land at S3.4a/S3.4b/S3.4c/S3.5 per the manifest
//! chain) forward normally and surface the typed refusal verbatim;
//! verbs with no registered op at all (`definition space`,
//! `experiment status|plans|runs`, `registry discover`,
//! `bundle attest|verify`) refuse `stage_pending` at invocation — a
//! noun may not ship without its binding (ADR-0167 D4 as amended).
//!
//! `--dry-run` parity: a work-injecting verb's dry form is the
//! owning contract's planning half (`apply --dry-run` = `plan`,
//! byte-equal minus retention — ADR-0147 S-1; `experiment open
//! --dry-run` = `expand` + no `open`; `run import --dry-run` =
//! `kernel.check_completeness` — validation with no lift). The result
//! record carries `dry_run: true` so the parity is visible on the wire.

use std::path::Path;

use hh_embed_client_generated::OutputFormat;
use hh_wire::json::Json;

use crate::boundary::{Boundary, CliError};
use crate::cli::{ok_outcome, require_pos, Io};
use crate::exit_class::ExitClass;
use crate::invocation::{parse_format, resolve_format, InvocationError};

/// The `--dry-run` switch.
fn dry(p: &crate::cli::Parsed) -> bool {
    p.has("dry-run")
}

/// The command's output format (`--format` else the tty/pipe default).
fn fmt(p: &crate::cli::Parsed, io: &Io) -> Result<OutputFormat, CliError> {
    resolve_format(
        p.flag("format")
            .map(|f| parse_format(&f))
            .transpose()
            .map_err(CliError::Invocation)?,
        false,
        io.tty.stdout,
    )
    .map_err(CliError::Invocation)
}

/// One boundary call.
fn call(b: &mut dyn Boundary, method: &str, params: Json) -> Result<Json, CliError> {
    b.call(method, &params)
}

/// A positional argument read as a file's text (`definition`/`spec`
/// operands are files; a `-` positional reads the piped stdin).
fn file_text(p: &crate::cli::Parsed, io: &Io, i: usize, name: &str) -> Result<String, CliError> {
    let path = require_pos(p, i, name)?;
    if path == "-" {
        let bytes = io.stdin.clone().unwrap_or_default();
        return String::from_utf8(bytes).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "invalid_utf8",
                name,
                &format!("stdin is not utf-8: {e}"),
            ))
        });
    }
    std::fs::read_to_string(&path).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "unreadable_file",
            &path,
            &format!("{e}"),
        ))
    })
}

/// A positional definition file parsed as canonical JSON — the
/// `definition` verbs take an `AssemblySource` (`hir/1` source JSON;
/// landed at S3.5). Malformed bytes are an invocation error, never
/// forwarded to the service.
fn source_json(p: &crate::cli::Parsed, io: &Io, i: usize, name: &str) -> Result<Json, CliError> {
    let text = file_text(p, io, i, name)?;
    hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at("malformed_json", name, &format!("{e}")))
    })
}

/// The CLI principal's `ProvenanceRecord` as a `registrar` argument —
/// `human(author_ref = principal, role = author)` mints at `principal`
/// (§8.1 #3 `default_authority`); `created_at` is the surface's
/// wall-clock-free `0` (the registry stamps its own times).
fn registrar(io: &Io) -> Json {
    Json::obj([
        (
            "origin",
            Json::obj([
                ("kind", Json::str("human")),
                ("author_ref", Json::str(io.principal.clone())),
                ("role", Json::str("author")),
            ]),
        ),
        ("authority", Json::str("principal")),
        // `user` — the human-principal scope (the closed
        // `PersistenceScope` vocabulary; `minted` derives the same
        // ceiling: principal at user scope).
        ("scope", Json::str("user")),
        ("created_at", Json::Int(0)),
    ])
}

/// A bundle operand — a directory (`path`) or a sealed container file
/// (`container`), decided by the filesystem so the caller never has to
/// name the encoding.
fn bundle_ref(p: &crate::cli::Parsed, i: usize, noun: &str) -> Result<(String, Json), CliError> {
    let path = require_pos(p, i, "<bundle-path>")?;
    let is_dir = Path::new(&path).is_dir();
    let key = if is_dir { "path" } else { "container" };
    let _ = noun;
    Ok((path.clone(), Json::obj([(key, Json::str(path))])))
}

/// A `stage_pending` refusal for verbs with no registered `hh-embed/1`
/// op yet — the same shape `env open/attach/…` uses.
fn stage_pending(cmd: &str, note: &str) -> CliError {
    CliError::Invocation(InvocationError::at(
        "stage_pending",
        cmd,
        &format!("{note} — the verb is refused, never faked"),
    ))
}

fn idem(p: &crate::cli::Parsed) -> Json {
    match p.flag("idempotency-key") {
        Some(k) => Json::str(k),
        None => Json::Null,
    }
}

// ── definition (Harness Definition) — Group L `lab.assembly.*` ─────────

/// `definition plan <file>` → `lab.assembly.plan` — the planning half
/// of apply (the `--dry-run` target; ADR-0147 S-1). The file is an
/// `AssemblySource` (`hir/1` source JSON); `--snapshot-id` pins the
/// snapshot the plan resolves against (`head`/absent → a fresh cut).
pub fn cmd_definition_plan(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let source = source_json(p, io, 0, "<definition-file>")?;
    let r = call(
        b,
        "lab.assembly.plan",
        Json::obj([
            ("source", source),
            (
                "snapshot",
                p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("definition_plan", r, fmt(p, io)?)
}

/// `definition apply <file>` → `lab.assembly.apply`; `--dry-run` lowers
/// to `lab.assembly.plan` (byte-equal minus retention — ADR-0147 S-1).
/// `--publish <name>` publishes the sealed definition under
/// `namespace/name` (`--namespace`, default `local`; `--label`,
/// `--supersedes`).
pub fn cmd_definition_apply(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let source = source_json(p, io, 0, "<definition-file>")?;
    if dry(p) {
        let r = call(
            b,
            "lab.assembly.plan",
            Json::obj([
                ("source", source),
                (
                    "snapshot",
                    p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
                ),
            ]),
        )?;
        return ok_outcome(
            "definition_plan",
            Json::obj([("dry_run", Json::Bool(true)), ("plan", r)]),
            fmt(p, io)?,
        );
    }
    let mut members: Vec<(&'static str, Json)> = vec![
        ("source", source),
        ("registrar", registrar(io)),
        ("idempotency_key", idem(p)),
    ];
    if let Some(name) = p.flag("publish") {
        members.push((
            "publish",
            Json::obj([
                (
                    "namespace",
                    Json::str(p.flag("namespace").unwrap_or_else(|| "local".into())),
                ),
                ("name", Json::str(name)),
                (
                    "label",
                    p.flag("label").map(Json::str).unwrap_or(Json::Null),
                ),
                (
                    "supersedes",
                    p.flag("supersedes").map(Json::str).unwrap_or(Json::Null),
                ),
            ]),
        ));
    }
    let r = call(b, "lab.assembly.apply", Json::obj(members))?;
    ok_outcome("definition_apply", r, fmt(p, io)?)
}

/// `definition diff <version-id-a> <version-id-b>` → `lab.assembly.diff`
/// — the `AssemblyDiff` over two sealed definitions (T-2: the diff's
/// endpoints are sealed definitions, never loose files).
pub fn cmd_definition_diff(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let a = require_pos(p, 0, "<version-id-a>")?;
    let bb = require_pos(p, 1, "<version-id-b>")?;
    let r = call(
        b,
        "lab.assembly.diff",
        Json::obj([("a", Json::str(a)), ("b", Json::str(bb))]),
    )?;
    ok_outcome("definition_diff", r, fmt(p, io)?)
}

/// `definition explain <file>` → `lab.assembly.explain` — the layer
/// attribution for an `AssemblySource`; `--sealed <version-id>` explains
/// a sealed definition instead.
pub fn cmd_definition_explain(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let params = match p.flag("sealed") {
        Some(vid) => Json::obj([("sealed", Json::str(vid))]),
        None => Json::obj([("source", source_json(p, io, 0, "<definition-file>")?)]),
    };
    let r = call(b, "lab.assembly.explain", params)?;
    ok_outcome("definition_explain", r, fmt(p, io)?)
}

/// `definition validate <file>…` → `lab.assembly.validate_batch` (one
/// file) or `assemble(mode = plan)` (the verb table's pair).
pub fn cmd_definition_validate(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    if p.positional.is_empty() {
        return Err(CliError::Invocation(InvocationError::at(
            "missing_operand",
            "definition validate",
            "usage: hh definition validate <file>…",
        )));
    }
    let mut sources = Vec::new();
    for i in 0..p.positional.len() {
        sources.push(source_json(p, io, i, "<definition-file>")?);
    }
    let r = if sources.len() == 1 {
        call(
            b,
            "lab.assembly.assemble",
            Json::obj([("mode", Json::str("plan")), ("source", sources.remove(0))]),
        )?
    } else {
        let points = sources
            .into_iter()
            .map(|s| Json::obj([("source", s)]))
            .collect();
        call(
            b,
            "lab.assembly.validate_batch",
            Json::obj([("points", Json::Arr(points))]),
        )?
    };
    ok_outcome("definition_validate", r, fmt(p, io)?)
}

/// `definition identity <version-id>` → `lab.assembly.identity` — the
/// sealed definition's `DefinitionIdentity`/refs.
pub fn cmd_definition_identity(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let vid = require_pos(p, 0, "<version-id>")?;
    let r = call(
        b,
        "lab.assembly.identity",
        Json::obj([("sealed", Json::str(vid))]),
    )?;
    ok_outcome("definition_identity", r, fmt(p, io)?)
}

/// `definition compile <file>` → `lab.assembly.compile` — plan-time
/// `compile` for `lcd_report`/`opacity_report` (ADR-0147–0149).
pub fn cmd_definition_compile(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    // `lab.assembly.compile` takes a `DefinitionInput` — the same
    // `<file|ref:…>` operand `serve`/`run start` speak (one spelling
    // across verbs; S4.12). `-` is the stdin document.
    let operand = require_pos(p, 0, "<definition-file|ref:…>")?;
    let definition = if operand == "-" {
        let text = file_text(p, io, 0, "<definition-file>")?;
        let doc = hh_wire::json::parse(&text).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "definition_invalid_json",
                "<definition-file>",
                &format!("{e}"),
            ))
        })?;
        Json::obj([("kind", Json::str("document")), ("document", doc)])
    } else {
        definition_operand(&operand)?
    };
    let r = call(
        b,
        "lab.assembly.compile",
        Json::Obj({
            let mut m = std::collections::BTreeMap::new();
            m.insert("definition".to_string(), definition);
            if let Some(prof) = p.flag("profile") {
                m.insert("profile_refs".to_string(), Json::Arr(vec![Json::str(prof)]));
            }
            m
        }),
    )?;
    ok_outcome("definition_compile", r, fmt(p, io)?)
}

// ── registry — Group L `lab.registry.*` (implemented S2.12) ────────────

/// `registry catalog` → `lab.registry.catalog`.
pub fn cmd_registry_catalog(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(
        b,
        "lab.registry.catalog",
        Json::obj([(
            "snapshot_id",
            p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
        )]),
    )?;
    ok_outcome("registry_catalog", r, fmt(p, io)?)
}

/// `registry query [--field op value]…` → `lab.registry.query`. Clause
/// flags: `--field`/`--op`/`--value` triples in order (the closed
/// `eq`/`contains` vocabulary is enforced kernel-side).
pub fn cmd_registry_query(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let fields = p.flag_all("field");
    let ops = p.flag_all("op");
    let values = p.flag_all("value");
    if !(fields.len() == ops.len() && ops.len() == values.len()) {
        return Err(CliError::Invocation(InvocationError::at(
            "clause_mismatch",
            "registry query",
            "each clause is one --field/--op/--value triple",
        )));
    }
    let clauses: Vec<Json> = fields
        .iter()
        .zip(ops.iter())
        .zip(values.iter())
        .map(|((f, o), v)| {
            Json::obj([
                ("field", Json::str(f.clone())),
                ("op", Json::str(o.clone())),
                ("value", Json::str(v.clone())),
            ])
        })
        .collect();
    let r = call(
        b,
        "lab.registry.query",
        Json::obj([
            ("clauses", Json::Arr(clauses)),
            (
                "snapshot_id",
                p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("registry_query", r, fmt(p, io)?)
}

/// `registry resolve <name> [--namespace] [--label] [--mode]
/// [--version-id]` → `lab.registry.resolve`.
pub fn cmd_registry_resolve(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let mut params = Json::obj([
        (
            "namespace",
            p.flag("namespace")
                .map(Json::str)
                .unwrap_or_else(|| Json::str("local")),
        ),
        (
            "label",
            p.flag("label").map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "mode",
            p.flag("mode")
                .map(Json::str)
                .unwrap_or_else(|| Json::str("execute")),
        ),
        (
            "snapshot_id",
            p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
        ),
    ]);
    if let Some(vid) = p.flag("version-id").or_else(|| {
        p.positional
            .first()
            .cloned()
            .filter(|s| s.starts_with("idp:") || s.contains('@'))
    }) {
        if let Json::Obj(m) = &mut params {
            m.insert("version_id".to_string(), Json::str(vid));
        }
    } else if let Ok(name) = require_pos(p, 0, "<name>") {
        if let Json::Obj(m) = &mut params {
            m.insert("name".to_string(), Json::str(name));
        }
    }
    let r = call(b, "lab.registry.resolve", params)?;
    ok_outcome("registry_resolve", r, fmt(p, io)?)
}

/// `registry register <record-file> --kind <kind>` →
/// `lab.registry.register`. `--kind` defaults to the body's `kind`/
/// `record_kind` member.
pub fn cmd_registry_register(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let text = file_text(p, io, 0, "<record-file>")?;
    let body = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "record-file",
            &format!("{e:?}"),
        ))
    })?;
    let kind = p
        .flag("kind")
        .or_else(|| {
            body.get("kind")
                .or_else(|| body.get("record_kind"))
                .and_then(Json::as_str)
                .map(String::from)
        })
        .ok_or_else(|| {
            CliError::Invocation(InvocationError::at(
                "missing_flag",
                "--kind",
                "registry register needs --kind (or a `kind` member in the record body)",
            ))
        })?;
    let r = call(
        b,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str(kind)),
            ("body", body),
            ("registrar", registrar(io)),
            (
                "trust_record_ref",
                p.flag("trust-record-ref")
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("registry_register", r, fmt(p, io)?)
}

/// `registry publish <name> --namespace <ns> --version-id <id>
/// [--label] [--supersedes]` → `lab.registry.publish`.
pub fn cmd_registry_publish(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let name = require_pos(p, 0, "<name>")?;
    let namespace = p.flag("namespace").ok_or_else(|| {
        CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--namespace",
            "registry publish needs --namespace",
        ))
    })?;
    let version_id = p.flag("version-id").ok_or_else(|| {
        CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--version-id",
            "registry publish needs --version-id",
        ))
    })?;
    let r = call(
        b,
        "lab.registry.publish",
        Json::obj([
            ("namespace", Json::str(namespace)),
            ("name", Json::str(name)),
            ("version_id", Json::str(version_id)),
            (
                "label",
                p.flag("label").map(Json::str).unwrap_or(Json::Null),
            ),
            (
                "supersedes",
                p.flag("supersedes").map(Json::str).unwrap_or(Json::Null),
            ),
            ("registrar", registrar(io)),
        ]),
    )?;
    ok_outcome("registry_publish", r, fmt(p, io)?)
}

/// `registry snapshot` → `lab.registry.snapshot`.
pub fn cmd_registry_snapshot(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.registry.snapshot", Json::obj([]))?;
    ok_outcome("registry_snapshot", r, fmt(p, io)?)
}

/// `registry conformance <report-file>` →
/// `lab.registry.record_conformance` (the file is a
/// `ConformanceReport` record body).
pub fn cmd_registry_conformance(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let text = file_text(p, io, 0, "<report-file>")?;
    let report = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "report-file",
            &format!("{e:?}"),
        ))
    })?;
    let r = call(
        b,
        "lab.registry.record_conformance",
        Json::obj([("report", report), ("registrar", registrar(io))]),
    )?;
    ok_outcome("registry_conformance", r, fmt(p, io)?)
}

/// `registry deprecate|yank <name> --namespace <ns>` →
/// `lab.registry.deprecate|yank`.
pub fn cmd_registry_name_status(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let name = require_pos(p, 0, "<name>")?;
    let namespace = p.flag("namespace").unwrap_or_else(|| "local".into());
    let r = call(
        b,
        &format!("lab.registry.{verb}"),
        Json::obj([
            ("namespace", Json::str(namespace)),
            ("name", Json::str(name)),
            ("registrar", registrar(io)),
        ]),
    )?;
    ok_outcome("registry_name_status", r, fmt(p, io)?)
}

/// `registry revoke <version-id> --reason <r>` → `lab.registry.revoke`.
pub fn cmd_registry_revoke(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let version_id = require_pos(p, 0, "<version-id>")?;
    let reason = p.flag("reason").ok_or_else(|| {
        CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--reason",
            "registry revoke needs --reason",
        ))
    })?;
    let r = call(
        b,
        "lab.registry.revoke",
        Json::obj([
            ("version_id", Json::str(version_id)),
            ("reason", Json::str(reason)),
            (
                "replacement",
                p.flag("replacement").map(Json::str).unwrap_or(Json::Null),
            ),
            ("registrar", registrar(io)),
        ]),
    )?;
    ok_outcome("registry_revoke", r, fmt(p, io)?)
}

/// `registry lineage <version-id>` → `lab.registry.lineage`.
pub fn cmd_registry_lineage(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let version_id = require_pos(p, 0, "<version-id>")?;
    let r = call(
        b,
        "lab.registry.lineage",
        Json::obj([("version_id", Json::str(version_id))]),
    )?;
    ok_outcome("registry_lineage", r, fmt(p, io)?)
}

/// `registry sameness <a> <b>` → `lab.registry.sameness`.
pub fn cmd_registry_sameness(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let a = require_pos(p, 0, "<version-id-a>")?;
    let bb = require_pos(p, 1, "<version-id-b>")?;
    let r = call(
        b,
        "lab.registry.sameness",
        Json::obj([("a", Json::str(a)), ("b", Json::str(bb))]),
    )?;
    ok_outcome("registry_sameness", r, fmt(p, io)?)
}

/// `registry verify` → `lab.registry.verify` — the content/snapshot/log
/// integrity check.
pub fn cmd_registry_verify(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.registry.verify", Json::obj([]))?;
    ok_outcome("registry_verify", r, fmt(p, io)?)
}

// ── experiment — Group L `lab.experiment.*` (service lands S3.4a) ──────

/// `experiment register <spec-file>` → `lab.experiment.register`.
/// `--dry-run` runs the same op — a registered spec is the record the
/// dry run previews (`expand` is the preview half).
pub fn cmd_experiment_register(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let text = file_text(p, io, 0, "<spec-file>")?;
    let spec = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "spec-file",
            &format!("{e:?}"),
        ))
    })?;
    let mut params = Json::obj([("spec", spec), ("idempotency_key", idem(p))]);
    if dry(p) {
        if let Json::Obj(m) = &mut params {
            m.insert("dry_run".to_string(), Json::Bool(true));
        }
    }
    let r = call(b, "lab.experiment.register", params)?;
    ok_outcome("experiment_register", r, fmt(p, io)?)
}

/// `experiment expand <experiment-id>` → `lab.experiment.expand` — the
/// `CellPlan`/`RunPlan` preview (already non-mutating; the `--dry-run`
/// of `open`).
pub fn cmd_experiment_expand(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let id = require_pos(p, 0, "<experiment-id>")?;
    let r = call(
        b,
        "lab.experiment.expand",
        Json::obj([("experiment_id", Json::str(id))]),
    )?;
    ok_outcome("experiment_expand", r, fmt(p, io)?)
}

/// `experiment open <experiment-id>` → `lab.experiment.open_experiment`;
/// `--dry-run` = `expand` + `validate_batch` with no `open` (the verb
/// table's dry form — here the `expand` half; `validate_batch` folds
/// into the engine's own admission at open).
pub fn cmd_experiment_open(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let id = require_pos(p, 0, "<experiment-id>")?;
    if dry(p) {
        let r = call(
            b,
            "lab.experiment.expand",
            Json::obj([("experiment_id", Json::str(id))]),
        )?;
        return ok_outcome(
            "experiment_expand",
            Json::obj([("dry_run", Json::Bool(true)), ("expand", r)]),
            fmt(p, io)?,
        );
    }
    let r = call(
        b,
        "lab.experiment.open_experiment",
        Json::obj([
            ("experiment_id", Json::str(id)),
            ("idempotency_key", idem(p)),
        ]),
    )?;
    ok_outcome("experiment_open", r, fmt(p, io)?)
}

/// `experiment next|claim|launch|settle|pause|resume|close
/// <experiment-id>` → the same-named Group L op. `close` returns the
/// `ExperimentReport` (bundle-bearing) per the verb table.
pub fn cmd_experiment_op(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let id = require_pos(p, 0, "<experiment-id>")?;
    let mut params = Json::obj([("experiment_id", Json::str(id))]);
    if let Some(v) = p.flag("cell") {
        if let Json::Obj(m) = &mut params {
            m.insert("cell".to_string(), Json::str(v));
        }
    }
    if let Some(v) = p.flag("run") {
        if let Json::Obj(m) = &mut params {
            m.insert("run_id".to_string(), Json::str(v));
        }
    }
    let r = call(b, &format!("lab.experiment.{verb}"), params)?;
    ok_outcome(&format!("experiment_{verb}"), r, fmt(p, io)?)
}

// ── results — Group L `lab.results.*` (service lands S3.4b) ────────────

/// `results query [--experiment <id>] [--metric <m>] [--cell <c>]
/// [--run <id>] [--limit <n>]` → `lab.results.query_rows{QuerySpec}`.
pub fn cmd_results_query(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let query = Json::obj([
        (
            "experiment_id",
            p.flag("experiment").map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "metric",
            p.flag("metric").map(Json::str).unwrap_or(Json::Null),
        ),
        ("cell", p.flag("cell").map(Json::str).unwrap_or(Json::Null)),
        ("run_id", p.flag("run").map(Json::str).unwrap_or(Json::Null)),
    ]);
    let mut params = Json::obj([("query", query)]);
    if let Some(l) = p.flag("limit") {
        if let Ok(n) = l.parse::<i64>() {
            if let Json::Obj(m) = &mut params {
                m.insert("limit".to_string(), Json::Int(n));
            }
        }
    }
    let r = call(b, "lab.results.query_rows", params)?;
    ok_outcome("results_query", r, fmt(p, io)?)
}

/// `results row|history <row-id>` → `get_row`/`row_history`.
pub fn cmd_results_row(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let row_id = require_pos(p, 0, "<row-id>")?;
    let op = if verb == "history" {
        "row_history"
    } else {
        "get_row"
    };
    let r = call(
        b,
        &format!("lab.results.{op}"),
        Json::obj([("row_id", Json::str(row_id))]),
    )?;
    ok_outcome(&format!("results_{verb}"), r, fmt(p, io)?)
}

/// `results cells <design>` → `lab.results.cells`.
pub fn cmd_results_cells(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let design = require_pos(p, 0, "<design>")?;
    let r = call(
        b,
        "lab.results.cells",
        Json::obj([("design", Json::str(design))]),
    )?;
    ok_outcome("results_cells", r, fmt(p, io)?)
}

/// `results distribution <metric>` → `lab.results.distribution`.
pub fn cmd_results_distribution(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let metric = require_pos(p, 0, "<metric>")?;
    let r = call(
        b,
        "lab.results.distribution",
        Json::obj([("metric", Json::str(metric))]),
    )?;
    ok_outcome("results_distribution", r, fmt(p, io)?)
}

/// `results catalogue` → `lab.results.catalogue`.
pub fn cmd_results_catalogue(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.results.catalogue", Json::obj([]))?;
    ok_outcome("results_catalogue", r, fmt(p, io)?)
}

/// `results verify-row <row-id>` → `lab.results.verify_row`.
pub fn cmd_results_verify_row(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let row_id = require_pos(p, 0, "<row-id>")?;
    let r = call(
        b,
        "lab.results.verify_row",
        Json::obj([("row_id", Json::str(row_id))]),
    )?;
    ok_outcome("results_verify_row", r, fmt(p, io)?)
}

/// `results export <target>` → `lab.results.export_rows` — only
/// `ledger_native_rows` is lossless; `n/a{reason}` never lowers to 0.
pub fn cmd_results_export(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let target = require_pos(p, 0, "<target>")?;
    let r = call(
        b,
        "lab.results.export_rows",
        Json::obj([("target", Json::str(target))]),
    )?;
    ok_outcome("results_export", r, fmt(p, io)?)
}

// ── compare — Group L `lab.analysis.*` (service lands S3.4c) ───────────

/// `compare report <baseline> <candidate>` → `lab.analysis.analyze`
/// then `lab.analysis.render{view = report}` — the `ComparisonReport`
/// with `benefit_kind` + `budget_match.status` (the verb table's
/// two-op sequence).
pub fn cmd_compare_report(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let baseline = require_pos(p, 0, "<baseline>")?;
    let candidate = require_pos(p, 1, "<candidate>")?;
    let analysis = call(
        b,
        "lab.analysis.analyze",
        Json::obj([
            ("baseline", Json::str(baseline)),
            ("candidate", Json::str(candidate)),
        ]),
    )?;
    let analysis_ref = analysis
        .get("analysis_ref")
        .or_else(|| analysis.get("id"))
        .cloned()
        .unwrap_or(analysis.clone());
    let rendered = call(
        b,
        "lab.analysis.render",
        Json::obj([("analysis", analysis_ref), ("view", Json::str("report"))]),
    )?;
    ok_outcome("compare_report", rendered, fmt(p, io)?)
}

/// `compare scorecard [--experiment <id>]` →
/// `lab.analysis.render{view = scorecard}`.
pub fn cmd_compare_scorecard(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(
        b,
        "lab.analysis.render",
        Json::obj([
            ("view", Json::str("scorecard")),
            (
                "experiment_id",
                p.flag("experiment").map(Json::str).unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("compare_scorecard", r, fmt(p, io)?)
}

// ── bundle — Group M `kernel.bundle`/`check_completeness`/`reproduce` ──

/// `bundle create <run-id> [--sink <dir>]` → `kernel.bundle` — the
/// `bundle(kind = run)` assembly; `--sink` names the `deliver_sink`
/// directory (the `measurement.export.delivered` row lands when set).
pub fn cmd_bundle_create(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run-id>")?;
    let r = call(
        b,
        "kernel.bundle",
        Json::obj([
            ("run_id", Json::str(run_id)),
            (
                "deliver_sink",
                p.flag("sink").map(Json::str).unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("bundle_create", r, fmt(p, io)?)
}

/// `bundle validate <path>` → `kernel.check_completeness` — the staged
/// `validate_bundle` gate; the report is the result (`ok`/`errors`),
/// `max_supported_level` and `dirty` are never hidden.
pub fn cmd_bundle_validate(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (_, arg) = bundle_ref(p, 0, "bundle")?;
    let r = call(b, "kernel.check_completeness", arg)?;
    ok_outcome("bundle_validate", r, fmt(p, io)?)
}

/// `bundle reproduce <path> --level <R0|R1|R2|R3>` →
/// `kernel.reproduce` — the `ReproReport`; level is required (no silent
/// default — the claim is the level).
pub fn cmd_bundle_reproduce(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (_, arg) = bundle_ref(p, 0, "bundle")?;
    let level = p.flag("level").ok_or_else(|| {
        CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--level",
            "bundle reproduce needs --level R0|R1|R2|R3",
        ))
    })?;
    let mut params = arg;
    if let Json::Obj(m) = &mut params {
        m.insert("level".to_string(), Json::str(level));
        if let Some(eb) = p.flag("eval-budget") {
            if let Ok(n) = eb.parse::<i64>() {
                m.insert("eval_budget".to_string(), Json::Int(n));
            }
        }
    }
    let r = call(b, "kernel.reproduce", params)?;
    ok_outcome("bundle_reproduce", r, fmt(p, io)?)
}

/// `bundle show <path>` → `kernel.check_completeness` — the manifest
/// summary + member table (the decode+inspect half of validation; the
/// full report is `bundle validate`).
pub fn cmd_bundle_show(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (_, arg) = bundle_ref(p, 0, "bundle")?;
    let r = call(b, "kernel.check_completeness", arg)?;
    let summary = r.get("summary").cloned().unwrap_or(r.clone());
    ok_outcome("bundle_show", summary, fmt(p, io)?)
}

// ── run export/import — Group M `kernel.bundle`/`kernel.import` ────────

/// `run export <run-id> [--out <dir>]` → `kernel.bundle` with
/// `deliver_sink` — the ADR-0141 lowering (Group M `bundle` + Group R
/// `project`/`read`, composed kernel-side with the `LedgerExport`
/// manifest section and the `measurement.export.delivered` row).
pub fn cmd_run_export(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run-id>")?;
    let sink = p.flag("out").or_else(|| p.flag("sink")).ok_or_else(|| {
        CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--out",
            "run export needs --out <dir> (the delivery sink)",
        ))
    })?;
    let r = call(
        b,
        "kernel.bundle",
        Json::obj([
            ("run_id", Json::str(run_id)),
            ("deliver_sink", Json::str(sink)),
        ]),
    )?;
    ok_outcome("run_export", r, fmt(p, io)?)
}

/// `run import <path>` → `kernel.import` — the ADR-0021/0141 lift;
/// every lifted fact is `authority = unverified` and the
/// `lifecycle.run.imported` receipt names the source coordinates.
/// `--dry-run` runs `kernel.check_completeness` only — the validation
/// half with no lift.
pub fn cmd_run_import(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (_, arg) = bundle_ref(p, 0, "bundle")?;
    if dry(p) {
        let r = call(b, "kernel.check_completeness", arg)?;
        return ok_outcome(
            "run_import",
            Json::obj([("dry_run", Json::Bool(true)), ("completeness", r)]),
            fmt(p, io)?,
        );
    }
    let mut params = arg;
    if let Json::Obj(m) = &mut params {
        m.insert("holder".to_string(), Json::str(io.principal.clone()));
    }
    let r = call(b, "kernel.import", params)?;
    ok_outcome("run_import", r, fmt(p, io)?)
}

// ── lab serve — Group L `lab.serve` (R-2.11.3⁰; ADR-0097 D7) ───────────

/// `lab serve <bundle-path>` → `lab.serve` — decode the bundle, lower
/// its `target:mcp` member and return the `stdio_launch` CallerBinding
/// (fixed test principal) plus the `hh-mcp-serve` launch descriptor.
/// The CLI surface writes the binding (ADR-0174 D2); the stdio pair is
/// the caller's — `hh-mcp-serve <path>` is the spawned half.
pub fn cmd_lab_serve(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (_, arg) = bundle_ref(p, 0, "bundle")?;
    let r = call(b, "lab.serve", arg)?;
    if let Some(launch) = r
        .get("launch")
        .and_then(|l| l.get("command"))
        .and_then(Json::as_str)
    {
        let _ = writeln!(io.err, "serve: spawn `{launch}` over stdio");
    }
    ok_outcome("lab_serve", r, fmt(p, io)?)
}

/// The stage_pending refusals for verbs without a registered op.
pub fn lab_stage_pending(
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let (cmd, note) = match (p.noun.as_str(), p.verb.as_str()) {
        ("definition", "space") => (
            "definition space",
            "the ADR-0025 `space` op has no `hh-embed/1` binding yet",
        ),
        ("experiment", "status" | "plans" | "runs") => (
            "experiment status|plans|runs",
            "the experiment-ledger projection lands with the engine (S3.4a)",
        ),
        ("registry", "discover") => (
            "registry discover",
            "WS-H5 declared-source discovery is unlanded",
        ),
        ("bundle", "attest" | "verify") => (
            "bundle attest|verify",
            "the ADR-0067 attestation surface is a later stage",
        ),
        (n, v) => (n, v),
    };
    let _ = note;
    Err(stage_pending(cmd, note))
}

/// Exit-class helper re-exported for the stage_pending path (the
/// `CliOutcome` these commands produce on success is `ExitClass::Ok`).
#[allow(dead_code)]
fn _ok_class() -> ExitClass {
    ExitClass::Ok
}

// ── eval — Group L `lab.eval.*` (S3.3; R-2.9.2/R-2.9.4⁰ᵇ) ────────────
// The eval ops are records-in/records-out: the verb takes a params file —
// the canonical-JSON request body (`runs`/`tasks`/`design`/`arm_specs`/
// `metrics`/…) — and the response is the report record. No eval maths in
// the CLI (CC7).

/// `eval catalogue` → `lab.eval.catalogue` — the catalogue conformance
/// report (missing rows, declaration coverage).
pub fn cmd_eval_catalogue(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.eval.catalogue", Json::obj([]))?;
    ok_outcome("eval_catalogue", r, fmt(p, io)?)
}

/// `eval compare|scorecard|equivalence|loss-report <params-file>` → the
/// `lab.eval.*` op; the file is the canonical-JSON params body.
pub fn cmd_eval_op(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let method = match verb {
        "compare" => "lab.eval.compare",
        "scorecard" => "lab.eval.render_scorecard",
        "equivalence" => "lab.eval.equivalence_run",
        "loss-report" => "lab.eval.loss_report",
        _ => {
            return Err(CliError::Invocation(InvocationError::at(
                "unknown_command",
                &format!("eval {verb}"),
                "unknown eval verb",
            )))
        }
    };
    let text = file_text(p, io, 0, "<params-file>")?;
    let params = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "params-file",
            &format!("{e:?}"),
        ))
    })?;
    let r = call(b, method, params)?;
    ok_outcome(&format!("eval_{verb}"), r, fmt(p, io)?)
}

// ── S4.5a — the `participant` noun (§7.1 R-2.11.1²; §6.6) ─────────────
//
// One named `hh-embed/1` op per verb — the CLI never carries hosting
// semantics (AC-R-2.11.1-1). `participant probe --drive` forwards through
// the boundary's removable Hosting Plane; an absent plane refuses
// `hosting_plane_absent` (honest tier absence, never a faked report).

/// The shared participant selector — a positional `<name>` (with
/// `--namespace`/`--label`) or a `--participant-ref <version-id>` pin.
fn participant_params(p: &crate::cli::Parsed) -> Result<Json, CliError> {
    let mut m = std::collections::BTreeMap::new();
    if let Some(vid) = p.flag("participant-ref").or_else(|| {
        p.positional
            .first()
            .cloned()
            .filter(|s| s.starts_with("idp:"))
    }) {
        m.insert("participant_ref".to_string(), Json::str(vid));
    } else {
        let name = require_pos(p, 0, "<name>")?;
        m.insert("name".to_string(), Json::str(name));
        m.insert(
            "namespace".to_string(),
            p.flag("namespace")
                .map(Json::str)
                .unwrap_or_else(|| Json::str("local")),
        );
        if let Some(l) = p.flag("label") {
            m.insert("label".to_string(), Json::str(l));
        }
        if let Some(s) = p.flag("snapshot-id") {
            m.insert("snapshot_id".to_string(), Json::str(s));
        }
    }
    Ok(Json::Obj(m))
}

/// `participant describe <name|version-id> [--namespace] [--label]` →
/// `lab.hosting.describe` — the participant record + the reconciled
/// `capability_vector` + `stale`/`drift`/`quarantined` stamps (§6.6 §9).
pub fn cmd_participant_describe(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.hosting.describe", participant_params(p)?)?;
    ok_outcome("participant_describe", r, fmt(p, io)?)
}

/// `participant conformance <name|version-id>` → `lab.hosting.describe`'s
/// `conformance_entries` + reconciled vector — the registered
/// `hosted_entries` view (probes declare; they never mutate the record's
/// claimed conformance — a new declaration does).
pub fn cmd_participant_conformance(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let r = call(b, "lab.hosting.describe", participant_params(p)?)?;
    ok_outcome("participant_conformance", r, fmt(p, io)?)
}

/// `participant register <record-file>` → `lab.registry.register{kind:
/// participant}` — a participant record registers through the ordinary
/// registry path (the ABI never extends the record grammar).
pub fn cmd_participant_register(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let text = file_text(p, io, 0, "<record-file>")?;
    let body = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "record-file",
            &format!("{e:?}"),
        ))
    })?;
    let r = call(
        b,
        "lab.registry.register",
        Json::obj([
            ("kind", Json::str("participant")),
            ("body", body),
            ("registrar", registrar(io)),
            (
                "trust_record_ref",
                p.flag("trust-record-ref")
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
        ]),
    )?;
    ok_outcome("participant_register", r, fmt(p, io)?)
}

/// `participant probe <name|version-id> (--report <file> | --drive
/// [--probes <file>] --base <file>)` → `lab.hosting.probe`.
///
/// `--report` registers a `conformance_report{subject_kind: participant}`
/// verbatim (records-in). `--drive` forwards `drive_probe` through the
/// boundary's Hosting Plane — absent plane → `hosting_plane_absent`. A
/// P0-dimension DRIFT quarantines the participant version (the op's
/// result carries `quarantined`/`drift_dimensions`).
pub fn cmd_participant_probe(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let mut params = match participant_params(p)? {
        Json::Obj(m) => m,
        _ => std::collections::BTreeMap::new(),
    };
    params.insert("registrar".to_string(), registrar(io));
    if let Some(report_path) = p.flag("report") {
        let text = std::fs::read_to_string(&report_path).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "unreadable_file",
                &report_path,
                &format!("{e}"),
            ))
        })?;
        let body = hh_wire::json::parse(&text).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "malformed_json",
                "--report",
                &format!("{e}"),
            ))
        })?;
        params.insert("report".to_string(), body);
    } else if p.has("drive")
        || p.flag("drive").is_some()
        || p.positional.iter().any(|s| s == "drive")
    {
        let mut drive = Json::obj([]);
        // The drive params document (`{probes[], adapter_version_id?}`)
        // arrives either as the `--probes <file>` value or — with the
        // bare `--drive` switch — as the positional operand.
        let probes_path = p.flag("probes").or_else(|| p.positional.first().cloned());
        if let Some(probes_path) = probes_path {
            let text = std::fs::read_to_string(&probes_path).map_err(|e| {
                CliError::Invocation(InvocationError::at(
                    "unreadable_file",
                    &probes_path,
                    &format!("{e}"),
                ))
            })?;
            drive = hh_wire::json::parse(&text).map_err(|e| {
                CliError::Invocation(InvocationError::at(
                    "malformed_json",
                    "--probes",
                    &format!("{e}"),
                ))
            })?;
        }
        if let Json::Obj(ref mut d) = drive {
            if let Some(a) = p.flag("adapter-version-id") {
                d.insert("adapter_version_id".to_string(), Json::str(a));
            }
        }
        params.insert("drive".to_string(), drive);
        let base_path = p.flag("base").ok_or_else(|| {
            CliError::Invocation(InvocationError::at(
                "missing_flag",
                "--base",
                "participant probe --drive needs --base (the report scaffolding the driven entries pack into)",
            ))
        })?;
        let text = std::fs::read_to_string(&base_path).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "unreadable_file",
                &base_path,
                &format!("{e}"),
            ))
        })?;
        params.insert(
            "report_base".to_string(),
            hh_wire::json::parse(&text).map_err(|e| {
                CliError::Invocation(InvocationError::at(
                    "malformed_json",
                    "--base",
                    &format!("{e}"),
                ))
            })?,
        );
    } else {
        return Err(CliError::Invocation(InvocationError::at(
            "missing_flag",
            "--report|--drive",
            "participant probe needs --report <file> or --drive [--probes <file>] --base <file>",
        )));
    }
    let r = call(b, "lab.hosting.probe", Json::Obj(params))?;
    ok_outcome("participant_probe", r, fmt(p, io)?)
}

// ── S4.12 — `serve`/`acp`/`leaderboard`/`profile` (§7.1 C1/Stage-4 row;
// R-2.11.1; ADR-0173/0174 `serve`, ADR-0098 `acp`, ADR-0304 `async`/`defer`,
// ADR-0161/0163 `leaderboard`, the `model_profile`/`profile_test_report`
// record kinds) ─────────────────────────────────────────────────────────
//
// Every verb is one named `hh-embed/1` operation — the CLI never re-lowers
// an artefact, never computes a leaderboard, never interprets a profile
// record (records-in/records-out; K-2).

/// A `<file|ref:…>` operand → `DefinitionInput` (the same shape
/// `run start`'s `definition_input` mints — one spelling across verbs).
fn definition_operand(s: &str) -> Result<Json, CliError> {
    if let Some(r) = s.strip_prefix("ref:") {
        return Ok(Json::obj([
            ("kind", Json::str("ref")),
            ("ref", Json::str(r)),
        ]));
    }
    let text = std::fs::read_to_string(s).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "definition_unreadable",
            "<definition>",
            &format!("cannot read {s}: {e}"),
        ))
    })?;
    let doc = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "definition_invalid_json",
            "<definition>",
            &e.to_string(),
        ))
    })?;
    Ok(Json::obj([
        ("kind", Json::str("document")),
        ("document", doc),
    ]))
}

/// `hh serve <bundle-path|container> [--target acp|mcp]` /
/// `hh serve --definition <file|ref:…> [--target]` → `lab.serve`
/// verbatim (ADR-0173/0174 — the serve op resolves the target member or
/// runs the on-demand compile and returns the `hh-lab-serve/1` result
/// `{artifact, binding, launch}`; the CLI prints the launch descriptor,
/// never spawns it — `hh acp` is the *serving* verb).
pub fn cmd_serve(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let mut m = std::collections::BTreeMap::new();
    if let Some(d) = p.flag("definition").or_else(|| {
        p.positional
            .first()
            .cloned()
            .filter(|s| s.starts_with("ref:") || s.ends_with(".json"))
    }) {
        m.insert("definition".to_string(), definition_operand(&d)?);
    } else {
        let path = require_pos(p, 0, "<bundle-path|container>")?;
        let key = if Path::new(&path).is_dir() {
            "path"
        } else {
            "container"
        };
        m.insert(key.to_string(), Json::str(path));
    }
    if let Some(t) = p.flag("target") {
        m.insert("target".to_string(), Json::str(t));
    }
    if let Some(prof) = p.flag("profile") {
        m.insert("profile_refs".to_string(), Json::Arr(vec![Json::str(prof)]));
    }
    let r = call(b, "lab.serve", Json::Obj(m))?;
    if let Some(launch) = r
        .get("launch")
        .and_then(|l| l.get("program"))
        .and_then(Json::as_str)
    {
        let _ = writeln!(io.err, "serve: launch `{launch}` over stdio");
    }
    ok_outcome("serve", r, fmt(p, io)?)
}

/// `hh acp <definition|ref:…>` → `lab.serve{definition, target:"acp"}`
/// for the `hh-acp-target/1` artefact, then the agent-side
/// `serve_session` loop over the stdio frame channel, driven by an
/// `EmbedDriver` over *this invocation's* `Boundary` — the ACP surface
/// is a client of `hh-embed/1`, never a second kernel path (§7.1 C1;
/// ADR-0098; AC-R-2.11.1-1). stdin/stdout is the ACP wire; every
/// `session/new` opens `OpenSpec::new` over the same definition
/// (attendance `interactive` + `approval_mode = manual` — asks reach
/// the ACP client as `session/request_permission`, never a policy
/// default).
pub fn cmd_acp(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let operand = require_pos(p, 0, "<definition|ref:…>")?;
    let definition = definition_operand(&operand)?;
    // Pre-ledger gate parity with `run start` — a session opens a run;
    // a run needs a declared budget (the kernel re-checks; this is the
    // UX gate, never the authority).
    crate::invocation::missing_budget(
        definition.get("document").unwrap_or(&Json::Null),
        p.flag("budget").is_some(),
    )
    .map_err(CliError::Invocation)?;

    // 1) The artefact — `lab.serve`'s on-demand compile arm (the
    //    boundary owns lowering; the CLI consumes the record).
    let mut serve_params = Json::obj([
        ("definition", definition.clone()),
        ("target", Json::str("acp")),
    ]);
    if let Some(prof) = p.flag("profile") {
        if let Json::Obj(m) = &mut serve_params {
            m.insert(
                "profile_refs".to_string(),
                Json::Arr(vec![Json::str(prof.clone())]),
            );
        }
    }
    let serve = call(b, "lab.serve", serve_params)?;
    let artifact_json = serve.get("artifact").cloned().ok_or_else(|| {
        CliError::Transport(format!(
            "lab.serve{{target:acp}}: response has no `artifact`: {}",
            serve.to_canonical_string()
        ))
    })?;
    let artifact = hh_acp::AcpArtifact::from_json(&artifact_json)
        .map_err(|e| CliError::Transport(format!("hh-acp-target/1 decode: {e}")))?;

    // 2) The open spec every `session/new` drives — interactive asks
    //    round-trip as `session/request_permission` on the ACP wire.
    let mut spec = Json::obj([
        ("kind", Json::str("new")),
        ("definition", definition),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "supplies",
            Json::obj([
                ("context", Json::Arr(vec![])),
                ("host_capabilities", Json::Arr(vec![])),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(vec![])),
            ]),
        ),
        (
            "attendance",
            Json::obj([
                ("value", Json::str("interactive")),
                ("source", Json::str("declared")),
            ]),
        ),
        ("approval_mode", Json::str("manual")),
    ]);
    if let Some(budget) = p.flag("budget") {
        let bj = hh_wire::json::parse(&budget).map_err(|e| {
            CliError::Invocation(InvocationError::at(
                "budget_invalid_json",
                "--budget",
                &e.to_string(),
            ))
        })?;
        if let Json::Obj(m) = &mut spec {
            m.insert("budget".to_string(), bj);
        }
    }
    // `--profile` binds the run's profile coordinate — the same
    // coordinate the `lab.serve` compile resolved. `profile_binding` is
    // the role→entry map (AC-R-2.3.3-11): a `--profile` override names
    // the `primary` row under the canonical `{profile_ref:{profile}}`
    // spelling `set_coordinate`/`profile_binding` entries share.
    if let Some(prof) = p.flag("profile") {
        if let Json::Obj(m) = &mut spec {
            m.insert(
                "profile_binding".to_string(),
                Json::obj([(
                    "primary",
                    Json::obj([("profile_ref", Json::obj([("profile", Json::str(prof))]))]),
                )]),
            );
        }
    }

    // 3) The driver over the command's own boundary + the stdio frame
    //    channel — piped stdin bytes (tests, a one-shot ACP exchange)
    //    or the live stdin/stdout pair (`hh acp` spawned by an editor).
    let call = crate::boundary::BoundaryEmbedCall::new(b);
    let mut driver = hh_acp::EmbedDriver::new(call, spec);
    let served = match &io.stdin {
        Some(bytes) => {
            let mut t = hh_acp::StdioSessionTransport::new(
                std::io::Cursor::new(bytes.clone()),
                &mut *io.out,
            );
            hh_acp::serve_session(&artifact, &mut driver, &mut t)
        }
        None => {
            let si = std::io::stdin();
            let so = std::io::stdout();
            let mut t = hh_acp::StdioSessionTransport::new(si.lock(), so.lock());
            hh_acp::serve_session(&artifact, &mut driver, &mut t)
        }
    };
    served.map_err(|e| CliError::Transport(format!("acp serve: {}", e.refusal())))?;
    ok_outcome("acp_session_end", Json::obj([]), OutputFormat::Json)
}

/// `leaderboard <verb> <params-file>` → the `lab.leaderboard.*` op —
/// the params file is the canonical-JSON request body
/// (`definition`/`snapshot_id`/`rows`/`policy`/`supersedes`…);
/// `-` reads it from stdin (records-in/records-out; the CLI computes
/// nothing, R-2.11.1 stage row).
pub fn cmd_leaderboard(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    let method = match verb {
        "define" => "lab.leaderboard.define",
        "show" => "lab.leaderboard.leaderboard",
        "snapshots" => "lab.leaderboard.snapshots",
        "diff" => "lab.leaderboard.diff_snapshots",
        "publish" => "lab.leaderboard.publish",
        "retract" => "lab.leaderboard.retract_entry",
        _ => {
            return Err(CliError::Invocation(InvocationError::at(
                "unknown_command",
                &format!("leaderboard {verb}"),
                "leaderboard verbs: define|show|snapshots|diff|publish|retract",
            )))
        }
    };
    let text = file_text(p, io, 0, "<params-file>")?;
    let params = hh_wire::json::parse(&text).map_err(|e| {
        CliError::Invocation(InvocationError::at(
            "invalid_json",
            "params-file",
            &format!("{e:?}"),
        ))
    })?;
    let r = call(b, method, params)?;
    ok_outcome(&format!("leaderboard_{verb}"), r, fmt(p, io)?)
}

/// `profile <verb>` — the `model_profile`/`profile_test_report`
/// registry records over `lab.registry.*` (§7.4's profile coordinates
/// are registry-resolved — the CLI never interprets a profile body):
///
/// - `profile register <record-file>` → `lab.registry.register{kind:
///   model_profile}` (a `profile_test_report` file registers under its
///   own kind — the body's `schema` member names it).
/// - `profile list` → `lab.registry.query{clauses: kind = model_profile}`.
/// - `profile show|status <name|version-id> [--namespace] [--label]` →
///   `lab.registry.resolve` — the resolved record (its `status`/
///   `admission`/`update` members are the status the verb surfaces).
/// - `profile report <name|version-id>` → `lab.registry.resolve`
///   against `profile_test_report` (the same selector; the record's
///   `kind` member disambiguates).
pub fn cmd_profile(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &crate::cli::Parsed,
    verb: &str,
) -> Result<(crate::cli::CliOutcome, OutputFormat), CliError> {
    match verb {
        "register" => {
            let text = file_text(p, io, 0, "<record-file>")?;
            let body = hh_wire::json::parse(&text).map_err(|e| {
                CliError::Invocation(InvocationError::at(
                    "invalid_json",
                    "record-file",
                    &format!("{e:?}"),
                ))
            })?;
            // The body's own `schema` member names the kind — a
            // `profile_test_report` file is never re-typed by the CLI.
            let kind = match body.get("schema").and_then(Json::as_str) {
                Some("hh-profile-test-report/1") | Some("profile_test_report") => {
                    "profile_test_report"
                }
                _ => "model_profile",
            };
            let r = call(
                b,
                "lab.registry.register",
                Json::obj([
                    ("kind", Json::str(kind)),
                    ("body", body),
                    ("registrar", registrar(io)),
                    (
                        "trust_record_ref",
                        p.flag("trust-record-ref")
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                ]),
            )?;
            ok_outcome("profile_register", r, fmt(p, io)?)
        }
        "list" => {
            let r = call(
                b,
                "lab.registry.query",
                Json::obj([(
                    "clauses",
                    Json::Arr(vec![Json::obj([
                        ("field", Json::str("kind")),
                        ("op", Json::str("eq")),
                        ("value", Json::str("model_profile")),
                    ])]),
                )]),
            )?;
            ok_outcome("profile_list", r, fmt(p, io)?)
        }
        "show" | "status" | "report" => {
            let mut params = Json::obj([
                (
                    "namespace",
                    p.flag("namespace")
                        .map(Json::str)
                        .unwrap_or_else(|| Json::str("local")),
                ),
                (
                    "label",
                    p.flag("label").map(Json::str).unwrap_or(Json::Null),
                ),
                (
                    "snapshot_id",
                    p.flag("snapshot-id").map(Json::str).unwrap_or(Json::Null),
                ),
            ]);
            if let Some(vid) = p.flag("version-id").or_else(|| {
                p.positional
                    .first()
                    .cloned()
                    .filter(|s| s.starts_with("idp:") || s.starts_with("sha256:"))
            }) {
                if let Json::Obj(m) = &mut params {
                    m.insert("version_id".to_string(), Json::str(vid));
                }
            } else {
                let name = require_pos(p, 0, "<name|version-id>")?;
                if let Json::Obj(m) = &mut params {
                    m.insert("name".to_string(), Json::str(name));
                }
            }
            // `show`/`status`/`report` are *inspection* verbs — audit
            // mode: a quarantined record is the status answer, never a
            // refusal (admission is data — `execute` is the lift mode
            // and would hide the record the verb exists to print).
            if let Json::Obj(m) = &mut params {
                m.insert("mode".to_string(), Json::str("audit"));
            }
            let r = call(b, "lab.registry.resolve", params)?;
            // `report` asserts the resolved record *is* a test report —
            // a model_profile answer is the honest `not a report` line,
            // never a fabricated one.
            if verb == "report" {
                let kind = r
                    .get("record")
                    .and_then(|rec| rec.get("kind"))
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                if kind != "profile_test_report" {
                    return Err(CliError::Invocation(InvocationError::at(
                        "not_a_profile_test_report",
                        "<name|version-id>",
                        &format!("resolved record kind is `{kind}`, not `profile_test_report`"),
                    )));
                }
            }
            ok_outcome(&format!("profile_{verb}"), r, fmt(p, io)?)
        }
        _ => Err(CliError::Invocation(InvocationError::at(
            "unknown_command",
            &format!("profile {verb}"),
            "profile verbs: register|list|show|status|report",
        ))),
    }
}
