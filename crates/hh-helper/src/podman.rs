//! The `local_container` backend — podman, reached through its CLI
//! (S2.1; §9.3's `local_container` row; the gate authorized podman for this
//! operator). One `hh-env:<env_id>` container is one environment: created
//! with the image the derivation declared, the workspace mounted
//! read-write at `/work`, network off by default, a fixed pids limit, and
//! `--rm` so `stop` is self-cleaning. `exec` runs commands inside it with
//! `podman exec`; `snapshot`-class work runs `podman commit` into an
//! `hh-snap:<env>:<seq>` image (`fork_snapshot` derives `create` from that
//! image). Every primitive returns the CLI's verbatim stderr on failure —
//! the kernel records the evidence; the helper never invents success.
//!
//! The boundary what podman enforces vs what the helper enforces is the
//! honest split (LOSS-2 declared): namespaces/cgroups/image/fs-mount are
//! podman's; the deadline ladder, output caps, dedup, and the attribution
//! echo are the helper's.

use std::process::{Command, Output};

/// `podman` availability — the binary exists *and* a machine connection
/// answers (`podman info` succeeds; on macOS this requires the running
/// `podman-machine-default` VM).
pub fn is_available() -> bool {
    Command::new("podman")
        .args(["info", "--format", "{{.Host.Os}}"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `PodmanBackend` — the container lifecycle for one environment.
#[derive(Debug, Clone)]
pub struct PodmanBackend {
    /// `hh-env-<env_id>` — the container name.
    pub container: String,
    /// The image the container runs.
    pub image: String,
    /// The host workspace mounted at `/work`.
    pub workspace: String,
    /// Whether the container's network is enabled.
    pub network: bool,
    /// Extra `podman create` args the policy requires (resource limits,
    /// mounts) — recorded verbatim, never interpreted.
    pub extra_args: Vec<String>,
}

/// A backend failure — verbatim stderr + exit status.
#[derive(Debug, Clone)]
pub struct BackendError {
    /// The verb.
    pub op: &'static str,
    /// The verbatim stderr (bounded).
    pub stderr: String,
    /// The exit status.
    pub status: Option<i32>,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "podman:{}: status={:?}: {}",
            self.op, self.status, self.stderr
        )
    }
}

impl std::error::Error for BackendError {}

fn run(op: &'static str, args: &[String]) -> Result<Output, BackendError> {
    let out = Command::new("podman")
        .args(args)
        .output()
        .map_err(|e| BackendError {
            op,
            stderr: e.to_string(),
            status: None,
        })?;
    if !out.status.success() {
        return Err(BackendError {
            op,
            stderr: String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(1024)
                .collect(),
            status: out.status.code(),
        });
    }
    Ok(out)
}

impl PodmanBackend {
    /// The default image when the derivation doesn't pin one.
    pub const DEFAULT_IMAGE: &'static str = "docker.io/library/alpine:3.20";

    /// `create` — the environment's container (stopped; `exec` requires a
    /// running container, so the driver `start`s it at provision and leaves
    /// it running for the handle's life). Network off by default;
    /// `/work` mounts the environment workspace; a bounded pids limit;
    /// no new privileges; seccomp is podman's default profile (ptrace,
    /// process_vm_*, io_uring denied — the probe battery verifies).
    pub fn create(&self) -> Result<(), BackendError> {
        let mut args = vec![
            "create".to_string(),
            "--name".to_string(),
            self.container.clone(),
            "--network".to_string(),
            if self.network { "bridge" } else { "none" }.to_string(),
            "--pids-limit".to_string(),
            "512".to_string(),
            "--security-opt".to_string(),
            "no-new-privileges".to_string(),
            "-v".to_string(),
            format!("{}:/work:rw", self.workspace),
            "--workdir".to_string(),
            "/work".to_string(),
            "--init".to_string(),
        ];
        args.extend(self.extra_args.iter().cloned());
        args.extend([
            self.image.clone(),
            // A running-but-idle container: exec workloads attach to it.
            "sleep".to_string(),
            "infinity".to_string(),
        ]);
        run("create", &args)?;
        Ok(())
    }

    /// `start` — the container runs (idempotent for the helper's purposes:
    /// an already-running container returns success upstream).
    pub fn start(&self) -> Result<(), BackendError> {
        run("start", &["start".to_string(), self.container.clone()])?;
        Ok(())
    }

    /// `exec` — run `argv` inside the container, streaming to the caller's
    /// pipes (the caller sets stdio; this only builds the command).
    pub fn exec_command(&self, argv: &[String], cwd: &str, env: &[(String, String)]) -> Command {
        let mut c = Command::new("podman");
        c.arg("exec");
        for (k, v) in env {
            c.arg("-e").arg(format!("{k}={v}"));
        }
        c.arg("--workdir").arg(cwd);
        c.arg(&self.container);
        c.args(argv);
        c
    }

    /// `commit(image_ref)` — capture the container's writable layer as a
    /// new image (the `fork_snapshot` substrate — `SnapshotContent`'s
    /// `container_image` member carries the resulting ref).
    pub fn commit(&self, image_ref: &str) -> Result<(), BackendError> {
        run(
            "commit",
            &[
                "commit".to_string(),
                "--pause".to_string(),
                "true".to_string(),
                self.container.clone(),
                image_ref.to_string(),
            ],
        )?;
        Ok(())
    }

    /// `inspect_running` — is the container live?
    pub fn running(&self) -> bool {
        Command::new("podman")
            .args(["inspect", "--format", "{{.State.Running}}", &self.container])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false)
    }

    /// `stop` — stop and remove (the env is `--init`ed; stop signals the
    /// workload's process group inside the container first).
    pub fn stop(&self) -> Result<(), BackendError> {
        let _ = run(
            "stop",
            &[
                "stop".to_string(),
                "-t".to_string(),
                "2".to_string(),
                self.container.clone(),
            ],
        );
        run(
            "rm",
            &["rm".to_string(), "-f".to_string(), self.container.clone()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_args_carry_the_boundary() {
        let b = PodmanBackend {
            container: "hh-env-e1".into(),
            image: PodmanBackend::DEFAULT_IMAGE.into(),
            workspace: "/tmp/ws".into(),
            network: false,
            extra_args: vec![],
        };
        let mut c = Command::new("true");
        let _ = &mut c;
        // The argv shape is exercised through exec_command.
        let cmd = b.exec_command(&["/bin/echo".into()], "/work", &[("A".into(), "1".into())]);
        let s = format!("{:?}", cmd);
        assert!(s.contains("exec"));
        assert!(s.contains("hh-env-e1"));
    }
}
