//! The `local_host` environment handle (R-2.2.5; §5d/§5g.3). **Throwaway Stage-0 subset.**
//!
//! §9.1 R-2.2.5/R-2.5.5 slice: "`local_host` class (`isolation_class = none`, recorded
//! honestly), `EnvHandle`, `declared → ready → torn_down`". At Stage 0 there is no sandbox
//! (S1.16) and no container (S2.1): the handle records its isolation as `none` **honestly**
//! (never claims containment it does not have — CC3/CC10), scopes the tool executor to a
//! per-run root directory, and exposes only a deny-by-default, masked view of the process
//! environment (the allow-list half of AC-R-2.8.3-1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::secrets::{leak_scan, mask_known_values, AllowList, HeldSecret, Leak};

/// The honest isolation class of a Stage-0 handle. There is exactly one value: `none`. The
/// enum exists so the honesty is a type, not a comment — a later stage adds `subprocess`,
/// `container`, … and the `local_host` handle keeps declaring `none` (T-LCD-06 removability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationClass {
    None,
}

impl IsolationClass {
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationClass::None => "none",
        }
    }
}

/// The environment-handle lifecycle (`declared → ready → torn_down`). Monotonic: a handle
/// never moves backwards, and the tool executor runs only in `Ready`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvState {
    Declared,
    Ready,
    TornDown,
}

impl EnvState {
    pub fn as_str(self) -> &'static str {
        match self {
            EnvState::Declared => "declared",
            EnvState::Ready => "ready",
            EnvState::TornDown => "torn_down",
        }
    }
}

/// A local-host environment handle scoped to one run.
#[derive(Debug)]
pub struct EnvHandle {
    pub isolation_class: IsolationClass,
    state: EnvState,
    root: PathBuf,
    /// The real host environment, as name → value (secret values already excluded — they are
    /// held in `secrets` and never placed here).
    host_env: BTreeMap<String, String>,
    allow: AllowList,
    secrets: Vec<HeldSecret>,
}

impl EnvHandle {
    /// Declare a handle over `root` with an allow-list and the kernel-held secrets. State is
    /// `Declared`; the executor cannot run until `ready()`.
    pub fn declare(
        root: impl Into<PathBuf>,
        allow: AllowList,
        secrets: Vec<HeldSecret>,
        host_env: BTreeMap<String, String>,
    ) -> Self {
        Self {
            isolation_class: IsolationClass::None,
            state: EnvState::Declared,
            root: root.into(),
            host_env,
            allow,
            secrets,
        }
    }

    /// Transition `declared → ready`, creating the sandbox root directory.
    pub fn ready(&mut self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        self.state = EnvState::Ready;
        Ok(())
    }

    /// Transition `ready → torn_down` (best-effort cleanup of the root).
    pub fn tear_down(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        self.state = EnvState::TornDown;
    }

    pub fn state(&self) -> EnvState {
        self.state
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn secrets(&self) -> &[HeldSecret] {
        &self.secrets
    }

    /// The masked environment the sandbox actually sees: allow-listed variables verbatim,
    /// everything else withheld, every known secret value replaced by its placeholder. This is
    /// what a tool would read from the process environment.
    pub fn sandbox_env(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (k, v) in &self.host_env {
            if self.allow.is_allowed(k) {
                out.insert(k.clone(), mask_known_values(v, &self.secrets));
            }
        }
        // Secret carriers appear only as placeholders, never as values.
        for s in &self.secrets {
            out.insert(s.reference.name.clone(), s.reference.placeholder());
        }
        out
    }

    /// An environment dump (`env`-style), masked. AC-R-2.8.3-1: placeholders only.
    pub fn env_dump(&self) -> String {
        self.sandbox_env()
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A read of the process-environment file (`/proc/self/environ`-style), masked. Distinct
    /// surface from `env_dump` so the AC's "environment dump **and** a read of the
    /// process-environment file" are both covered.
    pub fn read_process_env_file(&self) -> String {
        self.sandbox_env()
            .iter()
            .map(|(k, v)| format!("{k}={v}\0"))
            .collect::<String>()
    }

    /// Scan the handle's two exposed surfaces for any raw secret value (AC-R-2.8.3-1
    /// `leak_scan(run) = ∅`).
    pub fn leak_scan(&self) -> Vec<Leak> {
        leak_scan(
            &[
                ("env_dump".into(), self.env_dump()),
                ("process_env_file".into(), self.read_process_env_file()),
            ],
            &self.secrets,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretRef;

    fn handle(root: PathBuf) -> EnvHandle {
        let mut host = BTreeMap::new();
        host.insert("PATH".into(), "/usr/bin".into());
        host.insert("HOME".into(), "/home/agent".into());
        let secrets = vec![HeldSecret::new(
            SecretRef::new("API_TOKEN", "gateway credential"),
            "sk-secret-123",
        )];
        EnvHandle::declare(root, AllowList::new().allow("PATH"), secrets, host)
    }

    #[test]
    fn isolation_is_recorded_honestly_as_none() {
        let h = handle(std::env::temp_dir().join("hh-baseline-test-iso"));
        assert_eq!(h.isolation_class, IsolationClass::None);
        assert_eq!(h.isolation_class.as_str(), "none");
    }

    #[test]
    fn lifecycle_moves_declared_ready_torn_down() {
        let root = std::env::temp_dir().join("hh-baseline-test-lifecycle");
        let mut h = handle(root.clone());
        assert_eq!(h.state(), EnvState::Declared);
        h.ready().unwrap();
        assert_eq!(h.state(), EnvState::Ready);
        assert!(root.exists());
        h.tear_down();
        assert_eq!(h.state(), EnvState::TornDown);
    }

    #[test]
    fn env_dump_and_proc_file_show_placeholders_only() {
        let h = handle(std::env::temp_dir().join("hh-baseline-test-dump"));
        let dump = h.env_dump();
        let procf = h.read_process_env_file();
        // AC-R-2.8.3-1: neither surface contains the raw value.
        assert!(!dump.contains("sk-secret-123"));
        assert!(!procf.contains("sk-secret-123"));
        assert!(dump.contains("${SECRET:API_TOKEN}"));
        // HOME was not allow-listed: deny-by-default.
        assert!(!dump.contains("HOME"));
        assert!(dump.contains("PATH=/usr/bin"));
    }

    #[test]
    fn leak_scan_is_empty() {
        let h = handle(std::env::temp_dir().join("hh-baseline-test-leak"));
        assert!(h.leak_scan().is_empty());
    }
}
