//! `hh-secret-scan` — the thin CLI over `hh_secrets`' one canonical detector
//! set (`DetectorSet::standard` + `redact`/`leak_scan`), built for the
//! live-evidence gate (ADR-0363 D6; ticket R4.5a). CC1: no second spelling of
//! the patterns or mask machinery exists anywhere else.
//!
//! ```text
//! hh-secret-scan scan   [--mask-env FILE] [--mask-var NAME]... [--] FILE...
//! hh-secret-scan redact [--mask-env FILE] [--mask-var NAME]... < in > out
//! ```
//!
//! The mask set's known values come from `--mask-env` (a `NAME=VALUE` env
//! file — values stay process-local, never printed) and `--mask-var` (names
//! read from the process environment). `scan` prints one content-free line
//! per leak (`<file>: <detector> <label-or-fingerprint>`) and exits 1; 0 when
//! clean. `redact` writes the redacted text to stdout and reports the hit
//! count + detector names (never spans of matched bytes) on stderr.
//! Usage/IO errors exit 2.

use std::io::{Read, Write};
use std::process::ExitCode;

use hh_secrets::{
    is_inheritable, redact, CredentialBroker, DenyAllResolver, DetectorSet, MaskEntry, MaskSet,
    ScanTarget,
};

const USAGE: &str =
    "usage: hh-secret-scan <scan|redact> [--mask-env FILE] [--mask-var NAME]... [--] [FILE...]\n\
     scan: FILE args are swept (`-` or none = stdin); exit 1 on any leak\n\
     redact: stdin -> stdout through the one canonical redactor";

/// Parse `NAME=VALUE` / `export NAME=VALUE` lines; comments + blanks skipped.
/// Values stay in-process; this function never prints.
fn parse_env_file(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let mut value = value.trim();
        if value.len() >= 2 {
            let b = value.as_bytes();
            if (b[0] == b'"' && b[value.len() - 1] == b'"')
                || (b[0] == b'\'' && b[value.len() - 1] == b'\'')
            {
                value = &value[1..value.len() - 1];
            }
        }
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !value.is_empty()
        {
            out.push((name.to_string(), value.to_string()));
        }
    }
    out
}

/// Build the detector set: the registered patterns plus a known-value mask
/// entry per provisioned *secret*. The mask boundary is the kernel's own
/// credential-shaping rule (CC1 — `is_inheritable`): `*_API_KEY`/`_TOKEN`/
/// `_SECRET`-class values are masked; non-secret config (`HH_LOCAL_MODEL`,
/// base URLs, OAuth client ids) is not — those values are exactly what an
/// evidence record may legitimately carry (`model_reported`,
/// `endpoint_host`). `--mask-var` is unconditional: an explicit caller
/// request always masks. The broker is instantiated only to mint the keyed
/// fingerprints (the one `secret.fingerprint` primitive — CC1).
fn build_detectors(env_files: &[String], mask_vars: &[String]) -> Result<DetectorSet, String> {
    let broker = CredentialBroker::new(Box::new(DenyAllResolver), "hh-secret-scan");
    let mut masks = MaskSet::default();
    let mut insert = |name: &str, value: &str| {
        masks.insert(MaskEntry {
            channel_id: name.to_string(),
            revision: 0,
            value: value.to_string(),
            fingerprint: broker.fingerprint(value, name, 0),
            live: true,
            placeholder: None,
        });
    };
    for path in env_files {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read mask env file {path}: {e}"))?;
        for (name, value) in parse_env_file(&text) {
            if !is_inheritable(&name) {
                insert(&name, &value);
            }
        }
    }
    for name in mask_vars {
        match std::env::var(name) {
            Ok(v) if !v.is_empty() => insert(name, &v),
            _ => {} // absent/empty names contribute no mask entry
        }
    }
    Ok(DetectorSet::standard(masks))
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(mode) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut env_files = Vec::new();
    let mut mask_vars = Vec::new();
    let mut files = Vec::new();
    let mut literal = false;
    let rest: Vec<String> = args.collect();
    let mut i = 0;
    while i < rest.len() {
        let a = &rest[i];
        if !literal && a == "--" {
            literal = true;
        } else if !literal && a == "--mask-env" {
            i += 1;
            match rest.get(i) {
                Some(p) => env_files.push(p.clone()),
                None => {
                    eprintln!("hh-secret-scan: --mask-env needs a path\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        } else if !literal && a == "--mask-var" {
            i += 1;
            match rest.get(i) {
                Some(n) => mask_vars.push(n.clone()),
                None => {
                    eprintln!("hh-secret-scan: --mask-var needs a name\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        } else {
            files.push(a.clone());
        }
        i += 1;
    }
    let detectors = match build_detectors(&env_files, &mask_vars) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("hh-secret-scan: {e}");
            return ExitCode::from(2);
        }
    };
    match mode.as_str() {
        "redact" => {
            if !files.is_empty() {
                eprintln!("hh-secret-scan: redact reads stdin only\n{USAGE}");
                return ExitCode::from(2);
            }
            let mut text = String::new();
            if std::io::stdin().read_to_string(&mut text).is_err() {
                eprintln!("hh-secret-scan: cannot read stdin");
                return ExitCode::from(2);
            }
            let (out, hits) = redact(&text, &detectors);
            if std::io::stdout().write_all(out.as_bytes()).is_err() {
                eprintln!("hh-secret-scan: cannot write stdout");
                return ExitCode::from(2);
            }
            eprintln!("hh-secret-scan: redacted {} hit(s)", hits.len());
            ExitCode::SUCCESS
        }
        "scan" => {
            if files.is_empty() {
                files.push("-".to_string());
            }
            let mut leaks = 0u32;
            for f in &files {
                let (name, text) = if f == "-" {
                    let mut t = String::new();
                    match std::io::stdin().read_to_string(&mut t) {
                        Ok(_) => ("<stdin>".to_string(), t),
                        Err(e) => {
                            eprintln!("hh-secret-scan: cannot read stdin: {e}");
                            return ExitCode::from(2);
                        }
                    }
                } else {
                    match std::fs::read_to_string(f) {
                        Ok(t) => (f.clone(), t),
                        Err(e) => {
                            eprintln!("hh-secret-scan: cannot read {f}: {e}");
                            return ExitCode::from(2);
                        }
                    }
                };
                for leak in hh_secrets::leak_scan(
                    &[(ScanTarget::Artifact(name.clone()), text.as_str())],
                    &detectors,
                ) {
                    let detail = leak
                        .fingerprint
                        .clone()
                        .or_else(|| leak.label.clone())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{}: {} {}",
                        leak.location_string(),
                        leak.detector.as_str(),
                        detail
                    );
                    leaks += 1;
                }
            }
            if leaks > 0 {
                eprintln!("hh-secret-scan: {leaks} leak(s)");
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        other => {
            eprintln!("hh-secret-scan: unknown mode {other}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
