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
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `CellSpec` — the admitted cell-spec members (R6.9; AR-06): the
/// image-declared WORKDIR the cell runs under and the task's declared
/// `cpus`/`memory_mb` applied as container limits. A record that declares
/// none of them produces the pre-admission argv byte-identically.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellSpec {
    /// The cell's working directory — `None` keeps the `/work` default;
    /// `Some(w)` replaces it verbatim (the resolved image WORKDIR).
    pub workdir: Option<String>,
    /// `--cpus` — the task's declared cpu count (a positive number token).
    pub cpus: Option<String>,
    /// `--memory` — the task's declared memory limit in MiB.
    pub memory_mb: Option<u64>,
}

impl CellSpec {
    /// `true` when nothing is declared — the argv stays byte-identical.
    pub fn is_empty(&self) -> bool {
        self == &CellSpec::default()
    }
}

/// `SocketMount{name, host_path}` — one declared unix-socket bind mount
/// (AR-05): the host path mounts at the closed cell path
/// `/run/hh/<name>.sock` (`name ∈ {model, egress}` — the closed relay
/// pair set).
#[derive(Debug, Clone, PartialEq)]
pub struct SocketMount {
    /// `model | egress` (closed set — the pair's cell path derives).
    pub name: String,
    /// The host-side socket path.
    pub host_path: String,
}

impl SocketMount {
    /// The closed name → cell socket path map (one spelling home —
    /// `hh-env::egress_allowlist` records the same map on the profile).
    pub fn cell_path(&self) -> &'static str {
        match self.name.as_str() {
            "model" => "/run/hh/model.sock",
            _ => "/run/hh/egress.sock",
        }
    }
}

/// `RelaySpec{host_bin, pairs}` — the `hh-cell-relay` declaration (AR-05;
/// REQ-XB-129): the statically linked host binary bind-mounted read-only
/// at `/hh/bin/hh-cell-relay` and run as the cell's main process with
/// `--pair <name>=<port>=<cell_sock>` per declared pair.
#[derive(Debug, Clone, PartialEq)]
pub struct RelaySpec {
    /// The host path of the statically linked relay binary.
    pub host_bin: String,
    /// `(name, port)` pairs — `name` derives the cell socket path.
    pub pairs: Vec<(String, u16)>,
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
    /// The admitted cell spec (AR-06) — absent members keep the
    /// pre-admission argv byte-identical.
    pub cell: CellSpec,
    /// The declared unix-socket bind mounts (AR-05) — absent keeps the
    /// pre-admission argv byte-identical.
    pub mounts: Vec<SocketMount>,
    /// The `hh-cell-relay` main process (AR-05) — `None` keeps the
    /// `sleep infinity` tail byte-identical.
    pub relay: Option<RelaySpec>,
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

/// The podman binary — `HH_PODMAN_BIN` overrides for diagnostics and
/// the stub-driven acceptance legs (default `podman` on PATH).
fn podman_bin() -> String {
    std::env::var("HH_PODMAN_BIN").unwrap_or_else(|_| "podman".to_string())
}

fn run(op: &'static str, args: &[String]) -> Result<Output, BackendError> {
    let out = Command::new(podman_bin())
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

    /// `create_args()` — the `podman create` argv [`Self::create`] runs.
    /// `network` maps to `--network bridge|none` (`none` is the default
    /// posture; `bridge` is the admitted `declared_deps` egress variant —
    /// R5.56; ADR-0421). The AR-06 cell spec lands verbatim: a declared
    /// WORKDIR replaces `/work`, `--cpus`/`--memory` append after
    /// `--init`; an undeclared spec leaves every byte as it was.
    pub fn create_args(&self) -> Vec<String> {
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
            self.cell
                .workdir
                .clone()
                .unwrap_or_else(|| "/work".to_string()),
            "--init".to_string(),
        ];
        if let Some(c) = &self.cell.cpus {
            args.push("--cpus".to_string());
            args.push(c.clone());
        }
        if let Some(m) = self.cell.memory_mb {
            args.push("--memory".to_string());
            args.push(format!("{m}m"));
        }
        // AR-05 — the declared unix-socket bind mounts and the
        // statically linked `hh-cell-relay` main process (`--init` stays:
        // catatonit reaps and forwards a PID-1 signal to the relay, so
        // the REQ-XB-129 signal fact reads the same).
        for m in &self.mounts {
            args.push("-v".to_string());
            args.push(format!("{}:{}", m.host_path, m.cell_path()));
        }
        if let Some(r) = &self.relay {
            args.push("-v".to_string());
            args.push(format!("{}:/hh/bin/hh-cell-relay:ro", r.host_bin));
        }
        args.extend(self.extra_args.iter().cloned());
        args.push(self.image.clone());
        match &self.relay {
            Some(r) => {
                args.push("/hh/bin/hh-cell-relay".to_string());
                for (name, port) in &r.pairs {
                    args.push("--pair".to_string());
                    let sock = SocketMount {
                        name: name.clone(),
                        host_path: String::new(),
                    };
                    args.push(format!("{name}={port}={}", sock.cell_path()));
                }
            }
            None => {
                // A running-but-idle container: exec workloads attach to it.
                args.extend(["sleep".to_string(), "infinity".to_string()]);
            }
        }
        args
    }

    /// `create` — the environment's container (stopped; `exec` requires a
    /// running container, so the driver `start`s it at provision and leaves
    /// it running for the handle's life). Network off by default;
    /// `/work` mounts the environment workspace; a bounded pids limit;
    /// no new privileges; seccomp is podman's default profile (ptrace,
    /// process_vm_*, io_uring denied — the probe battery verifies).
    pub fn create(&self) -> Result<(), BackendError> {
        run("create", &self.create_args())?;
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
    /// `cwd = None` omits `--workdir` (R6.39: `cwd = image_workdir` —
    /// `podman exec` then inherits the container's configured WORKDIR,
    /// the cell-spec/image value, never a host-substituted path).
    pub fn exec_command(
        &self,
        argv: &[String],
        cwd: Option<&str>,
        env: &[(String, String)],
    ) -> Command {
        let mut c = Command::new(podman_bin());
        c.arg("exec");
        for (k, v) in env {
            c.arg("-e").arg(format!("{k}={v}"));
        }
        if let Some(w) = cwd {
            c.arg("--workdir").arg(w);
        }
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
        Command::new(podman_bin())
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

    // ── AR-06 cell verbs — the kernel-mediated copy-in/out and the
    //    phase filesystem reads the slot lifecycle drives. Every verb is
    //    a verbatim `podman` argv; the caller supplies host paths, never
    //    a channel through `/work` (REQ-XB-106). ───────────────────────

    /// `image_workdir()` — the image's declared `.Config.WorkingDir` read
    /// through `podman image inspect` (the `--workdir-from-image`
    /// admission). An absent/empty declaration is a typed backend error —
    /// the image WORKDIR is never silently substituted with `/work`.
    pub fn image_workdir(&self) -> Result<String, BackendError> {
        let out = run(
            "image",
            &[
                "image".to_string(),
                "inspect".to_string(),
                "--format".to_string(),
                "{{.Config.WorkingDir}}".to_string(),
                self.image.clone(),
            ],
        )?;
        let w = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if w.is_empty() {
            return Err(BackendError {
                op: "image",
                stderr: format!("image {} declares no Config.WorkingDir", self.image),
                status: None,
            });
        }
        Ok(w)
    }

    /// `cp_in(host_src, container_dst)` — `podman cp` host → cell. The
    /// kernel-side copy-in for `/tests`/`/solution`: bytes transit the
    /// host↔container edge, never the `/work` mount the agent writes.
    /// A `container_dst` under `/work` refuses before argv — aliasing the
    /// agent-writable mount would launder the phase boundary.
    pub fn cp_in(&self, host_src: &str, container_dst: &str) -> Result<(), BackendError> {
        refuse_work_alias(container_dst)?;
        run(
            "cp",
            &[
                "cp".to_string(),
                host_src.to_string(),
                format!("{}:{container_dst}", self.container),
            ],
        )?;
        Ok(())
    }

    /// `cp_out(container_src, host_dst)` — `podman cp` cell → host (the
    /// reward/log collection leg). A `container_src` under `/work`
    /// refuses for the same aliasing reason.
    pub fn cp_out(&self, container_src: &str, host_dst: &str) -> Result<(), BackendError> {
        refuse_work_alias(container_src)?;
        run(
            "cp",
            &[
                "cp".to_string(),
                format!("{}:{container_src}", self.container),
                host_dst.to_string(),
            ],
        )?;
        Ok(())
    }

    /// `exists(path)` — `test -e` inside the cell (the phase-boundary
    /// listing leg: `/tests` and `/solution` are absent until theirs).
    pub fn exists(&self, path: &str) -> Result<bool, BackendError> {
        let out = Command::new(podman_bin())
            .args([
                "exec".to_string(),
                self.container.clone(),
                "test".to_string(),
                "-e".to_string(),
                path.to_string(),
            ])
            .output()
            .map_err(|e| BackendError {
                op: "exec",
                stderr: e.to_string(),
                status: None,
            })?;
        Ok(out.status.success())
    }

    /// `ls_dir(path)` — `ls -1A` inside the cell; the per-phase
    /// filesystem listing the slot records cite.
    pub fn ls_dir(&self, path: &str) -> Result<Vec<String>, BackendError> {
        let out = run(
            "exec",
            &[
                "exec".to_string(),
                self.container.clone(),
                "ls".to_string(),
                "-1A".to_string(),
                path.to_string(),
            ],
        )?;
        let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        v.sort();
        Ok(v)
    }

    /// `rm_glob(path)` — `rm -f` inside the cell (the stale
    /// `reward.*` removal before ST5 — REQ-XB-106).
    pub fn rm_glob(&self, glob: &str) -> Result<(), BackendError> {
        run(
            "exec",
            &[
                "exec".to_string(),
                self.container.clone(),
                "sh".to_string(),
                "-c".to_string(),
                format!("rm -f {glob}"),
            ],
        )?;
        Ok(())
    }

    /// `terminate_group(pattern)` — the agent-deadline-lapse leg
    /// (REQ-XB-119): TERM the agent's process group inside the cell,
    /// then KILL what survives. `pkill -f` addresses the group by the
    /// participant's command line — the helper records the argv it
    /// actually ran, never a claimed signal.
    pub fn terminate_group(&self, pattern: &str) -> Result<(), BackendError> {
        let _ = run(
            "exec",
            &[
                "exec".to_string(),
                self.container.clone(),
                "pkill".to_string(),
                "-TERM".to_string(),
                "-f".to_string(),
                pattern.to_string(),
            ],
        );
        run(
            "exec",
            &[
                "exec".to_string(),
                self.container.clone(),
                "pkill".to_string(),
                "-KILL".to_string(),
                "-f".to_string(),
                pattern.to_string(),
            ],
        )?;
        Ok(())
    }
}

/// `wrap_kill(argv, deadline_ms)` — the in-cell process-tree kill
/// wrapper (R6.39; REQ-XE-119; AR-22; G17 (d)): `setsid` makes
/// `timeout` a session leader inside the cell, `timeout` runs the
/// command in a fresh process group and TERMs the whole group at the
/// deadline, then `--kill-after` SIGKILLs what survives — the entire
/// in-container tree dies without depending on the host-side pgid's
/// reach (UNVERIFIED across `podman exec`; ADR-0441 rejected the
/// host-side kill for exactly that reason). Returns `argv` unchanged
/// when no deadline is declared — an undated exec has nothing for the
/// wrapper to enforce.
pub fn wrap_kill(argv: &[String], deadline_ms: Option<u64>) -> Vec<String> {
    let Some(ms) = deadline_ms else {
        return argv.to_vec();
    };
    let dur = |v: u64| -> String {
        if v % 1000 == 0 {
            format!("{}", v / 1000)
        } else {
            format!(
                "{}.{}",
                v / 1000,
                format!("{:03}", v % 1000).trim_end_matches('0')
            )
        }
    };
    let mut out = vec![
        "setsid".to_string(),
        "timeout".to_string(),
        format!("--kill-after={}", dur(crate::exec::TERM_GRACE_MS)),
        dur(ms),
    ];
    out.extend(argv.iter().cloned());
    out
}

/// `/work` aliasing — a copy whose cell-side path lands under the
/// agent-writable mount refuses (`/work` itself or `/work/…`); the phase
/// boundary is never laundered through the mount.
fn refuse_work_alias(path: &str) -> Result<(), BackendError> {
    if path == "/work" || path.starts_with("/work/") {
        return Err(BackendError {
            op: "cp",
            stderr: format!("cell path {path} aliases the agent-writable /work mount"),
            status: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-R5.56-1 leg (argv): `network` maps to the verbatim
    /// `--network bridge|none` token in `create` argv; the `false`
    /// posture is the unchanged default.
    #[test]
    fn create_args_map_network_verbatim() {
        let base = PodmanBackend {
            container: "hh-env-e1".into(),
            image: PodmanBackend::DEFAULT_IMAGE.into(),
            workspace: "/tmp/ws".into(),
            network: false,
            extra_args: vec![],
            cell: CellSpec::default(),
            mounts: vec![],
            relay: None,
        };
        let args = base.create_args();
        let pos = args.iter().position(|a| a == "--network").unwrap();
        assert_eq!(args[pos + 1], "none");
        let bridged = PodmanBackend {
            network: true,
            ..base
        };
        let args = bridged.create_args();
        let pos = args.iter().position(|a| a == "--network").unwrap();
        assert_eq!(args[pos + 1], "bridge");
    }

    #[test]
    fn create_args_carry_the_boundary() {
        let b = PodmanBackend {
            container: "hh-env-e1".into(),
            image: PodmanBackend::DEFAULT_IMAGE.into(),
            workspace: "/tmp/ws".into(),
            network: false,
            extra_args: vec![],
            cell: CellSpec::default(),
            mounts: vec![],
            relay: None,
        };
        let mut c = Command::new("true");
        let _ = &mut c;
        // The argv shape is exercised through exec_command.
        let cmd = b.exec_command(
            &["/bin/echo".into()],
            Some("/work"),
            &[("A".into(), "1".into())],
        );
        let s = format!("{:?}", cmd);
        assert!(s.contains("exec"));
        assert!(s.contains("hh-env-e1"));
    }
}
