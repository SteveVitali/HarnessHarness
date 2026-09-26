//! The golden corpus driver — the R-2.12.2 corpus/naive-checker requirement
//! (the one already used by `hh-embed-schema`/`hh-hir`: "the golden corpus +
//! naive checker is part of the contracts evidence"). The driver enumerates
//! `tests/fixtures/plugin-manifests/valid/*.json` (each must admit) and
//! `tests/fixtures/plugin-manifests/invalid/*.json` (each must fail, with the
//! failure kind named by `<stem>.expect` — `ContractIncompatible`,
//! `UnpinnedInSealedForm`, `UnknownRecordKind`, `RequestsExceedCap`,
//! `SchemaViolation`), and runs a caller-supplied checker — the *naive*
//! second implementation checks manifest bytes independently of
//! `PluginManifest::decode`'s own unit tests (shared-authority mitigation —
//! ADR-0176's corpus pattern).
//!
//! `hh-plugin` is corpus-engine-only: the registry's `extension::plugin`
//! supplies the `check` closure (it owns the codec); the acceptance test in
//! `crates/hh-registry/tests/` wires them (no circular dep).

use std::fs;
use std::path::{Path, PathBuf};

/// One corpus case (the fixture file + its expected outcome).
#[derive(Debug)]
pub struct CorpusCase {
    /// The fixture path.
    pub path: PathBuf,
    /// The expected failure kind for an invalid case (`*.expect` content —
    /// empty when the fixture must merely fail).
    pub expect: Option<String>,
    /// Whether the case is under `valid/`.
    pub valid: bool,
}

/// Enumerate `dir/{valid,invalid}/*.json` sorted by name.
pub fn enumerate(dir: &Path) -> Vec<CorpusCase> {
    let mut out = Vec::new();
    for (sub, valid) in [("valid", true), ("invalid", false)] {
        let d = dir.join(sub);
        let Ok(rd) = fs::read_dir(&d) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        for path in files {
            let expect_path = path.with_extension("expect");
            let expect = fs::read_to_string(&expect_path)
                .ok()
                .map(|s| s.trim().to_string());
            out.push(CorpusCase {
                path,
                expect,
                valid,
            });
        }
    }
    out
}

/// A corpus failure (naive checker disagreed with the case's expectation).
#[derive(Debug)]
pub struct CorpusFailure {
    /// The fixture.
    pub path: PathBuf,
    /// What went wrong.
    pub detail: String,
}

/// Run the corpus. `check(bytes) → Ok(manifest debug) | Err(kind)` — the kind
/// is the checker's error-tag string (`format!("{}", err)`'s first token, or a
/// stable `ErrorKind` spelling). For an invalid case, a `*.expect` file pins
/// the required failure kind (a failure of the *wrong* kind fails the corpus —
/// "invalid" is not a licence for any error).
pub fn run_corpus<F>(dir: &Path, check: F) -> Result<Vec<String>, Vec<CorpusFailure>>
where
    F: Fn(&[u8]) -> Result<String, String>,
{
    let cases = enumerate(dir);
    if cases.is_empty() {
        return Err(vec![CorpusFailure {
            path: dir.to_path_buf(),
            detail: "empty corpus".to_string(),
        }]);
    }
    let mut admitted = Vec::new();
    let mut failures = Vec::new();
    for case in &cases {
        let bytes = match fs::read(&case.path) {
            Ok(b) => b,
            Err(e) => {
                failures.push(CorpusFailure {
                    path: case.path.clone(),
                    detail: format!("unreadable: {e}"),
                });
                continue;
            }
        };
        match (case.valid, check(&bytes)) {
            (true, Ok(id)) => admitted.push(id),
            (true, Err(kind)) => failures.push(CorpusFailure {
                path: case.path.clone(),
                detail: format!("valid fixture refused: {kind}"),
            }),
            (false, Ok(id)) => failures.push(CorpusFailure {
                path: case.path.clone(),
                detail: format!("invalid fixture admitted as {id}"),
            }),
            (false, Err(kind)) => {
                if let Some(want) = &case.expect {
                    if kind != *want {
                        failures.push(CorpusFailure {
                            path: case.path.clone(),
                            detail: format!("wrong failure kind: {kind} (expected {want})"),
                        });
                    }
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(admitted)
    } else {
        Err(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "hh-plugin-corpus-test-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("valid")).unwrap();
        fs::create_dir_all(d.join("invalid")).unwrap();
        d
    }

    #[test]
    fn corpus_drives_both_directions() {
        let d = tmpdir("both");
        fs::write(d.join("valid/ok.json"), b"ok").unwrap();
        fs::write(d.join("invalid/bad.json"), b"bad").unwrap();
        fs::write(d.join("invalid/bad.expect"), "SchemaViolation").unwrap();
        let r = run_corpus(&d, |b| {
            if b == b"ok" {
                Ok("id".to_string())
            } else {
                Err("SchemaViolation".to_string())
            }
        });
        assert_eq!(r.unwrap(), vec!["id".to_string()]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn wrong_failure_kind_fails() {
        let d = tmpdir("wrong");
        fs::write(d.join("valid/ok.json"), b"ok").unwrap();
        fs::write(d.join("invalid/bad.json"), b"bad").unwrap();
        fs::write(d.join("invalid/bad.expect"), "ContractIncompatible").unwrap();
        let r = run_corpus(&d, |b| {
            if b == b"ok" {
                Ok("id".to_string())
            } else {
                Err("SchemaViolation".to_string())
            }
        });
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].detail.contains("wrong failure kind"));
        let _ = fs::remove_dir_all(&d);
    }
}
