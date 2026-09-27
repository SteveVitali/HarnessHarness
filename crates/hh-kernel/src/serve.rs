//! The stdio JSON-RPC 2.0 server — binding (b) of `hh-embed/1`.
//!
//! `hh-kernel serve` is a thin shell over [`hh_embed`]: the service
//! config comes from the process environment (`HH_STORE_ROOT` — the
//! ledger root; `HH_WORKSPACE_ROOT` — the `local_host` workspace
//! binding; both default under the current directory), and the framed
//! loop is `hh_embed::stdio::serve` verbatim so the wire is the
//! contract's, not the binary's (ADR-0179 D1(b)).

use std::io::{BufReader, Read, Write};

use hh_embed::{EmbedService, ServiceConfig};

use crate::KERNEL_VERSION_ID;

/// The ledger root `serve` binds (`HH_STORE_ROOT`, default
/// `<cwd>/.hh/store`).
fn store_root() -> std::path::PathBuf {
    match std::env::var_os("HH_STORE_ROOT") {
        Some(p) => p.into(),
        None => std::env::current_dir()
            .unwrap_or_else(|_| ".".into())
            .join(".hh")
            .join("store"),
    }
}

/// The workspace root `local_host` sessions bind (`HH_WORKSPACE_ROOT`,
/// default the current directory).
fn workspace_root() -> std::path::PathBuf {
    match std::env::var_os("HH_WORKSPACE_ROOT") {
        Some(p) => p.into(),
        None => std::env::current_dir().unwrap_or_else(|_| ".".into()),
    }
}

/// Run the serve loop until end-of-stream. `reader` supplies
/// newline-delimited requests; `writer` receives newline-delimited
/// responses and `stream.frame`/`upcall.*` notifications.
pub fn run<R: Read, W: Write>(reader: R, mut writer: W) -> std::io::Result<()> {
    let mut svc = EmbedService::open(service_config())
        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let mut buf = BufReader::new(reader);
    hh_embed::stdio::serve(&mut svc, &mut buf, &mut writer)
}

fn service_config() -> ServiceConfig {
    ServiceConfig {
        store_root: store_root(),
        kernel_version_id: KERNEL_VERSION_ID.to_string(),
        workspace_root: workspace_root(),
        holder: KERNEL_VERSION_ID.to_string(),
    }
}

/// `serve --http <addr>` — binding (c), the `local_network` transport
/// (§7.2; S4.10). `addr` must be loopback (`127.0.0.1:port`/`[::1]:port`)
/// — the (c) server refuses anything else before the first accept
/// (P5/OQ-401).
///
/// Token delivery (ADR-0301 D8; the WS-H3/K2 capability): `--token <t>`
/// uses the caller's token; otherwise a fresh ≥128-bit token is minted.
/// `--token-file <path>` writes it mode-0600 for the surface/launcher to
/// read; either way one line is printed to **stderr**
/// (`hh-kernel-serve-http bound=<addr>` — the token itself prints only
/// when no file path was given, the OQ-389 interim delivery; it never
/// rides a URL and never logs on the request path, P9).
pub fn run_http(
    addr: &str,
    token: Option<String>,
    token_file: Option<std::path::PathBuf>,
    log: &mut dyn Write,
) -> std::io::Result<()> {
    let token = match token {
        Some(t) => t,
        None => hh_embed::net::mint_token()?,
    };
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|e| std::io::Error::new(e.kind(), format!("bind {addr}: {e}")))?;
    let local = listener.local_addr()?;
    if let Some(path) = &token_file {
        write_token_file(path, &token)?;
    }
    if token_file.is_some() {
        writeln!(
            log,
            "hh-kernel-serve-http bound=http://{local} token_file=<redacted>"
        )?;
    } else {
        writeln!(
            log,
            "hh-kernel-serve-http bound=http://{local} token={token}"
        )?;
    }
    let mut svc = EmbedService::open(service_config())
        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    hh_embed::net::serve_net(listener, &mut svc, &token)
}

/// Write the token file mode-0600 (Unix; the token is a capability —
/// world-readable would defeat it).
fn write_token_file(path: &std::path::Path, token: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(token.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}
