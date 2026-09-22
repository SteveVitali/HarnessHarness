//! The fixture filesystem environment (R-2.9.4⁰ᵇ; L4 egress containment).
//!
//! `FsEnv` is the in-tree environment the fixture adapters materialize into:
//! a per-task directory under a caller-chosen root. The environment record
//! declares `network_mode`; at Stage 3 the fixture environment enforces L4
//! **by construction** — a fixture environment has no network at all, so
//! `network_mode ∈ {restricted, open}` is refused at `materialize`
//! (advisory enforcement is never claimed as enforcement — an unenforceable
//! posture refuses, per T-LCD-14).
//!
//! The verifier side of an environment is a *separate* directory
//! (`<root>/verifier/`) that never holds participant-visible material — the
//! `separate` isolation the grade pipeline runs under; `shared` isolation
//! reuses the participant root and is only admissible where the family
//! record declares it.

use std::fs;
use std::path::{Path, PathBuf};

use hh_lab::bench::EnvironmentSpec;
use hh_ontology::lab::{BenchmarkNetworkMode, HandleCapability, Support};

/// The filesystem environment's typed failures.
#[derive(Debug, Clone, PartialEq)]
pub enum FsEnvError {
    /// The declared network mode cannot be enforced in-process (L4 — refuse,
    /// never claim advisory as enforcement).
    UnenforceableNetwork(BenchmarkNetworkMode),
    /// The root could not be created/written.
    Io(String),
}

impl std::fmt::Display for FsEnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsEnvError::UnenforceableNetwork(m) => {
                write!(f, "UnenforceableNetwork({})", m.name())
            }
            FsEnvError::Io(m) => write!(f, "Io: {m}"),
        }
    }
}

impl std::error::Error for FsEnvError {}

/// `FsEnv` — a materialized filesystem environment.
#[derive(Debug, Clone)]
pub struct FsEnv {
    /// The participant-surface root.
    pub root: PathBuf,
    /// The verifier-surface root (separate isolation).
    pub verifier_root: PathBuf,
}

impl FsEnv {
    /// `materialize` — create the environment directories and check L4:
    /// `none`/`simulated` are enforceable in-process (no network exists);
    /// `restricted`/`open` refuse (the fixture env cannot enforce them).
    pub fn materialize(root: &Path, spec: &EnvironmentSpec) -> Result<FsEnv, FsEnvError> {
        match spec.network_mode {
            BenchmarkNetworkMode::None | BenchmarkNetworkMode::Simulated => {}
            other => return Err(FsEnvError::UnenforceableNetwork(other)),
        }
        let verifier_root = root.join("verifier");
        fs::create_dir_all(&verifier_root).map_err(|e| FsEnvError::Io(format!("{root:?}: {e}")))?;
        Ok(FsEnv {
            root: root.to_path_buf(),
            verifier_root,
        })
    }

    /// `expose` — write the visible surface into the participant root (the
    /// instruction and attachments; held-out members never cross — the
    /// caller passes only `visible` content).
    pub fn expose(
        &self,
        instruction: &str,
        attachments: &[(String, &[u8])],
    ) -> Result<(), FsEnvError> {
        fs::write(self.root.join("INSTRUCTION.md"), instruction)
            .map_err(|e| FsEnvError::Io(e.to_string()))?;
        let att = self.root.join("attachments");
        if !attachments.is_empty() {
            fs::create_dir_all(&att).map_err(|e| FsEnvError::Io(e.to_string()))?;
            for (name, bytes) in attachments {
                fs::write(att.join(name), bytes).map_err(|e| FsEnvError::Io(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// `collect_submission` — read `submission.json` from the participant
    /// root. `Ok(None)` = the participant produced no artifact (a scored
    /// failure, not infrastructure).
    pub fn collect_submission(&self) -> Result<Option<Vec<u8>>, FsEnvError> {
        let p = self.root.join("submission.json");
        match fs::read(&p) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(FsEnvError::Io(e.to_string())),
        }
    }

    /// Stage the held-out material into the verifier root (`grade`'s only
    /// held-out access path).
    pub fn stage_held_out(&self, name: &str, bytes: &[u8]) -> Result<(), FsEnvError> {
        fs::write(self.verifier_root.join(name), bytes).map_err(|e| FsEnvError::Io(e.to_string()))
    }

    /// Read a staged held-out artifact (verifier-side only).
    pub fn read_held_out(&self, name: &str) -> Result<Vec<u8>, FsEnvError> {
        fs::read(self.verifier_root.join(name)).map_err(|e| FsEnvError::Io(e.to_string()))
    }

    /// The handle capabilities the fixture environment declares
    /// (AC-R-2.9.4-12's `declared` side): filesystem + shell-class and
    /// clock handles are surfaced; `handle.network` is declared
    /// `unsupported` (the fixture env has no network to hand out);
    /// anything absent is `unknown` — a family `require`ing it refuses
    /// `FamilyUnsupported` at `materialize`.
    pub fn declared_support() -> std::collections::BTreeMap<HandleCapability, Support> {
        [
            (HandleCapability::File, Support::Supported),
            (HandleCapability::Bash, Support::Supported),
            (HandleCapability::Clock, Support::Supported),
            // `handle.long_running` — episode persistence rides the
            // filesystem; the handle is declared.
            (HandleCapability::LongRunning, Support::Supported),
            (HandleCapability::Network, Support::Unsupported),
        ]
        .into_iter()
        .collect()
    }
}
