//! The macOS Seatbelt backend support — profile generation for
//! `sandbox-exec(1)` (S2.1; the `process_sandbox` backend on darwin).
//!
//! The generated profile is deny-default with targeted allows derived from
//! `ContainmentPolicy/1`'s concrete grants: `fs.write.allow` roots get
//! `file-write*` (minus `read_only_subpaths`, `deny_within_allow`, and the
//! `protected_metadata ⊇ KERNEL_PROTECTED` names — deny rules ordered
//! last); `fs.read` maps `allow_all_except` to broad read with the deny
//! patterns as `(deny file-read* …)` regexes, `allow_only` to per-path
//! reads; `fs.exec` scopes `process-exec`; `net.mode` maps `none` to no
//! network grant (deny-default), `mediated`/`public` to
//! `network-outbound`/`network*` respectively.
//!
//! Envelope gaps are declared honestly (LOSS-2 — every gap lands on the
//! `ContainmentReport`'s `lowering_losses` via the attach path):
//!
//! - Seatbelt cannot enforce pids/CPU/time limits — the helper enforces
//!   `resources.wall_ms` as the exec deadline itself; `open_files` maps to
//!   `RLIMIT_NOFILE` in the child pre-exec; `pids`/`cpu_ms`/`memory_bytes`/
//!   `disk_bytes`/egress bounds are declared lowering losses on darwin.
//! - `ptrace`/`setuid = deny` rely on deny-default (no `mach-lookup`/
//!   privilege grant) — the concrete `probe_boundary` verdicts are the
//!   evidence, never self-attestation.

use hh_containment::paths;
use hh_containment::policy::{ContainmentPolicy, ExecPolicy, NetMode, ReadMode};
use hh_wire::json::Json;

/// `seatbelt_profile(policy)` — the SBPL source `sandbox-exec -p` consumes.
/// Deterministic in the policy (rule order is the policy's own order —
/// path sets sort).
pub fn seatbelt_profile(policy: &ContainmentPolicy) -> String {
    let mut s = String::from("(version 1)\n(deny default)\n");
    // Process primitives: `fork`/`signal`/`info` are launch plumbing — but
    // `process-exec` is governed by `fs.exec` below (an empty extent execs
    // *nothing*, including `/bin/sh` — the kernel's floor gate has already
    // refused such effects upstream; the helper stays fail-closed).
    s.push_str("(allow process-fork) (allow signal (target same-sandbox)) (allow process-info*)\n");
    s.push_str("(allow file-read-metadata)\n");
    // The loader floor — dyld reads `kern.bootargs` (sysctl) and the root
    // dir itself before it maps the shared cache; without both the child
    // aborts inside the sandbox with no spawn verdict at all (observed on
    // darwin/arm64 — the seatbelt profile can't claim containment if its
    // own baseline launch fails).
    s.push_str("(allow sysctl-read)\n");
    s.push_str(
        "(allow file-read* (literal \"/\") (subpath \"/usr/lib\") \
         (subpath \"/System/Library\") \
         (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/usr/bin\") \
         (subpath \"/usr/sbin\") (subpath \"/private/etc/resolv.conf\") \
         (subpath \"/dev\") (literal \"/etc/localtime\"))\n",
    );
    s.push_str(
        "(allow file-write* (subpath \"/dev/null\") (subpath \"/dev/dtracehelper\") \
         (subpath \"/private/tmp\") (subpath \"/private/var/tmp\"))\n",
    );
    // Reads.
    match policy.fs.read.mode {
        ReadMode::AllowAllExcept => {
            // `file-map` is not a bound SBPL operation on current macOS —
            // `file-read*` covers the mmap path.
            s.push_str("(allow file-read*)\n");
            for d in &policy.fs.read.deny {
                s.push_str(&deny_read_rule(d));
            }
            for a in &policy.fs.read.allow_within_deny {
                // Re-allow holes — ordered after the deny; sbpl's last
                // matching clause wins for a given operation class.
                s.push_str(&allow_read_rule(a));
            }
        }
        ReadMode::AllowOnly => {
            for a in &policy.fs.read.allow {
                s.push_str(&allow_read_rule(a));
            }
        }
    }
    // Writes — only under declared WritableRoots, minus the carved-out
    // subpaths/names (denies emit *after* the allows — sbpl's last matching
    // rule wins, so carve-outs beat the root grant).
    let mut writable: Vec<&str> = policy
        .fs
        .write
        .allow
        .iter()
        .map(|w| w.root.as_str())
        .collect();
    writable.sort();
    for root in &writable {
        s.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            escape(&resolve(root))
        ));
    }
    for w in &policy.fs.write.allow {
        for sub in &w.read_only_subpaths {
            let full = format!("{}/{}", resolve(&w.root), paths::normalize_path(sub));
            s.push_str(&format!(
                "(deny file-write* (subpath \"{}\"))\n",
                escape(&full)
            ));
        }
    }
    for d in &policy.fs.write.deny_within_allow {
        s.push_str(&deny_write_pattern(d));
    }
    // Protected metadata — every protected name is read-only inside every
    // writable root (I-C2), enforced at the basename level (a suffix rule —
    // seatbelt's `regex` carries the component-suffix match).
    for name in &policy.fs.protected_metadata {
        s.push_str(&deny_write_pattern(name));
    }
    // Exec — `any` is unrestricted; `Allow(paths)` scopes process-exec to
    // the extent (empty ⇒ nothing may exec — fail-closed).
    match &policy.fs.exec {
        ExecPolicy::Any => s.push_str("(allow process-exec*)\n"),
        ExecPolicy::Allow(set) => {
            let mut set: Vec<&String> = set.iter().collect();
            set.sort();
            for p in set {
                s.push_str(&format!(
                    "(allow process-exec* (subpath \"{}\") (literal \"{}\"))\n",
                    escape(&resolve(p)),
                    escape(&resolve(p))
                ));
            }
        }
    }
    // Network — `none` grants nothing (deny-default); `mediated` gets
    // outbound only (the bridged unix channel is kernel-side, not the
    // sandbox's); `public` gets the full class.
    match policy.net.mode {
        NetMode::None => {}
        NetMode::Mediated => s.push_str("(allow network-outbound)\n"),
        NetMode::Public => s.push_str("(allow network*)\n"),
    }
    // `unix_sockets.allow` is orthogonal to `net.mode` (S2.2 — a
    // `subprocess_confined` plugin under `net.mode = none` reaches exactly
    // its declared per-path sockets, its host channel included; every
    // other socket stays deny-default). The policy validator refuses a
    // populated `allow` under `bridged_only`, so a non-empty list here is
    // always `allow_listed`.
    for sock in &policy.net.unix_sockets.allow {
        // The kernel-side filter that actually matches an AF_UNIX connect
        // on current macOS is the regex form — `literal`/`path-literal`/
        // `subpath` inside `remote unix-socket` never match (verified by
        // the isolation suite's allow-list probe: the channel connects,
        // a sibling socket is EPERM).
        s.push_str(&format!(
            "(allow network* (remote unix-socket (regex #\"{}\")))\n",
            regex_escape_components(&resolve(sock))
        ));
    }
    if policy.net.local_binding {
        s.push_str("(allow network-bind network-inbound)\n");
    }
    s
}

/// A read-deny rule for a path pattern — `/`-anchored patterns become
/// `(subpath …)`/literal denials; basename patterns become regex denials.
fn deny_read_rule(pattern: &str) -> String {
    let p = resolve(pattern);
    if p.starts_with('/') {
        format!(
            "(deny file-read* (subpath \"{}\") (literal \"{}\"))\n",
            escape(&p),
            escape(&p)
        )
    } else {
        format!(
            "(deny file-read* (regex #\"(^|/){}(/|$)\"))\n",
            regex_escape_components(&p)
        )
    }
}

fn allow_read_rule(pattern: &str) -> String {
    let p = resolve(pattern);
    if p.starts_with('/') {
        format!(
            "(allow file-read* (subpath \"{}\") (literal \"{}\"))\n",
            escape(&p),
            escape(&p)
        )
    } else {
        format!(
            "(allow file-read* (regex #\"(^|/){}(/|$)\"))\n",
            regex_escape_components(&p)
        )
    }
}

fn deny_write_pattern(pattern: &str) -> String {
    let p = resolve(pattern);
    if p.starts_with('/') {
        format!(
            "(deny file-write* (subpath \"{}\") (literal \"{}\"))\n",
            escape(&p),
            escape(&p)
        )
    } else {
        format!(
            "(deny file-write* (regex #\"(^|/){}(/|$)\"))\n",
            regex_escape_components(&p)
        )
    }
}

/// The seatbelt evaluator matches *resolved* vnode paths — a `/var/…`
/// spelling (the per-user temp dir's public face; the real dir is
/// `/private/var/…`) never matches a rule written under the unresolved
/// name. Every absolute path emitted into the profile goes through
/// `resolve`: canonicalize when the path exists, else the normalized
/// spelling (a deny rule on a not-yet-created path still lands).
fn resolve(p: &str) -> String {
    let n = paths::normalize_path(p);
    if !n.starts_with('/') {
        return n;
    }
    std::fs::canonicalize(&n)
        .map(|c| c.to_string_lossy().to_string())
        .unwrap_or(n)
}

fn escape(p: &str) -> String {
    p.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a component-wise pattern into a regex fragment — `*` inside a
/// component means `[^/]*` (the `paths` matcher's glob half).
fn regex_escape_components(p: &str) -> String {
    let mut out = String::new();
    for c in p.chars() {
        match c {
            '*' => out.push_str("[^/]*"),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '?' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// `is_available()` — `sandbox-exec` exists (macOS only).
pub fn is_available() -> bool {
    cfg!(target_os = "macos") && std::path::Path::new("/usr/bin/sandbox-exec").exists()
}

/// `spawn_argv(profile_src, inner_argv)` — the `sandbox-exec -p <profile>
/// <argv>` command line.
pub fn spawn_argv(profile_src: &str, inner_argv: &[String]) -> Vec<String> {
    let mut v = vec![
        "/usr/bin/sandbox-exec".to_string(),
        "-p".to_string(),
        profile_src.to_string(),
    ];
    v.extend(inner_argv.iter().cloned());
    v
}

/// The JSON the helper's diagnostics surface needs.
pub fn describe() -> Json {
    Json::obj([
        ("backend", Json::str("seatbelt")),
        ("available", Json::Bool(is_available())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_containment::policy::{kernel_default, WritableRoot};

    #[test]
    fn profile_is_deny_default_and_deterministic() {
        let mut p = kernel_default(0);
        p.fs.write.allow.push(WritableRoot {
            root: "/ws".into(),
            read_only_subpaths: vec!["frozen".into()],
            protected_metadata_names: vec![],
        });
        let s = seatbelt_profile(&p);
        assert!(s.contains("(deny default)"));
        assert!(s.contains("(allow file-write* (subpath \"/ws\"))"));
        assert!(s.contains("(deny file-write* (subpath \"/ws/frozen\"))"));
        assert!(s.contains(".hh"));
        assert_eq!(s, seatbelt_profile(&p));
    }

    #[test]
    fn unix_socket_allow_survives_net_mode_none() {
        // S2.2 (§8.4 subprocess_confined): the plugin's only reach is its
        // host channel — `net.mode = none` + `unix_sockets = allow_listed`
        // must still emit the per-path rule (deny-default covers the rest).
        use hh_containment::policy::{UnixSocketMode, UnixSockets};
        let mut p = kernel_default(0);
        p.net.unix_sockets = UnixSockets {
            mode: UnixSocketMode::AllowListed,
            allow: vec!["/tmp/chan.sock".into()],
        };
        let s = seatbelt_profile(&p);
        assert!(s.contains("(deny default)"));
        assert!(s.contains("(allow network* (remote unix-socket (regex #\"/tmp/chan\\.sock\")))"));
        // No broad outbound grant under `none`.
        assert!(!s.contains("network-outbound"));
    }
}
